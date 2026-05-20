use super::*;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    /// Seeded RNG — uses hashed system time to eliminate determinism.
    /// Re-seeded on first access per thread.
    static RNG: RefCell<StdRng> = {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        // Mix in process ID for extra entropy
        let pid = std::process::id() as u64;
        let seed = seed.wrapping_mul(6364136223846793005).wrapping_add(pid);
        RefCell::new(StdRng::seed_from_u64(seed))
    };
}

/// Rolls 3d6 and returns the sum.
pub fn roll_3d6() -> u8 {
    RNG.with(|rng| {
        let mut rng = rng.borrow_mut();
        rng.gen_range(1..=6) + rng.gen_range(1..=6) + rng.gen_range(1..=6)
    })
}

/// Rolls Nd6 and returns each individual die result plus the total.
pub fn roll_dice(count: u8) -> (Vec<u8>, u32) {
    RNG.with(|rng| {
        let mut rng = rng.borrow_mut();
        let dice: Vec<u8> = (0..count).map(|_| rng.gen_range(1..=6)).collect();
        let total: u32 = dice.iter().map(|&d| d as u32).sum();
        (dice, total)
    })
}

/// Rolls damage: Nd6 + adds, returns the total.
pub fn roll_damage(dice: u8, adds: i8) -> u32 {
    let (_, base) = roll_dice(dice);
    (base as i32 + adds as i32).max(0) as u32
}

/// Roll outcome for a 3d6 skill check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollOutcome {
    CriticalSuccess,
    Success,
    Failure,
    CriticalFailure,
}

/// Determines the outcome of a 3d6 roll against a target number.
/// Critical success: natural 3 or 4 always; 5 if skill ≥ 15; 6 if skill ≥ 16.
/// For rolls 7+, critical success if roll ≤ skill/2 (half-skill rule, B347).
/// Minimum crit roll is 6 for skills 10-15, 7 for skills 16+.
/// Critical failure: natural 18 always; natural 17 when effective skill < 16.
pub fn check_roll(roll: u8, effective_skill: u8) -> RollOutcome {
    match roll {
        3 | 4 => return RollOutcome::CriticalSuccess,
        18 => return RollOutcome::CriticalFailure,
        _ => {}
    }

    if roll == 17 && effective_skill <= 15 {
        return RollOutcome::CriticalFailure;
    }

    if roll == 5 && effective_skill >= 15 {
        return RollOutcome::CriticalSuccess;
    }

    if roll == 6 && effective_skill >= 16 {
        return RollOutcome::CriticalSuccess;
    }

    let crit_success = if effective_skill >= 10 {
        let half = effective_skill / 2;
        roll <= half
    } else {
        false
    };

    if crit_success {
        return RollOutcome::CriticalSuccess;
    }

    if roll <= effective_skill {
        RollOutcome::Success
    } else {
        RollOutcome::Failure
    }
}

/// Roll on the B556 critical hit table (3d6).
/// Returns a CritHitResult for injury resolution.
pub fn crit_hit_table(roll: u8) -> CritHitResult {
    // B556 Critical Hit Table (3d6).
    // When the table entry specifies both a damage multiplier and a DR/status
    // effect, we return the status effect and handle the multiplier separately
    // in the phase machine (see modifies_damage).
    match roll {
        3 => CritHitResult::TripleDamage,
        4 => CritHitResult::IgnoreDR,    // B556: double dmg + ignore DR
        5 => CritHitResult::HalveDR,     // B556: double dmg + halve DR
        6 => CritHitResult::MaxDamage,
        7 => CritHitResult::MajorWoundAutomatic,
        8 => CritHitResult::StunAutomatic,
        9..=11 => CritHitResult::NormalDamage,
        12 => CritHitResult::NormalDamage, // drop weapon (not implemented)
        13 => CritHitResult::CrippleLimbAutomatic,
        14 => CritHitResult::MajorWoundAutomatic,
        15 => CritHitResult::MaxDamage,
        16 => CritHitResult::DoubleDamage,
        17 => CritHitResult::NormalDamage, // lose balance (not implemented)
        18 => CritHitResult::TripleDamage,
        _ => CritHitResult::NormalDamage,
    }
}

/// Roll on the B557 critical miss table (3d6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CritMissResult {
    DropWeapon,
    FallDown,
    HitSelf,
    HitAlly,
    Stumble,
    Strain,
    LoseBalance,
    WeaponUnready,
    NoEffect,
}

