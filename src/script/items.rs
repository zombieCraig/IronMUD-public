// src/script/items.rs
// Item system functions including containers, liquids, food, and prototypes

use crate::db::Db;
use crate::{
    BodyPart, DamageType, EffectType, ItemAffect, ItemData, ItemEffect, ItemFlags, ItemLocation, ItemType, LiquidType,
    OnHitEffect, WeaponSkill, WearLocation,
};
use rhai::{Dynamic, Engine, Map};
use std::sync::Arc;

/// Parse N.keyword syntax (e.g., "2.guard" -> (2, "guard"), "sword" -> (1, "sword"))
/// Only triggers when prefix before first `.` is a positive integer.
pub(crate) fn parse_nth_keyword(input: &str) -> (usize, &str) {
    if let Some(dot_pos) = input.find('.') {
        let prefix = &input[..dot_pos];
        if let Ok(n) = prefix.parse::<usize>() {
            if n >= 1 {
                return (n, &input[dot_pos + 1..]);
            }
        }
    }
    (1, input)
}

/// Does the character own a non-prototype item with this vnum, either in
/// their inventory or equipped to any slot? Used by `evaluate_entry_gate`.
pub fn character_has_item_vnum(db: &Db, char_name: &str, vnum: &str) -> bool {
    if vnum.is_empty() {
        return false;
    }
    if let Ok(inv) = db.get_items_in_inventory(char_name) {
        for item in &inv {
            if !item.is_prototype && item.vnum.as_deref() == Some(vnum) {
                return true;
            }
        }
    }
    if let Ok(eq) = db.get_equipped_items(char_name) {
        for item in &eq {
            if !item.is_prototype && item.vnum.as_deref() == Some(vnum) {
                return true;
            }
        }
    }
    false
}

/// Check if an item matches a keyword by name or keywords list.
pub(crate) fn item_matches_keyword(name: &str, keywords: &[String], kw_lower: &str) -> bool {
    if name.to_lowercase().contains(kw_lower) {
        return true;
    }
    for item_kw in keywords {
        let item_kw_lower = item_kw.to_lowercase();
        if item_kw_lower == kw_lower || item_kw_lower.contains(kw_lower) {
            return true;
        }
    }
    false
}

