# AGENTS.md — Fantactical

**Fantactical** is a GURPS 4e combat tracker and virtual tabletop.

This document is the canonical reference for all agents working on this codebase.
Read it in full before writing any code. Do not begin rendering or UI work until
the data model crate has been reviewed and signed off.

---

## Project Overview

A desktop GURPS 4e combat tracker with integrated VTT, full injury resolution,
network sync, and GCS sheet import. Built in Rust using Bevy (ECS, rendering,
input) and bevy_egui (all UI panels). The application is a single binary; a
lightweight authoritative server handles network sync, clients connect via
WebSocket. Crate name: `fantactical`.

**Out of scope (do not implement, do not reference):**
- Arm/hand crippling and inventory-by-slot tracking
- Blunt Force Trauma (BFT)
- Tip Slash, Stop Hit, Spraying Fire maneuvers
- Battlemap image import (grid only for now)
- Flexible armor / layered DR rules (sum DR per location)

---

## Tech Stack

| Concern | Crate/Tool |
|---|---|
| Game engine / ECS | `bevy` |
| UI panels | `bevy_egui` |
| Network | `tokio` + `tokio-tungstenite` (WebSocket) |
| Serialization | `serde` + `serde_json` |
| GCS sheet import | `serde_json` |
| State persistence | Event-sourced snapshot stack (see below) |

Do not use Tauri, web frameworks, or any Webkit-based rendering.
Do not use `std::sync::Mutex` for game state — use Bevy `Resource` and `Events`.

---

## Architecture Rules

### 1. Data model first
The data model crate (`src/model/`) must be complete and reviewed before any
Bevy systems or egui panels are written. All structs must derive `Serialize` and
`Deserialize`. All structs must be `Clone`.

### 2. Immutable state history (event sourcing)
`GameState` is never mutated in place. Every phase transition produces a new
`GameState` clone pushed onto `GameStateHistory`. Rewind = decrement the current
index. This supports the Luck Advantage, misclick recovery, and network sync.

```rust
pub struct GameStateHistory {
    pub snapshots: Vec<GameState>,
    pub current: usize,
}
```

Rewind: `history.current = history.current.saturating_sub(1);`
All game logic functions take `&GameState` and return `GameState`. No in-place
mutation of game state anywhere.

### 3. Bevy ECS idioms
- Use `Component` for per-entity data (token position, actor reference)
- Use `Resource` for singleton data (GameStateHistory, NetworkState)
- Use `Event` for phase transitions (AttackDeclared, DefenseResolved, etc.)
- Use `Commands` for spawning/despawning entities
- Do not call `world.get_resource_mut()` directly in systems; use `ResMut`
- Systems that read game state: `Res<GameStateHistory>`
- Systems that write game state: `ResMut<GameStateHistory>`, always push new snapshot

### 4. bevy_egui panels
- All egui rendering happens in systems scheduled in `Update`
- Use `EguiContexts` to get the egui context
- Panels are immediate-mode: they read from `GameStateHistory` and emit `Event`s
- Panels never mutate game state directly

### 5. Network authority
The server is authoritative. Clients send intent events; the server validates and
broadcasts the resulting state snapshot. Do not implement client-side prediction
for now.

---

## Data Model

### Core types

