use super::theme::apply_theme;
use crate::logging::LogEvent;
use crate::model::maneuver_legality::{available_maneuvers, is_directed_offensive};
use crate::model::{ActorId, ManeuverType, PainThreshold, Posture, TurnPhase};
use crate::settings::SettingsResource;
use crate::systems::phase_machine::ManeuverDeclaredEvent;
use crate::ui::battlemap::{
    EventLogResource, GameStateResource, GmActionEvent, ImportSheetEvent, ManeuverDragState,
    ReloadSheetEvent, RemoveActorEvent,
};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub struct PanelsPlugin;

impl Plugin for PanelsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply_theme);
        app.add_systems(Update, render_panels);
    }
}

#[derive(Default)]
struct PanelUiState {
    portrait_cache: std::collections::HashMap<String, egui::TextureHandle>,
    selected_maneuver: Option<ManeuverType>,
    selected_tab: usize,
    expanded_actor: Option<ActorId>,
    just_opened_actor: Option<ActorId>,
    pending_reorder: Option<(usize, usize)>,
}

pub fn hex_under_cursor(
    cursor: Option<Vec2>,
    camera: &Transform,
    proj: &OrthographicProjection,
    hex_size: f32,
    window_size: Vec2,
) -> Option<(i32, i32)> {
    let cursor = cursor?;
    let world = camera.translation.truncate();
    // Camera2d has viewport centered at (0,0) — convert screen coords (top-left origin) to world coords
    let px = (cursor.x - window_size.x / 2.0) * proj.scale + world.x;
    let py = (window_size.y / 2.0 - cursor.y) * proj.scale + world.y;

    let s = hex_size;
    let q_float = (2.0 / 3.0 * px) / s;
    let r_float = (-1.0 / 3.0 * px + 3f32.sqrt() / 3.0 * py) / s;

    let q = q_float.round();
    let r = r_float.round();
    let q_diff = q_float - q;
    let r_diff = r_float - r;

    let (q, r) = if q_diff.abs() >= r_diff.abs() {
        let s_cube = -q - r;
        let s_diff = -q_float - r_float - s_cube;
        if q_diff.abs() >= s_diff.abs() {
            ((q_float - q_diff).round() as i32, r as i32)
        } else {
            (q as i32, (r_float - r_diff).round() as i32)
        }
    } else {
        (q as i32, (r_float - r_diff).round() as i32)
    };

    Some((q, r))
}

