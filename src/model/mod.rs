use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod gcs_import;
pub mod injury;
pub mod maneuver_legality;
pub mod rolls;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CritHitResult {
    NormalDamage,
    DoubleDamage,
    TripleDamage,
    MaxDamage,
    HalveDR,
    IgnoreDR,
    MajorWoundAutomatic,
    StunAutomatic,
    CrippleLimbAutomatic,
    KnockdownAutomatic,
}

impl CritHitResult {
    pub fn modifies_damage(&self) -> Option<u32> {
        match self {
            Self::DoubleDamage => Some(2),
            Self::TripleDamage => Some(3),
            Self::MaxDamage => None,
            _ => Some(1),
        }
    }

    pub fn halve_dr(&self) -> bool {
        matches!(self, Self::HalveDR)
    }

    pub fn ignore_dr(&self) -> bool {
        matches!(self, Self::IgnoreDR)
    }
}

pub type ActorId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Srgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

// ---------------------------------------------------------------------------
// Hit Locations — full extended humanoid table (B552)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HitLocation {
    #[default]
    Torso,
    Skull,
    Face,
    Neck,
    Vitals,
    Groin,
    RightArm,
    LeftArm,
    RightHand,
    LeftHand,
    RightLeg,
    LeftLeg,
    RightFoot,
    LeftFoot,
    Eye,
    Ear,
    Nose,
    Jaw,
    Abdomen,
    Pelvis,
    Spine,
    DigestiveTract,
    Heart,
    LimbVascular,
    NeckVascular,
    ArmLegJoint,
    HandFootJoint,
}

impl HitLocation {
    pub fn to_hit_penalty(&self) -> i8 {
        match self {
            Self::Torso => 0,
            Self::Skull => -7,
            Self::Face => -5,
            Self::Neck => -5,
            Self::Vitals => -3,
            Self::Groin => -3,
            Self::RightArm | Self::LeftArm => -2,
            Self::RightLeg | Self::LeftLeg => -2,
            Self::RightHand | Self::LeftHand => -4,
            Self::RightFoot | Self::LeftFoot => -4,
            Self::Eye => -9,
            Self::Ear => -7,
            Self::Nose => -7,
            Self::Jaw => -6,
            Self::Abdomen => -1,
            Self::Pelvis => -3,
            Self::Spine => -8,
            Self::DigestiveTract => -2,
            Self::Heart => -5,
            Self::LimbVascular => -5,
            Self::NeckVascular => -8,
            Self::ArmLegJoint => -5,
            Self::HandFootJoint => -7,
        }
    }

    pub fn inherent_dr(&self) -> u8 {
        match self {
            Self::Skull => 2,
            Self::Spine => 3,
            _ => 0,
        }
    }

    pub fn is_head_location(&self) -> bool {
        matches!(
            self,
            Self::Skull | Self::Face | Self::Eye | Self::Ear | Self::Nose | Self::Jaw
        )
    }

    pub fn from_random_roll(roll: u8) -> (Self, i8) {
        match roll {
            3 | 4 => (Self::Torso, 0),
            5 | 6 => (Self::RightArm, -2),
            7..=10 => (Self::Torso, 0),
            11 => (Self::Groin, -3),
            12 | 13 => (Self::RightArm, -2),
            14 | 15 => (Self::RightLeg, -2),
            16 => (Self::RightHand, -4),
            17 => (Self::RightFoot, -4),
            18 => (Self::Neck, -5),
            _ => (Self::Torso, 0),
        }
    }

    pub fn with_random_side(self, side_roll: u8) -> Self {
        if side_roll <= 3 {
            self
        } else {
            self.flip_side()
        }
    }

    pub fn flip_side(&self) -> Self {
        match self {
            Self::RightArm => Self::LeftArm,
            Self::LeftArm => Self::RightArm,
            Self::RightHand => Self::LeftHand,
            Self::LeftHand => Self::RightHand,
            Self::RightLeg => Self::LeftLeg,
            Self::LeftLeg => Self::RightLeg,
            Self::RightFoot => Self::LeftFoot,
            Self::LeftFoot => Self::RightFoot,
            other => *other,
        }
    }

