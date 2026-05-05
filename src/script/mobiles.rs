// src/script/mobiles.rs
// Mobile/NPC system functions

use crate::db::Db;
use crate::{
    ActivityState, DamageType, EffectType, MobileData, MobileFlags, RememberedEnemy, RoutineEntry,
    find_active_entry,
};
use rhai::Engine;
use std::collections::HashMap;
use std::sync::Arc;

/// Maximum number of remembered enemies a MOB_MEMORY mob can hold. Excess
/// entries fall off the front (FIFO).
pub const MEMORY_CAP: usize = 10;

/// How long a remembered enemy stays in memory after being recorded /
/// re-attacked (wall-clock seconds; 30 minutes).
pub const MEMORY_DURATION_SECS: i64 = 1800;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Stamps `attacker_name` into `mob.remembered_enemies` if `mob.flags.memory`.
/// Re-attacking refreshes the timer; FIFO eviction at MEMORY_CAP. The caller
/// is responsible for persisting the mob afterwards.
pub fn record_mob_memory(mob: &mut MobileData, attacker_name: &str) {
    if !mob.flags.memory {
        return;
    }
    let key = attacker_name.to_lowercase();
    if key.is_empty() {
        return;
    }
    let expires = now_secs() + MEMORY_DURATION_SECS;
    if let Some(existing) = mob
        .remembered_enemies
        .iter_mut()
        .find(|e| e.name.to_lowercase() == key)
    {
        existing.expires_at_secs = expires;
        return;
    }
    mob.remembered_enemies.push(RememberedEnemy {
        name: key,
        expires_at_secs: expires,
    });
    while mob.remembered_enemies.len() > MEMORY_CAP {
        mob.remembered_enemies.remove(0);
    }
}

/// Returns true if `mob` should treat `attacker_name` as a remembered enemy
/// right now. Lazy-prunes expired entries (caller persists if `pruned`
/// matters). Returns `(remembers, pruned_any)`.
pub fn check_and_prune_memory(mob: &mut MobileData, attacker_name: &str) -> (bool, bool) {
    let now = now_secs();
    let before = mob.remembered_enemies.len();
    mob.remembered_enemies.retain(|e| e.expires_at_secs > now);
    let pruned = mob.remembered_enemies.len() != before;
    let key = attacker_name.to_lowercase();
    let remembers = mob
        .remembered_enemies
        .iter()
        .any(|e| e.name.to_lowercase() == key);
    (remembers, pruned)
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    fn mob_with_memory(flag: bool) -> MobileData {
        let mut m = MobileData::new("ogre".to_string());
        m.flags.memory = flag;
        m
    }

    #[test]
    fn flag_off_is_noop() {
        let mut m = mob_with_memory(false);
        record_mob_memory(&mut m, "alice");
        assert!(m.remembered_enemies.is_empty());
    }

    #[test]
    fn records_attacker_lowercased() {
        let mut m = mob_with_memory(true);
        record_mob_memory(&mut m, "Alice");
        assert_eq!(m.remembered_enemies.len(), 1);
        assert_eq!(m.remembered_enemies[0].name, "alice");
    }

    #[test]
    fn dedupes_and_refreshes() {
        let mut m = mob_with_memory(true);
        record_mob_memory(&mut m, "alice");
        let first = m.remembered_enemies[0].expires_at_secs;
        std::thread::sleep(std::time::Duration::from_millis(1100));
        record_mob_memory(&mut m, "Alice");
        assert_eq!(m.remembered_enemies.len(), 1);
        assert!(m.remembered_enemies[0].expires_at_secs > first);
    }

    #[test]
    fn fifo_eviction_at_cap() {
        let mut m = mob_with_memory(true);
        for i in 0..(MEMORY_CAP + 2) {
            record_mob_memory(&mut m, &format!("foe{}", i));
        }
        assert_eq!(m.remembered_enemies.len(), MEMORY_CAP);
        assert_eq!(m.remembered_enemies[0].name, "foe2");
    }

    #[test]
    fn check_prunes_expired() {
        let mut m = mob_with_memory(true);
        m.remembered_enemies.push(RememberedEnemy {
            name: "ghost".to_string(),
            expires_at_secs: 1,
        });
        let (remembers, pruned) = check_and_prune_memory(&mut m, "ghost");
        assert!(!remembers);
        assert!(pruned);
        assert!(m.remembered_enemies.is_empty());
    }

    #[test]
    fn check_finds_active() {
        let mut m = mob_with_memory(true);
        record_mob_memory(&mut m, "Bob");
        let (remembers, _) = check_and_prune_memory(&mut m, "bob");
        assert!(remembers);
    }
}