```rust
pub type ActorId = u64;

pub struct GameState {
    pub actors: HashMap<ActorId, Actor>,
    pub relations: Vec<ManeuverRelation>,
    pub turn_order: Vec<ActorId>,       // ordered by initiative, GM-draggable
    pub current_actor: ActorId,
    pub current_phase: TurnPhase,
    pub global_modifiers: Vec<Modifier>,
    pub round: u32,
}

// The event log is explicitly NOT part of GameState.
// It is a separate append-only Resource that does not get snapshotted.
// Storing it in GameState would duplicate an ever-growing log across every
// snapshot, inflating save files by an order of magnitude.
pub struct EventLog {
    pub entries: Vec<LogEntry>,
}

pub struct LogEntry {
    pub round: u32,
    pub turn: ActorId,
    pub phase: TurnPhase,
    pub message: String,
    pub kind: LogEntryKind,
}

pub enum LogEntryKind {
    ManeuverDeclared,
    RollResult,
    ModifierApplied,
    InjuryResolved,
    PhaseTransition,
    GmAction,
}

pub struct Actor {
    pub id: ActorId,
    pub name: String,
    pub portrait_path: Option<String>,
    pub is_npc: bool,

    // GCS-imported stats
    pub st: u8, pub dx: u8, pub iq: u8, pub ht: u8,
    pub hp_max: i16, pub fp_max: i16,
    pub basic_speed: f32, pub basic_move: u8,
    pub will: u8, pub per: u8,
    pub attacks: Vec<Attack>,
    pub skills: Vec<Skill>,
    pub armor: Vec<ArmorPiece>,

    // Live state
    pub hp_current: i16,
    pub fp_current: i16,
    pub posture: Posture,
    pub encumbrance: Encumbrance,
    pub flags: StatusFlags,
    pub leg_state: LegState,
    pub individual_modifiers: Vec<Modifier>,
    pub pain_threshold: PainThreshold,   // manual GM toggle, default Normal

    // Extra turns/attacks — set by GM modal, not imported from GCS
    // Handles: Altered Time Rate, Extra Attack, Compartmentalized Mind,
    //          Enhanced Time Sense (initiative), arbitrary boss turns
    pub turns_per_round: u8,    // default 1; ATR adds 1 per level
    pub attacks_per_turn: u8,   // default 1; Extra Attack adds 1 per level
    pub enhanced_time_sense: bool,  // if true, actor sorts before all non-ETS actors
                                    // regardless of Basic Speed/DX; ETS ties broken
                                    // by Basic Speed then DX as normal

    // Per-turn state (cleared at TurnPhase::Complete)
    pub current_maneuver: Option<ManeuverType>,
    pub active_attack: Option<usize>,       // index into attacks vec
    pub extra_effort: Vec<ExtraEffort>,
}

pub struct StatusFlags {
    pub stunned: bool,
    pub stun_turns: u8,         // IQ roll bonus = stun_turns, roll to recover each turn
    pub knocked_down: bool,
    pub unconscious: bool,
    pub dead: bool,
}

pub struct LegState {
    pub left: LimbStatus,
    pub right: LimbStatus,
}

pub enum LimbStatus { Healthy, Crippled }

pub enum Posture {
    Standing, Kneeling, Crouching,
    Sitting, Prone, Crawling,
}

pub enum Encumbrance { None, Light, Medium, Heavy, ExtraHeavy }
```

### Attack and armor

```rust
pub struct Attack {
    pub name: String,
    pub skill_level: u8,
    pub damage_dice: u8,
    pub damage_adds: i8,
    pub damage_type: DamageType,
    pub reach: Vec<u8>,             // e.g. [1, 2]
    pub parry_bonus: Option<i8>,    // None = unparryable
    pub block_bonus: Option<i8>,
    pub is_ranged: bool,
    pub acc: Option<u8>,            // ranged only
    pub rof: Option<u8>,            // ranged only
    pub rcl: Option<u8>,            // ranged only
}

pub enum DamageType { Crushing, Cutting, Impaling, Piercing, LargePiercing,
                      HugePiercing, Burning, Toxic, Corrosive, FatigueDmg }

pub struct ArmorPiece {
    pub name: String,
    pub dr: HashMap<HitLocation, u8>,   // DR per location, sum if multiple pieces
}
```

### Hit locations

Use the full extended humanoid table (B552) always. Do not use the basic table.

```rust
pub enum HitLocation {
    // Primary
    Torso, Skull, Face, Neck, Vitals, Groin,
    // Arms/Hands
    RightArm, LeftArm, RightHand, LeftHand,
    // Legs/Feet
    RightLeg, LeftLeg, RightFoot, LeftFoot,
    // Head details
    Eye, Ear, Nose, Jaw,
    // Torso details
    Abdomen, Pelvis, Spine, DigestiveTract, Heart,
    // Vascular
    LimbVascular,   // arm or leg vascular — record which limb separately
    NeckVascular,
    // Joints
    ArmLegJoint,    // record which limb separately
    HandFootJoint,  // record which extremity separately
}
```

**Random hit location roll (3d6, B552 humanoid table):**

| Roll | Location | To-Hit Penalty |
|------|----------|---------------|
| 3–4  | Torso    | 0             |
| 5    | Arm (random L/R, 1d: 1-3=R 4-6=L) | -2 |
| 6    | Arm (random) | -2          |
| 7    | Torso    | 0             |
| 8    | Torso    | 0             |
| 9    | Torso    | 0             |
| 10   | Torso    | 0             |
| 11   | Groin    | -3            |
| 12   | Arm (random) | -2          |
| 13   | Arm (random) | -2          |
| 14   | Leg (random L/R, 1d: 1-3=R 4-6=L) | -2 |
| 15   | Leg (random) | -2           |
| 16   | Hand (random) | -4          |
| 17   | Foot (random) | -4          |
| 18   | Neck     | -5            |

Specific targeted locations and their to-hit penalties:
- Skull: -7 front / -5 back
- Face: -5 front / -7 back
- Eye: -9
- Ear: -7
- Nose: -7
- Jaw: -6
- Neck: -5
- Vitals: -3
- Spine: -8 (back only)
- Abdomen: -1
- Groin: -3
- Pelvis: -3
- Digestive Tract: -2
- Heart: -5
- Limb Vascular: -5
- Neck Vascular: -8
- Arm/Leg Joint: -5
- Hand/Foot Joint: -7