    pub fn iter_all() -> impl Iterator<Item = HitLocation> {
        [
            Self::Torso,
            Self::Skull,
            Self::Face,
            Self::Neck,
            Self::Vitals,
            Self::Groin,
            Self::RightArm,
            Self::LeftArm,
            Self::RightHand,
            Self::LeftHand,
            Self::RightLeg,
            Self::LeftLeg,
            Self::RightFoot,
            Self::LeftFoot,
            Self::Eye,
            Self::Ear,
            Self::Nose,
            Self::Jaw,
            Self::Abdomen,
            Self::Pelvis,
            Self::Spine,
            Self::DigestiveTract,
            Self::Heart,
            Self::LimbVascular,
            Self::NeckVascular,
            Self::ArmLegJoint,
            Self::HandFootJoint,
        ]
        .into_iter()
    }

    pub fn is_legal_target(&self, damage_type: DamageType) -> bool {
        match (self, damage_type) {
            (Self::Eye, DamageType::Impaling)
            | (Self::Eye, DamageType::Piercing)
            | (Self::Eye, DamageType::SmallPiercing)
            | (Self::Eye, DamageType::LargePiercing)
            | (Self::Eye, DamageType::HugePiercing)
            | (Self::Eye, DamageType::TightBeamBurning) => true,
            (Self::Eye, _) => false,

            (Self::Vitals, DamageType::Cutting)
            | (Self::Vitals, DamageType::Impaling)
            | (Self::Vitals, DamageType::Piercing)
            | (Self::Vitals, DamageType::SmallPiercing)
            | (Self::Vitals, DamageType::LargePiercing)
            | (Self::Vitals, DamageType::HugePiercing)
            | (Self::Vitals, DamageType::TightBeamBurning) => true,
            (Self::Vitals, _) => false,

            (Self::LimbVascular, DamageType::Cutting)
            | (Self::LimbVascular, DamageType::Impaling)
            | (Self::LimbVascular, DamageType::Piercing)
            | (Self::LimbVascular, DamageType::SmallPiercing)
            | (Self::LimbVascular, DamageType::LargePiercing)
            | (Self::LimbVascular, DamageType::HugePiercing)
            | (Self::LimbVascular, DamageType::TightBeamBurning) => true,
            (Self::LimbVascular, _) => false,

            (Self::NeckVascular, DamageType::Cutting)
            | (Self::NeckVascular, DamageType::Impaling)
            | (Self::NeckVascular, DamageType::Piercing)
            | (Self::NeckVascular, DamageType::SmallPiercing)
            | (Self::NeckVascular, DamageType::LargePiercing)
            | (Self::NeckVascular, DamageType::HugePiercing)
            | (Self::NeckVascular, DamageType::TightBeamBurning) => true,
            (Self::NeckVascular, _) => false,

            (Self::Heart, DamageType::Cutting)
            | (Self::Heart, DamageType::Impaling)
            | (Self::Heart, DamageType::Piercing)
            | (Self::Heart, DamageType::SmallPiercing)
            | (Self::Heart, DamageType::LargePiercing)
            | (Self::Heart, DamageType::HugePiercing)
            | (Self::Heart, DamageType::TightBeamBurning) => true,
            (Self::Heart, _) => false,

            (Self::Spine, DamageType::Cutting)
            | (Self::Spine, DamageType::Impaling)
            | (Self::Spine, DamageType::Piercing)
            | (Self::Spine, DamageType::SmallPiercing)
            | (Self::Spine, DamageType::LargePiercing)
            | (Self::Spine, DamageType::HugePiercing)
            | (Self::Spine, DamageType::TightBeamBurning) => true,
            (Self::Spine, _) => false,

            (Self::ArmLegJoint, DamageType::Crushing)
            | (Self::ArmLegJoint, DamageType::Impaling)
            | (Self::ArmLegJoint, DamageType::Piercing)
            | (Self::ArmLegJoint, DamageType::TightBeamBurning) => true,
            (Self::ArmLegJoint, _) => false,

            (Self::HandFootJoint, DamageType::Crushing)
            | (Self::HandFootJoint, DamageType::Impaling)
            | (Self::HandFootJoint, DamageType::Piercing)
            | (Self::HandFootJoint, DamageType::TightBeamBurning) => true,
            (Self::HandFootJoint, _) => false,

            _ => true,
        }
    }

    pub fn wounding_multiplier(&self, damage_type: DamageType) -> f32 {
        match damage_type {
            DamageType::Toxic => 1.0,
            DamageType::Corrosive => {
                if matches!(self, Self::Face) {
                    1.5
                } else {
                    1.0
                }
            }

            DamageType::Crushing => match self {
                Self::Face => 1.5,
                Self::Neck => 1.5,
                Self::Skull => 2.0,
                _ => 1.0,
            },

            DamageType::Cutting => match self {
                Self::Neck => 2.0,
                Self::Skull => 2.0,
                Self::NeckVascular => 2.0,
                _ => 1.5,
            },

            DamageType::Impaling => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::Groin => 2.0,
                Self::Neck => 2.0,
                Self::RightArm | Self::LeftArm => 1.0,
                Self::RightLeg | Self::LeftLeg => 1.0,
                Self::NeckVascular => 2.0,
                _ => 2.0,
            },

            DamageType::Piercing => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::NeckVascular => 1.5,
                Self::LimbVascular => 1.5,
                _ => 1.0,
            },

