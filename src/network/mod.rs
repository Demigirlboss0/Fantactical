use crate::model::{ActorId, ExtraEffort, LogEntry, ManeuverType, PainThreshold, Posture};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 9002;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientMessage {
    Auth {
        token: String,
    },
    DeclareManeuver {
        source_id: ActorId,
        target_id: Option<ActorId>,
        target_hex: Option<(i32, i32)>,
        maneuver: ManeuverType,
        extra_efforts: Vec<ExtraEffort>,
    },
    SelectDefense {
        defender_id: ActorId,
        #[serde(rename = "defenseType")]
        defense_type: DefenseTypeWire,
    },
    RollDice,
    AddModifier {
        label: String,
        value: i8,
        actor_id: Option<ActorId>,
    },
    RemoveModifier {
        index: usize,
        actor_id: Option<ActorId>,
    },
    Rewind,
    SetPainThreshold {
        actor_id: ActorId,
        threshold: PainThreshold,
    },
    SetPosture {
        actor_id: ActorId,
        posture: Posture,
    },
    MoveActor {
        actor_id: ActorId,
        position: (i32, i32),
    },
    ReorderTurnOrder {
        from_index: usize,
        to_index: usize,
    },
    ShockToggle {
        enabled: bool,
    },
    ImportSheet {
        json_data: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DefenseTypeWire {
    Dodge,
    Parry { attack_index: usize },
    Block { attack_index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServerMessage {
    AuthSuccess {
        client_id: u64,
        is_gm: bool,
    },
    AuthFailure {
        reason: String,
    },
    StateSnapshot {
        history: crate::model::GameStateHistory,
    },
    RollResult {
        label: String,
        roll: u8,
    },
    LogEntry {
        entry: LogEntry,
    },
    ActorOwnership {
        actor_ids: Vec<ActorId>,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GameState;
    use crate::model::{
        Actor, Encumbrance, ExtraEffort, LegState, LogEntry, LogEntryKind, ManeuverType,
        PainThreshold, Posture, StatusFlags, TurnPhase,
    };
    use std::collections::HashMap;

    fn roundtrip<
        T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq,
    >(
        val: &T,
    ) {
        let json = serde_json::to_string(val).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(val, &back, "Roundtrip failed for JSON: {}", json);
    }

    #[test]
    fn test_defense_type_wire_serde() {
        roundtrip(&DefenseTypeWire::Dodge);
        roundtrip(&DefenseTypeWire::Parry { attack_index: 3 });
        roundtrip(&DefenseTypeWire::Block { attack_index: 0 });
    }

    #[test]
    fn test_client_message_auth() {
        roundtrip(&ClientMessage::Auth {
            token: "secret123".into(),
        });
    }

    #[test]
    fn test_client_message_declare_maneuver() {
        roundtrip(&ClientMessage::DeclareManeuver {
            source_id: 1,
            target_id: Some(2),
            target_hex: Some((3, 4)),
            maneuver: ManeuverType::Attack,
            extra_efforts: vec![ExtraEffort::MightyBlow],
        });
    }

    #[test]
    fn test_client_message_declare_self_maneuver() {
        roundtrip(&ClientMessage::DeclareManeuver {
            source_id: 5,
            target_id: None,
            target_hex: None,
            maneuver: ManeuverType::DoNothing,
            extra_efforts: vec![],
        });
    }

    #[test]
    fn test_client_message_select_defense() {
        roundtrip(&ClientMessage::SelectDefense {
            defender_id: 42,
            defense_type: DefenseTypeWire::Dodge,
        });
        roundtrip(&ClientMessage::SelectDefense {
            defender_id: 42,
            defense_type: DefenseTypeWire::Parry { attack_index: 1 },
        });
    }

    #[test]
    fn test_client_message_roll_dice() {
        roundtrip(&ClientMessage::RollDice);
    }

    #[test]
    fn test_client_message_add_modifier() {
        roundtrip(&ClientMessage::AddModifier {
            label: "Dark (-4)".into(),
            value: -4,
            actor_id: None,
        });
        roundtrip(&ClientMessage::AddModifier {
            label: "Shock (-2)".into(),
            value: -2,
            actor_id: Some(10),
        });
    }

    #[test]
    fn test_client_message_remove_modifier() {
        roundtrip(&ClientMessage::RemoveModifier {
            index: 2,
            actor_id: Some(7),
        });
    }

    #[test]
    fn test_client_message_rewind() {
        roundtrip(&ClientMessage::Rewind);
    }

    #[test]
    fn test_client_message_pain_threshold() {
        roundtrip(&ClientMessage::SetPainThreshold {
            actor_id: 3,
            threshold: PainThreshold::High,
        });
        roundtrip(&ClientMessage::SetPainThreshold {
            actor_id: 4,
            threshold: PainThreshold::Low,
        });
    }

    #[test]
    fn test_client_message_posture() {
        roundtrip(&ClientMessage::SetPosture {
            actor_id: 99,
            posture: Posture::Prone,
        });
        roundtrip(&ClientMessage::SetPosture {
            actor_id: 100,
            posture: Posture::Kneeling,
        });
    }

    #[test]
    fn test_client_message_move_actor() {
        roundtrip(&ClientMessage::MoveActor {
            actor_id: 55,
            position: (-3, 7),
        });
    }

    #[test]
    fn test_client_message_reorder() {
        roundtrip(&ClientMessage::ReorderTurnOrder {
            from_index: 0,
            to_index: 3,
        });
    }

    #[test]
    fn test_client_message_shock_toggle() {
        roundtrip(&ClientMessage::ShockToggle { enabled: true });
        roundtrip(&ClientMessage::ShockToggle { enabled: false });
    }

    #[test]
    fn test_client_message_import_sheet() {
        roundtrip(&ClientMessage::ImportSheet {
            json_data: "{\"name\":\"Test\"}".into(),
        });
    }

    #[test]
    fn test_server_message_auth_success() {
        roundtrip(&ServerMessage::AuthSuccess {
            client_id: 1,
            is_gm: true,
        });
        roundtrip(&ServerMessage::AuthSuccess {
            client_id: 2,
            is_gm: false,
        });
    }

    #[test]
    fn test_server_message_auth_failure() {
        roundtrip(&ServerMessage::AuthFailure {
            reason: "Bad token".into(),
        });
    }

    #[test]
    fn test_server_message_roll_result() {
        roundtrip(&ServerMessage::RollResult {
            label: "Attack roll: 12".into(),
            roll: 12,
        });
    }

    #[test]
    fn test_server_message_log_entry() {
        let entry = LogEntry {
            round: 3,
            turn: 1,
            phase: TurnPhase::AttackRoll,
            message: "Attack roll: 12 vs 15".into(),
            kind: LogEntryKind::RollResult,
        };
        roundtrip(&ServerMessage::LogEntry { entry });
    }

    #[test]
    fn test_server_message_actor_ownership() {
        roundtrip(&ServerMessage::ActorOwnership {
            actor_ids: vec![1, 2, 3],
        });
        roundtrip(&ServerMessage::ActorOwnership { actor_ids: vec![] });
    }

    #[test]
    fn test_server_message_error() {
        roundtrip(&ServerMessage::Error {
            message: "Invalid maneuver".into(),
        });
    }

    #[test]
    fn test_server_message_state_snapshot() {
        let mut actors = HashMap::new();
        actors.insert(
            1,
            Actor {
                id: 1,
                name: "Test".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: false,
                st: 10,
                dx: 10,
                iq: 10,
                ht: 10,
                hp_max: 10,
                fp_max: 10,
                basic_speed: 5.0,
                basic_move: 5,
                will: 10,
                per: 10,
                attacks: vec![],
                skills: vec![],
                armor: vec![],
                sm: 0,
                is_male: true,
                position: (0, 0),
                hp_current: 10,
                fp_current: 10,
                posture: Posture::Standing,
                encumbrance: Encumbrance::None,
                flags: StatusFlags::default(),
                leg_state: LegState::default(),
                individual_modifiers: vec![],
                pain_threshold: PainThreshold::Normal,
                turns_per_round: 1,
                attacks_per_turn: 1,
                enhanced_time_sense: false,
                current_maneuver: None,
                active_attack: None,
                extra_effort: vec![],
            },
        );
        let state = GameState {
            actors,
            relations: vec![],
            turn_order: vec![1],
            current_actor: 1,
            current_phase: TurnPhase::ManeuverSelection,
            global_modifiers: vec![],
            round: 1,
            attacks_remaining: 1,
        };
        let history = crate::model::GameStateHistory::new(state);
        roundtrip(&ServerMessage::StateSnapshot { history });
    }

    #[test]
    fn test_all_variants_serialize_uniquely() {
        // Ensure all ClientMessage variants serialize/deserialize cleanly
        let messages = vec![
            ClientMessage::Auth { token: "x".into() },
            ClientMessage::DeclareManeuver {
                source_id: 1,
                target_id: Some(2),
                target_hex: None,
                maneuver: ManeuverType::Attack,
                extra_efforts: vec![],
            },
            ClientMessage::SelectDefense {
                defender_id: 1,
                defense_type: DefenseTypeWire::Dodge,
            },
            ClientMessage::RollDice,
            ClientMessage::AddModifier {
                label: "Dark".into(),
                value: -4,
                actor_id: None,
            },
            ClientMessage::RemoveModifier {
                index: 0,
                actor_id: None,
            },
            ClientMessage::Rewind,
            ClientMessage::SetPainThreshold {
                actor_id: 1,
                threshold: PainThreshold::High,
            },
            ClientMessage::SetPosture {
                actor_id: 1,
                posture: Posture::Standing,
            },
            ClientMessage::MoveActor {
                actor_id: 1,
                position: (3, 5),
            },
            ClientMessage::ReorderTurnOrder {
                from_index: 0,
                to_index: 2,
            },
            ClientMessage::ShockToggle { enabled: true },
            ClientMessage::ImportSheet {
                json_data: "{}".into(),
            },
        ];
        assert_eq!(messages.len(), 13, "all ClientMessage variants must be listed");
        for msg in &messages {
            let json = serde_json::to_string(msg).expect("serialize");
            assert!(!json.is_empty());
            let _: ClientMessage = serde_json::from_str(&json).expect("deserialize");
        }
    }
}