#[allow(clippy::too_many_arguments)]
fn render_panels(
    mut egui_ctx: EguiContexts,
    state: Option<Res<GameStateResource>>,
    event_log: Option<Res<EventLogResource>>,
    settings: Option<Res<SettingsResource>>,
    cameras: Query<(&Transform, &OrthographicProjection), With<Camera>>,
    window: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut ui_state: Local<PanelUiState>,
    mut import_events: EventWriter<ImportSheetEvent>,
    mut reload_events: EventWriter<ReloadSheetEvent>,
    mut remove_events: EventWriter<RemoveActorEvent>,
    mut gm_events: EventWriter<GmActionEvent>,
    mut log_events: EventWriter<LogEvent>,
    mut maneuver_declared: EventWriter<ManeuverDeclaredEvent>,
    mut drag_state: ResMut<ManeuverDragState>,
) {
    let ctx = egui_ctx.ctx_mut();

    let current = state.as_ref().map(|s| s.history.current());
    let current_actor = current.and_then(|c| c.actors.get(&c.current_actor));
    let phase = current
        .map(|c| c.current_phase)
        .unwrap_or(TurnPhase::ManeuverSelection);
    let round = current.map(|c| c.round).unwrap_or(0);

    let Ok(w) = window.get_single() else { return };
    let cursor_pos = w.cursor_position();
    let win_size = Vec2::new(w.width(), w.height());
    let cursor_hex = if let Ok((cam, proj)) = cameras.get_single() {
        hex_under_cursor(cursor_pos, cam, proj, 32.0, win_size)
    } else {
        None
    };

    let secondary_clicked = ctx.input(|i| i.pointer.secondary_clicked());
    let secondary_click_pos = if secondary_clicked {
        ctx.input(|i| i.pointer.interact_pos())
    } else {
        None
    };

    // Token right-click detection (egui path)
    if ctx.input(|i| i.pointer.secondary_clicked()) {
        if let Some(hex) = cursor_hex {
            if let Some(state) = &state {
                let cur = state.history.current();
                for (&actor_id, actor) in &cur.actors {
                    if actor.position == hex {
                        let was_expanded = ui_state.expanded_actor == Some(actor_id);
                        ui_state.expanded_actor = if was_expanded { None } else { Some(actor_id) };
                        if !was_expanded {
                            ui_state.just_opened_actor = Some(actor_id);
                        }
                        break;
                    }
                }
            }
        }
    }

    // phase + round bar at top
    egui::TopBottomPanel::top("phase_bar")
        .min_height(22.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Round {}  |  Phase: {:?}  |  ", round, phase));
                if let Some(actor) = current_actor {
                    ui.colored_label(
                        egui::Color32::from_rgb(0x3A, 0x7B, 0xD5),
                        actor.name.to_string(),
                    );
                } else {
                    ui.label("No actor");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((q, r)) = cursor_hex {
                        ui.label(format!("Hex ({q}, {r})"));
                    }
                    if let Some(_state) = &state {
                        if let Some(actor) = current_actor {
                            let dist = crate::model::hex_distance(
                                actor.position,
                                cursor_hex.unwrap_or(actor.position),
                            );
                            ui.label(format!("Range: {} yd", dist));
                        }
                    }
                });
            });
        });

    let tray_h = settings
        .as_ref()
        .map(|s| s.settings.maneuver_tray_height)
        .unwrap_or(190.0);
    let log_h = settings
        .as_ref()
        .map(|s| s.settings.event_log_height)
        .unwrap_or(120.0);

    egui::TopBottomPanel::bottom("maneuver_tray")
        .default_height(tray_h)
        .min_height(tray_h)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Maneuvers");
                if drag_state.dragging.is_some() {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xFF, 0x66, 0x00),
                        " ▲ Drop on token or Esc to cancel",
                    );
                }
            });
            ui.separator();

            if let Some(actor) = current_actor {
                let available = available_maneuvers(actor);
                let avail_count = available.len();
                let scroll_h = 162.0;
                let content_w = avail_count as f32 * 135.0 + 8.0;

                egui::ScrollArea::horizontal()
                    .id_salt("maneuver_scroll")
                    .max_height(scroll_h)
                    .show(ui, |ui| {
                        ui.set_height(scroll_h);
                        ui.set_width(content_w);
                        ui.horizontal(|ui| {
                            for m in &available {
                                let color = maneuver_category_color_egui(*m);
                                let is_armed = drag_state.dragging == Some(*m);
                                let name = maneuver_display_name(*m);
                                let desc = maneuver_description(*m);
                                let response = draw_maneuver_card(ui, &name, desc, color, is_armed);

                                let clicked = response.clicked();
                                let drag_started =
                                    response.drag_started_by(egui::PointerButton::Primary);

                                if clicked {
                                    info!(
                                        "Card CLICKED: {:?} (armed={}, hovered={}, rect={:?})",
                                        m,
                                        is_armed,
                                        response.hovered(),
                                        response.rect
                                    );
                                    if drag_state.dragging == Some(*m) {
                                        drag_state.dragging = None;
                                        info!("Deselected {:?}", m);
                                    } else {
                                        drag_state.dragging = Some(*m);
                                        info!("Armed {:?} via click", m);
                                    }
                                } else if drag_started {
                                    info!("Card DRAG_STARTED: {:?}", m);
                                    drag_state.dragging = Some(*m);
                                    info!("Armed {:?} via drag", m);
                                }
                            }
                        });
                    });

                if let Some(m) = ui_state.selected_maneuver {
                    if !available.contains(&m) {
                        ui_state.selected_maneuver = None;
                    }
                }
            } else {
                ui.label("No actor in turn order.");
            }
        });

    egui::SidePanel::right("side_panel")
        .min_width(280.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Character Panel");
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Import Sheet").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("GCS Sheets", &["gcs", "json"])
                        .pick_file()
                    {
                        let path_str = path.to_string_lossy().to_string();
                        import_events.send(ImportSheetEvent { path: path_str });
                    } else {
                        log_events.send(LogEvent::info("Import cancelled by user."));
                    }
                }
            });
            ui.separator();

            if let Some(state) = &state {
                let cur = state.history.current();
                let actor_ids: Vec<ActorId> = cur.turn_order.clone();

                for (index, &actor_id) in actor_ids.iter().enumerate() {
                    let Some(actor) = cur.actors.get(&actor_id) else {
                        continue;
                    };
                    let is_current = actor_id == cur.current_actor;
                    let is_expanded = ui_state.expanded_actor == Some(actor_id);

                    let PanelUiState {
                        ref mut portrait_cache,
                        ref mut pending_reorder,
                        ..
                    } = *ui_state;
                    let card_rect = draw_actor_card(
                        ui,
                        actor,
                        is_current,
                        index,
                        actor_ids.len(),
                        portrait_cache,
                        pending_reorder,
                        cur,
                    );

                    // Process reorder request
                    if let Some((from, to)) = ui_state.pending_reorder.take() {
                        gm_events.send(GmActionEvent::ReorderTurnOrder {
                            from_index: from,
                            to_index: to,
                        });
                    }

                    // Right-click on card
                    if let Some(pos) = secondary_click_pos {
                        if card_rect.contains(pos) {
                            let was_expanded = ui_state.expanded_actor == Some(actor_id);
                            ui_state.expanded_actor =
                                if was_expanded { None } else { Some(actor_id) };
                            if !was_expanded {
                                ui_state.just_opened_actor = Some(actor_id);
                            }
                        }
                    }

                    // Inline dropdown menu
                    if is_expanded {
                        let PanelUiState {
                            ref mut expanded_actor,
                            ..
                        } = *ui_state;
                        let dropdown_rect = draw_actor_dropdown(
                            ui,
                            actor,
                            cur,
                            &mut reload_events,
                            &mut remove_events,
                            &mut gm_events,
                            expanded_actor,
                        );
                        // Close if click outside both card and dropdown, unless just opened
                        if let Some(pos) = secondary_click_pos {
                            if Some(actor_id) != ui_state.just_opened_actor
                                && !card_rect.contains(pos)
                                && !dropdown_rect.contains(pos)
                            {
                                ui_state.expanded_actor = None;
                            }
                        }
                    }
                }
            } else {
                ui.label("No characters loaded.");
            }

            ui.separator();

            egui::TopBottomPanel::bottom("side_bottom_tabs")
                .min_height(200.0)
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(ui_state.selected_tab == 0, "Attacks")
                            .clicked()
                        {
                            ui_state.selected_tab = 0;
                        }
                        if ui
                            .selectable_label(ui_state.selected_tab == 1, "GM Config")
                            .clicked()
                        {
                            ui_state.selected_tab = 1;
                        }
                    });
                    ui.separator();

                    match ui_state.selected_tab {
                        0 => {
                            if let Some(actor) = current_actor {
                                for (i, atk) in actor.attacks.iter().enumerate() {
                                    let is_active = actor.active_attack == Some(i);
                                    let label = format!(
                                        "{}  {}  {}d{:+} {:?}  lvl {}  parry {}",
                                        if is_active { ">" } else { " " },
                                        atk.name,
                                        atk.damage_dice,
                                        atk.damage_adds,
                                        atk.damage_type,
                                        atk.skill_level,
                                        atk.parry_bonus
                                            .map(|p| p.to_string())
                                            .unwrap_or_else(|| "—".into()),
                                    );
                                    ui.label(label);
                                }
                            }
                        }
                        1 => {
                            gm_config_panel(
                                ui,
                                state.as_deref(),
                                settings.as_deref(),
                                &mut gm_events,
                            );
                        }
                        _ => {}
                    }
                });
        });

    egui::TopBottomPanel::bottom("event_log")
        .default_height(log_h)
        .min_height(log_h)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Event Log");
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(log_h - 30.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if let Some(log_res) = &event_log {
                        for entry in log_res.log.entries.iter().rev() {
                            let color = match entry.kind {
                                crate::model::LogEntryKind::Error => {
                                    egui::Color32::from_rgb(0xCC, 0x33, 0x33)
                                }
                                crate::model::LogEntryKind::Warning => {
                                    egui::Color32::from_rgb(0xCC, 0xAA, 0x00)
                                }
                                _ => egui::Color32::from_rgb(0x88, 0x88, 0x88),
                            };
                            ui.colored_label(
                                color,
                                format!("[R{}] {:?} | {}", entry.round, entry.kind, entry.message),
                            );
                        }
                    }
                });
        });

    // ESC cancels drag AND closes dropdown
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if drag_state.dragging.is_some() {
            info!("ESC pressed — cancelling drag");
            drag_state.dragging = None;
        }
        if ui_state.expanded_actor.is_some() {
            ui_state.expanded_actor = None;
        }
    }

    // ———— Drag-and-drop: visual indicator + drop resolution ————
    if let Some(m) = drag_state.dragging {
        let primary_released = mouse.just_released(MouseButton::Left);
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());

        // Draw ghost card following cursor
        if let Some(cursor) = hover_pos {
            let color = maneuver_category_color_egui(m);
            let name = maneuver_display_name(m);
            let desc = maneuver_description(m);
            let card_w = 125.0;
            let card_h = 150.0;

            egui::Area::new("drag_ghost".into())
                .fixed_pos(cursor + egui::Vec2::new(10.0, 10.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let (rect, _) = ui
                        .allocate_exact_size(egui::Vec2::new(card_w, card_h), egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(0x16, 0x16, 0x16, 220),
                    );
                    // Solid colored border (5px)
                    painter.rect_stroke(rect, 0.0, egui::Stroke::new(5.0, color));

                    let margin = 8.0;
                    let inner = rect.shrink(margin);

                    // Name header
                    painter.text(
                        inner.min + egui::Vec2::new(0.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        &name,
                        egui::FontId::proportional(13.0),
                        egui::Color32::from_rgb(0xE0, 0xE0, 0xE0),
                    );

                    // Separator line
                    let sep_y = inner.min.y + 18.0;
                    painter.line_segment(
                        [
                            egui::Pos2::new(inner.min.x, sep_y),
                            egui::Pos2::new(inner.max.x, sep_y),
                        ],
                        egui::Stroke::new(1.0, color.gamma_multiply(0.4)),
                    );

                    // Description
                    let desc_galley = ui.fonts(|f| {
                        f.layout(
                            desc.to_string(),
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_rgb(0x88, 0x88, 0x88),
                            inner.width(),
                        )
                    });
                    painter.galley(
                        egui::Pos2::new(inner.min.x, sep_y + 4.0),
                        desc_galley,
                        egui::Color32::from_rgb(0x88, 0x88, 0x88),
                    );
                });
        }

        // Handle drop on button release
        if primary_released {
            // Recompute cursor_hex fresh at drop time
            let drop_pos = w.cursor_position();
            let drop_hex = if let Ok((cam, proj)) = cameras.get_single() {
                hex_under_cursor(drop_pos, cam, proj, 32.0, win_size)
            } else {
                info!("Drop: cameras.get_single() FAILED");
                None
            };

            info!(
                "Drop: computed hex={:?} (from pos={:?})",
                drop_hex, drop_pos
            );

            if let Some(state) = &state {
                let cur = state.history.current();
                info!(
                    "Drop: actor positions: {:?}",
                    cur.actors
                        .values()
                        .map(|a| format!("{} @ {:?}", a.name, a.position))
                        .collect::<Vec<_>>()
                );
            }

            let target_id = if is_directed_offensive(m) {
                drop_hex.and_then(|hex| {
                    if let Some(st) = &state {
                        st.history
                            .current()
                            .actors
                            .iter()
                            .find(|(_, a)| {
                                a.position == hex
                                    && a.id != current.map(|c| c.current_actor).unwrap_or(0)
                            })
                            .map(|(&id, _)| id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            if is_directed_offensive(m) && target_id.is_none() {
                info!(
                    "Drop: offensive maneuver {:?} but no valid target at cursor — cancelling",
                    m
                );
                drag_state.dragging = None;
            } else {
                let source_id = current.map(|c| c.current_actor).unwrap_or(0);
                let target_name = target_id.and_then(|tid| {
                    state
                        .as_ref()
                        .and_then(|s| s.history.current().actors.get(&tid).map(|a| a.name.clone()))
                });

                info!(
                    "Drop resolved: source={}, target={:?} ({:?}), maneuver={:?}",
                    source_id, target_id, target_name, m
                );

                maneuver_declared.send(ManeuverDeclaredEvent {
                    source_id,
                    target_id,
                    target_hex: drop_hex,
                    maneuver: m,
                    extra_efforts: vec![],
                });
                drag_state.dragging = None;
                info!("Drag cleared after drop");
            }
        }
    }

    // Clear just-opened flag at end of frame
    ui_state.just_opened_actor = None;
}

fn draw_actor_dropdown(
    ui: &mut egui::Ui,
    actor: &crate::model::Actor,
    _game_state: &crate::model::GameState,
    reload_events: &mut EventWriter<ReloadSheetEvent>,
    remove_events: &mut EventWriter<RemoveActorEvent>,
    gm_events: &mut EventWriter<GmActionEvent>,
    expanded_actor: &mut Option<ActorId>,
) -> egui::Rect {
    let dropdown_frame = egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgba_premultiplied(0x3A, 0x7B, 0xD5, 30));

    let response = dropdown_frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            if let Some(ref path) = actor.source_path {
                if ui.button("Reload Sheet").clicked() {
                    reload_events.send(ReloadSheetEvent {
                        actor_id: actor.id,
                        path: path.clone(),
                    });
                    *expanded_actor = None;
                }
            } else {
                ui.add_enabled(false, egui::Button::new("Reload Sheet"))
                    .on_disabled_hover_text("No source file available");
            }

            if ui.button("Remove Actor").clicked() {
                remove_events.send(RemoveActorEvent { actor_id: actor.id });
                *expanded_actor = None;
            }
        });

        ui.label("Change Posture:");
        ui.horizontal(|ui| {
            let postures = [
                Posture::Standing,
                Posture::Kneeling,
                Posture::Crouching,
                Posture::Sitting,
                Posture::Prone,
                Posture::Crawling,
            ];
            for p in &postures {
                if actor.posture == *p {
                    ui.add_enabled(false, egui::Button::new(format!("{:?}", p)));
                } else if ui.small_button(format!("{:?}", p)).clicked() {
                    gm_events.send(GmActionEvent::SetPosture {
                        actor_id: actor.id,
                        posture: *p,
                    });
                    *expanded_actor = None;
                }
            }
        });
    });

    response.response.rect
}

