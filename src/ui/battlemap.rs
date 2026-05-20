use bevy::prelude::*;
use bevy::sprite::{AlphaMode2d, ColorMaterial, MeshMaterial2d};
use crate::model::{sort_turn_order, ActorId, EventLog, GameState, GameStateHistory, ManeuverType, PainThreshold, Posture, Srgba, TurnPhase};
use crate::model::gcs_import::import_gcs_json;
use crate::logging::LogEvent;
use crate::settings::{save_settings, srgb_to_bevy_color};

pub struct BattlemapPlugin;

impl Plugin for BattlemapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattlemapConfig>()
            .init_resource::<TokenDragState>()
            .init_resource::<ManeuverDragState>()
            .add_event::<ImportSheetEvent>()
            .add_event::<ReloadSheetEvent>()
            .add_event::<RemoveActorEvent>()
            .add_event::<GmActionEvent>()
            .add_event::<TokenRightClickedEvent>()
            .add_systems(Startup, (setup_battlemap, import_example_sheet).chain())
            .add_systems(PreUpdate, (
                handle_import_sheet,
                handle_reload_sheet,
                handle_remove_actor,
                handle_gm_actions,
            ))
            .add_systems(Update, (
                detect_token_right_click,
                sync_tokens,
                draw_grid,
                draw_hex_outlines,
                draw_relation_arrows,
                update_hp_bars,
                camera_controls,
            ))
            .add_systems(First, detect_drag_input);
    }
}

#[derive(Event, Debug)]
pub struct ImportSheetEvent {
    pub path: String,
}

#[derive(Event, Debug)]
pub struct ReloadSheetEvent {
    pub actor_id: ActorId,
    pub path: String,
}

#[derive(Event, Debug)]
pub struct RemoveActorEvent {
    pub actor_id: ActorId,
}

#[derive(Event, Debug, Clone)]
pub enum GmActionEvent {
    AddModifier { label: String, value: i8, actor_id: Option<ActorId> },
    RemoveModifier { index: usize, actor_id: Option<ActorId> },
    Rewind,
    ShockEnabled { enabled: bool },
    PainThreshold { actor_id: ActorId, threshold: PainThreshold },
    MoveActor { actor_id: ActorId, position: (i32, i32) },
    SetPosture { actor_id: ActorId, posture: Posture },
    ReorderTurnOrder { from_index: usize, to_index: usize },
}

#[derive(Event, Debug)]
pub struct TokenRightClickedEvent {
    pub actor_id: ActorId,
}

