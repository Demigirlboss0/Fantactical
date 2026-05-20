use super::*;
use ManeuverType as Mt;
use ExtraEffort as Ee;

pub fn available_maneuvers(actor: &Actor) -> Vec<Mt> {
    if actor.flags.dead {
        return vec![];
    }

    if actor.flags.unconscious {
        return vec![];
    }

    if actor.flags.stunned {
        return vec![Mt::DoNothing];
    }

    if actor.flags.knocked_down {
        return vec![Mt::DoNothing, Mt::ChangePosture];
    }

    let effective_posture = if actor.both_legs_crippled() {
        Posture::Prone
    } else if actor.one_leg_crippled() {
        Posture::Kneeling
    } else {
        actor.posture
    };

    let mut maneuvers = posture_maneuvers(effective_posture);

    if actor.encumbrance == Encumbrance::ExtraHeavy {
        maneuvers.retain(|m| *m != Mt::Move && *m != Mt::Attack);
    }

    if maneuvers.contains(&Mt::Attack) && !has_any_attack(actor) {
        maneuvers.retain(|m| !is_directed_offensive(*m));
    }

    if maneuvers.contains(&Mt::Aim) && !has_ranged_weapon(actor) {
        maneuvers.retain(|m| *m != Mt::Aim);
    }

    if maneuvers.contains(&Mt::Feint) && !has_melee_skill(actor) {
        maneuvers.retain(|m| !matches!(*m, Mt::Feint | Mt::FeignBeat | Mt::FeignDefensive | Mt::FeignRuse));
    }

    if !has_shield(actor) {
        maneuvers.retain(|m| *m != Mt::AllOutDefenseDouble);
    }

    maneuvers
}

fn posture_maneuvers(posture: Posture) -> Vec<Mt> {
    let all = all_maneuvers();

    match posture {
        Posture::Standing | Posture::Crouching => all,

        Posture::Kneeling => {
            let mut m = all;
            m.retain(|x| *x != Mt::MoveAndAttack);
            m
        }

        Posture::Sitting => {
            all.into_iter()
                .filter(|x| {
                    !matches!(
                        *x,
                        Mt::Move
                            | Mt::MoveAndAttack
                            | Mt::AllOutAttackDetermined
                            | Mt::AllOutAttackDouble
                            | Mt::AllOutAttackFeint
                            | Mt::AllOutAttackLong
                            | Mt::AllOutAttackStrong
                            | Mt::AllOutAttackRangedDetermined
                            | Mt::CommittedAttackDetermined
                            | Mt::CommittedAttackStrong
                    )
                })
                .collect()
        }

        Posture::Prone => {
            all.into_iter()
                .filter(|x| {
                    !matches!(
                        *x,
                        Mt::Move
                            | Mt::MoveAndAttack
                            | Mt::AllOutAttackDetermined
                            | Mt::AllOutAttackDouble
                            | Mt::AllOutAttackFeint
                            | Mt::AllOutAttackLong
                            | Mt::AllOutAttackStrong
                            | Mt::CommittedAttackDetermined
                            | Mt::CommittedAttackStrong
                            | Mt::DefensiveAttack
                            | Mt::Feint
                            | Mt::FeignBeat
                            | Mt::FeignDefensive
                            | Mt::FeignRuse
                    )
                })
                .collect()
        }

        Posture::Crawling => {
            all.into_iter()
                .filter(|x| {
                    !matches!(
                        *x,
                        Mt::MoveAndAttack
                            | Mt::AllOutAttackDetermined
                            | Mt::AllOutAttackDouble
                            | Mt::AllOutAttackFeint
                            | Mt::AllOutAttackLong
                            | Mt::AllOutAttackStrong
                            | Mt::CommittedAttackDetermined
                            | Mt::CommittedAttackStrong
                            | Mt::DefensiveAttack
                            | Mt::Feint
                            | Mt::FeignBeat
                            | Mt::FeignDefensive
                            | Mt::FeignRuse
                    )
                })
                .collect()
        }
    }
}

fn all_maneuvers() -> Vec<Mt> {
    vec![
        Mt::Attack,
        Mt::AllOutAttackDetermined,
        Mt::AllOutAttackDouble,
        Mt::AllOutAttackFeint,
        Mt::AllOutAttackLong,
        Mt::AllOutAttackStrong,
        Mt::AllOutAttackRangedDetermined,
        Mt::CommittedAttackDetermined,
        Mt::CommittedAttackStrong,
        Mt::DefensiveAttack,
        Mt::MoveAndAttack,
        Mt::Feint,
        Mt::FeignBeat,
        Mt::FeignDefensive,
        Mt::FeignRuse,
        Mt::Evaluate,
        Mt::Aim,
        Mt::Wait,
        Mt::AllOutDefenseIncreased,
        Mt::AllOutDefenseDouble,
        Mt::AllOutDefenseMental,
        Mt::DoNothing,
        Mt::Concentrate,
        Mt::AllOutConcentrate,
        Mt::Ready,
        Mt::ChangePosture,
        Mt::Move,
    ]
}