fn gm_config_panel(
    ui: &mut egui::Ui,
    state: Option<&GameStateResource>,
    settings: Option<&SettingsResource>,
    gm_events: &mut EventWriter<GmActionEvent>,
) {
    let mut new_mod_label = String::new();
    let mut new_mod_value = 0i8;

    ui.label("Shock:");
    if let Some(s) = settings {
        let mut enabled = s.settings.shock_enabled;
        if ui.checkbox(&mut enabled, "enabled").changed() {
            gm_events.send(GmActionEvent::ShockEnabled { enabled });
        }
    } else {
        ui.label("enabled");
    }

    ui.separator();

    if let Some(state) = state {
        let current = state.history.current();
        let can_rewind = state.history.current > 0;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_rewind,
                    egui::Button::new(format!(
                        "Rewind Turn (R{} — {:?})",
                        current.round, current.current_phase
                    )),
                )
                .clicked()
            {
                gm_events.send(GmActionEvent::Rewind);
            }
            if !can_rewind {
                ui.label("(at oldest snapshot)");
            }
        });
    }

    ui.separator();
    ui.label("Global Modifiers:");
    if let Some(state) = state {
        let current = state.history.current();
        if current.global_modifiers.is_empty() {
            ui.label("  (none)");
        } else {
            let mut to_remove: Option<usize> = None;
            for (i, modifier) in current.global_modifiers.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("  {}: {:+}", modifier.label, modifier.value));
                    if ui.small_button("X").clicked() {
                        to_remove = Some(i);
                    }
                });
            }
            if let Some(idx) = to_remove {
                gm_events.send(GmActionEvent::RemoveModifier {
                    index: idx,
                    actor_id: None,
                });
            }
        }

        ui.separator();
        ui.label("Add Modifier:");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut new_mod_label)
                    .hint_text("label")
                    .desired_width(100.0),
            );
            ui.add(
                egui::DragValue::new(&mut new_mod_value)
                    .range(-10..=10)
                    .speed(1),
            );
            if ui.button("Add").clicked() && !new_mod_label.is_empty() {
                gm_events.send(GmActionEvent::AddModifier {
                    label: new_mod_label.clone(),
                    value: new_mod_value,
                    actor_id: None,
                });
            }
        });

        ui.separator();
        ui.label("Presets:");
        ui.horizontal_wrapped(|ui| {
            let presets = [
                ("Dim (-2)", "Lighting: Dim", -2),
                ("Dark (-4)", "Lighting: Dark", -4),
                ("Blind (-10)", "Lighting: Blind", -10),
                ("Bad Foot (-2)", "Terrain: Bad Footing", -2),
                ("V.Bad Foot (-4)", "Terrain: V.Bad", -4),
            ];
            for (label, full_label, val) in &presets {
                if ui.small_button(*label).clicked() {
                    gm_events.send(GmActionEvent::AddModifier {
                        label: full_label.to_string(),
                        value: *val,
                        actor_id: None,
                    });
                }
            }
        });
    } else {
        ui.label("  (none)");
    }

    ui.separator();
    ui.label("Per-Actor Settings:");
    if let Some(state) = state {
        let current = state.history.current();
        let selected_actor = current.actors.get(&current.current_actor);

        if let Some(actor) = selected_actor {
            ui.label(format!("Actor: {}", actor.name));

            let current_pt = actor.pain_threshold;
            egui::ComboBox::from_label("Pain Threshold")
                .selected_text(format!("{:?}", current_pt))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current_pt == PainThreshold::Normal, "Normal")
                        .clicked()
                    {
                        gm_events.send(GmActionEvent::PainThreshold {
                            actor_id: current.current_actor,
                            threshold: PainThreshold::Normal,
                        });
                    }
                    if ui
                        .selectable_label(current_pt == PainThreshold::High, "High")
                        .clicked()
                    {
                        gm_events.send(GmActionEvent::PainThreshold {
                            actor_id: current.current_actor,
                            threshold: PainThreshold::High,
                        });
                    }
                    if ui
                        .selectable_label(current_pt == PainThreshold::Low, "Low")
                        .clicked()
                    {
                        gm_events.send(GmActionEvent::PainThreshold {
                            actor_id: current.current_actor,
                            threshold: PainThreshold::Low,
                        });
                    }
                });

            ui.separator();
            ui.label("Individual Modifiers:");
            if actor.individual_modifiers.is_empty() {
                ui.label("  (none)");
            } else {
                let mut to_remove: Option<usize> = None;
                for (i, modifier) in actor.individual_modifiers.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("  {}: {:+}", modifier.label, modifier.value));
                        if ui.small_button("X").clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
                if let Some(idx) = to_remove {
                    gm_events.send(GmActionEvent::RemoveModifier {
                        index: idx,
                        actor_id: Some(current.current_actor),
                    });
                }
            }
        } else {
            ui.label("No actor selected.");
        }
    } else {
        ui.label("No data loaded.");
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_actor_card(
    ui: &mut egui::Ui,
    actor: &crate::model::Actor,
    is_current: bool,
    index: usize,
    total: usize,
    portrait_cache: &mut std::collections::HashMap<String, egui::TextureHandle>,
    pending_reorder: &mut Option<(usize, usize)>,
    game_state: &crate::model::GameState,
) -> egui::Rect {
    let hp_ratio = actor.hp_current as f32 / actor.hp_max.max(1) as f32;
    let fp_ratio = actor.fp_current as f32 / actor.fp_max.max(1) as f32;

    let frame = if is_current {
        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgba_premultiplied(0x3A, 0x7B, 0xD5, 40))
    } else {
        egui::Frame::group(ui.style())
    };

    let response = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let portrait_size = 32.0;

            if let Some(img_data) = &actor.portrait_data {
                if let Some(tex) = portrait_cache.get(&actor.name) {
                    ui.add(
                        egui::Image::new(tex).fit_to_exact_size(egui::Vec2::splat(portrait_size)),
                    );
                } else if let Ok(dyn_img) = image::load_from_memory(img_data) {
                    let rgba = dyn_img.to_rgba8();
                    let size = [rgba.width() as _, rgba.height() as _];
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    let tex = ui.ctx().load_texture(
                        format!("portrait_{}", actor.name),
                        color_img,
                        egui::TextureOptions::default(),
                    );
                    portrait_cache.insert(actor.name.clone(), tex.clone());
                    ui.add(
                        egui::Image::new(&tex).fit_to_exact_size(egui::Vec2::splat(portrait_size)),
                    );
                } else {
                    draw_portrait_placeholder(ui, actor, portrait_size);
                }
            } else {
                draw_portrait_placeholder(ui, actor, portrait_size);
            }

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&actor.name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if index > 0 {
                            if ui.small_button("Up").clicked() {
                                *pending_reorder = Some((index, index - 1));
                            }
                        } else {
                            ui.add_enabled(false, egui::Button::new("Up"));
                        }
                        if index + 1 < total {
                            if ui.small_button("Dn").clicked() {
                                *pending_reorder = Some((index, index + 1));
                            }
                        } else {
                            ui.add_enabled(false, egui::Button::new("Dn"));
                        }
                    });
                });
                ui.label(format!(
                    "{}  HP {}/{}",
                    if actor.is_npc { "NPC" } else { "PC" },
                    actor.hp_current,
                    actor.hp_max
                ));
            });
        });

        draw_stat_bar(ui, "HP", hp_ratio, actor.hp_current, actor.hp_max);
        draw_stat_bar(ui, "FP", fp_ratio, actor.fp_current, actor.fp_max);

        ui.label(format!(
            "ST {}  DX {}  IQ {}  HT {}",
            actor.st, actor.dx, actor.iq, actor.ht
        ));
        ui.label(format!(
            "Spd {:.2}  Move {}   SM {}",
            actor.basic_speed,
            actor.effective_move(),
            actor.sm
        ));
        ui.label(format!(
            "Dodge {}  Parry {}",
            actor.dodge(),
            actor
                .attacks
                .first()
                .and_then(|a| a.parry_bonus)
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into())
        ));
        ui.label(format!("Pos: ({}, {})", actor.position.0, actor.position.1));
        ui.label(format!(
            "Enc: {:?}  Posture: {:?}",
            actor.encumbrance, actor.posture
        ));
        if actor.turns_per_round > 1 {
            ui.colored_label(
                egui::Color32::from_rgb(0x3A, 0x7B, 0xD5),
                format!("Turns: {}/round", actor.turns_per_round),
            );
        }
        if is_current && game_state.attacks_remaining > 1 {
            ui.colored_label(
                egui::Color32::from_rgb(0xCC, 0xAA, 0x00),
                format!("Attacks remaining: {}", game_state.attacks_remaining),
            );
        }
        if actor.attacks_per_turn > 1 {
            ui.colored_label(
                egui::Color32::from_rgb(0x88, 0x88, 0x88),
                format!("{} attacks/turn", actor.attacks_per_turn),
            );
        }
        if actor.enhanced_time_sense {
            ui.colored_label(
                egui::Color32::from_rgb(0xAA, 0x88, 0xFF),
                "Enhanced Time Sense",
            );
        }

        let mut flags = Vec::new();
        if actor.flags.stunned {
            flags.push("Stunned");
        }
        if actor.flags.knocked_down {
            flags.push("Knocked Down");
        }
        if actor.flags.unconscious {
            flags.push("Unconscious");
        }
        if actor.flags.dead {
            flags.push("Dead");
        }
        if actor.one_leg_crippled() {
            flags.push("Crippled Leg");
        }
        if actor.both_legs_crippled() {
            flags.push("Both Legs Crippled");
        }
        if !flags.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0xCC, 0x33, 0x33), flags.join(" | "));
        }
    });

    response.response.rect
}