### Maneuver relations

```rust
pub struct ManeuverRelation {
    pub source: ActorId,
    pub target: ActorId,        // Wait: target is declared, arrow renders immediately
    pub maneuver: ManeuverType,
    pub state: RelationState,
    pub payload: ManeuverPayload,
}

pub enum RelationState { Active, Triggered }

pub enum ManeuverPayload {
    None,
    AimBonus { accumulated: u8, turns: u8, weapon_acc: u8 },
    EvaluateBonus { accumulated: u8 },      // max 3
    FeintMargin { margin: i8 },             // set after roll
    AoADetermined { to_hit_bonus: u8 },
    AoAStrong { damage_bonus: u8 },
    AoADouble,
    AoAFeint,                               // feint + attack same turn
    AoALong,
    CommittedDetermined,
    CommittedStrong,
    DefensiveAttack,
    MoveAndAttack,
}
```

### Maneuver types

```rust
pub enum ManeuverType {
    // Self-targeted (no arrow, badge only)
    AllOutDefenseIncreased,
    AllOutDefenseDouble,
    AllOutDefenseMental,
    DoNothing,
    Concentrate,
    AllOutConcentrate,
    Ready,
    ChangePosture,
    Move,

    // Directed (arrow renders)
    Attack,
    AllOutAttackDetermined,
    AllOutAttackDouble,
    AllOutAttackFeint,
    AllOutAttackLong,
    AllOutAttackStrong,
    AllOutAttackRangedDetermined,
    CommittedAttackDetermined,
    CommittedAttackStrong,
    DefensiveAttack,
    MoveAndAttack,
    Feint,
    FeignBeat,
    FeignDefensive,
    FeignRuse,
    Evaluate,
    Aim,
    Wait,                   // declared with target; player dismisses on trigger
}
```

### Modifiers

```rust
pub struct Modifier {
    pub label: String,
    pub value: i8,
    pub applies_to: ModifierTarget,
}

pub enum ModifierTarget {
    AllRolls,
    AttackRolls,
    DefenseRolls,
    DamageRolls,
    SpecificActor(ActorId),
}
```

Common preset modifier labels (GM can add arbitrary ones):
- "Lighting: Dim (-2)", "Lighting: Dark (-4)", "Lighting: Blind (-10)"
- "Shock (-N)" — auto-applied per injury
- "Posture penalty" — auto-applied from posture
- "Range penalty" — auto-calculated
- "SM bonus/penalty" — auto-calculated from target SM

---

## Maneuver Legality

`available_maneuvers(actor: &Actor) -> Vec<ManeuverType>` is a pure function.
It is called every time the maneuver tray renders. Never cache it.

```
Priority order (each level overrides everything below):
1. Dead → empty set
2. Unconscious → empty set
3. Stunned → [DoNothing] only; stun_turns increments each turn for IQ recovery roll
4. KnockedDown → [DoNothing, ChangePosture]
5. CrippledLeg (one) → forced Kneeling/Crawling posture, Move halved; apply posture filter
6. CrippledLeg (both) → forced Prone; apply posture filter
7. Posture filter → see posture blacklists below
8. Encumbrance filter → ExtraHeavy removes Move and Attack
9. Full set available
```

Posture blacklists (maneuvers unavailable in that posture):
- Prone: AllOutAttack variants (melee), MoveAndAttack, Move (full)
- Kneeling: MoveAndAttack
- Crawling: most attack maneuvers except ranged at penalty
- (implement per B364)

Stun recovery: each turn actor is stunned, roll IQ + stun_turns ≤ IQ to recover.
On success: stunned = false, stun_turns = 0.

### Combo whitelist
Each maneuver declares what may stack with it. Not on the whitelist = illegal.
ExtraEffort options are overlays on a base maneuver, not standalone cards.

```
Aim → [ExtraEffort variants are not compatible with Aim]
AllOutAttack* → [] (nothing stacks with AOA)
Wait → [] (stands alone)
Attack → [ExtraEffort::MightyBlow, ExtraEffort::RapidStrike (separate rule)]
DefensiveAttack → [] 
Feint variants → []
Evaluate → []
(etc. — implement per Basic Set)
```

---

## Turn Phase State Machine

