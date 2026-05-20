# Phase Machine

The turn phase state machine lives in `src/systems/phase_machine.rs` (723 lines).
It is the core game loop — all combat resolution flows through it.

## Architecture

The phase machine is a single Bevy system (`process_phase_machine`) that runs
in `Update`. It reads Bevy `Event`s emitted by UI panels and transitions the
game through each phase. All state mutations produce new `GameState` snapshots
pushed to `GameStateHistory`.

### Events

| Event | Emitted By | Consumed In |
|-------|-----------|-------------|
| `ManeuverDeclaredEvent` | `panels.rs` (drag-and-drop) | `ManeuverSelection` |
| `AttackSetupConfirmedEvent` | `roll_modal.rs` | `AttackSetup` |
| `RollRequestedEvent` | `roll_modal.rs` | `AttackRoll` |
| `DefenseSelectedEvent` | `roll_modal.rs` | `DefenseResolution` |
| `PhaseAdvanceEvent` | `roll_modal.rs` | `ManeuverConfirmed`, `DefenseResolution`, `NonCombatResolution` |
| `CancelPhaseEvent` | `roll_modal.rs` | All phases (returns to `ManeuverSelection`) |

### ModalState Resource

`src/systems/phase_machine.rs:70` — shared mutable state between the phase
machine and the roll modal UI:

```rust
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
```

## Phase Walkthrough

### 1. ManeuverSelection (`src/systems/phase_machine.rs:112`)

Entry point for a new turn. The maneuver tray shows `available_maneuvers(actor)`.

When `ManeuverDeclaredEvent` is received:
- Validates the maneuver is in `available_maneuvers()` output
- Validates `extra_efforts` against `combo_whitelist()`
- Sets `actor.current_maneuver` (persists until next ManeuverSelection)
- For `Move`: updates `actor.position` if within `effective_move()` range
- Creates `ManeuverRelation` for aimed/evaluated targets
- Routes to `AttackSetup` (offensive) or `Complete` (non-offensive)

**Cancel** returns to `ManeuverSelection` on the same actor.

### 2. AttackSetup (`src/systems/phase_machine.rs:218`)

User selects an attack (from `actor.attacks`) and a hit location (from
`HitLocation::iter_all()`). The modal shows attack names, damage formulas,
and location penalties.

On `AttackSetupConfirmedEvent`:
- Builds full modifier breakdown:
  - Base skill
  - Hit location penalty
  - Attacker posture penalty (Prone: -4, Kneeling/Crawling: -2)
  - Range penalty (auto-calculated from Chebyshev distance)
  - Target SM modifier
  - Target posture penalty
  - Aim/Evaluate accumulated bonuses from relations
  - Global modifiers (`AttackRolls` or `AllRolls`)
  - Individual actor modifiers
- Computes `effective_skill = max(0, base_skill + total_mod)`
- Sets `actor.active_attack` to the selected index
- Transitions to `ManeuverConfirmed`

### 3. ManeuverConfirmed (`src/systems/phase_machine.rs:327`)

Displays the effective skill and full modifier breakdown. Player reviews and
either proceeds to `AttackRoll` or cancels.

### 4. AttackRoll (`src/systems/phase_machine.rs:346`)

Player clicks "Roll 3d6!" → `RollRequestedEvent` is sent with the roll result.

Processing:
- Calls `check_roll(roll, effective_skill)` → `RollOutcome`
- **Critical Success** (3-4, 5 if skill≥15, 6 if skill≥16):
  - Rolls on crit hit table → `CritHitResult`
  - Rolls damage via `roll_damage()`
  - Stores in `modal.rolled_damage` with crit multiplier applied
  - Transitions to `DefenseResolution`
- **Success**: Rolls damage, transitions to `DefenseResolution`
- **Failure / Critical Failure**: Transitions to `Complete` (miss)

### 5. DefenseResolution (`src/systems/phase_machine.rs:427`)

