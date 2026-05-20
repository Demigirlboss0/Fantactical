use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;
use fantactical::model::{
    Actor, ActorId, Attack, DamageType, GameState, GameStateHistory, HitLocation,
    TurnPhase, Posture, Encumbrance, StatusFlags, LegState, PainThreshold,
};
use fantactical::model::injury::resolve_injury;
use fantactical::model::rolls::{check_roll, roll_3d6, roll_damage, RollOutcome};
use fantactical::model::maneuver_legality::available_maneuvers;
use fantactical::state::history;

#[derive(Parser)]
#[command(name = "ftctl")]
#[command(about = "Fantactical CLI test/control interface")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Dice {
        #[arg(short, long, default_value = "3")]
        count: u8,
    },
    CheckRoll {
        roll: u8,
        skill: u8,
    },
    DamageRoll {
        dice: u8,
        #[arg(long, default_value = "0")]
        adds: i8,
    },
    TestInjury {
        #[arg(long)]
        hp: i16,
        #[arg(long)]
        location: String,
        #[arg(long)]
        damage: u32,
        #[arg(long, default_value = "cr")]
        damage_type: String,
        #[arg(long)]
        dr: Option<u8>,
    },
    AvailableManeuvers {
        #[arg(long, default_value = "standing")]
        posture: String,
        #[arg(long, default_value = "normal")]
        status: String,
        #[arg(long, default_value = "none")]
        encumbrance: String,
    },
    RangePenalty {
        yards: f32,
    },
    HexDistance {
        q1: i32,
        r1: i32,
        q2: i32,
        r2: i32,
    },
    SaveState {
        path: PathBuf,
        #[arg(long, default_value = "1")]
        round: u32,
    },
    LoadState {
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Dice { count } => {
            let (dice, total) = fantactical::model::rolls::roll_dice(count);
            println!("{}d6: {:?} = {}", count, dice, total);
        }

        Command::CheckRoll { roll, skill } => {
            let outcome = check_roll(roll, skill);
            println!("Roll {} vs skill {}: {:?}", roll, skill, outcome);
            match outcome {
                RollOutcome::CriticalSuccess => println!("Result: CRITICAL SUCCESS"),
                RollOutcome::Success => println!("Result: SUCCESS"),
                RollOutcome::Failure => println!("Result: FAILURE"),
                RollOutcome::CriticalFailure => println!("Result: CRITICAL FAILURE"),
            }
        }

        Command::DamageRoll { dice, adds } => {
            let dmg = roll_damage(dice, adds);
            println!("{}d{:+}: {}", dice, adds, dmg);
        }

        Command::TestInjury { hp, location, damage, damage_type, dr } => {
            let loc = parse_location(&location);
            let dt = parse_damage_type(&damage_type);

            let mut armor = vec![];
            if let Some(dr_val) = dr {
                let mut dr_map = HashMap::new();
                dr_map.insert(loc, dr_val as u8);
                armor.push(fantactical::model::ArmorPiece {
                    name: "Test Armor".into(),
                    dr: dr_map,
                });
            }

            let target = Actor {
                id: 2,
                name: "TestTarget".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: true,
                st: 10, dx: 10, iq: 10, ht: 10,
                hp_max: hp,
                fp_max: 10,
                basic_speed: 6.0,
                basic_move: 6,
                will: 10,
                per: 10,
                attacks: vec![],
                skills: vec![],
                armor,
                sm: 0,
                is_male: true,
                position: (0, 0),
                hp_current: hp,
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

            let mut actors = HashMap::new();
            actors.insert(2, target);
            let state = GameState {
                actors,
                relations: vec![],
                turn_order: vec![2],
                current_actor: 2,
                current_phase: TurnPhase::InjuryResolution,
                global_modifiers: vec![],
                round: 1,
                attacks_remaining: 1,
            };

            match resolve_injury(&state, 2, loc, damage, dt, None, true) {
                Ok((new_state, outcome)) => {
                    let updated = new_state.actors.get(&2).unwrap();
                    println!("Location: {:?}", loc);
                    println!("Damage type: {:?}", dt);
                    println!("Raw damage rolled: {}", damage);
                    println!("Effective DR: {}", outcome.effective_dr);
                    println!("Wounding multiplier: x{}", outcome.wounding_multiplier);
                    println!("Raw injury: {}", outcome.raw_injury);
                    println!("HP: {} → {}", hp, updated.hp_current);
                    println!("Shock penalty: {}", outcome.shock_penalty);
                    println!("Major wound: {}", outcome.major_wound);
                    println!("Knockdown: {} (mod {})", outcome.knockdown, outcome.knockdown_modifier);
                    println!("Stunned: {}", outcome.stunned);
                    println!("Limb crippled: {:?}", outcome.limb_crippled.iter().map(|l| format!("{:?}", l.location)).collect::<Vec<_>>());
                    println!("Consciousness roll: {} (penalty {})", outcome.consciousness_roll_needed, outcome.consciousness_penalty);
                    println!("Dead: {}", outcome.dead);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
        }

        Command::AvailableManeuvers { posture, status, encumbrance } => {
            let mut actor = Actor {
                id: 1,
                name: "Test".into(),
                portrait_path: None,
                portrait_data: None,
                source_path: None,
                is_npc: false,
                st: 10, dx: 10, iq: 10, ht: 10,
                hp_max: 10,
                fp_max: 10,
                basic_speed: 6.0,
                basic_move: 6,
                will: 10,
                per: 10,
                attacks: vec![Attack {
                    name: "Punch".into(),
                    skill_level: 12,
                    damage_dice: 1,
                    damage_adds: -1,
                    damage_type: DamageType::Crushing,
                    reach: vec![1],
                    parry_bonus: Some(0),
                    block_bonus: None,
                    is_ranged: false,
                    acc: None, rof: None, rcl: None,
                }],
                skills: vec![],
                armor: vec![],
                sm: 0,
                is_male: true,
                position: (0, 0),
                hp_current: 10,
                fp_current: 10,
                posture: parse_posture(&posture),
                encumbrance: parse_encumbrance(&encumbrance),
                flags: parse_status(&status),
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

            let maneuvers = available_maneuvers(&actor);
            println!("Posture: {:?}, Encumbrance: {:?}, Status: {:?}", actor.posture, actor.encumbrance, status);
            println!("Available maneuvers: {:?}", maneuvers);
            println!("Count: {}", maneuvers.len());
        }

        Command::RangePenalty { yards } => {
            let penalty = fantactical::model::range_penalty(yards);
            println!("Range {} yd: penalty {}", yards, penalty);
        }

        Command::HexDistance { q1, r1, q2, r2 } => {
            let dist = fantactical::model::hex_distance((q1, r1), (q2, r2));
            println!("Hex distance ({}, {}) to ({}, {}): {} yd", q1, r1, q2, r2, dist);
        }

        Command::SaveState { path, round } => {
            let mut actors = HashMap::new();
            let state = GameState {
                actors,
                relations: vec![],
                turn_order: vec![],
                current_actor: 0,
                current_phase: TurnPhase::ManeuverSelection,
                global_modifiers: vec![],
                round,
                attacks_remaining: 1,
            };
            let history = GameStateHistory::new(state);
            history::save_history(&history, &path)
                .map_err(|e| eprintln!("Save error: {e}"))
                .ok();
            println!("Saved state to {}", path.display());
        }

        Command::LoadState { path } => {
            match history::load_history(&path) {
                Ok(history) => {
                    let current = history.current();
                    println!("Loaded state: round {}, {} snapshots, {} actors",
                        current.round, history.snapshots.len(), current.actors.len());
                }
                Err(e) => eprintln!("Load error: {e}"),
            }
        }
    }
}

fn parse_location(s: &str) -> HitLocation {
    match s.to_lowercase().as_str() {
        "torso" => HitLocation::Torso,
        "skull" => HitLocation::Skull,
        "face" => HitLocation::Face,
        "neck" => HitLocation::Neck,
        "vitals" => HitLocation::Vitals,
        "groin" => HitLocation::Groin,
        "eye" => HitLocation::Eye,
        "ear" => HitLocation::Ear,
        "nose" => HitLocation::Nose,
        "jaw" => HitLocation::Jaw,
        "abdomen" => HitLocation::Abdomen,
        "pelvis" => HitLocation::Pelvis,
        "spine" => HitLocation::Spine,
        "heart" => HitLocation::Heart,
        "right_arm" | "right arm" => HitLocation::RightArm,
        "left_arm" | "left arm" => HitLocation::LeftArm,
        "right_leg" | "right leg" => HitLocation::RightLeg,
        "left_leg" | "left leg" => HitLocation::LeftLeg,
        "right_hand" | "right hand" => HitLocation::RightHand,
        "left_hand" | "left hand" => HitLocation::LeftHand,
        "right_foot" | "right foot" => HitLocation::RightFoot,
        "left_foot" | "left foot" => HitLocation::LeftFoot,
        _ => HitLocation::Torso,
    }
}

fn parse_damage_type(s: &str) -> DamageType {
    match s.to_lowercase().as_str() {
        "cr" | "crushing" => DamageType::Crushing,
        "cut" | "cutting" => DamageType::Cutting,
        "imp" | "impaling" => DamageType::Impaling,
        "pi-" | "small_piercing" => DamageType::SmallPiercing,
        "pi" | "piercing" => DamageType::Piercing,
        "pi+" | "large_piercing" => DamageType::LargePiercing,
        "pi++" | "huge_piercing" => DamageType::HugePiercing,
        "burn" | "burning" => DamageType::Burning,
        "tox" | "toxic" => DamageType::Toxic,
        "cor" | "corrosive" => DamageType::Corrosive,
        "fat" | "fatigue" => DamageType::FatigueDmg,
        "tbb" | "tight_beam" => DamageType::TightBeamBurning,
        _ => DamageType::Crushing,
    }
}

fn parse_posture(s: &str) -> Posture {
    match s.to_lowercase().as_str() {
        "standing" | "stand" => Posture::Standing,
        "kneeling" | "kneel" => Posture::Kneeling,
        "crouching" | "crouch" => Posture::Crouching,
        "sitting" | "sit" => Posture::Sitting,
        "prone" => Posture::Prone,
        "crawling" | "crawl" => Posture::Crawling,
        _ => Posture::Standing,
    }
}

fn parse_encumbrance(s: &str) -> Encumbrance {
    match s.to_lowercase().as_str() {
        "none" | "0" => Encumbrance::None,
        "light" | "1" => Encumbrance::Light,
        "medium" | "2" => Encumbrance::Medium,
        "heavy" | "3" => Encumbrance::Heavy,
        "extra_heavy" | "extraheavy" | "4" => Encumbrance::ExtraHeavy,
        _ => Encumbrance::None,
    }
}

fn parse_status(s: &str) -> StatusFlags {
    let normal = StatusFlags::default();
    match s.to_lowercase().as_str() {
        "dead" => StatusFlags { dead: true, ..normal },
        "unconscious" | "uncon" => StatusFlags { unconscious: true, ..normal },
        "stunned" | "stun" => StatusFlags { stunned: true, ..normal },
        "knocked_down" | "knockdown" | "kd" => StatusFlags { knocked_down: true, ..normal },
        _ => normal,
    }
}