```
TurnPhase::ManeuverSelection
  - Compute available_maneuvers(actor) → render tray
  - Player drags card(s) onto own token or target token
  - Validate combo whitelist
  - If non-offensive → TurnPhase::NonCombatResolution
  - If offensive → TurnPhase::AttackSetup

TurnPhase::AttackSetup
  - Player selects Attack from actor.attacks list (no Ready check)
  - Player selects HitLocation or system rolls random (B552 table)
  - Auto-calculate and display full modifier breakdown:
      base skill
      + maneuver bonus (Aim accumulated, Evaluate accumulated, AOA Determined, etc.)
      + global modifiers
      + individual modifiers for this actor
      - range penalty (auto, based on token distance × yard scale)
      - SM modifier (auto, target SM)
      - posture penalties (attacker and target)
      - shock penalty if applicable
  - Present total to player
  - TurnPhase::ManeuverConfirmed  ← player reviews math, decides to commit

TurnPhase::ManeuverConfirmed
  - Locked in
  - Push ManeuverRelation to GameState.relations
  - TurnPhase::AttackRoll

TurnPhase::AttackRoll
  - Roll 3d6
  - Compare to modified skill
  - Check critical: ≤4 always crit hit; ≤ skill/2 (min 5) crit hit;
    =17 crit miss if skill<16; =18 always crit miss; =3/4 always crit hit
  - Roll on B556 crit hit table / B557 crit miss table
  - Miss / crit miss → TurnPhase::Complete
  - Hit / crit hit → TurnPhase::DefenseResolution

TurnPhase::DefenseResolution
  - Pull target's active defenses (Dodge, Parry per weapon, Block if shield)
  - Apply target maneuver modifiers:
      AOA any variant → no active defense available
      AOD Increased → +2 to one defense, up to half Move as step
      AOD Double → two defenses vs this one attack (phase loops once)
      AOD Mental → +2 to all resistance rolls (non-combat, skip)
      Defensive Attack → +1 to defense
  - Apply posture penalties to defense
  - Apply shock penalty to defense
  - Sort options best→worst, present to GM
  - GM selects defense (or accepts auto-best for NPC)
  - Roll 3d6
  - Critical defense check (≤8 on Dodge is critical per MA)
  - Success → TurnPhase::Complete
  - Failure → TurnPhase::InjuryResolution

TurnPhase::InjuryResolution
  - Pull DR for hit location (sum all armor pieces at that location)
  - Validate attack legality for location (see Hit Location × Damage Type Matrix)
  - Apply damage type × location wounding multiplier (see full matrix below)
  - Calculate: raw_injury = max(0, rolled_damage - dr) × wounding_multiplier
  - Apply Pain Threshold modifier to threshold rolls:
      High Pain Threshold → shock penalty eliminated; +3 to HT rolls
      Low Pain Threshold  → double shock penalty; -4 to HT rolls
      Controlled by per-actor GM toggle, not sheet import
  - Apply Shock: -min(4, raw_injury) to all rolls next turn (auto Modifier)
      Skip entirely if global setting shock_enabled == false
  - Apply to actor.hp_current
  - Check Major Wound: raw_injury > actor.hp_max / 2
      → HT roll (modified by pain threshold) or Knockdown + Stunned
      → Jaw hit: extra -1 to knockdown roll
      → Groin hit (male): -5 to knockdown roll on cr damage
  - Check HP thresholds:
      hp_current ≤ 0        → consciousness roll HT each turn
      hp_current ≤ -hp_max  → consciousness roll HT-1
      hp_current ≤ -2×hp_max → consciousness roll HT-2
      hp_current ≤ -3×hp_max → consciousness roll HT-3
      hp_current ≤ -4×hp_max → Dead (no roll)
      hp_current ≤ -5×hp_max → Destroyed
  - Check leg crippling:
      Damage to leg > hp_max/2 → LimbStatus::Crippled
      One leg crippled → force posture to Kneeling, Move = floor(basic_move/2)
      Both legs crippled → force posture to Prone, Move = 0
  - Check crit hit special results if applicable (from crit table roll)
  - Update actor.flags and actor.leg_state
  - TurnPhase::Complete

TurnPhase::NonCombatResolution
  - Update ManeuverRelations (Aim accumulates, Evaluate accumulates up to 3)
  - Clear transient relations for self-targeted completed maneuvers
  - TurnPhase::Complete

TurnPhase::Complete
  - Advance turn_order index
  - Clear transient ManeuverRelations (non-accumulating)
  - Persist accumulating relations (Aim, Evaluate)
  - Increment actor.flags.stun_turns if stunned, roll IQ recovery
  - Push snapshot to GameStateHistory
  - Next actor → TurnPhase::ManeuverSelection
```

---

## Hit Location × Damage Type Matrix

### Targeting restrictions (illegal — UI must prevent these drops)

- **Eye**: only `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`; only from front or sides (not rear arc)
- **Vitals**: only `cut`, `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`
- **Spine**: rear arc only; only `cut`, `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`
- **Neck Vascular**: only `cut`, `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`
- **Limb Vascular**: only `cut`, `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`
- **Heart**: only `cut`, `imp`, `pi`, `pi+`, `pi++`, `pi-`, `tbb`
- `tox`: wounding multiplier always ×1, no location bonus ever applies
- `cor`: ×1 everywhere except Face (×1.5)

### Wounding multipliers and special rules per location

