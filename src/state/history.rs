use std::path::Path;

use crate::model::GameStateHistory;
use anyhow::Context;

pub fn save_history(history: &GameStateHistory, path: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(history)?;
    std::fs::write(path, &json)
        .with_context(|| format!("Failed to write history to {}", path.display()))?;
    Ok(())
}

pub fn load_history(path: &Path) -> anyhow::Result<GameStateHistory> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read history from {}", path.display()))?;
    let history: GameStateHistory = serde_json::from_str(&json)?;
    Ok(history)
}

pub fn save_file_path(session_name: &str, round: u32, output_dir: &Path) -> std::path::PathBuf {
    output_dir.join(format!("{session_name}_round{round}.json"))
}

pub fn log_file_path(session_name: &str, output_dir: &Path) -> std::path::PathBuf {
    output_dir.join(format!("{session_name}_log.json"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{GameState, TurnPhase};

    fn empty_state(round: u32) -> GameState {
        GameState {
            actors: HashMap::new(),
            relations: vec![],
            turn_order: vec![],
            current_actor: 0,
            current_phase: TurnPhase::ManeuverSelection,
            global_modifiers: vec![],
            round,
            attacks_remaining: 1,
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_file_path("test_session", 1, dir.path());
        let history = GameStateHistory::new(empty_state(1));

        save_history(&history, &path).unwrap();
        let loaded = load_history(&path).unwrap();
        assert_eq!(loaded.current().round, 1);
        assert_eq!(loaded.snapshots.len(), 1);
    }

    #[test]
    fn test_save_file_naming() {
        let path = save_file_path("combat", 3, Path::new("/tmp"));
        assert_eq!(path.to_str().unwrap(), "/tmp/combat_round3.json");
    }
}
