use bevy::prelude::*;
use crate::model::{
    ActorId, CritHitResult, ExtraEffort, HitLocation,
    ManeuverRelation, ManeuverType, ManeuverPayload, Posture, RelationState, TurnPhase,
};
use crate::model::injury::resolve_injury;
use crate::model::rolls::{check_roll, crit_hit_table, roll_3d6, roll_damage, RollOutcome};
use crate::model::maneuver_legality::{available_maneuvers, combo_whitelist, is_directed_offensive};
use crate::logging::LogEvent;
use crate::settings::SettingsResource;
use crate::ui::battlemap::GameStateResource;

pub struct PhaseMachinePlugin;

impl Plugin for PhaseMachinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModalState>()
            .add_event::<ManeuverDeclaredEvent>()
            .add_event::<AttackSetupConfirmedEvent>()
            .add_event::<RollRequestedEvent>()
            .add_event::<DefenseSelectedEvent>()
            .add_event::<PhaseAdvanceEvent>()
            .add_event::<CancelPhaseEvent>()
            .add_systems(Update, process_phase_machine);
    }
}

#[derive(Event, Debug, Clone)]
pub struct ManeuverDeclaredEvent {
    pub source_id: ActorId,
    pub target_id: Option<ActorId>,
    pub target_hex: Option<(i32, i32)>,
    pub maneuver: ManeuverType,
    pub extra_efforts: Vec<ExtraEffort>,
}

#[derive(Event, Debug, Clone)]
pub struct AttackSetupConfirmedEvent {
    pub attacker_id: ActorId,
    pub attack_index: usize,
    pub hit_location: HitLocation,
    pub target_id: ActorId,
}

#[derive(Event, Debug, Clone)]
pub struct RollRequestedEvent {
    pub label: String,
    pub roll: u8,
}

