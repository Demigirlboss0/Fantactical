# GCS Import

`src/model/gcs_import.rs` (632 lines) parses `.gcs` JSON character sheets and
produces an `Actor` struct.

## Entry Point

```rust
pub fn import_gcs_json(json_str: &str) -> anyhow::Result<Actor>
```

## Extraction Pipeline

### Attributes
Parsed from `root.attributes[]` by `attr_id`:
- `st`, `dx`, `iq`, `ht` → `u8` from `calc.value`
- `hp`, `fp` → `i16` from `calc.value`
- `hp_current`, `fp_current` → `i16` from `calc.current`
- `will`, `per`, `basic_speed`, `basic_move`, `sm` → from respective attrs

### Skills
`extract_skills()` (`src/model/gcs_import.rs:147`) iterates `skills[]` array
recursively via `collect_skills()`. Each skill extracts:
- `name`, `calc.level` (u8)
- `difficulty` → `SkillDifficulty` (Easy/Average/Hard/VeryHard)
- `calc.rsl` → `relative_level` (e.g., "IQ+2" → 2)
- Children processed recursively for technique trees

### Combat Gear
`extract_combat_gear()` (`src/model/gcs_import.rs:257`) returns
`(Vec<Attack>, Vec<ArmorPiece>)`.

#### Trait Weapons
Iterates `traits[]` → items with `weapons[]` arrays (e.g., Natural Attacks).
Each weapon is parsed by `parse_weapon()`.

#### Equipment Weapons
Iterates `equipment[]` → items with `weapons[]` arrays AND equipped items
with `children[]` containers (e.g., Signature Gear containing weapons).

Recursive `collect_equipment_weapons()` function traverses:
1. Direct `weapons[]` on the equipment item
2. `children[]` arrays recursively (containers)
3. `features[]` for `dr_bonus` armor features

#### Weapon Parsing (`parse_weapon_impl()` at line 352)

| GCS Field | Attack Field | Notes |
|-----------|-------------|-------|
| `usage` | Used in name | e.g., "Swing", "Thrust", "Punch" |
| `description` (parent) | Name prefix | Equipment name used in `parse_weapon_with_desc()` |
| `calc.level` | `skill_level` | Effective skill |
| `calc.damage` | `damage_dice`, `damage_adds`, `damage_type` | Via `parse_damage_string()` |
| `reach` | `reach` | Via `parse_reach()` |
| `parry` + `calc.parry` | `parry_bonus` | "0" or "No" → None |
| `accuracy` | `acc` | Ranged only |
| `rate_of_fire` / `rof` | `rof` | Via `parse_first_number()` |
| `recoil` / `rcl` | `rcl` | Via `parse_first_number()` |
| (auto) | `is_ranged` | True if `acc`, `rof`, or `rcl` is Some |

#### Weapon Naming
- Equipment weapons: `"{description} ({usage})"` — e.g., "Ruger Mini-14, .223 Remington (Punch)"
- Trait weapons: `"{usage}"` — e.g., "Punch", "Kick", "Bite"
- No `description` or `usage`: falls back to `id`

#### Damage String Parsing
`parse_damage_string()` (`src/model/gcs_import.rs:420`) handles:
- `"2d+1 imp"` → (2 dice, +1, Impaling)
- `"1d-1 cr"` → (1 die, -1, Crushing)
- `"5d pi"` → (5 dice, +0, Piercing)
- Type codes: `cr`, `cut`, `imp`, `pi-`, `pi`, `pi+`, `pi++`, `burn`, `tox`, `cor`, `fat`

#### Reach Parsing
`parse_reach()` (`src/model/gcs_import.rs:459`) handles:
- `"1"` → `[1]`
- `"1-2"` → `[1, 2]`
- `"C"` → `[1]` (close combat)
- `"C,1-2"` → `[1, 1, 2]`

#### Ranged Stat Parsing
`parse_first_number()` (`src/model/gcs_import.rs:383`) handles compound values:
- `"3"` → 3
- `"2x9"` → 2 (rate of fire, first number)
- `"1/4"` → 1 (recoil, first number)

Fields checked: `rate_of_fire` (GCS canonical name) and `rof` (shorthand),
`recoil` (GCS canonical) and `rcl` (shorthand).

### Armor Parsing
`parse_armor_piece()` (`src/model/gcs_import.rs:481`) maps GCS location strings
to `HitLocation` variants:

| GCS String | HitLocation |
|-----------|-------------|
| `skull` | Skull |
| `face` | Face |
| `eyes` | Eye |
| `neck` | Neck |
| `torso` | Torso |
| `vitals` | Vitals |
| `groin` | Groin |
| `left arm`, `arms` | LeftArm |
| `right arm` | RightArm |
| `left hand` | LeftHand |
| `right hand` | RightHand |
| `left leg`, `legs` | LeftLeg |
| `right leg` | RightLeg |
| `left foot` | LeftFoot |
| `right foot`, `foot` | RightFoot |
| `abdomen` | Abdomen |

### Portrait
Base64-encoded portrait data extracted from `profile.portrait` and decoded
into `Vec<u8>` for later rendering as an egui texture.

## Known Limitations

- Armor DR mapping does not cover all 28 hit locations (missing: Spine,
  DigestiveTract, Heart, vascular/joint locations)
- Equipment `type` field is not used for weapon detection — `weapons[]`
  array presence is the indicator
- Container children are traversed but not deeply validated for equipment
  state correctness
- `calc_level == 0` weapons are silently skipped (intentional — Follow-Up
  attacks without independent skill)

## Tests

`src/model/gcs_import.rs` tests (7 total):
- `test_parse_damage_string_basic` / `_penalty` — damage string parsing
- `test_parse_reach` — reach string parsing
- `test_parse_difficulty` / `test_parse_relative_level` — skill parsing
- `test_import_gcs_example_sheet` — full Francesca Vanorder import with
  assertions on attributes, attacks (≥10), ranged weapons, equipment naming
- `test_import_nur_sheet` — full Nur import with ≥10 attacks, ranged
  weapons, melee weapon naming