`compute_defense_options()` builds the defender's available active defenses:
- Dodge (always available — value from `actor.dodge()`)
- Parry (per weapon with `parry_bonus`)
- Block (per weapon with `block_bonus`)

**AOA defense denial**: `denies_active_defenses()` checks the defender's
`current_maneuver`. If it's an AOA variant or All-Out Concentrate, the defender
gets zero defense options and defense value 0.

Defense roll: `check_roll(roll, defense_value)`.
- **Success / Critical Success**: Attack defended → `Complete`
- **Failure**: Calls `apply_pending_injury()` → `resolve_injury()`

**Skip Defense**: `PhaseAdvanceEvent` also calls `apply_pending_injury()` —
the attack connects without a defense roll.

### 6. InjuryResolution

Not a separate phase — injury is applied inline in `DefenseResolution` via
`apply_pending_injury()` (`src/systems/phase_machine.rs:588`).

See [INJURY_PIPELINE.md](INJURY_PIPELINE.md) for the full injury resolution
walkthrough.

### 7. NonCombatResolution (`src/systems/phase_machine.rs:523`)

For accumulated maneuvers (Aim, Evaluate):
- Aim: `turns += 1`, `accumulated = min(weapon_acc + turns - 1, weapon_acc + 2)`
- Evaluate: `accumulated = min(accumulated + 1, 3)`

Relations are updated in-place (since this is a new snapshot anyway).

### 8. Complete (`src/systems/phase_machine.rs:106`)

Auto-advances every frame:

```
advance_turn()
    │
    ├── Clear actor.current_maneuver (removed — persists until next ManeuverSelection)
    ├── Clear actor.extra_effort
    ├── Increment stun_turns if stunned
    ├── Retain only accumulating relations (Aim, Evaluate)
    │
    ├── Extra attacks remaining? → stay on same actor, decrement attacks_remaining
    │
    └── No extra attacks → advance turn_order index
         ├── Wrap → increment round
         └── Set new current_actor + attacks_remaining
```

## Extra Attacks

Actors with `attacks_per_turn > 1` get multiple attack cycles within one turn.
The `attacks_remaining` counter on `GameState` tracks this:

```
ManeuverSelection → AttackSetup → ... → Complete
    │                                         │
    └─── attacks_remaining > 1 ──────────────┘ (loop)
    │
    └─── attacks_remaining == 1 → advance to next actor
```

## Defense Computation

`defense_value()` (`src/systems/phase_machine.rs:638`):
- **Dodge**: `actor.dodge()` (basic_speed + 3 floor, minus encumbrance)
- **Parry**: `(basic_speed / 2).floor() + 3 + parry_bonus`
- **Block**: `(basic_speed / 2).floor() + 3 + block_bonus` (uses the
  specified `attack_index`, not first weapon found)

`compute_defense_options()` (`src/systems/phase_machine.rs:674`) checks
`denies_active_defenses()` first — returns empty options for AOA actors.

## Turn Order Sorting

`sort_turn_order()` (`src/model/mod.rs:885`):
1. ETS actors first (sorted by Basic Speed desc, then DX desc)
2. Non-ETS actors by Basic Speed desc
3. Ties broken by DX desc
4. GM can reorder via character panel buttons

## Maneuver Legality

`available_maneuvers()` (`src/model/maneuver_legality.rs`) is a pure function
called every frame the tray renders. Priority order (each level overrides):

1. Dead → empty set
2. Unconscious → empty set
3. Stunned → `[DoNothing]` only
4. KnockedDown → `[DoNothing, ChangePosture]`
5. Crippled leg(s) → forced posture, Move halved/zero
6. Posture filter → blacklists for Prone, Kneeling, Crawling
7. Encumbrance → ExtraHeavy removes Move and Attack
8. No attacks → removes offensive maneuvers
9. Ranged only → removes Feint variants
10. Full set available
