# Architecture

## High-Level Overview

Fantactical is a single Rust binary using the [Bevy](https://bevyengine.org/) ECS
game engine for rendering and input, and
[`bevy_egui`](https://github.com/vladbat00/bevy_egui) for all UI panels. A
lightweight authoritative WebSocket server (Tokio + `tokio-tungstenite`) handles
network sync; clients connect and receive state snapshots.

```
┌──────────────────────────────────────────────────┐
│                    main.rs                        │
│  Plugin registration, settings load, event loop  │
├──────────────────────────────────────────────────┤
│  Bevy App                                        │
│  ┌────────────┐  ┌─────────────┐  ┌───────────┐ │
│  │ Battlemap  │  │  Panels     │  │  Roll     │ │
│  │ Plugin     │  │  Plugin     │  │  Modal    │ │
│  └────────────┘  └─────────────┘  └───────────┘ │
│  ┌────────────────────────────────────────────┐  │
│  │  Phase Machine Plugin                      │  │
│  │  (state transitions, damage, injury)       │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────┐  ┌────────────────────────────┐  │
│  │ Persist    │  │  Network Plugin             │  │
│  │ Plugin     │  │  (tokio tasks, channels)    │  │
│  └────────────┘  └────────────────────────────┘  │
└──────────────────────────────────────────────────┘
```

## Crate Module Structure

```
src/
├── main.rs                  # Binary entry point, plugin registration
├── lib.rs                   # Crate root — public module declarations
├── logging.rs               # LogEvent system (Bevy events → event log + terminal)
├── settings.rs              # Theme registry, settings load/save
├── bin/
│   └── ftctl.rs             # CLI test/control tool
├── model/
│   ├── mod.rs               # All core data types (Actor, GameState, etc.)
│   ├── maneuver_legality.rs # available_maneuvers() pure function
│   ├── injury.rs            # resolve_injury() pure function
│   ├── rolls.rs             # roll_3d6, check_roll, crit tables
│   └── gcs_import.rs        # GCS JSON → Actor parser
├── state/
│   ├── mod.rs
│   └── history.rs           # GameStateHistory persistence (save/load)
├── systems/
│   ├── mod.rs
│   ├── phase_machine.rs     # Turn phase state machine (core game loop)
│   ├── persistence.rs       # Auto-save on round change
│   └── network.rs           # Bevy ↔ network channel bridge
├── ui/
│   ├── mod.rs
│   ├── battlemap.rs         # Bevy viewport, grid, tokens, arrows, events
│   ├── panels.rs            # egui panels (character, tray, event log, GM config)
│   ├── roll_modal.rs        # Attack setup / defense resolution modals
│   └── theme.rs             # egui style injection from Theme trait
├── network/
│   └── mod.rs               # Message protocol (ClientMessage, ServerMessage)
├── server/
│   └── mod.rs               # WebSocket server (tokio task, connection handler)
└── client/
    └── mod.rs               # WebSocket client (connect, reconnect backoff)
```

## Data Flow: Maneuver Declaration

```
User drags maneuver card onto token
        │
        ▼
  panels.rs: render_panels()
  ─ Sends ManeuverDeclaredEvent (Bevy Event)
        │
        ▼
  phase_machine.rs: process_phase_machine()
  ─ Validates maneuver legality + combo whitelist
  ─ Clones current GameState → modifies → pushes new snapshot
  ─ Routes to AttackSetup (offensive) or Complete (non-offensive)
        │
        ▼
  roll_modal.rs: render_roll_modal()
  ─ Renders modal overlay for current phase
  ─ User selects attack, hit location → AttackSetupConfirmedEvent
  ─ User clicks "Roll 3d6!" → RollRequestedEvent
  ─ User selects defense → DefenseSelectedEvent
        │
        ▼
  phase_machine.rs: resolve_injury()
  ─ Calculates DR, applies wounding multiplier
  ─ Checks major wound, knockdown, death thresholds
  ─ Pushes final state with injury applied
        │
        ▼
  logging.rs: process_log_events()
  ─ Appends LogEntry to EventLog
  ─ Prints to terminal (info!/warn!/error!)
```

## Bevy Plugin Ordering

Plugins are registered in `main.rs` in this order, which determines system
execution order within the `Update` schedule:

1. **BattlemapPlugin** — Grid rendering, token sync, GM action handling, camera
2. **PanelsPlugin** — All egui panel rendering, drag-and-drop
3. **PhaseMachinePlugin** — State machine processing, event handling
4. **RollModalPlugin** — Modal overlay rendering (runs after panels for
   layering)

The `detect_drag_input` and `detect_token_right_click` systems run in `First`
schedule (before any `Update` systems), ensuring raw input is captured before
egui consumes it.

## Event Sourcing

All game state mutations go through `GameStateHistory` — never in-place:

```rust
pub struct GameStateHistory {
    pub snapshots: Vec<GameState>,
    pub current: usize,    // index into snapshots
}
```

- **Push**: `history.push(new_state)` clones the current snapshot, applies
  modifications, and appends. Previous snapshots are immutable.
- **Rewind**: `history.rewind()` decrements `current`. Any future snapshots
  beyond the new `current` are truncated when a new push occurs.
- **Current**: `history.current()` returns `&snapshots[current]`.

The event log (`EventLog`) is a separate `Resource` — NOT part of
`GameState`. This prevents an ever-growing log from being duplicated across
every snapshot.

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **No in-place mutation** | Supports Luck advantage, misclick recovery, network sync |
| **Pure model functions** | `available_maneuvers()`, `resolve_injury()`, `check_roll()` are pure — no Bevy deps, fully testable |
| **Event-driven phase transitions** | UI panels emit `Event`s; phase machine reads them. Loose coupling. |
| **Logically-sized viewport** | All cursor→world conversions use `window.width()` / `window.height()` (logical pixels), not physical, to avoid HiDPI offset bugs |
| **Single binary** | Server and client share the same binary; mode selected at startup via `NetworkPlugin` config |
| **Thread-local seeded RNG** | `StdRng` seeded from hashed system time + process ID at first access per thread — eliminates deterministic dice sequences across runs |