/// Register item-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Item System Functions ==========

    // Register ItemFlags type with getters/setters
    engine.register_type_with_name::<ItemFlags>("ItemFlags");
    register_bool_flags!(
        engine,
        ItemFlags,
        no_drop,
        no_get,
        no_remove,
        invisible,
        glow,
        hum,
        magical,
        holy,
        no_sell,
        no_donate,
        unique,
        quest_item,
        vending,
        provides_light,
        night_vision,
        fishing_rod,
        bait,
        foraging_tool,
        waterproof,
        provides_warmth,
        reduces_glare,
        medical_tool,
        is_corpse,
        corpse_is_player,
        preserves_contents,
        death_only,
        atm,
        broken,
        plant_pot,
        lockpick,
        is_skinned,
        boat,
        buried,
        can_dig,
        detect_buried,
        anti_good,
        anti_evil,
        anti_neutral,
        cursed
    );

    // Non-bool ItemFlags fields exposed to Rhai. Option<String> serializes as
    // "" when unset so cast.rhai can do `if vnum == "" { ... }` without type
    // gymnastics.
    engine
        .register_get("corpse_owner", |f: &mut ItemFlags| f.corpse_owner.clone())
        .register_get("corpse_source_vnum", |f: &mut ItemFlags| {
            f.corpse_source_vnum.clone().unwrap_or_default()
        });

    // Register ItemData type
    engine.register_type_with_name::<ItemData>("ItemData");

    // String getter+setter pairs
    register_string!(engine, ItemData, name, short_desc, long_desc);

    // Bool getter+setter pairs
    register_bool_flags!(engine, ItemData, board_read_admin_only, board_write_admin_only);

    // Read-only bool getters (computed/plain bool fields exposed without setters)
    register_bool_ro!(engine, ItemData, two_handed, container_closed, container_locked,
        liquid_poisoned, food_poisoned, is_prototype);

    // i32 fields exposed as i64 with setters
    register_i32!(engine, ItemData, weight, value);

    // Read-only i32 fields exposed as i64
    register_i32_ro!(engine, ItemData,
        damage_dice_count, damage_dice_sides,
        container_max_items, container_max_weight,
        liquid_current, liquid_max, food_nutrition,
        level_requirement,
        stat_str, stat_dex, stat_con, stat_int, stat_wis, stat_cha,
        hit_bonus, damage_bonus, max_hp_bonus, max_mana_bonus,
        light_hours_remaining, insulation, vending_sell_rate,
        bait_uses, quality, weight_reduction,
        medical_tier, medical_uses, preservation_level,
        ammo_count, ammo_damage_bonus, magazine_size, loaded_ammo, loaded_ammo_bonus,
        ammo_effect_duration, ammo_effect_damage,
        loaded_ammo_effect_duration, loaded_ammo_effect_damage,
        attachment_accuracy_bonus, attachment_noise_reduction, attachment_magazine_bonus);

    // Read-only String getters (clone)
    register_string_ro!(engine, ItemData,
        fire_mode, noise_level, ammo_effect_type, loaded_ammo_effect_type,
        attachment_slot, max_treatable_wound, treats_infestation, plant_prototype_vnum);

    // Specials: id (uuid), Option<T> handlers, computed booleans, struct/array views.
    register_uuid_ro!(engine, ItemData, id);
    register_option_string!(engine, ItemData, note_content);
    register_option_string_ro!(
        engine,
        ItemData,
        vnum,
        container_key_vnum,
        caliber,
        ranged_type
    );
    register_string_vec_ro!(
        engine,
        ItemData,
        keywords,
        vending_stock,
        treats_wound_types,
        supported_fire_modes,
        attachment_compatible_types
    );

    engine
        .register_get("area_id", |i: &mut ItemData| {
            i.area_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("has_area_id", |i: &mut ItemData| i.area_id.is_some())
        .register_get("board_max_messages", |i: &mut ItemData| {
            i.board_max_messages.unwrap_or(0) as i64
        })
        .register_get("has_board_max_messages", |i: &mut ItemData| {
            i.board_max_messages.is_some()
        })
        .register_set("board_max_messages", |i: &mut ItemData, v: i64| {
            i.board_max_messages = if v > 0 { Some(v as i32) } else { None };
        })
        .register_get("extra_descs", |i: &mut ItemData| {
            i.extra_descs
                .iter()
                .map(|e| rhai::Dynamic::from(e.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("item_type", |i: &mut ItemData| {
            i.item_type.to_display_string().to_string()
        })
        .register_get("wear_locations", |i: &mut ItemData| {
            i.wear_locations
                .iter()
                .map(|w| rhai::Dynamic::from(w.to_display_string().to_string()))
                .collect::<Vec<_>>()
        })
        .register_get("armor_class", |i: &mut ItemData| i.armor_class.unwrap_or(0))
        .register_get("has_armor_class", |i: &mut ItemData| i.armor_class.is_some())
        .register_get("flags", |i: &mut ItemData| i.flags.clone())
        .register_get("world_max_count", |i: &mut ItemData| i.world_max_count.unwrap_or(0) as i64)
        .register_get("has_world_max_count", |i: &mut ItemData| i.world_max_count.is_some())
        .register_get("damage_type", |i: &mut ItemData| {
            i.damage_type.to_display_string().to_string()
        })
        .register_get("has_damage", |i: &mut ItemData| {
            i.damage_dice_count > 0 && i.damage_dice_sides > 0
        })
        .register_get("container_contents", |i: &mut ItemData| {
            i.container_contents
                .iter()
                .map(|u| rhai::Dynamic::from(u.to_string()))
                .collect::<Vec<_>>()
        })
        .register_get("is_container", |i: &mut ItemData| i.item_type == ItemType::Container)
        .register_get("is_key", |i: &mut ItemData| i.item_type == ItemType::Key)
        .register_get("liquid_type", |i: &mut ItemData| {
            i.liquid_type.to_display_string().to_string()
        })
        .register_get("liquid_effects", |i: &mut ItemData| {
            i.liquid_effects
                .iter()
                .map(|e| rhai::Dynamic::from(e.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("is_liquid_container", |i: &mut ItemData| {
            i.item_type == ItemType::LiquidContainer
        })
        .register_get("is_empty", |i: &mut ItemData| match i.item_type {
            ItemType::Container => i.container_contents.is_empty(),
            ItemType::LiquidContainer => i.liquid_current <= 0,
            _ => true,
        })
        // Food properties
        .register_get("food_spoil_duration", |i: &mut ItemData| i.food_spoil_duration)
        .register_get("food_created_at", |i: &mut ItemData| i.food_created_at.unwrap_or(0))
        .register_get("food_effects", |i: &mut ItemData| {
            i.food_effects
                .iter()
                .map(|e| rhai::Dynamic::from(e.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("is_food", |i: &mut ItemData| i.item_type == ItemType::Food)
        .register_get("is_spoiled", |i: &mut ItemData| {
            if i.item_type != ItemType::Food {
                return false;
            }
            // Tick-based spoilage check takes priority
            if i.food_spoilage_points >= 1.0 {
                return true;
            }
            // If no spoil duration set and no points accumulated, never spoils
            if i.food_spoil_duration == 0 {
                return false;
            }
            // Legacy fallback: if no spoilage points accumulated yet, use time-based check
            if i.food_spoilage_points == 0.0 {
                if let Some(created) = i.food_created_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    return now > created + i.food_spoil_duration;
                }
            }
            false
        })
        .register_get("food_spoilage_points", |i: &mut ItemData| i.food_spoilage_points)
        .register_get("freshness_label", |i: &mut ItemData| {
            if i.item_type != ItemType::Food {
                return "preserved".to_string();
            }
            // Check spoilage points first (authoritative for tick-based system)
            if i.food_spoilage_points >= 1.0 {
                return "spoiled".to_string();
            }
            if i.food_spoilage_points >= 0.75 {
                return "nearly spoiled".to_string();
            }
            if i.food_spoilage_points >= 0.50 {
                return "stale".to_string();
            }
            if i.food_spoilage_points >= 0.25 {
                return "slightly aged".to_string();
            }
            // No spoilage points accumulated - check if it even can spoil
            if i.food_spoil_duration == 0 && i.food_spoilage_points == 0.0 {
                return "preserved".to_string();
            }
            "fresh".to_string()
        })
        // CircleMUD POTION/WAND/STAFF on-use spell payload. Returns Rhai Map
        // `#{spell, min_level, charges, max_charges, cooldown_secs}` or `()`
        // when unset. `cooldown_secs` is `()` (UNIT) when no per-item override.
        .register_get("cast_on_use", |i: &mut ItemData| match &i.cast_on_use {
            Some(c) => {
                let mut m = rhai::Map::new();
                m.insert("spell".into(), c.spell.clone().into());
                m.insert("min_level".into(), (c.min_level as i64).into());
                m.insert("charges".into(), (c.charges as i64).into());
                m.insert("max_charges".into(), (c.max_charges as i64).into());
                m.insert(
                    "cooldown_secs".into(),
                    match c.cooldown_secs {
                        Some(n) => (n as i64).into(),
                        None => rhai::Dynamic::UNIT,
                    },
                );
                rhai::Dynamic::from_map(m)
            }
            None => rhai::Dynamic::UNIT,
        })
        .register_get("has_cast_on_use", |i: &mut ItemData| i.cast_on_use.is_some())
        .register_get("has_stats", |i: &mut ItemData| {
            i.stat_str != 0
                || i.stat_dex != 0
                || i.stat_con != 0
                || i.stat_int != 0
                || i.stat_wis != 0
                || i.stat_cha != 0
                || i.hit_bonus != 0
                || i.damage_bonus != 0
        })
        // Computed booleans backed by item_type or flag fields
        .register_get("is_vending", |i: &mut ItemData| i.flags.vending)
        .register_get("is_fishing_rod", |i: &mut ItemData| i.flags.fishing_rod)
        .register_get("is_bait", |i: &mut ItemData| i.flags.bait)
        .register_get("is_foraging_tool", |i: &mut ItemData| i.flags.foraging_tool)
        .register_get("is_medical_tool", |i: &mut ItemData| i.flags.medical_tool)
        .register_get("is_ammunition", |i: &mut ItemData| i.item_type == ItemType::Ammunition)
        .register_get("is_plant_pot", |i: &mut ItemData| i.flags.plant_pot)
        // Has-flags for the option_string fields above
        .register_get("has_caliber", |i: &mut ItemData| i.caliber.is_some())
        .register_get("has_ranged_type", |i: &mut ItemData| i.ranged_type.is_some())
        // Misc untyped fields kept inline (i64 already, no cast)
        .register_get("fertilizer_duration", |i: &mut ItemData| i.fertilizer_duration);

    // Register ItemEffect type
    engine
        .register_type_with_name::<ItemEffect>("ItemEffect")
        .register_get("effect_type", |e: &mut ItemEffect| {
            e.effect_type.to_display_string().to_string()
        })
        .register_get("magnitude", |e: &mut ItemEffect| e.magnitude as i64)
        .register_get("duration", |e: &mut ItemEffect| e.duration as i64);
    register_option_string_ro!(engine, ItemEffect, script_callback);

    // new_effect(effect_type, magnitude, duration) -> ItemEffect
    engine.register_fn(
        "new_effect",
        |effect_type_str: String, magnitude: i64, duration: i64| ItemEffect {
            effect_type: EffectType::from_str(&effect_type_str).unwrap_or_default(),
            magnitude: magnitude as i32,
            duration: duration as i32,
            script_callback: None,
        },
    );

    // get_all_effect_types() -> Array
    engine.register_fn("get_all_effect_types", || {
        EffectType::all()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });

    // ItemAffect type — equip-time effects stamped on the wearer as ActiveBuffs.
    engine
        .register_type_with_name::<ItemAffect>("ItemAffect")
        .register_get("effect_type", |a: &mut ItemAffect| {
            a.effect_type.to_display_string().to_string()
        })
        .register_get("magnitude", |a: &mut ItemAffect| a.magnitude as i64)
        .register_get("damage_type", |a: &mut ItemAffect| {
            a.damage_type.map(|d| d.to_display_string().to_string()).unwrap_or_default()
        })
        .register_get("has_damage_type", |a: &mut ItemAffect| a.damage_type.is_some())
        .register_get("vs_effect", |a: &mut ItemAffect| {
            a.vs_effect.clone().unwrap_or_default()
        })
        .register_get("has_vs_effect", |a: &mut ItemAffect| a.vs_effect.is_some())
        .register_get("skill_key", |a: &mut ItemAffect| {
            a.skill_key.clone().unwrap_or_default()
        })
        .register_get("has_skill_key", |a: &mut ItemAffect| a.skill_key.is_some());

    // get_item_affects(item_id) -> Array of ItemAffect
    let cloned_db = db.clone();
    engine.register_fn("get_item_affects", move |item_id: String| -> rhai::Array {
        let uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };
        match cloned_db.get_item_data(&uuid) {
            Ok(Some(item)) => item
                .affects
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect(),
            _ => Vec::new(),
        }
    });

    // item_add_affect(item_id, effect_str, magnitude, tag) -> String
    // Tag is interpreted by effect_str:
    //   "damage_resistance"   -> tag is the damage_type ("acid", "fire", ...).
    //   "status_resistance"   -> tag is the effect being warded ("sleep", "*", ...).
    //   "custom_skill_boost"  -> tag is the registered custom skill key
    //                            (must be published via `lookup skill publish`).
    //   anything else: tag must be "" (rejected otherwise).
    // Returns "" on success, or an error message describing the validation failure.
    let cloned_db = db.clone();
    engine.register_fn(
        "item_add_affect",
        move |item_id: String, effect_str: String, magnitude: i64, tag: String| -> String {
            let uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return "Invalid item id.".to_string(),
            };
            let effect = match EffectType::from_str(&effect_str) {
                Some(e) => e,
                None => return format!("Unknown effect type '{}'.", effect_str),
            };
            let (damage_type, vs_effect, skill_key) = match effect {
                EffectType::DamageResistance => {
                    if tag.is_empty() {
                        return "damage_resistance requires a damage type (e.g. acid, fire, cold).".to_string();
                    }
                    let dt = match DamageType::from_str(&tag) {
                        Some(d) => d,
                        None => return format!("Unknown damage type '{}'.", tag),
                    };
                    (Some(dt), None, None)
                }
                EffectType::StatusResistance => {
                    if tag.is_empty() {
                        return "status_resistance requires an effect to ward (e.g. sleep, charmed, poison, or '*' for all).".to_string();
                    }
                    if tag != "*" && EffectType::from_str(&tag).is_none() {
                        return format!("Unknown effect '{}'. Use a snake_case effect name or '*' for all.", tag);
                    }
                    (None, Some(tag), None)
                }
                EffectType::CustomSkillBoost => {
                    if tag.is_empty() {
                        return "custom_skill_boost requires a published skill key (use `lookup skill publish <key>` first).".to_string();
                    }
                    let key_lc = tag.to_lowercase();
                    match cloned_db.get_custom_skill(&key_lc) {
                        Ok(Some(_)) => {}
                        _ => {
                            return format!(
                                "Custom skill '{}' is not published. Use `lookup skill publish {} <description>` first.",
                                key_lc, key_lc
                            );
                        }
                    }
                    (None, None, Some(key_lc))
                }
                _ => {
                    if !tag.is_empty() {
                        return format!("Effect '{}' does not take a tag.", effect_str);
                    }
                    (None, None, None)
                }
            };
            let mut item = match cloned_db.get_item_data(&uuid) {
                Ok(Some(i)) => i,
                _ => return "Item not found.".to_string(),
            };
            item.affects.push(ItemAffect {
                effect_type: effect,
                magnitude: magnitude as i32,
                damage_type,
                vs_effect,
                skill_key,
            });
            if cloned_db.save_item_data(item).is_err() {
                return "Failed to save item.".to_string();
            }
            String::new()
        },
    );

    // item_remove_affect(item_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "item_remove_affect",
        move |item_id: String, index: i64| -> bool {
            let uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut item = match cloned_db.get_item_data(&uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= item.affects.len() {
                return false;
            }
            item.affects.remove(idx);
            cloned_db.save_item_data(item).is_ok()
        },
    );

    // item_clear_affects(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("item_clear_affects", move |item_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut item = match cloned_db.get_item_data(&uuid) {
            Ok(Some(i)) => i,
            _ => return false,
        };
        item.affects.clear();
        cloned_db.save_item_data(item).is_ok()
    });

    // format_item_affect(affect) -> String (display form, e.g. "strength_boost +2")
    engine.register_fn("format_item_affect", |a: ItemAffect| -> String {
        let mag_str = if a.magnitude >= 0 {
            format!("+{}", a.magnitude)
        } else {
            a.magnitude.to_string()
        };
        let base = a.effect_type.to_display_string();
        if let Some(sk) = a.skill_key.as_deref() {
            return format!("{} {} {}", base, sk, mag_str);
        }
        match (a.damage_type, a.vs_effect.as_deref()) {
            (Some(dt), _) => format!("{} {} {}%", base, dt.to_display_string(), mag_str),
            (_, Some(vs)) => format!("{} vs {} {}%", base, vs, mag_str),
            _ => format!("{} {}", base, mag_str),
        }
    });

    // get_all_liquid_types() -> Array
    engine.register_fn("get_all_liquid_types", || {
        LiquidType::all()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });

    // get_current_time() -> i64 (Unix timestamp)
    engine.register_fn("get_current_time", || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    });

    // new_item(name, short_desc, long_desc) -> ItemData
    engine.register_fn("new_item", |name: String, short_desc: String, long_desc: String| {
        ItemData::new(name, short_desc, long_desc)
    });

    // get_item_data(item_id) -> ItemData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_data", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => rhai::Dynamic::from(item),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // save_item_data(item) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_item_data", move |item: ItemData| {
        cloned_db.save_item_data(item).is_ok()
    });

    // delete_item(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_item", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            cloned_db.delete_item(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // list_all_items() -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("list_all_items", move || {
        cloned_db
            .list_all_items()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // get_items_in_room(room_id) -> Array of ItemData (excludes buried items)
    let cloned_db = db.clone();
    engine.register_fn("get_items_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_items_in_room(&uuid)
                .unwrap_or_default()
                .into_iter()
                .filter(|i| !i.flags.buried)
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // get_buried_items_in_room(room_id) -> Array of ItemData (only buried items)
    let cloned_db = db.clone();
    engine.register_fn("get_buried_items_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_items_in_room(&uuid)
                .unwrap_or_default()
                .into_iter()
                .filter(|i| i.flags.buried)
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // get_items_in_inventory(char_name) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_items_in_inventory", move |char_name: String| {
        cloned_db
            .get_items_in_inventory(&char_name)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // get_equipped_items(char_name) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_equipped_items", move |char_name: String| {
        cloned_db
            .get_equipped_items(&char_name)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // move_item_to_room(item_id, room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("move_item_to_room", move |item_id: String, room_id: String| {
        let item_uuid = uuid::Uuid::parse_str(&item_id).ok();
        let room_uuid = uuid::Uuid::parse_str(&room_id).ok();
        match (item_uuid, room_uuid) {
            (Some(iid), Some(rid)) => cloned_db.move_item_to_room(&iid, &rid).unwrap_or(false),
            _ => false,
        }
    });

    // move_item_to_inventory(item_id, char_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("move_item_to_inventory", move |item_id: String, char_name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            cloned_db.move_item_to_inventory(&uuid, &char_name).unwrap_or(false)
        } else {
            false
        }
    });

    // move_item_to_equipped(item_id, char_name) -> bool
    // Auto-picks the first free slot in `item.wear_locations`.
    let cloned_db = db.clone();
    engine.register_fn("move_item_to_equipped", move |item_id: String, char_name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            cloned_db.move_item_to_equipped(&uuid, &char_name).unwrap_or(false)
        } else {
            false
        }
    });

    // move_item_to_equipped_at(item_id, char_name, slot) -> bool
    // Explicit-slot variant for callers (zone resets, admin force-equip)
    // that already know which slot they want. Empty `slot` clears the
    // worn-slot marker but still moves the item to Equipped state.
    let cloned_db = db.clone();
    engine.register_fn(
        "move_item_to_equipped_at",
        move |item_id: String, char_name: String, slot: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                let parsed_slot = if slot.is_empty() {
                    None
                } else {
                    crate::types::WearLocation::from_str(&slot)
                };
                cloned_db
                    .move_item_to_equipped_at(&uuid, &char_name, parsed_slot)
                    .unwrap_or(false)
            } else {
                false
            }
        },
    );

    // pick_wear_slot(char_name, item_id) -> String
    // Returns the slot name auto-picked for `item_id` on `char_name`
    // (empty if no free slot). Powers wear.rhai's choose-and-display
    // flow; the actual equip uses the same logic via the auto-pick path
    // of move_item_to_equipped.
    let cloned_db = db.clone();
    engine.register_fn(
        "pick_wear_slot",
        move |char_name: String, item_id: String| -> String {
            let uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            let item = match cloned_db.get_item_data(&uuid) {
                Ok(Some(i)) => i,
                _ => return String::new(),
            };
            cloned_db
                .pick_free_wear_slot(&char_name, &item)
                .unwrap_or(None)
                .map(|slot| slot.to_display_string().to_string())
                .unwrap_or_default()
        },
    );

    // move_item_to_nowhere(item_id) -> bool
    // Removes item from any inventory/location (useful for selling, destroying, etc.)
    let cloned_db = db.clone();
    engine.register_fn("move_item_to_nowhere", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            cloned_db.move_item_to_nowhere(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // ========== Mobile Inventory/Equipment Functions ==========

    // move_item_to_mobile_inventory(item_id, mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "move_item_to_mobile_inventory",
        move |item_id: String, mobile_id: String| match (
            uuid::Uuid::parse_str(&item_id),
            uuid::Uuid::parse_str(&mobile_id),
        ) {
            (Ok(iid), Ok(mid)) => cloned_db.move_item_to_mobile_inventory(&iid, &mid).unwrap_or(false),
            _ => false,
        },
    );

    // move_item_to_mobile_equipped(item_id, mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "move_item_to_mobile_equipped",
        move |item_id: String, mobile_id: String| match (
            uuid::Uuid::parse_str(&item_id),
            uuid::Uuid::parse_str(&mobile_id),
        ) {
            (Ok(iid), Ok(mid)) => cloned_db.move_item_to_mobile_equipped(&iid, &mid).unwrap_or(false),
            _ => false,
        },
    );

    // get_items_in_mobile_inventory(mobile_id) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_items_in_mobile_inventory", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            cloned_db
                .get_items_in_mobile_inventory(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    });

    // get_items_equipped_on_mobile(mobile_id) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_items_equipped_on_mobile", move |mobile_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            cloned_db
                .get_items_equipped_on_mobile(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    });

    // search_items(keyword) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("search_items", move |keyword: String| {
        cloned_db
            .search_items(&keyword)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // set_item_keywords(item_id, keywords_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_keywords", move |item_id: String, keywords: rhai::Array| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.keywords = keywords
                    .into_iter()
                    .filter_map(|d| d.try_cast::<String>())
                    .map(|s| s.to_lowercase())
                    .collect();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_wear_locations(item_id, locations_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_wear_locations",
        move |item_id: String, locations: rhai::Array| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.wear_locations = locations
                        .into_iter()
                        .filter_map(|d| d.try_cast::<String>())
                        .filter_map(|s| WearLocation::from_str(&s))
                        .collect();
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_item_type(item_id, type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_type", move |item_id: String, type_str: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if let Some(item_type) = ItemType::from_str(&type_str) {
                    item.item_type = item_type;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
        }
        false
    });

    // set_item_armor_class(item_id, ac) -> bool (use negative to clear; positive values clamped)
    let cloned_db = db.clone();
    engine.register_fn("set_item_armor_class", move |item_id: String, ac: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let bound = crate::api::validate::STAT_BONUS_ABS_MAX as i64;
                item.armor_class = if ac < 0 { None } else { Some(ac.min(bound) as i32) };
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // ========== Armor Protection Functions ==========

    // get_item_protects(item_id) -> Array<String>
    // Returns the body parts this armor protects
    let cloned_db = db.clone();
    engine.register_fn("get_item_protects", move |item_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item
                    .protects
                    .iter()
                    .map(|p| rhai::Dynamic::from(p.to_display_string().to_string()))
                    .collect();
            }
        }
        vec![]
    });

    // set_item_protects(item_id, parts_array) -> bool
    // Sets the body parts this armor protects (replaces all)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_protects",
        move |item_id: String, parts: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.protects = parts
                        .into_iter()
                        .filter_map(|d| d.try_cast::<String>())
                        .filter_map(|s| BodyPart::from_str(&s))
                        .collect();
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // add_item_protects(item_id, body_part) -> bool
    // Adds a body part to the armor's protection (if not already present)
    let cloned_db = db.clone();
    engine.register_fn("add_item_protects", move |item_id: String, body_part: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if let Some(bp) = BodyPart::from_str(&body_part) {
                    if !item.protects.contains(&bp) {
                        item.protects.push(bp);
                        return cloned_db.save_item_data(item).is_ok();
                    }
                    return true; // Already has it
                }
            }
        }
        false
    });

    // remove_item_protects(item_id, body_part) -> bool
    // Removes a body part from the armor's protection
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_item_protects",
        move |item_id: String, body_part: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if let Some(bp) = BodyPart::from_str(&body_part) {
                        let original_len = item.protects.len();
                        item.protects.retain(|p| *p != bp);
                        if item.protects.len() != original_len {
                            return cloned_db.save_item_data(item).is_ok();
                        }
                    }
                }
            }
            false
        },
    );

    // clear_item_protects(item_id) -> bool
    // Clears all body part protection from the armor
    let cloned_db = db.clone();
    engine.register_fn("clear_item_protects", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.protects.clear();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_damage(item_id, dice_count, dice_sides) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_damage",
        move |item_id: String, dice_count: i64, dice_sides: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.damage_dice_count = dice_count as i32;
                    item.damage_dice_sides = dice_sides as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_item_damage_type(item_id, damage_type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_damage_type",
        move |item_id: String, damage_type_str: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if let Some(dt) = DamageType::from_str(&damage_type_str) {
                        item.damage_type = dt;
                        return cloned_db.save_item_data(item).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_item_two_handed(item_id, two_handed) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_two_handed", move |item_id: String, two_handed: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.two_handed = two_handed;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_on_hit_effects(item_id) -> Array<Map>
    let cloned_db = db.clone();
    engine.register_fn(
        "get_item_on_hit_effects",
        move |item_id: String| -> rhai::Array {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                    return item.on_hit_effects.iter().map(on_hit_effect_to_map).collect();
                }
            }
            Vec::new()
        },
    );

    // set_item_on_hit_effects(item_id, effects_array) -> bool
    // Each entry is a Map with {effect, chance, magnitude, duration}.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_on_hit_effects",
        move |item_id: String, effects: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.on_hit_effects = parse_on_hit_array(effects);
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_all_damage_types() -> Array of strings
    engine.register_fn("get_all_damage_types", || {
        DamageType::all()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });

    // ========== Weapon Skill Functions ==========

    // get_all_weapon_skills() -> Array of strings
    engine.register_fn("get_all_weapon_skills", || -> rhai::Array {
        WeaponSkill::all()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_skill_key().to_string()))
            .collect()
    });

    // get_item_weapon_skill(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_weapon_skill", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if let Some(skill) = item.weapon_skill {
                    return rhai::Dynamic::from(skill.to_skill_key().to_string());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // set_item_weapon_skill(item_id, skill_str) -> bool
    // Use empty string or "none" to clear
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_weapon_skill",
        move |item_id: String, skill_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if skill_str.is_empty() || skill_str.to_lowercase() == "none" {
                        item.weapon_skill = None;
                    } else if let Some(skill) = WeaponSkill::from_str(&skill_str) {
                        item.weapon_skill = Some(skill);
                    } else {
                        return false; // Invalid skill name
                    }
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_equipped_weapon_skill(char_name) -> String or ()
    // Returns the weapon skill of the wielded weapon (if any)
    let cloned_db = db.clone();
    engine.register_fn("get_equipped_weapon_skill", move |char_name: String| -> rhai::Dynamic {
        let equipped = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
        for item in equipped {
            // Check if it's in the wielded slot
            if item
                .wear_locations
                .iter()
                .any(|loc| matches!(loc, WearLocation::Wielded))
            {
                if let Some(skill) = item.weapon_skill {
                    return rhai::Dynamic::from(skill.to_display_string().to_string());
                }
            }
        }
        // No weapon equipped or weapon has no skill - default to unarmed
        rhai::Dynamic::from("unarmed".to_string())
    });

    // ========== Ammunition Functions ==========

    // get_item_caliber(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_caliber", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if let Some(ref cal) = item.caliber {
                    return rhai::Dynamic::from(cal.clone());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // set_item_caliber(item_id, caliber) -> bool (empty/"none" to clear)
    let cloned_db = db.clone();
    engine.register_fn("set_item_caliber", move |item_id: String, caliber: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if caliber.is_empty() || caliber.to_lowercase() == "none" {
                    item.caliber = None;
                } else {
                    item.caliber = Some(caliber.to_lowercase());
                }
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_ammo_count(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_ammo_count", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.ammo_count as i64;
            }
        }
        0
    });

    // set_item_ammo_count(item_id, count) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_ammo_count", move |item_id: String, count: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.ammo_count = count as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_ammo_damage_bonus(item_id, bonus) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_ammo_damage_bonus",
        move |item_id: String, bonus: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.ammo_damage_bonus = bonus as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // consume_ammo(item_id, amount) -> bool - reduces ammo_count, deletes item when it reaches 0
    let cloned_db = db.clone();
    engine.register_fn("consume_ammo", move |item_id: String, amount: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.ammo_count -= amount as i32;
                if item.ammo_count <= 0 {
                    // Delete the item when ammo is exhausted
                    return cloned_db.delete_item(&uuid).is_ok();
                }
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // ========== Crossbow/Firearm Functions ==========

    // get_item_ranged_type(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_ranged_type", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if let Some(ref rt) = item.ranged_type {
                    return rhai::Dynamic::from(rt.clone());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // set_item_ranged_type(item_id, type) -> bool (empty/"none" to clear)
    let cloned_db = db.clone();
    engine.register_fn("set_item_ranged_type", move |item_id: String, rtype: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if rtype.is_empty() || rtype.to_lowercase() == "none" {
                    item.ranged_type = None;
                } else {
                    let lower = rtype.to_lowercase();
                    if lower == "bow" || lower == "crossbow" || lower == "firearm" {
                        item.ranged_type = Some(lower);
                    } else {
                        return false;
                    }
                }
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_magazine_size(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_magazine_size", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.magazine_size as i64;
            }
        }
        0
    });

    // set_item_magazine_size(item_id, size) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_magazine_size", move |item_id: String, size: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.magazine_size = size as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_loaded_ammo(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.loaded_ammo as i64;
            }
        }
        0
    });

    // set_item_loaded_ammo(item_id, count) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_loaded_ammo", move |item_id: String, count: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.loaded_ammo = count as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_loaded_ammo_bonus(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo_bonus", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.loaded_ammo_bonus as i64;
            }
        }
        0
    });

    // set_item_loaded_ammo_bonus(item_id, bonus) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_loaded_ammo_bonus",
        move |item_id: String, bonus: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.loaded_ammo_bonus = bonus as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_loaded_ammo_vnum(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo_vnum", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if let Some(ref vnum) = item.loaded_ammo_vnum {
                    return rhai::Dynamic::from(vnum.clone());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // set_item_loaded_ammo_vnum(item_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_loaded_ammo_vnum",
        move |item_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.loaded_ammo_vnum = if vnum.is_empty() { None } else { Some(vnum) };
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_fire_mode(item_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_item_fire_mode", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.fire_mode;
            }
        }
        String::new()
    });

    // set_item_fire_mode(item_id, mode) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_fire_mode", move |item_id: String, mode: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.fire_mode = mode.to_lowercase();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_item_supported_fire_modes(item_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn(
        "get_item_supported_fire_modes",
        move |item_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                    return item
                        .supported_fire_modes
                        .iter()
                        .map(|s| rhai::Dynamic::from(s.clone()))
                        .collect();
                }
            }
            vec![]
        },
    );

    // set_item_supported_fire_modes(item_id, modes_str) -> bool (comma or space separated)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_supported_fire_modes",
        move |item_id: String, modes_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    let modes: Vec<String> = modes_str
                        .split(|c: char| c == ',' || c == ' ')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty() && (s == "single" || s == "burst" || s == "auto"))
                        .collect();
                    item.supported_fire_modes = modes;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_noise_level(item_id) -> String (effective noise: explicit or default from ranged_type)
    let cloned_db = db.clone();
    engine.register_fn("get_item_noise_level", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if !item.noise_level.is_empty() {
                    return item.noise_level.clone();
                }
                // Default based on ranged_type
                return match item.ranged_type.as_deref() {
                    Some("bow") => "silent".to_string(),
                    Some("crossbow") => "quiet".to_string(),
                    Some("firearm") => "loud".to_string(),
                    _ => "normal".to_string(),
                };
            }
        }
        "normal".to_string()
    });

    // set_item_noise_level(item_id, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_noise_level", move |item_id: String, level: String| -> bool {
        let valid = ["silent", "quiet", "normal", "loud", ""];
        let l = level.to_lowercase();
        if !valid.contains(&l.as_str()) {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.noise_level = if l == "clear" || l.is_empty() { String::new() } else { l };
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // === Special Ammunition Effect Functions ===

    // get_item_ammo_effect_type(item_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_item_ammo_effect_type", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.ammo_effect_type.clone();
            }
        }
        String::new()
    });

    // set_item_ammo_effect_type(item_id, effect_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_ammo_effect_type",
        move |item_id: String, effect_type: String| -> bool {
            let valid = ["fire", "cold", "poison", "acid", ""];
            let et = effect_type.to_lowercase();
            if !valid.contains(&et.as_str()) {
                return false;
            }
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.ammo_effect_type = et;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_ammo_effect_duration(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_ammo_effect_duration", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.ammo_effect_duration as i64;
            }
        }
        0
    });

    // set_item_ammo_effect_duration(item_id, duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_ammo_effect_duration",
        move |item_id: String, duration: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.ammo_effect_duration = duration as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_ammo_effect_damage(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_ammo_effect_damage", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.ammo_effect_damage as i64;
            }
        }
        0
    });

    // set_item_ammo_effect_damage(item_id, damage) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_ammo_effect_damage",
        move |item_id: String, damage: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.ammo_effect_damage = damage as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_loaded_ammo_effect_type(item_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo_effect_type", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.loaded_ammo_effect_type.clone();
            }
        }
        String::new()
    });

    // set_item_loaded_ammo_effect_type(item_id, effect_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_loaded_ammo_effect_type",
        move |item_id: String, effect_type: String| -> bool {
            let valid = ["fire", "cold", "poison", "acid", ""];
            let et = effect_type.to_lowercase();
            if !valid.contains(&et.as_str()) {
                return false;
            }
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.loaded_ammo_effect_type = et;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_loaded_ammo_effect_duration(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo_effect_duration", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.loaded_ammo_effect_duration as i64;
            }
        }
        0
    });

    // set_item_loaded_ammo_effect_duration(item_id, duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_loaded_ammo_effect_duration",
        move |item_id: String, duration: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.loaded_ammo_effect_duration = duration as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_loaded_ammo_effect_damage(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_loaded_ammo_effect_damage", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.loaded_ammo_effect_damage as i64;
            }
        }
        0
    });

    // set_item_loaded_ammo_effect_damage(item_id, damage) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_loaded_ammo_effect_damage",
        move |item_id: String, damage: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.loaded_ammo_effect_damage = damage as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // has_ammo_effect(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_ammo_effect", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return !item.ammo_effect_type.is_empty();
            }
        }
        false
    });

    // === Weapon Attachment Functions ===

    // get_item_attachment_slot(item_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_item_attachment_slot", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.attachment_slot.clone();
            }
        }
        String::new()
    });

    // set_item_attachment_slot(item_id, slot) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_attachment_slot",
        move |item_id: String, slot: String| -> bool {
            let valid = ["scope", "suppressor", "magazine", "accessory", ""];
            let s = slot.to_lowercase();
            if !valid.contains(&s.as_str()) {
                return false;
            }
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.attachment_slot = s;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_attachment_accuracy_bonus(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_attachment_accuracy_bonus", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.attachment_accuracy_bonus as i64;
            }
        }
        0
    });

    // set_item_attachment_accuracy_bonus(item_id, bonus) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_attachment_accuracy_bonus",
        move |item_id: String, bonus: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.attachment_accuracy_bonus = bonus as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_attachment_noise_reduction(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_attachment_noise_reduction", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.attachment_noise_reduction as i64;
            }
        }
        0
    });

    // set_item_attachment_noise_reduction(item_id, reduction) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_attachment_noise_reduction",
        move |item_id: String, reduction: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.attachment_noise_reduction = reduction as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_attachment_magazine_bonus(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_attachment_magazine_bonus", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.attachment_magazine_bonus as i64;
            }
        }
        0
    });

    // set_item_attachment_magazine_bonus(item_id, bonus) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_attachment_magazine_bonus",
        move |item_id: String, bonus: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.attachment_magazine_bonus = bonus as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_attachment_compatible_types(item_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn(
        "get_item_attachment_compatible_types",
        move |item_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                    return item
                        .attachment_compatible_types
                        .iter()
                        .map(|s| rhai::Dynamic::from(s.clone()))
                        .collect();
                }
            }
            Vec::new()
        },
    );

    // set_item_attachment_compatible_types(item_id, types_str) -> bool (comma or space separated)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_attachment_compatible_types",
        move |item_id: String, types_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    let valid_types = ["bow", "crossbow", "firearm"];
                    let types: Vec<String> = types_str
                        .split(|c: char| c == ',' || c == ' ')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty() && valid_types.contains(&s.as_str()))
                        .collect();
                    item.attachment_compatible_types = types;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_weapon_attachment_bonuses(weapon_id) -> Map { accuracy_bonus, noise_reduction, magazine_bonus }
    let cloned_db = db.clone();
    engine.register_fn("get_weapon_attachment_bonuses", move |weapon_id: String| -> rhai::Map {
        let mut result = rhai::Map::new();
        result.insert("accuracy_bonus".into(), rhai::Dynamic::from(0_i64));
        result.insert("noise_reduction".into(), rhai::Dynamic::from(0_i64));
        result.insert("magazine_bonus".into(), rhai::Dynamic::from(0_i64));

        let weapon_uuid = match uuid::Uuid::parse_str(&weapon_id) {
            Ok(u) => u,
            Err(_) => return result,
        };
        let weapon = match cloned_db.get_item_data(&weapon_uuid) {
            Ok(Some(w)) => w,
            _ => return result,
        };

        let mut accuracy_bonus: i64 = 0;
        let mut noise_reduction: i64 = 0;
        let mut magazine_bonus: i64 = 0;

        for content_id in &weapon.container_contents {
            if let Ok(Some(attachment)) = cloned_db.get_item_data(content_id) {
                if !attachment.attachment_slot.is_empty() {
                    accuracy_bonus += attachment.attachment_accuracy_bonus as i64;
                    noise_reduction += attachment.attachment_noise_reduction as i64;
                    magazine_bonus += attachment.attachment_magazine_bonus as i64;
                }
            }
        }

        result.insert("accuracy_bonus".into(), rhai::Dynamic::from(accuracy_bonus));
        result.insert("noise_reduction".into(), rhai::Dynamic::from(noise_reduction));
        result.insert("magazine_bonus".into(), rhai::Dynamic::from(magazine_bonus));
        result
    });

    // can_attach_to_weapon(attachment_id, weapon_id) -> Map { allowed, reason }
    let cloned_db = db.clone();
    engine.register_fn(
        "can_attach_to_weapon",
        move |attachment_id: String, weapon_id: String| -> rhai::Map {
            let mut result = rhai::Map::new();
            result.insert("allowed".into(), rhai::Dynamic::from(false));
            result.insert("reason".into(), rhai::Dynamic::from(String::new()));

            let att_uuid = match uuid::Uuid::parse_str(&attachment_id) {
                Ok(u) => u,
                Err(_) => {
                    result.insert(
                        "reason".into(),
                        rhai::Dynamic::from("Invalid attachment ID.".to_string()),
                    );
                    return result;
                }
            };
            let wpn_uuid = match uuid::Uuid::parse_str(&weapon_id) {
                Ok(u) => u,
                Err(_) => {
                    result.insert("reason".into(), rhai::Dynamic::from("Invalid weapon ID.".to_string()));
                    return result;
                }
            };

            let attachment = match cloned_db.get_item_data(&att_uuid) {
                Ok(Some(a)) => a,
                _ => {
                    result.insert(
                        "reason".into(),
                        rhai::Dynamic::from("Attachment not found.".to_string()),
                    );
                    return result;
                }
            };
            let weapon = match cloned_db.get_item_data(&wpn_uuid) {
                Ok(Some(w)) => w,
                _ => {
                    result.insert("reason".into(), rhai::Dynamic::from("Weapon not found.".to_string()));
                    return result;
                }
            };

            // Check attachment has a slot
            if attachment.attachment_slot.is_empty() {
                result.insert(
                    "reason".into(),
                    rhai::Dynamic::from("That item is not an attachment.".to_string()),
                );
                return result;
            }

            // Check weapon is ranged
            let ranged_type = match &weapon.ranged_type {
                Some(rt) => rt.clone(),
                None => {
                    result.insert(
                        "reason".into(),
                        rhai::Dynamic::from("That weapon is not a ranged weapon.".to_string()),
                    );
                    return result;
                }
            };

            // Check compatible types
            if !attachment.attachment_compatible_types.is_empty() {
                let rt_lower = ranged_type.to_lowercase();
                if !attachment
                    .attachment_compatible_types
                    .iter()
                    .any(|t| t.to_lowercase() == rt_lower)
                {
                    result.insert(
                        "reason".into(),
                        rhai::Dynamic::from(format!(
                            "That attachment is not compatible with {} weapons.",
                            ranged_type
                        )),
                    );
                    return result;
                }
            }

            // Check slot capacity
            if weapon.container_max_items > 0 && weapon.container_contents.len() as i32 >= weapon.container_max_items {
                result.insert(
                    "reason".into(),
                    rhai::Dynamic::from("That weapon has no free attachment slots.".to_string()),
                );
                return result;
            }

            // Check duplicate slot
            for content_id in &weapon.container_contents {
                if let Ok(Some(existing)) = cloned_db.get_item_data(content_id) {
                    if existing.attachment_slot == attachment.attachment_slot {
                        result.insert(
                            "reason".into(),
                            rhai::Dynamic::from(format!(
                                "That weapon already has a {} attached.",
                                attachment.attachment_slot
                            )),
                        );
                        return result;
                    }
                }
            }

            result.insert("allowed".into(), rhai::Dynamic::from(true));
            result
        },
    );

    // === Arrow Recovery Functions ===

    // embed_projectile(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("embed_projectile", move |mobile_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.embedded_projectiles.push(vnum);
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // get_embedded_projectiles(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn(
        "get_embedded_projectiles",
        move |mobile_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    return mobile
                        .embedded_projectiles
                        .iter()
                        .map(|s| rhai::Dynamic::from(s.clone()))
                        .collect();
                }
            }
            Vec::new()
        },
    );

    // clear_embedded_projectiles(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_embedded_projectiles", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.embedded_projectiles.clear();
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // process_arrow_recovery(mobile_id, corpse_id) -> Map { recovered, broken, lost }
    let cloned_db = db.clone();
    engine.register_fn(
        "process_arrow_recovery",
        move |mobile_id: String, corpse_id: String| -> rhai::Map {
            use rand::Rng;
            let mut result = rhai::Map::new();
            result.insert("recovered".into(), rhai::Dynamic::from(0_i64));
            result.insert("broken".into(), rhai::Dynamic::from(0_i64));
            result.insert("lost".into(), rhai::Dynamic::from(0_i64));

            let mob_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return result,
            };
            let corpse_uuid = match uuid::Uuid::parse_str(&corpse_id) {
                Ok(u) => u,
                Err(_) => return result,
            };

            let mobile = match cloned_db.get_mobile_data(&mob_uuid) {
                Ok(Some(m)) => m,
                _ => return result,
            };

            let bullet_calibers = ["9mm", "5.56mm", ".45", ".308", "12gauge"];
            let mut rng = rand::thread_rng();
            let mut recovered: i64 = 0;
            let mut broken_count: i64 = 0;
            let mut lost: i64 = 0;

            for vnum in &mobile.embedded_projectiles {
                let prototype = match cloned_db.get_item_by_vnum(vnum) {
                    Ok(Some(p)) => p,
                    _ => {
                        lost += 1;
                        continue;
                    }
                };

                // Skip bullets
                if let Some(ref cal) = prototype.caliber {
                    let cal_lower = cal.to_lowercase();
                    if bullet_calibers.iter().any(|b| cal_lower == *b) {
                        continue;
                    }
                }

                // Skip special ammo
                if !prototype.ammo_effect_type.is_empty() {
                    lost += 1;
                    continue;
                }

                // 50% chance recoverable
                if rng.gen_range(0..100) >= 50 {
                    lost += 1;
                    continue;
                }

                let mut spawned = match cloned_db.spawn_item_from_prototype(vnum) {
                    Ok(Some(item)) => item,
                    _ => {
                        lost += 1;
                        continue;
                    }
                };

                // 25% broken
                if rng.gen_range(0..100) < 25 {
                    spawned.flags.broken = true;
                    broken_count += 1;
                }

                spawned.location = ItemLocation::Container(corpse_uuid);
                spawned.ammo_count = 1;
                if cloned_db.save_item_data(spawned.clone()).is_ok() {
                    if let Ok(Some(mut corpse)) = cloned_db.get_item_data(&corpse_uuid) {
                        corpse.container_contents.push(spawned.id);
                        let _ = cloned_db.save_item_data(corpse);
                    }
                }
                recovered += 1;
            }

            result.insert("recovered".into(), rhai::Dynamic::from(recovered));
            result.insert("broken".into(), rhai::Dynamic::from(broken_count));
            result.insert("lost".into(), rhai::Dynamic::from(lost));
            result
        },
    );

    // perform_reload(weapon_id, char_name) -> Map{success, loaded, message}
    // Transfers ammo from ready slot (or inventory) into weapon's loaded_ammo
    let cloned_db = db.clone();
    engine.register_fn(
        "perform_reload",
        move |weapon_id: String, char_name: String| -> rhai::Map {
            let mut result = rhai::Map::new();
            result.insert("success".into(), rhai::Dynamic::from(false));
            result.insert("loaded".into(), rhai::Dynamic::from(0_i64));
            result.insert("message".into(), rhai::Dynamic::from(String::new()));

            let weapon_uuid = match uuid::Uuid::parse_str(&weapon_id) {
                Ok(u) => u,
                Err(_) => {
                    result.insert("message".into(), rhai::Dynamic::from("Invalid weapon ID.".to_string()));
                    return result;
                }
            };

            let weapon = match cloned_db.get_item_data(&weapon_uuid) {
                Ok(Some(w)) => w,
                _ => {
                    result.insert("message".into(), rhai::Dynamic::from("Weapon not found.".to_string()));
                    return result;
                }
            };

            let caliber = match &weapon.caliber {
                Some(c) => c.clone(),
                None => {
                    result.insert(
                        "message".into(),
                        rhai::Dynamic::from("This weapon has no caliber set.".to_string()),
                    );
                    return result;
                }
            };

            let space = weapon.magazine_size - weapon.loaded_ammo;
            if space <= 0 {
                result.insert(
                    "message".into(),
                    rhai::Dynamic::from("Your weapon is already fully loaded.".to_string()),
                );
                return result;
            }

            // Search for compatible ammo: ready slot first, then inventory
            let equipped = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
            let ready_items: Vec<ItemData> = equipped
                .into_iter()
                .filter(|i| i.wear_locations.iter().any(|loc| matches!(loc, WearLocation::Ready)))
                .collect();
            let inventory = cloned_db.get_items_in_inventory(&char_name).unwrap_or_default();

            // Combine: ready items first, then inventory
            let mut ammo_source: Option<ItemData> = None;
            for item in ready_items.iter().chain(inventory.iter()) {
                if item.item_type == ItemType::Ammunition {
                    if let Some(ref item_cal) = item.caliber {
                        if *item_cal.to_lowercase() == *caliber.to_lowercase()
                            && item.ammo_count > 0
                            && !item.flags.broken
                        {
                            ammo_source = Some(item.clone());
                            break;
                        }
                    }
                }
            }

            let ammo = match ammo_source {
                Some(a) => a,
                None => {
                    result.insert(
                        "message".into(),
                        rhai::Dynamic::from(format!("You have no {} ammunition to load.", caliber)),
                    );
                    return result;
                }
            };

            let to_load = space.min(ammo.ammo_count);
            let remaining = ammo.ammo_count - to_load;

            // Update weapon
            let mut weapon = match cloned_db.get_item_data(&weapon_uuid) {
                Ok(Some(w)) => w,
                _ => return result,
            };
            weapon.loaded_ammo += to_load;
            weapon.loaded_ammo_bonus = ammo.ammo_damage_bonus;
            weapon.loaded_ammo_vnum = ammo.vnum.clone();
            // Capture special ammo effect payload
            weapon.loaded_ammo_effect_type = ammo.ammo_effect_type.clone();
            weapon.loaded_ammo_effect_duration = ammo.ammo_effect_duration;
            weapon.loaded_ammo_effect_damage = ammo.ammo_effect_damage;
            let _ = cloned_db.save_item_data(weapon);

            // Consume from ammo stack
            if remaining <= 0 {
                let _ = cloned_db.delete_item(&ammo.id);
            } else {
                let mut ammo_item = match cloned_db.get_item_data(&ammo.id) {
                    Ok(Some(a)) => a,
                    _ => return result,
                };
                ammo_item.ammo_count = remaining;
                let _ = cloned_db.save_item_data(ammo_item);
            }

            result.insert("success".into(), rhai::Dynamic::from(true));
            result.insert("loaded".into(), rhai::Dynamic::from(to_load as i64));
            result.insert(
                "message".into(),
                rhai::Dynamic::from(format!(
                    "You load {} round{} of {} ammunition.",
                    to_load,
                    if to_load == 1 { "" } else { "s" },
                    caliber
                )),
            );
            result
        },
    );

    // perform_unload(weapon_id, char_name) -> Map{success, count, message}
    // Ejects loaded ammo from weapon into player inventory
    let cloned_db = db.clone();
    engine.register_fn(
        "perform_unload",
        move |weapon_id: String, char_name: String| -> rhai::Map {
            let mut result = rhai::Map::new();
            result.insert("success".into(), rhai::Dynamic::from(false));
            result.insert("count".into(), rhai::Dynamic::from(0_i64));
            result.insert("message".into(), rhai::Dynamic::from(String::new()));

            let weapon_uuid = match uuid::Uuid::parse_str(&weapon_id) {
                Ok(u) => u,
                Err(_) => {
                    result.insert("message".into(), rhai::Dynamic::from("Invalid weapon ID.".to_string()));
                    return result;
                }
            };

            let weapon = match cloned_db.get_item_data(&weapon_uuid) {
                Ok(Some(w)) => w,
                _ => {
                    result.insert("message".into(), rhai::Dynamic::from("Weapon not found.".to_string()));
                    return result;
                }
            };

            if weapon.loaded_ammo <= 0 {
                result.insert(
                    "message".into(),
                    rhai::Dynamic::from("Your weapon is not loaded.".to_string()),
                );
                return result;
            }

            let caliber = weapon.caliber.clone().unwrap_or_else(|| "unknown".to_string());
            let count = weapon.loaded_ammo;
            let bonus = weapon.loaded_ammo_bonus;

            // Create ammo item in inventory — prefer spawning from prototype if vnum is known
            let char_name_clone = char_name.clone();
            let ammo_item = if let Some(ref vnum) = weapon.loaded_ammo_vnum {
                match cloned_db.spawn_item_from_prototype(vnum) {
                    Ok(Some(mut spawned)) => {
                        spawned.ammo_count = count;
                        spawned.ammo_damage_bonus = bonus;
                        spawned.location = ItemLocation::Inventory(char_name_clone);
                        if cloned_db.save_item_data(spawned).is_err() {
                            result.insert(
                                "message".into(),
                                rhai::Dynamic::from("Failed to create ammo item.".to_string()),
                            );
                            return result;
                        }
                        true
                    }
                    _ => false, // fall through to generic creation
                }
            } else {
                false
            };

            if !ammo_item {
                // Fallback: create generic ammo (legacy data or missing prototype)
                let mut generic = ItemData::new(
                    format!("{} ammunition", caliber),
                    format!(
                        "{} round{} of {} ammunition",
                        count,
                        if count == 1 { "" } else { "s" },
                        caliber
                    ),
                    format!("A stack of {} ammunition.", caliber),
                );
                generic.item_type = ItemType::Ammunition;
                generic.caliber = Some(caliber.clone());
                generic.ammo_count = count;
                generic.ammo_damage_bonus = bonus;
                generic.keywords = vec![caliber.clone(), "ammunition".to_string(), "ammo".to_string()];
                generic.location = ItemLocation::Inventory(char_name);

                if cloned_db.save_item_data(generic).is_err() {
                    result.insert(
                        "message".into(),
                        rhai::Dynamic::from("Failed to create ammo item.".to_string()),
                    );
                    return result;
                }
            }

            // Clear weapon
            let mut weapon = match cloned_db.get_item_data(&weapon_uuid) {
                Ok(Some(w)) => w,
                _ => return result,
            };
            weapon.loaded_ammo = 0;
            weapon.loaded_ammo_bonus = 0;
            weapon.loaded_ammo_vnum = None;
            let _ = cloned_db.save_item_data(weapon);

            result.insert("success".into(), rhai::Dynamic::from(true));
            result.insert("count".into(), rhai::Dynamic::from(count as i64));
            result.insert(
                "message".into(),
                rhai::Dynamic::from(format!(
                    "You eject {} round{} of {} ammunition.",
                    count,
                    if count == 1 { "" } else { "s" },
                    caliber
                )),
            );
            result
        },
    );

    // ========== Container Functions ==========

    // get_items_in_container(container_id) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_items_in_container", move |container_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&container_id) {
            cloned_db
                .get_items_in_container(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // move_item_to_container(item_id, container_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "move_item_to_container",
        move |item_id: String, container_id: String| {
            let item_uuid = uuid::Uuid::parse_str(&item_id).ok();
            let container_uuid = uuid::Uuid::parse_str(&container_id).ok();
            match (item_uuid, container_uuid) {
                (Some(iid), Some(cid)) => cloned_db.move_item_to_container(&iid, &cid).unwrap_or(false),
                _ => false,
            }
        },
    );

    // remove_item_from_container(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_item_from_container", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            cloned_db.remove_item_from_container(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // set_container_capacity(item_id, max_items, max_weight) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_container_capacity",
        move |item_id: String, max_items: i64, max_weight: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.container_max_items = max_items as i32;
                    item.container_max_weight = max_weight as i32;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_container_closed(item_id, closed) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_container_closed", move |item_id: String, closed: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.container_closed = closed;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_container_locked(item_id, locked) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_container_locked", move |item_id: String, locked: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.container_locked = locked;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_buried(item_id, buried) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_buried", move |item_id: String, buried: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.flags.buried = buried;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // find_detector(char_name) -> ItemData or ()
    // Returns the first item in inventory or equipped that has flags.detect_buried.
    let cloned_db = db.clone();
    engine.register_fn("find_detector", move |char_name: String| {
        if let Ok(items) = cloned_db.get_items_in_inventory(&char_name) {
            if let Some(item) = items.into_iter().find(|i| i.flags.detect_buried) {
                return rhai::Dynamic::from(item);
            }
        }
        if let Ok(items) = cloned_db.get_equipped_items(&char_name) {
            if let Some(item) = items.into_iter().find(|i| i.flags.detect_buried) {
                return rhai::Dynamic::from(item);
            }
        }
        rhai::Dynamic::UNIT
    });

    // set_container_key_vnum(item_id, vnum) -> bool (empty / "clear" / "none" clears)
    let cloned_db = db.clone();
    engine.register_fn("set_container_key_vnum", move |item_id: String, key_vnum: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let trimmed = key_vnum.to_lowercase();
                item.container_key_vnum = if key_vnum.is_empty() || trimmed == "clear" || trimmed == "none" {
                    None
                } else {
                    Some(key_vnum)
                };
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // can_add_to_container(container_id, item_id) -> Map {allowed, reason}
    let cloned_db = db.clone();
    engine.register_fn("can_add_to_container", move |container_id: String, item_id: String| {
        let mut result = rhai::Map::new();
        result.insert("allowed".into(), rhai::Dynamic::from(false));
        result.insert("reason".into(), rhai::Dynamic::from(String::new()));

        let container_uuid = match uuid::Uuid::parse_str(&container_id) {
            Ok(u) => u,
            Err(_) => {
                result.insert("reason".into(), "Invalid container ID".into());
                return result;
            }
        };
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => {
                result.insert("reason".into(), "Invalid item ID".into());
                return result;
            }
        };

        let container = match cloned_db.get_item_data(&container_uuid) {
            Ok(Some(c)) if c.item_type == ItemType::Container => c,
            _ => {
                result.insert("reason".into(), "Not a container".into());
                return result;
            }
        };

        let item = match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(i)) => i,
            _ => {
                result.insert("reason".into(), "Item not found".into());
                return result;
            }
        };

        if container.container_closed {
            result.insert("reason".into(), "Container is closed".into());
            return result;
        }

        if container.container_max_items > 0
            && container.container_contents.len() >= container.container_max_items as usize
        {
            result.insert("reason".into(), "Container is full".into());
            return result;
        }

        if container.container_max_weight > 0 {
            let current_weight: i32 = container
                .container_contents
                .iter()
                .filter_map(|id| cloned_db.get_item_data(id).ok().flatten())
                .map(|i| i.weight)
                .sum();
            if current_weight + item.weight > container.container_max_weight {
                result.insert("reason".into(), "Too heavy for container".into());
                return result;
            }
        }

        result.insert("allowed".into(), rhai::Dynamic::from(true));
        result
    });

    // ========== Liquid Container Functions ==========

    // set_liquid(item_id, liquid_type_str, current, max) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_liquid",
        move |item_id: String, liquid_type_str: String, current: i64, max: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if let Some(lt) = LiquidType::from_str(&liquid_type_str) {
                        item.liquid_type = lt;
                        item.liquid_current = current as i32;
                        item.liquid_max = max as i32;
                        return cloned_db.save_item_data(item).is_ok();
                    }
                }
            }
            false
        },
    );

    // drink_from(item_id, sips) -> i32 (returns actual sips consumed)
    // If liquid_max == -1, the container is infinite (fountain, river, etc.)
    let cloned_db = db.clone();
    engine.register_fn("drink_from", move |item_id: String, sips: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                // Infinite source (e.g. fountain): always returns full request,
                // no decrement, no save.
                if item.liquid_max == -1 {
                    return sips;
                }
                let actual_sips = std::cmp::min(sips as i32, item.liquid_current);
                item.liquid_current -= actual_sips;
                if cloned_db.save_item_data(item).is_err() {
                    return 0_i64;
                }
                return actual_sips as i64;
            }
        }
        0_i64
    });

    // fill_liquid_container(container_id, source_id) -> Map {filled, message}
    let cloned_db = db.clone();
    engine.register_fn(
        "fill_liquid_container",
        move |container_id: String, source_id: String| {
            let mut result = rhai::Map::new();
            result.insert("filled".into(), rhai::Dynamic::from(false));
            result.insert("message".into(), rhai::Dynamic::from(String::new()));

            let container_uuid = uuid::Uuid::parse_str(&container_id).ok();
            let source_uuid = uuid::Uuid::parse_str(&source_id).ok();

            match (container_uuid, source_uuid) {
                (Some(cid), Some(sid)) => {
                    if let Ok(Some(mut container)) = cloned_db.get_item_data(&cid) {
                        if container.item_type != ItemType::LiquidContainer {
                            result.insert("message".into(), "That's not a liquid container".into());
                            return result;
                        }
                        if container.liquid_current >= container.liquid_max {
                            result.insert("message".into(), "It's already full".into());
                            return result;
                        }
                        if let Ok(Some(mut source)) = cloned_db.get_item_data(&sid) {
                            if source.item_type != ItemType::LiquidContainer {
                                result.insert("message".into(), "You can't fill from that".into());
                                return result;
                            }
                            let source_infinite = source.liquid_max == -1;
                            if !source_infinite && source.liquid_current <= 0 {
                                result.insert("message".into(), "The source is empty".into());
                                return result;
                            }
                            let space = container.liquid_max - container.liquid_current;
                            let transfer = if source_infinite {
                                space
                            } else {
                                std::cmp::min(space, source.liquid_current)
                            };
                            container.liquid_current += transfer;
                            container.liquid_type = source.liquid_type;
                            container.liquid_poisoned = source.liquid_poisoned;
                            container.liquid_effects = source.liquid_effects.clone();
                            if !source_infinite {
                                source.liquid_current -= transfer;
                                let _ = cloned_db.save_item_data(source);
                            }
                            let _ = cloned_db.save_item_data(container);
                            result.insert("filled".into(), rhai::Dynamic::from(true));
                        }
                    }
                }
                _ => {
                    result.insert("message".into(), "Invalid item".into());
                }
            }
            result
        },
    );

    // set_liquid_poisoned(item_id, poisoned) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_liquid_poisoned", move |item_id: String, poisoned: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.liquid_poisoned = poisoned;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // add_liquid_effect(item_id, effect_type, magnitude, duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_liquid_effect",
        move |item_id: String, effect_type_str: String, magnitude: i64, duration: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if let Some(et) = EffectType::from_str(&effect_type_str) {
                        item.liquid_effects.push(ItemEffect {
                            effect_type: et,
                            magnitude: magnitude as i32,
                            duration: duration as i32,
                            script_callback: None,
                        });
                        return cloned_db.save_item_data(item).is_ok();
                    }
                }
            }
            false
        },
    );

    // clear_liquid_effects(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_liquid_effects", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.liquid_effects.clear();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // ========== Food Functions ==========

    // set_food_properties(item_id, nutrition, spoil_duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_food_properties",
        move |item_id: String, nutrition: i64, spoil_duration: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.food_nutrition = nutrition as i32;
                    item.food_spoil_duration = spoil_duration;
                    if spoil_duration > 0 && item.food_created_at.is_none() {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        item.food_created_at = Some(now);
                    }
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_food_poisoned(item_id, poisoned) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_food_poisoned", move |item_id: String, poisoned: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.food_poisoned = poisoned;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_food_created_at(item_id, timestamp) -> bool (0 means now)
    let cloned_db = db.clone();
    engine.register_fn("set_food_created_at", move |item_id: String, timestamp: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let ts = if timestamp == 0 {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                } else {
                    timestamp
                };
                item.food_created_at = Some(ts);
                item.food_spoilage_points = 0.0; // Reset spoilage on fresh spawn
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_food_spoilage_points(item_id, points) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_food_spoilage_points", move |item_id: String, points: f64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.food_spoilage_points = points.clamp(0.0, 1.0);
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_preservation_level(item_id, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_preservation_level", move |item_id: String, level: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.preservation_level = (level as i32).clamp(0, 2);
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // add_food_effect(item_id, effect_type, magnitude, duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_food_effect",
        move |item_id: String, effect_type_str: String, magnitude: i64, duration: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if let Some(et) = EffectType::from_str(&effect_type_str) {
                        item.food_effects.push(ItemEffect {
                            effect_type: et,
                            magnitude: magnitude as i32,
                            duration: duration as i32,
                            script_callback: None,
                        });
                        return cloned_db.save_item_data(item).is_ok();
                    }
                }
            }
            false
        },
    );

    // clear_food_effects(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_food_effects", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.food_effects.clear();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // consume_food(item_id) -> bool (deletes the food item)
    let cloned_db = db.clone();
    engine.register_fn("consume_food", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            return cloned_db.delete_item(&uuid).unwrap_or(false);
        }
        false
    });

    // ========== Level Requirement and Stat Functions ==========

    // set_item_level_requirement(item_id, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_level_requirement", move |item_id: String, level: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.level_requirement = level as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_stat(item_id, stat_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_stat",
        move |item_id: String, stat_name: String, value: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    match stat_name.to_lowercase().as_str() {
                        "str" | "strength" => item.stat_str = value as i32,
                        "dex" | "dexterity" => item.stat_dex = value as i32,
                        "con" | "constitution" => item.stat_con = value as i32,
                        "int" | "intelligence" => item.stat_int = value as i32,
                        "wis" | "wisdom" => item.stat_wis = value as i32,
                        "cha" | "charisma" => item.stat_cha = value as i32,
                        _ => return false,
                    }
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_item_hit_bonus(item_id, value) -> bool. Clamped to ±STAT_BONUS_ABS_MAX for overflow safety.
    let cloned_db = db.clone();
    engine.register_fn("set_item_hit_bonus", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let bound = crate::api::validate::STAT_BONUS_ABS_MAX as i64;
                item.hit_bonus = value.clamp(-bound, bound) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_damage_bonus(item_id, value) -> bool. Clamped to ±STAT_BONUS_ABS_MAX.
    let cloned_db = db.clone();
    engine.register_fn("set_item_damage_bonus", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let bound = crate::api::validate::STAT_BONUS_ABS_MAX as i64;
                item.damage_bonus = value.clamp(-bound, bound) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_max_hp_bonus(item_id, value) -> bool — APPLY_MAXHIT parity. Clamped.
    let cloned_db = db.clone();
    engine.register_fn("set_item_max_hp_bonus", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let bound = crate::api::validate::STAT_BONUS_ABS_MAX as i64;
                item.max_hp_bonus = value.clamp(-bound, bound) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_max_mana_bonus(item_id, value) -> bool — APPLY_MAXMANA parity. Clamped.
    let cloned_db = db.clone();
    engine.register_fn("set_item_max_mana_bonus", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let bound = crate::api::validate::STAT_BONUS_ABS_MAX as i64;
                item.max_mana_bonus = value.clamp(-bound, bound) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_light_hours(item_id, value) -> bool — burn time in game hours; 0 = permanent
    let cloned_db = db.clone();
    engine.register_fn("set_item_light_hours", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.light_hours_remaining = (value as i32).max(0);
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_cast_on_use(item_id, spell, min_level, charges) -> bool
    // Sets `max_charges = charges`, no cooldown override. Empty spell rejects.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_cast_on_use",
        move |item_id: String, spell: String, min_level: i64, charges: i64| {
            set_item_cast_on_use_impl(&cloned_db, &item_id, spell, min_level, charges, -1)
        },
    );

    // set_item_cast_on_use(item_id, spell, min_level, charges, cooldown_secs) -> bool
    // 5-arg variant with per-item cooldown override. `cooldown_secs <= 0` clears
    // the override (use spell's default cooldown).
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_cast_on_use",
        move |item_id: String,
              spell: String,
              min_level: i64,
              charges: i64,
              cooldown_secs: i64| {
            set_item_cast_on_use_impl(
                &cloned_db,
                &item_id,
                spell,
                min_level,
                charges,
                cooldown_secs,
            )
        },
    );

    // clear_item_cast_on_use(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_item_cast_on_use", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.cast_on_use = None;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // decrement_item_charges(item_id) -> i64 — consumes one charge, returns
    // remaining (or -1 if item missing / no cast_on_use). Saves on success.
    let cloned_db = db.clone();
    engine.register_fn("decrement_item_charges", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if let Some(ref mut cou) = item.cast_on_use {
                    if cou.charges > 0 {
                        cou.charges -= 1;
                    }
                    let remaining = cou.charges as i64;
                    let _ = cloned_db.save_item_data(item);
                    return remaining;
                }
            }
        }
        -1
    });

    // ========== Prototype Functions ==========

    // set_item_prototype(item_id, is_prototype) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_prototype", move |item_id: String, is_prototype: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.is_prototype = is_prototype;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_vnum(item_id, vnum) -> bool (empty string clears vnum)
    let cloned_db = db.clone();
    engine.register_fn("set_item_vnum", move |item_id: String, vnum: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if vnum.is_empty() {
                    item.vnum = None;
                } else {
                    item.vnum = Some(vnum);
                }
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_transport_link(item_id, transport_id) -> bool (empty string clears link)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_transport_link",
        move |item_id: String, transport_id: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    if transport_id.is_empty() {
                        item.transport_link = None;
                    } else if let Ok(transport_uuid) = uuid::Uuid::parse_str(&transport_id) {
                        item.transport_link = Some(transport_uuid);
                    } else {
                        return false;
                    }
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_transport_link(item_id) -> String (transport ID or empty)
    let cloned_db = db.clone();
    engine.register_fn("get_item_transport_link", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.transport_link.map(|u| u.to_string()).unwrap_or_default();
            }
        }
        String::new()
    });

    // get_item_note_content(item_id) -> String ("" if unset)
    let cloned_db = db.clone();
    engine.register_fn("get_item_note_content", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.note_content.clone().unwrap_or_default();
            }
        }
        String::new()
    });

    // set_item_note_content(item_id, body) -> bool (empty body clears)
    let cloned_db = db.clone();
    engine.register_fn("set_item_note_content", move |item_id: String, body: String| -> bool {
        if body.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.note_content = if body.is_empty() { None } else { Some(body) };
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // mark_item_donated(item_id) -> bool — stamps `donated_at` to now
    // (Unix epoch). The donation-decay tick uses this as its gate.
    let cloned_db = db.clone();
    engine.register_fn("mark_item_donated", move |item_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
            item.donated_at = Some(now);
            return cloned_db.save_item_data(item).is_ok();
        }
        false
    });

    // clear_item_donation_timer(item_id) -> bool — no-op if already cleared.
    // Called from `get`/`take` so picked-up items stop decaying.
    let cloned_db = db.clone();
    engine.register_fn("clear_item_donation_timer", move |item_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
            if item.donated_at.is_none() {
                return true;
            }
            item.donated_at = None;
            return cloned_db.save_item_data(item).is_ok();
        }
        false
    });

    // get_item_extra_desc(item_id, keyword) -> Gets extra description by keyword
    let cloned_db = db.clone();
    engine.register_fn("get_item_extra_desc", move |item_id: String, keyword: String| -> String {
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        if let Ok(Some(item)) = cloned_db.get_item_data(&item_uuid) {
            let keyword_lower = keyword.to_lowercase();
            for extra in &item.extra_descs {
                for kw in &extra.keywords {
                    if kw.to_lowercase() == keyword_lower {
                        return extra.description.clone();
                    }
                }
            }
        }
        String::new()
    });

    // add_item_extra_desc(item_id, keywords, description) -> bool
    // keywords is a space-separated string of keywords
    let cloned_db = db.clone();
    engine.register_fn(
        "add_item_extra_desc",
        move |item_id: String, keywords: String, description: String| -> bool {
            if description.len() > crate::MAX_DESC_BYTES {
                return false;
            }
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&item_uuid) {
                let keyword_vec: Vec<String> =
                    keywords.split_whitespace().map(|s| s.to_string()).collect();
                if keyword_vec.is_empty() {
                    return false;
                }
                item.extra_descs.push(crate::ExtraDesc {
                    keywords: keyword_vec,
                    description,
                });
                if let Err(e) = cloned_db.save_item_data(item) {
                    tracing::error!("Failed to save item after adding extra desc: {}", e);
                    return false;
                }
                return true;
            }
            false
        },
    );

    // remove_item_extra_desc(item_id, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_item_extra_desc", move |item_id: String, keyword: String| -> bool {
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut item)) = cloned_db.get_item_data(&item_uuid) {
            let keyword_lower = keyword.to_lowercase();
            let original_len = item.extra_descs.len();
            item.extra_descs
                .retain(|extra| !extra.keywords.iter().any(|kw| kw.to_lowercase() == keyword_lower));
            if item.extra_descs.len() < original_len {
                if let Err(e) = cloned_db.save_item_data(item) {
                    tracing::error!("Failed to save item after removing extra desc: {}", e);
                    return false;
                }
                return true;
            }
        }
        false
    });

    // set_item_world_max_count(item_id, n) -> bool (n <= 0 clears the cap)
    let cloned_db = db.clone();
    engine.register_fn("set_item_world_max_count", move |item_id: String, n: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.world_max_count = if n <= 0 { None } else { Some(n as i32) };
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_area_id(item_id, area_id) -> bool. Empty area_id clears
    // the assignment back to orphan. Permission gating happens in OLC.
    let cloned_db = db.clone();
    engine.register_fn("set_item_area_id", move |item_id: String, area_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let new_area = if area_id.trim().is_empty() {
            None
        } else {
            match uuid::Uuid::parse_str(area_id.trim()) {
                Ok(a) => Some(a),
                Err(_) => return false,
            }
        };
        if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
            item.area_id = new_area;
            return cloned_db.save_item_data(item).is_ok();
        }
        false
    });

    // get_item_by_vnum(vnum) -> ItemData or () (finds prototype by vnum)
    let cloned_db = db.clone();
    engine.register_fn("get_item_by_vnum", move |vnum: String| -> rhai::Dynamic {
        if let Ok(items) = cloned_db.list_all_items() {
            for item in items {
                if item.is_prototype {
                    if let Some(ref item_vnum) = item.vnum {
                        if item_vnum == &vnum {
                            return rhai::Dynamic::from(item);
                        }
                    }
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // count_items_by_vnum(vnum) -> i64 (counts non-prototype items with vnum, for unique enforcement)
    let cloned_db = db.clone();
    engine.register_fn("count_items_by_vnum", move |vnum: String| -> i64 {
        match cloned_db.count_non_prototype_items_by_vnum(&vnum) {
            Ok(count) => count as i64,
            Err(_) => 0,
        }
    });

    // spawn_item_from_prototype(vnum) -> ItemData or () (creates new item from prototype)
    let cloned_db = db.clone();
    engine.register_fn("spawn_item_from_prototype", move |vnum: String| -> rhai::Dynamic {
        if let Ok(items) = cloned_db.list_all_items() {
            for item in items {
                if item.is_prototype {
                    if let Some(ref item_vnum) = item.vnum {
                        if item_vnum == &vnum {
                            // Clone the item and create a new one
                            let mut new_item = item.clone();
                            new_item.id = uuid::Uuid::new_v4();
                            new_item.is_prototype = false;
                            new_item.location = ItemLocation::Nowhere;
                            // Clear container contents for spawned items
                            new_item.container_contents = Vec::new();
                            if cloned_db.save_item_data(new_item.clone()).is_ok() {
                                return rhai::Dynamic::from(new_item);
                            }
                        }
                    }
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // spawn_item_to_inventory(vnum, char_name) -> ItemData or () (creates item and places in inventory)
    let cloned_db = db.clone();
    engine.register_fn(
        "spawn_item_to_inventory",
        move |vnum: String, char_name: String| -> rhai::Dynamic {
            if let Ok(items) = cloned_db.list_all_items() {
                for item in items {
                    if item.is_prototype {
                        if let Some(ref item_vnum) = item.vnum {
                            if item_vnum == &vnum {
                                // Clone the item and create a new one
                                let mut new_item = item.clone();
                                new_item.id = uuid::Uuid::new_v4();
                                new_item.is_prototype = false;
                                new_item.location = ItemLocation::Inventory(char_name.clone());
                                // Clear container contents for spawned items
                                new_item.container_contents = Vec::new();
                                if cloned_db.save_item_data(new_item.clone()).is_ok() {
                                    return rhai::Dynamic::from(new_item);
                                }
                            }
                        }
                    }
                }
            }
            rhai::Dynamic::UNIT
        },
    );

    // get_item_instances_by_vnum(vnum) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_item_instances_by_vnum", move |vnum: String| {
        cloned_db
            .get_item_instances_by_vnum(&vnum)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // refresh_item_from_prototype(item_id) -> ItemData or ()
    let cloned_db = db.clone();
    engine.register_fn("refresh_item_from_prototype", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.refresh_item_from_prototype(&uuid) {
                Ok(Some(item)) => rhai::Dynamic::from(item),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // ========== Gold Functions ==========

    // spawn_gold(amount) -> ItemData - Creates gold item at Nowhere
    let cloned_db = db.clone();
    engine.register_fn("spawn_gold", move |amount: i64| -> rhai::Dynamic {
        let gold = crate::create_gold_item(amount as i32);
        match cloned_db.save_item_data(gold.clone()) {
            Ok(_) => rhai::Dynamic::from(gold),
            Err(_) => rhai::Dynamic::UNIT,
        }
    });

    // spawn_gold_in_room(amount, room_id) -> ItemData or ()
    let cloned_db = db.clone();
    engine.register_fn(
        "spawn_gold_in_room",
        move |amount: i64, room_id: String| -> rhai::Dynamic {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return rhai::Dynamic::UNIT,
            };
            match cloned_db.spawn_gold_in_room(amount as i32, &room_uuid) {
                Ok(gold) => rhai::Dynamic::from(gold),
                Err(_) => rhai::Dynamic::UNIT,
            }
        },
    );

    // spawn_gold_in_container(amount, container_id) -> ItemData or ()
    let cloned_db = db.clone();
    engine.register_fn(
        "spawn_gold_in_container",
        move |amount: i64, container_id: String| -> rhai::Dynamic {
            let container_uuid = match uuid::Uuid::parse_str(&container_id) {
                Ok(u) => u,
                Err(_) => return rhai::Dynamic::UNIT,
            };
            match cloned_db.spawn_gold_in_container(amount as i32, &container_uuid) {
                Ok(Some(gold)) => rhai::Dynamic::from(gold),
                _ => rhai::Dynamic::UNIT,
            }
        },
    );

    // is_gold_item(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_gold_item", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.item_type == ItemType::Gold;
            }
        }
        false
    });

    // get_gold_description(amount) -> String
    engine.register_fn("get_gold_description", |amount: i64| -> String {
        crate::get_gold_tier_description(amount as i32).to_string()
    });

    // ========== Item Flag and Property Setters ==========

    // set_item_flag(item_id, flag_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_flag",
        move |item_id: String, flag_name: String, value: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    match flag_name.to_lowercase().as_str() {
                        "no_drop" | "nodrop" => item.flags.no_drop = value,
                        "no_get" | "noget" => item.flags.no_get = value,
                        "no_remove" | "noremove" => item.flags.no_remove = value,
                        "invisible" => item.flags.invisible = value,
                        "glow" => item.flags.glow = value,
                        "hum" => item.flags.hum = value,
                        "magical" | "magic" => item.flags.magical = value,
                        "holy" | "blessed" | "silver" => item.flags.holy = value,
                        "no_sell" | "nosell" => item.flags.no_sell = value,
                        "no_donate" | "nodonate" => item.flags.no_donate = value,
                        "unique" => item.flags.unique = value,
                        "quest_item" | "questitem" | "quest" => item.flags.quest_item = value,
                        "vending" => item.flags.vending = value,
                        "provides_light" | "provideslight" | "light" => item.flags.provides_light = value,
                        "night_vision" | "nightvision" | "infravision" => item.flags.night_vision = value,
                        "fishing_rod" | "fishingrod" | "rod" => item.flags.fishing_rod = value,
                        "bait" => item.flags.bait = value,
                        "foraging_tool" | "foragingtool" | "forage" => item.flags.foraging_tool = value,
                        "waterproof" => item.flags.waterproof = value,
                        "provides_warmth" | "provideswarmth" | "warmth" => item.flags.provides_warmth = value,
                        "reduces_glare" | "reducesglare" | "glare" => item.flags.reduces_glare = value,
                        "medical_tool" | "medicaltool" | "medical" => item.flags.medical_tool = value,
                        "preserves_contents" | "preservescontents" | "preserves" => {
                            item.flags.preserves_contents = value
                        }
                        "death_only" | "deathonly" => item.flags.death_only = value,
                        "atm" => item.flags.atm = value,
                        "lockpick" => item.flags.lockpick = value,
                        "is_skinned" | "skinned" => item.flags.is_skinned = value,
                        "boat" => item.flags.boat = value,
                        "buried" => item.flags.buried = value,
                        "can_dig" | "candig" => item.flags.can_dig = value,
                        "detect_buried" | "detectburied" => item.flags.detect_buried = value,
                        "anti_good" | "antigood" => item.flags.anti_good = value,
                        "anti_evil" | "antievil" => item.flags.anti_evil = value,
                        "anti_neutral" | "antineutral" => item.flags.anti_neutral = value,
                        "cursed" | "curse" => item.flags.cursed = value,
                        _ => return false,
                    }
                    item.sync_flag_categories();
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // get_item_flag(item_id, flag_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("get_item_flag", move |item_id: String, flag_name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return match flag_name.to_lowercase().as_str() {
                    "no_drop" | "nodrop" => item.flags.no_drop,
                    "no_get" | "noget" => item.flags.no_get,
                    "no_remove" | "noremove" => item.flags.no_remove,
                    "invisible" => item.flags.invisible,
                    "glow" => item.flags.glow,
                    "hum" => item.flags.hum,
                    "magical" | "magic" => item.flags.magical,
                    "holy" | "blessed" | "silver" => item.flags.holy,
                    "no_sell" | "nosell" => item.flags.no_sell,
                    "no_donate" | "nodonate" => item.flags.no_donate,
                    "unique" => item.flags.unique,
                    "quest_item" | "questitem" | "quest" => item.flags.quest_item,
                    "vending" => item.flags.vending,
                    "provides_light" | "provideslight" | "light" => item.flags.provides_light,
                    "night_vision" | "nightvision" | "infravision" => item.flags.night_vision,
                    "fishing_rod" | "fishingrod" | "rod" => item.flags.fishing_rod,
                    "bait" => item.flags.bait,
                    "foraging_tool" | "foragingtool" | "forage" => item.flags.foraging_tool,
                    "waterproof" => item.flags.waterproof,
                    "provides_warmth" | "provideswarmth" | "warmth" => item.flags.provides_warmth,
                    "reduces_glare" | "reducesglare" | "glare" => item.flags.reduces_glare,
                    "medical_tool" | "medicaltool" | "medical" => item.flags.medical_tool,
                    "preserves_contents" | "preservescontents" | "preserves" => item.flags.preserves_contents,
                    "death_only" | "deathonly" => item.flags.death_only,
                    "atm" => item.flags.atm,
                    "lockpick" => item.flags.lockpick,
                    "is_skinned" | "skinned" => item.flags.is_skinned,
                    "boat" => item.flags.boat,
                    "buried" => item.flags.buried,
                    "can_dig" | "candig" => item.flags.can_dig,
                    "detect_buried" | "detectburied" => item.flags.detect_buried,
                    "anti_good" | "antigood" => item.flags.anti_good,
                    "anti_evil" | "antievil" => item.flags.anti_evil,
                    "anti_neutral" | "antineutral" => item.flags.anti_neutral,
                    "cursed" | "curse" => item.flags.cursed,
                    _ => false,
                };
            }
        }
        false
    });

    // room_has_atm(room_id) -> bool (checks if any item in room has atm flag)
    let cloned_db = db.clone();
    engine.register_fn("room_has_atm", move |room_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(items) = cloned_db.get_items_in_room(&uuid) {
                return items.iter().any(|item| item.flags.atm);
            }
        }
        false
    });

    // player_has_item_flag(char_name, flag_name) -> bool
    // Checks if any item in player's inventory or equipment has the given flag
    let cloned_db = db.clone();
    engine.register_fn(
        "player_has_item_flag",
        move |char_name: String, flag_name: String| -> bool {
            let check_flag = |item: &ItemData| -> bool {
                match flag_name.to_lowercase().as_str() {
                    "boat" => item.flags.boat,
                    "provides_light" | "light" => item.flags.provides_light,
                    "waterproof" => item.flags.waterproof,
                    "provides_warmth" | "warmth" => item.flags.provides_warmth,
                    "fishing_rod" | "rod" => item.flags.fishing_rod,
                    "lockpick" => item.flags.lockpick,
                    _ => false,
                }
            };
            if let Ok(items) = cloned_db.get_items_in_inventory(&char_name) {
                if items.iter().any(check_flag) {
                    return true;
                }
            }
            if let Ok(items) = cloned_db.get_equipped_items(&char_name) {
                if items.iter().any(check_flag) {
                    return true;
                }
            }
            false
        },
    );

    // set_item_name(item_id, name) -> bool. Capped at NAME_MAX bytes.
    let cloned_db = db.clone();
    engine.register_fn("set_item_name", move |item_id: String, name: String| {
        if name.len() > crate::api::validate::NAME_MAX {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.name = name;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_short_desc(item_id, desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_short_desc", move |item_id: String, desc: String| {
        if desc.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.short_desc = desc;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_long_desc(item_id, desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_long_desc", move |item_id: String, desc: String| {
        if desc.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.long_desc = desc;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_weight(item_id, weight) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_weight", move |item_id: String, weight: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.weight = weight as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_item_value(item_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_value", move |item_id: String, value: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.value = value as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // can_wear_item(char_name, item_id) -> Map with "can_wear" bool and "conflicts" array
    let cloned_db = db.clone();
    engine.register_fn("can_wear_item", move |char_name: String, item_id: String| {
        let mut result = rhai::Map::new();
        result.insert("can_wear".into(), rhai::Dynamic::from(false));
        result.insert("conflicts".into(), rhai::Dynamic::from(Vec::<rhai::Dynamic>::new()));

        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return result,
        };

        let item = match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(i)) => i,
            _ => return result,
        };

        if item.wear_locations.is_empty() {
            return result; // Not wearable
        }

        let equipped = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
        let mut conflicts: Vec<rhai::Dynamic> = Vec::new();

        for eq_item in equipped {
            for item_loc in &item.wear_locations {
                if eq_item.wear_locations.contains(item_loc) {
                    conflicts.push(rhai::Dynamic::from(eq_item.name.clone()));
                    break; // Only add each conflicting item once
                }
            }
        }

        let can_wear = conflicts.is_empty();
        result.insert("can_wear".into(), rhai::Dynamic::from(can_wear));
        result.insert("conflicts".into(), rhai::Dynamic::from(conflicts));
        result
    });

    // find_item_by_keyword(keyword, items_array) -> ItemData or ()
    // Helper for targeting items by keyword from a list
    // Supports N.keyword syntax (e.g., "2.sword" returns the 2nd matching sword)
    engine.register_fn("find_item_by_keyword", |keyword: String, items: rhai::Array| {
        let (nth, actual_keyword) = parse_nth_keyword(&keyword);
        let kw_lower = actual_keyword.to_lowercase();
        let mut match_count: usize = 0;
        for item_dyn in items {
            if let Some(item) = item_dyn.clone().try_cast::<ItemData>() {
                if item_matches_keyword(&item.name, &item.keywords, &kw_lower) {
                    match_count += 1;
                    if match_count == nth {
                        return rhai::Dynamic::from(item);
                    }
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // find_items_by_keyword(keyword, items_array) -> Array of ItemData
    // Returns ALL matching items (for "all" and "all.keyword" support)
    engine.register_fn("find_items_by_keyword", |keyword: String, items: rhai::Array| {
        let kw_lower = keyword.to_lowercase();
        let mut results: rhai::Array = rhai::Array::new();
        for item_dyn in items {
            if let Some(item) = item_dyn.clone().try_cast::<ItemData>() {
                let mut matched = false;
                // Check name
                if item.name.to_lowercase().contains(&kw_lower) {
                    matched = true;
                }
                // Check keywords
                if !matched {
                    for item_kw in &item.keywords {
                        if item_kw.to_lowercase() == kw_lower || item_kw.to_lowercase().contains(&kw_lower) {
                            matched = true;
                            break;
                        }
                    }
                }
                if matched {
                    results.push(rhai::Dynamic::from(item));
                }
            }
        }
        results
    });

    // parse_all_syntax(args) -> #{ is_all: bool, keyword: String }
    // Parses "all" or "all.keyword" syntax
    // Returns is_all=true if "all" prefix detected, keyword="" for plain "all" or the filter word
    engine.register_fn("parse_all_syntax", |args: String| {
        let mut result = rhai::Map::new();
        let args_lower = args.to_lowercase();

        if args_lower == "all" {
            result.insert("is_all".into(), rhai::Dynamic::from(true));
            result.insert("keyword".into(), rhai::Dynamic::from("".to_string()));
        } else if args_lower.starts_with("all.") {
            result.insert("is_all".into(), rhai::Dynamic::from(true));
            result.insert("keyword".into(), rhai::Dynamic::from(args[4..].to_string()));
        } else {
            result.insert("is_all".into(), rhai::Dynamic::from(false));
            result.insert("keyword".into(), rhai::Dynamic::from(args));
        }

        result
    });

    // find_item_in_inventory(char_name, keyword) -> ItemData or ()
    // Convenience function to find item in a character's inventory
    // Supports N.keyword syntax (e.g., "2.potion" returns the 2nd matching potion)
    let db_clone = db.clone();
    engine.register_fn("find_item_in_inventory", move |char_name: String, keyword: String| {
        let (nth, actual_keyword) = parse_nth_keyword(&keyword);
        let kw_lower = actual_keyword.to_lowercase();
        let items = match db_clone.get_items_in_inventory(&char_name) {
            Ok(items) => items,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let mut match_count: usize = 0;
        for item in items {
            if item.is_prototype {
                continue;
            }
            if item_matches_keyword(&item.name, &item.keywords, &kw_lower) {
                match_count += 1;
                if match_count == nth {
                    return rhai::Dynamic::from(item);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // find_inventory_item_by_vnum(char_name, vnum) -> ItemData or ()
    // Returns the first non-prototype inventory item whose prototype vnum matches.
    let db_clone = db.clone();
    engine.register_fn("find_inventory_item_by_vnum", move |char_name: String, vnum: String| {
        if vnum.is_empty() {
            return rhai::Dynamic::UNIT;
        }
        let items = match db_clone.get_items_in_inventory(&char_name) {
            Ok(items) => items,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        for item in items {
            if item.is_prototype {
                continue;
            }
            if item.vnum.as_deref() == Some(vnum.as_str()) {
                return rhai::Dynamic::from(item);
            }
        }
        rhai::Dynamic::UNIT
    });

    // find_item_in_room(room_id, keyword) -> ItemData or ()
    // Convenience function to find item in a room
    // Supports N.keyword syntax (e.g., "2.corpse" returns the 2nd matching corpse)
    let db_clone = db.clone();
    engine.register_fn("find_item_in_room", move |room_id: String, keyword: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(id) => id,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let (nth, actual_keyword) = parse_nth_keyword(&keyword);
        let kw_lower = actual_keyword.to_lowercase();
        let items = match db_clone.get_items_in_room(&room_uuid) {
            Ok(items) => items,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let mut match_count: usize = 0;
        for item in items {
            if item.is_prototype || item.flags.buried {
                continue;
            }
            if item_matches_keyword(&item.name, &item.keywords, &kw_lower) {
                match_count += 1;
                if match_count == nth {
                    return rhai::Dynamic::from(item);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // get_all_wear_locations() -> Array of location strings
    engine.register_fn("get_all_wear_locations", || {
        WearLocation::all()
            .into_iter()
            .map(|w| rhai::Dynamic::from(w.to_display_string().to_string()))
            .collect::<Vec<_>>()
    });

    // ========== Weight and Encumbrance Functions ==========

    // set_item_weight_reduction(item_id, percent) -> bool
    // Set weight reduction percentage for a container (0-100)
    // When worn, container contents weigh (100 - percent)% of normal
    let cloned_db = db.clone();
    engine.register_fn("set_item_weight_reduction", move |item_id: String, percent: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.weight_reduction = percent.clamp(0, 100) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // get_character_carry_weight(char_name) -> i64
    // Calculate total weight carried by character, accounting for:
    // - Inventory items (full weight)
    // - Equipped non-container items (full weight)
    // - Equipped containers (container weight + reduced contents weight)
    // - Gold is weightless
    let cloned_db = db.clone();
    engine.register_fn("get_character_carry_weight", move |char_name: String| -> i64 {
        let mut total_weight: i64 = 0;

        // Get character data
        let _char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return 0,
        };

        // Calculate inventory weight
        let inventory_items = cloned_db.get_items_in_inventory(&char_name).unwrap_or_default();
        for item in &inventory_items {
            if item.item_type == ItemType::Container {
                // For containers in inventory, add container weight + full contents weight
                total_weight += item.weight as i64;
                let contents_weight = calculate_container_contents_weight(&cloned_db, item);
                total_weight += contents_weight;
            } else {
                total_weight += item.weight as i64;
            }
        }

        // Calculate equipped weight
        let equipped_items = cloned_db.get_equipped_items(&char_name).unwrap_or_default();
        for item in &equipped_items {
            if item.item_type == ItemType::Container {
                // Equipped container: apply weight reduction to contents
                total_weight += item.weight as i64;
                let contents_weight = calculate_container_contents_weight(&cloned_db, &item);
                // Apply weight reduction (default 50% if not set)
                let reduction = if item.weight_reduction > 0 {
                    item.weight_reduction
                } else {
                    50
                };
                let reduced_weight = (contents_weight * (100 - reduction as i64)) / 100;
                total_weight += reduced_weight;
            } else {
                total_weight += item.weight as i64;
            }
        }

        total_weight
    });

    // get_encumbrance_level(char_name) -> String
    // Returns: "light", "medium", "heavy", or "overloaded"
    let cloned_db = db.clone();
    engine.register_fn("get_encumbrance_level", move |char_name: String| -> String {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return "light".to_string(),
        };

        // Calculate max carry weight: 50 + (STR * 10)
        let max_carry = 50 + (char.stat_str as i64 * 10);

        // Calculate current weight
        let current_weight = calculate_total_carry_weight(&cloned_db, &char_name);

        // Calculate percentage
        let percent = if max_carry > 0 {
            (current_weight * 100) / max_carry
        } else {
            0
        };

        if percent > 100 {
            "overloaded".to_string()
        } else if percent > 75 {
            "heavy".to_string()
        } else if percent > 50 {
            "medium".to_string()
        } else {
            "light".to_string()
        }
    });

    // get_encumbrance_percent(char_name) -> i64
    // Returns the percentage of carry capacity used (0-100+)
    let cloned_db = db.clone();
    engine.register_fn("get_encumbrance_percent", move |char_name: String| -> i64 {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return 0,
        };

        let max_carry = 50 + (char.stat_str as i64 * 10);
        let current_weight = calculate_total_carry_weight(&cloned_db, &char_name);

        if max_carry > 0 {
            (current_weight * 100) / max_carry
        } else {
            0
        }
    });

    // can_carry_item(char_name, item_id) -> bool
    // Check if picking up the item would cause overload
    let cloned_db = db.clone();
    engine.register_fn("can_carry_item", move |char_name: String, item_id: String| -> bool {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return false,
        };

        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let item = match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(i)) => i,
            _ => return false,
        };

        let max_carry = 50 + (char.stat_str as i64 * 10);
        let current_weight = calculate_total_carry_weight(&cloned_db, &char_name);

        // Calculate item weight (including contents if container)
        let mut item_weight = item.weight as i64;
        if item.item_type == ItemType::Container {
            item_weight += calculate_container_contents_weight(&cloned_db, &item);
        }

        current_weight + item_weight <= max_carry
    });

    // get_encumbrance_movement_penalty(char_name) -> i64
    // Returns movement speed penalty as percentage (0, 25, 50, or 100)
    let cloned_db = db.clone();
    engine.register_fn("get_encumbrance_movement_penalty", move |char_name: String| -> i64 {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return 0,
        };

        let max_carry = 50 + (char.stat_str as i64 * 10);
        let current_weight = calculate_total_carry_weight(&cloned_db, &char_name);
        let percent = if max_carry > 0 {
            (current_weight * 100) / max_carry
        } else {
            0
        };

        if percent > 100 {
            100 // Cannot move
        } else if percent > 75 {
            50 // Heavy: -50% speed
        } else if percent > 50 {
            25 // Medium: -25% speed
        } else {
            0 // Light: no penalty
        }
    });

    // get_encumbrance_stamina_modifier(char_name) -> i64
    // Returns stamina cost multiplier as percentage (100, 125, 150, or 0 for cannot act)
    let cloned_db = db.clone();
    engine.register_fn("get_encumbrance_stamina_modifier", move |char_name: String| -> i64 {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return 100,
        };

        let max_carry = 50 + (char.stat_str as i64 * 10);
        let current_weight = calculate_total_carry_weight(&cloned_db, &char_name);
        let percent = if max_carry > 0 {
            (current_weight * 100) / max_carry
        } else {
            0
        };

        if percent > 100 {
            0 // Cannot act (signal for scripts to block actions)
        } else if percent > 75 {
            150 // Heavy: +50% stamina cost
        } else if percent > 50 {
            125 // Medium: +25% stamina cost
        } else {
            100 // Light: normal stamina cost
        }
    });

    // ========== Medical Tool Functions ==========

    // set_medical_tier(item_id, tier: 1-3) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_medical_tier", move |item_id: String, tier: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.medical_tier = tier.clamp(0, 3) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_medical_uses(item_id, uses: 0=reusable, >0=consumable) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_medical_uses", move |item_id: String, uses: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.medical_uses = uses.max(0) as i32;
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // add_treats_wound_type(item_id, wound_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_treats_wound_type", move |item_id: String, wound_type: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                let wound_lower = wound_type.to_lowercase();
                if !item.treats_wound_types.contains(&wound_lower) {
                    item.treats_wound_types.push(wound_lower);
                    return cloned_db.save_item_data(item).is_ok();
                }
                return true; // Already has this wound type
            }
        }
        false
    });

    // remove_treats_wound_type(item_id, wound_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_treats_wound_type",
        move |item_id: String, wound_type: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    let wound_lower = wound_type.to_lowercase();
                    item.treats_wound_types.retain(|t| t != &wound_lower);
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // clear_treats_wound_types(item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_treats_wound_types", move |item_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.treats_wound_types.clear();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // set_max_treatable_wound(item_id, severity) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_max_treatable_wound", move |item_id: String, severity: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                item.max_treatable_wound = severity.to_lowercase();
                return cloned_db.save_item_data(item).is_ok();
            }
        }
        false
    });

    // ========== Gardening Item Field Setters ==========

    // set_item_plant_prototype_vnum(item_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_plant_prototype_vnum",
        move |item_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.plant_prototype_vnum = vnum;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_item_fertilizer_duration(item_id, duration) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_fertilizer_duration",
        move |item_id: String, duration: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.fertilizer_duration = duration;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );

    // set_item_treats_infestation(item_id, treats) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_treats_infestation",
        move |item_id: String, treats: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                    item.treats_infestation = treats;
                    return cloned_db.save_item_data(item).is_ok();
                }
            }
            false
        },
    );
}

