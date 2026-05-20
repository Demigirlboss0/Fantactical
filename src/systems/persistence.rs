use crate::state::history;
use crate::ui::battlemap::GameStateResource;
use bevy::prelude::*;

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, auto_save_on_round_change);
    }
}

#[derive(Resource, Default)]
pub struct PersistenceState {
    pub last_saved_round: u32,
    pub session_name: String,
    pub output_dir: String,
    pub last_saved_snapshot: usize,
}

fn auto_save_on_round_change(
    state: Option<Res<GameStateResource>>,
    mut ps: Local<PersistenceState>,
) {
    let Some(state) = state else { return };
    let history = &state.history;
    let current = history.current();

    if ps.output_dir.is_empty() {
        ps.output_dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".into());
        ps.session_name = "fantactical_session".into();
    }

    if current.round > ps.last_saved_round && ps.last_saved_round > 0 {
        let dir = std::path::Path::new(&ps.output_dir);
        let path = history::save_file_path(&ps.session_name, ps.last_saved_round, dir);
        if let Err(e) = history::save_history(history, &path) {
            error!(
                "Failed to save history for round {}: {e}",
                ps.last_saved_round
            );
        } else {
            info!("Saved: {}", path.display());
        }
    }

    ps.last_saved_round = current.round;
    ps.last_saved_snapshot = history.current;
}