#[derive(Event, Debug, Clone)]
pub struct DefenseSelectedEvent {
    pub defender_id: ActorId,
    pub defense_type: DefenseType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefenseType {
    Dodge,
    Parry { attack_index: usize },
    Block { attack_index: usize },
}

#[derive(Event, Debug, Clone)]
pub struct PhaseAdvanceEvent;

#[derive(Event, Debug, Clone)]
pub struct CancelPhaseEvent;

#[derive(Resource, Default)]
pub struct ModalState {
    pub show: bool,
    pub pending_roll: Option<u8>,
    pub attack_index: usize,
    pub hit_location: HitLocation,
    pub target_id_for_modal: Option<ActorId>,
    pub modifier_breakdown: Vec<(String, i8)>,
    pub effective_skill: u8,
    pub rolled_damage: u32,
    pub last_outcome_text: Vec<String>,
    pub defense_options: Vec<(DefenseType, u8)>,
    pub pending_defense: Option<(ActorId, DefenseType)>,
    pub pending_crit_result: Option<CritHitResult>,
}

fn process_phase_machine(
    mut state: Option<ResMut<GameStateResource>>,
    settings: Option<Res<SettingsResource>>,
    mut modal: ResMut<ModalState>,
    mut maneuver_events: EventReader<ManeuverDeclaredEvent>,
    mut attack_events: EventReader<AttackSetupConfirmedEvent>,
    mut roll_events: EventReader<RollRequestedEvent>,
    mut defense_events: EventReader<DefenseSelectedEvent>,
    mut advance_events: EventReader<PhaseAdvanceEvent>,
    mut cancel_events: EventReader<CancelPhaseEvent>,
    mut log_events: EventWriter<LogEvent>,
) {
    let Some(ref mut state_res) = state else { return };
    let history = &mut state_res.history;
    let phase = history.current().current_phase;
    let shock_enabled = settings.map(|s| s.settings.shock_enabled).unwrap_or(true);
    let ctx_round = history.current().round;
    let ctx_turn = history.current().current_actor;

    // --- Complete auto-advances every frame (no event needed) ---
    if phase == TurnPhase::Complete {
        advance_turn(history, &mut modal, &mut log_events, ctx_round, ctx_turn);
        return;
    }

    match phase {
        TurnPhase::ManeuverSelection => {
            for ev in maneuver_events.read() {
                let cur = history.current();
                info!("PhaseMachine: received ManeuverDeclaredEvent source={} target={:?} maneuver={:?} (current_actor={})",
                    ev.source_id, ev.target_id, ev.maneuver, cur.current_actor);
                if ev.source_id != cur.current_actor {
                    info!("PhaseMachine: skipping — source {} != current {}", ev.source_id, cur.current_actor);
                    continue;
                }
                let Some(actor) = cur.actors.get(&ev.source_id) else { continue };
                let available = available_maneuvers(actor);
                if !available.contains(&ev.maneuver) {
                    log_events.send(LogEvent::warn(format!(
                        "{:?} not available for {}", ev.maneuver, actor.name
                    )));
                    continue;
                }
                let combos = combo_whitelist(ev.maneuver);
                for eff in &ev.extra_efforts {
                    if !combos.contains(eff) {
                        log_events.send(LogEvent::warn(format!(
                            "{:?} not allowed with {:?}", eff, ev.maneuver
                        )));
                        continue;
                    }
                }

                let mut new_state = cur.clone();
                {
                    let a = new_state.actors.get_mut(&ev.source_id).unwrap();
                    a.current_maneuver = Some(ev.maneuver);
                    a.extra_effort = ev.extra_efforts.clone();

                    if ev.maneuver == ManeuverType::Move {
                        if let Some(hex) = ev.target_hex {
                            let max_dist = a.effective_move() as i32;
                            let dx = hex.0 - a.position.0;
                            let dy = hex.1 - a.position.1;
                            let dist = dx.abs().max(dy.abs());
                            if dist <= max_dist && dist > 0 {
                                a.position = hex;
                                info!("Move: {} moved to {:?}", a.name, hex);
                            } else {
                                info!("Move: {} can't reach {:?} (dist={}, max={})", a.name, hex, dist, max_dist);
                            }
                        }
                    }
                }

                let is_aim_or_eval = matches!(ev.maneuver, ManeuverType::Aim | ManeuverType::Evaluate);
                let offensive = is_directed_offensive(ev.maneuver) && !is_aim_or_eval;

                if let Some(target_id) = ev.target_id {
                    let payload = if ev.maneuver == ManeuverType::Aim {
                        let weapon_acc = actor.attacks.first()
                            .and_then(|a| a.acc)
                            .unwrap_or(0);
                        ManeuverPayload::AimBonus { accumulated: 0, turns: 0, weapon_acc }
                    } else if ev.maneuver == ManeuverType::Evaluate {
                        ManeuverPayload::EvaluateBonus { accumulated: 0 }
                    } else {
                        ManeuverPayload::None
                    };

                    new_state.relations.push(ManeuverRelation {
                        source: ev.source_id,
                        target: target_id,
                        maneuver: ev.maneuver,
                        state: RelationState::Active,
                        payload,
                    });
                }

                let name = new_state.actors.get(&ev.source_id).unwrap().name.clone();
                let attacks_per_turn = new_state.actors.get(&ev.source_id).unwrap().attacks_per_turn;
                let active_attack = new_state.actors.get(&ev.source_id).unwrap().active_attack;

                if offensive {
                    new_state.current_phase = TurnPhase::AttackSetup;
                    info!("PhaseMachine: offensive {:?} → AttackSetup, attacks_remaining={}", ev.maneuver, attacks_per_turn);
                    new_state.attacks_remaining = attacks_per_turn;
                    modal.show = true;
                    modal.attack_index = active_attack.unwrap_or(0);
                    modal.hit_location = HitLocation::Torso;
                    modal.target_id_for_modal = ev.target_id;
                    modal.modifier_breakdown.clear();
                    modal.effective_skill = 0;
                    modal.defense_options.clear();
                    modal.last_outcome_text.clear();
                    modal.pending_defense = None;
                    modal.pending_crit_result = None;
                } else {
                    // Non-offensive: accumulate Aim/Evaluate, then go straight to Complete
                    process_non_combat_accumulation(&mut new_state, ev.source_id);
                    new_state.current_phase = TurnPhase::Complete;
                    modal.show = false;
                    info!("PhaseMachine: non-offensive {:?} → Complete", ev.maneuver);
                }

                history.push(new_state);
                log_events.send(LogEvent::info(format!(
                    "{} declared {:?}", name, ev.maneuver
                )).with_phase_context(ctx_round, ctx_turn, phase));
            }
        }

        TurnPhase::AttackSetup => {
            for _ev in cancel_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                new_state.current_phase = TurnPhase::ManeuverSelection;
                if let Some(actor) = new_state.actors.get_mut(&cur.current_actor) {
                    actor.current_maneuver = None;
                }
                modal.show = false;
                history.push(new_state);
                info!("PhaseMachine: AttackSetup cancelled");
            }
            for ev in attack_events.read() {
                let cur = history.current();
                let Some(attacker) = cur.actors.get(&ev.attacker_id) else { continue };
                let Some(attack) = attacker.attacks.get(ev.attack_index) else { continue };

                let base_skill = attack.skill_level;
                let mut mods: Vec<(String, i8)> = Vec::new();
                let mut total_mod: i8 = 0;

                mods.push(("Base skill".into(), base_skill as i8));

                let hit_pen = ev.hit_location.to_hit_penalty();
                if hit_pen != 0 {
                    mods.push((format!("Hit location ({:?})", ev.hit_location), hit_pen));
                    total_mod += hit_pen;
                }

                match attacker.posture {
                    Posture::Prone => { mods.push(("Prone posture".into(), -4)); total_mod -= 4; }
                    Posture::Kneeling | Posture::Crawling => { mods.push(("Kneeling/Crawling".into(), -2)); total_mod -= 2; }
                    _ => {}
                }

                if let Some(target) = cur.actors.get(&ev.target_id) {
                    let dist = crate::model::hex_distance(attacker.position, target.position);
                    let rp = crate::model::range_penalty(dist as f32);
                    if attack.is_ranged && rp != 0 {
                        mods.push((format!("Range ({} yd)", dist), rp));
                        total_mod += rp;
                    }
                    if target.sm != 0 {
                        mods.push((format!("Target SM ({})", target.sm), target.sm));
                        total_mod += target.sm;
                    }
                    match target.posture {
                        Posture::Prone => { mods.push(("Target prone".into(), -3)); total_mod -= 3; }
                        Posture::Kneeling | Posture::Crawling | Posture::Sitting => {
                            mods.push((format!("Target {:?}", target.posture), -2)); total_mod -= 2;
                        }
                        _ => {}
                    }
                }

                for r in &cur.relations {
                    if r.source == ev.attacker_id && r.target == ev.target_id {
                        match &r.payload {
                            ManeuverPayload::AimBonus { accumulated, turns: _, weapon_acc: _ } => {
                                let bonus = *accumulated as i8;
                                if bonus > 0 {
                                    mods.push(("Aim bonus".into(), bonus));
                                    total_mod += bonus;
                                }
                            }
                            ManeuverPayload::EvaluateBonus { accumulated } => {
                                let bonus = *accumulated as i8;
                                if bonus > 0 {
                                    mods.push(("Evaluate bonus".into(), bonus));
                                    total_mod += bonus;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                for m in &cur.global_modifiers {
                    if matches!(m.applies_to, crate::model::ModifierTarget::AllRolls | crate::model::ModifierTarget::AttackRolls) {
                        mods.push((m.label.clone(), m.value));
                        total_mod += m.value;
                    }
                }
                for m in &attacker.individual_modifiers {
                    mods.push((m.label.clone(), m.value));
                    total_mod += m.value;
                }

                let effective_skill = (base_skill as i16 + total_mod as i16).max(0) as u8;
                let attack_name = attack.name.clone();
                let hit_loc = ev.hit_location;

                let mut new_state = cur.clone();
                new_state.actors.get_mut(&ev.attacker_id).unwrap().active_attack = Some(ev.attack_index);
                new_state.current_phase = TurnPhase::ManeuverConfirmed;

                modal.modifier_breakdown = mods;
                modal.effective_skill = effective_skill;
                modal.attack_index = ev.attack_index;
                modal.hit_location = ev.hit_location;

                history.push(new_state);
                log_events.send(LogEvent::info(format!(
                    "Attack setup: {} → {:?}, effective skill {}",
                    attack_name, hit_loc, effective_skill
                )).with_phase_context(ctx_round, ctx_turn, phase));
            }
        }

        TurnPhase::ManeuverConfirmed => {
            for _ev in cancel_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                new_state.current_phase = TurnPhase::ManeuverSelection;
                if let Some(actor) = new_state.actors.get_mut(&cur.current_actor) {
                    actor.current_maneuver = None;
                }
                modal.show = false;
                history.push(new_state);
            }
            for _ev in advance_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                new_state.current_phase = TurnPhase::AttackRoll;
                history.push(new_state);
            }
        }

        TurnPhase::AttackRoll => {
            for _ev in cancel_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                new_state.current_phase = TurnPhase::ManeuverSelection;
                if let Some(actor) = new_state.actors.get_mut(&cur.current_actor) {
                    actor.current_maneuver = None;
                }
                modal.show = false;
                history.push(new_state);
            }
            for ev in roll_events.read() {
                let cur = history.current();
                let skill = modal.effective_skill;
                let outcome = check_roll(ev.roll, skill);

                log_events.send(LogEvent::info(format!(
                    "Attack roll: {} vs {}, outcome {:?}", ev.roll, skill, outcome
                )).with_phase_context(ctx_round, ctx_turn, phase));

                let mut new_state = cur.clone();

                match outcome {
                    RollOutcome::CriticalSuccess => {
                        let crit = crit_hit_table(roll_3d6());
                        modal.pending_crit_result = Some(crit);
                        modal.last_outcome_text = vec![
                            format!("Roll: {} — CRITICAL SUCCESS! ({:?})", ev.roll, crit),
                        ];
                        // Only modify damage here; non-damage crit effects pass through to injury
                        let a = cur.actors.get(&cur.current_actor).unwrap();
                        if let Some(idx) = a.active_attack {
                            if let Some(atk) = a.attacks.get(idx) {
                                let base_dmg = roll_damage(atk.damage_dice, atk.damage_adds);
                                modal.rolled_damage = match crit.modifies_damage() {
                                    Some(2) => base_dmg * 2,
                                    Some(3) => base_dmg * 3,
                                    _ => base_dmg,
                                };
                                log_events.send(LogEvent::info(format!(
                                    "Damage roll: {}d{:+} = {} (crit: {:?})", atk.damage_dice, atk.damage_adds, base_dmg, crit
                                )).with_phase_context(ctx_round, ctx_turn, phase));
                                modal.last_outcome_text.push(format!(
                                    "Damage: {}d{:+} = {}", atk.damage_dice, atk.damage_adds, base_dmg
                                ));
                            }
                        }
                        modal.pending_roll = Some(ev.roll);
                        new_state.current_phase = TurnPhase::DefenseResolution;
                        compute_defense_options(&cur, &mut modal);
                    }
                    RollOutcome::Success => {
                        modal.pending_crit_result = None;
                        modal.last_outcome_text = vec![format!(
                            "Roll: {} — HIT (vs {})", ev.roll, skill
                        )];
                        let a = cur.actors.get(&cur.current_actor).unwrap();
                        if let Some(idx) = a.active_attack {
                            if let Some(atk) = a.attacks.get(idx) {
                                let dmg = roll_damage(atk.damage_dice, atk.damage_adds);
                                modal.rolled_damage = dmg;
                                log_events.send(LogEvent::info(format!(
                                    "Damage roll: {}d{:+} = {}", atk.damage_dice, atk.damage_adds, dmg
                                )).with_phase_context(ctx_round, ctx_turn, phase));
                                modal.last_outcome_text.push(format!(
                                    "Damage: {}d{:+} = {}", atk.damage_dice, atk.damage_adds, dmg
                                ));
                            }
                        }
                        modal.pending_roll = Some(ev.roll);
                        new_state.current_phase = TurnPhase::DefenseResolution;
                        compute_defense_options(&cur, &mut modal);
                    }
                    _ => {
                        modal.pending_crit_result = None;
                        modal.last_outcome_text = vec![format!(
                            "Roll: {} — MISS (vs {})", ev.roll, skill
                        )];
                        modal.show = false;
                        new_state.current_phase = TurnPhase::Complete;
                    }
                }

                history.push(new_state);
            }
        }

        TurnPhase::DefenseResolution => {
            for _ev in cancel_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                new_state.current_phase = TurnPhase::ManeuverSelection;
                if let Some(actor) = new_state.actors.get_mut(&cur.current_actor) {
                    actor.current_maneuver = None;
                }
                modal.show = false;
                history.push(new_state);
            }
            for ev in defense_events.read() {
                let cur = history.current();
                let d_id = ev.defender_id;
                let d_val = defense_value(&cur, d_id, ev.defense_type);

                let roll = roll_3d6();
                let outcome = check_roll(roll, d_val);

                log_events.send(LogEvent::info(format!(
                    "Defense roll: {} vs {}, outcome {:?}", roll, d_val, outcome
                )).with_phase_context(ctx_round, ctx_turn, phase));

                let mut new_state = cur.clone();

                match outcome {
                    RollOutcome::CriticalSuccess | RollOutcome::Success => {
                        modal.last_outcome_text.push(format!(
                            "Defense: {} vs {} — DEFENDED!", roll, d_val
                        ));
                        modal.show = false;
                        new_state.current_phase = TurnPhase::Complete;
                    }
                    _ => {
                        modal.last_outcome_text.push(format!(
                            "Defense: {} vs {} — FAILED!", roll, d_val
                        ));

                        apply_pending_injury(
                            &cur, &mut new_state, &mut modal, &mut log_events,
                            shock_enabled, ctx_round, ctx_turn,
                        );

                        modal.show = false;
                        new_state.current_phase = TurnPhase::Complete;
                    }
                }

                history.push(new_state);
            }

            for _ev in advance_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();
                apply_pending_injury(
                    &cur, &mut new_state, &mut modal, &mut log_events,
                    shock_enabled, ctx_round, ctx_turn,
                );
                new_state.current_phase = TurnPhase::Complete;
                modal.show = false;
                history.push(new_state);
            }
        }

        TurnPhase::NonCombatResolution => {
            for _ev in advance_events.read() {
                let cur = history.current();
                let mut new_state = cur.clone();

                for relation in &mut new_state.relations {
                    if relation.source == cur.current_actor {
                        match &mut relation.payload {
                            ManeuverPayload::AimBonus { accumulated, turns, weapon_acc } => {
                                *turns += 1;
                                *accumulated = (*weapon_acc + *turns - 1)
                                    .min(*weapon_acc + 2);
                            }
                            ManeuverPayload::EvaluateBonus { accumulated } => {
                                *accumulated = (*accumulated + 1).min(3);
                            }
                            _ => {}
                        }
                    }
                }

                new_state.current_phase = TurnPhase::Complete;
                history.push(new_state);
            }
        }

        _ => {}
    }
}

