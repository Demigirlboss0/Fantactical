# Fantactical

A plug-and-play solution for running GURPS 4e combat, with integrated tracking,
virtual tabletop, dice rolling, and network multiplayer.

Fantactical is a desktop application built in Rust using the Bevy game engine.
It imports GCS character sheets directly, manages the full GURPS turn-phase
state machine, resolves injuries with the complete hit-location × damage-type
matrix, and renders a hex-grid VTT with maneuver cards styled like a trading
card game.

## How It Works

1. **Import a GCS sheet** — Drop a `.gcs` file into the app. All stats, skills,
   attacks (melee and ranged), armor, and portrait are parsed automatically.
2. **Arrange the battlemap** — Drag tokens to hexes. The grid uses Chebyshev
   (axial) distance for range penalties.
3. **Play maneuvers as cards** — On an actor's turn, a tray of maneuver cards
   appears. Each card shows the maneuver name and a rules summary. Drag a card
   onto your token (self-targeted) or an enemy token (offensive) to declare it.
4. **Resolve attacks** — A modal overlay guides you through attack selection,
   hit-location targeting, modifier breakdown, the 3d6 attack roll, defense
   resolution, and injury application — following B552 location rules, B556
   critical hit tables, and full wounding multipliers.
5. **GM tools** — Rewind the turn history, toggle shock rules, set per-actor
   pain thresholds, manage global modifiers, and adjustable turn order.

## Features — ✅ Complete

| Area | Details |
|------|---------|
| **Turn phase state machine** | ManeuverSelection → AttackSetup → ManeuverConfirmed → AttackRoll → DefenseResolution → InjuryResolution → NonCombatResolution → Complete, with auto-advance and extra attacks |
| **GCS sheet import** | Direct `.gcs` JSON parsing — attributes, skills, equipment weapons (melee + ranged), armor DR by location, portrait, recursive container children |
| **GUI panels** | Character panel (sortable turn-order cards with HP/FP bars), event log, GM config tab, maneuver tray, attack setup modal, defense-resolution modal |
| **Maneuver trading-card UI** | All 28 maneuver types rendered as tall cards with colored borders, name header, rules description; drag-and-drop onto tokens |
| **Hit locations** | Full hit-location support for bipedal humanoids (28 locations across head, torso, limbs, extremities, joints, and vasculature), including optional locations from ***GURPS Martial Arts*** and later supplements |
| **Injury resolution** | Full hit-location × damage-type matrix, DR calculation (armor stacking + inherent), wounding multipliers, major wounds, knockdown/stun, consciousness/death checks, limb crippling, shock |
| **Crit tables** | B556 critical hit table (11 result variants including HalveDR, IgnoreDR, StunAutomatic, CrippleLimbAutomatic) and B557 critical miss table |
| **Dice rolling** | 3d6 success checks with proper half-skill crit rules; seeded RNG via hashed system time for near-zero determinism |
| **Maneuver legality** | Posture/status/encumbrance/crippled-leg filters; combo whitelist for Extra Effort |
| **Event sourcing** | Immutable GameStateHistory with push/rewind; per-round auto-save |
| **VTT battlemap** | Hex grid (flat-top axial), token spawning with portrait circles, maneuver relation arrows colored by category, camera pan/zoom, token drag-to-move |
| **GM config** | Global/per-actor modifiers, lighting presets, shock toggle, pain threshold per actor, rewind |
| **WebSocket networking** | Authoritative server + client with session-token auth, exponential-backoff reconnect, StateSnapshot broadcast, GM/client role distinction |
| **Mil-sim theme** | Dark tactical aesthetic with Theme trait infrastructure for future themes |

## Features — 🧪 Needs QA/QC

These systems are implemented end-to-end but need real-session battle-testing:

- Certain combat maneuvers (Feint margin calculation, Wait trigger resolution, Evaluate/Aim accumulation across turns)
- Longer combats with many actors — turn-order correctness, multi-turn state consistency
- Full injury edge cases (neck vascular, limb vascular, joint crippling, eye destruction, Spine rear-arc only)
- Networking under load, multi-client synchronization, reconnection mid-combat
- Cross-platform compatibility (Linux primary, Windows/macOS untested)
- Configurable panel heights via `fantactical_settings.json`
- Crit miss table application (defined but not yet wired into the phase machine)

## Planned — 🔨 In the Works

- **Visual overhaul / polish** — Consistent theme colors plumbed through all panels, runtime theme switching
- **3D battlemap** — Bevy 3D rendering with camera rotation, elevation, and terrain
- **Fog of war** — GM-controlled visibility per token
- **GM scratchpad** — Free-text note-taking panel for session tracking
- **In-built chat** — Text chat between connected clients
- **Multiple stock themes** — Beyond mil-sim: high-contrast, fantasy-parchment, cyberpunk
- **Expanded crit tables** — Full Critical Head Blow table, crit miss effects wired into phase machine
- **Edge/optional rules** — Minute-of-Angle, point-blank shotshell, hit-location sub-tables (Abdomen → Vitals/DigestiveTract/Pelvis/Groin)
- **Better config menu** — In-GUI settings for panel heights, theme selection, network configuration

## Stretch Goals — 🌌 Someday

- **Physically-based 3D dice** — Rendered dice that tumble across the tabletop
- **VPS-to-WebUI hosting** — Run the server headless, serve the UI to browsers via WebGPU
- **Sound effects** — Combat audio, dice rolls, injury feedback
- **Modularity / plugin support** — User-defined themes, house rule plugins, custom maneuver cards

## Getting Started

### Prerequisites
- Rust toolchain (1.80+)
- System libraries for Bevy (see [Bevy's setup guide](https://bevyengine.org/learn/quick-start/getting-started/setup/))

### Build & Run
```bash
cargo run --release
```

### CLI Test Tool
```bash
# Roll 3d6
cargo run --bin ftctl -- dice

# Check a roll against a skill
cargo run --bin ftctl -- check-roll --roll 12 --skill 15

# Test injury resolution
cargo run --bin ftctl -- test-injury --hp 12 --location torso --damage 5 --damage-type cut

# List available maneuvers for a given posture
cargo run --bin ftctl -- available-maneuvers --posture standing
```

### Running Tests
```bash
cargo test                # 115 unit tests + 3 integration tests
```

### Network Mode
Edit `src/main.rs` to set `NetworkMode::Server` or `NetworkMode::Client` with a
host, port, and session token.

## License

MIT

---

**Fantactical is pre-alpha software.** If you find bugs, rough edges, or have
feature ideas, please [open an issue](https://github.com/Demigirlboss0/Fantactical/issues)
or submit a pull request. Feedback is hugely appreciated at this stage.