**Skull**
- Inherent DR +2 (stacks with armor DR)
- All damage ×2 after DR
- `imp`, all `pi` variants: ×4 after DR
- `tox`: ×1
- Knockdown roll at -10
- Critical hits and major wounds: use Critical Head Blow table (B556)

**Face**
- `cr`: ×1.5; `cor`: ×1.5
- All others: ×1
- Knockdown penalty: -5
- Critical hits: use Critical Head Blow table
- Major wound: blinds one eye; both eyes if dmg > full HP
- Miss by 1 on a skull attack: treat as face hit

**Eye**
- Legal types only: `imp`, `pi` variants, `tbb`; front/side arcs only
- All legal types: ×4, ignores DR entirely
- Dmg > HP/10: blinds the eye permanently
- Otherwise: treat as Skull but without extra DR

**Ear**
- Standard wounding ×1
- `cut` at damage ≥ HP/4: removes ear, -1 Appearance

**Nose**
- Standard wounding ×1
- Damage ≥ HP/4: breaks nose (major wound effect; no Sense of Smell/Taste until healed)
- `cut` at damage ≥ HP/4×2 as major wound: lops off nose, -2 Appearance
  (note: knockdown not at -5 for cut-nose major wound)

**Jaw**
- Standard wounding ×1
- `cr`: extra -1 to knockdown roll
- Major wound at HP/4: doubles and may remove ear

**Neck**
- `cut`: ×2; `imp`: ×2; `cr`: ×1.5
- `tbb`: add ×0.5 on top of applicable base multiplier
- Miss by 1: attacker hits Torso instead
- On 1d roll of 1 (`cr`, `imp`, `pi`, `tbb`): hit Neck Vascular [17]
- `cut` from behind at major wound: decapitation (GM adjudicates)

**Neck Vascular**
- All legal types: base multiplier +0.5 (stacks with neck multipliers)
- No crippling damage limit
- Miss by 1: hits neck, arm, or leg as appropriate

**Torso**
- All types: ×1 (standard)
- `cr`, `imp`, `pi`, `tbb` on 1d roll of 1: hit Vitals

**Vitals**
- `imp`, all `pi` variants, `tbb`: ×3
- `cr`: ×1; if major wound, shock-penalty roll vs knockdown at -5
- All other types: ×1, no special rules

**Abdomen**
- Standard wounding ×1
- On any hit roll 1d for sub-location: 1=Vitals, 2-4=Digestive Tract, 5=Pelvis, 6=Groin

**Groin**
- `imp`: ×2
- Male targets + `cr`: double shock value; -5 to knockdown rolls
- Otherwise: standard ×1

**Pelvis**
- Apply Groin rules
- Major wound: roll 1d for sub-location result

**Digestive Tract**
- Standard wounding ×1
- Major wound: HT-3 roll or special infection (B444) — flag for GM

**Heart**
- `cut`, `imp`, `pi` variants, `tbb`: ×3
- Major wound: HT-3 roll or special infection — flag for GM

**Spine**
- Rear arc only; inherent DR 3
- Legal types only: `cut`, `imp`, `pi` variants, `tbb`
- Shock-penalty roll vs knockdown on any hit
- Damage = HP: automatic knockdown and stun
- Damage ≥ HP (crippling threshold): flag for GM adjudication (paralysis)

**Arms / Legs**
- `large pi`, `huge pi`, `imp`: reduce wounding multiplier to ×1
- Major wound (single hit > HP/2): cripples limb
  - Leg: see LegState rules
  - Arm: out of scope, flag for GM
- `cr`, `imp`, `pi`, `tbb` on 1d roll of 1: hit Limb Vascular [17]

**Limb Vascular**
- Legal types only: `cut`, `imp`, `pi` variants, `tbb`
- Wounding multiplier: base +0.5
- No crippling damage limit
- Miss by 1: hits the limb or extremity, not the joint

**Hands / Feet**
- Cripple threshold: single hit > HP/3
- `cr`, `imp`, `pi`, `tbb` on 1d roll of 1: hit joint [14]
- Treat as limb for wounding otherwise

**Arm/Leg Joint**
- Cripple threshold: HP/3
- HT roll to recover from crippling at -2
- `cr`, `imp`, `pi`, `tbb` only

**Hand/Foot Joint**
- Same rules as Arm/Leg Joint

---

## Settings

Settings are stored in `AppSettings` (a separate `Resource`, not part of `GameState`).
Changes to settings do not push a `GameState` snapshot.

```rust
pub struct AppSettings {
    pub shock_enabled: bool,        // default: true; global toggle
    pub theme: String,              // theme identifier, default: "mil-sim"
}
```

A Settings panel will be added as a tab when there are enough settings to warrant it.
For now, shock_enabled is accessible from the GM Config tab as a checkbox.

---

## Pain Threshold (per-actor, manual GM toggle)

```rust
pub enum PainThreshold { Normal, High, Low }
```