// Shared impl for set_item_cast_on_use (4-arg + 5-arg variants).
// `cooldown_secs <= 0` clears the per-item override (use spell default).
fn set_item_cast_on_use_impl(
    db: &Arc<Db>,
    item_id: &str,
    spell: String,
    min_level: i64,
    charges: i64,
    cooldown_secs: i64,
) -> bool {
    if spell.trim().is_empty() {
        return false;
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(item_id) {
        if let Ok(Some(mut item)) = db.get_item_data(&uuid) {
            let charges = (charges as i32).max(0);
            let cd = if cooldown_secs > 0 {
                Some(cooldown_secs as i32)
            } else {
                None
            };
            item.cast_on_use = Some(crate::types::CastOnUse {
                spell,
                min_level: (min_level as i32).max(0),
                charges,
                max_charges: charges,
                cooldown_secs: cd,
            });
            return db.save_item_data(item).is_ok();
        }
    }
    false
}

// Helper function to calculate container contents weight
fn calculate_container_contents_weight(db: &Arc<Db>, container: &ItemData) -> i64 {
    let mut weight: i64 = 0;
    for item_id in &container.container_contents {
        if let Ok(Some(item)) = db.get_item_data(item_id) {
            weight += item.weight as i64;
            // Recursively add nested container contents (no reduction for nested)
            if item.item_type == ItemType::Container {
                weight += calculate_container_contents_weight(db, &item);
            }
        }
    }
    weight
}

// Helper function to calculate total carry weight for a character
fn calculate_total_carry_weight(db: &Arc<Db>, char_name: &str) -> i64 {
    let mut total_weight: i64 = 0;

    // Get inventory items
    let inventory_items = db.get_items_in_inventory(char_name).unwrap_or_default();
    for item in &inventory_items {
        if item.item_type == ItemType::Container {
            total_weight += item.weight as i64;
            total_weight += calculate_container_contents_weight(db, item);
        } else {
            total_weight += item.weight as i64;
        }
    }

    // Get equipped items (with weight reduction for containers)
    let equipped_items = db.get_equipped_items(char_name).unwrap_or_default();
    for item in &equipped_items {
        if item.item_type == ItemType::Container {
            total_weight += item.weight as i64;
            let contents_weight = calculate_container_contents_weight(db, &item);
            let reduction = if item.weight_reduction > 0 {
                item.weight_reduction
            } else {
                50
            };
            let reduced_weight = (contents_weight * (100 - reduction as i64)) / 100;
            total_weight += reduced_weight;
        } else {
            total_weight += item.weight as i64;
        }
    }

    total_weight
}

pub(crate) fn on_hit_effect_to_map(effect: &OnHitEffect) -> Dynamic {
    let mut map = Map::new();
    map.insert("effect".into(), Dynamic::from(effect.effect.clone()));
    map.insert("chance".into(), Dynamic::from(effect.chance as i64));
    map.insert("magnitude".into(), Dynamic::from(effect.magnitude as i64));
    map.insert("duration".into(), Dynamic::from(effect.duration as i64));
    Dynamic::from(map)
}

pub(crate) fn parse_on_hit_array(arr: rhai::Array) -> Vec<OnHitEffect> {
    arr.into_iter()
        .filter_map(|d| d.try_cast::<Map>())
        .map(|m| OnHitEffect {
            effect: m
                .get("effect")
                .and_then(|d| d.clone().try_cast::<String>())
                .unwrap_or_default(),
            chance: m
                .get("chance")
                .and_then(|d| d.as_int().ok())
                .map(|n| (n as i32).clamp(0, 100))
                .unwrap_or(0),
            magnitude: m
                .get("magnitude")
                .and_then(|d| d.as_int().ok())
                .map(|n| n as i32)
                .unwrap_or(0),
            duration: m
                .get("duration")
                .and_then(|d| d.as_int().ok())
                .map(|n| (n as i32).max(0))
                .unwrap_or(0),
        })
        .filter(|e| !e.effect.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nth_keyword_plain() {
        assert_eq!(parse_nth_keyword("sword"), (1, "sword"));
        assert_eq!(parse_nth_keyword("iron_sword"), (1, "iron_sword"));
    }

    #[test]
    fn test_parse_nth_keyword_numbered() {
        assert_eq!(parse_nth_keyword("2.guard"), (2, "guard"));
        assert_eq!(parse_nth_keyword("1.sword"), (1, "sword"));
        assert_eq!(parse_nth_keyword("3.corpse"), (3, "corpse"));
        assert_eq!(parse_nth_keyword("10.potion"), (10, "potion"));
    }

    #[test]
    fn test_parse_nth_keyword_zero_falls_through() {
        // 0 is not a valid N, treat as plain keyword
        assert_eq!(parse_nth_keyword("0.sword"), (1, "0.sword"));
    }

    #[test]
    fn test_parse_nth_keyword_non_numeric_prefix() {
        // Non-numeric prefix before dot should not trigger N.keyword
        assert_eq!(parse_nth_keyword("all.sword"), (1, "all.sword"));
        assert_eq!(parse_nth_keyword("foo.bar"), (1, "foo.bar"));
    }

    #[test]
    fn test_parse_nth_keyword_no_dot() {
        assert_eq!(parse_nth_keyword("guard"), (1, "guard"));
    }

    #[test]
    fn test_parse_nth_keyword_with_dots_in_keyword() {
        // "2.some.thing" -> nth=2, keyword="some.thing"
        assert_eq!(parse_nth_keyword("2.some.thing"), (2, "some.thing"));
    }

    #[test]
    fn test_item_matches_keyword_by_name() {
        let keywords = vec!["blade".to_string()];
        assert!(item_matches_keyword("iron sword", &keywords, "sword"));
        assert!(item_matches_keyword("iron sword", &keywords, "iron"));
        assert!(!item_matches_keyword("iron sword", &keywords, "shield"));
    }

    #[test]
    fn test_item_matches_keyword_by_keyword() {
        let keywords = vec!["blade".to_string(), "weapon".to_string()];
        assert!(item_matches_keyword("a fancy item", &keywords, "blade"));
        assert!(item_matches_keyword("a fancy item", &keywords, "weapon"));
        assert!(!item_matches_keyword("a fancy item", &keywords, "armor"));
    }

    #[test]
    fn test_item_matches_keyword_case_insensitive() {
        let keywords = vec!["Blade".to_string()];
        assert!(item_matches_keyword("Iron Sword", &keywords, "iron"));
        assert!(item_matches_keyword("Iron Sword", &keywords, "blade"));
    }

    #[test]
    fn test_item_matches_keyword_partial() {
        let keywords = vec!["longsword".to_string()];
        assert!(item_matches_keyword("a weapon", &keywords, "sword"));
    }
}
