use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjuryOutcome {
    pub target_id: ActorId,
    pub hit_location: HitLocation,
    pub raw_injury: u32,
    pub effective_dr: u8,
    pub wounding_multiplier: f32,
    pub rolled_damage: u32,
    pub damage_type: DamageType,
    pub major_wound: bool,
    pub knockdown: bool,
    pub stunned: bool,
    pub limb_crippled: Vec<LimbCrippleInfo>,
    pub hp_lost: u32,
    pub shock_penalty: i8,
    pub consciousness_roll_needed: bool,
    pub consciousness_penalty: i8,
    pub knockdown_modifier: i8,
    pub dead: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimbCrippleInfo {
    pub location: HitLocation,
}

pub fn resolve_injury(
    state: &GameState,
    target_id: ActorId,
    hit_location: HitLocation,
    rolled_damage: u32,
    damage_type: DamageType,
    crit_result: Option<CritHitResult>,
    shock_enabled: bool,
) -> anyhow::Result<(GameState, InjuryOutcome)> {
    let target = state
        .actors
        .get(&target_id)
        .ok_or_else(|| anyhow::anyhow!("Target actor {} not found", target_id))?;

    if !hit_location.is_legal_target(damage_type) {
        return Err(anyhow::anyhow!(
            "Damage type {:?} cannot target {:?}",
            damage_type,
            hit_location
        ));
    }

    let multiplier = hit_location.wounding_multiplier(damage_type);

    let effective_dr = if hit_location == HitLocation::Eye {
        0 // Eye ignores DR entirely
    } else {
        calculate_dr(target, hit_location, crit_result)
    };
    let (final_damage, is_max_damage) = apply_crit_damage_modifier(rolled_damage, crit_result);

    let penetrating = if final_damage > effective_dr as u32 {
        final_damage - effective_dr as u32
    } else {
        0
    };

    let raw_injury = if is_max_damage {
        penetrating
    } else {
        (penetrating as f32 * multiplier).floor() as u32
    };

    let mut modified = target.clone();
    modified.hp_current -= raw_injury as i16;

    let hp_max = modified.hp_max;
    let major_wound = raw_injury > hp_max as u32 / 2;

    let (knockdown, stunned, knockdown_modifier) = determine_knockdown(
        target,
        hit_location,
        damage_type,
        major_wound,
        raw_injury,
        crit_result,
    );

    let is_high_pain = matches!(target.pain_threshold, PainThreshold::High);
    let is_low_pain = matches!(target.pain_threshold, PainThreshold::Low);

    let shock_penalty: i8 = if shock_enabled && !is_high_pain {
        let base = (raw_injury as i16).min(4);
        let penalty = if is_low_pain { (base * 2).min(4) } else { base };
        -(penalty as i8)
    } else {
        0
    };

    if shock_penalty != 0 {
        modified.individual_modifiers.push(Modifier {
            label: format!("Shock (-{})", shock_penalty.abs()),
            value: -shock_penalty,
            applies_to: ModifierTarget::SpecificActor(target_id),
        });
    }

    if stunned {
        modified.flags.stunned = true;
        modified.flags.stun_turns = 0;
    }

    if knockdown {
        modified.flags.knocked_down = true;
    }

    let limb_crippled = check_limb_crippling(target, hit_location, raw_injury, crit_result);
    for info in &limb_crippled {
        apply_limb_cripple(&mut modified, info.location);
    }

    let dead = modified.is_dead();
    if dead {
        modified.flags.dead = true;
    }

    let (consciousness_roll_needed, consciousness_penalty) =
        check_consciousness(&modified, hp_max);

    let mut new_state = state.clone();
    new_state.actors.insert(target_id, modified);

    let outcome = InjuryOutcome {
        target_id,
        hit_location,
        raw_injury,
        effective_dr,
        wounding_multiplier: multiplier,
        rolled_damage,
        damage_type,
        major_wound,
        knockdown,
        stunned,
        limb_crippled,
        hp_lost: raw_injury,
        shock_penalty,
        consciousness_roll_needed,
        consciousness_penalty,
        knockdown_modifier,
        dead,
    };

    Ok((new_state, outcome))
}

fn calculate_dr(target: &Actor, hit_location: HitLocation, crit_result: Option<CritHitResult>) -> u8 {
    if let Some(cr) = crit_result {
        if cr.ignore_dr() {
            return 0;
        }
    }

    let armor_dr: u8 = target.armor.iter().fold(0, |sum, piece| {
        sum + piece.dr.get(&hit_location).copied().unwrap_or(0)
    });

    let inherent = hit_location.inherent_dr();

    let total = armor_dr + inherent;

    if let Some(cr) = crit_result {
        if cr.halve_dr() {
            return total / 2;
        }
    }

    total
}

fn apply_crit_damage_modifier(damage: u32, crit_result: Option<CritHitResult>) -> (u32, bool) {
    match crit_result {
        Some(CritHitResult::DoubleDamage) => (damage * 2, false),
        Some(CritHitResult::TripleDamage) => (damage * 3, false),
        Some(CritHitResult::MaxDamage) => (damage, true),
        _ => (damage, false),
    }
}

fn determine_knockdown(
    target: &Actor,
    hit_location: HitLocation,
    damage_type: DamageType,
    major_wound: bool,
    _raw_injury: u32,
    crit_result: Option<CritHitResult>,
) -> (bool, bool, i8) {
    if target.flags.dead {
        return (false, false, 0);
    }

    if let Some(CritHitResult::KnockdownAutomatic) = crit_result {
        return (true, true, 0);
    }

    if let Some(CritHitResult::StunAutomatic) = crit_result {
        return (false, true, 0);
    }

    let mut modifier: i8 = 0;

    let knockout_from_crit = matches!(
        crit_result,
        Some(CritHitResult::MajorWoundAutomatic)
    );

    if !major_wound && !knockout_from_crit {
        return (false, false, 0);
    }

    if hit_location == HitLocation::Skull {
        modifier -= 10;
    } else if hit_location == HitLocation::Face {
        modifier -= 5;
    } else if hit_location == HitLocation::Jaw && damage_type == DamageType::Crushing {
        modifier -= 1;
    }

    if hit_location == HitLocation::Groin && damage_type == DamageType::Crushing && target.is_male {
        modifier -= 5;
    }

    let is_high_pain = matches!(target.pain_threshold, PainThreshold::High);
    let is_low_pain = matches!(target.pain_threshold, PainThreshold::Low);

    if is_high_pain {
        modifier += 3;
    } else if is_low_pain {
        modifier -= 4;
    }

    (true, true, modifier)
}

fn check_limb_crippling(
    target: &Actor,
    hit_location: HitLocation,
    raw_injury: u32,
    crit_result: Option<CritHitResult>,
) -> Vec<LimbCrippleInfo> {
    if let Some(CritHitResult::CrippleLimbAutomatic) = crit_result {
        return vec![LimbCrippleInfo {
            location: hit_location,
        }];
    }

    let is_leg = matches!(hit_location, HitLocation::RightLeg | HitLocation::LeftLeg);
    let is_arm = matches!(hit_location, HitLocation::RightArm | HitLocation::LeftArm);
    let is_hand = matches!(hit_location, HitLocation::RightHand | HitLocation::LeftHand);
    let is_foot = matches!(hit_location, HitLocation::RightFoot | HitLocation::LeftFoot);

    let cripple_threshold = if is_hand || is_foot {
        target.hp_max as u32 / 3
    } else if is_leg || is_arm {
        target.hp_max as u32 / 2
    } else {
        u32::MAX
    };

    if raw_injury > cripple_threshold {
        vec![LimbCrippleInfo {
            location: hit_location,
        }]
    } else {
        vec![]
    }
}

fn apply_limb_cripple(actor: &mut Actor, location: HitLocation) {
    match location {
        HitLocation::RightLeg => actor.leg_state.right = LimbStatus::Crippled,
        HitLocation::LeftLeg => actor.leg_state.left = LimbStatus::Crippled,
        HitLocation::RightFoot => actor.leg_state.right = LimbStatus::Crippled,
        HitLocation::LeftFoot => actor.leg_state.left = LimbStatus::Crippled,
        _ => {}
    }
}

fn check_consciousness(actor: &Actor, hp_max: i16) -> (bool, i8) {
    if actor.hp_current > 0 {
        return (false, 0);
    }

    if actor.flags.dead || actor.flags.unconscious {
        return (false, 0);
    }

    let penalty = if actor.hp_current <= -4 * hp_max {
        -3
    } else if actor.hp_current <= -3 * hp_max {
        -3
    } else if actor.hp_current <= -2 * hp_max {
        -2
    } else if actor.hp_current <= -hp_max {
        -1
    } else {
        0
    };

    (true, penalty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_target() -> Actor {
        Actor {
            id: 2,
            name: "Target".into(),
            portrait_path: None,
            portrait_data: None,
            source_path: None,
            is_npc: true,
            st: 10,
            dx: 10,
            iq: 10,
            ht: 10,
            hp_max: 12,
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
            position: (5, 0),
            hp_current: 12,
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
        }
    }

    fn test_state(target: Actor) -> GameState {
        let mut actors = HashMap::new();
        actors.insert(target.id, target);
        GameState {
            actors,
            relations: vec![],
            turn_order: vec![2],
            current_actor: 2,
            current_phase: TurnPhase::InjuryResolution,
            global_modifiers: vec![],
            round: 1,
            attacks_remaining: 1,
        }
    }

    #[test]
    fn test_basic_injury_torso_crushing() {
        let target = test_target();
        let state = test_state(target);
        let (new_state, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.raw_injury, 5);
        assert_eq!(outcome.effective_dr, 0);
        assert_eq!(outcome.wounding_multiplier, 1.0);
        assert_eq!(outcome.shock_penalty, -4); // min(4, 5) = 4 shock
        let updated = new_state.actors.get(&2).unwrap();
        assert_eq!(updated.hp_current, 7); // 12 - 5
    }

    #[test]
    fn test_dr_reduces_damage() {
        let mut target = test_target();
        target.armor.push(ArmorPiece {
            name: "Vest".into(),
            dr: {
                let mut m = HashMap::new();
                m.insert(HitLocation::Torso, 3);
                m
            },
        });
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.raw_injury, 2); // 5 - 3 DR = 2 penetrating
        assert_eq!(outcome.effective_dr, 3);
    }

    #[test]
    fn test_dr_absorbs_all_damage() {
        let mut target = test_target();
        target.armor.push(ArmorPiece {
            name: "Heavy Vest".into(),
            dr: {
                let mut m = HashMap::new();
                m.insert(HitLocation::Torso, 10);
                m
            },
        });
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.raw_injury, 0);
        assert_eq!(outcome.shock_penalty, 0);
    }

    #[test]
    fn test_skull_impaling_injury() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Skull,
            6,
            DamageType::Impaling,
            None,
            true,
        )
        .unwrap();
        // Skull DR 2 innate; 6 - 2 = 4 penetrating; ×4 = 16
        assert_eq!(outcome.effective_dr, 2);
        assert_eq!(outcome.wounding_multiplier, 4.0);
        assert_eq!(outcome.raw_injury, 16);
        assert!(outcome.major_wound); // 16 > 6 (half of 12 HP)
        assert!(outcome.knockdown);
    }

    #[test]
    fn test_neck_cutting_wounding() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Neck,
            4,
            DamageType::Cutting,
            None,
            true,
        )
        .unwrap();
        // Neck cut = ×2; 4 - 0 = 4; 4 × 2 = 8
        assert_eq!(outcome.wounding_multiplier, 2.0);
        assert_eq!(outcome.raw_injury, 8);
    }

    #[test]
    fn test_vitals_impaling() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Vitals,
            5,
            DamageType::Impaling,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.wounding_multiplier, 3.0);
        assert_eq!(outcome.raw_injury, 15); // 5 × 3
    }

    #[test]
    fn test_eye_illegal_crushing_rejected() {
        let target = test_target();
        let state = test_state(target);
        let result = resolve_injury(
            &state,
            2,
            HitLocation::Eye,
            3,
            DamageType::Crushing,
            None,
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_eye_impaling_ignores_dr() {
        let mut target = test_target();
        target.armor.push(ArmorPiece {
            name: "Sunglasses".into(),
            dr: {
                let mut m = HashMap::new();
                m.insert(HitLocation::Eye, 4);
                m
            },
        });
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Eye,
            3,
            DamageType::Impaling,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.effective_dr, 0);
        assert_eq!(outcome.raw_injury, 12); // 3 × 4 = 12, DR ignored
    }

    #[test]
    fn test_leg_crippling() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::RightLeg,
            6,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        // 6 > 10/2 = 5, so leg crippled
        assert_eq!(outcome.limb_crippled.len(), 1);
        assert_eq!(outcome.limb_crippled[0].location, HitLocation::RightLeg);
    }

    #[test]
    fn test_leg_not_crippled_below_threshold() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::RightLeg,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert!(outcome.limb_crippled.is_empty());
    }

    #[test]
    fn test_shock_disabled() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            10,
            DamageType::Crushing,
            None,
            false, // shock disabled
        )
        .unwrap();
        assert_eq!(outcome.shock_penalty, 0);
    }

    #[test]
    fn test_high_pain_threshold_no_shock() {
        let mut target = test_target();
        target.pain_threshold = PainThreshold::High;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.shock_penalty, 0);
    }

    #[test]
    fn test_low_pain_threshold_double_shock() {
        let mut target = test_target();
        target.pain_threshold = PainThreshold::Low;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            2,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        // Normal shock: min(4, 2) = 2. Doubled = 4, capped at 4.
        assert_eq!(outcome.shock_penalty, -4);
    }

    #[test]
    fn test_major_wound_knockdown() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            6,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        // 6 > 10/2 = 5, major wound
        assert!(outcome.major_wound);
        assert!(outcome.knockdown);
        assert!(outcome.stunned);
    }

    #[test]
    fn test_death_check() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            50,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert!(outcome.dead); // -40 HP = beyond -4×HP = dead
    }

    #[test]
    fn test_consciousness_check_needed() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            21,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        // 10 - 21 = -11 HP. -11 <= -10 (hp_max) → penalty -1
        assert!(outcome.consciousness_roll_needed);
        assert_eq!(outcome.consciousness_penalty, -1);
    }

    #[test]
    fn test_toxic_always_multiplier_one() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Toxic,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.wounding_multiplier, 1.0);
        assert_eq!(outcome.raw_injury, 5);
    }

    #[test]
    fn test_corrosive_face_multiplier() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Face,
            3,
            DamageType::Corrosive,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.wounding_multiplier, 1.5);
    }

    #[test]
    fn test_crit_double_damage() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            4,
            DamageType::Crushing,
            Some(CritHitResult::DoubleDamage),
            true,
        )
        .unwrap();
        assert_eq!(outcome.rolled_damage, 4);
        assert_eq!(outcome.raw_injury, 8); // 4 × 2 = 8
    }

    #[test]
    fn test_crit_ignore_dr() {
        let mut target = test_target();
        target.armor.push(ArmorPiece {
            name: "Plate".into(),
            dr: {
                let mut m = HashMap::new();
                m.insert(HitLocation::Torso, 10);
                m
            },
        });
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            Some(CritHitResult::IgnoreDR),
            true,
        )
        .unwrap();
        assert_eq!(outcome.effective_dr, 0);
        assert_eq!(outcome.raw_injury, 5);
    }

    #[test]
    fn test_vitals_toxic_rejected() {
        let target = test_target();
        let state = test_state(target);
        let result = resolve_injury(
            &state,
            2,
            HitLocation::Vitals,
            5,
            DamageType::Toxic,
            None,
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_skull_inherent_dr() {
        let target = test_target();
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Skull,
            3,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.effective_dr, 2);
    }

    #[test]
    fn test_shock_modifier_applied_to_actor() {
        let target = test_target();
        let state = test_state(target);
        let (new_state, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            5,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert_eq!(outcome.shock_penalty, -4);
        let updated = new_state.actors.get(&2).unwrap();
        assert!(
            updated.individual_modifiers.iter().any(|m| m.label == "Shock (-4)"),
            "Shock modifier should be pushed to actor's individual_modifiers"
        );
    }

    #[test]
    fn test_consciousness_not_needed_when_dead() {
        let mut target = test_target();
        target.hp_max = 10;
        target.hp_current = 10;
        let state = test_state(target);
        let (_, outcome) = resolve_injury(
            &state,
            2,
            HitLocation::Torso,
            50,
            DamageType::Crushing,
            None,
            true,
        )
        .unwrap();
        assert!(outcome.dead);
        assert!(!outcome.consciousness_roll_needed);
    }
}