Stored on `Actor`. Default: `Normal`. Set manually by GM — do not infer from GCS sheet data.

Effects:
- `High`: shock penalty eliminated entirely; +3 to all HT-based knockdown/stun rolls
- `Low`: shock penalty doubled; -4 to all HT-based knockdown/stun rolls
- `Normal`: no modification

GM sets this in the actor config panel at session start or any time.

---

## Theming

Visual themes are modular. Each theme is a struct implementing the `Theme` trait,
registered at startup. New themes can be added by implementing the trait and
registering — no other code changes required.

```rust
pub trait Theme {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn colors(&self) -> ThemeColors;
    fn typography(&self) -> ThemeTypography;
}

pub struct ThemeColors {
    pub background: Color,
    pub panel_surface: Color,
    pub panel_border: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub accent: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
    // Maneuver category colors
    pub maneuver_offensive_defended: Color,
    pub maneuver_offensive_undefended: Color,
    pub maneuver_setup: Color,
    pub maneuver_defensive: Color,
    pub maneuver_mental: Color,
    // HP/FP bar thresholds
    pub bar_healthy: Color,
    pub bar_low: Color,
    pub bar_critical: Color,
    pub bar_dead: Color,
    // Encumbrance tints
    pub enc_none: Color,
    pub enc_light: Color,
    pub enc_medium: Color,
    pub enc_heavy: Color,
    pub enc_extra: Color,
}

pub struct ThemeTypography {
    pub body_font: &'static str,
    pub mono_font: &'static str,
    pub panel_corner_radius: f32,   // 0.0 = hard edges (mil-sim default)
}
```

Default theme: `"mil-sim"` (values as specified in Visual Design Language section).
Active theme stored in `AppSettings::theme`. Theme switching redraws all egui panels.

---

## Turn Order

Initialized by sorting `actors` with the following priority:

1. `enhanced_time_sense == true` actors first (among themselves, sort by Basic Speed desc, then DX desc)
2. Remaining actors by Basic Speed desc
3. Ties broken by DX desc
4. Further ties by GM drag

The character panel renders actors as draggable compact stat blocks in turn order.
GM can drag to reorder at any time; this pushes a new GameState snapshot.

**Altered Time Rate / Extra Turns:**
Actors with `turns_per_round > 1` appear in the turn order multiple times per
round — once per turn. The turn order list shows repeated entries for that actor
with a turn index badge (e.g. "SOVEREIGN (1/3)", "SOVEREIGN (2/3)").
Turn slots are inserted at equal spacing through the round order (so a 3-turn
actor in a 6-actor round takes slots 1, 3, 5 by default; GM can drag to adjust).

**Extra Attacks:**
Actors with `attacks_per_turn > 1` may make that many attack declarations within
a single turn. The attack phase loops `attacks_per_turn` times before advancing
to the next actor. Each attack goes through its own full
AttackSetup → ManeuverConfirmed → AttackRoll → DefenseResolution →
InjuryResolution cycle.

**GM modal for extra turns/attacks:**
A per-actor config modal (accessible from the character panel) exposes:
- `turns_per_round` spinner (default 1, min 1, no hard max)
- `attacks_per_turn` spinner (default 1, min 1, no hard max)
- `enhanced_time_sense` checkbox
- `pain_threshold` selector (Normal / High / Low)

The GM is responsible for adjudicating what extra turns/attacks may legally be
used for (e.g. Compartmentalized Mind restrictions). The software enforces no
restrictions beyond making the resources available.

---

## VTT / Battlemap

- Grid only. No image import.
- Each cell = 1 yard. Scale is configurable (pixels per yard).
- Tokens are circles with portrait texture (or colored circle fallback).
- Token position stored as `(i32, i32)` grid coordinates on `Actor`.
- Range between two tokens = Chebyshev distance (GURPS uses facing-free range).

### Maneuver relation arrows
Rendered as directional arrows from source token to target token.
Arrow color by maneuver category:

| Category | Color | Maneuvers |
|---|---|---|
| Offensive-defended | `#CC3333` red | Attack, Feint variants, Evaluate, Telegraphic |
| Offensive-undefended | `#FF6600` orange | AOA all variants, Committed Attack, Rapid Strike |
| Setup/positional | `#CCAA00` yellow | Aim, Move, Move and Attack, Change Posture |
| Defensive | `#336699` blue | AOD all variants, Defensive Attack, Ready |
| Mental/wait | `#7755AA` purple | Concentrate, Do Nothing, Wait |

Self-targeted maneuvers (AOD, DoNothing, etc.) render as a colored ring on the
token, no arrow.

Arrow label: show payload value where relevant (Aim: "+2", Evaluate: "+1").
Wait arrows render as dashed line until dismissed by player.

### Range ruler
Drag between two tokens to display distance in yards and the GURPS range penalty.

