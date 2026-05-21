# Data Model

All core data types live in `src/model/mod.rs` (1577 lines). Every struct derives
`Serialize`, `Deserialize`, `Clone`, `Debug`, and `PartialEq` where applicable.

## GameState

`src/model/mod.rs:742`

```rust
pub struct GameState {
    pub actors: HashMap<ActorId, Actor>,
    pub relations: Vec<ManeuverRelation>,
    pub turn_order: Vec<ActorId>,
    pub current_actor: ActorId,
    pub current_phase: TurnPhase,
    pub global_modifiers: Vec<Modifier>,
    pub round: u32,
    pub attacks_remaining: u8,      // per-turn extra attack tracking
}
```

`GameState` is the complete snapshot of the game world at a point in time.
It is never mutated in place — every change produces a new `GameState` pushed
onto `GameStateHistory`.

## GameStateHistory

`src/model/mod.rs:758`

```rust
pub struct GameStateHistory {
    pub snapshots: Vec<GameState>,
    pub current: usize,
}
```

Event sourcing container. `push()` appends a new snapshot (truncating any future
ones), `rewind()` decrements the index, `current()` returns the active snapshot.

See [ARCHITECTURE.md](ARCHITECTURE.md#event-sourcing) for the full design
rationale.

## Actor

`src/model/mod.rs:625`

| Field | Type | Description |
|-------|------|-------------|
| `id` | `ActorId` (u64) | Unique identifier |
| `name` | `String` | Display name |
| `portrait_path` | `Option<String>` | Original GCS portrait path |
| `portrait_data` | `Option<Vec<u8>>` | Decoded base64 portrait bytes |
| `source_path` | `Option<String>` | Path to source `.gcs` file for reload |
| `is_npc` | `bool` | NPC flag |
| `st, dx, iq, ht` | `u8` | Primary attributes |
| `hp_max, fp_max` | `i16` | Maximum HP/FP |
| `basic_speed` | `f32` | Basic Speed |
| `basic_move` | `u8` | Basic Move |
| `will, per` | `u8` | Secondary characteristics |
| `attacks` | `Vec<Attack>` | Weapon attacks (melee + ranged) |
| `skills` | `Vec<Skill>` | Trained skills |
| `armor` | `Vec<ArmorPiece>` | Equipped armor (DR by location) |
| `sm` | `i8` | Size Modifier |
| `is_male` | `bool` | Affects groin knockdown penalty |
| `position` | `(i32, i32)` | Hex grid coordinates |
| `hp_current, fp_current` | `i16` | Current HP/FP |
| `posture` | `Posture` | Standing, Kneeling, Prone, etc. |
| `encumbrance` | `Encumbrance` | None through ExtraHeavy |
| `flags` | `StatusFlags` | Stunned, knocked down, unconscious, dead |
| `leg_state` | `LegState` | Left/right limb crippled status |
| `individual_modifiers` | `Vec<Modifier>` | Per-actor modifiers |
| `pain_threshold` | `PainThreshold` | Normal/High/Low (GM toggle) |
| `turns_per_round` | `u8` | Extra turns (ATR, boss turns) |
| `attacks_per_turn` | `u8` | Extra attacks |
| `enhanced_time_sense` | `bool` | ETS initiative priority |
| `current_maneuver` | `Option<ManeuverType>` | Last declared maneuver |
| `active_attack` | `Option<usize>` | Index into `attacks` for current turn |
| `extra_effort` | `Vec<ExtraEffort>` | Active extra efforts |

### Actor Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `dodge()` | `u8` | Basic Speed + 3 floor, minus encumbrance |
| `effective_move()` | `u8` | Move × encumbrance multiplier, 0 if both legs crippled |
| `is_dead()` | `bool` | Dead flag OR HP ≤ -4×max |
| `is_unconscious()` | `bool` | Unconscious flag OR dead |
| `one_leg_crippled()` | `bool` | Left OR right crippled |
| `both_legs_crippled()` | `bool` | Both legs crippled |

## TurnPhase

`src/model/mod.rs:609`

```
ManeuverSelection   ← User picks maneuver card
       │
       ├── offensive → AttackSetup → ManeuverConfirmed → AttackRoll
       │                                                    │
       │                                              hit → DefenseResolution
       │                                                    │
       │                                              fail → InjuryResolution
       │                                                    │
       └── non-offensive → NonCombatResolution ────────────┘
                                                             │
                                                        Complete
```

See [PHASE_MACHINE.md](PHASE_MACHINE.md) for the full state transition logic.

## ManeuverType

`src/model/mod.rs:498` — 28 variants:

| Category | Variants |
|----------|----------|
| Offensive (defended) | Attack, Feint, FeignBeat, FeignDefensive, FeignRuse, Evaluate |
| Offensive (undefended) | AllOutAttackDetermined, AOA Double/Feint/Long/Strong/RangedDetermined, CommittedAttackDetermined/Strong |
| Setup / positional | Aim, Move, MoveAndAttack, ChangePosture |
| Defensive | AllOutDefenseIncreased/Double/Mental, DefensiveAttack, Ready |
| Mental / wait | Concentrate, AllOutConcentrate, DoNothing, Wait |

## ManeuverRelation

`src/model/mod.rs:566`

```rust
pub struct ManeuverRelation {
    pub source: ActorId,        // attacker / acting actor
    pub target: ActorId,        // defender / target
    pub maneuver: ManeuverType,
    pub state: RelationState,   // Active or Triggered
    pub payload: ManeuverPayload,
}
```

Arrows are rendered between source and target tokens on the battlemap, colored
by maneuver category. Accumulating relations (Aim, Evaluate) persist across
turns; transient ones are cleared on Complete.

## HitLocation

`src/model/mod.rs:57` — 27 variants covering the full extended humanoid table:

| Region | Locations |
|--------|-----------|
| Head | Skull, Face, Eye, Ear, Nose, Jaw |
| Neck | Neck, NeckVascular |
| Torso | Torso, Vitals, Abdomen, Pelvis, Spine, DigestiveTract, Heart, Groin |
| Arms | RightArm, LeftArm, RightHand, LeftHand |
| Legs | RightLeg, LeftLeg, RightFoot, LeftFoot |
| Vascular | LimbVascular |
| Joints | ArmLegJoint, HandFootJoint |

Key methods:
- `to_hit_penalty()` → `i8` (0 for Torso, -3 for Groin, -7 for Skull, -9 for Eye, etc.)
- `inherent_dr()` → `u8` (Skull +2, Spine +3)
- `is_legal_target(damage_type)` → `bool` (e.g., Eye rejects Crushing/Cutting)
- `wounding_multiplier(damage_type)` → `f32` (location × damage type matrix)
- `from_random_roll(u8)` → `(Self, i8)` (B552 3d6 table)
- `iter_all()` → iterator over all 28 locations

## DamageType

`src/model/mod.rs:359` — 12 variants:

Crushing, Cutting, Impaling, Piercing, SmallPiercing, LargePiercing,
HugePiercing, Burning, Toxic, Corrosive, FatigueDmg, TightBeamBurning

## Attack

`src/model/mod.rs:472`

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Display name (e.g., "Broadsword (Swing)") |
| `skill_level` | `u8` | Effective skill |
| `damage_dice` | `u8` | Number of d6 |
| `damage_adds` | `i8` | Flat bonus/penalty |
| `damage_type` | `DamageType` | e.g., Cutting, Impaling |
| `reach` | `Vec<u8>` | Reach in hexes (e.g., [1, 2]) |
| `parry_bonus` | `Option<i8>` | None = unparryable |
| `block_bonus` | `Option<i8>` | Shield block bonus |
| `is_ranged` | `bool` | Auto-detected from acc/rof/rcl presence |
| `acc` | `Option<u8>` | Accuracy (ranged) |
| `rof` | `Option<u8>` | Rate of fire (ranged) |
| `rcl` | `Option<u8>` | Recoil (ranged) |

## CritHitResult

`src/model/mod.rs:11` — 11 variants:

NormalDamage, DoubleDamage, TripleDamage, MaxDamage, HalveDR, IgnoreDR,
MajorWoundAutomatic, StunAutomatic, CrippleLimbAutomatic, KnockdownAutomatic

Generated by `crit_hit_table()` (`src/model/rolls.rs:99`), consumed by
`resolve_injury()` (`src/model/injury.rs:58`).

## Modifier

`src/model/mod.rs:589`

```rust
pub struct Modifier {
    pub label: String,         // e.g., "Lighting: Dim (-2)"
    pub value: i8,
    pub applies_to: ModifierTarget,
}
```

`ModifierTarget` (`src/model/mod.rs:596`): AllRolls, AttackRolls, DefenseRolls,
DamageRolls, SpecificActor(ActorId).

Global modifiers apply to all actors; individual modifiers apply to one actor.

## AppSettings

`src/model/mod.rs:911`

```rust
pub struct AppSettings {
    pub shock_enabled: bool,         // global shock toggle
    pub theme: String,               // theme identifier
    pub event_log_height: f32,       // configurable panel height
    pub maneuver_tray_height: f32,   // configurable panel height
}
```

Stored in a separate `fantactical_settings.json` file (not part of
`GameState`). Loaded on startup, saved on changes.

## Theme System

`src/model/mod.rs:926` — `ThemeColors`, `ThemeTypography`, `Theme` trait.

```rust
pub trait Theme {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn colors(&self) -> ThemeColors;
    fn typography(&self) -> ThemeTypography;
}
```

Default theme: `MilSimTheme` (`src/model/mod.rs:971`) — dark tactical aesthetic.
Theme switching infrastructure exists (`register_themes()` in
`src/settings.rs:17`) but runtime switching is not yet wired.