pub fn is_directed_offensive(maneuver: Mt) -> bool {
    matches!(
        maneuver,
        Mt::Attack
            | Mt::AllOutAttackDetermined
            | Mt::AllOutAttackDouble
            | Mt::AllOutAttackFeint
            | Mt::AllOutAttackLong
            | Mt::AllOutAttackStrong
            | Mt::AllOutAttackRangedDetermined
            | Mt::CommittedAttackDetermined
            | Mt::CommittedAttackStrong
            | Mt::DefensiveAttack
            | Mt::MoveAndAttack
            | Mt::Feint
            | Mt::FeignBeat
            | Mt::FeignDefensive
            | Mt::FeignRuse
    )
}

fn has_any_attack(actor: &Actor) -> bool {
    !actor.attacks.is_empty()
}

fn has_ranged_weapon(actor: &Actor) -> bool {
    actor.attacks.iter().any(|a| a.is_ranged)
}

fn has_melee_skill(actor: &Actor) -> bool {
    actor.attacks.iter().any(|a| !a.is_ranged)
}

fn has_shield(actor: &Actor) -> bool {
    actor.armor.iter().any(|a| a.name.to_lowercase().contains("shield"))
}

pub fn combo_whitelist(maneuver: Mt) -> Vec<Ee> {
    match maneuver {
        Mt::Attack => vec![Ee::MightyBlow, Ee::RapidStrike, Ee::FlurryOfBlows, Ee::GreatLunge],
        Mt::AllOutAttackDetermined
        | Mt::AllOutAttackDouble
        | Mt::AllOutAttackFeint
        | Mt::AllOutAttackLong
        | Mt::AllOutAttackStrong
        | Mt::AllOutAttackRangedDetermined
        | Mt::CommittedAttackDetermined
        | Mt::CommittedAttackStrong => vec![],
        Mt::DefensiveAttack => vec![],
        Mt::MoveAndAttack => vec![Ee::HeroicCharge, Ee::GiantStep],
        Mt::Aim => vec![],
        Mt::Feint | Mt::FeignRuse => vec![],
        Mt::Evaluate => vec![],
        Mt::Wait => vec![],
        Mt::AllOutDefenseIncreased | Mt::AllOutDefenseDouble | Mt::AllOutDefenseMental => {
            vec![Ee::FeverishDefense]
        }
        Mt::DoNothing | Mt::Concentrate | Mt::Ready => vec![],
        Mt::Move => vec![Ee::HeroicCharge],
        Mt::ChangePosture | Mt::AllOutConcentrate | Mt::FeignBeat | Mt::FeignDefensive => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_actor() -> Actor {
        Actor {
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
                acc: None,
                rof: None,
                rcl: None,
            }],
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
        }
    }

    #[test]
    fn test_dead_actor_no_maneuvers() {
        let mut actor = test_actor();
        actor.flags.dead = true;
        assert!(available_maneuvers(&actor).is_empty());
    }

    #[test]
    fn test_unconscious_actor_no_maneuvers() {
        let mut actor = test_actor();
        actor.flags.unconscious = true;
        assert!(available_maneuvers(&actor).is_empty());
    }

    #[test]
    fn test_stunned_actor_only_do_nothing() {
        let mut actor = test_actor();
        actor.flags.stunned = true;
        assert_eq!(available_maneuvers(&actor), vec![Mt::DoNothing]);
    }

    #[test]
    fn test_knocked_down_actor() {
        let mut actor = test_actor();
        actor.flags.knocked_down = true;
        let m = available_maneuvers(&actor);
        assert!(m.contains(&Mt::DoNothing));
        assert!(m.contains(&Mt::ChangePosture));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn test_standing_actor_full_maneuvers() {
        let actor = test_actor();
        let m = available_maneuvers(&actor);
        assert!(m.contains(&Mt::Attack));
        assert!(m.contains(&Mt::Move));
        assert!(m.contains(&Mt::ChangePosture));
        assert!(m.contains(&Mt::DoNothing));
        assert!(m.contains(&Mt::Evaluate));
        assert!(m.contains(&Mt::Wait));
    }

    #[test]
    fn test_prone_posture_restrictions() {
        let mut actor = test_actor();
        actor.posture = Posture::Prone;
        // Add a ranged weapon so Aim is available
        actor.attacks.push(Attack {
            name: "Bow".into(),
            skill_level: 12,
            damage_dice: 1,
            damage_adds: 1,
            damage_type: DamageType::Impaling,
            reach: vec![],
            parry_bonus: None,
            block_bonus: None,
            is_ranged: true,
            acc: Some(2),
            rof: Some(1),
            rcl: Some(2),
        });
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::Move));
        assert!(!m.contains(&Mt::MoveAndAttack));
        assert!(!m.contains(&Mt::AllOutAttackDetermined));
        assert!(m.contains(&Mt::DoNothing));
        assert!(m.contains(&Mt::ChangePosture));
        assert!(m.contains(&Mt::Aim));
        assert!(m.contains(&Mt::Wait));
        assert!(m.contains(&Mt::AllOutAttackRangedDetermined));
    }

    #[test]
    fn test_kneeling_posture_restrictions() {
        let mut actor = test_actor();
        actor.posture = Posture::Kneeling;
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::MoveAndAttack));
        assert!(m.contains(&Mt::Attack));
        assert!(m.contains(&Mt::Move));
    }

    #[test]
    fn test_crawling_posture_restrictions() {
        let mut actor = test_actor();
        actor.posture = Posture::Crawling;
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::AllOutAttackDetermined));
        assert!(!m.contains(&Mt::Feint));
        assert!(!m.contains(&Mt::MoveAndAttack));
        assert!(m.contains(&Mt::Attack));
        assert!(m.contains(&Mt::DoNothing));
    }

    #[test]
    fn test_both_legs_crippled_forces_prone() {
        let mut actor = test_actor();
        actor.posture = Posture::Standing;
        actor.leg_state.left = LimbStatus::Crippled;
        actor.leg_state.right = LimbStatus::Crippled;
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::Move));
        assert!(!m.contains(&Mt::AllOutAttackDetermined));
    }

    #[test]
    fn test_one_leg_crippled_forces_kneeling() {
        let mut actor = test_actor();
        actor.posture = Posture::Standing;
        actor.leg_state.right = LimbStatus::Crippled;
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::MoveAndAttack));
        assert!(m.contains(&Mt::Attack));
    }

    #[test]
    fn test_extra_heavy_encumbrance() {
        let mut actor = test_actor();
        actor.encumbrance = Encumbrance::ExtraHeavy;
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::Move));
        assert!(!m.contains(&Mt::Attack));
    }

    #[test]
    fn test_no_attacks_removes_offensive_maneuvers() {
        let mut actor = test_actor();
        actor.attacks.clear();
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::Attack));
        assert!(!m.contains(&Mt::AllOutAttackDetermined));
        assert!(!m.contains(&Mt::Feint));
        assert!(m.contains(&Mt::Move));
        assert!(m.contains(&Mt::ChangePosture));
    }

    #[test]
    fn test_combo_whitelist_attack() {
        let combos = combo_whitelist(Mt::Attack);
        assert!(combos.contains(&Ee::MightyBlow));
        assert!(combos.contains(&Ee::RapidStrike));
    }

    #[test]
    fn test_combo_whitelist_all_out_attack_empty() {
        assert!(combo_whitelist(Mt::AllOutAttackDetermined).is_empty());
        assert!(combo_whitelist(Mt::AllOutAttackDouble).is_empty());
    }

    #[test]
    fn test_combo_whitelist_wait_empty() {
        assert!(combo_whitelist(Mt::Wait).is_empty());
    }

    #[test]
    fn test_combo_whitelist_aim_empty() {
        assert!(combo_whitelist(Mt::Aim).is_empty());
    }

    #[test]
    fn test_ranged_only_actor_no_feint() {
        let mut actor = test_actor();
        actor.attacks = vec![Attack {
            name: "Bow".into(),
            skill_level: 12,
            damage_dice: 1,
            damage_adds: 1,
            damage_type: DamageType::Impaling,
            reach: vec![],
            parry_bonus: None,
            block_bonus: None,
            is_ranged: true,
            acc: Some(2),
            rof: Some(1),
            rcl: Some(2),
        }];
        let m = available_maneuvers(&actor);
        assert!(!m.contains(&Mt::Feint), "Feint should not be available without a melee skill");
        assert!(!m.contains(&Mt::FeignBeat));
        assert!(!m.contains(&Mt::FeignDefensive));
        assert!(!m.contains(&Mt::FeignRuse));
    }
}