Range penalty is looked up from a static hashmap keyed on the B550 range/speed
table. Do not attempt to calculate it algorithmically — the progression
(1, 2, 3, 5, 7, 10, 15, 20, 30, 50, 70, 100, ...) is a hand-tuned stepped
approximation, not a clean log curve. Round the actual distance **up** to the
next entry in the table.

```rust
// B550 Range/Speed table — penalty by range band upper bound (yards)
// Key = upper bound of band, Value = penalty
static RANGE_PENALTIES: &[(u32, i8)] = &[
    (1,    0),
    (2,   -1),
    (3,   -2),
    (5,   -3),
    (7,   -4),
    (10,  -5),
    (15,  -6),
    (20,  -7),
    (30,  -8),
    (50,  -9),
    (70,  -10),
    (100, -11),
    (150, -12),
    (200, -13),
    (300, -14),
    (500, -15),
    (700, -16),
    (1000,-17),
    // extend as needed following the 1-2-3-5-7 pattern × powers of 10
];

pub fn range_penalty(yards: f32) -> i8 {
    let ceil = yards.ceil() as u32;
    RANGE_PENALTIES
        .iter()
        .find(|(bound, _)| *bound >= ceil)
        .map(|(_, pen)| *pen)
        .unwrap_or(-18) // beyond table = GM adjudicates; use -18 as floor
}
```

---

## GCS Import

Parse `.gcs` JSON files (`serde_json`). Map to `Actor` struct.
Fields to extract: name, ST, DX, IQ, HT, HP, FP, Basic Speed, Basic Move,
Will, Per, SM, attacks (name, skill, damage, type, reach, parry, ranged stats),
skills (name, level), equipment with DR by location.
Weapons are extracted from both `traits[*].weapons` (natural attacks) and
`equipment[*].weapons` (equipped items), including recursive container children.

On import, portrait image path is set if GCS file references one.

---

## UI Layout

```
┌─────────────────────────────┬──────────────────┐
│                             │                  │
│         Battlemap           │  Character Panel │
│         (Bevy viewport)     │  (scrollable)    │
│                             │  turn-order list │
│                             │                  │
├─────────────────────────────┼──────────────────┤
│                             │  [tabs]          │
│    Event Log / Console      │  Attacks │       │
│    (scrollable, timestamped)│  GM Config       │
│                             │                  │
├─────────────────────────────┴──────────────────┤
│              Maneuver Card Tray                 │
│   (full width, renders available_maneuvers())   │
└─────────────────────────────────────────────────┘
```

Roll resolution prompts render as modal overlays anchored to the relevant token
on the battlemap. They do not occupy panel space.

### Character panel — compact stat block

Each actor renders as a ~90px card:
```
┌──────────────────────────────────────┐
│ [portrait 40px] NAME        [PC/NPC] │
│ [HP bar ██████████░░░░] HP 14/18     │
│ [FP bar ████░░░░░░░░░░] FP  4/12     │
│ Spd 6.25  Move 6   Enc: [tint+label] │
│ Dodge 9   Parry 11 (Sword)           │
│ [status badges]                      │
└──────────────────────────────────────┘
```

HP/FP bar colors (dynamic):
- > 1/3 max: green `#44AA44`
- ≤ 1/3 max: yellow `#CCAA00`
- ≤ 0: red `#CC3333`
- ≤ -max: deep red `#880000`

Encumbrance label + Move value background tint:
- None: white `#FFFFFF` text
- Light: off-white `#DDDDCC` text
- Medium: yellow `#CCAA00` text
- Heavy: orange `#FF6600` text
- Extra-Heavy: red `#CC3333` text

Parry/Block: show value for currently active attack weapon. If shield equipped,
show both. Pull from `actor.active_attack` index.

Status badges: small SVG icons in portrait corner (todo: produce SVGs).
Flags to badge: Stunned, KnockedDown, Prone, CrippledLeg (one/both).

Cards are draggable to reorder turn order. Current actor highlighted with
accent border.

### Maneuver cards
Cards in the tray are colored by category (see arrow color table above).
Each card shows: maneuver name, one-line summary of key effect, color band.
Greyed-out / absent if not in `available_maneuvers()`.
Dragged onto own token = self-targeted. Dragged onto other token = directed.
Illegal drop targets are visually indicated (red highlight on token).

### Event log
Timestamped by round and turn. Records:
- Maneuver declarations
- All roll results (show dice, target number, margin)
- Modifier breakdowns
- Injury calculations
- Phase transitions
- GM modifier changes
Format: monospace font. Color-coded by event type.

### GM config tab
- Global modifier list: label + value, add/remove arbitrarily
- Per-actor modifier list for selected actor
- Preset buttons for common modifiers (lighting, etc.)
- Rewind button (with round/turn label of target state)

---

## Visual Design Language