            DamageType::SmallPiercing => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::NeckVascular => 1.0,
                Self::LimbVascular => 1.0,
                _ => 0.5,
            },

            DamageType::LargePiercing => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::RightArm | Self::LeftArm => 1.0,
                Self::RightLeg | Self::LeftLeg => 1.0,
                Self::NeckVascular => 2.0,
                Self::LimbVascular => 2.0,
                _ => 1.5,
            },

            DamageType::HugePiercing => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::RightArm | Self::LeftArm => 1.0,
                Self::RightLeg | Self::LeftLeg => 1.0,
                Self::NeckVascular => 2.5,
                Self::LimbVascular => 2.5,
                _ => 2.0,
            },

            DamageType::Burning => 1.0,

            DamageType::TightBeamBurning => match self {
                Self::Skull => 4.0,
                Self::Eye => 4.0,
                Self::Vitals => 3.0,
                Self::Heart => 3.0,
                Self::Neck => 2.0,
                Self::NeckVascular => 1.5,
                Self::LimbVascular => 1.5,
                _ => 1.0,
            },

            DamageType::FatigueDmg => 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Damage types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Crushing,
    Cutting,
    Impaling,
    Piercing,
    SmallPiercing,
    LargePiercing,
    HugePiercing,
    Burning,
    Toxic,
    Corrosive,
    FatigueDmg,
    TightBeamBurning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LimbStatus {
    Healthy,
    Crippled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Posture {
    Standing,
    Kneeling,
    Crouching,
    Sitting,
    Prone,
    Crawling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Encumbrance {
    None,
    Light,
    Medium,
    Heavy,
    ExtraHeavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PainThreshold {
    Normal,
    High,
    Low,
}

// ---------------------------------------------------------------------------
// Actor sub-types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StatusFlags {
    pub stunned: bool,
    pub stun_turns: u8,
    pub knocked_down: bool,
    pub unconscious: bool,
    pub dead: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegState {
    pub left: LimbStatus,
    pub right: LimbStatus,
}

impl Default for LegState {
    fn default() -> Self {
        Self {
            left: LimbStatus::Healthy,
            right: LimbStatus::Healthy,
        }
    }
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub level: u8,
    pub relative_level: i8,
    pub difficulty: SkillDifficulty,
    pub controlling_attribute: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillDifficulty {
    Easy,
    Average,
    Hard,
    VeryHard,
}

// ---------------------------------------------------------------------------
// Attacks and armor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attack {
    pub name: String,
    pub skill_level: u8,
    pub damage_dice: u8,
    pub damage_adds: i8,
    pub damage_type: DamageType,
    pub reach: Vec<u8>,
    pub parry_bonus: Option<i8>,
    pub block_bonus: Option<i8>,
    pub is_ranged: bool,
    pub acc: Option<u8>,
    pub rof: Option<u8>,
    pub rcl: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmorPiece {
    pub name: String,
    pub dr: HashMap<HitLocation, u8>,
}

// ---------------------------------------------------------------------------
// Maneuvers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManeuverType {
    AllOutDefenseIncreased,
    AllOutDefenseDouble,
    AllOutDefenseMental,
    DoNothing,
    Concentrate,
    AllOutConcentrate,
    Ready,
    ChangePosture,
    Move,

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
    Wait,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ManeuverPayload {
    None,
    AimBonus {
        accumulated: u8,
        turns: u8,
        weapon_acc: u8,
    },
    EvaluateBonus {
        accumulated: u8,
    },
    FeintMargin {
        margin: i8,
    },
    AoADetermined {
        to_hit_bonus: u8,
    },
    AoAStrong {
        damage_bonus: u8,
    },
    AoADouble,
    AoAFeint,
    AoALong,
    CommittedDetermined,
    CommittedStrong,
    DefensiveAttack,
    MoveAndAttack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationState {
    Active,
    Triggered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManeuverRelation {
    pub source: ActorId,
    pub target: ActorId,
    pub maneuver: ManeuverType,
    pub state: RelationState,
    pub payload: ManeuverPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtraEffort {
    MightyBlow,
    RapidStrike,
    FlurryOfBlows,
    HeroicCharge,
    GreatLunge,
    GiantStep,
    FeverishDefense,
}

// ---------------------------------------------------------------------------
// Modifiers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub label: String,
    pub value: i8,
    pub applies_to: ModifierTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierTarget {
    AllRolls,
    AttackRolls,
    DefenseRolls,
    DamageRolls,
    SpecificActor(ActorId),
}

// ---------------------------------------------------------------------------
// Turn phase state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TurnPhase {
    ManeuverSelection,
    AttackSetup,
    ManeuverConfirmed,
    AttackRoll,
    DefenseResolution,
    InjuryResolution,
    NonCombatResolution,
    Complete,
}

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    pub id: ActorId,
    pub name: String,
    pub portrait_path: Option<String>,
    pub portrait_data: Option<Vec<u8>>,
    pub source_path: Option<String>,
    pub is_npc: bool,

    pub st: u8,
    pub dx: u8,
    pub iq: u8,
    pub ht: u8,
    pub hp_max: i16,
    pub fp_max: i16,
    pub basic_speed: f32,
    pub basic_move: u8,
    pub will: u8,
    pub per: u8,
    pub attacks: Vec<Attack>,
    pub skills: Vec<Skill>,
    pub armor: Vec<ArmorPiece>,
    pub sm: i8,
    pub is_male: bool,

    pub position: (i32, i32),

    pub hp_current: i16,
    pub fp_current: i16,
    pub posture: Posture,
    pub encumbrance: Encumbrance,
    pub flags: StatusFlags,
    pub leg_state: LegState,
    pub individual_modifiers: Vec<Modifier>,
    pub pain_threshold: PainThreshold,

    pub turns_per_round: u8,
    pub attacks_per_turn: u8,
    pub enhanced_time_sense: bool,

    pub current_maneuver: Option<ManeuverType>,
    pub active_attack: Option<usize>,
    pub extra_effort: Vec<ExtraEffort>,
}

impl Actor {
    pub fn dodge(&self) -> u8 {
        let base = (self.basic_speed + 3.0).floor() as u8;
        let enc_penalty = match self.encumbrance {
            Encumbrance::None => 0,
            Encumbrance::Light => -1,
            Encumbrance::Medium => -2,
            Encumbrance::Heavy => -3,
            Encumbrance::ExtraHeavy => -4,
        };
        (base as i16 + enc_penalty).max(0) as u8
    }

    pub fn current_hp_ratio(&self) -> f32 {
        self.hp_current as f32 / self.hp_max as f32
    }

    pub fn current_fp_ratio(&self) -> f32 {
        self.fp_current as f32 / self.fp_max as f32
    }

    pub fn is_dead(&self) -> bool {
        self.flags.dead || self.hp_current <= -4 * self.hp_max
    }

    pub fn is_destroyed(&self) -> bool {
        self.flags.dead || self.hp_current <= -5 * self.hp_max
    }

    pub fn is_unconscious(&self) -> bool {
        self.flags.unconscious || self.is_dead()
    }

    pub fn is_stunned(&self) -> bool {
        self.flags.stunned
    }

    pub fn is_knocked_down(&self) -> bool {
        self.flags.knocked_down
    }

    pub fn both_legs_crippled(&self) -> bool {
        self.leg_state.left == LimbStatus::Crippled && self.leg_state.right == LimbStatus::Crippled
    }

    pub fn one_leg_crippled(&self) -> bool {
        self.leg_state.left == LimbStatus::Crippled || self.leg_state.right == LimbStatus::Crippled
    }

    pub fn effective_move(&self) -> u8 {
        let base = if self.both_legs_crippled() {
            0
        } else if self.one_leg_crippled() {
            self.basic_move / 2
        } else {
            self.basic_move
        };
        let enc_mult = match self.encumbrance {
            Encumbrance::None => 1.0,
            Encumbrance::Light => 0.8,
            Encumbrance::Medium => 0.6,
            Encumbrance::Heavy => 0.4,
            Encumbrance::ExtraHeavy => 0.2,
        };
        (base as f32 * enc_mult).floor().max(0.0) as u8
    }
}

// ---------------------------------------------------------------------------
// Game state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub actors: HashMap<ActorId, Actor>,
    pub relations: Vec<ManeuverRelation>,
    pub turn_order: Vec<ActorId>,
    pub current_actor: ActorId,
    pub current_phase: TurnPhase,
    pub global_modifiers: Vec<Modifier>,
    pub round: u32,
    pub attacks_remaining: u8,
}

// ---------------------------------------------------------------------------
// Game state history (event sourcing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameStateHistory {
    pub snapshots: Vec<GameState>,
    pub current: usize,
}

impl GameStateHistory {
    pub fn new(initial: GameState) -> Self {
        Self {
            snapshots: vec![initial],
            current: 0,
        }
    }

    pub fn push(&mut self, state: GameState) {
        self.snapshots.truncate(self.current + 1);
        self.snapshots.push(state);
        self.current = self.snapshots.len() - 1;
    }

    pub fn rewind(&mut self) -> Option<&GameState> {
        if self.current > 0 {
            self.current -= 1;
            self.snapshots.get(self.current)
        } else {
            None
        }
    }

    pub fn current(&self) -> &GameState {
        &self.snapshots[self.current]
    }
}

// ---------------------------------------------------------------------------
// Event log (separate from GameState)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventLog {
    pub entries: Vec<LogEntry>,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub round: u32,
    pub turn: ActorId,
    pub phase: TurnPhase,
    pub message: String,
    pub kind: LogEntryKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogEntryKind {
    Info,
    Warning,
    Error,
    ManeuverDeclared,
    RollResult,
    ModifierApplied,
    InjuryResolved,
    PhaseTransition,
    GmAction,
}

// ---------------------------------------------------------------------------
// Ranged combat helpers (B550 range/speed table)
// ---------------------------------------------------------------------------

pub static RANGE_PENALTIES: &[(u32, i8)] = &[
    (1, 0),
    (2, -1),
    (3, -2),
    (5, -3),
    (7, -4),
    (10, -5),
    (15, -6),
    (20, -7),
    (30, -8),
    (50, -9),
    (70, -10),
    (100, -11),
    (150, -12),
    (200, -13),
    (300, -14),
    (500, -15),
    (700, -16),
    (1000, -17),
];

pub fn range_penalty(yards: f32) -> i8 {
    let ceil = yards.ceil() as u32;
    RANGE_PENALTIES
        .iter()
        .find(|(bound, _)| *bound >= ceil)
        .map(|(_, pen)| *pen)
        .unwrap_or(-18)
}

pub fn hex_distance(a: (i32, i32), b: (i32, i32)) -> u32 {
    let dq = (a.0 - b.0).unsigned_abs();
    let dr = (a.1 - b.1).unsigned_abs();
    dq.max(dr).max((a.0 + a.1 - b.0 - b.1).unsigned_abs())
}

/// Sort turn order by initiative priority:
/// 1. ETS actors first (among themselves, sort by Basic Speed desc, then DX desc)
/// 2. Remaining actors by Basic Speed desc
/// 3. Ties broken by DX desc
pub fn sort_turn_order(actors: &HashMap<ActorId, Actor>, turn_order: &mut [ActorId]) {
    turn_order.sort_by(|a, b| {
        let a_actor = actors.get(a);
        let b_actor = actors.get(b);
        match (a_actor, b_actor) {
            (Some(a_actor), Some(b_actor)) => {
                if a_actor.enhanced_time_sense != b_actor.enhanced_time_sense {
                    b_actor
                        .enhanced_time_sense
                        .cmp(&a_actor.enhanced_time_sense)
                } else {
                    b_actor
                        .basic_speed
                        .partial_cmp(&a_actor.basic_speed)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(b_actor.dx.cmp(&a_actor.dx))
                }
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

// ---------------------------------------------------------------------------
// Settings and theming
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub shock_enabled: bool,
    pub theme: String,
    pub event_log_height: f32,
    pub maneuver_tray_height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shock_enabled: true,
            theme: "mil-sim".to_string(),
            event_log_height: 120.0,
            maneuver_tray_height: 190.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub background: Srgba,
    pub panel_surface: Srgba,
    pub panel_border: Srgba,
    pub text_primary: Srgba,
    pub text_secondary: Srgba,
    pub accent: Srgba,
    pub danger: Srgba,
    pub warning: Srgba,
    pub success: Srgba,
    pub maneuver_offensive_defended: Srgba,
    pub maneuver_offensive_undefended: Srgba,
    pub maneuver_setup: Srgba,
    pub maneuver_defensive: Srgba,
    pub maneuver_mental: Srgba,
    pub bar_healthy: Srgba,
    pub bar_low: Srgba,
    pub bar_critical: Srgba,
    pub bar_dead: Srgba,
    pub enc_none: Srgba,
    pub enc_light: Srgba,
    pub enc_medium: Srgba,
    pub enc_heavy: Srgba,
    pub enc_extra: Srgba,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeTypography {
    pub body_font: &'static str,
    pub mono_font: &'static str,
    pub panel_corner_radius: f32,
}

pub trait Theme {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn colors(&self) -> ThemeColors;
    fn typography(&self) -> ThemeTypography;
}

// ---------------------------------------------------------------------------
// Default mil-sim theme
// ---------------------------------------------------------------------------

pub struct MilSimTheme;

impl Theme for MilSimTheme {
    fn id(&self) -> &'static str {
        "mil-sim"
    }

    fn display_name(&self) -> &'static str {
        "Mil-Sim"
    }

    fn colors(&self) -> ThemeColors {
        ThemeColors {
            background: Srgba {
                r: 0.05,
                g: 0.05,
                b: 0.05,
                a: 1.0,
            },
            panel_surface: Srgba {
                r: 0.086,
                g: 0.086,
                b: 0.086,
                a: 1.0,
            },
            panel_border: Srgba {
                r: 0.165,
                g: 0.165,
                b: 0.165,
                a: 1.0,
            },
            text_primary: Srgba {
                r: 0.878,
                g: 0.878,
                b: 0.878,
                a: 1.0,
            },
            text_secondary: Srgba {
                r: 0.533,
                g: 0.533,
                b: 0.533,
                a: 1.0,
            },
            accent: Srgba {
                r: 0.227,
                g: 0.482,
                b: 0.835,
                a: 1.0,
            },
            danger: Srgba {
                r: 0.8,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            },
            warning: Srgba {
                r: 0.8,
                g: 0.667,
                b: 0.0,
                a: 1.0,
            },
            success: Srgba {
                r: 0.267,
                g: 0.667,
                b: 0.267,
                a: 1.0,
            },
            maneuver_offensive_defended: Srgba {
                r: 0.8,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            },
            maneuver_offensive_undefended: Srgba {
                r: 1.0,
                g: 0.4,
                b: 0.0,
                a: 1.0,
            },
            maneuver_setup: Srgba {
                r: 0.8,
                g: 0.667,
                b: 0.0,
                a: 1.0,
            },
            maneuver_defensive: Srgba {
                r: 0.2,
                g: 0.4,
                b: 0.6,
                a: 1.0,
            },
            maneuver_mental: Srgba {
                r: 0.467,
                g: 0.333,
                b: 0.667,
                a: 1.0,
            },
            bar_healthy: Srgba {
                r: 0.267,
                g: 0.667,
                b: 0.267,
                a: 1.0,
            },
            bar_low: Srgba {
                r: 0.8,
                g: 0.667,
                b: 0.0,
                a: 1.0,
            },
            bar_critical: Srgba {
                r: 0.8,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            },
            bar_dead: Srgba {
                r: 0.533,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            enc_none: Srgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            enc_light: Srgba {
                r: 0.867,
                g: 0.867,
                b: 0.8,
                a: 1.0,
            },
            enc_medium: Srgba {
                r: 0.8,
                g: 0.667,
                b: 0.0,
                a: 1.0,
            },
            enc_heavy: Srgba {
                r: 1.0,
                g: 0.4,
                b: 0.0,
                a: 1.0,
            },
            enc_extra: Srgba {
                r: 0.8,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            },
        }
    }

    fn typography(&self) -> ThemeTypography {
        ThemeTypography {
            body_font: "Inter",
            mono_font: "JetBrains Mono",
            panel_corner_radius: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_location_penalties() {
        assert_eq!(HitLocation::Torso.to_hit_penalty(), 0);
        assert_eq!(HitLocation::Skull.to_hit_penalty(), -7);
        assert_eq!(HitLocation::Face.to_hit_penalty(), -5);
        assert_eq!(HitLocation::Eye.to_hit_penalty(), -9);
        assert_eq!(HitLocation::Neck.to_hit_penalty(), -5);
        assert_eq!(HitLocation::Vitals.to_hit_penalty(), -3);
        assert_eq!(HitLocation::Groin.to_hit_penalty(), -3);
        assert_eq!(HitLocation::ArmLegJoint.to_hit_penalty(), -5);
        assert_eq!(HitLocation::HandFootJoint.to_hit_penalty(), -7);
    }

    #[test]
    fn test_inherent_dr() {
        assert_eq!(HitLocation::Skull.inherent_dr(), 2);
        assert_eq!(HitLocation::Spine.inherent_dr(), 3);
        assert_eq!(HitLocation::Torso.inherent_dr(), 0);
    }

    #[test]
    fn test_random_hit_location_table() {
        assert_eq!(HitLocation::from_random_roll(3).0, HitLocation::Torso);
        assert_eq!(HitLocation::from_random_roll(4).0, HitLocation::Torso);
        assert_eq!(HitLocation::from_random_roll(5).0, HitLocation::RightArm);
        assert_eq!(HitLocation::from_random_roll(11).0, HitLocation::Groin);
        assert_eq!(HitLocation::from_random_roll(18).0, HitLocation::Neck);
    }

    #[test]
    fn test_eye_illegal_damage_types() {
        assert!(!HitLocation::Eye.is_legal_target(DamageType::Crushing));
        assert!(!HitLocation::Eye.is_legal_target(DamageType::Cutting));
        assert!(HitLocation::Eye.is_legal_target(DamageType::Impaling));
        assert!(HitLocation::Eye.is_legal_target(DamageType::Piercing));
        assert!(HitLocation::Eye.is_legal_target(DamageType::TightBeamBurning));
    }

    #[test]
    fn test_vitals_tox_wounding() {
        assert_eq!(
            HitLocation::Vitals.wounding_multiplier(DamageType::Toxic),
            1.0
        );
        assert_eq!(
            HitLocation::Skull.wounding_multiplier(DamageType::Toxic),
            1.0
        );
    }

    #[test]
    fn test_skull_impaling_wounding() {
        assert_eq!(
            HitLocation::Skull.wounding_multiplier(DamageType::Impaling),
            4.0
        );
    }

    #[test]
    fn test_face_cr_wounding() {
        assert_eq!(
            HitLocation::Face.wounding_multiplier(DamageType::Crushing),
            1.5
        );
    }

    #[test]
    fn test_neck_cut_wounding() {
        assert_eq!(
            HitLocation::Neck.wounding_multiplier(DamageType::Cutting),
            2.0
        );
    }

    #[test]
    fn test_face_corrosive_wounding() {
        assert_eq!(
            HitLocation::Face.wounding_multiplier(DamageType::Corrosive),
            1.5
        );
    }

    #[test]
    fn test_torso_corrosive_wounding() {
        assert_eq!(
            HitLocation::Torso.wounding_multiplier(DamageType::Corrosive),
            1.0
        );
    }

    #[test]
    fn test_range_penalty() {
        assert_eq!(range_penalty(0.5), 0);
        assert_eq!(range_penalty(1.0), 0);
        assert_eq!(range_penalty(1.5), -1);
        assert_eq!(range_penalty(3.0), -2);
        assert_eq!(range_penalty(10.0), -5);
        assert_eq!(range_penalty(100.0), -11);
        assert_eq!(range_penalty(101.0), -12);
        assert_eq!(range_penalty(1000.0), -17);
        assert_eq!(range_penalty(2000.0), -18);
    }

    #[test]
    fn test_hex_distance() {
        assert_eq!(hex_distance((0, 0), (0, 0)), 0);
        assert_eq!(hex_distance((0, 0), (1, 0)), 1);
        assert_eq!(hex_distance((0, 0), (0, 1)), 1);
        assert_eq!(hex_distance((0, 0), (-1, 1)), 1);
        assert_eq!(hex_distance((0, 0), (1, -1)), 1);
        assert_eq!(hex_distance((0, 0), (1, 1)), 2);
        assert_eq!(hex_distance((0, 0), (2, 0)), 2);
        assert_eq!(hex_distance((3, 2), (5, 4)), 4);
    }

    #[test]
    fn test_dodge_calculation() {
        let actor = Actor {
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
            basic_speed: 6.25,
            basic_move: 6,
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
        };
        assert_eq!(actor.dodge(), 9);
    }

    #[test]
    fn test_dodge_with_encumbrance() {
        let actor = Actor {
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
            basic_speed: 6.25,
            basic_move: 6,
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
            encumbrance: Encumbrance::Light,
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
        };
        assert_eq!(actor.dodge(), 8);
    }

    #[test]
    fn test_history_push_and_rewind() {
        let state1 = GameState {
            actors: HashMap::new(),
            relations: vec![],
            turn_order: vec![],
            current_actor: 0,
            current_phase: TurnPhase::ManeuverSelection,
            global_modifiers: vec![],
            round: 1,
            attacks_remaining: 1,
        };
        let mut state2 = state1.clone();
        state2.round = 2;
        let state3 = state2.clone();

        let mut history = GameStateHistory::new(state1);
        history.push(state2.clone());
        history.push(state3.clone());

        assert_eq!(history.snapshots.len(), 3);
        assert_eq!(history.current, 2);
        assert_eq!(history.current(), &state3);

        history.rewind();
        assert_eq!(history.current, 1);
        assert_eq!(history.current(), &state2);
    }

    #[test]
    fn test_rewind_truncation() {
        let state1 = GameState {
            actors: HashMap::new(),
            relations: vec![],
            turn_order: vec![],
            current_actor: 0,
            current_phase: TurnPhase::ManeuverSelection,
            global_modifiers: vec![],
            round: 1,
            attacks_remaining: 1,
        };
        let mut state2 = state1.clone();
        state2.round = 2;
        let mut state3 = state2.clone();
        state3.round = 3;
        let mut state4 = state1.clone();
        state4.round = 99;

        let mut history = GameStateHistory::new(state1);
        history.push(state2);
        history.push(state3);
        assert_eq!(history.current, 2);
        assert_eq!(history.snapshots.len(), 3);

        history.rewind();
        assert_eq!(history.current, 1);

        history.push(state4);
        assert_eq!(history.current, 2);
        assert_eq!(history.snapshots.len(), 3);
        assert_eq!(history.current().round, 99);
    }

    #[test]
    fn test_effective_move_one_leg_crippled() {
        let mut actor = Actor {
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
            basic_speed: 6.0,
            basic_move: 6,
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
        };
        actor.leg_state.right = LimbStatus::Crippled;
        assert_eq!(actor.effective_move(), 3);
    }

    #[test]
    fn test_effective_move_both_legs_crippled() {
        let mut actor = Actor {
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
            basic_speed: 6.0,
            basic_move: 6,
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
        };
        actor.leg_state.left = LimbStatus::Crippled;
        actor.leg_state.right = LimbStatus::Crippled;
        assert_eq!(actor.effective_move(), 0);
    }

    #[test]
    fn test_serde_roundtrip_game_state() {
        let state = GameState {
            actors: HashMap::new(),
            relations: vec![],
            turn_order: vec![],
            current_actor: 1,
            current_phase: TurnPhase::ManeuverSelection,
            global_modifiers: vec![],
            round: 1,
            attacks_remaining: 1,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.round, 1);
        assert_eq!(decoded.current_phase, TurnPhase::ManeuverSelection);
    }

    #[test]
    fn test_flip_side() {
        assert_eq!(HitLocation::RightArm.flip_side(), HitLocation::LeftArm);
        assert_eq!(HitLocation::LeftArm.flip_side(), HitLocation::RightArm);
        assert_eq!(HitLocation::RightLeg.flip_side(), HitLocation::LeftLeg);
        assert_eq!(HitLocation::Torso.flip_side(), HitLocation::Torso);
    }

    #[test]
    fn test_sort_turn_order_ets_first() {
        let mut actors = HashMap::new();
        actors.insert(
            1,
            Actor {
                id: 1,
                name: "Slow".into(),
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
        actors.insert(
            2,
            Actor {
                id: 2,
                name: "Fast".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: false,
                st: 10,
                dx: 12,
                iq: 10,
                ht: 10,
                hp_max: 10,
                fp_max: 10,
                basic_speed: 7.0,
                basic_move: 7,
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
        actors.insert(
            3,
            Actor {
                id: 3,
                name: "ETS".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: false,
                st: 10,
                dx: 9,
                iq: 10,
                ht: 10,
                hp_max: 10,
                fp_max: 10,
                basic_speed: 4.0,
                basic_move: 4,
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
                enhanced_time_sense: true,
                current_maneuver: None,
                active_attack: None,
                extra_effort: vec![],
            },
        );

        let mut turn_order = vec![1, 2, 3];
        sort_turn_order(&actors, &mut turn_order);
        assert_eq!(turn_order, vec![3, 2, 1]); // ETS first, then Fast, then Slow
    }

    #[test]
    fn test_sort_turn_order_dx_tiebreaker() {
        let mut actors = HashMap::new();
        actors.insert(
            1,
            Actor {
                id: 1,
                name: "Equal Speed Low DX".into(),
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
                basic_speed: 6.0,
                basic_move: 6,
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
        actors.insert(
            2,
            Actor {
                id: 2,
                name: "Equal Speed High DX".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: false,
                st: 10,
                dx: 14,
                iq: 10,
                ht: 10,
                hp_max: 10,
                fp_max: 10,
                basic_speed: 6.0,
                basic_move: 6,
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

        let mut turn_order = vec![1, 2];
        sort_turn_order(&actors, &mut turn_order);
        assert_eq!(turn_order, vec![2, 1]); // higher DX goes first
    }
}