fn advance_turn(
    history: &mut crate::model::GameStateHistory,
    modal: &mut ResMut<ModalState>,
    log_events: &mut EventWriter<LogEvent>,
    ctx_round: u32,
    ctx_turn: u64,
) {
    let cur = history.current();
    let mut new_state = cur.clone();
    let cur_idx = new_state.turn_order.iter()
        .position(|&id| id == cur.current_actor)
        .unwrap_or(0);

    new_state.current_phase = TurnPhase::ManeuverSelection;

    let atk_id = cur.current_actor;
    if let Some(actor) = new_state.actors.get_mut(&atk_id) {
        // current_maneuver persists until next ManeuverSelection — needed for AOA defense denial
            // actor.current_maneuver = None;
        actor.extra_effort.clear();
        if actor.flags.stunned {
            actor.flags.stun_turns += 1;
        }
    }

    new_state.relations.retain(|r| {
        matches!(r.payload, ManeuverPayload::AimBonus { .. } | ManeuverPayload::EvaluateBonus { .. })
    });

    let has_extra_attacks = cur.attacks_remaining > 1;

    if has_extra_attacks {
        new_state.attacks_remaining = cur.attacks_remaining - 1;
    } else {
        let n = new_state.turn_order.len().max(1);
        let next_idx = (cur_idx + 1) % n;
        if next_idx == 0 {
            new_state.round += 1;
        }
        if let Some(&next_id) = new_state.turn_order.get(next_idx) {
            new_state.current_actor = next_id;
        }
        new_state.attacks_remaining = new_state.actors
            .get(&new_state.current_actor)
            .map(|a| a.attacks_per_turn)
            .unwrap_or(1);
    }

    modal.show = false;
    modal.last_outcome_text.clear();
    modal.pending_crit_result = None;

    let report_round = new_state.round;
    let next_name = new_state.actors.get(&new_state.current_actor)
        .map(|a| a.name.clone()).unwrap_or_else(|| "nobody".into());

    history.push(new_state);
    log_events.send(LogEvent::info(format!(
        "Turn complete → Round {}, next: {}",
        report_round, next_name
    )).with_context(ctx_round, ctx_turn));
}