fn draw_portrait_placeholder(ui: &mut egui::Ui, actor: &crate::model::Actor, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.circle_filled(
        rect.center(),
        size / 2.0,
        egui::Color32::from_rgb(0x3A, 0x7B, 0xD5),
    );
    let initials = actor
        .name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>();
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials,
        egui::FontId::proportional(12.0),
        egui::Color32::WHITE,
    );
}

fn draw_stat_bar(ui: &mut egui::Ui, label: &str, ratio: f32, current: i16, max: i16) {
    let ratio = ratio.clamp(-1.0, 1.0);
    let color = if ratio <= 0.0 {
        egui::Color32::from_rgb(0x88, 0x00, 0x00)
    } else if ratio <= 1.0 / 3.0 {
        egui::Color32::from_rgb(0xCC, 0xAA, 0x00)
    } else {
        egui::Color32::from_rgb(0x44, 0xAA, 0x44)
    };
    ui.horizontal(|ui| {
        ui.label(label.to_string());
        let bar = egui::widgets::ProgressBar::new(ratio.max(0.0))
            .desired_width(ui.available_width() - 60.0)
            .fill(color)
            .text(format!("{}/{}", current, max));
        ui.add(bar);
    });
}

fn maneuver_category_color_egui(maneuver: ManeuverType) -> egui::Color32 {
    use ManeuverType as Mt;
    match maneuver {
        Mt::Attack
        | Mt::Feint
        | Mt::FeignBeat
        | Mt::FeignDefensive
        | Mt::FeignRuse
        | Mt::Evaluate => egui::Color32::from_rgb(0xCC, 0x33, 0x33),
        Mt::AllOutAttackDetermined
        | Mt::AllOutAttackDouble
        | Mt::AllOutAttackFeint
        | Mt::AllOutAttackLong
        | Mt::AllOutAttackStrong
        | Mt::AllOutAttackRangedDetermined
        | Mt::CommittedAttackDetermined
        | Mt::CommittedAttackStrong => egui::Color32::from_rgb(0xFF, 0x66, 0x00),
        Mt::Aim | Mt::Move | Mt::MoveAndAttack | Mt::ChangePosture => {
            egui::Color32::from_rgb(0xCC, 0xAA, 0x00)
        }
        Mt::AllOutDefenseIncreased
        | Mt::AllOutDefenseDouble
        | Mt::AllOutDefenseMental
        | Mt::DefensiveAttack
        | Mt::Ready => egui::Color32::from_rgb(0x33, 0x66, 0x99),
        Mt::Concentrate | Mt::AllOutConcentrate | Mt::DoNothing | Mt::Wait => {
            egui::Color32::from_rgb(0x77, 0x55, 0xAA)
        }
    }
}

