use super::*;
use serde_json::Value;

pub fn import_gcs_json(json_str: &str) -> anyhow::Result<Actor> {
    let root: Value = serde_json::from_str(json_str)?;

    let profile = root
        .get("profile")
        .ok_or_else(|| anyhow::anyhow!("Missing profile"))?;
    let name = profile
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let gender = profile.get("gender").and_then(|v| v.as_str()).unwrap_or("");
    let is_male = !gender.to_lowercase().contains("female");

    let sm = extract_sm(&root).unwrap_or(0);

    let st = extract_attr_value(&root, "st").unwrap_or(10);
    let dx = extract_attr_value(&root, "dx").unwrap_or(10);
    let iq = extract_attr_value(&root, "iq").unwrap_or(10);
    let ht = extract_attr_value(&root, "ht").unwrap_or(10);

    let hp_max = extract_calc_value(&root, "hp").unwrap_or(st as i16);
    let fp_max = extract_calc_value(&root, "fp").unwrap_or(ht as i16);

    let hp_current = extract_current_value(&root, "hp").unwrap_or(hp_max);
    let fp_current = extract_current_value(&root, "fp").unwrap_or(fp_max);

    let portrait_data = extract_portrait(&root);

    let will = extract_attr_value(&root, "will").unwrap_or(iq);
    let per = extract_attr_value(&root, "per").unwrap_or(iq);

    let basic_speed =
        extract_calc_float(&root, "basic_speed").unwrap_or((ht as f32 + dx as f32) / 4.0);
    let basic_move_val =
        extract_calc_value(&root, "basic_move").unwrap_or((basic_speed.floor() as i16).max(1));

    let skills = extract_skills(&root);
    let (attacks, armor) = extract_combat_gear(&root);

    let actor = Actor {
        id: 0,
        name,
        portrait_path: None,
        portrait_data,
        source_path: None,
        is_npc: false,
        st,
        dx,
        iq,
        ht,
        hp_max,
        fp_max,
        basic_speed,
        basic_move: basic_move_val as u8,
        will,
        per,
        attacks,
        skills,
        armor,
        sm,
        is_male,
        position: (0, 0),
        hp_current,
        fp_current,
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

    Ok(actor)
}

fn extract_attr_value(root: &Value, attr_id: &str) -> Option<u8> {
    root.get("attributes")?
        .as_array()?
        .iter()
        .find(|attr| attr.get("attr_id").and_then(|v| v.as_str()) == Some(attr_id))
        .and_then(|attr| attr.pointer("/calc/value"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
}

fn extract_calc_value(root: &Value, attr_id: &str) -> Option<i16> {
    root.get("attributes")?
        .as_array()?
        .iter()
        .find(|attr| attr.get("attr_id").and_then(|v| v.as_str()) == Some(attr_id))
        .and_then(|attr| attr.pointer("/calc/value"))
        .and_then(|v| v.as_i64())
        .map(|v| v as i16)
}

fn extract_calc_float(root: &Value, attr_id: &str) -> Option<f32> {
    root.get("attributes")?
        .as_array()?
        .iter()
        .find(|attr| attr.get("attr_id").and_then(|v| v.as_str()) == Some(attr_id))
        .and_then(|attr| attr.pointer("/calc/value"))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
}

fn extract_sm(root: &Value) -> Option<i8> {
    root.get("attributes")?
        .as_array()?
        .iter()
        .find(|attr| attr.get("attr_id").and_then(|v| v.as_str()) == Some("sm"))
        .and_then(|attr| attr.pointer("/calc/value"))
        .and_then(|v| v.as_i64())
        .map(|v| v as i8)
}

fn extract_current_value(root: &Value, attr_id: &str) -> Option<i16> {
    root.get("attributes")?
        .as_array()?
        .iter()
        .find(|attr| attr.get("attr_id").and_then(|v| v.as_str()) == Some(attr_id))
        .and_then(|attr| attr.pointer("/calc/current"))
        .and_then(|v| v.as_i64())
        .map(|v| v as i16)
}

fn extract_portrait(root: &Value) -> Option<Vec<u8>> {
    let b64 = root.get("profile")?.get("portrait")?.as_str()?;
    if b64.is_empty() {
        return None;
    }
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

fn extract_skills(root: &Value) -> Vec<Skill> {
    let mut skills = Vec::new();
    if let Some(skills_array) = root.get("skills").and_then(|v| v.as_array()) {
        collect_skills(skills_array, &mut skills);
    }
    skills
}

fn collect_skills(items: &[Value], skills: &mut Vec<Skill>) {
    for skill in items {
        let name = skill
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }

        let level = skill
            .pointer("/calc/level")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let has_children = skill.get("children").and_then(|v| v.as_array()).is_some();

        if level > 0 && !has_children {
            let diff_str = skill
                .get("difficulty")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let difficulty = parse_difficulty(diff_str);

            let rsl = skill
                .pointer("/calc/rsl")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let relative_level = parse_relative_level(rsl);

            let controlling_attribute = if let Some(attr_part) = diff_str.split('/').next() {
                attr_part.to_string()
            } else {
                String::new()
            };

            skills.push(Skill {
                name,
                level,
                relative_level,
                difficulty,
                controlling_attribute,
            });
        }

        if let Some(children) = skill.get("children").and_then(|v| v.as_array()) {
            collect_skills(children, skills);
        }
    }
}

fn parse_difficulty(type_str: &str) -> SkillDifficulty {
    let lower = type_str.to_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    if parts.len() >= 2 {
        match parts[1] {
            "e" => SkillDifficulty::Easy,
            "a" => SkillDifficulty::Average,
            "h" => SkillDifficulty::Hard,
            "vh" => SkillDifficulty::VeryHard,
            _ => SkillDifficulty::Average,
        }
    } else {
        SkillDifficulty::Average
    }
}

fn parse_relative_level(rl: &str) -> i8 {
    if rl.is_empty() || rl == "0" {
        return 0;
    }
    let rl = rl.trim();
    if let Some(num_str) = rl.strip_prefix("IQ+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("IQ-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("DX+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("DX-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("HT+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("HT-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("ST+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("ST-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("Per+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("Per-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("Will+") {
        num_str.parse().unwrap_or(0)
    } else if let Some(num_str) = rl.strip_prefix("Will-") {
        -num_str.parse::<i8>().unwrap_or(0)
    } else {
        rl.parse().unwrap_or(0)
    }
}

fn extract_combat_gear(root: &Value) -> (Vec<Attack>, Vec<ArmorPiece>) {
    let mut attacks = Vec::new();
    let mut armor = Vec::new();

    if let Some(traits) = root.get("traits").and_then(|v| v.as_array()) {
        for trait_item in traits {
            if let Some(weapons) = trait_item.get("weapons").and_then(|v| v.as_array()) {
                for weapon in weapons {
                    if let Some(attack) = parse_weapon(weapon) {
                        attacks.push(attack);
                    }
                }
            }
        }
    }

    if let Some(equipment) = root.get("equipment").and_then(|v| v.as_array()) {
        fn collect_equipment_weapons(
            items: &[Value],
            attacks: &mut Vec<Attack>,
            armor: &mut Vec<ArmorPiece>,
        ) {
            for item in items {
                let equipped = item
                    .get("equipped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !equipped && item.get("children").is_none() {
                    continue;
                }

                let parent_desc = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Direct weapons on this equipment item
                if let Some(weapons) = item.get("weapons").and_then(|v| v.as_array()) {
                    for weapon in weapons {
                        if let Some(attack) = parse_weapon_with_desc(weapon, parent_desc) {
                            attacks.push(attack);
                        }
                    }
                }

                // Container children (e.g. Signature Gear, Loadout containers)
                if let Some(children) = item.get("children").and_then(|v| v.as_array()) {
                    collect_equipment_weapons(children, attacks, armor);
                }

                // Armor features
                if let Some(features) = item.get("features").and_then(|v| v.as_array()) {
                    for feature in features {
                        let feat_type = feature.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if feat_type == "dr_bonus" {
                            if let Some(piece) = parse_armor_piece(item, feature) {
                                armor.push(piece);
                            }
                        }
                    }
                }
            }
        }

        collect_equipment_weapons(equipment, &mut attacks, &mut armor);
    }

    (attacks, armor)
}

fn parse_first_number(s: &str) -> Option<u8> {
    s.parse::<u8>().ok().or_else(|| {
        s.split(|c: char| !c.is_ascii_digit())
            .find(|part| !part.is_empty())
            .and_then(|p| p.parse::<u8>().ok())
    })
}

fn parse_weapon_with_desc(weapon: &Value, parent_desc: &str) -> Option<Attack> {
    parse_weapon_impl(weapon, Some(parent_desc))
}

fn parse_weapon(weapon: &Value) -> Option<Attack> {
    parse_weapon_impl(weapon, None)
}

fn parse_weapon_impl(weapon: &Value, parent_desc: Option<&str>) -> Option<Attack> {
    let usage = weapon
        .get("usage")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let desc = weapon
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Prefer parent description (equipment item name) over weapon description
    let effective_desc = parent_desc.unwrap_or(desc);

    let name = if !effective_desc.is_empty() && !usage.is_empty() {
        format!("{} ({})", effective_desc, usage)
    } else if !effective_desc.is_empty() {
        effective_desc.to_string()
    } else if !usage.is_empty() {
        usage.clone()
    } else {
        weapon
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed")
            .to_string()
    };

    let calc_level = weapon
        .pointer("/calc/level")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    let calc_damage = weapon
        .pointer("/calc/damage")
        .and_then(|v| v.as_str())
        .unwrap_or("0");

    let (dice, adds, damage_type) = parse_damage_string(calc_damage);

    let reach = parse_reach(weapon.get("reach").and_then(|v| v.as_str()).unwrap_or(""));

    let parry_str = weapon.get("parry").and_then(|v| v.as_str()).unwrap_or("");
    let calc_parry = weapon
        .pointer("/calc/parry")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<i8>().ok());

    let parry_bonus = match (parry_str, calc_parry) {
        ("0", _) | ("No", _) => None,
        (_, Some(p)) => Some(p),
        _ => None,
    };

    let acc = weapon
        .get("accuracy")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u8>().ok());

    let rof = weapon
        .get("rate_of_fire")
        .and_then(|v| v.as_str())
        .and_then(parse_first_number)
        .or_else(|| {
            weapon
                .get("rof")
                .and_then(|v| v.as_str())
                .and_then(parse_first_number)
        });

    let rcl = weapon
        .get("recoil")
        .and_then(|v| v.as_str())
        .and_then(parse_first_number)
        .or_else(|| {
            weapon
                .get("rcl")
                .and_then(|v| v.as_str())
                .and_then(parse_first_number)
        });

    let is_ranged = acc.is_some() || rof.is_some() || rcl.is_some();

    if calc_level == 0 {
        return None;
    }

    Some(Attack {
        name,
        skill_level: calc_level,
        damage_dice: dice,
        damage_adds: adds,
        damage_type,
        reach,
        parry_bonus,
        block_bonus: None,
        is_ranged,
        acc,
        rof,
        rcl,
    })
}

fn parse_damage_string(damage: &str) -> (u8, i8, DamageType) {
    let parts: Vec<&str> = damage.split_whitespace().collect();
    if parts.len() < 2 {
        return (0, 0, DamageType::Crushing);
    }

    let dice_str = parts[0];
    let (dice, adds) = if let Some(idx) = dice_str.find('+') {
        let d = dice_str[..idx].trim_end_matches('d');
        let a = &dice_str[idx + 1..];
        (d.parse().unwrap_or(0), a.parse().unwrap_or(0))
    } else if let Some(idx) = dice_str.find('-') {
        let d = dice_str[..idx].trim_end_matches('d');
        let a = &dice_str[idx..];
        (d.parse().unwrap_or(0), a.parse().unwrap_or(0))
    } else {
        let d = dice_str.trim_end_matches('d');
        (d.parse().unwrap_or(0), 0)
    };

    let type_str = parts.last().unwrap_or(&"cr");
    let damage_type = match *type_str {
        "cr" => DamageType::Crushing,
        "cut" => DamageType::Cutting,
        "imp" => DamageType::Impaling,
        "pi-" => DamageType::SmallPiercing,
        "pi" => DamageType::Piercing,
        "pi+" => DamageType::LargePiercing,
        "pi++" => DamageType::HugePiercing,
        "burn" | "burning" => DamageType::Burning,
        "tox" => DamageType::Toxic,
        "cor" => DamageType::Corrosive,
        "fat" => DamageType::FatigueDmg,
        _ => DamageType::Crushing,
    };

    (dice, adds, damage_type)
}

fn parse_reach(reach_str: &str) -> Vec<u8> {
    if reach_str.is_empty() {
        return vec![1];
    }
    let mut result = Vec::new();
    for part in reach_str.split(',') {
        let part = part.trim();
        if part == "C" {
            result.push(1);
        } else if let Some(idx) = part.find('-') {
            let lo: u8 = part[..idx].parse().unwrap_or(1);
            let hi: u8 = part[idx + 1..].parse().unwrap_or(lo);
            for v in lo..=hi {
                result.push(v);
            }
        } else if let Ok(v) = part.parse() {
            result.push(v);
        }
    }
    result
}

fn parse_armor_piece(item: &Value, feature: &Value) -> Option<ArmorPiece> {
    let description = item
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown Armor")
        .to_string();

    let amount = feature.get("amount").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
    if amount == 0 {
        return None;
    }

    let locations = feature.get("locations").and_then(|v| v.as_array());
    if locations.is_none() || locations?.is_empty() {
        return None;
    }

    let mut dr = HashMap::new();
    for loc in locations.unwrap() {
        let loc_str = loc.as_str().unwrap_or("");
        let hit_loc = match loc_str {
            "skull" => HitLocation::Skull,
            "face" => HitLocation::Face,
            "eyes" => HitLocation::Eye,
            "neck" => HitLocation::Neck,
            "torso" => HitLocation::Torso,
            "vitals" => HitLocation::Vitals,
            "groin" => HitLocation::Groin,
            "left arm" | "arms" => HitLocation::LeftArm,
            "right arm" => HitLocation::RightArm,
            "left hand" => HitLocation::LeftHand,
            "right hand" => HitLocation::RightHand,
            "left leg" | "legs" => HitLocation::LeftLeg,
            "right leg" => HitLocation::RightLeg,
            "left foot" => HitLocation::LeftFoot,
            "right foot" | "foot" => HitLocation::RightFoot,
            "abdomen" => HitLocation::Abdomen,
            _ => continue,
        };
        dr.insert(hit_loc, amount);
    }

    if dr.is_empty() {
        return None;
    }

    Some(ArmorPiece {
        name: description,
        dr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_damage_string_basic() {
        let (dice, adds, dt) = parse_damage_string("2d+1 imp");
        assert_eq!(dice, 2);
        assert_eq!(adds, 1);
        assert_eq!(dt, DamageType::Impaling);
    }

    #[test]
    fn test_parse_damage_string_penalty() {
        let (dice, adds, dt) = parse_damage_string("1d-1 cr");
        assert_eq!(dice, 1);
        assert_eq!(adds, -1);
        assert_eq!(dt, DamageType::Crushing);
    }

    #[test]
    fn test_parse_reach() {
        let reach = parse_reach("1-2");
        assert_eq!(reach, vec![1, 2]);
        let reach2 = parse_reach("C,1");
        assert_eq!(reach2, vec![1, 1]);
        let reach3 = parse_reach("C");
        assert_eq!(reach3, vec![1]);
    }

    #[test]
    fn test_parse_difficulty() {
        assert_eq!(parse_difficulty("iq/a"), SkillDifficulty::Average);
        assert_eq!(parse_difficulty("dx/e"), SkillDifficulty::Easy);
        assert_eq!(parse_difficulty("IQ/H"), SkillDifficulty::Hard);
        assert_eq!(parse_difficulty("iq/vh"), SkillDifficulty::VeryHard);
    }

    #[test]
    fn test_parse_relative_level() {
        assert_eq!(parse_relative_level("IQ+2"), 2);
        assert_eq!(parse_relative_level("IQ-1"), -1);
        assert_eq!(parse_relative_level("DX+0"), 0);
        assert_eq!(parse_relative_level(""), 0);
    }

    #[test]
    fn test_import_gcs_example_sheet() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Francesca Vanorder.gcs");
        let json_str = std::fs::read_to_string(path).expect("Failed to read GCS file");
        let actor = import_gcs_json(&json_str).expect("Failed to import GCS JSON");

        assert_eq!(actor.name, "Francesca Vanorder");
        assert_eq!(actor.st, 11);
        assert_eq!(actor.dx, 12);
        assert_eq!(actor.iq, 16);
        assert_eq!(actor.ht, 12);
        assert_eq!(actor.hp_max, 15);
        assert_eq!(actor.fp_max, 12);
        assert_eq!(actor.will, 16);
        assert_eq!(actor.per, 16);
        assert!((actor.basic_speed - 6.0).abs() < 0.1);
        assert_eq!(actor.basic_move, 6);
        assert_eq!(actor.sm, 0);
        assert!(!actor.is_male);

        // Francesca has 10 equipment weapons + 3 natural attacks
        let names: Vec<&str> = actor.attacks.iter().map(|a| a.name.as_str()).collect();
        assert!(
            actor.attacks.len() >= 10,
            "Expected >=10 attacks, got {}: {:?}",
            actor.attacks.len(),
            names
        );
        assert!(actor.skills.len() > 0, "Should have at least one skill");
        assert!(
            actor.armor.len() > 0,
            "Should have at least one armor piece"
        );

        // Verify a ranged weapon was imported
        let has_ranged = actor.attacks.iter().any(|a| a.is_ranged);
        assert!(
            has_ranged,
            "Expected at least one ranged weapon (Ruger Mini-14 or Reutech Protecta)"
        );

        // Verify equipment weapon naming
        let has_named_weapon = names
            .iter()
            .any(|n| n.contains("Ruger") || n.contains("Reutech"));
        assert!(
            has_named_weapon,
            "Expected equipment weapon names to include 'Ruger' or 'Reutech', got: {:?}",
            names
        );
    }

    #[test]
    fn test_import_nur_sheet() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Nur.gcs");
        let json_str = std::fs::read_to_string(path).expect("Failed to read Nur GCS file");
        let actor = import_gcs_json(&json_str).expect("Failed to import Nur GCS JSON");

        assert_eq!(actor.name, "Nur");
        assert!(
            actor.attacks.len() >= 10,
            "Nur should have >=10 attacks, got {}",
            actor.attacks.len()
        );

        let has_ranged = actor.attacks.iter().any(|a| a.is_ranged);
        assert!(
            has_ranged,
            "Nur should have ranged weapons (Desert Eagle, Benelli M1, Alexander .50)"
        );

        let names: Vec<&str> = actor.attacks.iter().map(|a| a.name.as_str()).collect();
        let has_melee = names
            .iter()
            .any(|n| n.contains("Greatsword") || n.contains("Lance"));
        assert!(has_melee, "Nur should have melee weapons, got: {:?}", names);
    }
}
