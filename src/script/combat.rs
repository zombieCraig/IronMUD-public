// src/script/combat.rs
// Combat system functions: state management, zone checks, body parts, wounds, death/corpse/respawn

use crate::db::Db;
use crate::{
    ActiveBuff, BodyPart, CombatDistance, CombatState, CombatTarget, CombatTargetType, CombatZoneType, EffectType,
    MobileData, OngoingEffect, WeaponSkill, Wound, WoundLevel, WoundType,
};
use crate::{ItemData, ItemFlags, ItemLocation, ItemType, LiquidType, STARTING_ROOM_ID, WearLocation};
use rand::Rng;
use rhai::{Dynamic, Engine, Map};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Append on-hit DoT effects driven by mobile flags. Each enabled flag pushes a 3-round
/// `OngoingEffect` of the matching element with `damage_per_round = max(1, mobile.level / 2)`.
/// Flags compose — a mobile with both `poisonous` and `fiery` applies both DoTs.
pub fn apply_mobile_on_hit_dots(mobile: &MobileData, effects: &mut Vec<OngoingEffect>, body_part: &str) {
    let dpr = (mobile.level / 2).max(1);
    let rounds = 3;
    let mut push = |kind: &str| {
        effects.push(OngoingEffect {
            effect_type: kind.to_string(),
            rounds_remaining: rounds,
            damage_per_round: dpr,
            body_part: body_part.to_string(),
        });
    };
    if mobile.flags.poisonous {
        push("poison");
    }
    if mobile.flags.fiery {
        push("fire");
    }
    if mobile.flags.chilling {
        push("cold");
    }
    if mobile.flags.corrosive {
        push("acid");
    }
    if mobile.flags.shocking {
        push("lightning");
    }
}

/// Returns `damage` reduced by the highest-magnitude active `DamageReduction` buff,
/// or unchanged if none. Floors at 1 to preserve the "you got hit" feedback.
/// Magnitude is treated as a percentage (0..=95 expected).
pub fn apply_damage_reduction(damage: i32, buffs: &[ActiveBuff]) -> i32 {
    if damage <= 0 {
        return damage;
    }
    let mag = buffs
        .iter()
        .filter(|b| b.effect_type == EffectType::DamageReduction)
        .map(|b| b.magnitude.clamp(0, 95))
        .max()
        .unwrap_or(0);
    if mag == 0 {
        return damage;
    }
    ((damage as i64 * (100 - mag) as i64) / 100).max(1) as i32
}

#[cfg(test)]
mod damage_reduction_tests {
    use super::*;

    fn buff(mag: i32) -> ActiveBuff {
        ActiveBuff {
            effect_type: EffectType::DamageReduction,
            magnitude: mag,
            remaining_secs: -1,
            source: "test".to_string(),
        }
    }

    #[test]
    fn no_buffs_returns_damage_unchanged() {
        assert_eq!(apply_damage_reduction(20, &[]), 20);
    }

    #[test]
    fn unrelated_buff_does_nothing() {
        let b = ActiveBuff {
            effect_type: EffectType::Haste,
            magnitude: 50,
            remaining_secs: -1,
            source: "test".to_string(),
        };
        assert_eq!(apply_damage_reduction(20, &[b]), 20);
    }

    #[test]
    fn fifty_percent_halves_damage() {
        assert_eq!(apply_damage_reduction(20, &[buff(50)]), 10);
    }

    #[test]
    fn highest_magnitude_wins() {
        assert_eq!(apply_damage_reduction(100, &[buff(25), buff(50)]), 50);
    }

    #[test]
    fn floors_at_one() {
        assert_eq!(apply_damage_reduction(1, &[buff(95)]), 1);
    }

    #[test]
    fn magnitude_above_95_clamps() {
        // magnitude clamped to 95, so 100 dmg * 5% = 5
        assert_eq!(apply_damage_reduction(100, &[buff(120)]), 5);
    }
}