fn draw_maneuver_card(
    ui: &mut egui::Ui,
    name: &str,
    description: &str,
    color: egui::Color32,
    selected: bool,
) -> egui::Response {
    let card_w = 125.0;
    let card_h = 150.0;
    let desired_size = egui::Vec2::new(card_w, card_h);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

    let bg = egui::Color32::from_rgb(0x16, 0x16, 0x16);
    let border_c = if selected {
        egui::Color32::from_rgb(0x3A, 0x7B, 0xD5)
    } else if response.hovered() {
        color
    } else {
        color.gamma_multiply(0.6)
    };
    let dragged = response.dragged();

    let painter = ui.painter_at(rect);

    // Card background
    if dragged {
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_premultiplied(0x16, 0x16, 0x16, 150),
        );
    } else {
        painter.rect_filled(rect, 0.0, bg);
    }

    // Solid colored border (5px)
    let border_stroke = egui::Stroke::new(5.0, border_c);
    painter.rect_stroke(rect, 0.0, border_stroke);

    // Inner margin for text
    let margin = 8.0;
    let inner = rect.shrink(margin);

    // Name header at top
    let name_pos = inner.min + egui::Vec2::new(0.0, 2.0);
    painter.text(
        name_pos,
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(13.0),
        egui::Color32::from_rgb(0xE0, 0xE0, 0xE0),
    );

    // Separator line below name
    let sep_y = name_pos.y + 18.0;
    painter.line_segment(
        [
            egui::Pos2::new(inner.min.x, sep_y),
            egui::Pos2::new(inner.max.x, sep_y),
        ],
        egui::Stroke::new(1.0, color.gamma_multiply(0.4)),
    );

    // Description text below separator
    let desc_pos = egui::Pos2::new(inner.min.x, sep_y + 4.0);
    let desc_galley = ui.fonts(|f| {
        f.layout(
            description.to_string(),
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(0x88, 0x88, 0x88),
            inner.width(),
        )
    });
    painter.galley(
        desc_pos,
        desc_galley,
        egui::Color32::from_rgb(0x88, 0x88, 0x88),
    );

    response
}

