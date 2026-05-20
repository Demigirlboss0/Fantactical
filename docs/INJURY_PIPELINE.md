# Injury Pipeline

Injury resolution is implemented in `src/model/injury.rs` (855 lines) as a pure
function `resolve_injury()`. It takes a `&GameState` reference and returns a new
`GameState` with injury applied — never mutates in place.

## Entry Point

```rust
pub fn resolve_injury(
    state: &GameState,
    target_id: ActorId,
    hit_location: HitLocation,
    rolled_damage: u32,
    damage_type: DamageType,
    crit_result: Option<CritHitResult>,
    shock_enabled: bool,
) -> Result<(GameState, InjuryOutcome), String>
```

Returns `InjuryOutcome`:
```rust
pub struct InjuryOutcome {
    pub effective_dr: u8,
    pub penetrating_damage: u32,
    pub wounding_multiplier: f32,
    pub raw_injury: u32,
    pub hp_lost: u16,
    pub major_wound: bool,
    pub knockdown: bool,
    pub knockdown_modifier: i8,
    pub dead: bool,
}
```

## Pipeline Steps

### 1. Legality Check

Validates that the `damage_type` can target the `hit_location`:

```rust
if !hit_location.is_legal_target(damage_type) {
    return Err(format!("{:?} cannot target {:?}", damage_type, hit_location));
}
```

Illegal combinations (rejected):
- Eye + Crushing, Cutting, Burning
- Vitals + Toxic, Crushing (non-head), etc.
- Spine + Burning, Toxic, etc.
- See `HitLocation::is_legal_target()` for the full matrix

### 2. DR Calculation

`calculate_dr()` (`src/model/injury.rs:154`):

```
effective_dr = sum(armor DR at location) + inherent_dr(location)
```

| Location | Inherent DR |
|----------|-------------|
| Skull | +2 |
| Spine | +3 |
| All others | 0 |

**Crit overrides:**
- `CritHitResult::HalveDR` → `effective_dr = effective_dr / 2`
- `CritHitResult::IgnoreDR` → `effective_dr = 0`
- Handled at `src/model/injury.rs:155-165`

### 3. Damage Modification

`apply_crit_damage_modifier()` (`src/model/injury.rs:178`):

| CritHitResult | Effect |
|--------------|--------|
| `DoubleDamage` | `damage * 2` |
| `TripleDamage` | `damage * 3` |
| `MaxDamage` | `damage = dice * 6 + adds` |
| All others | No modification |

### 4. Penetrating Damage & Wounding

```
penetrating_damage = max(0, rolled_damage - effective_dr)
raw_injury = penetrating_damage * wounding_multiplier(location, damage_type)
```

`wounding_multiplier()` returns `f32` values by location × damage type:

| Location | cr | cut | imp | pi | pi- | pi+ | pi++ | tbb | tox |
|----------|----|-----|-----|----|-----|-----|------|-----|-----|
| Torso | ×1 | ×1.5 | ×2 | ×1 | ×0.5 | ×1.5 | ×2 | ×1 | ×1 |
| Skull | ×2 | ×2 | ×4 | ×4 | ×4 | ×4 | ×4 | ×4 | ×1 |
| Face | ×1.5 | ×1.5 | ×2 | ×1 | ×0.5 | ×1.5 | ×2 | ×1 | ×1 |
| Eye | — | — | ×4 | ×4 | ×4 | ×4 | ×4 | ×4 | — |
| Neck | ×1.5 | ×2 | ×2 | ×1 | ×0.5 | ×1.5 | ×2 | ×2 | ×1 |
| Vitals | ×1 | — | ×3 | ×3 | ×3 | ×3 | ×3 | ×3 | — |
| Groin | ×1 | ×1.5 | ×2 | ×1 | ×0.5 | ×1.5 | ×2 | ×1 | ×1 |
| Limbs | ×1 | ×1.5 | ×1 | ×1 | ×0.5 | ×1 | ×1 | ×1 | ×1 |
| NeckVascular | ×1.5 | ×2 | ×2 | ×1.5 | ×1 | ×2 | ×2.5 | ×1.5 | — |
| Heart | ×1 | — | ×3 | ×3 | ×3 | ×3 | ×3 | ×3 | — |