fn apply_pending_injury(
    cur: &crate::model::GameState,
    new_state: &mut crate::model::GameState,
    modal: &mut ModalState,
    log_events: &mut EventWriter<LogEvent>,
    shock_enabled: bool,
    ctx_round: u32,
    ctx_turn: u64,
) {
    let crit = modal.pending_crit_result;
    let Some(d_id) = modal.target_id_for_modal else { return };
    let attacker = cur.actors.get(&cur.current_actor);
    let target = cur.actors.get(&d_id);
    if let (Some(attacker), Some(_target)) = (attacker, target) {
        if let Some(idx) = attacker.active_attack {
            if let Some(atk) = attacker.attacks.get(idx) {
                let loc = modal.hit_location;
                let dmg = if modal.rolled_damage > 0 {
                    modal.rolled_damage
                } else {
                    let re_rolled = roll_damage(atk.damage_dice, atk.damage_adds);
                    log_events.send(LogEvent::info(format!(
                        "Damage roll (re-roll): {}d{:+} = {}", atk.damage_dice, atk.damage_adds, re_rolled
                    )).with_context(ctx_round, ctx_turn));
                    re_rolled
                };
                match resolve_injury(
                    cur, d_id, loc, dmg, atk.damage_type,
                    crit, shock_enabled,
                ) {
                    Ok((inj_state, o)) => {
                        *new_state = inj_state;
                        modal.last_outcome_text.push(format!(
                            "Injury: {} {:?} to {:?}, DR {} → {} hp",
                            dmg, atk.damage_type, loc, o.effective_dr, o.hp_lost
                        ));
                        if o.major_wound {
                            modal.last_outcome_text.push("Major wound!".into());
                        }
                        if o.knockdown {
                            modal.last_outcome_text.push(format!(
                                "Knockdown! (mod {})", o.knockdown_modifier
                            ));
                        }
                        if o.dead {
                            modal.last_outcome_text.push("DEAD!".into());
                        }
                        log_events.send(LogEvent::info(format!(
                            "Injury resolved: {} to {} ({:?}) — {} hp lost",
                            attacker.name, dmg, loc, o.hp_lost
                        )).with_context(ctx_round, ctx_turn));
                    }
                    Err(e) => {
                        log_events.send(LogEvent::error(format!("Injury: {e}")));
                    }
                }
            }
        }
    }
}