fn handle_import_sheet(
    mut events: EventReader<ImportSheetEvent>,
    mut state: Option<ResMut<GameStateResource>>,
    mut portraits: Option<ResMut<PortraitCacheRes>>,
    mut log_events: EventWriter<LogEvent>,
    mut assets_images: ResMut<Assets<Image>>,
) {
    for ev in events.read() {
        let json_str = match std::fs::read_to_string(&ev.path) {
            Ok(s) => s,
            Err(e) => {
                log_events.send(LogEvent::error(format!("Failed to read {}: {e}", ev.path)));
                continue;
            }
        };

        let mut actor = match import_gcs_json(&json_str) {
            Ok(a) => a,
            Err(e) => {
                log_events.send(LogEvent::error(format!("Failed to parse GCS sheet: {e}")));
                continue;
            }
        };

        let Some(ref mut state_res) = state else {
            log_events.send(LogEvent::error("No GameState loaded; cannot import sheet."));
            continue;
        };

        let history = &mut state_res.history;

        let new_id: ActorId = history.current().actors.keys().max().map(|id| id + 1).unwrap_or(1);
        actor.id = new_id;
        actor.source_path = Some(ev.path.clone());
        actor.position = (5, new_id as i32 % 20);

        let actor_name = actor.name.clone();
        let hp_info = format!("HP {}/{}, ST {}, DX {}, IQ {}, HT {}",
            actor.hp_current, actor.hp_max, actor.st, actor.dx, actor.iq, actor.ht);

        if let Some(img_data) = &actor.portrait_data {
            if let Ok(dyn_img) = image::load_from_memory(img_data) {
                let rgba = dyn_img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let bevy_img = Image::new_fill(
                    bevy::render::render_resource::Extent3d {
                        width: w, height: h, depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    &rgba,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                let handle = assets_images.add(bevy_img);
                if let Some(ref mut p) = portraits {
                    p.images.insert(new_id, handle);
                }
            }
        }

        let mut new_state = history.current().clone();
        new_state.actors.insert(new_id, actor);
        if !new_state.turn_order.contains(&new_id) {
            new_state.turn_order.push(new_id);
        }
        sort_turn_order(&new_state.actors, &mut new_state.turn_order);

        history.push(new_state.clone());

        log_events.send(LogEvent::info(format!("Imported sheet: {actor_name} — {hp_info}"))
            .with_context(new_state.round, new_id));
    }
}

fn handle_reload_sheet(
    mut events: EventReader<ReloadSheetEvent>,
    mut state: Option<ResMut<GameStateResource>>,
    mut portraits: Option<ResMut<PortraitCacheRes>>,
    mut log_events: EventWriter<LogEvent>,
    mut assets_images: ResMut<Assets<Image>>,
) {
    for ev in events.read() {
        let Some(ref mut state_res) = state else {
            log_events.send(LogEvent::error("No GameState loaded; cannot reload sheet."));
            continue;
        };

        let json_str = match std::fs::read_to_string(&ev.path) {
            Ok(s) => s,
            Err(e) => {
                log_events.send(LogEvent::error(format!("Failed to read {}: {e}", ev.path)));
                continue;
            }
        };

        let fresh_actor = match import_gcs_json(&json_str) {
            Ok(a) => a,
            Err(e) => {
                log_events.send(LogEvent::error(format!("Failed to parse GCS sheet: {e}")));
                continue;
            }
        };

        let history = &mut state_res.history;
        let current = history.current();

        let Some(existing) = current.actors.get(&ev.actor_id) else {
            log_events.send(LogEvent::warn(format!("Actor #{} not found for reload", ev.actor_id)));
            continue;
        };

        let hp_current = existing.hp_current;
        let fp_current = existing.fp_current;
        let position = existing.position;
        let flags = existing.flags.clone();
        let leg_state = existing.leg_state.clone();
        let posture = existing.posture;
        let encumbrance = existing.encumbrance;
        let individual_modifiers = existing.individual_modifiers.clone();
        let pain_threshold = existing.pain_threshold;
        let turns_per_round = existing.turns_per_round;
        let attacks_per_turn = existing.attacks_per_turn;
        let current_maneuver = existing.current_maneuver;
        let active_attack = existing.active_attack;
        let extra_effort = existing.extra_effort.clone();

        let mut updated = fresh_actor;
        updated.id = ev.actor_id;
        updated.source_path = Some(ev.path.clone());
        updated.hp_current = hp_current;
        updated.fp_current = fp_current;
        updated.position = position;
        updated.flags = flags;
        updated.leg_state = leg_state;
        updated.posture = posture;
        updated.encumbrance = encumbrance;
        updated.individual_modifiers = individual_modifiers;
        updated.pain_threshold = pain_threshold;
        updated.turns_per_round = turns_per_round;
        updated.attacks_per_turn = attacks_per_turn;
        updated.current_maneuver = current_maneuver;
        updated.active_attack = active_attack;
        updated.extra_effort = extra_effort;

        if let Some(img_data) = &updated.portrait_data {
            if let Ok(dyn_img) = image::load_from_memory(img_data) {
                let rgba = dyn_img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let bevy_img = Image::new_fill(
                    bevy::render::render_resource::Extent3d {
                        width: w, height: h, depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    &rgba,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                let handle = assets_images.add(bevy_img);
                if let Some(ref mut p) = portraits {
                    p.images.insert(ev.actor_id, handle);
                }
            }
        }

        let mut new_state = current.clone();
        new_state.actors.insert(ev.actor_id, updated);

        history.push(new_state);

        log_events.send(LogEvent::info(format!("Reloaded sheet for actor #{}", ev.actor_id)));
    }
}

fn handle_remove_actor(
    mut events: EventReader<RemoveActorEvent>,
    mut state: Option<ResMut<GameStateResource>>,
    mut log_events: EventWriter<LogEvent>,
) {
    for ev in events.read() {
        let Some(ref mut state_res) = state else {
            log_events.send(LogEvent::error("No GameState loaded; cannot remove actor."));
            continue;
        };

        let history = &mut state_res.history;
        let current = history.current();

        let Some(actor) = current.actors.get(&ev.actor_id) else {
            log_events.send(LogEvent::warn(format!("Actor #{} not found for removal", ev.actor_id)));
            continue;
        };

        let name = actor.name.clone();

        let mut new_state = current.clone();
        new_state.actors.remove(&ev.actor_id);
        new_state.turn_order.retain(|id| *id != ev.actor_id);

        if new_state.current_actor == ev.actor_id {
            if let Some(&next_id) = new_state.turn_order.first() {
                new_state.current_actor = next_id;
            } else {
                new_state.current_actor = 0;
            }
        }

        history.push(new_state);

        log_events.send(LogEvent::info(format!("Removed actor: {name}")));
    }
}

fn handle_gm_actions(
    mut events: EventReader<GmActionEvent>,
    mut state: Option<ResMut<GameStateResource>>,
    mut settings: Option<ResMut<crate::settings::SettingsResource>>,
    mut log_events: EventWriter<LogEvent>,
) {
    for ev in events.read() {
        match ev {
            GmActionEvent::AddModifier { label, value, actor_id } => {
                let Some(ref mut state_res) = state else { continue };
                let modifier = crate::model::Modifier {
                    label: label.clone(),
                    value: *value,
                    applies_to: crate::model::ModifierTarget::AllRolls,
                };
                let mut new_state = state_res.history.current().clone();
                if let Some(aid) = actor_id {
                    if let Some(actor) = new_state.actors.get_mut(aid) {
                        actor.individual_modifiers.push(modifier.clone());
                    }
                } else {
                    new_state.global_modifiers.push(modifier.clone());
                }
                state_res.history.push(new_state);
            }
            GmActionEvent::RemoveModifier { index, actor_id } => {
                let Some(ref mut state_res) = state else { continue };
                let mut new_state = state_res.history.current().clone();
                if let Some(aid) = actor_id {
                    if let Some(actor) = new_state.actors.get_mut(aid) {
                        if *index < actor.individual_modifiers.len() {
                            actor.individual_modifiers.remove(*index);
                        }
                    }
                } else {
                    if *index < new_state.global_modifiers.len() {
                        new_state.global_modifiers.remove(*index);
                    }
                }
                state_res.history.push(new_state);
            }
            GmActionEvent::Rewind => {
                let Some(ref mut state_res) = state else { continue };
                let prev_snapshot = state_res.history.current;
                let round_before = state_res.history.current().round;
                if state_res.history.rewind().is_some() {
                    let cur = state_res.history.current();
                    if cur.current_phase == TurnPhase::Complete {
                        let mut reset = cur.clone();
                        reset.current_phase = TurnPhase::ManeuverSelection;
                        if let Some(actor) = reset.actors.get_mut(&cur.current_actor) {
                            actor.current_maneuver = None;
                        }
                        state_res.history.push(reset);
                    }
                    log_events.send(LogEvent::info(format!(
                        "Rewind: snapshot {} → {} (R{} → R{})",
                        prev_snapshot, state_res.history.current, round_before, state_res.history.current().round,
                    )));
                }
            }
            GmActionEvent::ShockEnabled { enabled } => {
                if let Some(ref mut s) = settings {
                    s.settings.shock_enabled = *enabled;
                    save_settings(&s.settings);
                }
            }
            GmActionEvent::PainThreshold { actor_id, threshold } => {
                let Some(ref mut state_res) = state else { continue };
                let mut new_state = state_res.history.current().clone();
                if let Some(actor) = new_state.actors.get_mut(actor_id) {
                    actor.pain_threshold = *threshold;
                }
                state_res.history.push(new_state);
            }
            GmActionEvent::MoveActor { actor_id, position } => {
                let Some(ref mut state_res) = state else { continue };
                let mut new_state = state_res.history.current().clone();
                if let Some(actor) = new_state.actors.get_mut(actor_id) {
                    actor.position = *position;
                }
                state_res.history.push(new_state);
            }
            GmActionEvent::SetPosture { actor_id, posture } => {
                let Some(ref mut state_res) = state else { continue };
                let mut new_state = state_res.history.current().clone();
                if let Some(actor) = new_state.actors.get_mut(actor_id) {
                    actor.posture = *posture;
                }
                state_res.history.push(new_state);
            }
            GmActionEvent::ReorderTurnOrder { from_index, to_index } => {
                let Some(ref mut state_res) = state else { continue };
                let mut new_state = state_res.history.current().clone();
                if *from_index < new_state.turn_order.len() && *to_index <= new_state.turn_order.len() {
                    let actor_id = new_state.turn_order.remove(*from_index);
                    let insert_at = if *to_index > *from_index { to_index - 1 } else { *to_index };
                    new_state.turn_order.insert(insert_at, actor_id);
                }
                state_res.history.push(new_state);
            }
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct BattlemapConfig {
    pub hex_size: f32,
    pub grid_extent: u32,
    pub grid_color: Srgba,
    pub background_color: Srgba,
}

impl Default for BattlemapConfig {
    fn default() -> Self {
        Self {
            hex_size: 32.0,
            grid_extent: 20,
            grid_color: Srgba { r: 0.15, g: 0.15, b: 0.15, a: 0.6 },
            background_color: Srgba { r: 0.05, g: 0.05, b: 0.05, a: 1.0 },
        }
    }
}

pub fn token_hex_radius(config: &BattlemapConfig) -> f32 {
    config.hex_size * 0.75
}

pub fn world_position(q: i32, r: i32, size: f32) -> Vec2 {
    let x = size * (3.0 / 2.0 * q as f32);
    let y = size * (3f32.sqrt() / 2.0 * q as f32 + 3f32.sqrt() * r as f32);
    Vec2::new(x, y)
}

fn hex_vertices(center: Vec2, size: f32) -> [Vec2; 6] {
    [0f32, 60f32, 120f32, 180f32, 240f32, 300f32].map(|deg| {
        let rad = deg.to_radians();
        center + Vec2::new(size * rad.cos(), size * rad.sin())
    })
}

#[derive(Resource)]
pub struct GameStateResource {
    pub history: GameStateHistory,
}

#[derive(Resource)]
pub struct EventLogResource {
    pub log: EventLog,
}

#[derive(Resource, Default)]
pub struct PortraitCacheRes {
    pub images: std::collections::HashMap<ActorId, Handle<Image>>,
}

#[derive(Resource, Default)]
pub struct HexMeshCacheRes {
    pub meshes: std::collections::HashMap<f32, Handle<Mesh>>,
}

#[derive(Component)]
pub struct TokenMarker {
    pub actor_id: ActorId,
}

#[derive(Component)]
pub struct HpBarMarker;

#[derive(Resource, Default)]
pub struct TokenDragState {
    pub dragging: Option<ActorId>,
}

#[derive(Resource, Default)]
pub struct ManeuverDragState {
    pub dragging: Option<ManeuverType>,
}

fn import_example_sheet(world: &mut World) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Francesca Vanorder.gcs");
    let json_str = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to read GCS file: {e}");
            return;
        }
    };

    let mut actor = match import_gcs_json(&json_str) {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to import GCS sheet: {e}");
            return;
        }
    };

    actor.id = 1;
    actor.source_path = Some(path.to_string());
    actor.position = (5, 5);

    let mut portrait_images = std::collections::HashMap::new();
    if let Some(img_data) = &actor.portrait_data {
        if let Ok(dyn_img) = image::load_from_memory(img_data) {
            let rgba = dyn_img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let bevy_img = Image::new_fill(
                bevy::render::render_resource::Extent3d {
                    width: w, height: h, depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                &rgba,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            );
            let handle = world.resource_mut::<bevy::asset::Assets<Image>>().add(bevy_img);
            portrait_images.insert(actor.id, handle);
        }
    }
    world.insert_resource(PortraitCacheRes { images: portrait_images });

    let mut actors = std::collections::HashMap::new();
    actors.insert(actor.id, actor.clone());

    let state = GameState {
        actors,
        relations: vec![],
        turn_order: vec![1],
        current_actor: 1,
        current_phase: crate::model::TurnPhase::ManeuverSelection,
        global_modifiers: vec![],
        round: 1,
        attacks_remaining: 1,
    };

    let history = GameStateHistory::new(state);
    world.insert_resource(GameStateResource { history });

    let mut log = EventLog::new();
    log.push(crate::model::LogEntry {
        round: 1,
        turn: actor.id,
        phase: TurnPhase::ManeuverSelection,
        message: format!("{} joined combat — Maneuver Selection", actor.name),
        kind: crate::model::LogEntryKind::PhaseTransition,
    });
    world.insert_resource(EventLogResource { log });

    info!("Imported GCS sheet: {} (HP {}/{}, ST {}, DX {}, IQ {}, HT {})",
        actor.name, actor.hp_current, actor.hp_max, actor.st, actor.dx, actor.iq, actor.ht);
}

fn setup_battlemap(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.insert_resource(ClearColor(Color::srgba(0.05, 0.05, 0.05, 1.0)));
}

fn hex_mesh(radius: f32) -> Mesh {
    Mesh::from(RegularPolygon::new(radius, 6))
}

fn detect_drag_input(
    mouse: Res<ButtonInput<MouseButton>>,
    mut drag_state: ResMut<TokenDragState>,
    maneuver_drag: Res<ManeuverDragState>,
    state: Option<Res<GameStateResource>>,
    window: Query<&Window>,
    cameras: Query<(&Transform, &OrthographicProjection), With<Camera>>,
    mut move_events: EventWriter<GmActionEvent>,
) {
    if maneuver_drag.dragging.is_some() {
        return;
    }
    let left_pressed = mouse.just_pressed(MouseButton::Left);
    let left_released = mouse.just_released(MouseButton::Left);

    if !left_pressed && !left_released && drag_state.dragging.is_none() {
        return;
    }

    let Some(state) = state else { return };
    let Ok(window) = window.get_single() else { return };
    let cursor = window.cursor_position();
    let win_size = Vec2::new(window.width() as f32, window.height() as f32);
    let Ok((cam, proj)) = cameras.get_single() else { return };
    let cursor_hex = crate::ui::panels::hex_under_cursor(cursor, cam, proj, 32.0, win_size);

    let current = state.history.current();
    let mut closest_actor: Option<(ActorId, f32)> = None;

    if let Some(pos) = cursor {
        let world = cam.translation.truncate();
        let p_world = Vec2::new(
            (pos.x - win_size.x / 2.0) * proj.scale + world.x,
            (win_size.y / 2.0 - pos.y) * proj.scale + world.y,
        );
        for (&actor_id, actor) in &current.actors {
            let actor_world = world_position(actor.position.0, actor.position.1, 32.0);
            let dist = p_world.distance(actor_world);
            if dist < 32.0 {
                if closest_actor.map_or(true, |(_, d)| dist < d) {
                    closest_actor = Some((actor_id, dist));
                }
            }
        }
    }

    if left_pressed {
        info!("Token drag: left_pressed, cursor={:?}, cursor_hex={:?}, closest={:?}",
            cursor, cursor_hex, closest_actor);
        if let Some((actor_id, _dist)) = closest_actor {
            drag_state.dragging = Some(actor_id);
            info!("Token drag: started dragging actor {}", actor_id);
        }
    }

    if let Some(actor_id) = drag_state.dragging {
        if left_released {
            info!("Token drag: left_released, actor={}, cursor_hex={:?}", actor_id, cursor_hex);
            if let Some(hex) = cursor_hex {
                move_events.send(GmActionEvent::MoveActor { actor_id, position: hex });
                info!("Token drag: moving actor {} to {:?}", actor_id, hex);
            }
            drag_state.dragging = None;
        }
    }
}

fn detect_token_right_click(
    mouse: Res<ButtonInput<MouseButton>>,
    state: Option<Res<GameStateResource>>,
    cameras: Query<(&Transform, &OrthographicProjection), With<Camera>>,
    window: Query<&Window>,
    mut events: EventWriter<TokenRightClickedEvent>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Some(state) = state else { return };
    let Ok(window) = window.get_single() else { return };
    let cursor = window.cursor_position();
    let win_size = Vec2::new(window.width() as f32, window.height() as f32);
    let Ok((cam, proj)) = cameras.get_single() else { return };

    let Some(hex) = crate::ui::panels::hex_under_cursor(cursor, cam, proj, 32.0, win_size) else { return };

    let current = state.history.current();
    for (&actor_id, actor) in &current.actors {
        if actor.position == hex {
            events.send(TokenRightClickedEvent { actor_id });
            break;
        }
    }
}

fn sync_tokens(
    state: Option<Res<GameStateResource>>,
    portraits: Option<Res<PortraitCacheRes>>,
    config: Res<BattlemapConfig>,
    drag_state: Res<TokenDragState>,
    cameras: Query<(&Transform, &OrthographicProjection), With<Camera>>,
    window: Query<&Window>,
    mut meshes: Local<Option<std::collections::HashMap<u32, Handle<Mesh>>>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut assets_mesh: ResMut<Assets<Mesh>>,
    mut spawned: Local<Option<std::collections::HashMap<ActorId, Entity>>>,
    mut commands: Commands,
    mut existing: Query<(Entity, &TokenMarker, &mut Transform), (Without<Camera>, Without<HpBarMarker>)>,
) {
    let Some(state) = state else { return };
    let current = state.history.current();

    // --- Drag visual update ---
    if let Some(actor_id) = drag_state.dragging {
        let Ok(window_res) = window.get_single() else { return };
        let cursor = window_res.cursor_position();
        let win_size = Vec2::new(window_res.width() as f32, window_res.height() as f32);
        if let Ok((cam, proj)) = cameras.get_single() {
            if let Some(hex) = crate::ui::panels::hex_under_cursor(cursor, cam, proj, 32.0, win_size) {
                let new_world = world_position(hex.0, hex.1, 32.0);
                for (_, marker, mut transform) in &mut existing {
                    if marker.actor_id == actor_id {
                        transform.translation = new_world.extend(transform.translation.z);
                        break;
                    }
                }
            }
        }
        // Don't return - still do sync_tokens for other tokens
    }

    let hex_r = token_hex_radius(&config);
    let bar_y = -hex_r * 3f32.sqrt() / 2.0 - 3.0;
    let bar_w = hex_r;
    let bar_h = 4.0;

    if meshes.is_none() {
        *meshes = Some(std::collections::HashMap::new());
    }
    let _mesh_map = meshes.as_mut().unwrap();

    if spawned.is_none() {
        *spawned = Some(std::collections::HashMap::new());
    }
    let map = spawned.as_mut().unwrap();

    let current_ids: std::collections::HashSet<ActorId> = current.actors.keys().copied().collect();

    map.retain(|id, entity| {
        if !current_ids.contains(id) {
            commands.entity(*entity).despawn_recursive();
            false
        } else {
            true
        }
    });

    for (id, actor) in &current.actors {
        if !map.contains_key(id) {
            let pos = world_position(actor.position.0, actor.position.1, 32.0);

            let hex_handle = {
                let mesh_map = meshes.as_mut().unwrap();
                mesh_map.entry(hex_r.to_bits()).or_insert_with(|| {
                    assets_mesh.add(hex_mesh(hex_r))
                }).clone()
            };

            let portrait_mat = if let Some(p) = &portraits {
                if let Some(img_handle) = p.images.get(id) {
                    materials.add(ColorMaterial {
                        color: Color::WHITE,
                        texture: Some(img_handle.clone()),
                        alpha_mode: AlphaMode2d::Blend,
                    })
                } else {
                    materials.add(ColorMaterial {
                        color: Color::srgba(0.227, 0.482, 0.835, 1.0),
                        texture: None,
                        alpha_mode: AlphaMode2d::Blend,
                    })
                }
            } else {
                materials.add(ColorMaterial {
                    color: Color::srgba(0.227, 0.482, 0.835, 1.0),
                    texture: None,
                    alpha_mode: AlphaMode2d::Blend,
                })
            };

            let parent_id = commands.spawn((
                TokenMarker { actor_id: *id },
                Transform::from_translation(pos.extend(0.1)),
                Visibility::Visible,
            )).id();

            let portrait_id = commands.spawn((
                Mesh2d(hex_handle.clone()),
                MeshMaterial2d(portrait_mat),
                Transform::from_rotation(Quat::from_rotation_z(30f32.to_radians())),
            )).id();
            commands.entity(parent_id).add_child(portrait_id);

            let hp_border_id = commands.spawn((
                Sprite {
                    color: Color::BLACK,
                    custom_size: Some(Vec2::new(bar_w + 4.0, bar_h + 2.0)),
                    ..default()
                },
                Transform::from_xyz(0.0, bar_y, 0.15),
            )).id();
            commands.entity(parent_id).add_child(hp_border_id);

            let hp_bg_id = commands.spawn((
                Sprite {
                    color: Color::srgba(0.8, 0.2, 0.2, 1.0),
                    custom_size: Some(Vec2::new(bar_w, bar_h)),
                    ..default()
                },
                Transform::from_xyz(0.0, bar_y, 0.18),
            )).id();
            commands.entity(parent_id).add_child(hp_bg_id);

            let hp_id = commands.spawn((
                HpBarMarker,
                Sprite {
                    color: Color::srgba(0.267, 0.667, 0.267, 0.9),
                    custom_size: Some(Vec2::new(bar_w, bar_h)),
                    ..default()
                },
                Transform::from_xyz(0.0, bar_y, 0.2),
            )).id();
            commands.entity(parent_id).add_child(hp_id);

            map.insert(*id, parent_id);
        }
    }

    // Update positions of existing tokens (skip dragged)
    for (_entity, marker, mut transform) in &mut existing {
        if drag_state.dragging == Some(marker.actor_id) {
            continue;
        }
        if let Some(actor) = current.actors.get(&marker.actor_id) {
            let new_pos = world_position(actor.position.0, actor.position.1, 32.0);
            let current_pos = transform.translation.truncate();
            if (new_pos - current_pos).length_squared() > 0.01 {
                transform.translation = new_pos.extend(transform.translation.z);
            }
        }
    }
}

fn draw_grid(
    config: Res<BattlemapConfig>,
    mut gizmos: Gizmos,
) {
    let color = srgb_to_bevy_color(config.grid_color);
    let size = config.hex_size;
    let half = config.grid_extent as i32;

    for q in -half..=half {
        let r_min = (-half - q).max(-half);
        let r_max = (half - q).min(half);
        for r in r_min..=r_max {
            let center = world_position(q, r, size);
            let verts = hex_vertices(center, size);
            for i in 0..6 {
                gizmos.line_2d(verts[i], verts[(i + 1) % 6], color);
            }
        }
    }
}

fn draw_hex_outlines(
    config: Res<BattlemapConfig>,
    mut gizmos: Gizmos,
    tokens: Query<(&TokenMarker, &Transform), Without<HpBarMarker>>,
) {
    let size = token_hex_radius(&config);
    let outline_color = Color::srgba(0.227, 0.482, 0.835, 1.0);

    for (_marker, transform) in &tokens {
        let center = transform.translation.truncate();
        let verts = hex_vertices(center, size);
        for i in 0..6 {
            gizmos.line_2d(verts[i], verts[(i + 1) % 6], outline_color);
        }
    }
}

fn update_hp_bars(
    config: Res<BattlemapConfig>,
    mut hp_bars: Query<(&Parent, &mut Sprite, &mut Transform), With<HpBarMarker>>,
    tokens: Query<(&TokenMarker, &Transform), Without<HpBarMarker>>,
    state: Option<Res<GameStateResource>>,
) {
    let Some(state) = state else { return };
    let current = state.history.current();
    let hex_r = token_hex_radius(&config);
    let bar_h = 4.0;

    for (parent, mut sprite, mut transform) in &mut hp_bars {
        let Ok((marker, _token_transform)) = tokens.get(parent.get()) else { continue };
        let Some(actor) = current.actors.get(&marker.actor_id) else { continue };

        let hp_ratio = (actor.hp_current as f32 / actor.hp_max.max(1) as f32).clamp(0.0, 1.0);
        let hp_color = if hp_ratio <= 0.0 {
            Color::srgba(0.8, 0.2, 0.2, 0.9)
        } else if hp_ratio <= 1.0 / 3.0 {
            Color::srgba(0.8, 0.667, 0.0, 0.9)
        } else {
            Color::srgba(0.267, 0.667, 0.267, 0.9)
        };

        sprite.color = hp_color;
        sprite.custom_size = Some(Vec2::new(hex_r * hp_ratio, bar_h));
        transform.translation.x = -(1.0 - hp_ratio) * hex_r / 2.0;
    }
}

fn camera_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut scroll_evr: EventReader<bevy::input::mouse::MouseWheel>,
    mut cameras: Query<(&mut Transform, &mut OrthographicProjection), With<Camera>>,
    time: Res<Time>,
    mut last_mouse: Local<Option<Vec2>>,
    window: Query<&Window>,
) {
    let Ok((mut cam_transform, mut projection)) = cameras.get_single_mut() else {
        return;
    };

    let pan_speed = 300.0 * time.delta_secs();
    let zoom_speed = 0.1;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        cam_transform.translation.y += pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        cam_transform.translation.y -= pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        cam_transform.translation.x -= pan_speed;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        cam_transform.translation.x += pan_speed;
    }

    let Ok(window) = window.get_single() else { return };
    let cursor = window.cursor_position();

    if mouse.just_pressed(MouseButton::Middle) {
        *last_mouse = cursor;
    }

    if mouse.pressed(MouseButton::Middle) {
        if let (Some(current), Some(prev)) = (cursor, *last_mouse) {
            let delta = current - prev;
            cam_transform.translation.x -= delta.x * projection.scale;
            cam_transform.translation.y += delta.y * projection.scale;
            *last_mouse = Some(current);
        }
    } else {
        *last_mouse = None;
    }

    for ev in scroll_evr.read() {
        if !keyboard.pressed(KeyCode::ShiftLeft) && !keyboard.pressed(KeyCode::ShiftRight) {
            projection.scale = (projection.scale - ev.y * zoom_speed).max(0.1).min(5.0);
        }
    }
}

fn draw_relation_arrows(
    config: Res<BattlemapConfig>,
    mut gizmos: Gizmos,
    state: Option<Res<GameStateResource>>,
) {
    let Some(state) = state else { return };
    let current = state.history.current();
    let hex_size = config.hex_size;

    for relation in &current.relations {
        let Some(source) = current.actors.get(&relation.source) else { continue };
        let Some(target) = current.actors.get(&relation.target) else { continue };

        let start = world_position(source.position.0, source.position.1, hex_size);
        let end = world_position(target.position.0, target.position.1, hex_size);

        let color = maneuver_arrow_color(relation.maneuver);
        let is_wait = relation.maneuver == ManeuverType::Wait;

        let dir = (end - start).normalize_or_zero();
        let arrow_len = hex_size * 0.4;
        let perp = Vec2::new(-dir.y, dir.x) * arrow_len * 0.3;

        if is_wait {
            let segment_count = 8;
            let line_len = (end - start).length();
            for i in 0..segment_count {
                if i % 2 == 0 {
                    let t0 = i as f32 / segment_count as f32;
                    let t1 = (i + 1) as f32 / segment_count as f32;
                    let s0 = start + dir * line_len * t0;
                    let s1 = start + dir * line_len * t1;
                    gizmos.line_2d(s0, s1, color);
                }
            }
            let tip = end - dir * arrow_len * 0.5;
            gizmos.line_2d(tip, tip - dir * arrow_len + perp, color);
            gizmos.line_2d(tip, tip - dir * arrow_len - perp, color);
        } else {
            let mid = (start + end) / 2.0;
            gizmos.line_2d(start, end, color);
            let tip = mid + dir * arrow_len;
            gizmos.line_2d(tip, tip - dir * arrow_len + perp, color);
            gizmos.line_2d(tip, tip - dir * arrow_len - perp, color);
        }
    }
}

fn maneuver_arrow_color(maneuver: ManeuverType) -> Color {
    use ManeuverType as Mt;
    match maneuver {
        Mt::Attack | Mt::Feint | Mt::FeignBeat | Mt::FeignDefensive | Mt::FeignRuse | Mt::Evaluate => {
            Color::srgba(0.8, 0.2, 0.2, 0.9)
        }
        Mt::AllOutAttackDetermined | Mt::AllOutAttackDouble | Mt::AllOutAttackFeint
        | Mt::AllOutAttackLong | Mt::AllOutAttackStrong | Mt::AllOutAttackRangedDetermined
        | Mt::CommittedAttackDetermined | Mt::CommittedAttackStrong => {
            Color::srgba(1.0, 0.4, 0.0, 0.9)
        }
        Mt::Aim | Mt::Move | Mt::MoveAndAttack | Mt::ChangePosture => {
            Color::srgba(0.8, 0.667, 0.0, 0.9)
        }
        Mt::AllOutDefenseIncreased | Mt::AllOutDefenseDouble | Mt::AllOutDefenseMental
        | Mt::DefensiveAttack | Mt::Ready => {
            Color::srgba(0.2, 0.4, 0.6, 0.9)
        }
        Mt::Concentrate | Mt::AllOutConcentrate | Mt::DoNothing | Mt::Wait => {
            Color::srgba(0.467, 0.333, 0.667, 0.9)
        }
    }
}
