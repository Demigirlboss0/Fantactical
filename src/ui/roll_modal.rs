use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crate::model::{HitLocation, TurnPhase};
use crate::model::rolls::roll_3d6;
use crate::systems::phase_machine::{
    AttackSetupConfirmedEvent, CancelPhaseEvent, DefenseSelectedEvent, DefenseType, ModalState,
    PhaseAdvanceEvent, RollRequestedEvent,
};
use crate::ui::battlemap::GameStateResource;

pub struct RollModalPlugin;

impl Plugin for RollModalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, render_roll_modal);
    }
}

fn render_roll_modal(
    mut egui_ctx: EguiContexts,
    state: Option<Res<GameStateResource>>,
    mut modal: ResMut<ModalState>,
    mut attack_events: EventWriter<AttackSetupConfirmedEvent>,
    mut roll_events: EventWriter<RollRequestedEvent>,
    mut defense_events: EventWriter<DefenseSelectedEvent>,
    mut advance_events: EventWriter<PhaseAdvanceEvent>,
    mut cancel_events: EventWriter<CancelPhaseEvent>,
) {
    if !modal.show {
        return;
    }

    let Some(state) = state else { return };
    let current = state.history.current();
    let phase = current.current_phase;

    let ctx = egui_ctx.ctx_mut();

    let title = match phase {
        TurnPhase::AttackSetup => "Attack Setup",
        TurnPhase::ManeuverConfirmed => "Maneuver Confirmed",
        TurnPhase::AttackRoll => "Attack Roll",
        TurnPhase::DefenseResolution => "Defense Resolution",
        _ => "Combat",
    };

    egui::Window::new(title)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            match phase {
                TurnPhase::AttackSetup => render_attack_setup(
                    ui, &current, &mut modal, &mut attack_events, &mut cancel_events,
                ),
                TurnPhase::ManeuverConfirmed => render_maneuver_confirmed(
                    ui, &mut modal, &mut advance_events, &mut cancel_events,
                ),
                TurnPhase::AttackRoll => render_attack_roll(
                    ui, &mut modal, &mut roll_events, &mut cancel_events,
                ),
                TurnPhase::DefenseResolution => render_defense_resolution(
                    ui, &current, &mut modal, &mut defense_events, &mut advance_events, &mut cancel_events,
                ),
                _ => {
                    ui.label(format!("Phase: {:?}", phase));
                }
            }
        });
}

fn render_attack_setup(
    ui: &mut egui::Ui,
    current: &crate::model::GameState,
    modal: &mut ModalState,
    attack_events: &mut EventWriter<AttackSetupConfirmedEvent>,
    cancel_events: &mut EventWriter<CancelPhaseEvent>,
) {
    let attacker_id = current.current_actor;
    let Some(attacker) = current.actors.get(&attacker_id) else {
        ui.label("No attacker.");
        return;
    };

    ui.heading(format!("{} — Select Attack", attacker.name));

    ui.separator();

    if attacker.attacks.is_empty() {
        ui.label("No attacks available.");
        return;
    }

    let attack_names: Vec<String> = attacker.attacks.iter().map(|a| {
        format!("{} ({}d{:+} {:?}, lvl {})", a.name, a.damage_dice, a.damage_adds, a.damage_type, a.skill_level)
    }).collect();

    ui.label("Attack:");
    egui::ComboBox::from_id_salt("attack_selector")
        .selected_text(&attack_names.get(modal.attack_index).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (i, name) in attack_names.iter().enumerate() {
                if ui.selectable_label(modal.attack_index == i, name).clicked() {
                    modal.attack_index = i;
                }
            }
        });

    ui.separator();

    let loc_names: Vec<String> = HitLocation::iter_all()
        .map(|l| format!("{:?} ({})", l, l.to_hit_penalty()))
        .collect();

    let current_loc_name = format!("{:?} ({})", modal.hit_location, modal.hit_location.to_hit_penalty());

    ui.label("Target Hit Location:");
    egui::ComboBox::from_id_salt("loc_selector")
        .selected_text(current_loc_name)
        .show_ui(ui, |ui| {
            for name in &loc_names {
                if ui.selectable_label(false, name).clicked() {
                    for loc in HitLocation::iter_all() {
                        if format!("{:?} ({})", loc, loc.to_hit_penalty()) == *name {
                            modal.hit_location = loc;
                            break;
                        }
                    }
                }
            }
        });

    let target_id = modal.target_id_for_modal.unwrap_or(attacker_id);

    ui.separator();
    ui.label(format!("Target: {}", current.actors.get(&target_id)
        .map(|a| a.name.as_str()).unwrap_or("unknown")));

    ui.separator();
    if ui.button("Confirm Attack Setup").clicked() {
        attack_events.send(AttackSetupConfirmedEvent {
            attacker_id,
            attack_index: modal.attack_index,
            hit_location: modal.hit_location,
            target_id,
        });
    }
    if ui.button("✕ Cancel").clicked() {
        cancel_events.send(CancelPhaseEvent);
    }
}