(Full table at `src/model/mod.rs:255`)

### 5. Apply Damage to HP

```
actor.hp_current -= raw_injury as i16
```

HP is clamped but death thresholds are checked separately.

### 6. Shock

Applied only if `shock_enabled == true`:

```
shock_value = min(4, raw_injury)
```

A `Modifier { label: "Shock (-N)", value: -shock_value }` is pushed to
`actor.individual_modifiers`. This applies a penalty to all rolls on the
actor's next turn.

**Pain Threshold effects** (`src/model/injury.rs:106-130`):
- **High Pain Threshold**: Shock eliminated entirely (no modifier added)
- **Low Pain Threshold**: Shock value doubled

### 7. Major Wound

A major wound occurs if `raw_injury > actor.hp_max / 2`.

On major wound → HT roll to avoid knockdown/stun:
- Base knockdown modifier: 0
- Skull: -10
- Face: -5
- Jaw + Crushing: extra -1
- Groin + Crushing + male: extra -5

On failure: `actor.flags.knocked_down = true`, `actor.flags.stunned = true`.

**Crit overrides:**
- `MajorWoundAutomatic` → triggers major wound regardless of damage amount
- `KnockdownAutomatic` → knockdown without HT roll
- `StunAutomatic` → stun without HT roll

### 8. HP Threshold Checks

`check_consciousness()` (`src/model/injury.rs:286`):

| HP Threshold | Effect |
|-------------|--------|
| `hp_current ≤ 0` | HT roll to remain conscious |
| `hp_current ≤ -hp_max` | HT-1 roll |
| `hp_current ≤ -2×hp_max` | HT-2 roll |
| `hp_current ≤ -3×hp_max` | HT-3 roll |
| `hp_current ≤ -4×hp_max` | Automatic death |
| `hp_current ≤ -5×hp_max` | Destroyed |

**Pain Threshold modifiers on consciousness rolls:**
- High Pain Threshold: +3 to HT rolls
- Low Pain Threshold: -4 to HT rolls

### 9. Limb Crippling

`check_limb_crippling()` (`src/model/injury.rs:246`):

| Location | Cripple Threshold |
|----------|------------------|
| Arm / Leg | `raw_injury > hp_max / 2` |
| Hand / Foot | `raw_injury > hp_max / 3` |

On cripple:
- Leg: `actor.leg_state.{left|right} = Crippled`
  - One leg → forced Kneeling, Move = `basic_move / 2`
  - Both legs → forced Prone, Move = 0

**Crit override:** `CrippleLimbAutomatic` → cripples regardless of damage.

### 10. Return

The function returns `(new_state, outcome)` where `new_state` is a clone of
the input state with all injury effects applied.

## Call Site

`apply_pending_injury()` (`src/systems/phase_machine.rs:588`) wraps the call:
- Called from `DefenseResolution` when defense fails
- Called from `DefenseResolution` when "Skip Defense" is clicked
- Re-rolls damage if `modal.rolled_damage` is 0
- Updates `modal.last_outcome_text` with injury details
- Sends `LogEvent` for the injury resolution

Both the attacker's `active_attack` and the defender's presence are validated
before calling `resolve_injury()`.

## Tests

`src/model/injury.rs` tests (22 total) covering:
- Basic torso crushing, DR reduction, DR absorbs all
- Skull impaling (×4 + inherent DR 2)
- Neck cutting (×2)
- Vitals impaling (×3), Vitals toxic rejected
- Eye illegal crushing rejected, Eye impaling ignores DR
- Leg crippling, leg not crippled below threshold
- Shock disabled, shock modifier applied to actor
- High Pain Threshold (no shock), Low Pain Threshold (double shock)
- Major wound knockdown
- Death check, consciousness check
- Toxic always ×1, Corrosive face ×1.5
- Crit double damage, crit ignore DR
- Skull inherent DR