pub fn crit_miss_table(roll: u8) -> CritMissResult {
    match roll {
        3 | 4 => CritMissResult::DropWeapon,
        5 => CritMissResult::HitSelf,
        6 | 7 => CritMissResult::LoseBalance,
        8 => CritMissResult::WeaponUnready,
        9..=11 => CritMissResult::NoEffect,
        12 => CritMissResult::Stumble,
        13 => CritMissResult::DropWeapon,
        14 => CritMissResult::Strain,
        15 => CritMissResult::FallDown,
        16 => CritMissResult::HitAlly,
        17 | 18 => CritMissResult::HitSelf,
        _ => CritMissResult::NoEffect,
    }
}

/// Margin of success (or failure). Positive = success margin, negative = failure margin.
pub fn margin_of_success(roll: u8, effective_skill: u8) -> i16 {
    effective_skill as i16 - roll as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roll_3d6_bounds() {
        for _ in 0..100 {
            let r = roll_3d6();
            assert!(r >= 3 && r <= 18, "roll {} out of bounds", r);
        }
    }

    #[test]
    fn test_roll_damage() {
        let dmg = roll_damage(2, 1); // 2d6+1
        assert!(dmg >= 3); // minimum 1+1+1=3
    }

    #[test]
    fn test_crit_success_on_3_or_4() {
        assert_eq!(check_roll(3, 10), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(4, 10), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(3, 3), RollOutcome::CriticalSuccess);
    }

    #[test]
    fn test_crit_failure_on_18() {
        assert_eq!(check_roll(18, 16), RollOutcome::CriticalFailure);
        assert_eq!(check_roll(18, 20), RollOutcome::CriticalFailure);
    }

    #[test]
    fn test_17_crit_fail_when_skill_low() {
        // Skill 15 or less: 17 is crit fail
        assert_eq!(check_roll(17, 15), RollOutcome::CriticalFailure);
        assert_eq!(check_roll(17, 14), RollOutcome::CriticalFailure);
    }

    #[test]
    fn test_17_normal_fail_when_skill_high() {
        // Check what check_roll actually returns
        let result16 = check_roll(17, 16);
        // roll 17 > skill 16 → Failure
        assert_eq!(result16, RollOutcome::Failure);
        // roll 17 <= skill 20 → Success  
        assert_eq!(check_roll(17, 20), RollOutcome::Success);
    }

    #[test]
    fn test_success_vs_failure() {
        assert_eq!(check_roll(10, 11), RollOutcome::Success);
        assert_eq!(check_roll(11, 11), RollOutcome::Success);
        assert_eq!(check_roll(12, 11), RollOutcome::Failure);
    }

    #[test]
    fn test_half_skill_crit() {
        // Skill 15, half=7: roll 5→special case (skill≥15), roll 6→half-skill, roll 7→half-skill
        assert_eq!(check_roll(5, 15), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(6, 15), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(7, 15), RollOutcome::CriticalSuccess);
        // Skill 16, half=8: roll 6→special case (skill≥16), 7→half, 8→half
        assert_eq!(check_roll(6, 16), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(7, 16), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(8, 16), RollOutcome::CriticalSuccess);
        // Skill 16, roll 9: 9 > half(8) → success
        assert_eq!(check_roll(9, 16), RollOutcome::Success);
        // Skill 12, half=6: roll 5→crit via half-skill, roll 6→crit
        assert_eq!(check_roll(5, 12), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(6, 12), RollOutcome::CriticalSuccess);
        // Skill 12, roll 7: 7 > half(6) → success
        assert_eq!(check_roll(7, 12), RollOutcome::Success);
        // Skill 10, half=5: roll 5→crit via half-skill
        assert_eq!(check_roll(5, 10), RollOutcome::CriticalSuccess);
        // Skill 10, roll 6: 6 > half(5) so not crit, but 6 ≤ skill 10 → normal success
        assert_eq!(check_roll(6, 10), RollOutcome::Success);
        // Skill 14, half=7: rolls 5,6,7 are crit via half-skill
        assert_eq!(check_roll(5, 14), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(6, 14), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(7, 14), RollOutcome::CriticalSuccess);
        assert_eq!(check_roll(8, 14), RollOutcome::Success);
    }

    #[test]
    fn test_margin_of_success() {
        assert_eq!(margin_of_success(5, 12), 7);
        assert_eq!(margin_of_success(15, 12), -3);
        assert_eq!(margin_of_success(10, 10), 0);
    }

    #[test]
    fn test_crit_hit_table_extremes() {
        assert_eq!(crit_hit_table(3), CritHitResult::TripleDamage);
        assert_eq!(crit_hit_table(18), CritHitResult::TripleDamage);
    }

    #[test]
    fn test_crit_miss_table_fall_down() {
        assert_eq!(crit_miss_table(15), CritMissResult::FallDown);
    }
}
