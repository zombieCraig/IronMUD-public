// src/script/areas.rs
// Area system functions: CRUD, permissions, trustees, vnums, forage tables

use crate::db::Db;
use crate::{AreaData, AreaPermission, CombatZoneType};
use rhai::{Engine, EvalAltResult, Position};
use std::sync::Arc;

/// Check if a character is in build mode AND can edit the area containing the given room.
/// Used by Rust tick files to bypass restrictions for builders in their areas.
pub fn check_build_mode(db: &Db, char_name: &str, room_id: &uuid::Uuid) -> bool {
    let char = match db.get_character_data(char_name) {
        Ok(Some(c)) => c,
        _ => return false,
    };
    if !char.build_mode {
        return false;
    }
    let room = match db.get_room_data(room_id) {
        Ok(Some(r)) => r,
        _ => return false,
    };
    let area_id = match room.area_id {
        Some(id) => id,
        None => return false,
    };
    // Admins always pass
    if char.is_admin {
        return true;
    }
    let area = match db.get_area_data(&area_id) {
        Ok(Some(a)) => a,
        _ => return true, // Area not found = allow (matches can_edit_area behavior)
    };
    // No owner = any builder can edit
    if area.owner.is_none() {
        return true;
    }
    let owner = area.owner.as_ref().unwrap();
    match area.permission_level {
        AreaPermission::OwnerOnly => owner.eq_ignore_ascii_case(char_name),
        AreaPermission::Trusted => {
            owner.eq_ignore_ascii_case(char_name)
                || area.trusted_builders.iter().any(|t| t.eq_ignore_ascii_case(char_name))
        }
        AreaPermission::AllBuilders => true,
    }
}