fn maneuver_description(maneuver: ManeuverType) -> &'static str {
    match maneuver {
        ManeuverType::Attack => "Strike with a ready weapon. Normal active defenses apply.",
        ManeuverType::AllOutAttackDetermined => "A single attack at +4 to hit. You may make no active defenses this turn.",
        ManeuverType::AllOutAttackDouble => "Make two attacks against the same target. No active defenses.",
        ManeuverType::AllOutAttackFeint => "Feint then immediately Attack the same target. No active defenses.",
        ManeuverType::AllOutAttackLong => "Attack at Reach +1. No active defenses.",
        ManeuverType::AllOutAttackStrong => "+2 damage, or +1 per die of damage. No active defenses.",
        ManeuverType::AllOutAttackRangedDetermined => "+1 to hit with a ranged weapon. No active defenses.",
        ManeuverType::CommittedAttackDetermined => "+2 to hit. All defense rolls this turn at -2.",
        ManeuverType::CommittedAttackStrong => "+1 damage or +1 per 2d. All defense rolls this turn at -2.",
        ManeuverType::DefensiveAttack => "+1 to one active defense. Damage is reduced by -2 or -1 per die.",
        ManeuverType::MoveAndAttack => "Move then attack. Melee capped at 9 effective skill, ranged at Bulk penalty.",
        ManeuverType::Feint => "Roll a Quick Contest of weapon skills. Margin of victory penalizes enemy defense next turn.",
        ManeuverType::FeignBeat => "Feign a strike against the foe's weapon. Margin penalizes their Parry vs your next attack.",
        ManeuverType::FeignDefensive => "Feign a weak defense to draw an attack from your opponent.",
        ManeuverType::FeignRuse => "Deceive opponent with footwork or body language. Quick Contest vs IQ-based skill.",
        ManeuverType::Evaluate => "Study a single opponent. +1 to attack, defense, or feint per turn (max +3).",
        ManeuverType::Aim => "Sight a ranged weapon. +Accuracy after 1 turn, additional +1 after 2, +2 after 3+.",
        ManeuverType::Wait => "Hold your action. Trigger on a specified event before your next turn.",
        ManeuverType::AllOutDefenseIncreased => "+2 to one active defense of your choice. May move up to half Move.",
        ManeuverType::AllOutDefenseDouble => "May attempt two different active defenses against a single attack.",
        ManeuverType::AllOutDefenseMental => "+2 to all Will, resistance, and self-control rolls.",
        ManeuverType::DoNothing => "Stand still. May parry, block, or dodge normally.",
        ManeuverType::Concentrate => "Focus on a mental task, spell, or Sense roll. May move up to Step.",
        ManeuverType::AllOutConcentrate => "Concentrate with total focus. You may make no active defense rolls.",
        ManeuverType::Ready => "Draw a weapon, ready an item, or reload a ranged weapon.",
        ManeuverType::ChangePosture => "Change to any adjacent posture (e.g. Standing→Kneeling, Prone→Crawling).",
        ManeuverType::Move => "Move up to your full Basic Move in yards. May sprint for +20%.",
    }
}