/// Register mobile-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Mobile/NPC System ==========

    // Register MobileFlags type with getters/setters
    engine.register_type_with_name::<MobileFlags>("MobileFlags");
    register_bool_flags!(
        engine,
        MobileFlags,
        aggressive,
        sentinel,
        scavenger,
        shopkeeper,
        no_attack,
        healer,
        leasing_agent,
        cowardly,
        can_open_doors,
        guard,
        helper,
        thief,
        cant_swim,
        poisonous,
        fiery,
        chilling,
        corrosive,
        shocking,
        unique,
        stay_zone,
        aware,
        memory,
        no_sleep,
        no_blind,
        no_bash,
        no_summon
    );

    // Register MobileData type with getters
    engine
        .register_type_with_name::<MobileData>("MobileData")
        .register_get("id", |m: &mut MobileData| m.id.to_string())
        .register_get("name", |m: &mut MobileData| m.name.clone())
        .register_set("name", |m: &mut MobileData, v: String| m.name = v)
        .register_get("short_desc", |m: &mut MobileData| m.short_desc.clone())
        .register_set("short_desc", |m: &mut MobileData, v: String| m.short_desc = v)
        .register_get("long_desc", |m: &mut MobileData| m.long_desc.clone())
        .register_set("long_desc", |m: &mut MobileData, v: String| m.long_desc = v)
        .register_get("keywords", |m: &mut MobileData| {
            m.keywords
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("current_room_id", |m: &mut MobileData| {
            m.current_room_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("is_prototype", |m: &mut MobileData| m.is_prototype)
        .register_get("vnum", |m: &mut MobileData| m.vnum.clone())
        .register_get("world_max_count", |m: &mut MobileData| m.world_max_count.unwrap_or(0) as i64)
        .register_get("has_world_max_count", |m: &mut MobileData| m.world_max_count.is_some())
        .register_get("faction", |m: &mut MobileData| m.faction.clone().unwrap_or_default())
        .register_get("level", |m: &mut MobileData| m.level as i64)
        .register_get("max_hp", |m: &mut MobileData| m.max_hp as i64)
        .register_get("current_hp", |m: &mut MobileData| m.current_hp as i64)
        .register_get("hp", |m: &mut MobileData| m.current_hp as i64)
        .register_set("hp", |m: &mut MobileData, val: i64| m.current_hp = val as i32)
        .register_get("max_stamina", |m: &mut MobileData| m.max_stamina as i64)
        .register_get("current_stamina", |m: &mut MobileData| m.current_stamina as i64)
        .register_get("stamina", |m: &mut MobileData| m.current_stamina as i64)
        .register_set("stamina", |m: &mut MobileData, val: i64| m.current_stamina = val as i32)
        .register_get("damage_dice", |m: &mut MobileData| m.damage_dice.clone())
        .register_get("damage_type", |m: &mut MobileData| {
            m.damage_type.to_display_string().to_string()
        })
        .register_get("armor_class", |m: &mut MobileData| m.armor_class as i64)
        .register_get("hit_modifier", |m: &mut MobileData| m.hit_modifier as i64)
        .register_get("gold", |m: &mut MobileData| m.gold as i64)
        .register_get("stat_str", |m: &mut MobileData| m.stat_str as i64)
        .register_get("stat_dex", |m: &mut MobileData| m.stat_dex as i64)
        .register_get("stat_con", |m: &mut MobileData| m.stat_con as i64)
        .register_get("stat_int", |m: &mut MobileData| m.stat_int as i64)
        .register_get("stat_wis", |m: &mut MobileData| m.stat_wis as i64)
        .register_get("stat_cha", |m: &mut MobileData| m.stat_cha as i64)
        .register_get("flags", |m: &mut MobileData| m.flags.clone())
        .register_get("shop_stock", |m: &mut MobileData| {
            m.shop_stock
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_buy_rate", |m: &mut MobileData| m.shop_buy_rate as i64)
        .register_get("shop_sell_rate", |m: &mut MobileData| m.shop_sell_rate as i64)
        .register_get("healer_type", |m: &mut MobileData| m.healer_type.clone())
        .register_get("healing_free", |m: &mut MobileData| m.healing_free)
        .register_get("healing_cost_multiplier", |m: &mut MobileData| {
            m.healing_cost_multiplier as i64
        })
        .register_get("leasing_area_id", |m: &mut MobileData| {
            m.leasing_area_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("property_templates", |m: &mut MobileData| {
            m.property_templates
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_buys_categories", |m: &mut MobileData| {
            m.shop_buys_categories
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_preset_vnum", |m: &mut MobileData| m.shop_preset_vnum.clone())
        .register_get("shop_extra_types", |m: &mut MobileData| {
            m.shop_extra_types
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_extra_categories", |m: &mut MobileData| {
            m.shop_extra_categories
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_deny_types", |m: &mut MobileData| {
            m.shop_deny_types
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_deny_categories", |m: &mut MobileData| {
            m.shop_deny_categories
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("shop_min_value", |m: &mut MobileData| m.shop_min_value as i64)
        .register_get("shop_max_value", |m: &mut MobileData| m.shop_max_value as i64)
        .register_get("pursuit_target_name", |m: &mut MobileData| {
            m.pursuit_target_name.clone()
        })
        .register_get("pursuit_direction", |m: &mut MobileData| m.pursuit_direction.clone())
        .register_get("pursuit_certain", |m: &mut MobileData| m.pursuit_certain)
        .register_get("pursuit_target_room", |m: &mut MobileData| {
            m.pursuit_target_room.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("embedded_projectiles", |m: &mut MobileData| {
            m.embedded_projectiles
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("perception", |m: &mut MobileData| m.perception as i64)
        // Migrant/resident fields
        .register_get("resident_of", |m: &mut MobileData| {
            m.resident_of.clone().unwrap_or_default()
        })
        .register_get("household_id", |m: &mut MobileData| {
            m.household_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("has_characteristics", |m: &mut MobileData| m.characteristics.is_some())
        .register_get("age", |m: &mut MobileData| {
            m.characteristics.as_ref().map(|c| c.age as i64).unwrap_or(0)
        })
        .register_get("age_label", |m: &mut MobileData| {
            m.characteristics
                .as_ref()
                .map(|c| c.age_label.clone())
                .unwrap_or_default()
        })
        .register_get("gender", |m: &mut MobileData| {
            m.characteristics.as_ref().map(|c| c.gender.clone()).unwrap_or_default()
        })
        .register_get("birth_day", |m: &mut MobileData| {
            m.characteristics.as_ref().map(|c| c.birth_day).unwrap_or(0)
        })
        .register_get("life_stage", |m: &mut MobileData| {
            m.characteristics
                .as_ref()
                .map(|c| crate::types::life_stage_for_age(c.age).to_display_string().to_string())
                .unwrap_or_default()
        })
        .register_get("relationships", |m: &mut MobileData| {
            m.relationships
                .iter()
                .map(|r| {
                    let mut map = rhai::Map::new();
                    map.insert("other_id".into(), rhai::Dynamic::from(r.other_id.to_string()));
                    map.insert(
                        "kind".into(),
                        rhai::Dynamic::from(r.kind.to_display_string().to_string()),
                    );
                    rhai::Dynamic::from(map)
                })
                .collect::<Vec<_>>()
        });

    // new_mobile(name) -> MobileData
    engine.register_fn("new_mobile", |name: String| MobileData::new(name));

    // get_mobile_data(mobile_id) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_data", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mobile)) => rhai::Dynamic::from(mobile),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // save_mobile_data(mobile) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_mobile_data", move |mobile: MobileData| {
        cloned_db.save_mobile_data(mobile).is_ok()
    });

    // delete_mobile(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_mobile", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            cloned_db.delete_mobile(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // list_all_mobiles() -> Array of MobileData
    let cloned_db = db.clone();
    engine.register_fn("list_all_mobiles", move || {
        cloned_db
            .list_all_mobiles()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // get_mobiles_in_room(room_id) -> Array of MobileData
    let cloned_db = db.clone();
    engine.register_fn("get_mobiles_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_mobiles_in_room(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // get_mobile_by_vnum(vnum) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_by_vnum", move |vnum: String| {
        match cloned_db.get_mobile_by_vnum(&vnum) {
            Ok(Some(mobile)) => rhai::Dynamic::from(mobile),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // search_mobiles(keyword) -> Array of MobileData
    let cloned_db = db.clone();
    engine.register_fn("search_mobiles", move |keyword: String| {
        cloned_db
            .search_mobiles(&keyword)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // find_mobile_by_keyword_anywhere(keyword) -> MobileData or ()
    // World-wide non-prototype mob lookup. Skips prototypes, admin
    // mobiles, and no_attack mobiles (shopkeepers / healers / guards
    // shouldn't be summon targets). Returns the first match by name
    // contains-keyword or any keyword starts-with-keyword.
    let cloned_db = db.clone();
    engine.register_fn("find_mobile_by_keyword_anywhere", move |keyword: String| {
        let lower = keyword.to_lowercase();
        if lower.is_empty() {
            return rhai::Dynamic::UNIT;
        }
        let mobiles = match cloned_db.list_all_mobiles() {
            Ok(m) => m,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        for mobile in mobiles {
            if mobile.is_prototype {
                continue;
            }
            if mobile.flags.no_attack {
                continue;
            }
            if mobile.name.to_lowercase().contains(&lower) {
                return rhai::Dynamic::from(mobile);
            }
            if mobile
                .keywords
                .iter()
                .any(|kw| kw.to_lowercase().starts_with(&lower))
            {
                return rhai::Dynamic::from(mobile);
            }
        }
        rhai::Dynamic::UNIT
    });

    // move_mobile_to_room(mobile_id, room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("move_mobile_to_room", move |mobile_id: String, room_id: String| {
        let mobile_uuid = uuid::Uuid::parse_str(&mobile_id).ok();
        let room_uuid = uuid::Uuid::parse_str(&room_id).ok();
        match (mobile_uuid, room_uuid) {
            (Some(mid), Some(rid)) => cloned_db.move_mobile_to_room(&mid, &rid).unwrap_or(false),
            _ => false,
        }
    });

    // spawn_mobile_from_prototype(vnum) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn("spawn_mobile_from_prototype", move |vnum: String| -> rhai::Dynamic {
        match cloned_db.spawn_mobile_from_prototype(&vnum) {
            Ok(Some(mobile)) => rhai::Dynamic::from(mobile),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // refresh_mobile_from_prototype(mobile_id) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn(
        "refresh_mobile_from_prototype",
        move |mobile_id: String| -> rhai::Dynamic {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.refresh_mobile_from_prototype(&uuid) {
                    Ok(Some(mobile)) => rhai::Dynamic::from(mobile),
                    _ => rhai::Dynamic::UNIT,
                }
            } else {
                rhai::Dynamic::UNIT
            }
        },
    );

    // get_mobile_instances_by_vnum(vnum) -> Array of MobileData
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_instances_by_vnum", move |vnum: String| {
        cloned_db
            .get_mobile_instances_by_vnum(&vnum)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // set_mobile_short_desc(mobile_id, desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_short_desc", move |mobile_id: String, desc: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.short_desc = desc;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_long_desc(mobile_id, desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_long_desc", move |mobile_id: String, desc: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.long_desc = desc;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_keywords(mobile_id, keywords_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_keywords",
        move |mobile_id: String, keywords: rhai::Array| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.keywords = keywords
                        .into_iter()
                        .filter_map(|d| d.try_cast::<String>())
                        .map(|s| s.to_lowercase())
                        .collect();
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // set_mobile_stat(mobile_id, stat_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_stat",
        move |mobile_id: String, stat_name: String, value: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    match stat_name.to_lowercase().as_str() {
                        "str" | "strength" => mobile.stat_str = value as i32,
                        "dex" | "dexterity" => mobile.stat_dex = value as i32,
                        "con" | "constitution" => mobile.stat_con = value as i32,
                        "int" | "intelligence" => mobile.stat_int = value as i32,
                        "wis" | "wisdom" => mobile.stat_wis = value as i32,
                        "cha" | "charisma" => mobile.stat_cha = value as i32,
                        _ => return false,
                    }
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // set_mobile_level(mobile_id, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_level", move |mobile_id: String, level: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.level = level as i32;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_hp(mobile_id, max_hp) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_hp", move |mobile_id: String, max_hp: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.max_hp = max_hp as i32;
                mobile.current_hp = max_hp as i32; // Set current HP to max
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_damage(mobile_id, damage_dice) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_damage", move |mobile_id: String, damage_dice: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.damage_dice = damage_dice;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_damage_type(mobile_id, damage_type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_damage_type",
        move |mobile_id: String, damage_type_str: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if let Some(dt) = DamageType::from_str(&damage_type_str) {
                        mobile.damage_type = dt;
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_mobile_ac(mobile_id, armor_class) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_ac", move |mobile_id: String, armor_class: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.armor_class = armor_class as i32;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_hit_modifier(mobile_id, hit_modifier) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_hit_modifier",
        move |mobile_id: String, hit_modifier: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.hit_modifier = hit_modifier as i32;
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // set_mobile_gold(mobile_id, gold) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_gold", move |mobile_id: String, gold: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.gold = gold as i32;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_level_stats(level) -> Map with HP, AC, damage, hit_modifier, stats
    // Level stat table for mobile difficulty scaling:
    // Level 1: HP 15, AC 0, 1d4, hit -2, stats 10 (Trivial)
    // Level 2: HP 25, AC 1, 1d6, hit -1, stats 10 (Easy)
    // Level 3: HP 40, AC 2, 1d6+1, hit 0, stats 11 (Normal)
    // Level 4: HP 60, AC 3, 1d8+1, hit +1, stats 12 (Challenging)
    // Level 5: HP 80, AC 4, 1d8+2, hit +2, stats 13 (Tough)
    // Level 6: HP 100, AC 5, 2d6, hit +3, stats 14 (Dangerous)
    // Level 7: HP 130, AC 6, 2d6+2, hit +4, stats 15 (Elite)
    // Level 8: HP 170, AC 8, 2d8+2, hit +5, stats 16 (Boss)
    // Level 9: HP 220, AC 10, 2d8+4, hit +6, stats 17 (Mini-boss)
    // Level 10: HP 300, AC 12, 3d8+4, hit +8, stats 18 (Legendary)
    engine.register_fn("get_level_stats", |level: i64| -> rhai::Map {
        let mut map = rhai::Map::new();
        let (hp, ac, damage, hit_mod, stats) = match level {
            1 => (15, 0, "1d4", -2, 10),
            2 => (25, 1, "1d6", -1, 10),
            3 => (40, 2, "1d6+1", 0, 11),
            4 => (60, 3, "1d8+1", 1, 12),
            5 => (80, 4, "1d8+2", 2, 13),
            6 => (100, 5, "2d6", 3, 14),
            7 => (130, 6, "2d6+2", 4, 15),
            8 => (170, 8, "2d8+2", 5, 16),
            9 => (220, 10, "2d8+4", 6, 17),
            10 => (300, 12, "3d8+4", 8, 18),
            _ => {
                // For levels outside 1-10, clamp to nearest
                if level < 1 {
                    (15, 0, "1d4", -2, 10)
                } else {
                    (300, 12, "3d8+4", 8, 18)
                }
            }
        };
        map.insert("hp".into(), rhai::Dynamic::from(hp as i64));
        map.insert("ac".into(), rhai::Dynamic::from(ac as i64));
        map.insert("damage".into(), rhai::Dynamic::from(damage.to_string()));
        map.insert("hit_modifier".into(), rhai::Dynamic::from(hit_mod as i64));
        map.insert("stats".into(), rhai::Dynamic::from(stats as i64));
        map
    });

    // apply_level_stats(mobile_id, level) -> bool
    // Sets HP, AC, damage, hit_modifier, and all stats based on level table
    let cloned_db = db.clone();
    engine.register_fn("apply_level_stats", move |mobile_id: String, level: i64| {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };

        // Get stats from level table
        let (hp, ac, damage, hit_mod, stats) = match level {
            1 => (15, 0, "1d4", -2, 10),
            2 => (25, 1, "1d6", -1, 10),
            3 => (40, 2, "1d6+1", 0, 11),
            4 => (60, 3, "1d8+1", 1, 12),
            5 => (80, 4, "1d8+2", 2, 13),
            6 => (100, 5, "2d6", 3, 14),
            7 => (130, 6, "2d6+2", 4, 15),
            8 => (170, 8, "2d8+2", 5, 16),
            9 => (220, 10, "2d8+4", 6, 17),
            10 => (300, 12, "3d8+4", 8, 18),
            _ => {
                if level < 1 {
                    (15, 0, "1d4", -2, 10)
                } else {
                    (300, 12, "3d8+4", 8, 18)
                }
            }
        };

        // Apply all stats
        mobile.level = level as i32;
        mobile.max_hp = hp;
        mobile.current_hp = hp;
        mobile.armor_class = ac;
        mobile.damage_dice = damage.to_string();
        mobile.hit_modifier = hit_mod;
        mobile.stat_str = stats;
        mobile.stat_dex = stats;
        mobile.stat_con = stats;
        mobile.stat_int = stats;
        mobile.stat_wis = stats;
        mobile.stat_cha = stats;

        cloned_db.save_mobile_data(mobile).is_ok()
    });

    // set_mobile_flag(mobile_id, flag_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_flag",
        move |mobile_id: String, flag_name: String, value: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    match flag_name.to_lowercase().as_str() {
                        "aggressive" => mobile.flags.aggressive = value,
                        "sentinel" => mobile.flags.sentinel = value,
                        "scavenger" => mobile.flags.scavenger = value,
                        "shopkeeper" => mobile.flags.shopkeeper = value,
                        "no_attack" | "noattack" => mobile.flags.no_attack = value,
                        "healer" => mobile.flags.healer = value,
                        "leasing_agent" | "leasingagent" => mobile.flags.leasing_agent = value,
                        "cowardly" => mobile.flags.cowardly = value,
                        "can_open_doors" | "canopendoors" => mobile.flags.can_open_doors = value,
                        "guard" => mobile.flags.guard = value,
                        "thief" => mobile.flags.thief = value,
                        "cant_swim" | "cantswim" => mobile.flags.cant_swim = value,
                        "poisonous" => mobile.flags.poisonous = value,
                        "fiery" => mobile.flags.fiery = value,
                        "chilling" => mobile.flags.chilling = value,
                        "corrosive" => mobile.flags.corrosive = value,
                        "shocking" => mobile.flags.shocking = value,
                        "unique" => mobile.flags.unique = value,
                        "no_sleep" | "nosleep" => mobile.flags.no_sleep = value,
                        "no_blind" | "noblind" => mobile.flags.no_blind = value,
                        "no_bash" | "nobash" => mobile.flags.no_bash = value,
                        "no_summon" | "nosummon" => mobile.flags.no_summon = value,
                        _ => return false,
                    }
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // set_mobile_vnum(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_vnum", move |mobile_id: String, vnum: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.vnum = vnum;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_world_max_count(mobile_id, n) -> bool (n <= 0 clears the cap)
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_world_max_count", move |mobile_id: String, n: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.world_max_count = if n <= 0 { None } else { Some(n as i32) };
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_mobile_faction(mobile_id, value) -> bool. Empty string clears to None.
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_faction", move |mobile_id: String, value: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.faction = if value.is_empty() { None } else { Some(value) };
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_mobile_faction(mobile_id) -> String (empty if None)
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_faction", move |mobile_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.faction.unwrap_or_default();
            }
        }
        String::new()
    });

    // mobile_has_buff(mobile_id, effect_type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "mobile_has_buff",
        move |mobile_id: String, effect_type_str: String| -> bool {
            let effect_type = match EffectType::from_str(&effect_type_str) {
                Some(et) => et,
                None => return false,
            };
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    return mobile.active_buffs.iter().any(|b| b.effect_type == effect_type);
                }
            }
            false
        },
    );

    // count_mobiles_by_vnum(vnum) -> i64 (counts non-prototype mobiles with vnum)
    let cloned_db = db.clone();
    engine.register_fn("count_mobiles_by_vnum", move |vnum: String| -> i64 {
        match cloned_db.count_non_prototype_mobiles_by_vnum(&vnum) {
            Ok(count) => count as i64,
            Err(_) => 0,
        }
    });

    // set_mobile_prototype(mobile_id, is_prototype) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_prototype", move |mobile_id: String, is_prototype: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.is_prototype = is_prototype;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_mobile_dialogue(mobile_id, keyword) -> String (response or empty)
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_dialogue", move |mobile_id: String, keyword: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                let kw_lower = keyword.to_lowercase();
                if let Some(response) = mobile.dialogue.get(&kw_lower) {
                    return response.clone();
                }
            }
        }
        String::new()
    });

    // set_mobile_dialogue(mobile_id, keyword, response) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_dialogue",
        move |mobile_id: String, keyword: String, response: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.dialogue.insert(keyword.to_lowercase(), response);
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // remove_mobile_dialogue(mobile_id, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_mobile_dialogue", move |mobile_id: String, keyword: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                if mobile.dialogue.remove(&keyword.to_lowercase()).is_some() {
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
        }
        false
    });

    // get_all_mobile_dialogues(mobile_id) -> Map (keyword -> response)
    let cloned_db = db.clone();
    engine.register_fn("get_all_mobile_dialogues", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                let mut map = rhai::Map::new();
                for (k, v) in mobile.dialogue {
                    map.insert(k.into(), rhai::Dynamic::from(v));
                }
                return rhai::Dynamic::from(map);
            }
        }
        rhai::Dynamic::from(rhai::Map::new())
    });

    // filter_visible_mobiles(mobiles_array, viewer_name) -> Array of MobileData
    // Drops mobiles with the Invisibility buff unless the viewer has the
    // DetectInvisible buff or is admin. Used to hide invisible mobs from
    // player-facing target resolution (kill/look/examine/etc.).
    let cloned_db = db.clone();
    engine.register_fn(
        "filter_visible_mobiles",
        move |mobiles: rhai::Array, viewer_name: String| -> rhai::Array {
            let viewer = cloned_db.get_character_data(&viewer_name).ok().flatten();
            let viewer_detects = viewer
                .as_ref()
                .map(|v| {
                    v.is_admin
                        || v.active_buffs
                            .iter()
                            .any(|b| b.effect_type == crate::EffectType::DetectInvisible)
                })
                .unwrap_or(false);
            mobiles
                .into_iter()
                .filter(|mob_dyn| {
                    if let Some(mobile) = mob_dyn.clone().try_cast::<MobileData>() {
                        let invisible = mobile
                            .active_buffs
                            .iter()
                            .any(|b| b.effect_type == crate::EffectType::Invisibility);
                        return !invisible || viewer_detects;
                    }
                    true
                })
                .collect()
        },
    );

    // find_mobile_by_keyword(keyword, mobiles_array) -> MobileData or ()
    // Supports N.keyword syntax (e.g., "2.guard" returns the 2nd matching guard)
    engine.register_fn("find_mobile_by_keyword", |keyword: String, mobiles: rhai::Array| {
        let (nth, actual_keyword) = super::items::parse_nth_keyword(&keyword);
        let kw_lower = actual_keyword.to_lowercase();
        let mut match_count: usize = 0;
        for mob_dyn in mobiles {
            if let Some(mobile) = mob_dyn.clone().try_cast::<MobileData>() {
                if super::items::item_matches_keyword(&mobile.name, &mobile.keywords, &kw_lower) {
                    match_count += 1;
                    if match_count == nth {
                        return rhai::Dynamic::from(mobile);
                    }
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // === NPC Transport Route Functions ===

    // set_mobile_transport_route(mobile_id, transport_id, home_stop, dest_stop, schedule_type, schedule_arg1, schedule_arg2)
    // schedule_type: "fixed" (arg1=depart_hour, arg2=return_hour), "random" (arg1=chance), "permanent" (no args)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_transport_route",
        move |mobile_id: String,
              transport_id: String,
              home_stop: i64,
              dest_stop: i64,
              schedule_type: String,
              arg1: i64,
              arg2: i64| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let transport_uuid = match uuid::Uuid::parse_str(&transport_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            let schedule = match schedule_type.to_lowercase().as_str() {
                "fixed" => crate::NPCTravelSchedule::FixedHours {
                    depart_hour: arg1 as u8,
                    return_hour: arg2 as u8,
                },
                "random" => crate::NPCTravelSchedule::Random {
                    chance_per_hour: arg1 as i32,
                },
                "permanent" => crate::NPCTravelSchedule::Permanent,
                _ => return false,
            };

            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&mobile_uuid) {
                mobile.transport_route = Some(crate::TransportRoute {
                    transport_id: transport_uuid,
                    home_stop_index: home_stop as usize,
                    destination_stop_index: dest_stop as usize,
                    schedule,
                    is_at_destination: false,
                    is_on_transport: false,
                });
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
            false
        },
    );

    // clear_mobile_transport_route(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_mobile_transport_route", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.transport_route = None;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_mobile_transport_route(mobile_id) -> Map or ()
    // Returns: { transport_id, home_stop, dest_stop, schedule_type, depart_hour, return_hour, chance, is_at_destination, is_on_transport }
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_transport_route",
        move |mobile_id: String| -> rhai::Dynamic {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if let Some(route) = mobile.transport_route {
                        let mut map = rhai::Map::new();
                        map.insert("transport_id".into(), route.transport_id.to_string().into());
                        map.insert("home_stop".into(), (route.home_stop_index as i64).into());
                        map.insert("dest_stop".into(), (route.destination_stop_index as i64).into());
                        map.insert("is_at_destination".into(), route.is_at_destination.into());
                        map.insert("is_on_transport".into(), route.is_on_transport.into());

                        match route.schedule {
                            crate::NPCTravelSchedule::FixedHours {
                                depart_hour,
                                return_hour,
                            } => {
                                map.insert("schedule_type".into(), "fixed".into());
                                map.insert("depart_hour".into(), (depart_hour as i64).into());
                                map.insert("return_hour".into(), (return_hour as i64).into());
                            }
                            crate::NPCTravelSchedule::Random { chance_per_hour } => {
                                map.insert("schedule_type".into(), "random".into());
                                map.insert("chance".into(), (chance_per_hour as i64).into());
                            }
                            crate::NPCTravelSchedule::Permanent => {
                                map.insert("schedule_type".into(), "permanent".into());
                            }
                        }
                        return rhai::Dynamic::from(map);
                    }
                }
            }
            rhai::Dynamic::UNIT
        },
    );

    // has_mobile_transport_route(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_mobile_transport_route", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.transport_route.is_some();
            }
        }
        false
    });

    // set_mobile_transport_state(mobile_id, is_at_destination, is_on_transport) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_transport_state",
        move |mobile_id: String, is_at_dest: bool, is_on_transport: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if let Some(ref mut route) = mobile.transport_route {
                        route.is_at_destination = is_at_dest;
                        route.is_on_transport = is_on_transport;
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
            }
            false
        },
    );

    // ========== Pursuit System ==========

    // start_mob_pursuit(mobile_id, target_name, target_room_id, direction, certain) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "start_mob_pursuit",
        move |mobile_id: String, target_name: String, target_room_id: String, direction: String, certain: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&target_room_id) {
                    if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                        mobile.pursuit_target_name = target_name;
                        mobile.pursuit_target_room = Some(room_uuid);
                        mobile.pursuit_direction = direction;
                        mobile.pursuit_certain = certain;
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
            }
            false
        },
    );

    // clear_mob_pursuit(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_mob_pursuit", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.pursuit_target_name = String::new();
                mobile.pursuit_target_room = None;
                mobile.pursuit_direction = String::new();
                mobile.pursuit_certain = false;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // is_mob_pursuing(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_mob_pursuing", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.pursuit_target_room.is_some();
            }
        }
        false
    });

    // ========== Daily Routine System ==========

    // get_mobile_activity(mobile_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_activity", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.current_activity.to_display_string();
            }
        }
        "working".to_string()
    });

    // set_mobile_activity(mobile_id, activity_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_activity", move |mobile_id: String, activity_str: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.current_activity = ActivityState::from_str(&activity_str);
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_mobile_schedule_visible(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_schedule_visible", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.schedule_visible;
            }
        }
        false
    });

    // set_mobile_schedule_visible(mobile_id, visible) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_schedule_visible",
        move |mobile_id: String, visible: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.schedule_visible = visible;
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // get_mobile_routine_entry_count(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_routine_entry_count", move |mobile_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.daily_routine.len() as i64;
            }
        }
        0
    });

    // get_mobile_routine_entries(mobile_id) -> Array of Rhai maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_routine_entries",
        move |mobile_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    return mobile
                        .daily_routine
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("start_hour".into(), rhai::Dynamic::from(entry.start_hour as i64));
                            map.insert(
                                "activity".into(),
                                rhai::Dynamic::from(entry.activity.to_display_string()),
                            );
                            map.insert(
                                "destination_vnum".into(),
                                rhai::Dynamic::from(entry.destination_vnum.clone().unwrap_or_default()),
                            );
                            map.insert(
                                "transition_message".into(),
                                rhai::Dynamic::from(entry.transition_message.clone().unwrap_or_default()),
                            );
                            map.insert("suppress_wander".into(), rhai::Dynamic::from(entry.suppress_wander));
                            // Include dialogue overrides as a nested map
                            let mut dialogue_map = rhai::Map::new();
                            for (k, v) in &entry.dialogue_overrides {
                                dialogue_map.insert(k.clone().into(), rhai::Dynamic::from(v.clone()));
                            }
                            map.insert("dialogue_overrides".into(), rhai::Dynamic::from(dialogue_map));
                            rhai::Dynamic::from(map)
                        })
                        .collect();
                }
            }
            Vec::new()
        },
    );

    // add_mobile_routine_entry(mobile_id, start_hour, activity_str, dest_vnum, message, suppress_wander) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_mobile_routine_entry",
        move |mobile_id: String,
              start_hour: i64,
              activity_str: String,
              dest_vnum: String,
              message: String,
              suppress_wander: bool| {
            if start_hour < 0 || start_hour > 23 {
                return false;
            }
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    // Remove existing entry at same hour
                    mobile.daily_routine.retain(|e| e.start_hour != start_hour as u8);
                    mobile.daily_routine.push(RoutineEntry {
                        start_hour: start_hour as u8,
                        activity: ActivityState::from_str(&activity_str),
                        destination_vnum: if dest_vnum.is_empty() { None } else { Some(dest_vnum) },
                        transition_message: if message.is_empty() { None } else { Some(message) },
                        suppress_wander,
                        dialogue_overrides: std::collections::HashMap::new(),
                    });
                    // Sort by start_hour for consistent ordering
                    mobile.daily_routine.sort_by_key(|e| e.start_hour);
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // remove_mobile_routine_entry(mobile_id, start_hour) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_mobile_routine_entry",
        move |mobile_id: String, start_hour: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    let original_len = mobile.daily_routine.len();
                    mobile.daily_routine.retain(|e| e.start_hour != start_hour as u8);
                    if mobile.daily_routine.len() < original_len {
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                }
            }
            false
        },
    );

    // clear_mobile_routine(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_mobile_routine", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.daily_routine.clear();
                mobile.current_activity = ActivityState::default();
                mobile.routine_destination_room = None;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_routine_entry_message(mobile_id, start_hour, message) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_routine_entry_message",
        move |mobile_id: String, start_hour: i64, message: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    for entry in &mut mobile.daily_routine {
                        if entry.start_hour == start_hour as u8 {
                            entry.transition_message = if message.is_empty() { None } else { Some(message) };
                            return cloned_db.save_mobile_data(mobile).is_ok();
                        }
                    }
                }
            }
            false
        },
    );

    // set_routine_entry_wander(mobile_id, start_hour, suppress) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_routine_entry_wander",
        move |mobile_id: String, start_hour: i64, suppress: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    for entry in &mut mobile.daily_routine {
                        if entry.start_hour == start_hour as u8 {
                            entry.suppress_wander = suppress;
                            return cloned_db.save_mobile_data(mobile).is_ok();
                        }
                    }
                }
            }
            false
        },
    );

    // get_routine_dialogue(mobile_id, keyword) -> String
    // Checks the active entry's dialogue_overrides for the current game hour
    let cloned_db = db.clone();
    engine.register_fn("get_routine_dialogue", move |mobile_id: String, keyword: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if !mobile.daily_routine.is_empty() {
                    if let Ok(game_time) = cloned_db.get_game_time() {
                        if let Some(entry) = find_active_entry(&mobile.daily_routine, game_time.hour) {
                            let kw_lower = keyword.to_lowercase();
                            if let Some(response) = entry.dialogue_overrides.get(&kw_lower) {
                                return response.clone();
                            }
                        }
                    }
                }
            }
        }
        String::new()
    });

    // set_routine_entry_dialogue(mobile_id, start_hour, keyword, response) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_routine_entry_dialogue",
        move |mobile_id: String, start_hour: i64, keyword: String, response: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    for entry in &mut mobile.daily_routine {
                        if entry.start_hour == start_hour as u8 {
                            entry.dialogue_overrides.insert(keyword.to_lowercase(), response);
                            return cloned_db.save_mobile_data(mobile).is_ok();
                        }
                    }
                }
            }
            false
        },
    );

    // remove_routine_entry_dialogue(mobile_id, start_hour, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_routine_entry_dialogue",
        move |mobile_id: String, start_hour: i64, keyword: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    for entry in &mut mobile.daily_routine {
                        if entry.start_hour == start_hour as u8 {
                            if entry.dialogue_overrides.remove(&keyword.to_lowercase()).is_some() {
                                return cloned_db.save_mobile_data(mobile).is_ok();
                            }
                            return false;
                        }
                    }
                }
            }
            false
        },
    );

    // ========== Routine Presets ==========

    // list_routine_presets() -> Array of maps with name and description
    engine.register_fn("list_routine_presets", || -> Vec<rhai::Dynamic> {
        let data = match std::fs::read_to_string("scripts/data/routine_presets.json") {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        let presets: Vec<serde_json::Value> = match serde_json::from_str(&data) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        presets
            .iter()
            .map(|p| {
                let mut map = rhai::Map::new();
                map.insert(
                    "name".into(),
                    rhai::Dynamic::from(p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()),
                );
                map.insert(
                    "description".into(),
                    rhai::Dynamic::from(p.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string()),
                );
                rhai::Dynamic::from(map)
            })
            .collect()
    });

    // apply_routine_preset(mobile_id, preset_name, mappings_array) -> bool
    // mappings_array is an array of "key=vnum" strings, e.g. ["shop=market_square", "home=blacksmith_home"]
    let cloned_db = db.clone();
    engine.register_fn(
        "apply_routine_preset",
        move |mobile_id: String, preset_name: String, mappings: rhai::Array| -> bool {
            let uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            let mut mobile = match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };

            // Parse mappings: "key=vnum" -> HashMap
            let mut vnum_map: HashMap<String, String> = HashMap::new();
            for m in mappings {
                if let Some(s) = m.try_cast::<String>() {
                    if let Some((key, vnum)) = s.split_once('=') {
                        vnum_map.insert(key.to_string(), vnum.to_string());
                    }
                }
            }

            // Load presets from JSON
            let data = match std::fs::read_to_string("scripts/data/routine_presets.json") {
                Ok(d) => d,
                Err(_) => return false,
            };
            let presets: Vec<serde_json::Value> = match serde_json::from_str(&data) {
                Ok(p) => p,
                Err(_) => return false,
            };

            // Find the preset by name
            let preset = match presets
                .iter()
                .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(&preset_name))
            {
                Some(p) => p,
                None => return false,
            };

            let entries = match preset.get("entries").and_then(|v| v.as_array()) {
                Some(e) => e,
                None => return false,
            };

            // Clear existing routine
            mobile.daily_routine.clear();

            for entry_val in entries {
                let start_hour = entry_val.get("start_hour").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                let activity_str = entry_val.get("activity").and_then(|v| v.as_str()).unwrap_or("working");
                let dest_key = entry_val.get("destination_key").and_then(|v| v.as_str()).unwrap_or("");
                let msg_template = entry_val
                    .get("transition_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let suppress = entry_val
                    .get("suppress_wander")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Resolve destination_key to vnum via mappings
                let dest_vnum = if !dest_key.is_empty() {
                    vnum_map.get(dest_key).cloned()
                } else {
                    None
                };

                // Replace {name} in transition message
                let message = if !msg_template.is_empty() {
                    Some(msg_template.replace("{name}", &mobile.name))
                } else {
                    None
                };

                mobile.daily_routine.push(RoutineEntry {
                    start_hour,
                    activity: ActivityState::from_str(activity_str),
                    destination_vnum: dest_vnum,
                    transition_message: message,
                    suppress_wander: suppress,
                    dialogue_overrides: HashMap::new(),
                });
            }

            // Sort by start_hour
            mobile.daily_routine.sort_by_key(|e| e.start_hour);

            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );
}