/// Register area-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Area Functions ==========

    // create_area(name, prefix) -> Creates a new area (no owner)
    let cloned_db = db.clone();
    engine.register_fn("create_area", move |name: String, prefix: String| {
        let area = AreaData {
            id: uuid::Uuid::new_v4(),
            name,
            prefix,
            description: String::new(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::Pve,
            flags: crate::AreaFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: crate::types::ImmigrationVariationChances::default(),
            immigration_family_chance: crate::types::ImmigrationFamilyChance::default(),
        };
        if let Err(e) = cloned_db.save_area_data(area.clone()) {
            tracing::error!("Failed to save new area: {}", e);
        }
        area
    });

    // create_area_with_owner(name, prefix, owner) -> Creates a new area with owner
    let cloned_db = db.clone();
    engine.register_fn(
        "create_area_with_owner",
        move |name: String, prefix: String, owner: String| {
            let area = AreaData {
                id: uuid::Uuid::new_v4(),
                name,
                prefix,
                description: String::new(),
                level_min: 0,
                level_max: 0,
                theme: String::new(),
                owner: if owner.is_empty() { None } else { Some(owner) },
                permission_level: AreaPermission::AllBuilders,
                trusted_builders: Vec::new(),
                city_forage_table: Vec::new(),
                wilderness_forage_table: Vec::new(),
                shallow_water_forage_table: Vec::new(),
                deep_water_forage_table: Vec::new(),
                underwater_forage_table: Vec::new(),
                combat_zone: CombatZoneType::Pve,
                flags: crate::AreaFlags::default(),
                immigration_enabled: false,
                immigration_room_vnum: String::new(),
                immigration_name_pool: String::new(),
                immigration_visual_profile: String::new(),
                migration_interval_days: 0,
                migration_max_per_check: 0,
                migrant_sim_defaults: None,
                last_migration_check_day: None,
                immigration_variation_chances: crate::types::ImmigrationVariationChances::default(),
                immigration_family_chance: crate::types::ImmigrationFamilyChance::default(),
            };
            if let Err(e) = cloned_db.save_area_data(area.clone()) {
                tracing::error!("Failed to save new area: {}", e);
            }
            area
        },
    );

    // get_area_data(area_id) -> Gets area by ID
    let cloned_db = db.clone();
    engine.register_fn("get_area_data", move |area_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            match cloned_db.get_area_data(&uuid) {
                Ok(Some(area)) => rhai::Dynamic::from(area),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // save_area_data(area) -> Saves area data
    let cloned_db = db.clone();
    engine.register_fn("save_area_data", move |area: AreaData| {
        cloned_db
            .save_area_data(area)
            .map_err(|e| {
                Box::new(EvalAltResult::ErrorRuntime(
                    rhai::Dynamic::from(format!("DB Error: {}", e)),
                    Position::NONE,
                ))
            })
            .map(|_| rhai::Dynamic::UNIT)
    });

    // ========== Area Setter Functions ==========

    // set_area_name(area_id, name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_name", move |area_id: String, name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.name = name;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_description(area_id, desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_description", move |area_id: String, desc: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.description = desc;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_prefix(area_id, prefix) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_prefix", move |area_id: String, prefix: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.prefix = prefix;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_theme(area_id, theme) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_theme", move |area_id: String, theme: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.theme = theme;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_levels(area_id, min, max) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_levels", move |area_id: String, min: i64, max: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.level_min = min as i32;
                area.level_max = max as i32;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_owner(area_id, owner_name) -> bool (empty string clears owner)
    let cloned_db = db.clone();
    engine.register_fn("set_area_owner", move |area_id: String, owner: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.owner = if owner.is_empty() { None } else { Some(owner) };
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // set_area_permission(area_id, level_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_area_permission", move |area_id: String, level: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.permission_level = match level.to_lowercase().as_str() {
                    "owner_only" | "owner" => AreaPermission::OwnerOnly,
                    "trusted" => AreaPermission::Trusted,
                    "all_builders" | "all" | "builders" => AreaPermission::AllBuilders,
                    _ => return false,
                };
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // add_area_trustee(area_id, character_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_area_trustee", move |area_id: String, name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                // Check if already in list (case-insensitive)
                if !area.trusted_builders.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
                    area.trusted_builders.push(name);
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
        }
        false
    });

    // remove_area_trustee(area_id, character_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_area_trustee", move |area_id: String, name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                let before_len = area.trusted_builders.len();
                area.trusted_builders.retain(|t| !t.eq_ignore_ascii_case(&name));
                if area.trusted_builders.len() < before_len {
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
        }
        false
    });

    // can_edit_area(area_id, character_name) -> bool
    // Checks if a character has permission to edit the area
    let cloned_db = db.clone();
    engine.register_fn("can_edit_area", move |area_id: String, char_name: String| {
        // Admins can always edit any area
        if let Ok(Some(character)) = cloned_db.get_character_data(&char_name) {
            if character.is_admin {
                return true;
            }
        }

        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(area)) = cloned_db.get_area_data(&uuid) {
                // No owner = any builder can edit (caller should check is_builder)
                if area.owner.is_none() {
                    return true;
                }

                let owner = area.owner.as_ref().unwrap();

                match area.permission_level {
                    AreaPermission::OwnerOnly => owner.eq_ignore_ascii_case(&char_name),
                    AreaPermission::Trusted => {
                        owner.eq_ignore_ascii_case(&char_name)
                            || area.trusted_builders.iter().any(|t| t.eq_ignore_ascii_case(&char_name))
                    }
                    AreaPermission::AllBuilders => true,
                }
            } else {
                true // Area not found = allow (let other checks handle it)
            }
        } else {
            true // Invalid ID = allow
        }
    });

    // delete_area(area_id) -> Deletes an area (unassigns rooms, doesn't delete them)
    let cloned_db = db.clone();
    engine.register_fn("delete_area", move |area_id: String| {
        let area_uuid = match uuid::Uuid::parse_str(&area_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.delete_area(&area_uuid).unwrap_or(false)
    });

    // list_all_areas() -> Returns array of all AreaData
    let cloned_db = db.clone();
    engine.register_fn("list_all_areas", move || {
        cloned_db
            .list_all_areas()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // get_area_by_prefix(prefix) -> Gets area by prefix (case-insensitive)
    let cloned_db = db.clone();
    engine.register_fn("get_area_by_prefix", move |prefix: String| {
        let prefix_lower = prefix.to_lowercase();
        for area in cloned_db.list_all_areas().unwrap_or_default() {
            if area.prefix.to_lowercase() == prefix_lower {
                return rhai::Dynamic::from(area);
            }
        }
        rhai::Dynamic::UNIT
    });

    // ========== Unique VNUM Generation ==========

    // generate_unique_vnum(prefix, base_name) -> Generates unique vnum like "prefix:slug" or "prefix:slug_2"
    let cloned_db = db.clone();
    engine.register_fn("generate_unique_vnum", move |prefix: String, base_name: String| {
        // Slugify: lowercase, spaces/dashes -> underscores, alphanumeric only
        let mut slug = String::new();
        let mut last_was_underscore = true; // Skip leading underscores

        for c in base_name.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
                last_was_underscore = false;
            } else if (c == ' ' || c == '-' || c == '_') && !last_was_underscore {
                slug.push('_');
                last_was_underscore = true;
            }
        }
        // Trim trailing underscore
        while slug.ends_with('_') {
            slug.pop();
        }
        // Truncate to 24 chars max
        if slug.len() > 24 {
            slug.truncate(24);
            while slug.ends_with('_') {
                slug.pop();
            }
        }

        let prefix_lower = prefix.to_lowercase();
        let base_vnum = format!("{}:{}", prefix_lower, slug);

        // Check if available
        if cloned_db.get_room_by_vnum(&base_vnum).ok().flatten().is_none() {
            return base_vnum;
        }

        // Try numbered suffixes: _2, _3, ...
        for i in 2..=999 {
            let candidate = format!("{}_{}", base_vnum, i);
            if cloned_db.get_room_by_vnum(&candidate).ok().flatten().is_none() {
                return candidate;
            }
        }

        // Fallback with UUID fragment
        format!(
            "{}_{}",
            base_vnum,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        )
    });

    // generate_unique_item_vnum(base_name) -> Generates unique item vnum like "slug" or "slug_2"
    let cloned_db = db.clone();
    engine.register_fn("generate_unique_item_vnum", move |base_name: String| {
        // Slugify: lowercase, spaces/dashes -> underscores, alphanumeric only
        let mut slug = String::new();
        let mut last_was_underscore = true; // Skip leading underscores

        for c in base_name.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
                last_was_underscore = false;
            } else if c == ':' {
                slug.push(':');
                last_was_underscore = false;
            } else if (c == ' ' || c == '-' || c == '_') && !last_was_underscore {
                slug.push('_');
                last_was_underscore = true;
            }
        }
        // Trim trailing underscore
        while slug.ends_with('_') {
            slug.pop();
        }
        // Truncate to 24 chars max
        if slug.len() > 24 {
            slug.truncate(24);
            while slug.ends_with('_') {
                slug.pop();
            }
        }

        // No prefix for items, just the slug
        let base_vnum = slug;

        // Check if available
        if cloned_db.get_item_by_vnum(&base_vnum).ok().flatten().is_none() {
            return base_vnum;
        }

        // Try numbered suffixes: _2, _3, ...
        for i in 2..=999 {
            let candidate = format!("{}_{}", base_vnum, i);
            if cloned_db.get_item_by_vnum(&candidate).ok().flatten().is_none() {
                return candidate;
            }
        }

        // Fallback with UUID fragment
        format!(
            "{}_{}",
            base_vnum,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        )
    });

    // generate_unique_mobile_vnum(base_name) -> Generates unique mobile vnum like "slug" or "slug_2"
    let cloned_db = db.clone();
    engine.register_fn("generate_unique_mobile_vnum", move |base_name: String| {
        // Slugify: lowercase, spaces/dashes -> underscores, alphanumeric only
        let mut slug = String::new();
        let mut last_was_underscore = true; // Skip leading underscores

        for c in base_name.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
                last_was_underscore = false;
            } else if c == ':' {
                slug.push(':');
                last_was_underscore = false;
            } else if (c == ' ' || c == '-' || c == '_') && !last_was_underscore {
                slug.push('_');
                last_was_underscore = true;
            }
        }
        // Trim trailing underscore
        while slug.ends_with('_') {
            slug.pop();
        }
        // Truncate to 24 chars max
        if slug.len() > 24 {
            slug.truncate(24);
            while slug.ends_with('_') {
                slug.pop();
            }
        }

        // No prefix for mobiles, just the slug
        let base_vnum = slug;

        // Check if available
        if cloned_db.get_mobile_by_vnum(&base_vnum).ok().flatten().is_none() {
            return base_vnum;
        }

        // Try numbered suffixes: _2, _3, ...
        for i in 2..=999 {
            let candidate = format!("{}_{}", base_vnum, i);
            if cloned_db.get_mobile_by_vnum(&candidate).ok().flatten().is_none() {
                return candidate;
            }
        }

        // Fallback with UUID fragment
        format!(
            "{}_{}",
            base_vnum,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        )
    });

    // generate_unique_property_vnum(base_name) -> Generates unique property template vnum like "slug" or "slug_2"
    let cloned_db = db.clone();
    engine.register_fn("generate_unique_property_vnum", move |base_name: String| {
        // Slugify: lowercase, spaces/dashes -> underscores, alphanumeric only
        let mut slug = String::new();
        let mut last_was_underscore = true; // Skip leading underscores

        for c in base_name.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
                last_was_underscore = false;
            } else if c == ':' {
                slug.push(':');
                last_was_underscore = false;
            } else if (c == ' ' || c == '-' || c == '_') && !last_was_underscore {
                slug.push('_');
                last_was_underscore = true;
            }
        }
        // Trim trailing underscore
        while slug.ends_with('_') {
            slug.pop();
        }
        // Truncate to 24 chars max
        if slug.len() > 24 {
            slug.truncate(24);
            while slug.ends_with('_') {
                slug.pop();
            }
        }

        // No prefix for property templates, just the slug
        let base_vnum = slug;

        // Check if available
        if cloned_db
            .get_property_template_by_vnum(&base_vnum)
            .ok()
            .flatten()
            .is_none()
        {
            return base_vnum;
        }

        // Try numbered suffixes: _2, _3, ...
        for i in 2..=999 {
            let candidate = format!("{}_{}", base_vnum, i);
            if cloned_db
                .get_property_template_by_vnum(&candidate)
                .ok()
                .flatten()
                .is_none()
            {
                return candidate;
            }
        }

        // Fallback with UUID fragment
        format!(
            "{}_{}",
            base_vnum,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        )
    });

    // ========== Room-Area Linking ==========

    // set_room_area(room_id, area_id) -> Assigns room to area
    let cloned_db = db.clone();
    engine.register_fn("set_room_area", move |room_id: String, area_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let area_uuid = match uuid::Uuid::parse_str(&area_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.set_room_area(&room_uuid, &area_uuid).unwrap_or(false)
    });

    // clear_room_area(room_id) -> Removes room from its area
    let cloned_db = db.clone();
    engine.register_fn("clear_room_area", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.clear_room_area(&room_uuid).unwrap_or(false)
    });

    // get_rooms_in_area(area_id) -> Returns array of rooms in area
    let cloned_db = db.clone();
    engine.register_fn("get_rooms_in_area", move |area_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            cloned_db
                .get_rooms_in_area(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // ========== Area Forage Table Functions ==========

    // add_area_city_forage(area_id, vnum, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_area_city_forage",
        move |area_id: String, vnum: String, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        // Check if vnum already exists
                        if area.city_forage_table.iter().any(|e| e.vnum == vnum) {
                            return false;
                        }
                        area.city_forage_table.push(crate::ForageEntry {
                            vnum,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_area_data(area).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_area_city_forage(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_area_city_forage",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        let orig_len = area.city_forage_table.len();
                        area.city_forage_table.retain(|e| e.vnum != vnum);
                        if area.city_forage_table.len() < orig_len {
                            cloned_db.save_area_data(area).is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_area_city_forage_table(area_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_area_city_forage_table",
        move |area_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => area
                        .city_forage_table
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                            map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                            map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                            rhai::Dynamic::from(map)
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // add_area_wilderness_forage(area_id, vnum, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_area_wilderness_forage",
        move |area_id: String, vnum: String, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        // Check if vnum already exists
                        if area.wilderness_forage_table.iter().any(|e| e.vnum == vnum) {
                            return false;
                        }
                        area.wilderness_forage_table.push(crate::ForageEntry {
                            vnum,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_area_data(area).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_area_wilderness_forage(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_area_wilderness_forage",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        let orig_len = area.wilderness_forage_table.len();
                        area.wilderness_forage_table.retain(|e| e.vnum != vnum);
                        if area.wilderness_forage_table.len() < orig_len {
                            cloned_db.save_area_data(area).is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_area_wilderness_forage_table(area_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_area_wilderness_forage_table",
        move |area_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => area
                        .wilderness_forage_table
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                            map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                            map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                            rhai::Dynamic::from(map)
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // ========== Water Forage Table Functions ==========

    // add_area_shallow_water_forage(area_id, vnum, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_area_shallow_water_forage",
        move |area_id: String, vnum: String, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        if area.shallow_water_forage_table.iter().any(|e| e.vnum == vnum) {
                            return false;
                        }
                        area.shallow_water_forage_table.push(crate::ForageEntry {
                            vnum,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_area_data(area).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_area_shallow_water_forage(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_area_shallow_water_forage",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        let orig_len = area.shallow_water_forage_table.len();
                        area.shallow_water_forage_table.retain(|e| e.vnum != vnum);
                        if area.shallow_water_forage_table.len() < orig_len {
                            cloned_db.save_area_data(area).is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_area_shallow_water_forage_table(area_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_area_shallow_water_forage_table",
        move |area_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => area
                        .shallow_water_forage_table
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                            map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                            map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                            rhai::Dynamic::from(map)
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // add_area_deep_water_forage(area_id, vnum, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_area_deep_water_forage",
        move |area_id: String, vnum: String, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        if area.deep_water_forage_table.iter().any(|e| e.vnum == vnum) {
                            return false;
                        }
                        area.deep_water_forage_table.push(crate::ForageEntry {
                            vnum,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_area_data(area).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_area_deep_water_forage(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_area_deep_water_forage",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        let orig_len = area.deep_water_forage_table.len();
                        area.deep_water_forage_table.retain(|e| e.vnum != vnum);
                        if area.deep_water_forage_table.len() < orig_len {
                            cloned_db.save_area_data(area).is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_area_deep_water_forage_table(area_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_area_deep_water_forage_table",
        move |area_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => area
                        .deep_water_forage_table
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                            map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                            map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                            rhai::Dynamic::from(map)
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // add_area_underwater_forage(area_id, vnum, min_skill, rarity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_area_underwater_forage",
        move |area_id: String, vnum: String, min_skill: i64, rarity: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        if area.underwater_forage_table.iter().any(|e| e.vnum == vnum) {
                            return false;
                        }
                        area.underwater_forage_table.push(crate::ForageEntry {
                            vnum,
                            min_skill: min_skill as i32,
                            rarity,
                        });
                        cloned_db.save_area_data(area).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // remove_area_underwater_forage(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_area_underwater_forage",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(mut area)) => {
                        let orig_len = area.underwater_forage_table.len();
                        area.underwater_forage_table.retain(|e| e.vnum != vnum);
                        if area.underwater_forage_table.len() < orig_len {
                            cloned_db.save_area_data(area).is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_area_underwater_forage_table(area_id) -> Array of Maps
    let cloned_db = db.clone();
    engine.register_fn(
        "get_area_underwater_forage_table",
        move |area_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => area
                        .underwater_forage_table
                        .iter()
                        .map(|entry| {
                            let mut map = rhai::Map::new();
                            map.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                            map.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                            map.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                            rhai::Dynamic::from(map)
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // select_area_forage(area_id, forage_type, skill_level) -> Map with vnum and rarity, or () if nothing
    // forage_type: "city", "wilderness", "shallow_water", "deep_water", or "underwater"
    // Uses item weight from prototype for weighted selection
    let cloned_db = db.clone();
    engine.register_fn(
        "select_area_forage",
        move |area_id: String, forage_type: String, skill_level: i64| -> rhai::Dynamic {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                match cloned_db.get_area_data(&uuid) {
                    Ok(Some(area)) => {
                        let forage_table = match forage_type.to_lowercase().as_str() {
                            "city" => &area.city_forage_table,
                            "wilderness" => &area.wilderness_forage_table,
                            "shallow_water" => &area.shallow_water_forage_table,
                            "deep_water" => &area.deep_water_forage_table,
                            "underwater" => &area.underwater_forage_table,
                            _ => return rhai::Dynamic::UNIT,
                        };

                        // Filter by skill level
                        let available: Vec<_> = forage_table
                            .iter()
                            .filter(|e| (e.min_skill as i64) <= skill_level)
                            .collect();

                        if available.is_empty() {
                            return rhai::Dynamic::UNIT;
                        }

                        // Get weights from item prototypes
                        let mut weighted_entries: Vec<(&crate::ForageEntry, i32)> = Vec::new();
                        for entry in &available {
                            // Try to get item weight from prototype
                            let weight = if let Ok(Some(item)) = cloned_db.get_item_by_vnum(&entry.vnum) {
                                item.weight.max(1) // Minimum weight of 1
                            } else {
                                1 // Default weight if prototype not found
                            };
                            weighted_entries.push((entry, weight));
                        }

                        let total_weight: i32 = weighted_entries.iter().map(|(_, w)| *w).sum();
                        if total_weight <= 0 {
                            return rhai::Dynamic::UNIT;
                        }

                        // Random selection
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let seed = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let mut roll = (seed % total_weight as u64) as i32;

                        for (entry, weight) in weighted_entries {
                            roll -= weight;
                            if roll < 0 {
                                let mut result = rhai::Map::new();
                                result.insert("vnum".into(), rhai::Dynamic::from(entry.vnum.clone()));
                                result.insert("rarity".into(), rhai::Dynamic::from(entry.rarity.clone()));
                                result.insert("min_skill".into(), rhai::Dynamic::from(entry.min_skill as i64));
                                return rhai::Dynamic::from(result);
                            }
                        }
                        rhai::Dynamic::UNIT
                    }
                    _ => rhai::Dynamic::UNIT,
                }
            } else {
                rhai::Dynamic::UNIT
            }
        },
    );

    // ========== Combat Zone Functions ==========

    // set_area_combat_zone(area_id, zone_type) -> bool
    // Sets the area's combat zone type ("pve", "safe", or "pvp")
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_combat_zone",
        move |area_id: String, zone_type: String| -> bool {
            if let Some(zone) = CombatZoneType::from_str(&zone_type) {
                if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                    if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                        area.combat_zone = zone;
                        return cloned_db.save_area_data(area).is_ok();
                    }
                }
            }
            false
        },
    );

    // get_area_combat_zone(area_id) -> String
    // Gets the area's combat zone type ("pve", "safe", or "pvp")
    let cloned_db = db.clone();
    engine.register_fn("get_area_combat_zone", move |area_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(area)) = cloned_db.get_area_data(&uuid) {
                return area.combat_zone.to_display_string().to_string();
            }
        }
        "pve".to_string() // Default
    });

    // ========== Area Flags Functions ==========

    // set_area_flag(area_id, flag_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_flag",
        move |area_id: String, flag_name: String, value: bool| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    match flag_name.to_lowercase().as_str() {
                        "climate_controlled" => area.flags.climate_controlled = value,
                        _ => return false,
                    }
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // get_area_flag(area_id, flag_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("get_area_flag", move |area_id: String, flag_name: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(area)) = cloned_db.get_area_data(&uuid) {
                return match flag_name.to_lowercase().as_str() {
                    "climate_controlled" => area.flags.climate_controlled,
                    _ => false,
                };
            }
        }
        false
    });

    // get_effective_climate_controlled(room_id) -> bool
    // Checks room flag first, then falls back to area flag
    let cloned_db = db.clone();
    engine.register_fn("get_effective_climate_controlled", move |room_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                // Room flag takes precedence
                if room.flags.climate_controlled {
                    return true;
                }
                // Fall back to area flag
                if let Some(area_id) = room.area_id {
                    if let Ok(Some(area)) = cloned_db.get_area_data(&area_id) {
                        return area.flags.climate_controlled;
                    }
                }
            }
        }
        false
    });

    // get_room_temperature(room_id) -> Map { temperature: i64, temperature_desc: String }
    // Returns effective temperature for a room accounting for indoor moderation, always_cold/always_hot
    let cloned_db = db.clone();
    engine.register_fn("get_room_temperature", move |room_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let room = match cloned_db.get_room_data(&uuid) {
            Ok(Some(r)) => r,
            _ => return rhai::Dynamic::UNIT,
        };

        let game_time = match cloned_db.get_game_time() {
            Ok(gt) => gt,
            Err(_) => return rhai::Dynamic::UNIT,
        };

        let outdoor_temp = game_time.calculate_effective_temperature();

        // Check climate controlled (room or area)
        let is_climate_controlled = room.flags.climate_controlled
            || room
                .area_id
                .and_then(|aid| cloned_db.get_area_data(&aid).ok().flatten())
                .map(|area| area.flags.climate_controlled)
                .unwrap_or(false);
        let is_outdoors = !room.flags.indoors && !is_climate_controlled;

        let effective_temp = if room.flags.always_cold {
            -5
        } else if room.flags.always_hot {
            36
        } else if is_climate_controlled {
            15
        } else if !is_outdoors {
            let target = 15;
            outdoor_temp + ((target - outdoor_temp) * 60 / 100)
        } else {
            outdoor_temp
        };

        let temp_desc = match effective_temp {
            t if t < 0 => "freezing cold",
            t if t < 10 => "cold",
            t if t < 15 => "cool",
            t if t < 20 => "mild",
            t if t < 25 => "warm",
            t if t < 35 => "hot",
            _ => "sweltering",
        };

        let mut map = rhai::Map::new();
        map.insert("temperature".into(), rhai::Dynamic::from(effective_temp as i64));
        map.insert("temperature_desc".into(), rhai::Dynamic::from(temp_desc.to_string()));
        rhai::Dynamic::from(map)
    });

    // is_in_build_mode(char_name, room_id) -> bool
    // Checks if a character has build_mode enabled AND can edit the area containing the room
    let cloned_db = db.clone();
    engine.register_fn("is_in_build_mode", move |char_name: String, room_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            check_build_mode(&cloned_db, &char_name, &uuid)
        } else {
            false
        }
    });

    // ========== Migrant Immigration Setters ==========

    // set_area_immigration_enabled(area_id, enabled) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_enabled",
        move |area_id: String, enabled: bool| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    area.immigration_enabled = enabled;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_immigration_room(area_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_room",
        move |area_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    area.immigration_room_vnum = vnum;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_immigration_name_pool(area_id, pool_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_name_pool",
        move |area_id: String, pool: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    area.immigration_name_pool = pool;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_immigration_visual_profile(area_id, profile_name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_visual_profile",
        move |area_id: String, profile: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    area.immigration_visual_profile = profile;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_migration_interval_days(area_id, days) -> bool (clamped 1..=30)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_migration_interval_days",
        move |area_id: String, days: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    let clamped = days.clamp(1, 30) as u8;
                    area.migration_interval_days = clamped;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_migration_max_per_check(area_id, max) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_migration_max_per_check",
        move |area_id: String, max: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    area.migration_max_per_check = max.clamp(0, 255) as u8;
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_migrant_sim_work_hours(area_id, start, end) -> bool
    // Initializes migrant_sim_defaults if None.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_migrant_sim_work_hours",
        move |area_id: String, start: i64, end: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    let mut sim = area.migrant_sim_defaults.clone().unwrap_or_else(default_sim);
                    sim.work_start_hour = start.clamp(0, 23) as u8;
                    sim.work_end_hour = end.clamp(0, 23) as u8;
                    area.migrant_sim_defaults = Some(sim);
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_migrant_sim_work_pay(area_id, pay) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_migrant_sim_work_pay",
        move |area_id: String, pay: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    let mut sim = area.migrant_sim_defaults.clone().unwrap_or_else(default_sim);
                    sim.work_pay = pay.clamp(0, i32::MAX as i64) as i32;
                    area.migrant_sim_defaults = Some(sim);
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_immigration_variation_chance(area_id, role, chance) -> bool
    // Adding a new role: append a match arm here + a new field on
    // ImmigrationVariationChances + a getter in src/script/mod.rs.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_variation_chance",
        move |area_id: String, role: String, chance: f64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    let c = (chance as f32).clamp(0.0, 1.0);
                    match role.as_str() {
                        "guard" => area.immigration_variation_chances.guard = c,
                        "healer" => area.immigration_variation_chances.healer = c,
                        "scavenger" => area.immigration_variation_chances.scavenger = c,
                        _ => return false,
                    }
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // set_area_immigration_family_chance(area_id, shape, chance) -> bool
    // `shape`: "parent_child" | "sibling_pair". Clamps chance to [0.0, 1.0].
    let cloned_db = db.clone();
    engine.register_fn(
        "set_area_immigration_family_chance",
        move |area_id: String, shape: String, chance: f64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
                if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                    let c = (chance as f32).clamp(0.0, 1.0);
                    match shape.as_str() {
                        "parent_child" => area.immigration_family_chance.parent_child = c,
                        "sibling_pair" => area.immigration_family_chance.sibling_pair = c,
                        _ => return false,
                    }
                    return cloned_db.save_area_data(area).is_ok();
                }
            }
            false
        },
    );

    // clear_area_migrant_sim_defaults(area_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_area_migrant_sim_defaults", move |area_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            if let Ok(Some(mut area)) = cloned_db.get_area_data(&uuid) {
                area.migrant_sim_defaults = None;
                return cloned_db.save_area_data(area).is_ok();
            }
        }
        false
    });

    // is_valid_name_pool(name) -> bool - check if a name pool config file exists
    engine.register_fn("is_valid_name_pool", |name: String| -> bool {
        let path = std::path::Path::new("scripts/data/names").join(format!("{}.json", name));
        path.exists()
    });

    // is_valid_visual_profile(name) -> bool - check if a visual profile config file exists
    engine.register_fn("is_valid_visual_profile", |name: String| -> bool {
        let path = std::path::Path::new("scripts/data/visuals").join(format!("{}.json", name));
        path.exists()
    });

    // list_name_pools() -> Array - list available name pool names
    engine.register_fn("list_name_pools", || -> rhai::Array {
        let dir = std::path::Path::new("scripts/data/names");
        let mut pools = rhai::Array::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        pools.push(rhai::Dynamic::from(name.to_string_lossy().to_string()));
                    }
                }
            }
        }
        pools
    });

    // list_visual_profiles() -> Array - list available visual profile names
    engine.register_fn("list_visual_profiles", || -> rhai::Array {
        let dir = std::path::Path::new("scripts/data/visuals");
        let mut profiles = rhai::Array::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        profiles.push(rhai::Dynamic::from(name.to_string_lossy().to_string()));
                    }
                }
            }
        }
        profiles
    });
}

fn default_sim() -> crate::SimulationConfig {
    crate::SimulationConfig {
        home_room_vnum: String::new(),
        work_room_vnum: String::new(),
        shop_room_vnum: String::new(),
        preferred_food_vnum: String::new(),
        work_pay: 50,
        work_start_hour: 8,
        work_end_hour: 17,
        hunger_decay_rate: 0,
        energy_decay_rate: 0,
        comfort_decay_rate: 0,
        low_gold_threshold: 10,
    }
}