fn defense_value(state: &crate::model::GameState, actor_id: ActorId, dt: DefenseType) -> u8 {
    let Some(actor) = state.actors.get(&actor_id) else { return 0 };
    if denies_active_defenses(actor.current_maneuver) {
        return 0;
    }
    match dt {
        DefenseType::Dodge => actor.dodge(),
        DefenseType::Parry { attack_index } => {
            actor.attacks.get(attack_index)
                .and_then(|a| a.parry_bonus)
                .map(|p| (actor.basic_speed / 2.0).floor() as u8 + 3 + p.max(0) as u8)
                .unwrap_or(0)
        }
        DefenseType::Block { attack_index } => {
            actor.attacks.get(attack_index)
                .and_then(|a| a.block_bonus)
                .map(|b| (actor.basic_speed / 2.0).floor() as u8 + 3 + b.max(0) as u8)
                .unwrap_or(0)
        }
    }
}

fn denies_active_defenses(maneuver: Option<ManeuverType>) -> bool {
    match maneuver {
        Some(ManeuverType::AllOutAttackDetermined)
        | Some(ManeuverType::AllOutAttackDouble)
        | Some(ManeuverType::AllOutAttackFeint)
        | Some(ManeuverType::AllOutAttackLong)
        | Some(ManeuverType::AllOutAttackStrong)
        | Some(ManeuverType::AllOutAttackRangedDetermined)
        | Some(ManeuverType::AllOutConcentrate) => true,
        _ => false,
    }
}

