use bevy::prelude::*;
use crate::model::{LogEntry, LogEntryKind, TurnPhase};
use crate::ui::battlemap::EventLogResource;

#[derive(Event, Debug, Clone)]
pub struct LogEvent {
    pub message: String,
    pub kind: LogEntryKind,
    pub round: u32,
    pub turn: u64,
    pub phase: TurnPhase,
}

impl LogEvent {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: LogEntryKind::Info,
            round: 0,
            turn: 0,
            phase: TurnPhase::ManeuverSelection,
        }
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: LogEntryKind::Warning,
            round: 0,
            turn: 0,
            phase: TurnPhase::ManeuverSelection,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: LogEntryKind::Error,
            round: 0,
            turn: 0,
            phase: TurnPhase::ManeuverSelection,
        }
    }

    pub fn with_context(mut self, round: u32, turn: u64) -> Self {
        self.round = round;
        self.turn = turn;
        self
    }

    #[allow(dead_code)]
    pub fn with_phase_context(mut self, round: u32, turn: u64, phase: TurnPhase) -> Self {
        self.round = round;
        self.turn = turn;
        self.phase = phase;
        self
    }

    pub fn with_phase(mut self, phase: TurnPhase) -> Self {
        self.phase = phase;
        self
    }
}

pub struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LogEvent>()
            .add_systems(Update, process_log_events);
    }
}

fn process_log_events(
    mut events: EventReader<LogEvent>,
    mut event_log: Option<ResMut<EventLogResource>>,
) {
    for ev in events.read() {
        match ev.kind {
            LogEntryKind::Info => info!("{}", ev.message),
            LogEntryKind::Warning => warn!("{}", ev.message),
            LogEntryKind::Error => error!("{}", ev.message),
            _ => info!("{}", ev.message),
        }

        if let Some(ref mut log_res) = event_log {
            log_res.log.push(LogEntry {
                round: ev.round,
                turn: ev.turn,
                phase: ev.phase,
                message: ev.message.clone(),
                kind: ev.kind.clone(),
            });
        }
    }
}