Aesthetic: mil-sim / tactical operations. Dark theme. Feels like JRTF command
software, not a fantasy game.

```
Background:       #0D0D0D  (near black)
Panel surface:    #161616
Panel border:     #2A2A2A
Text primary:     #E0E0E0
Text secondary:   #888888
Text monospace:   JetBrains Mono or Fira Code (event log, stat numbers)
Accent:           #3A7BD5  (current actor highlight, interactive elements)
Danger:           #CC3333
Warning:          #CCAA00
Success:          #44AA44
```

No rounded corners on panels — hard edges. Subtle scanline or noise texture on
battlemap background is acceptable. Token circles may have slight drop shadow.
Maneuver cards have a 3px color band on the left edge (category color) and are
otherwise dark surface.

---

---

## Persistence

At the end of each round (`round` increments in `TurnPhase::Complete` after the
last actor in turn order), serialize the **full `GameStateHistory`** (not just
the current snapshot) to a JSON file using `serde_json`.

```rust
// Triggered automatically at end of each round
pub fn save_history(history: &GameStateHistory, path: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(history)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_history(path: &Path) -> anyhow::Result<GameStateHistory> {
    let json = std::fs::read_to_string(path)?;
    let history = serde_json::from_str(&json)?;
    Ok(history)
}
```

**Save file location:** `{session_name}_round{N}.json` in a configurable output
directory (default: current working directory). Do not overwrite previous rounds —
append the round number so every round is recoverable independently.

**On load:** restore full `GameStateHistory` including all snapshots. Rewind
works normally after loading. The GM can load a previous round's file to rewind
further back than the current session's memory.

**Event log persistence:** the `EventLog` is saved separately as
`{session_name}_log.json`, appended continuously throughout the session (not
per-round). It is never embedded in `GameStateHistory`. Loading a save file
restores game state but does not require the log file to be present.
not resolve on another machine. All other state is fully portable. This is a
known limitation, not a bug — do not attempt to embed portrait image data in the
save file.

**Server behaviour:** the server triggers the save. Clients do not save
independently. On reconnect, the server rebroadcasts the current
`GameStateHistory` so all clients are back in sync.

Server is authoritative. One player hosts (or a dedicated binary).
Clients connect via WebSocket. All messages are JSON-serialized.

```
Client → Server:  IntentEvent (maneuver declared, defense chosen, modifier added, rewind)
Server → All:     StateSnapshot (full GameState after validation)
Server → All:     RollResult (dice values, breakdown — for display only, not state)
```

State snapshots are full `GameState` serializations, not deltas (simpler, and
GURPS combat state is small enough this is fine).

Authentication: shared session token set by the GM on server start, passed by
clients on connect. No user accounts.

---

## Implementation Order

**Do not deviate from this order without explicit instruction.**

1. `src/model/` — all data structs, serialization, zero Bevy dependencies
2. `src/model/maneuver_legality.rs` — `available_maneuvers()` pure function + tests
3. `src/model/injury.rs` — injury resolution pure function + tests (full location matrix)
4. `src/model/rolls.rs` — success roll, crit check, crit tables, damage roll
5. `src/model/gcs_import.rs` — GCS JSON → Actor parser
6. `src/state/history.rs` — GameStateHistory, push/rewind
7. `src/settings.rs` — AppSettings resource, Theme trait, mil-sim default theme
8. `src/server/` — WebSocket server, session auth, broadcast
9. `src/client/network.rs` — WebSocket client, reconnect logic
10. `src/ui/battlemap.rs` — Bevy viewport, grid, token rendering, arrows
11. `src/ui/character_panel.rs` — compact stat blocks, drag to reorder
12. `src/ui/card_tray.rs` — maneuver cards, drag-drop onto tokens
13. `src/ui/event_log.rs` — console panel
14. `src/ui/gm_panel.rs` — modifier config, rewind, shock toggle, pain threshold toggles
15. `src/ui/roll_modal.rs` — overlay prompts anchored to tokens
16. Integration: wire all events through state machine phase transitions

---

## Tests Required

Every pure function in `src/model/` must have unit tests before UI work begins.
Priority test cases:
- `available_maneuvers`: stunned actor, prone actor, both legs crippled
- Injury pipeline: skull impaling hit, leg crippling threshold, death threshold
- Injury pipeline: eye hit with illegal damage type (must reject), vitals with tox (×1 only)
- Injury pipeline: HPT actor — verify shock halved; LPT actor — verify shock doubled
- Crit table: roll=3 (always crit), roll=17 at skill 15 vs skill 16
- GCS import: round-trip a sample sheet, verify all fields populated
- Stun recovery: IQ accumulator increments correctly
- Rewind: push 3 snapshots, rewind 2, verify correct state restored
- Shock toggle: global shock_enabled=false, verify no shock modifier applied
- Theme: register a second theme, switch to it, verify ThemeColors values change