fn compute_defense_options(state: &crate::model::GameState, modal: &mut ModalState) {
    modal.defense_options.clear();
    let Some(tid) = modal.target_id_for_modal else { return };
    let Some(target) = state.actors.get(&tid) else { return };
    if denies_active_defenses(target.current_maneuver) {
        // All-Out Attack — no active defenses available
        return;
    }
    modal.defense_options.push((DefenseType::Dodge, target.dodge()));
    for (i, atk) in target.attacks.iter().enumerate() {
        if let Some(pb) = atk.parry_bonus {
            let val = (target.basic_speed / 2.0).floor() as u8 + 3 + pb.max(0) as u8;
            modal.defense_options.push((DefenseType::Parry { attack_index: i }, val));
        }
        if let Some(bb) = atk.block_bonus {
            let val = (target.basic_speed / 2.0).floor() as u8 + 3 + bb.max(0) as u8;
            modal.defense_options.push((DefenseType::Block { attack_index: i }, val));
        }
    }
}

fn process_non_combat_accumulation(new_state: &mut crate::model::GameState, source_id: ActorId) {
    for relation in &mut new_state.relations {
        if relation.source == source_id {
            match &mut relation.payload {
                ManeuverPayload::AimBonus { accumulated, turns, weapon_acc } => {
                    *turns += 1;
                    *accumulated = (*weapon_acc + *turns - 1)
                        .min(*weapon_acc + 2);
                }
                ManeuverPayload::EvaluateBonus { accumulated } => {
                    *accumulated = (*accumulated + 1).min(3);
                }
                _ => {}
            }
        }
    }
}
