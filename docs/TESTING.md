# Testing

Fantactical has 118 tests total: 115 unit tests across the library crate, and 3
async integration tests for networking.

## Running Tests

```bash
# All tests
cargo test

# Unit tests only (fast)
cargo test --lib

# Integration tests only (requires network)
cargo test --test network_integration

# Single-threaded (avoids port conflicts in integration tests)
cargo test -- --test-threads=1

# Specific module
cargo test --lib -- injury
cargo test --lib -- gcs

# With output
cargo test -- --nocapture
```

## Unit Tests by Module

| Module | Tests | Key Areas |
|--------|-------|-----------|
| `model/mod.rs` | 15 | Hit location penalties, inherent DR, random table, wounding multipliers, Dodge, encumbrance, effective move, history push/rewind/truncation, serde roundtrip, flip_side, turn order sorting (ETS priority, DX tiebreaker) |
| `model/maneuver_legality.rs` | 14 | Dead/unconscious/stunned/knocked-down actors, all postures (Standing through Crawling), leg crippling (single/both), extra-heavy encumbrance, no-attacks, combo whitelists (Aim, AOA, Attack, Wait), ranged-only |
| `model/injury.rs` | 22 | Torso crushing, DR, skull impaling, neck cutting, vitals impaling, eye illegal/ignores DR, leg crippling, shock toggle, pain thresholds (HPT/LPT), major wound, death, consciousness, toxic, corrosive, crit double/ignore DR |
| `model/rolls.rs` | 10 | 3d6 bounds, crit success/failure (3-4, 17-18), half-skill crit, margins, crit hit table, crit miss table, damage roll |
| `model/gcs_import.rs` | 7 | Damage string parsing, reach parsing, difficulty/relative level, Francesca import (attributes, attacks, ranged, naming), Nur import (melee + ranged weapons) |
| `state/history.rs` | 2 | Save/load roundtrip, file naming convention |
| `network/mod.rs` | 19 | Serde roundtrip for every ClientMessage variant, every ServerMessage variant, DefenseTypeWire, all-variant uniqueness |
| `server/mod.rs` | 3 | Default config, custom config, channel creation + Send/Sync |
| `client/mod.rs` | 4 | Channel creation, variant matching, Send/Sync, disconnect before connect |

**Plus** 19 tests in `network/mod.rs` (serde roundtrips for all protocol variants).

## Integration Tests (`tests/network_integration.rs`)

3 async tokio tests using multi-threaded runtime:

| Test | What It Verifies |
|------|-----------------|
| `test_server_client_auth_and_broadcast` | Full roundtrip: server startup → client connect → token auth → ClientConnected → message forwarding (DeclareManeuver) → server broadcast (RollResult) → client receives |
| `test_server_multi_client_second_is_non_gm` | GM assignment: first client gets `is_gm: true`, second gets `is_gm: false` |
| `test_client_connect_to_dead_server` | Client channels survive without panicking when connecting to a closed port; receives Disconnected event |

Each test uses `find_free_port()` to avoid port conflicts with other
running services or leftover TIME_WAIT sockets.

## CLI Test Tool (`ftctl`)

`src/bin/ftctl.rs` (361 lines) provides manual testing commands:

```bash
cargo run --bin ftctl -- dice                    # Roll 3d6
cargo run --bin ftctl -- dice --count 5          # Roll 5d6
cargo run --bin ftctl -- check-roll 12 15        # Check roll 12 vs skill 15
cargo run --bin ftctl -- damage-roll 2 --adds 1  # Roll 2d+1
cargo run --bin ftctl -- test-injury \
    --hp 12 --location skull --damage 8 --damage-type imp --dr 2
cargo run --bin ftctl -- available-maneuvers --posture standing
cargo run --bin ftctl -- range-penalty 15        # Range penalty for 15 yards
cargo run --bin ftctl -- hex-distance 0 0 3 2    # Distance between (0,0) and (3,2)
cargo run --bin ftctl -- save-state              # Save current test state
cargo run --bin ftctl -- load-state              # Load test state
```

## Test Coverage Gaps

The following areas lack test coverage and would benefit from additional tests:

### Injury Pipeline
- Jaw + Crushing knockdown modifier (-1)
- Groin + Crushing + male target knockdown modifier (-5)
- Spine injury (inherent DR 3, rear-only targeting)
- NeckVascular / LimbVascular wounding multipliers
- FatigueDmg at different locations
- Multiple armor pieces stacking DR at same location
- CritHitResult::StunAutomatic and KnockdownAutomatic effects

### Crit Tables
- crit_miss_table for rolls other than 15 (FallDown):
  - 3-4 (DropWeapon), 5 (HitSelf), 8 (WeaponUnready), 16 (HitAlly)
- check_roll at edge skills (skill=3, skill=30)

### Maneuver Legality
- Sitting posture restrictions
- Crouching posture (should match Standing)
- Move + HeroicCharge combo whitelist
- FeignBeat / FeignDefensive combo whitelists

### UI / Systems
- No automated tests for any Bevy system or egui panel rendering
- Manual testing required for UI interactions

### Networking
- No test for server broadcasting to multiple concurrent clients
- No test for client reconnection (backoff behavior)
- No test for GM vs non-GM permission enforcement
- No test for server under load / many concurrent messages

## Adding Tests

Tests follow Rust's standard `#[cfg(test)] mod tests { ... }` pattern.
Model-level tests use pure functions and are preferred. Bevy system tests
would require `App` construction and are not yet implemented.