fn maneuver_display_name(maneuver: ManeuverType) -> String {
    use ManeuverType as Mt;
    match maneuver {
        Mt::AllOutDefenseIncreased => "AOD Increased".into(),
        Mt::AllOutDefenseDouble => "AOD Double".into(),
        Mt::AllOutDefenseMental => "AOD Mental".into(),
        Mt::DoNothing => "Do Nothing".into(),
        Mt::Concentrate => "Concentrate".into(),
        Mt::AllOutConcentrate => "AOC".into(),
        Mt::Ready => "Ready".into(),
        Mt::ChangePosture => "Change Posture".into(),
        Mt::Move => "Move".into(),
        Mt::Attack => "Attack".into(),
        Mt::AllOutAttackDetermined => "AOA Determined".into(),
        Mt::AllOutAttackDouble => "AOA Double".into(),
        Mt::AllOutAttackFeint => "AOA Feint".into(),
        Mt::AllOutAttackLong => "AOA Long".into(),
        Mt::AllOutAttackStrong => "AOA Strong".into(),
        Mt::AllOutAttackRangedDetermined => "AOA Ranged Det.".into(),
        Mt::CommittedAttackDetermined => "Committed Determined".into(),
        Mt::CommittedAttackStrong => "Committed Strong".into(),
        Mt::DefensiveAttack => "Defensive Attack".into(),
        Mt::MoveAndAttack => "Move and Attack".into(),
        Mt::Feint => "Feint".into(),
        Mt::FeignBeat => "Feign Beat".into(),
        Mt::FeignDefensive => "Feign Defensive".into(),
        Mt::FeignRuse => "Feign Ruse".into(),
        Mt::Evaluate => "Evaluate".into(),
        Mt::Aim => "Aim".into(),
        Mt::Wait => "Wait".into(),
    }
}