/// Register combat-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Type Registrations ==========

    // Register CombatTargetType enum
    engine.register_type_with_name::<CombatTargetType>("CombatTargetType");

    // Register CombatTarget struct with getters
    engine
        .register_type_with_name::<CombatTarget>("CombatTarget")
        .register_get("target_type", |t: &mut CombatTarget| {
            t.target_type.to_display_string().to_string()
        })
        .register_get("target_id", |t: &mut CombatTarget| t.target_id.to_string());

    // Register CombatState struct with getters
    engine
        .register_type_with_name::<CombatState>("CombatState")
        .register_get("in_combat", |s: &mut CombatState| s.in_combat)
        .register_get("stun_rounds_remaining", |s: &mut CombatState| {
            s.stun_rounds_remaining as i64
        })
        .register_get("targets", |s: &mut CombatState| {
            s.targets
                .iter()
                .map(|t| rhai::Dynamic::from(t.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("reloading", |s: &mut CombatState| s.reloading);

    // ========== Reloading State Functions ==========

    // set_reloading(char_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_reloading", move |char_name: String, value: bool| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.combat.reloading = value;
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // set_mobile_reloading(mobile_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_reloading", move |mobile_id: String, value: bool| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.combat.reloading = value;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // ========== Zone Check Functions ==========

    // Helper to get effective combat zone for a room
    fn get_effective_zone(db: &Db, room_id: &str) -> CombatZoneType {
        if let Ok(uuid) = uuid::Uuid::parse_str(room_id) {
            if let Ok(Some(room)) = db.get_room_data(&uuid) {
                // Room override takes precedence
                if let Some(room_zone) = room.flags.combat_zone {
                    return room_zone;
                }
                // Fall back to area zone
                if let Some(area_id) = room.area_id {
                    if let Ok(Some(area)) = db.get_area_data(&area_id) {
                        return area.combat_zone;
                    }
                }
            }
        }
        CombatZoneType::Pve // Default: PvE (can attack mobiles, not players)
    }

    // get_combat_zone(room_id) -> String
    // Returns the effective combat zone type: "pve", "safe", or "pvp"
    let cloned_db = db.clone();
    engine.register_fn("get_combat_zone", move |room_id: String| -> String {
        get_effective_zone(&cloned_db, &room_id).to_display_string().to_string()
    });

    // can_attack_mobiles(room_id) -> bool
    // Returns true if players can attack mobiles in this location (PvE or PvP zones)
    let cloned_db = db.clone();
    engine.register_fn("can_attack_mobiles", move |room_id: String| -> bool {
        get_effective_zone(&cloned_db, &room_id).can_attack_mobiles()
    });

    // can_attack_players(room_id) -> bool
    // Returns true if players can attack other players in this location (PvP zones only)
    let cloned_db = db.clone();
    engine.register_fn("can_attack_players", move |room_id: String| -> bool {
        get_effective_zone(&cloned_db, &room_id).can_attack_players()
    });

    // ========== Body Part Functions ==========

    // get_all_body_parts() -> Array<String>
    // Returns list of all body parts
    engine.register_fn("get_all_body_parts", || -> rhai::Array {
        BodyPart::all()
            .iter()
            .map(|p| Dynamic::from(p.to_display_string().to_string()))
            .collect()
    });

    // is_vital_body_part(part) -> bool
    // Returns true if body part is vital (head, neck, torso)
    engine.register_fn("is_vital_body_part", |part: String| -> bool {
        BodyPart::from_str(&part).map(|p| p.is_vital()).unwrap_or(false)
    });

    // get_body_part_hit_weight(part) -> i64
    // Returns the hit weight for targeting calculations
    engine.register_fn("get_body_part_hit_weight", |part: String| -> i64 {
        BodyPart::from_str(&part).map(|p| p.hit_weight() as i64).unwrap_or(0)
    });

    // roll_random_body_part() -> String
    // Returns a weighted random body part for targeting
    engine.register_fn("roll_random_body_part", || -> String {
        let mut rng = rand::thread_rng();
        let total_weight: u32 = BodyPart::all().iter().map(|p| p.hit_weight()).sum();
        let roll = rng.gen_range(0..total_weight);

        let mut cumulative = 0u32;
        for part in BodyPart::all() {
            cumulative += part.hit_weight();
            if roll < cumulative {
                return part.to_display_string().to_string();
            }
        }
        // Fallback (shouldn't happen)
        "torso".to_string()
    });

    // ========== Wound Level Functions ==========

    // get_all_wound_levels() -> Array<String>
    engine.register_fn("get_all_wound_levels", || -> rhai::Array {
        vec!["none", "minor", "moderate", "severe", "critical", "disabled"]
            .into_iter()
            .map(|s| Dynamic::from(s.to_string()))
            .collect()
    });

    // get_wound_level_penalty(level) -> i64
    // Returns the penalty percentage for a wound level (0-100)
    engine.register_fn("get_wound_level_penalty", |level: String| -> i64 {
        match WoundLevel::from_str(&level) {
            Some(l) => l.penalty() as i64,
            None => 0,
        }
    });

    // compare_wound_levels(a, b) -> i64
    // Returns -1 if a < b, 0 if a == b, 1 if a > b
    engine.register_fn("compare_wound_levels", |a: String, b: String| -> i64 {
        let level_a = WoundLevel::from_str(&a).unwrap_or(WoundLevel::None);
        let level_b = WoundLevel::from_str(&b).unwrap_or(WoundLevel::None);
        match level_a.cmp(&level_b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    });

    // escalate_wound_level(level) -> String
    // Returns the next worse wound level, or "disabled" if already at max
    engine.register_fn("escalate_wound_level", |level: String| -> String {
        match WoundLevel::from_str(&level) {
            Some(l) => l.escalate().to_display_string().to_string(),
            None => "minor".to_string(),
        }
    });

    // ========== Wound Type Functions ==========

    // get_all_wound_types() -> Array<String>
    engine.register_fn("get_all_wound_types", || -> rhai::Array {
        vec!["cut", "puncture", "bruise", "fracture"]
            .into_iter()
            .map(|s| Dynamic::from(s.to_string()))
            .collect()
    });

    // get_wound_type_for_damage(damage_type, severity) -> String
    // Returns appropriate wound type based on damage type and severity
    engine.register_fn(
        "get_wound_type_for_damage",
        |damage_type: String, severity: String| -> String {
            let wound_level = WoundLevel::from_str(&severity).unwrap_or(WoundLevel::Minor);
            match damage_type.to_lowercase().as_str() {
                "slashing" => "cut".to_string(),
                "piercing" => "puncture".to_string(),
                "bludgeoning" => {
                    // Bludgeoning causes bruises for minor/moderate, fractures for severe+
                    if wound_level >= WoundLevel::Severe {
                        "fracture".to_string()
                    } else {
                        "bruise".to_string()
                    }
                }
                // Elemental damage types
                "fire" => "burn".to_string(),
                "cold" => "frostbite".to_string(),
                "poison" => "poisoned".to_string(),
                "acid" => "corroded".to_string(),
                // Lightning causes internal bruising
                "lightning" => "bruise".to_string(),
                // Bite causes punctures, fractures at severe+ (crushing bite force)
                "bite" => {
                    if wound_level >= WoundLevel::Severe {
                        "fracture".to_string()
                    } else {
                        "puncture".to_string()
                    }
                }
                // Ballistic always causes puncture wounds
                "ballistic" => "puncture".to_string(),
                _ => "bruise".to_string(), // Default
            }
        },
    );

    // ========== Character Wound Functions ==========

    // get_character_wounds(char_name) -> Array<Map>
    // Returns all wounds for a character
    let cloned_db = db.clone();
    engine.register_fn("get_character_wounds", move |char_name: String| -> rhai::Array {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            char.wounds.iter().map(wound_to_map).collect()
        } else {
            vec![]
        }
    });

    // get_character_wound_level(char_name, body_part) -> String
    // Returns the wound level for a specific body part ("none" if not wounded)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_character_wound_level",
        move |char_name: String, body_part: String| -> String {
            if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
                if let Some(bp) = BodyPart::from_str(&body_part) {
                    if let Some(wound) = char.wounds.iter().find(|w| w.body_part == bp) {
                        return wound.level.to_display_string().to_string();
                    }
                }
            }
            "none".to_string()
        },
    );

    // inflict_character_wound(char_name, body_part, wound_type, level, bleeding) -> bool
    // Inflicts or escalates a wound on a character
    let cloned_db = db.clone();
    engine.register_fn(
        "inflict_character_wound",
        move |char_name: String, body_part: String, wound_type: String, level: String, bleeding: i64| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };
            let wt = match WoundType::from_str(&wound_type) {
                Some(t) => t,
                None => return false,
            };
            let wl = match WoundLevel::from_str(&level) {
                Some(l) => l,
                None => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                // Check if wound already exists on this body part
                if let Some(existing) = char.wounds.iter_mut().find(|w| w.body_part == bp) {
                    // Escalate if new wound is worse
                    if wl > existing.level {
                        existing.level = wl;
                        existing.wound_type = wt;
                    }
                    // Stack bleeding
                    existing.bleeding_severity = (existing.bleeding_severity + bleeding as i32).min(5);
                } else {
                    // Add new wound
                    char.wounds.push(Wound {
                        body_part: bp,
                        level: wl,
                        wound_type: wt,
                        bleeding_severity: (bleeding as i32).min(5),
                    });
                }
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // heal_character_wound(char_name, body_part) -> bool
    // Removes a wound from a character's body part
    let cloned_db = db.clone();
    engine.register_fn(
        "heal_character_wound",
        move |char_name: String, body_part: String| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                let original_len = char.wounds.len();
                char.wounds.retain(|w| w.body_part != bp);
                if char.wounds.len() != original_len {
                    return cloned_db.save_character_data(char).is_ok();
                }
            }
            false
        },
    );

    // get_character_body_part_penalty(char_name, body_part) -> i64
    // Returns the penalty percentage for a body part based on wounds (0-100)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_character_body_part_penalty",
        move |char_name: String, body_part: String| -> i64 {
            if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
                if let Some(bp) = BodyPart::from_str(&body_part) {
                    if let Some(wound) = char.wounds.iter().find(|w| w.body_part == bp) {
                        return wound.level.penalty() as i64;
                    }
                }
            }
            0
        },
    );

    // get_character_total_bleeding(char_name) -> i64
    // Returns the total bleeding severity across all wounds
    let cloned_db = db.clone();
    engine.register_fn("get_character_total_bleeding", move |char_name: String| -> i64 {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.wounds.iter().map(|w| w.bleeding_severity as i64).sum();
        }
        0
    });

    // clear_character_wounds(char_name) -> bool
    // Removes all wounds from a character (e.g., on respawn)
    let cloned_db = db.clone();
    engine.register_fn("clear_character_wounds", move |char_name: String| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.wounds.clear();
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // ========== Mobile Wound Functions ==========

    // get_mobile_wounds(mobile_id) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_wounds", move |mobile_id: String| -> rhai::Array {
        tracing::debug!("get_mobile_wounds called with mobile_id={}", mobile_id);
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                tracing::debug!(
                    "get_mobile_wounds: found mobile {}, wounds count={}",
                    mobile.name,
                    mobile.wounds.len()
                );
                return mobile.wounds.iter().map(wound_to_map).collect();
            } else {
                tracing::debug!("get_mobile_wounds: mobile not found for uuid {}", uuid);
            }
        } else {
            tracing::debug!("get_mobile_wounds: failed to parse uuid from {}", mobile_id);
        }
        vec![]
    });

    // get_mobile_wound_level(mobile_id, body_part) -> String
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_wound_level",
        move |mobile_id: String, body_part: String| -> String {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if let Some(bp) = BodyPart::from_str(&body_part) {
                        if let Some(wound) = mobile.wounds.iter().find(|w| w.body_part == bp) {
                            return wound.level.to_display_string().to_string();
                        }
                    }
                }
            }
            "none".to_string()
        },
    );

    // inflict_mobile_wound(mobile_id, body_part, wound_type, level, bleeding) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "inflict_mobile_wound",
        move |mobile_id: String, body_part: String, wound_type: String, level: String, bleeding: i64| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };
            let wt = match WoundType::from_str(&wound_type) {
                Some(t) => t,
                None => return false,
            };
            let wl = match WoundLevel::from_str(&level) {
                Some(l) => l,
                None => return false,
            };

            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    // Check if wound already exists
                    if let Some(existing) = mobile.wounds.iter_mut().find(|w| w.body_part == bp) {
                        if wl > existing.level {
                            existing.level = wl;
                            existing.wound_type = wt;
                        }
                        existing.bleeding_severity = (existing.bleeding_severity + bleeding as i32).min(5);
                    } else {
                        mobile.wounds.push(Wound {
                            body_part: bp,
                            level: wl,
                            wound_type: wt,
                            bleeding_severity: (bleeding as i32).min(5),
                        });
                    }
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // heal_mobile_wound(mobile_id, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "heal_mobile_wound",
        move |mobile_id: String, body_part: String| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    let original_len = mobile.wounds.len();
                    let bp_name = bp.to_display_string().to_string();
                    mobile.wounds.retain(|w| w.body_part != bp);
                    if mobile.wounds.len() != original_len {
                        // Also clear any ongoing effects on this body part
                        mobile.ongoing_effects.retain(|e| e.body_part != bp_name);
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
            }
            false
        },
    );

    // get_mobile_body_part_penalty(mobile_id, body_part) -> i64
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_body_part_penalty",
        move |mobile_id: String, body_part: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if let Some(bp) = BodyPart::from_str(&body_part) {
                        if let Some(wound) = mobile.wounds.iter().find(|w| w.body_part == bp) {
                            return wound.level.penalty() as i64;
                        }
                    }
                }
            }
            0
        },
    );

    // get_mobile_total_bleeding(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_total_bleeding", move |mobile_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.wounds.iter().map(|w| w.bleeding_severity as i64).sum();
            }
        }
        0
    });

    // clear_mobile_wounds(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_mobile_wounds", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.wounds.clear();
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // ========== Combat Calculation Functions (Phase 4) ==========

    // roll_dice(count, sides) -> i64
    // Rolls count dice with sides faces and returns the total
    engine.register_fn("roll_dice", |count: i64, sides: i64| -> i64 {
        if count <= 0 || sides <= 0 {
            return 0;
        }
        let mut rng = rand::thread_rng();
        let mut total = 0i64;
        for _ in 0..count {
            total += rng.gen_range(1..=sides);
        }
        total
    });

    // get_stat_modifier(stat_value) -> i64
    // Returns the modifier for a stat: (stat - 10) / 2
    engine.register_fn("get_stat_modifier", |stat_value: i64| -> i64 { (stat_value - 10) / 2 });

    // calculate_hit_chance(attacker_skill, attacker_dex_mod, target_dex_mod, target_ac) -> i64
    // Formula: 50 + (skill * 5) + attacker_DEX_mod - target_DEX_mod - target_AC (capped 5-95%)
    engine.register_fn(
        "calculate_hit_chance",
        |attacker_skill: i64, attacker_dex_mod: i64, target_dex_mod: i64, target_ac: i64| -> i64 {
            let base = 50 + (attacker_skill * 5) + attacker_dex_mod - target_dex_mod - target_ac;
            base.clamp(5, 95)
        },
    );

    // roll_attack(hit_chance) -> Map {hit: bool, roll: i64}
    // Rolls d100 against hit chance, returns whether it hit and the roll
    engine.register_fn("roll_attack", |hit_chance: i64| -> Map {
        let mut rng = rand::thread_rng();
        let roll = rng.gen_range(1..=100) as i64;
        let hit = roll <= hit_chance;
        let mut result = Map::new();
        result.insert("hit".into(), Dynamic::from(hit));
        result.insert("roll".into(), Dynamic::from(roll));
        Dynamic::from(result).cast::<Map>()
    });

    // calculate_damage(dice_count, dice_sides, damage_bonus) -> i64
    // Rolls damage dice and adds bonus
    engine.register_fn(
        "calculate_damage",
        |dice_count: i64, dice_sides: i64, damage_bonus: i64| -> i64 {
            if dice_count <= 0 || dice_sides <= 0 {
                return damage_bonus.max(0);
            }
            let mut rng = rand::thread_rng();
            let mut total = 0i64;
            for _ in 0..dice_count {
                total += rng.gen_range(1..=dice_sides);
            }
            (total + damage_bonus).max(1)
        },
    );

    // check_critical_hit(skill_level) -> bool
    // Crit chance: 5% + (skill_level * 1%), rolls d100
    engine.register_fn("check_critical_hit", |skill_level: i64| -> bool {
        let crit_chance = 5 + skill_level;
        let mut rng = rand::thread_rng();
        let roll = rng.gen_range(1..=100);
        roll <= crit_chance as i32
    });

    // ========== Critical Effect Functions (Phase 8) ==========

    // roll_critical_effect(damage_type) -> String
    // Rolls d4 to determine critical effect type based on damage type
    // Returns damage-type-specific effect names
    engine.register_fn("roll_critical_effect", |damage_type: String| -> String {
        let mut rng = rand::thread_rng();
        let roll = rng.gen_range(1..=4);
        match damage_type.to_lowercase().as_str() {
            "slashing" => match roll {
                1 => "deep_laceration",
                2 => "severed_tendon",
                3 => "arterial_cut",
                _ => "clean",
            },
            "piercing" => match roll {
                1 => "punctured_organ",
                2 => "impaled",
                3 => "nerve_strike",
                _ => "clean",
            },
            "bludgeoning" => match roll {
                1 => "broken_bone",
                2 => "concussion",
                3 => "crushed",
                _ => "clean",
            },
            "fire" => match roll {
                1 => "severe_burn",
                2 => "ignited",
                3 => "charred",
                _ => "clean",
            },
            "cold" => match roll {
                1 => "frozen_limb",
                2 => "hypothermic_shock",
                3 => "frostbitten",
                _ => "clean",
            },
            "lightning" => match roll {
                1 => "electrocuted",
                2 => "nerve_damage",
                3 => "cardiac_shock",
                _ => "clean",
            },
            "poison" => match roll {
                1 => "venom_surge",
                2 => "toxic_shock",
                3 => "paralysis",
                _ => "clean",
            },
            "acid" => match roll {
                1 => "acid_burn",
                2 => "corroded_armor",
                3 => "dissolved_flesh",
                _ => "clean",
            },
            "bite" => match roll {
                1 => "mauled",
                2 => "lockjaw",
                3 => "severed_chunk",
                _ => "clean",
            },
            "ballistic" => match roll {
                1 => "through_and_through",
                2 => "shrapnel",
                3 => "bullet_lodged",
                _ => "clean",
            },
            _ => match roll {
                1 => "bleeding",
                2 => "stun",
                3 => "disable",
                _ => "clean",
            },
        }
        .to_string()
    });

    // calculate_crit_damage(base_damage, skill_level) -> i64
    // Returns scaled crit damage: 1.5x if skill < 5, 2x if skill >= 5
    engine.register_fn("calculate_crit_damage", |base_damage: i64, skill_level: i64| -> i64 {
        if skill_level >= 5 {
            base_damage * 2
        } else {
            (base_damage * 3) / 2 // 1.5x
        }
    });

    // get_crit_stun_rounds(skill_level) -> i64
    // Returns stun duration for crit: 1 round if skill < 5, 2 rounds if skill >= 5
    engine.register_fn("get_crit_stun_rounds", |skill_level: i64| -> i64 {
        if skill_level >= 5 { 2 } else { 1 }
    });

    // get_crit_bleeding_severity(skill_level) -> i64
    // Returns bleeding severity for crit: 2 + skill_level/3 (2-5 range)
    engine.register_fn("get_crit_bleeding_severity", |skill_level: i64| -> i64 {
        (2 + skill_level / 3).min(5)
    });

    // add_wound_bleeding(char_name, body_part, severity) -> bool
    // Adds bleeding severity to an existing wound on a body part
    let cloned_db = db.clone();
    engine.register_fn(
        "add_wound_bleeding",
        move |char_name: String, body_part: String, severity: i64| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                // Find existing wound on this body part and add bleeding
                for wound in char.wounds.iter_mut() {
                    if wound.body_part == bp {
                        wound.bleeding_severity = (wound.bleeding_severity + severity as i32).min(5);
                        return cloned_db.save_character_data(char).is_ok();
                    }
                }
                // No existing wound - create minor cut with bleeding
                char.wounds.push(Wound {
                    body_part: bp,
                    level: WoundLevel::Minor,
                    wound_type: WoundType::Cut,
                    bleeding_severity: severity as i32,
                });
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // add_mobile_wound_bleeding(mobile_id, body_part, severity) -> bool
    // Adds bleeding severity to an existing wound on a mobile
    let cloned_db = db.clone();
    engine.register_fn(
        "add_mobile_wound_bleeding",
        move |mobile_id: String, body_part: String, severity: i64| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                // Find existing wound on this body part and add bleeding
                for wound in mobile.wounds.iter_mut() {
                    if wound.body_part == bp {
                        wound.bleeding_severity = (wound.bleeding_severity + severity as i32).min(5);
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
                // No existing wound - create minor cut with bleeding
                mobile.wounds.push(Wound {
                    body_part: bp,
                    level: WoundLevel::Minor,
                    wound_type: WoundType::Cut,
                    bleeding_severity: severity as i32,
                });
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
            false
        },
    );

    // escalate_wound_to_severe(char_name, body_part) -> bool
    // Escalates a wound to Severe level (limb disable crit)
    let cloned_db = db.clone();
    engine.register_fn(
        "escalate_wound_to_severe",
        move |char_name: String, body_part: String| -> bool {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                // Find existing wound on this body part and escalate to Severe
                for wound in char.wounds.iter_mut() {
                    if wound.body_part == bp {
                        wound.level = WoundLevel::Severe;
                        return cloned_db.save_character_data(char).is_ok();
                    }
                }
                // No existing wound - create new Severe wound
                char.wounds.push(Wound {
                    body_part: bp,
                    level: WoundLevel::Severe,
                    wound_type: WoundType::Cut,
                    bleeding_severity: 2, // Severe wounds bleed
                });
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // escalate_mobile_wound_to_severe(mobile_id, body_part) -> bool
    // Escalates a mobile wound to Severe level (limb disable crit)
    let cloned_db = db.clone();
    engine.register_fn(
        "escalate_mobile_wound_to_severe",
        move |mobile_id: String, body_part: String| -> bool {
            tracing::debug!(
                "escalate_mobile_wound_to_severe called: mobile_id={}, body_part={}",
                mobile_id,
                body_part
            );

            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => {
                    tracing::debug!(
                        "escalate_mobile_wound_to_severe: failed to parse body_part '{}'",
                        body_part
                    );
                    return false;
                }
            };

            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(e) => {
                    tracing::debug!(
                        "escalate_mobile_wound_to_severe: failed to parse mobile_id '{}': {}",
                        mobile_id,
                        e
                    );
                    return false;
                }
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                tracing::debug!(
                    "escalate_mobile_wound_to_severe: found mobile {}, current wounds={}",
                    mobile.name,
                    mobile.wounds.len()
                );
                // Find existing wound on this body part and escalate to Severe
                for wound in mobile.wounds.iter_mut() {
                    if wound.body_part == bp {
                        wound.level = WoundLevel::Severe;
                        let result = cloned_db.save_mobile_data(mobile);
                        tracing::debug!(
                            "escalate_mobile_wound_to_severe: escalated existing wound, save result={:?}",
                            result.is_ok()
                        );
                        return result.is_ok();
                    }
                }
                // No existing wound - create new Severe wound
                mobile.wounds.push(Wound {
                    body_part: bp,
                    level: WoundLevel::Severe,
                    wound_type: WoundType::Cut,
                    bleeding_severity: 2, // Severe wounds bleed
                });
                let result = cloned_db.save_mobile_data(mobile);
                tracing::debug!(
                    "escalate_mobile_wound_to_severe: created new wound, save result={:?}",
                    result.is_ok()
                );
                return result.is_ok();
            } else {
                tracing::debug!("escalate_mobile_wound_to_severe: mobile not found for id {}", mid);
            }
            false
        },
    );

    // consume_stamina_for_attack(char_name, amount) -> bool
    // Deducts stamina for an attack. Returns false if not enough stamina.
    let cloned_db = db.clone();
    engine.register_fn(
        "consume_stamina_for_attack",
        move |char_name: String, amount: i64| -> bool {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                if char.stamina >= amount as i32 {
                    char.stamina -= amount as i32;
                    return cloned_db.save_character_data(char).is_ok();
                }
            }
            false
        },
    );

    // ========== Armor System Functions (Phase 4) ==========

    // get_armor_for_body_part(char_name, body_part) -> Array<Map>
    // Returns all equipped armor items that protect the given body part
    let cloned_db = db.clone();
    engine.register_fn(
        "get_armor_for_body_part",
        move |char_name: String, body_part: String| -> rhai::Array {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return vec![],
            };

            let equipped = match cloned_db.get_equipped_items(&char_name) {
                Ok(items) => items,
                Err(_) => return vec![],
            };

            equipped
                .into_iter()
                .filter(|item| {
                    item.protects.contains(&bp)
                        || bp.parent_part().map_or(false, |parent| item.protects.contains(&parent))
                })
                .map(|item| {
                    let mut map = Map::new();
                    map.insert("id".into(), Dynamic::from(item.id.to_string()));
                    map.insert("name".into(), Dynamic::from(item.name.clone()));
                    map.insert(
                        "armor_class".into(),
                        Dynamic::from(item.armor_class.unwrap_or(0) as i64),
                    );
                    map.insert("holes".into(), Dynamic::from(item.holes as i64));
                    Dynamic::from(map)
                })
                .collect()
        },
    );

    // get_mobile_armor_for_body_part(mobile_id, body_part) -> Array<Map>
    // Returns all equipped armor items that protect the given body part for a mobile
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_armor_for_body_part",
        move |mobile_id: String, body_part: String| -> rhai::Array {
            let bp = match BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return vec![],
            };

            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return vec![],
            };

            let equipped = match cloned_db.get_items_equipped_on_mobile(&uuid) {
                Ok(items) => items,
                Err(_) => return vec![],
            };

            equipped
                .into_iter()
                .filter(|item| {
                    item.protects.contains(&bp)
                        || bp.parent_part().map_or(false, |parent| item.protects.contains(&parent))
                })
                .map(|item| {
                    let mut map = Map::new();
                    map.insert("id".into(), Dynamic::from(item.id.to_string()));
                    map.insert("name".into(), Dynamic::from(item.name.clone()));
                    map.insert(
                        "armor_class".into(),
                        Dynamic::from(item.armor_class.unwrap_or(0) as i64),
                    );
                    map.insert("holes".into(), Dynamic::from(item.holes as i64));
                    Dynamic::from(map)
                })
                .collect()
        },
    );

    // calculate_effective_ac(item_id) -> i64
    // Returns effective AC after hole degradation: base_AC * (1.0 - holes * 0.33)
    let cloned_db = db.clone();
    engine.register_fn("calculate_effective_ac", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                let base_ac = item.armor_class.unwrap_or(0) as f64;
                let degradation = 1.0 - (item.holes as f64 * 0.33);
                return (base_ac * degradation.max(0.0)) as i64;
            }
        }
        0
    });

    // roll_armor_save(armor_id, damage) -> Map {blocked: bool, damage_taken: i64}
    // Rolls to see if armor blocks the hit. Returns whether blocked and remaining damage.
    // If blocked, damage_taken is 0. If not blocked, damage_taken is full damage.
    let cloned_db = db.clone();
    engine.register_fn("roll_armor_save", move |armor_id: String, damage: i64| -> Map {
        let mut result = Map::new();
        result.insert("blocked".into(), Dynamic::from(false));
        result.insert("damage_taken".into(), Dynamic::from(damage));

        if let Ok(uuid) = uuid::Uuid::parse_str(&armor_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                let base_ac = item.armor_class.unwrap_or(0) as f64;
                let degradation = 1.0 - (item.holes as f64 * 0.33);
                let effective_ac = (base_ac * degradation.max(0.0)) as i32;

                // Armor save: roll d20 + effective_ac vs damage
                // If roll + AC >= damage, armor blocks
                let mut rng = rand::thread_rng();
                let roll = rng.gen_range(1..=20);
                let save_total = roll + effective_ac;

                if save_total >= damage as i32 {
                    result.insert("blocked".into(), Dynamic::from(true));
                    result.insert("damage_taken".into(), Dynamic::from(0i64));
                }
            }
        }
        result
    });

    // add_armor_hole(item_id) -> Map {success: bool, destroyed: bool, holes: i64}
    // Adds a hole to armor. Returns if successful and if armor is now destroyed (3+ holes).
    let cloned_db = db.clone();
    engine.register_fn("add_armor_hole", move |item_id: String| -> Map {
        let mut result = Map::new();
        result.insert("success".into(), Dynamic::from(false));
        result.insert("destroyed".into(), Dynamic::from(false));
        result.insert("holes".into(), Dynamic::from(0i64));

        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.holes += 1;
                let destroyed = item.holes >= 3;
                let holes = item.holes as i64;

                if cloned_db.save_item_data(item).is_ok() {
                    result.insert("success".into(), Dynamic::from(true));
                    result.insert("destroyed".into(), Dynamic::from(destroyed));
                    result.insert("holes".into(), Dynamic::from(holes));
                }
            }
        }
        result
    });

    // get_armor_condition(item_id) -> String
    // Returns condition: "pristine" (0), "damaged" (1), "battered" (2), "destroyed" (3+)
    let cloned_db = db.clone();
    engine.register_fn("get_armor_condition", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return match item.holes {
                    0 => "pristine".to_string(),
                    1 => "damaged".to_string(),
                    2 => "battered".to_string(),
                    _ => "destroyed".to_string(),
                };
            }
        }
        "unknown".to_string()
    });

    // get_item_holes(item_id) -> i64
    // Returns the number of holes in an item
    let cloned_db = db.clone();
    engine.register_fn("get_item_holes", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.holes as i64;
            }
        }
        0
    });

    // ========== Combat State Management (Phase 5) ==========

    // enter_combat(char_name, target_type, target_id) -> bool
    // Adds a target to character's combat state
    let cloned_db = db.clone();
    engine.register_fn(
        "enter_combat",
        move |char_name: String, target_type: String, target_id: String| -> bool {
            let tt = match CombatTargetType::from_str(&target_type) {
                Some(t) => t,
                None => return false,
            };
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                // Check if already targeting this entity
                if !char.combat.targets.iter().any(|t| t.target_id == tid) {
                    char.combat.targets.push(CombatTarget {
                        target_type: tt,
                        target_id: tid,
                    });
                }
                char.combat.in_combat = true;
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // exit_combat(char_name) -> bool
    // Removes character from combat entirely
    let cloned_db = db.clone();
    engine.register_fn("exit_combat", move |char_name: String| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.combat.in_combat = false;
            char.combat.targets.clear();
            char.combat.stun_rounds_remaining = 0;
            char.combat.distances.clear();
            char.combat.ammo_depleted = 0;
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // remove_combat_target(char_name, target_id) -> bool
    // Removes a specific target from combat
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_combat_target",
        move |char_name: String, target_id: String| -> bool {
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.combat.targets.retain(|t| t.target_id != tid);
                char.combat.distances.remove(&tid);
                // Exit combat if no targets left
                if char.combat.targets.is_empty() {
                    char.combat.in_combat = false;
                    char.combat.distances.clear();
                }
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // is_in_combat(char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_in_combat", move |char_name: String| -> bool {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.combat.in_combat;
        }
        false
    });

    // get_combat_targets(char_name) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("get_combat_targets", move |char_name: String| -> rhai::Array {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char
                .combat
                .targets
                .iter()
                .map(|t| {
                    let mut map = Map::new();
                    map.insert(
                        "target_type".into(),
                        Dynamic::from(t.target_type.to_display_string().to_string()),
                    );
                    map.insert("target_id".into(), Dynamic::from(t.target_id.to_string()));
                    Dynamic::from(map)
                })
                .collect();
        }
        vec![]
    });

    // get_primary_target(char_name) -> Map or ()
    // Returns the first combat target
    let cloned_db = db.clone();
    engine.register_fn("get_primary_target", move |char_name: String| -> Dynamic {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            if let Some(t) = char.combat.targets.first() {
                let mut map = Map::new();
                map.insert(
                    "target_type".into(),
                    Dynamic::from(t.target_type.to_display_string().to_string()),
                );
                map.insert("target_id".into(), Dynamic::from(t.target_id.to_string()));
                return Dynamic::from(map);
            }
        }
        Dynamic::UNIT
    });

    // get_primary_target_info(char_name) -> Map or ()
    // Returns detailed info about a character's primary target (for assist command)
    // Map includes: target_type, target_id, target_name
    let cloned_db = db.clone();
    engine.register_fn("get_primary_target_info", move |char_name: String| -> Dynamic {
        let char = match cloned_db.get_character_data(&char_name) {
            Ok(Some(c)) => c,
            _ => return Dynamic::UNIT,
        };

        if !char.combat.in_combat {
            return Dynamic::UNIT;
        }

        if let Some(target) = char.combat.targets.first() {
            let mut map = Map::new();
            map.insert(
                "target_type".into(),
                Dynamic::from(target.target_type.to_display_string().to_string()),
            );
            map.insert("target_id".into(), Dynamic::from(target.target_id.to_string()));

            // Get target name based on type
            let target_name = match target.target_type {
                CombatTargetType::Mobile => {
                    if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&target.target_id) {
                        mobile.name.clone()
                    } else {
                        "unknown".to_string()
                    }
                }
                CombatTargetType::Player => {
                    // For player targets we'd need the name, but we store UUIDs
                    // This would require iterating characters - return empty for now
                    "a player".to_string()
                }
            };
            map.insert("target_name".into(), Dynamic::from(target_name));

            return Dynamic::from(map);
        }

        Dynamic::UNIT
    });

    // apply_stun(char_name, rounds) -> bool
    let cloned_db = db.clone();
    engine.register_fn("apply_stun", move |char_name: String, rounds: i64| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.combat.stun_rounds_remaining = (char.combat.stun_rounds_remaining + rounds as i32).min(5);
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // get_stun_rounds(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_stun_rounds", move |char_name: String| -> i64 {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.combat.stun_rounds_remaining as i64;
        }
        0
    });

    // reduce_stun(char_name) -> i64
    // Reduces stun by 1 and returns remaining rounds
    let cloned_db = db.clone();
    engine.register_fn("reduce_stun", move |char_name: String| -> i64 {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            if char.combat.stun_rounds_remaining > 0 {
                char.combat.stun_rounds_remaining -= 1;
                let _ = cloned_db.save_character_data(char.clone());
            }
            return char.combat.stun_rounds_remaining as i64;
        }
        0
    });

    // ========== Combat Distance Functions ==========

    // get_combat_distance(char_name, target_id) -> String
    // Returns distance to target: "melee", "pole", or "ranged" (defaults to "melee")
    let cloned_db = db.clone();
    engine.register_fn(
        "get_combat_distance",
        move |char_name: String, target_id: String| -> String {
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return "melee".to_string(),
            };

            if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
                return char
                    .combat
                    .distances
                    .get(&tid)
                    .unwrap_or(&CombatDistance::Melee)
                    .to_display_string()
                    .to_string();
            }
            "melee".to_string()
        },
    );

    // set_combat_distance(char_name, target_id, distance) -> bool
    // Sets distance to target: "melee", "pole", or "ranged"
    let cloned_db = db.clone();
    engine.register_fn(
        "set_combat_distance",
        move |char_name: String, target_id: String, distance: String| -> bool {
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let dist = match CombatDistance::from_str(&distance) {
                Some(d) => d,
                None => return false,
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.combat.distances.insert(tid, dist);
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // get_distance_modifier(weapon_skill, distance) -> i64
    // Returns hit modifier for weapon skill at distance
    engine.register_fn(
        "get_distance_modifier",
        move |weapon_skill: String, distance: String| -> i64 {
            let skill = match WeaponSkill::from_str(&weapon_skill) {
                Some(s) => s,
                None => return 0,
            };
            let dist = match CombatDistance::from_str(&distance) {
                Some(d) => d,
                None => return 0,
            };
            skill.distance_modifier(dist) as i64
        },
    );

    // can_advance(char_name, target_id) -> bool
    // Returns true if character can advance toward target (not at melee)
    let cloned_db = db.clone();
    engine.register_fn("can_advance", move |char_name: String, target_id: String| -> bool {
        let tid = match uuid::Uuid::parse_str(&target_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            let current = char.combat.distances.get(&tid).unwrap_or(&CombatDistance::Melee);
            return current.closer().is_some();
        }
        false
    });

    // can_retreat(char_name, target_id) -> bool
    // Returns true if character can retreat from target (not at ranged)
    let cloned_db = db.clone();
    engine.register_fn("can_retreat", move |char_name: String, target_id: String| -> bool {
        let tid = match uuid::Uuid::parse_str(&target_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            let current = char.combat.distances.get(&tid).unwrap_or(&CombatDistance::Melee);
            return current.farther().is_some();
        }
        false
    });

    // attempt_advance(char_name, target_id) -> bool
    // Moves one step closer to target. Returns true on success.
    let cloned_db = db.clone();
    engine.register_fn("attempt_advance", move |char_name: String, target_id: String| -> bool {
        let tid = match uuid::Uuid::parse_str(&target_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            let current = char
                .combat
                .distances
                .get(&tid)
                .copied()
                .unwrap_or(CombatDistance::Melee);
            if let Some(closer) = current.closer() {
                char.combat.distances.insert(tid, closer);
                return cloned_db.save_character_data(char).is_ok();
            }
        }
        false
    });

    // attempt_retreat(char_name, target_id) -> String
    // Attempts to retreat from target. Returns "success", "failed", or "no_room"
    // Note: Stamina cost and DEX roll should be handled in Rhai script
    let cloned_db = db.clone();
    engine.register_fn(
        "attempt_retreat",
        move |char_name: String, target_id: String| -> String {
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return "failed".to_string(),
            };

            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                let current = char
                    .combat
                    .distances
                    .get(&tid)
                    .copied()
                    .unwrap_or(CombatDistance::Melee);
                if let Some(farther) = current.farther() {
                    char.combat.distances.insert(tid, farther);
                    if cloned_db.save_character_data(char).is_ok() {
                        return "success".to_string();
                    }
                } else {
                    return "no_room".to_string();
                }
            }
            "failed".to_string()
        },
    );

    // get_distance_step_name(from_distance, to_distance) -> String
    // Returns descriptive name for a distance transition (for messages)
    engine.register_fn("get_distance_step_name", move |from: String, to: String| -> String {
        match (from.as_str(), to.as_str()) {
            ("ranged", "pole") => "closes in".to_string(),
            ("pole", "melee") => "moves to melee range".to_string(),
            ("melee", "pole") => "backs away".to_string(),
            ("pole", "ranged") => "creates distance".to_string(),
            _ => "moves".to_string(),
        }
    });

    // weapon_prefers_melee(weapon_skill) -> bool
    // Returns true if weapon skill prefers melee range (for mob AI)
    engine.register_fn("weapon_prefers_melee", move |weapon_skill: String| -> bool {
        match WeaponSkill::from_str(&weapon_skill) {
            Some(skill) => skill.prefers_melee(),
            None => true, // Default to melee preference
        }
    });

    // ========== Ammunition Lookup Functions ==========

    // get_readied_item(char_name) -> ItemData or ()
    // Finds item equipped in the Ready wear slot
    let cloned_db = db.clone();
    engine.register_fn("get_readied_item", move |char_name: String| -> Dynamic {
        let equipped = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
        for item in equipped {
            if item.wear_locations.iter().any(|loc| matches!(loc, WearLocation::Ready)) {
                return Dynamic::from(item);
            }
        }
        Dynamic::UNIT
    });

    // find_compatible_ammo(char_name, caliber) -> Map{source, item_id, ammo_damage_bonus}
    // Searches Ready slot first, then inventory for matching caliber ammo
    // Returns source: "ready"/"inventory"/"none"
    let cloned_db = db.clone();
    engine.register_fn(
        "find_compatible_ammo",
        move |char_name: String, caliber: String| -> Map {
            let mut result = Map::new();
            let caliber_lower = caliber.to_lowercase();

            // Check Ready slot first
            let equipped = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
            for item in &equipped {
                if item.wear_locations.iter().any(|loc| matches!(loc, WearLocation::Ready))
                    && item.item_type == ItemType::Ammunition
                    && item.caliber.as_ref().map(|c: &String| c.to_lowercase()) == Some(caliber_lower.clone())
                    && item.ammo_count > 0
                    && !item.flags.broken
                {
                    result.insert("source".into(), Dynamic::from("ready".to_string()));
                    result.insert("item_id".into(), Dynamic::from(item.id.to_string()));
                    result.insert("ammo_damage_bonus".into(), Dynamic::from(item.ammo_damage_bonus as i64));
                    return result;
                }
            }

            // Check inventory
            let inventory = cloned_db.get_items_in_inventory(&char_name).unwrap_or_default();
            for item in &inventory {
                if item.item_type == ItemType::Ammunition
                    && item.caliber.as_ref().map(|c: &String| c.to_lowercase()) == Some(caliber_lower.clone())
                    && item.ammo_count > 0
                    && !item.flags.broken
                {
                    result.insert("source".into(), Dynamic::from("inventory".to_string()));
                    result.insert("item_id".into(), Dynamic::from(item.id.to_string()));
                    result.insert("ammo_damage_bonus".into(), Dynamic::from(item.ammo_damage_bonus as i64));
                    return result;
                }
            }

            result.insert("source".into(), Dynamic::from("none".to_string()));
            result.insert("item_id".into(), Dynamic::from("".to_string()));
            result.insert("ammo_damage_bonus".into(), Dynamic::from(0 as i64));
            result
        },
    );

    // ========== Mobile Combat State (Phase 5) ==========

    // enter_mobile_combat(mobile_id, target_type, target_id) -> bool
    // Note: For player targets, target_id is a player name (not UUID)
    // We use a nil UUID for player targets since we find them by room anyway
    let cloned_db = db.clone();
    engine.register_fn(
        "enter_mobile_combat",
        move |mobile_id: String, target_type: String, target_id: String| -> bool {
            tracing::debug!(
                "enter_mobile_combat called: mobile_id={}, target_type={}, target_id={}",
                mobile_id,
                target_type,
                target_id
            );

            let tt = match CombatTargetType::from_str(&target_type) {
                Some(t) => t,
                None => {
                    tracing::debug!("enter_mobile_combat: invalid target_type {}", target_type);
                    return false;
                }
            };
            // For player targets, use nil UUID (we find players by room, not by UUID)
            // For mobile targets, parse the UUID
            let tid = if tt == CombatTargetType::Player {
                uuid::Uuid::nil()
            } else {
                match uuid::Uuid::parse_str(&target_id) {
                    Ok(u) => u,
                    Err(_) => {
                        tracing::debug!("enter_mobile_combat: failed to parse target_id {}", target_id);
                        return false;
                    }
                }
            };
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => {
                    tracing::debug!("enter_mobile_combat: failed to parse mobile_id {}", mobile_id);
                    return false;
                }
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                tracing::debug!(
                    "enter_mobile_combat: loaded mobile {} ({}), current in_combat={}, targets={}",
                    mobile.name,
                    mid,
                    mobile.combat.in_combat,
                    mobile.combat.targets.len()
                );

                // For players, just check if we already have a player target
                let already_targeting = if tt == CombatTargetType::Player {
                    mobile
                        .combat
                        .targets
                        .iter()
                        .any(|t| t.target_type == CombatTargetType::Player)
                } else {
                    mobile.combat.targets.iter().any(|t| t.target_id == tid)
                };

                if !already_targeting {
                    mobile.combat.targets.push(CombatTarget {
                        target_type: tt,
                        target_id: tid,
                    });
                    tracing::debug!(
                        "enter_mobile_combat: added target, now {} targets",
                        mobile.combat.targets.len()
                    );
                }
                mobile.combat.in_combat = true;
                let mobile_name = mobile.name.clone();
                let save_result = cloned_db.save_mobile_data(mobile);
                tracing::debug!("enter_mobile_combat: saved mobile, result={:?}", save_result.is_ok());

                // Verify the save persisted by re-reading immediately
                if save_result.is_ok() {
                    if let Ok(Some(verify_mobile)) = cloned_db.get_mobile_data(&mid) {
                        tracing::debug!(
                            "enter_mobile_combat: VERIFY after save - {} in_combat={}, targets={}",
                            mobile_name,
                            verify_mobile.combat.in_combat,
                            verify_mobile.combat.targets.len()
                        );
                    } else {
                        tracing::debug!("enter_mobile_combat: VERIFY FAILED - could not re-read mobile after save");
                    }
                }

                return save_result.is_ok();
            } else {
                tracing::debug!("enter_mobile_combat: mobile {} not found in database", mid);
            }
            false
        },
    );

    // exit_mobile_combat(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("exit_mobile_combat", move |mobile_id: String| -> bool {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            mobile.combat.in_combat = false;
            mobile.combat.targets.clear();
            mobile.combat.stun_rounds_remaining = 0;
            mobile.combat.distances.clear();
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // is_mobile_in_combat(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_mobile_in_combat", move |mobile_id: String| -> bool {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            return mobile.combat.in_combat;
        }
        false
    });

    // get_mobile_combat_targets(mobile_id) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_combat_targets", move |mobile_id: String| -> rhai::Array {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return vec![],
        };

        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            return mobile
                .combat
                .targets
                .iter()
                .map(|t| {
                    let mut map = Map::new();
                    map.insert(
                        "target_type".into(),
                        Dynamic::from(t.target_type.to_display_string().to_string()),
                    );
                    map.insert("target_id".into(), Dynamic::from(t.target_id.to_string()));
                    Dynamic::from(map)
                })
                .collect();
        }
        vec![]
    });

    // get_mobile_primary_target(mobile_id) -> Map or ()
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_primary_target", move |mobile_id: String| -> Dynamic {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Dynamic::UNIT,
        };

        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            if let Some(t) = mobile.combat.targets.first() {
                let mut map = Map::new();
                map.insert(
                    "target_type".into(),
                    Dynamic::from(t.target_type.to_display_string().to_string()),
                );
                map.insert("target_id".into(), Dynamic::from(t.target_id.to_string()));
                return Dynamic::from(map);
            }
        }
        Dynamic::UNIT
    });

    // apply_mobile_stun(mobile_id, rounds) -> bool
    let cloned_db = db.clone();
    engine.register_fn("apply_mobile_stun", move |mobile_id: String, rounds: i64| -> bool {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            mobile.combat.stun_rounds_remaining = (mobile.combat.stun_rounds_remaining + rounds as i32).min(5);
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // reduce_mobile_stun(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("reduce_mobile_stun", move |mobile_id: String| -> i64 {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            if mobile.combat.stun_rounds_remaining > 0 {
                mobile.combat.stun_rounds_remaining -= 1;
                let _ = cloned_db.save_mobile_data(mobile.clone());
            }
            return mobile.combat.stun_rounds_remaining as i64;
        }
        0
    });

    // ========== Mobile Combat Distance Functions ==========

    // get_mobile_combat_distance(mobile_id, target_id) -> String
    // Returns distance to target: "melee", "pole", or "ranged"
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_combat_distance",
        move |mobile_id: String, target_id: String| -> String {
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return "melee".to_string(),
            };
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return "melee".to_string(),
            };

            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
                return mobile
                    .combat
                    .distances
                    .get(&tid)
                    .unwrap_or(&CombatDistance::Melee)
                    .to_display_string()
                    .to_string();
            }
            "melee".to_string()
        },
    );

    // set_mobile_combat_distance(mobile_id, target_id, distance) -> bool
    // For player targets, target_id is a player name - use nil UUID (consistent with enter_mobile_combat)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_combat_distance",
        move |mobile_id: String, target_id: String, distance: String| -> bool {
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            // If target_id is a valid UUID, use it; otherwise treat as player name and use nil UUID
            let tid = uuid::Uuid::parse_str(&target_id).unwrap_or(uuid::Uuid::nil());
            let dist = match CombatDistance::from_str(&distance) {
                Some(d) => d,
                None => return false,
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                mobile.combat.distances.insert(tid, dist);
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
            false
        },
    );

    // mobile_can_advance(mobile_id, target_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "mobile_can_advance",
        move |mobile_id: String, target_id: String| -> bool {
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
                let current = mobile.combat.distances.get(&tid).unwrap_or(&CombatDistance::Melee);
                return current.closer().is_some();
            }
            false
        },
    );

    // mobile_attempt_advance(mobile_id, target_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "mobile_attempt_advance",
        move |mobile_id: String, target_id: String| -> bool {
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let tid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                let current = mobile
                    .combat
                    .distances
                    .get(&tid)
                    .copied()
                    .unwrap_or(CombatDistance::Melee);
                if let Some(closer) = current.closer() {
                    mobile.combat.distances.insert(tid, closer);
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // ========== Combat Round Helpers (Phase 5) ==========

    // get_all_characters_in_combat() -> Array<String>
    // Returns names of all characters currently in combat
    let cloned_db = db.clone();
    engine.register_fn("get_all_characters_in_combat", move || -> rhai::Array {
        cloned_db
            .get_all_characters_in_combat()
            .unwrap_or_default()
            .into_iter()
            .map(|name| Dynamic::from(name))
            .collect()
    });

    // get_all_mobiles_in_combat() -> Array<String>
    // Returns IDs of all mobiles currently in combat
    let cloned_db = db.clone();
    engine.register_fn("get_all_mobiles_in_combat", move || -> rhai::Array {
        cloned_db
            .get_all_mobiles_in_combat()
            .unwrap_or_default()
            .into_iter()
            .map(|id| Dynamic::from(id.to_string()))
            .collect()
    });

    // apply_bleeding_damage(char_name) -> i64
    // Applies bleeding damage and returns amount dealt
    let cloned_db = db.clone();
    engine.register_fn("apply_bleeding_damage", move |char_name: String| -> i64 {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            let raw: i32 = char.wounds.iter().map(|w| w.bleeding_severity).sum();
            if raw > 0 {
                let bleeding = apply_damage_reduction(raw, &char.active_buffs);
                char.hp -= bleeding;
                let _ = cloned_db.save_character_data(char);
                return bleeding as i64;
            }
        }
        0
    });

    // apply_mobile_bleeding_damage(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("apply_mobile_bleeding_damage", move |mobile_id: String| -> i64 {
        let mid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            let raw: i32 = mobile.wounds.iter().map(|w| w.bleeding_severity).sum();
            if raw > 0 {
                let bleeding = apply_damage_reduction(raw, &mobile.active_buffs);
                mobile.current_hp -= bleeding;
                let _ = cloned_db.save_mobile_data(mobile);
                return bleeding as i64;
            }
        }
        0
    });

    // ========== Flee & Combat Lock Functions (Phase 6) ==========

    // roll_d100() -> i64
    // Returns a random number between 1 and 100
    engine.register_fn("roll_d100", || -> i64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(1..=100)
    });

    // get_leg_wound_penalty(char_name) -> i64
    // Returns the maximum penalty from left or right leg wounds (0-100)
    let cloned_db = db.clone();
    engine.register_fn("get_leg_wound_penalty", move |char_name: String| -> i64 {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            let left_penalty = char
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::LeftLeg)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0);

            let right_penalty = char
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::RightLeg)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0);

            return std::cmp::max(left_penalty, right_penalty) as i64;
        }
        0
    });

    // calculate_flee_chance(char_name, target_id) -> i64
    // Returns flee success chance (10-90) based on DEX comparison and leg wounds
    let cloned_db = db.clone();
    engine.register_fn(
        "calculate_flee_chance",
        move |char_name: String, target_id: String| -> i64 {
            let char = match cloned_db.get_character_data(&char_name) {
                Ok(Some(c)) => c,
                _ => return 50, // Default if character not found
            };

            // Get character's DEX modifier
            let char_dex_mod = (char.stat_dex as i32 - 10) / 2;

            // Get target's DEX modifier (try mobile first, then player)
            let target_dex_mod = if let Ok(uuid) = uuid::Uuid::parse_str(&target_id) {
                // Try as mobile
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    (mobile.stat_dex as i32 - 10) / 2
                } else {
                    0
                }
            } else {
                // Try as player name
                if let Ok(Some(target)) = cloned_db.get_character_data(&target_id) {
                    (target.stat_dex as i32 - 10) / 2
                } else {
                    0
                }
            };

            // Get leg wound penalty (max of left/right)
            let left_penalty = char
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::LeftLeg)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0);

            let right_penalty = char
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::RightLeg)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0);

            let leg_penalty = std::cmp::max(left_penalty, right_penalty) as i32;

            // Calculate flee chance:
            // base 50 + (char_dex * 5) - (target_dex * 3) - leg_penalty
            let chance = 50 + (char_dex_mod * 5) - (target_dex_mod * 3) - leg_penalty;

            // Clamp between 10 and 90
            chance.clamp(10, 90) as i64
        },
    );

    // get_valid_flee_directions(room_id) -> Array<String>
    // Returns array of valid exit directions (unlocked doors, valid exits)
    let cloned_db = db.clone();
    engine.register_fn("get_valid_flee_directions", move |room_id: String| -> rhai::Array {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Array::new(),
        };

        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return rhai::Array::new(),
        };

        let mut directions = rhai::Array::new();
        let exits = &room.exits;

        // Check each direction
        let check_exit = |exit: &Option<uuid::Uuid>, dir: &str| -> Option<String> {
            if exit.is_none() {
                return None;
            }

            // Check if door exists and is locked
            if let Some(door) = room.doors.get(dir) {
                if door.is_closed && door.is_locked {
                    return None; // Locked door blocks flee
                }
            }

            Some(dir.to_string())
        };

        if let Some(dir) = check_exit(&exits.north, "north") {
            directions.push(Dynamic::from(dir));
        }
        if let Some(dir) = check_exit(&exits.south, "south") {
            directions.push(Dynamic::from(dir));
        }
        if let Some(dir) = check_exit(&exits.east, "east") {
            directions.push(Dynamic::from(dir));
        }
        if let Some(dir) = check_exit(&exits.west, "west") {
            directions.push(Dynamic::from(dir));
        }
        if let Some(dir) = check_exit(&exits.up, "up") {
            directions.push(Dynamic::from(dir));
        }
        if let Some(dir) = check_exit(&exits.down, "down") {
            directions.push(Dynamic::from(dir));
        }

        directions
    });

    // remove_mobile_combat_target(mobile_id, target_name) -> bool
    // Removes a target from a mobile's combat list
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_mobile_combat_target",
        move |mobile_id: String, target_name: String| -> bool {
            let mid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                let original_len = mobile.combat.targets.len();

                // Try to parse target_name as UUID, or match by name
                if let Ok(target_uuid) = uuid::Uuid::parse_str(&target_name) {
                    mobile.combat.targets.retain(|t| t.target_id != target_uuid);
                    mobile.combat.distances.remove(&target_uuid);
                } else {
                    // For player targets, we use nil UUID
                    // Clear all player targets and their distances
                    mobile
                        .combat
                        .targets
                        .retain(|t| t.target_type != CombatTargetType::Player);
                    mobile.combat.distances.remove(&uuid::Uuid::nil());
                }

                // If no targets left, exit combat
                if mobile.combat.targets.is_empty() {
                    mobile.combat.in_combat = false;
                    mobile.combat.distances.clear();
                }

                if let Err(_) = cloned_db.save_mobile_data(mobile.clone()) {
                    return false;
                }

                return mobile.combat.targets.len() < original_len;
            }
            false
        },
    );

    // ========== Unconscious State Functions ==========

    // set_unconscious(char_name, is_unconscious) -> bool
    // Sets character unconscious state
    let cloned_db = db.clone();
    engine.register_fn(
        "set_unconscious",
        move |char_name: String, is_unconscious: bool| -> bool {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.is_unconscious = is_unconscious;
                if !is_unconscious {
                    char.bleedout_rounds_remaining = 0;
                }
                if let Err(_) = cloned_db.save_character_data(char) {
                    return false;
                }
                return true;
            }
            false
        },
    );

    // is_character_unconscious(char_name) -> bool
    // Returns true if character is unconscious
    let cloned_db = db.clone();
    engine.register_fn("is_character_unconscious", move |char_name: String| -> bool {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.is_unconscious;
        }
        false
    });

    // get_bleedout_rounds(char_name) -> i64
    // Returns remaining bleedout rounds for character
    let cloned_db = db.clone();
    engine.register_fn("get_bleedout_rounds", move |char_name: String| -> i64 {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.bleedout_rounds_remaining as i64;
        }
        0
    });

    // set_bleedout_rounds(char_name, rounds) -> bool
    // Sets bleedout rounds remaining
    let cloned_db = db.clone();
    engine.register_fn("set_bleedout_rounds", move |char_name: String, rounds: i64| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.bleedout_rounds_remaining = rounds as i32;
            if let Err(_) = cloned_db.save_character_data(char) {
                return false;
            }
            return true;
        }
        false
    });

    // Mobile unconscious state functions
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_unconscious",
        move |mobile_id: String, is_unconscious: bool| -> bool {
            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                mobile.is_unconscious = is_unconscious;
                if !is_unconscious {
                    mobile.bleedout_rounds_remaining = 0;
                }
                if let Err(_) = cloned_db.save_mobile_data(mobile) {
                    return false;
                }
                return true;
            }
            false
        },
    );

    let cloned_db = db.clone();
    engine.register_fn("is_mobile_unconscious", move |mobile_id: String| -> bool {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            return mobile.is_unconscious;
        }
        false
    });

    let cloned_db = db.clone();
    engine.register_fn("get_mobile_bleedout_rounds", move |mobile_id: String| -> i64 {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            return mobile.bleedout_rounds_remaining as i64;
        }
        0
    });

    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_bleedout_rounds",
        move |mobile_id: String, rounds: i64| -> bool {
            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                mobile.bleedout_rounds_remaining = rounds as i32;
                if let Err(_) = cloned_db.save_mobile_data(mobile) {
                    return false;
                }
                return true;
            }
            false
        },
    );

    // ========== Corpse Functions ==========

    // create_corpse(name, room_id, is_player) -> String (corpse item ID or empty)
    // Creates a corpse container item in the specified room
    let cloned_db = db.clone();
    engine.register_fn(
        "create_corpse",
        move |name: String, room_id: String, is_player: bool| -> String {
            let room_uuid = match Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let corpse = ItemData {
                id: Uuid::new_v4(),
                name: format!("corpse of {}", name),
                short_desc: format!("The corpse of {} lies here.", name),
                long_desc: format!("The lifeless body of {} lies in a crumpled heap.", name),
                keywords: vec!["corpse".to_string(), "body".to_string(), name.to_lowercase()],
                item_type: ItemType::Container,
                categories: Vec::new(),
                teaches_recipe: None,
                teaches_spell: None,
                note_content: None,
                wear_locations: vec![],
                armor_class: None,
                protects: vec![],
                flags: ItemFlags {
                    no_get: true, // Can't pick up corpses
                    is_corpse: true,
                    corpse_owner: name.clone(),
                    corpse_created_at: now,
                    corpse_is_player: is_player,
                    corpse_gold: 0,
                    ..Default::default()
                },
                weight: 100,
                value: 0,
                location: ItemLocation::Room(room_uuid),
                damage_dice_count: 0,
                damage_dice_sides: 0,
                damage_type: Default::default(),
                two_handed: false,
                weapon_skill: None,
                // Container fields - corpses are containers
                container_contents: vec![],
                container_max_items: 1000,
                container_max_weight: 10000,
                container_closed: false,
                container_locked: false,
                container_key_vnum: None,
                weight_reduction: 0,
                // Liquid container fields
                liquid_type: LiquidType::default(),
                liquid_current: 0,
                liquid_max: 0,
                liquid_poisoned: false,
                liquid_effects: vec![],
                // Food fields
                food_nutrition: 0,
                food_poisoned: false,
                food_spoil_duration: 0,
                food_created_at: None,
                food_effects: vec![],
                food_spoilage_points: 0.0,
                preservation_level: 0,
                // Level/stats
                level_requirement: 0,
                stat_str: 0,
                stat_dex: 0,
                stat_con: 0,
                stat_int: 0,
                stat_wis: 0,
                stat_cha: 0,
                insulation: 0,
                is_prototype: false,
                vnum: None,
                world_max_count: None,
                triggers: vec![],
                vending_stock: vec![],
                vending_sell_rate: 150,
                quality: 0,
                bait_uses: 0,
                holes: 0,
                medical_tier: 0,
                medical_uses: 0,
                treats_wound_types: vec![],
                max_treatable_wound: String::new(),
                transport_link: None,
                caliber: None,
                ammo_count: 0,
                ammo_damage_bonus: 0,
                ranged_type: None,
                magazine_size: 0,
                loaded_ammo: 0,
                loaded_ammo_bonus: 0,
                loaded_ammo_vnum: None,
                fire_mode: String::new(),
                supported_fire_modes: vec![],
                noise_level: String::new(),
                ammo_effect_type: String::new(),
                ammo_effect_duration: 0,
                ammo_effect_damage: 0,
                loaded_ammo_effect_type: String::new(),
                loaded_ammo_effect_duration: 0,
                loaded_ammo_effect_damage: 0,
                attachment_slot: String::new(),
                attachment_accuracy_bonus: 0,
                attachment_noise_reduction: 0,
                attachment_magazine_bonus: 0,
                attachment_compatible_types: Vec::new(),
                plant_prototype_vnum: String::new(),
                fertilizer_duration: 0,
                treats_infestation: String::new(),
            };

            let corpse_id = corpse.id.to_string();
            if let Err(_) = cloned_db.save_item_data(corpse) {
                return String::new();
            }
            corpse_id
        },
    );

    // transfer_inventory_to_corpse(char_name, corpse_id) -> bool
    // Moves all items from character's inventory to corpse container
    let cloned_db = db.clone();
    engine.register_fn(
        "transfer_inventory_to_corpse",
        move |char_name: String, corpse_id: String| -> bool {
            let corpse_uuid = match Uuid::parse_str(&corpse_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            // Verify character exists before transferring inventory
            if cloned_db.get_character_data(&char_name).ok().flatten().is_none() {
                return false;
            }

            let mut corpse = match cloned_db.get_item_data(&corpse_uuid) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            // Move each item from inventory to corpse container (source of truth is ItemLocation::Inventory)
            if let Ok(inventory_items) = cloned_db.get_items_in_inventory(&char_name) {
                for item in inventory_items {
                    let item_id = item.id;
                    let mut updated_item = item;
                    updated_item.location = ItemLocation::Container(corpse_uuid);
                    if let Err(_) = cloned_db.save_item_data(updated_item) {
                        continue;
                    }
                    corpse.container_contents.push(item_id);
                }
            }

            // Save corpse with updated contents
            if let Err(_) = cloned_db.save_item_data(corpse) {
                return false;
            }

            true
        },
    );

    // transfer_equipment_to_corpse(char_name, corpse_id) -> bool
    // Unequips and moves all equipped items to corpse container
    let cloned_db = db.clone();
    engine.register_fn(
        "transfer_equipment_to_corpse",
        move |char_name: String, corpse_id: String| -> bool {
            let corpse_uuid = match Uuid::parse_str(&corpse_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            // Get equipped items from database (equipment tracked via ItemLocation::Equipped)
            let equipped_items = match cloned_db.get_equipped_items(&char_name) {
                Ok(items) => items,
                Err(_) => return false,
            };

            let mut corpse = match cloned_db.get_item_data(&corpse_uuid) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            // Move each equipped item to corpse container
            for item in equipped_items {
                let item_id = item.id;
                let mut updated_item = item;
                updated_item.location = ItemLocation::Container(corpse_uuid);
                if let Err(_) = cloned_db.save_item_data(updated_item) {
                    continue;
                }
                corpse.container_contents.push(item_id);
            }

            // Save corpse with updated contents
            if let Err(_) = cloned_db.save_item_data(corpse) {
                return false;
            }

            true
        },
    );

    // transfer_mobile_items_to_corpse(mobile_id, corpse_id) -> bool
    // Transfers all inventory + equipped items from a mobile to a corpse container.
    // Clears death_only flag on each transferred item.
    let cloned_db = db.clone();
    engine.register_fn(
        "transfer_mobile_items_to_corpse",
        move |mobile_id: String, corpse_id: String| -> bool {
            let mobile_uuid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let corpse_uuid = match Uuid::parse_str(&corpse_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            let mut corpse = match cloned_db.get_item_data(&corpse_uuid) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            // Transfer inventory items
            if let Ok(inventory_items) = cloned_db.get_items_in_mobile_inventory(&mobile_uuid) {
                for item in inventory_items {
                    let item_id = item.id;
                    let mut updated_item = item;
                    updated_item.flags.death_only = false;
                    updated_item.location = ItemLocation::Container(corpse_uuid);
                    if cloned_db.save_item_data(updated_item).is_err() {
                        continue;
                    }
                    corpse.container_contents.push(item_id);
                }
            }

            // Transfer equipped items
            if let Ok(equipped_items) = cloned_db.get_items_equipped_on_mobile(&mobile_uuid) {
                for item in equipped_items {
                    let item_id = item.id;
                    let mut updated_item = item;
                    updated_item.flags.death_only = false;
                    updated_item.location = ItemLocation::Container(corpse_uuid);
                    if cloned_db.save_item_data(updated_item).is_err() {
                        continue;
                    }
                    corpse.container_contents.push(item_id);
                }
            }

            // Save corpse with updated contents
            cloned_db.save_item_data(corpse).is_ok()
        },
    );

    // set_corpse_gold(corpse_id, gold) -> bool
    // Sets gold on a corpse with +/-10% variance
    let cloned_db = db.clone();
    engine.register_fn("set_corpse_gold", move |corpse_id: String, gold: i64| -> bool {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let mut corpse = match cloned_db.get_item_data(&corpse_uuid) {
            Ok(Some(c)) => c,
            _ => return false,
        };

        let amount = if gold > 0 {
            let mut rng = rand::thread_rng();
            let variance = ((gold as f64 * 0.1) as i64).max(1);
            let min = (gold - variance).max(0);
            let max = gold + variance;
            rng.gen_range(min..=max)
        } else {
            0
        };

        corpse.flags.corpse_gold = amount;
        cloned_db.save_item_data(corpse).is_ok()
    });

    // transfer_gold_to_corpse(char_name, corpse_id) -> bool
    // Moves character's gold to corpse
    let cloned_db = db.clone();
    engine.register_fn(
        "transfer_gold_to_corpse",
        move |char_name: String, corpse_id: String| -> bool {
            let corpse_uuid = match Uuid::parse_str(&corpse_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            let char = match cloned_db.get_character_data(&char_name) {
                Ok(Some(c)) => c,
                _ => return false,
            };

            let gold = char.gold;

            // Clear character gold
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.gold = 0;
                let _ = cloned_db.save_character_data(char);
            }

            // Add gold to corpse (cast i32 to i64)
            if let Ok(Some(mut corpse)) = cloned_db.get_item_data(&corpse_uuid) {
                corpse.flags.corpse_gold = gold as i64;
                if let Err(_) = cloned_db.save_item_data(corpse) {
                    return false;
                }
            }

            true
        },
    );

    // get_corpse_gold(corpse_id) -> i64
    // Returns gold stored in corpse
    let cloned_db = db.clone();
    engine.register_fn("get_corpse_gold", move |corpse_id: String| -> i64 {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        if let Ok(Some(corpse)) = cloned_db.get_item_data(&corpse_uuid) {
            return corpse.flags.corpse_gold;
        }
        0
    });

    // take_corpse_gold(corpse_id, char_name) -> i64
    // Takes all gold from corpse and gives to character, returns amount taken
    let cloned_db = db.clone();
    engine.register_fn("take_corpse_gold", move |corpse_id: String, char_name: String| -> i64 {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let gold = if let Ok(Some(mut corpse)) = cloned_db.get_item_data(&corpse_uuid) {
            let g = corpse.flags.corpse_gold;
            corpse.flags.corpse_gold = 0;
            let _ = cloned_db.save_item_data(corpse);
            g
        } else {
            return 0;
        };

        if gold > 0 {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.gold += gold as i32; // Cast i64 to i32
                let _ = cloned_db.save_character_data(char);
            }
        }

        gold
    });

    // ========== Death Processing Functions ==========

    // respawn_character(char_name) -> bool
    // Respawns character at spawn point with 25% HP
    let cloned_db = db.clone();
    engine.register_fn("respawn_character", move |char_name: String| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            // Get spawn room (default to starting room)
            let spawn_room = char
                .spawn_room_id
                .unwrap_or_else(|| Uuid::parse_str(STARTING_ROOM_ID).unwrap());

            // Move to spawn room
            char.current_room_id = spawn_room;

            // Restore 25% HP
            char.hp = char.max_hp / 4;
            if char.hp < 1 {
                char.hp = 1;
            }

            // Clear death state
            char.is_unconscious = false;
            char.bleedout_rounds_remaining = 0;

            // Clear wounds
            char.wounds.clear();

            // Exit combat
            char.combat.in_combat = false;
            char.combat.targets.clear();
            char.combat.stun_rounds_remaining = 0;
            char.combat.ammo_depleted = 0;

            if let Err(_) = cloned_db.save_character_data(char) {
                return false;
            }
            return true;
        }
        false
    });

    // get_character_spawn_room(char_name) -> String
    // Returns character's spawn room ID (or starting room if not set)
    let cloned_db = db.clone();
    engine.register_fn("get_character_spawn_room", move |char_name: String| -> String {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char
                .spawn_room_id
                .map(|u| u.to_string())
                .unwrap_or_else(|| STARTING_ROOM_ID.to_string());
        }
        STARTING_ROOM_ID.to_string()
    });

    // set_character_spawn_room(char_name, room_id) -> bool
    // Sets the character's spawn room (used by bind command)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_character_spawn_room",
        move |char_name: String, room_id: String| -> bool {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                if room_id.is_empty() {
                    char.spawn_room_id = None;
                } else if let Ok(uuid) = Uuid::parse_str(&room_id) {
                    // Verify the room exists
                    if cloned_db.get_room_data(&uuid).ok().flatten().is_some() {
                        char.spawn_room_id = Some(uuid);
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // ========== Corpse Decay Functions ==========

    // get_all_corpses() -> Array of Maps with corpse info
    // Returns all corpse items in the game
    let cloned_db = db.clone();
    engine.register_fn("get_all_corpses", move || -> rhai::Array {
        let mut result = rhai::Array::new();

        if let Ok(items) = cloned_db.list_all_items() {
            for item in items {
                if item.flags.is_corpse {
                    let mut map = Map::new();
                    map.insert("id".into(), Dynamic::from(item.id.to_string()));
                    map.insert("name".into(), Dynamic::from(item.name.clone()));
                    map.insert("owner".into(), Dynamic::from(item.flags.corpse_owner.clone()));
                    map.insert("created_at".into(), Dynamic::from(item.flags.corpse_created_at));
                    map.insert("is_player".into(), Dynamic::from(item.flags.corpse_is_player));
                    map.insert("gold".into(), Dynamic::from(item.flags.corpse_gold));
                    result.push(Dynamic::from(map));
                }
            }
        }

        result
    });

    // should_corpse_decay(corpse_id) -> bool
    // Returns true if corpse should decay (based on age)
    let cloned_db = db.clone();
    engine.register_fn("should_corpse_decay", move |corpse_id: String| -> bool {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(corpse)) = cloned_db.get_item_data(&corpse_uuid) {
            if !corpse.flags.is_corpse {
                return false;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let age = now - corpse.flags.corpse_created_at;

            // Player corpses decay after 1 hour (3600 seconds)
            // Mobile corpses decay after 10 minutes (600 seconds)
            if corpse.flags.corpse_is_player {
                return age >= 3600;
            } else {
                return age >= 600;
            }
        }

        false
    });

    // remove_corpse(corpse_id) -> bool
    // Removes corpse and destroys all contents
    let cloned_db = db.clone();
    engine.register_fn("remove_corpse", move |corpse_id: String| -> bool {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(corpse)) = cloned_db.get_item_data(&corpse_uuid) {
            // Delete all items in the corpse
            for item_id in &corpse.container_contents {
                let _ = cloned_db.delete_item(item_id);
            }

            // Delete the corpse itself
            if let Err(_) = cloned_db.delete_item(&corpse_uuid) {
                return false;
            }

            return true;
        }

        false
    });

    // get_corpse_room(corpse_id) -> String
    // Returns the room ID where a corpse is located
    let cloned_db = db.clone();
    engine.register_fn("get_corpse_room", move |corpse_id: String| -> String {
        let corpse_uuid = match Uuid::parse_str(&corpse_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };

        if let Ok(Some(corpse)) = cloned_db.get_item_data(&corpse_uuid) {
            if let ItemLocation::Room(room_id) = corpse.location {
                return room_id.to_string();
            }
        }

        String::new()
    });

    // ========== Ongoing Effect Functions ==========

    // apply_ongoing_effect(char_name, effect_type, rounds, damage, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "apply_ongoing_effect",
        move |char_name: String, effect_type: String, rounds: i64, damage: i64, body_part: String| -> bool {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                char.ongoing_effects.push(OngoingEffect {
                    effect_type,
                    rounds_remaining: rounds as i32,
                    damage_per_round: damage as i32,
                    body_part,
                });
                return cloned_db.save_character_data(char).is_ok();
            }
            false
        },
    );

    // apply_mobile_ongoing_effect(mobile_id, effect_type, rounds, damage, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "apply_mobile_ongoing_effect",
        move |mobile_id: String, effect_type: String, rounds: i64, damage: i64, body_part: String| -> bool {
            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                mobile.ongoing_effects.push(OngoingEffect {
                    effect_type,
                    rounds_remaining: rounds as i32,
                    damage_per_round: damage as i32,
                    body_part,
                });
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
            false
        },
    );

    // get_ongoing_effects(char_name) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("get_ongoing_effects", move |char_name: String| -> rhai::Array {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            return char.ongoing_effects.iter().map(ongoing_effect_to_map).collect();
        }
        vec![]
    });

    // get_mobile_ongoing_effects(mobile_id) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_ongoing_effects", move |mobile_id: String| -> rhai::Array {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return vec![],
        };
        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            return mobile.ongoing_effects.iter().map(ongoing_effect_to_map).collect();
        }
        vec![]
    });

    // clear_ongoing_effects(char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_ongoing_effects", move |char_name: String| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            char.ongoing_effects.clear();
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // clear_mobile_ongoing_effects(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_mobile_ongoing_effects", move |mobile_id: String| -> bool {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            mobile.ongoing_effects.clear();
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // clear_ongoing_effects_for_part(char_name, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "clear_ongoing_effects_for_part",
        move |char_name: String, body_part: String| -> bool {
            if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
                let original_len = char.ongoing_effects.len();
                char.ongoing_effects.retain(|e| e.body_part != body_part);
                if char.ongoing_effects.len() != original_len {
                    return cloned_db.save_character_data(char).is_ok();
                }
            }
            false
        },
    );

    // clear_mobile_ongoing_effects_for_part(mobile_id, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "clear_mobile_ongoing_effects_for_part",
        move |mobile_id: String, body_part: String| -> bool {
            let mid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
                let original_len = mobile.ongoing_effects.len();
                mobile.ongoing_effects.retain(|e| e.body_part != body_part);
                if mobile.ongoing_effects.len() != original_len {
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // process_ongoing_effects_tick(char_name) -> i64
    // Applies damage from ongoing effects, decrements rounds, removes expired. Returns total damage.
    let cloned_db = db.clone();
    engine.register_fn("process_ongoing_effects_tick", move |char_name: String| -> i64 {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            let raw: i32 = char.ongoing_effects.iter().map(|e| e.damage_per_round).sum();
            let total_damage = apply_damage_reduction(raw, &char.active_buffs);
            if total_damage > 0 {
                char.hp -= total_damage;
            }
            // Decrement rounds and remove expired
            for effect in char.ongoing_effects.iter_mut() {
                effect.rounds_remaining -= 1;
            }
            char.ongoing_effects.retain(|e| e.rounds_remaining > 0);
            let _ = cloned_db.save_character_data(char);
            return total_damage as i64;
        }
        0
    });

    // process_mobile_ongoing_effects_tick(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("process_mobile_ongoing_effects_tick", move |mobile_id: String| -> i64 {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };
        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            let raw: i32 = mobile.ongoing_effects.iter().map(|e| e.damage_per_round).sum();
            let total_damage = apply_damage_reduction(raw, &mobile.active_buffs);
            if total_damage > 0 {
                mobile.current_hp -= total_damage;
            }
            for effect in mobile.ongoing_effects.iter_mut() {
                effect.rounds_remaining -= 1;
            }
            mobile.ongoing_effects.retain(|e| e.rounds_remaining > 0);
            let _ = cloned_db.save_mobile_data(mobile);
            return total_damage as i64;
        }
        0
    });

    // ========== Scar Functions ==========

    // add_scar(char_name, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_scar", move |char_name: String, body_part: String| -> bool {
        if let Ok(Some(mut char)) = cloned_db.get_character_data(&char_name) {
            let count = char.scars.entry(body_part).or_insert(0);
            *count += 1;
            return cloned_db.save_character_data(char).is_ok();
        }
        false
    });

    // add_mobile_scar(mobile_id, body_part) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_mobile_scar", move |mobile_id: String, body_part: String| -> bool {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mid) {
            let count = mobile.scars.entry(body_part).or_insert(0);
            *count += 1;
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // get_character_scars(char_name) -> Map (body_part -> count)
    let cloned_db = db.clone();
    engine.register_fn("get_character_scars", move |char_name: String| -> Map {
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name) {
            let mut map = Map::new();
            for (part, count) in &char.scars {
                map.insert(part.clone().into(), Dynamic::from(*count as i64));
            }
            return map;
        }
        Map::new()
    });

    // get_mobile_scars(mobile_id) -> Map (body_part -> count)
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_scars", move |mobile_id: String| -> Map {
        let mid = match Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return Map::new(),
        };
        if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&mid) {
            let mut map = Map::new();
            for (part, count) in &mobile.scars {
                map.insert(part.clone().into(), Dynamic::from(*count as i64));
            }
            return map;
        }
        Map::new()
    });

    // ========== Wound Gameplay Effect Functions ==========

    // get_effective_max_hp(char_name) -> i64
    // Returns max HP after torso wound penalty
    let cloned_db = db.clone();
    engine.register_fn("get_effective_max_hp", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let torso_penalty = character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::Torso)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                if torso_penalty > 0 {
                    (character.max_hp as i64 * (100 - torso_penalty) as i64 / 100).max(1)
                } else {
                    character.max_hp as i64
                }
            }
            _ => 0,
        }
    });

    // get_arm_wound_penalty(char_name, side) -> i64
    // Returns max penalty from arm+hand wounds on "left" or "right" side
    let cloned_db = db.clone();
    engine.register_fn("get_arm_wound_penalty", move |char_name: String, side: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let (arm_bp, hand_bp) = if side.to_lowercase() == "left" {
                    (BodyPart::LeftArm, BodyPart::LeftHand)
                } else {
                    (BodyPart::RightArm, BodyPart::RightHand)
                };
                character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == arm_bp || w.body_part == hand_bp)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0) as i64
            }
            _ => 0,
        }
    });

    // is_arm_disabled(char_name, side) -> bool
    // Returns true if either arm or hand on that side is Disabled
    let cloned_db = db.clone();
    engine.register_fn("is_arm_disabled", move |char_name: String, side: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let (arm_bp, hand_bp) = if side.to_lowercase() == "left" {
                    (BodyPart::LeftArm, BodyPart::LeftHand)
                } else {
                    (BodyPart::RightArm, BodyPart::RightHand)
                };
                character
                    .wounds
                    .iter()
                    .any(|w| (w.body_part == arm_bp || w.body_part == hand_bp) && w.level == WoundLevel::Disabled)
            }
            _ => false,
        }
    });

    // are_both_arms_disabled(char_name) -> bool
    // Returns true if both sides have a disabled arm or hand
    let cloned_db = db.clone();
    engine.register_fn("are_both_arms_disabled", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let left_disabled = character.wounds.iter().any(|w| {
                    (w.body_part == BodyPart::LeftArm || w.body_part == BodyPart::LeftHand)
                        && w.level == WoundLevel::Disabled
                });
                let right_disabled = character.wounds.iter().any(|w| {
                    (w.body_part == BodyPart::RightArm || w.body_part == BodyPart::RightHand)
                        && w.level == WoundLevel::Disabled
                });
                left_disabled && right_disabled
            }
            _ => false,
        }
    });

    // get_eye_wound_penalty(char_name) -> i64
    // Returns combined vision penalty from eye wounds
    let cloned_db = db.clone();
    engine.register_fn("get_eye_wound_penalty", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let left = character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::LeftEye)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                let right = character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::RightEye)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                if left > 0 && right > 0 {
                    (left + right).min(95) as i64
                } else {
                    (std::cmp::max(left, right) / 2) as i64
                }
            }
            _ => 0,
        }
    });

    // get_ear_wound_penalty(char_name) -> i64
    // Returns combined hearing penalty from ear wounds
    let cloned_db = db.clone();
    engine.register_fn("get_ear_wound_penalty", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => {
                let left = character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::LeftEar)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                let right = character
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::RightEar)
                    .map(|w| w.level.penalty())
                    .max()
                    .unwrap_or(0);
                if left > 0 && right > 0 {
                    (left + right).min(95) as i64
                } else {
                    (std::cmp::max(left, right) / 2) as i64
                }
            }
            _ => 0,
        }
    });

    // get_jaw_wound_penalty(char_name) -> i64
    // Returns jaw wound penalty percentage
    let cloned_db = db.clone();
    engine.register_fn("get_jaw_wound_penalty", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character
                .wounds
                .iter()
                .filter(|w| w.body_part == BodyPart::Jaw)
                .map(|w| w.level.penalty())
                .max()
                .unwrap_or(0) as i64,
            _ => 0,
        }
    });

    // garble_hearing(text, penalty) -> String
    // Replaces characters with dots (muffled) based on penalty%
    engine.register_fn("garble_hearing", |text: String, penalty: i64| -> String {
        let mut rng = rand::thread_rng();
        let mut result = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch == ' ' || ch == '.' || ch == '!' || ch == '?' || ch == ',' {
                result.push(ch);
            } else if rng.gen_range(1..=100) <= penalty as i32 {
                result.push('.');
            } else {
                result.push(ch);
            }
        }
        result
    });

    // garble_jaw_speech(text, penalty) -> String
    // Replaces dental/labial consonants based on penalty%
    engine.register_fn("garble_jaw_speech", |text: String, penalty: i64| -> String {
        let mut rng = rand::thread_rng();
        let mut result = String::with_capacity(text.len() + 10);
        for ch in text.chars() {
            if rng.gen_range(1..=100) <= penalty as i32 {
                match ch {
                    't' | 'd' => result.push_str("uh"),
                    'T' | 'D' => result.push_str("Uh"),
                    'b' | 'p' => result.push_str("mm"),
                    'B' | 'P' => result.push_str("Mm"),
                    's' => result.push_str("th"),
                    'S' => result.push_str("Th"),
                    'f' => result.push('h'),
                    'F' => result.push('H'),
                    _ => result.push(ch),
                }
            } else {
                result.push(ch);
            }
        }
        result
    });
}

// Helper to convert Wound to Rhai Map
fn wound_to_map(wound: &Wound) -> Dynamic {
    let mut map = Map::new();
    map.insert(
        "body_part".into(),
        Dynamic::from(wound.body_part.to_display_string().to_string()),
    );
    map.insert(
        "level".into(),
        Dynamic::from(wound.level.to_display_string().to_string()),
    );
    map.insert(
        "wound_type".into(),
        Dynamic::from(wound.wound_type.to_display_string().to_string()),
    );
    map.insert(
        "bleeding_severity".into(),
        Dynamic::from(wound.bleeding_severity as i64),
    );
    map.insert("penalty".into(), Dynamic::from(wound.level.penalty() as i64));
    Dynamic::from(map)
}

fn ongoing_effect_to_map(effect: &OngoingEffect) -> Dynamic {
    let mut map = Map::new();
    map.insert("effect_type".into(), Dynamic::from(effect.effect_type.clone()));
    map.insert("rounds_remaining".into(), Dynamic::from(effect.rounds_remaining as i64));
    map.insert("damage_per_round".into(), Dynamic::from(effect.damage_per_round as i64));
    map.insert("body_part".into(), Dynamic::from(effect.body_part.clone()));
    Dynamic::from(map)
}