fn render_maneuver_confirmed(
    ui: &mut egui::Ui,
    modal: &mut ModalState,
    advance_events: &mut EventWriter<PhaseAdvanceEvent>,
    cancel_events: &mut EventWriter<CancelPhaseEvent>,
) {
    ui.heading("Maneuver Confirmed");
    ui.separator();
    ui.label(format!("Effective Skill: {}", modal.effective_skill));
    ui.separator();
    ui.label("Modifiers:");
    for (label, val) in &modal.modifier_breakdown {
        ui.label(format!("  {}: {:+}", label, val));
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Proceed to Attack Roll").clicked() {
            advance_events.send(PhaseAdvanceEvent);
        }
        if ui.button("✕ Cancel").clicked() {
            cancel_events.send(CancelPhaseEvent);
        }
    });
}

fn render_attack_roll(
    ui: &mut egui::Ui,
    modal: &mut ModalState,
    roll_events: &mut EventWriter<RollRequestedEvent>,
    cancel_events: &mut EventWriter<CancelPhaseEvent>,
) {
    ui.heading("Attack Roll");
    ui.separator();
    ui.label(format!("Effective Skill: {}", modal.effective_skill));
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Roll 3d6!").clicked() {
            let roll = roll_3d6();
            roll_events.send(RollRequestedEvent {
                label: format!("Attack roll: {}", roll),
                roll,
            });
        }
        if ui.button("✕ Cancel").clicked() {
            cancel_events.send(CancelPhaseEvent);
        }
    });
    if !modal.last_outcome_text.is_empty() {
        ui.separator();
        for line in &modal.last_outcome_text {
            ui.label(line);
        }
    }
}

fn render_defense_resolution(
    ui: &mut egui::Ui,
    current: &crate::model::GameState,
    modal: &mut ModalState,
    defense_events: &mut EventWriter<DefenseSelectedEvent>,
    advance_events: &mut EventWriter<PhaseAdvanceEvent>,
    cancel_events: &mut EventWriter<CancelPhaseEvent>,
) {
    ui.heading("Defense Resolution");

    if !modal.last_outcome_text.is_empty() {
        ui.separator();
        for line in &modal.last_outcome_text {
            ui.label(line);
        }
        ui.separator();
    }

    let Some(tid) = modal.target_id_for_modal else {
        ui.label("No defender.");
        if ui.button("Continue").clicked() {
            advance_events.send(PhaseAdvanceEvent);
        }
        return;
    };

    let Some(defender) = current.actors.get(&tid) else {
        ui.label("Defender not found.");
        if ui.button("Continue").clicked() {
            advance_events.send(PhaseAdvanceEvent);
        }
        return;
    };

    ui.label(format!("Defender: {}", defender.name));
    ui.separator();

    ui.label("Select Defense:");
    for (dt, val) in &modal.defense_options {
        let label = match dt {
            DefenseType::Dodge => format!("Dodge ({})", val),
            DefenseType::Parry { attack_index } => {
                let name = defender.attacks.get(*attack_index)
                    .map(|a| a.name.as_str()).unwrap_or("?");
                format!("Parry ({}) — {}", val, name)
            }
            DefenseType::Block { attack_index: _ } => format!("Block ({})", val),
        };
        if ui.button(&label).clicked() {
            defense_events.send(DefenseSelectedEvent {
                defender_id: tid,
                defense_type: *dt,
            });
            return;
        }
    }

    ui.separator();
    if ui.button("Skip Defense (no roll)").clicked() {
        advance_events.send(PhaseAdvanceEvent);
    }
    if ui.button("✕ Cancel").clicked() {
        cancel_events.send(CancelPhaseEvent);
    }
}
