// src/script/characters.rs
// Character-related functions: creation wizard, permissions, settings, game time,
// thirst, stamina penalties, darkness/vision, gold, skills, buffs, mana, and foraging

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::{
    ActiveBuff, BodyPart, EffectType, STARTING_ROOM_ID, WeaponSkill, broadcast_to_all_players,
    broadcast_to_outdoor_players, fire_environmental_triggers_impl, get_season_transition_message,
    get_time_transition_message,
};
use rhai::Engine;
use std::sync::Arc;

/// Register character-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // ========== Character Creation Wizard Functions ==========

    // set_wizard_data(connection_id, data) -> Store wizard state (JSON string)
    let conns = connections.clone();
    engine.register_fn("set_wizard_data", move |connection_id: String, data: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.wizard_data = if data.is_empty() { None } else { Some(data) };
                return true;
            }
        }
        false
    });

    // get_wizard_data(connection_id) -> Get wizard state (empty string if none)
    let conns = connections.clone();
    engine.register_fn("get_wizard_data", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                return session.wizard_data.clone().unwrap_or_default();
            }
        }
        String::new()
    });

    // clear_wizard_data(connection_id) -> Clear wizard state
    let conns = connections.clone();
    engine.register_fn("clear_wizard_data", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.wizard_data = None;
                return true;
            }
        }
        false
    });

    // get_class_list() -> Array of available class IDs
    let state_clone = state.clone();
    engine.register_fn("get_class_list", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .class_definitions
            .iter()
            .filter(|(_, c)| c.available)
            .map(|(id, _)| rhai::Dynamic::from(id.clone()))
            .collect()
    });

    // get_class_info(class_id) -> Map with name, description, starting_skills, stat_bonuses
    let state_clone = state.clone();
    engine.register_fn("get_class_info", move |class_id: String| -> rhai::Map {
        let world = state_clone.lock().unwrap();
        let mut map = rhai::Map::new();
        if let Some(class) = world.class_definitions.get(&class_id) {
            map.insert("id".into(), rhai::Dynamic::from(class.id.clone()));
            map.insert("name".into(), rhai::Dynamic::from(class.name.clone()));
            map.insert("description".into(), rhai::Dynamic::from(class.description.clone()));
            map.insert("available".into(), rhai::Dynamic::from(class.available));
            // Convert starting_skills HashMap to Rhai Map
            let skills_map: rhai::Map = class
                .starting_skills
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                .collect();
            map.insert("starting_skills".into(), rhai::Dynamic::from(skills_map));
            // Convert stat_bonuses HashMap to Rhai Map
            let bonuses_map: rhai::Map = class
                .stat_bonuses
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                .collect();
            map.insert("stat_bonuses".into(), rhai::Dynamic::from(bonuses_map));
        }
        map
    });

    // get_random_class() -> String (random available class ID, excluding "unemployed")
    let state_clone = state.clone();
    engine.register_fn("get_random_class", move || -> String {
        let world = state_clone.lock().unwrap();
        let available: Vec<&String> = world
            .class_definitions
            .iter()
            .filter(|(id, c)| c.available && id.as_str() != "unemployed")
            .map(|(id, _)| id)
            .collect();
        if available.is_empty() {
            return "unemployed".to_string();
        }
        use rand::seq::SliceRandom;
        available
            .choose(&mut rand::thread_rng())
            .map(|id| (*id).clone())
            .unwrap_or_else(|| "unemployed".to_string())
    });

    // get_trait_list() -> Array of available trait IDs
    let state_clone = state.clone();
    engine.register_fn("get_trait_list", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .trait_definitions
            .iter()
            .filter(|(_, t)| t.available)
            .map(|(id, _)| rhai::Dynamic::from(id.clone()))
            .collect()
    });

    // get_trait_info(trait_id) -> Map with name, description, cost, category, effects
    let state_clone = state.clone();
    engine.register_fn("get_trait_info", move |trait_id: String| -> rhai::Map {
        let world = state_clone.lock().unwrap();
        let mut map = rhai::Map::new();
        if let Some(tr) = world.trait_definitions.get(&trait_id) {
            map.insert("id".into(), rhai::Dynamic::from(tr.id.clone()));
            map.insert("name".into(), rhai::Dynamic::from(tr.name.clone()));
            map.insert("description".into(), rhai::Dynamic::from(tr.description.clone()));
            map.insert("cost".into(), rhai::Dynamic::from(tr.cost as i64));
            map.insert(
                "category".into(),
                rhai::Dynamic::from(match tr.category {
                    crate::TraitCategory::Positive => "positive".to_string(),
                    crate::TraitCategory::Negative => "negative".to_string(),
                }),
            );
            map.insert("available".into(), rhai::Dynamic::from(tr.available));
            // Convert conflicts_with Vec to Array
            let conflicts: rhai::Array = tr
                .conflicts_with
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect();
            map.insert("conflicts_with".into(), rhai::Dynamic::from(conflicts));
            // Convert effects HashMap to Map
            let effects_map: rhai::Map = tr
                .effects
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                .collect();
            map.insert("effects".into(), rhai::Dynamic::from(effects_map));
        }
        map
    });

    // get_random_race() -> String (random available race definition ID)
    let state_clone = state.clone();
    engine.register_fn("get_random_race", move || -> String {
        let world = state_clone.lock().unwrap();
        let available: Vec<&String> = world
            .race_definitions
            .iter()
            .filter(|(_, r)| r.available)
            .map(|(id, _)| id)
            .collect();
        if available.is_empty() {
            return "human".to_string();
        }
        use rand::seq::SliceRandom;
        available
            .choose(&mut rand::thread_rng())
            .map(|id| (*id).clone())
            .unwrap_or_else(|| "human".to_string())
    });

    // get_race_list() -> Array of available race definition IDs
    let state_clone = state.clone();
    engine.register_fn("get_race_list", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .race_definitions
            .iter()
            .filter(|(_, r)| r.available)
            .map(|(id, _)| rhai::Dynamic::from(id.clone()))
            .collect()
    });

    // get_race_info(race_id) -> Map with all definition fields
    let state_clone = state.clone();
    engine.register_fn("get_race_info", move |race_id: String| -> rhai::Map {
        let world = state_clone.lock().unwrap();
        let mut map = rhai::Map::new();
        if let Some(race) = world.race_definitions.get(&race_id.to_lowercase()) {
            map.insert("id".into(), rhai::Dynamic::from(race.id.clone()));
            map.insert("name".into(), rhai::Dynamic::from(race.name.clone()));
            map.insert("description".into(), rhai::Dynamic::from(race.description.clone()));
            map.insert("available".into(), rhai::Dynamic::from(race.available));
            // stat_modifiers
            let stat_map: rhai::Map = race
                .stat_modifiers
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                .collect();
            map.insert("stat_modifiers".into(), rhai::Dynamic::from(stat_map));
            // granted_traits
            let traits: rhai::Array = race
                .granted_traits
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect();
            map.insert("granted_traits".into(), rhai::Dynamic::from(traits));
            // resistances
            let resist_map: rhai::Map = race
                .resistances
                .iter()
                .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                .collect();
            map.insert("resistances".into(), rhai::Dynamic::from(resist_map));
            // passive_abilities
            let passives: rhai::Array = race
                .passive_abilities
                .iter()
                .map(|p| {
                    let mut pmap = rhai::Map::new();
                    pmap.insert("id".into(), rhai::Dynamic::from(p.id.clone()));
                    pmap.insert("name".into(), rhai::Dynamic::from(p.name.clone()));
                    pmap.insert("description".into(), rhai::Dynamic::from(p.description.clone()));
                    let eff: rhai::Map = p
                        .effects
                        .iter()
                        .map(|(k, v)| (k.clone().into(), rhai::Dynamic::from(*v as i64)))
                        .collect();
                    pmap.insert("effects".into(), rhai::Dynamic::from(eff));
                    rhai::Dynamic::from(pmap)
                })
                .collect();
            map.insert("passive_abilities".into(), rhai::Dynamic::from(passives));
            // active_abilities
            let actives: rhai::Array = race
                .active_abilities
                .iter()
                .map(|a| {
                    let mut amap = rhai::Map::new();
                    amap.insert("id".into(), rhai::Dynamic::from(a.id.clone()));
                    amap.insert("name".into(), rhai::Dynamic::from(a.name.clone()));
                    amap.insert("description".into(), rhai::Dynamic::from(a.description.clone()));
                    amap.insert("cooldown_secs".into(), rhai::Dynamic::from(a.cooldown_secs as i64));
                    amap.insert("mana_cost".into(), rhai::Dynamic::from(a.mana_cost as i64));
                    amap.insert("stamina_cost".into(), rhai::Dynamic::from(a.stamina_cost as i64));
                    rhai::Dynamic::from(amap)
                })
                .collect();
            map.insert("active_abilities".into(), rhai::Dynamic::from(actives));
        }
        map
    });

    // is_valid_race(race_id) -> bool
    let state_clone = state.clone();
    engine.register_fn("is_valid_race", move |race_id: String| -> bool {
        let world = state_clone.lock().unwrap();
        world.race_definitions.contains_key(&race_id.to_lowercase())
    });

    // get_racial_resistance(char_name, damage_type_str) -> i64 (% modifier)
    let state_clone = state.clone();
    let cloned_db2 = db.clone();
    engine.register_fn(
        "get_racial_resistance",
        move |char_name: String, damage_type: String| -> i64 {
            let race_id = match cloned_db2.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c.race.to_lowercase(),
                _ => return 0,
            };
            let world = state_clone.lock().unwrap();
            if let Some(race) = world.race_definitions.get(&race_id) {
                *race.resistances.get(&damage_type.to_lowercase()).unwrap_or(&0) as i64
            } else {
                0
            }
        },
    );

    // has_racial_passive(char_name, passive_id) -> bool
    let state_clone = state.clone();
    let cloned_db2 = db.clone();
    engine.register_fn(
        "has_racial_passive",
        move |char_name: String, passive_id: String| -> bool {
            let race_id = match cloned_db2.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c.race.to_lowercase(),
                _ => return false,
            };
            let world = state_clone.lock().unwrap();
            if let Some(race) = world.race_definitions.get(&race_id) {
                race.passive_abilities.iter().any(|p| p.id == passive_id)
            } else {
                false
            }
        },
    );

    // get_racial_passive_effect(char_name, effect_key) -> i64
    let state_clone = state.clone();
    let cloned_db2 = db.clone();
    engine.register_fn(
        "get_racial_passive_effect",
        move |char_name: String, effect_key: String| -> i64 {
            let race_id = match cloned_db2.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c.race.to_lowercase(),
                _ => return 0,
            };
            let world = state_clone.lock().unwrap();
            if let Some(race) = world.race_definitions.get(&race_id) {
                for passive in &race.passive_abilities {
                    if let Some(val) = passive.effects.get(&effect_key) {
                        return *val as i64;
                    }
                }
            }
            0
        },
    );

    // get_race_name(race_id) -> String (display name from definition)
    let state_clone = state.clone();
    engine.register_fn("get_race_name", move |race_id: String| -> String {
        let world = state_clone.lock().unwrap();
        if let Some(race) = world.race_definitions.get(&race_id.to_lowercase()) {
            race.name.clone()
        } else if race_id.is_empty() {
            "(none)".to_string()
        } else {
            // Fallback: capitalize the raw ID for legacy characters
            let mut chars = race_id.chars();
            match chars.next() {
                None => race_id,
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    });

    // get_random_short_desc() -> Default description
    engine.register_fn("get_random_short_desc", || -> String {
        "A nondescript adventurer.".to_string()
    });

    // delete_character(name) -> Delete a character from the database (for cancelling creation)
    let cloned_db = db.clone();
    engine.register_fn("delete_character", move |name: String| -> bool {
        match cloned_db.delete_character_data(&name) {
            Ok(_) => true,
            Err(e) => {
                tracing::error!("Failed to delete character '{}': {}", name, e);
                false
            }
        }
    });

    // ========== Builder Permission Functions ==========

    // toggle_own_builder_flag(connection_id, is_builder) -> Toggle the caller's own builder status
    // Used for "builder_mode=all" where any player can self-toggle builder access
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "toggle_own_builder_flag",
        move |connection_id: String, is_builder: bool| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let char_name = {
                    let conns_guard = conns.lock().unwrap();
                    conns_guard
                        .get(&uuid)
                        .and_then(|s| s.character.as_ref())
                        .map(|c| c.name.clone())
                };
                if let Some(name) = char_name {
                    if let Ok(Some(mut character)) = cloned_db.get_character_data(&name) {
                        character.is_builder = is_builder;
                        if let Err(e) = cloned_db.save_character_data(character.clone()) {
                            tracing::error!("Failed to save builder flag: {}", e);
                            return false;
                        }
                        // Update session
                        let mut conns_guard = conns.lock().unwrap();
                        if let Some(session) = conns_guard.get_mut(&uuid) {
                            if let Some(ref mut session_char) = session.character {
                                session_char.is_builder = is_builder;
                            }
                        }
                        return true;
                    }
                }
            }
            false
        },
    );

    // set_builder_flag(connection_id, character_name, is_builder) -> Set builder status for a character
    // Requires calling connection to be an admin. Also updates the session if the character is online.
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "set_builder_flag",
        move |connection_id: String, character_name: String, is_builder: bool| {
            // Verify caller is admin
            if let Ok(caller_uuid) = uuid::Uuid::parse_str(&connection_id) {
                let conns_guard = conns.lock().unwrap();
                let is_admin = conns_guard
                    .get(&caller_uuid)
                    .and_then(|s| s.character.as_ref())
                    .map(|c| c.is_admin)
                    .unwrap_or(false);
                drop(conns_guard);

                if !is_admin {
                    tracing::warn!("[SECURITY] Non-admin attempted set_builder_flag for {}", character_name);
                    return false;
                }
            } else {
                return false;
            }

            if let Ok(Some(mut character)) = cloned_db.get_character_data(&character_name) {
                character.is_builder = is_builder;
                if let Err(e) = cloned_db.save_character_data(character.clone()) {
                    tracing::error!("Failed to save character builder flag: {}", e);
                    return false;
                }

                // Also update the session if this character is logged in
                let mut conns = conns.lock().unwrap();
                for (_id, session) in conns.iter_mut() {
                    if let Some(ref mut session_char) = session.character {
                        if session_char.name.eq_ignore_ascii_case(&character_name) {
                            session_char.is_builder = is_builder;
                            break;
                        }
                    }
                }

                true
            } else {
                false
            }
        },
    );

    // set_admin_flag(connection_id, character_name, is_admin) -> Set admin status for a character
    // Requires calling connection to be an admin. Also updates the session if the character is online.
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "set_admin_flag",
        move |connection_id: String, character_name: String, is_admin: bool| {
            // Verify caller is admin
            if let Ok(caller_uuid) = uuid::Uuid::parse_str(&connection_id) {
                let conns_guard = conns.lock().unwrap();
                let caller_is_admin = conns_guard
                    .get(&caller_uuid)
                    .and_then(|s| s.character.as_ref())
                    .map(|c| c.is_admin)
                    .unwrap_or(false);
                drop(conns_guard);

                if !caller_is_admin {
                    tracing::warn!("[SECURITY] Non-admin attempted set_admin_flag for {}", character_name);
                    return false;
                }
            } else {
                return false;
            }

            if let Ok(Some(mut character)) = cloned_db.get_character_data(&character_name) {
                character.is_admin = is_admin;
                if let Err(e) = cloned_db.save_character_data(character.clone()) {
                    tracing::error!("Failed to save character admin flag: {}", e);
                    return false;
                }

                // Also update the session if this character is logged in
                let mut conns = conns.lock().unwrap();
                for (_id, session) in conns.iter_mut() {
                    if let Some(ref mut session_char) = session.character {
                        if session_char.name.eq_ignore_ascii_case(&character_name) {
                            session_char.is_admin = is_admin;
                            break;
                        }
                    }
                }

                true
            } else {
                false
            }
        },
    );

    // ========== Settings Functions ==========

    // get_setting(key) -> String (empty if not set)
    let cloned_db = db.clone();
    engine.register_fn("get_setting", move |key: String| {
        cloned_db.get_setting(&key).unwrap_or(None).unwrap_or_default()
    });

    // get_setting_or_default(key, default) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_setting_or_default", move |key: String, default: String| {
        cloned_db.get_setting_or_default(&key, &default).unwrap_or(default)
    });

    // set_setting(key, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_setting", move |key: String, value: String| {
        cloned_db.set_setting(&key, &value).is_ok()
    });

    // resolve_starting_room_uuid() -> String
    // Reads the `starting_room_id` setting (a room vnum). If set and resolvable via the
    // vnum index, returns that room's UUID as a string. Falls back to STARTING_ROOM_ID
    // for unset or unresolvable values, so a misconfigured setting can't brick character
    // creation. Warns on unresolvable vnums so operators notice typos.
    let cloned_db = db.clone();
    engine.register_fn("resolve_starting_room_uuid", move || -> String {
        if let Ok(Some(vnum)) = cloned_db.get_setting("starting_room_id") {
            let trimmed = vnum.trim();
            if !trimmed.is_empty() {
                match cloned_db.get_room_by_vnum(trimmed) {
                    Ok(Some(room)) => return room.id.to_string(),
                    _ => tracing::warn!(
                        "starting_room_id setting '{}' does not resolve to a room; using default",
                        vnum
                    ),
                }
            }
        }
        STARTING_ROOM_ID.to_string()
    });

    // count_characters() -> i64 - Count total characters in database
    let cloned_db = db.clone();
    engine.register_fn("count_characters", move || {
        cloned_db.count_characters().unwrap_or(0) as i64
    });

    // ========== Game Time Functions ==========

    // get_game_time() -> Map with time info
    let cloned_db = db.clone();
    engine.register_fn("get_game_time", move || {
        match cloned_db.get_game_time() {
            Ok(gt) => {
                let mut map = rhai::Map::new();
                map.insert("hour".into(), rhai::Dynamic::from(gt.hour as i64));
                map.insert("day".into(), rhai::Dynamic::from(gt.day as i64));
                map.insert("month".into(), rhai::Dynamic::from(gt.month as i64));
                map.insert("year".into(), rhai::Dynamic::from(gt.year as i64));
                map.insert(
                    "season".into(),
                    rhai::Dynamic::from(gt.get_season().to_string().to_lowercase()),
                );
                map.insert(
                    "time_of_day".into(),
                    rhai::Dynamic::from(format!("{}", gt.get_time_of_day())),
                );
                map.insert(
                    "weather".into(),
                    rhai::Dynamic::from(format!("{:?}", gt.weather).to_lowercase()),
                );
                map.insert(
                    "temperature".into(),
                    rhai::Dynamic::from(gt.calculate_effective_temperature() as i64),
                );
                // Human-readable descriptions
                map.insert(
                    "time_of_day_desc".into(),
                    rhai::Dynamic::from(format!("{}", gt.get_time_of_day())),
                );
                map.insert("weather_desc".into(), rhai::Dynamic::from(format!("{}", gt.weather)));
                map.insert(
                    "temperature_desc".into(),
                    rhai::Dynamic::from(format!("{}", gt.get_temperature_category())),
                );
                map.insert("is_daytime".into(), rhai::Dynamic::from(gt.is_daytime()));
                rhai::Dynamic::from(map)
            }
            Err(_) => rhai::Dynamic::UNIT,
        }
    });

    // get_current_season() -> Current season as string (spring, summer, autumn, winter)
    let cloned_db = db.clone();
    engine.register_fn("get_current_season", move || -> String {
        match cloned_db.get_game_time() {
            Ok(gt) => gt.get_season().to_string().to_lowercase(),
            Err(_) => "spring".to_string(),
        }
    });

    // admin_set_time(hour, day, month, year) -> Map with changes and status
    // Sets the game time and returns what changed (for triggering purposes)
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("admin_set_time", move |hour: i64, day: i64, month: i64, year: i64| {
        let mut result = rhai::Map::new();
        result.insert("success".into(), rhai::Dynamic::from(false));

        // Validate inputs
        if hour < 0 || hour > 23 {
            result.insert("error".into(), rhai::Dynamic::from("Hour must be 0-23"));
            return rhai::Dynamic::from(result);
        }
        if day < 1 || day > 30 {
            result.insert("error".into(), rhai::Dynamic::from("Day must be 1-30"));
            return rhai::Dynamic::from(result);
        }
        if month < 1 || month > 12 {
            result.insert("error".into(), rhai::Dynamic::from("Month must be 1-12"));
            return rhai::Dynamic::from(result);
        }
        if year < 1 {
            result.insert("error".into(), rhai::Dynamic::from("Year must be >= 1"));
            return rhai::Dynamic::from(result);
        }

        // Get current time for comparison
        let old_time = match cloned_db.get_game_time() {
            Ok(t) => t,
            Err(e) => {
                result.insert(
                    "error".into(),
                    rhai::Dynamic::from(format!("Failed to get time: {}", e)),
                );
                return rhai::Dynamic::from(result);
            }
        };

        let old_time_of_day = old_time.get_time_of_day();
        let old_season = old_time.get_season();
        let old_month = old_time.month;

        // Create new time
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut new_time = old_time.clone();
        new_time.hour = hour as u8;
        new_time.day = day as u8;
        new_time.month = month as u8;
        new_time.year = year as u32;
        new_time.last_time_tick = now;

        let new_time_of_day = new_time.get_time_of_day();
        let new_season = new_time.get_season();

        // Save new time
        if let Err(e) = cloned_db.save_game_time(&new_time) {
            result.insert(
                "error".into(),
                rhai::Dynamic::from(format!("Failed to save time: {}", e)),
            );
            return rhai::Dynamic::from(result);
        }

        // Determine what changed
        let time_changed = old_time_of_day != new_time_of_day;
        let season_changed = old_season != new_season;
        let month_changed = old_month != new_time.month;

        result.insert("success".into(), rhai::Dynamic::from(true));
        result.insert("time_changed".into(), rhai::Dynamic::from(time_changed));
        result.insert("season_changed".into(), rhai::Dynamic::from(season_changed));
        result.insert("month_changed".into(), rhai::Dynamic::from(month_changed));
        result.insert(
            "old_time_of_day".into(),
            rhai::Dynamic::from(format!("{}", old_time_of_day)),
        );
        result.insert(
            "new_time_of_day".into(),
            rhai::Dynamic::from(format!("{}", new_time_of_day)),
        );
        result.insert(
            "old_season".into(),
            rhai::Dynamic::from(old_season.to_string().to_lowercase()),
        );
        result.insert(
            "new_season".into(),
            rhai::Dynamic::from(new_season.to_string().to_lowercase()),
        );
        result.insert("old_month".into(), rhai::Dynamic::from(old_month as i64));
        result.insert("new_month".into(), rhai::Dynamic::from(new_time.month as i64));

        // Broadcast messages for changes
        if time_changed {
            let msg = get_time_transition_message(&new_time_of_day);
            broadcast_to_outdoor_players(&cloned_db, &conns, &format!("\n{}\n", msg));
        }
        if season_changed {
            let msg = get_season_transition_message(&new_season);
            broadcast_to_all_players(&conns, &format!("\n{}\n", msg));
        }

        rhai::Dynamic::from(result)
    });

    // fire_time_triggers(trigger_type, context_map) -> fires environmental triggers
    // trigger_type: "time_change", "season_change", "month_change", "weather_change"
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("fire_time_triggers", move |trigger_type: String, context: rhai::Map| {
        use crate::TriggerType;

        let tt = match trigger_type.as_str() {
            "time_change" => TriggerType::OnTimeChange,
            "season_change" => TriggerType::OnSeasonChange,
            "month_change" => TriggerType::OnMonthChange,
            "weather_change" => TriggerType::OnWeatherChange,
            _ => return false,
        };

        // Convert rhai::Map to HashMap<String, String>
        let mut ctx: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (key, val) in context.iter() {
            ctx.insert(key.to_string(), val.to_string());
        }

        // Fire triggers
        fire_environmental_triggers_impl(&cloned_db, &conns, tt, &ctx)
    });

    // ========== Thirst Functions ==========

    // get_character_thirst(connection_id) -> Map with thirst/max_thirst/percent
    let conns = connections.clone();
    engine.register_fn("get_character_thirst", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    let mut map = rhai::Map::new();
                    map.insert("thirst".into(), rhai::Dynamic::from(char.thirst as i64));
                    map.insert("max_thirst".into(), rhai::Dynamic::from(char.max_thirst as i64));
                    let pct = if char.max_thirst > 0 {
                        (char.thirst * 100) / char.max_thirst
                    } else {
                        0
                    };
                    map.insert("percent".into(), rhai::Dynamic::from(pct as i64));
                    return rhai::Dynamic::from(map);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // restore_thirst(connection_id, amount) -> bool - Restore thirst (e.g., after drinking)
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("restore_thirst", move |connection_id: String, amount: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get_mut(&uuid) {
                if let Some(ref mut char) = session.character {
                    let old_thirst = char.thirst;
                    char.thirst = (char.thirst + amount as i32).min(char.max_thirst);
                    if char.thirst != old_thirst {
                        let _ = cloned_db.save_character_data(char.clone());
                    }
                    return true;
                }
            }
        }
        false
    });

    // ========== Hunger Functions ==========

    // get_character_hunger(connection_id) -> Map with hunger/max_hunger/percent
    let conns = connections.clone();
    engine.register_fn("get_character_hunger", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    let mut map = rhai::Map::new();
                    map.insert("current".into(), rhai::Dynamic::from(char.hunger as i64));
                    map.insert("max".into(), rhai::Dynamic::from(char.max_hunger as i64));
                    let pct = if char.max_hunger > 0 {
                        (char.hunger * 100) / char.max_hunger
                    } else {
                        0
                    };
                    map.insert("percent".into(), rhai::Dynamic::from(pct as i64));
                    return rhai::Dynamic::from(map);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // restore_hunger(connection_id, amount) -> bool - Restore hunger (e.g., after eating)
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("restore_hunger", move |connection_id: String, amount: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get_mut(&uuid) {
                if let Some(ref mut char) = session.character {
                    let old_hunger = char.hunger;
                    char.hunger = (char.hunger + amount as i32).min(char.max_hunger);
                    if char.hunger != old_hunger {
                        let _ = cloned_db.save_character_data(char.clone());
                    }
                    return true;
                }
            }
        }
        false
    });

    // get_character_insulation(connection_id) -> i64 - Sum of equipped item insulation
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("get_character_insulation", move |connection_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    let mut total = 0i64;
                    // Query database for equipped items (source of truth is ItemLocation::Equipped)
                    if let Ok(equipped_items) = cloned_db.get_equipped_items(&char.name) {
                        for item in equipped_items {
                            total += item.insulation as i64;
                        }
                    }
                    return total.min(100);
                }
            }
        }
        0
    });

    // set_item_insulation(item_id, value) -> bool - Set item insulation (for oedit)
    let cloned_db = db.clone();
    engine.register_fn("set_item_insulation", move |item_id: String, insulation: i64| {
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut item = match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(i)) => i,
            _ => return false,
        };
        item.insulation = insulation as i32;
        cloned_db.save_item_data(item).is_ok()
    });

    // has_trait(connection_id, trait_id) -> bool - Check if character has a specific trait
    // Also checks race definition's granted_traits
    let conns = connections.clone();
    let state_clone = state.clone();
    engine.register_fn("has_trait", move |connection_id: String, trait_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    // Check character's own traits
                    if char.traits.iter().any(|t| t == &trait_id) {
                        return true;
                    }
                    // Check race definition's granted traits
                    let race_id = char.race.to_lowercase();
                    drop(conns_guard);
                    let world = state_clone.lock().unwrap();
                    if let Some(race) = world.race_definitions.get(&race_id) {
                        return race.granted_traits.iter().any(|t| t == &trait_id);
                    }
                    return false;
                }
            }
        }
        false
    });

    // ========== Heat/Thirst Stamina Penalty Functions ==========

    // get_heat_stamina_penalty(connection_id) -> i64 - Returns 0, 1, or 2 based on heat
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("get_heat_stamina_penalty", move |connection_id: String| -> i64 {
        let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let conns_guard = conns.lock().unwrap();
        let session = match conns_guard.get(&conn_uuid) {
            Some(s) => s,
            None => return 0,
        };
        let char = match &session.character {
            Some(c) => c,
            None => return 0,
        };

        // God mode bypasses all stamina penalties
        if char.god_mode {
            return 0;
        }

        // Check for desert_born trait (immune to heat penalty)
        if char.traits.iter().any(|t| t == "desert_born") {
            return 0;
        }

        // Get room data
        let room = match cloned_db.get_room_data(&char.current_room_id) {
            Ok(Some(r)) => r,
            _ => return 0,
        };

        // Climate controlled rooms have no heat penalty (check area inheritance)
        let is_climate_controlled = room.flags.climate_controlled
            || room
                .area_id
                .and_then(|aid| cloned_db.get_area_data(&aid).ok().flatten())
                .map(|area| area.flags.climate_controlled)
                .unwrap_or(false);
        if is_climate_controlled {
            return 0;
        }

        // Get base temperature
        let game_time = match cloned_db.get_game_time() {
            Ok(gt) => gt,
            Err(_) => return 0,
        };

        let mut effective_temp = game_time.calculate_effective_temperature();

        // Room overrides
        if room.flags.always_hot {
            effective_temp = 35; // Sweltering
        } else if room.flags.always_cold {
            return 0; // No heat penalty in cold rooms
        } else if room.flags.indoors {
            // Indoors caps at 25°C (no heat penalty normally)
            effective_temp = effective_temp.min(25);
        }

        // High insulation adds one heat tier
        let mut insulation_bonus = 0i32;
        if let Ok(equipped) = cloned_db.get_equipped_items(&char.name) {
            let total_insulation: i32 = equipped.iter().map(|i| i.insulation).sum();
            if total_insulation >= 75 {
                insulation_bonus = 10; // Adds one tier (effectively +10°C)
            }
        }
        effective_temp += insulation_bonus;

        // Calculate base penalty
        let mut penalty: i64 = if effective_temp >= 35 {
            2 // Sweltering
        } else if effective_temp >= 25 {
            1 // Hot
        } else {
            0
        };

        // Apply trait modifiers
        if char.traits.iter().any(|t| t == "heat_sensitive") {
            penalty *= 2; // Double penalty
        }
        if char.traits.iter().any(|t| t == "thick_skinned") && penalty > 0 {
            penalty -= 1; // Reduce by 1, min 0
        }

        penalty
    });

    // get_thirst_stamina_penalty(connection_id) -> i64 - Returns 0-3 based on hydration
    let conns = connections.clone();
    engine.register_fn("get_thirst_stamina_penalty", move |connection_id: String| -> i64 {
        let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let conns_guard = conns.lock().unwrap();
        let session = match conns_guard.get(&conn_uuid) {
            Some(s) => s,
            None => return 0,
        };
        let char = match &session.character {
            Some(c) => c,
            None => return 0,
        };

        // God mode bypasses all stamina penalties
        if char.god_mode {
            return 0;
        }

        // Calculate thirst percentage
        if char.max_thirst == 0 {
            return 0;
        }
        let percent = (char.thirst * 100) / char.max_thirst;

        // Return penalty based on hydration level
        if percent < 10 {
            3 // Dehydrated
        } else if percent < 25 {
            2 // Very thirsty
        } else if percent < 50 {
            1 // Thirsty
        } else {
            0 // Well hydrated
        }
    });

    // get_activity_stamina_penalty(connection_id) -> i64 - Combined heat + thirst penalty
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("get_activity_stamina_penalty", move |connection_id: String| -> i64 {
        let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return 0,
        };

        let conns_guard = conns.lock().unwrap();
        let session = match conns_guard.get(&conn_uuid) {
            Some(s) => s,
            None => return 0,
        };
        let char = match &session.character {
            Some(c) => c,
            None => return 0,
        };

        // God mode bypasses all stamina penalties
        if char.god_mode {
            return 0;
        }

        let mut total_penalty: i64 = 0;

        // Calculate heat penalty
        if !char.traits.iter().any(|t| t == "desert_born") {
            if let Ok(Some(room)) = cloned_db.get_room_data(&char.current_room_id) {
                // Check area inheritance for climate_controlled
                let is_climate_controlled = room.flags.climate_controlled
                    || room
                        .area_id
                        .and_then(|aid| cloned_db.get_area_data(&aid).ok().flatten())
                        .map(|area| area.flags.climate_controlled)
                        .unwrap_or(false);
                if !is_climate_controlled {
                    if let Ok(game_time) = cloned_db.get_game_time() {
                        let mut effective_temp = game_time.calculate_effective_temperature();

                        if room.flags.always_hot {
                            effective_temp = 35;
                        } else if room.flags.always_cold {
                            effective_temp = 0;
                        } else if room.flags.indoors {
                            effective_temp = effective_temp.min(25);
                        }

                        // High insulation adds heat
                        if let Ok(equipped) = cloned_db.get_equipped_items(&char.name) {
                            let total_insulation: i32 = equipped.iter().map(|i| i.insulation).sum();
                            if total_insulation >= 75 {
                                effective_temp += 10;
                            }
                        }

                        let mut heat_penalty: i64 = if effective_temp >= 35 {
                            2
                        } else if effective_temp >= 25 {
                            1
                        } else {
                            0
                        };

                        if char.traits.iter().any(|t| t == "heat_sensitive") {
                            heat_penalty *= 2;
                        }
                        if char.traits.iter().any(|t| t == "thick_skinned") && heat_penalty > 0 {
                            heat_penalty -= 1;
                        }

                        total_penalty += heat_penalty;
                    }
                }
            }
        }

        // Calculate thirst penalty
        if char.max_thirst > 0 {
            let percent = (char.thirst * 100) / char.max_thirst;
            let thirst_penalty = if percent < 10 {
                3
            } else if percent < 25 {
                2
            } else if percent < 50 {
                1
            } else {
                0
            };
            total_penalty += thirst_penalty;
        }

        total_penalty
    });

    // ========== Darkness/Vision Functions ==========

    // has_light_source(connection_id) -> bool - Check if character has an equipped light source
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("has_light_source", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    if let Ok(equipped) = cloned_db.get_equipped_items(&char.name) {
                        return equipped.iter().any(|item| item.flags.provides_light);
                    }
                }
            }
        }
        false
    });

    // can_see(connection_id) -> bool - Check if character is not blind
    let conns = connections.clone();
    engine.register_fn("can_see", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    // Check if they have the blindness trait
                    return !char.traits.iter().any(|t| t == "blindness");
                }
            }
        }
        true // Default to can see if no character found
    });

    // is_room_dark(room_id, connection_id) -> bool - Check if room is effectively dark for character
    // Takes into account: room dark flag, time of day, city flag, night_vision, light sources, racial traits
    let conns = connections.clone();
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("is_room_dark", move |room_id: String, connection_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return false,
        };

        // Determine if room is inherently dark
        let is_dark = if room.flags.dark {
            true // Always dark rooms (caves, dungeons)
        } else if !room.flags.indoors && !room.flags.city {
            // Outdoor non-city: dark at night
            cloned_db.get_game_time().map(|gt| !gt.is_daytime()).unwrap_or(false)
        } else {
            false // Indoor or city rooms are lit
        };

        if !is_dark {
            return false;
        }

        // Check character's ability to see in darkness
        if let Ok(conn_uuid) = uuid::Uuid::parse_str(&connection_id) {
            let (god_mode, has_nv_trait, has_da_trait, race_id, char_name) = {
                let conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get(&conn_uuid) {
                    if let Some(ref char) = session.character {
                        (
                            char.god_mode,
                            char.traits.iter().any(|t| t == "night_vision"),
                            char.traits.iter().any(|t| t == "dark_adapted"),
                            char.race.to_lowercase(),
                            char.name.clone(),
                        )
                    } else {
                        return true;
                    }
                } else {
                    return true;
                }
            };

            // God mode can see in darkness
            if god_mode {
                return false;
            }

            // Check for night vision trait (own or racial)
            let has_racial_nv;
            let has_racial_da;
            {
                let world = state_clone.lock().unwrap();
                if let Some(race) = world.race_definitions.get(&race_id) {
                    has_racial_nv = race.granted_traits.iter().any(|t| t == "night_vision");
                    has_racial_da = race.granted_traits.iter().any(|t| t == "dark_adapted");
                } else {
                    has_racial_nv = false;
                    has_racial_da = false;
                }
            }

            if has_nv_trait || has_racial_nv {
                return false;
            }

            // Check for dark_adapted trait (helps in dim/dark but not pitch dark)
            if !room.flags.dark && (has_da_trait || has_racial_da) {
                return false;
            }

            // Check for light source
            if let Ok(equipped) = cloned_db.get_equipped_items(&char_name) {
                if equipped.iter().any(|item| item.flags.provides_light) {
                    return false;
                }
            }
        }

        true // Room is dark for this character
    });

    // get_lighting_level(room_id) -> String
    // Returns lighting condition: "bright", "normal", "dim", "dark", or "pitch"
    let cloned_db = db.clone();
    engine.register_fn("get_lighting_level", move |room_id: String| -> String {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return "normal".to_string(),
        };

        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return "normal".to_string(),
        };

        // Pitch dark: rooms with dark flag
        if room.flags.dark {
            return "pitch".to_string();
        }

        // Indoor/city rooms are normal lighting
        if room.flags.indoors || room.flags.city {
            return "normal".to_string();
        }

        // Outdoor rooms - check time and weather
        let game_time = match cloned_db.get_game_time() {
            Ok(gt) => gt,
            Err(_) => return "normal".to_string(),
        };

        let hour = game_time.hour;

        // Check weather condition
        let is_overcast = matches!(
            game_time.weather,
            crate::WeatherCondition::Overcast
                | crate::WeatherCondition::HeavyRain
                | crate::WeatherCondition::Thunderstorm
                | crate::WeatherCondition::Blizzard
                | crate::WeatherCondition::Fog
        );

        let is_cloudy = matches!(
            game_time.weather,
            crate::WeatherCondition::Cloudy
                | crate::WeatherCondition::LightRain
                | crate::WeatherCondition::Rain
                | crate::WeatherCondition::LightSnow
                | crate::WeatherCondition::Snow
        );

        // Night time (hours 20-4): dark
        if hour >= 20 || hour < 5 {
            return "dark".to_string();
        }

        // Dawn (hours 5-6) and dusk (hours 17-19): dim
        if hour < 7 || hour >= 17 {
            return "dim".to_string();
        }

        // Daytime (hours 7-16)
        if is_overcast {
            return "dim".to_string();
        }

        if is_cloudy {
            return "normal".to_string();
        }

        // Clear weather during day
        "bright".to_string()
    });

    // get_vision_penalty(room_id, connection_id) -> i64
    // Returns vision penalty 0-100 based on lighting and character traits
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "get_vision_penalty",
        move |room_id: String, connection_id: String| -> i64 {
            // Get room
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };

            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return 0,
            };

            // Get character
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return 0,
            };

            let conns_guard = conns.lock().unwrap();
            let session = match conns_guard.get(&conn_uuid) {
                Some(s) => s,
                None => return 0,
            };

            let char = match &session.character {
                Some(c) => c,
                None => return 0,
            };

            // God mode has no penalties
            if char.god_mode {
                return 0;
            }

            // Check blindness trait first - severe penalty
            if char.traits.iter().any(|t| t == "blindness") {
                return 100;
            }

            // Calculate lighting level
            let lighting = {
                if room.flags.dark {
                    "pitch"
                } else if room.flags.indoors || room.flags.city {
                    "normal"
                } else {
                    let game_time = match cloned_db.get_game_time() {
                        Ok(gt) => gt,
                        Err(_) => return 0,
                    };

                    let hour = game_time.hour;

                    let is_overcast = matches!(
                        game_time.weather,
                        crate::WeatherCondition::Overcast
                            | crate::WeatherCondition::HeavyRain
                            | crate::WeatherCondition::Thunderstorm
                            | crate::WeatherCondition::Blizzard
                            | crate::WeatherCondition::Fog
                    );

                    let is_cloudy = matches!(
                        game_time.weather,
                        crate::WeatherCondition::Cloudy
                            | crate::WeatherCondition::LightRain
                            | crate::WeatherCondition::Rain
                            | crate::WeatherCondition::LightSnow
                            | crate::WeatherCondition::Snow
                    );

                    if hour >= 20 || hour < 5 {
                        "dark"
                    } else if hour < 7 || hour >= 17 {
                        "dim"
                    } else if is_overcast {
                        "dim"
                    } else if is_cloudy {
                        "normal"
                    } else {
                        "bright"
                    }
                }
            };

            // Check traits
            let has_night_vision = char.traits.iter().any(|t| t == "night_vision");
            let has_dark_adapted = char.traits.iter().any(|t| t == "dark_adapted");
            let has_night_blind = char.traits.iter().any(|t| t == "night_blind");
            let has_light_sensitive = char.traits.iter().any(|t| t == "light_sensitive");

            // Check for light source
            let has_light = cloned_db
                .get_equipped_items(&char.name)
                .map(|items| items.iter().any(|item| item.flags.provides_light))
                .unwrap_or(false);

            // Calculate lighting penalty
            let lighting_penalty: i64 = match lighting {
                "bright" => {
                    if has_light_sensitive {
                        let has_glare_protection = cloned_db
                            .get_equipped_items(&char.name)
                            .map(|items| items.iter().any(|item| item.flags.reduces_glare))
                            .unwrap_or(false);
                        if has_glare_protection { 0 } else { 30 }
                    } else {
                        0
                    }
                }
                "normal" => 0,
                "dim" => {
                    if has_night_vision || has_light || has_dark_adapted {
                        0
                    } else if has_night_blind {
                        50
                    } else {
                        0
                    }
                }
                "dark" => {
                    if has_night_vision || has_light {
                        0
                    } else if has_night_blind {
                        50
                    } else if has_dark_adapted {
                        20
                    } else {
                        50
                    }
                }
                "pitch" => {
                    if has_night_vision || has_light {
                        0
                    } else if has_night_blind {
                        50
                    } else if has_dark_adapted {
                        40
                    } else {
                        50
                    }
                }
                _ => 0,
            };

            // Eye wound penalty
            let eye_penalty: i64 = {
                let left = char
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::LeftEye)
                    .map(|w| w.level.penalty() as i64)
                    .max()
                    .unwrap_or(0);
                let right = char
                    .wounds
                    .iter()
                    .filter(|w| w.body_part == BodyPart::RightEye)
                    .map(|w| w.level.penalty() as i64)
                    .max()
                    .unwrap_or(0);
                if left > 0 && right > 0 {
                    (left + right).min(95)
                } else {
                    std::cmp::max(left, right) / 2
                }
            };

            (lighting_penalty + eye_penalty).min(95)
        },
    );

    // ========== Gold Functions ==========

    // get_character_gold(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_character_gold", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => char.gold as i64,
            _ => 0,
        }
    });

    // set_character_gold(char_name, gold) -> bool
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("set_character_gold", move |char_name: String, gold: i64| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                char.gold = gold as i32;
                let new_gold = char.gold;
                let saved = cloned_db.save_character_data(char).is_ok();
                if saved {
                    // Sync gold to session cache to prevent stale overwrites by ticks
                    if let Ok(mut conns) = cloned_conns.lock() {
                        for (_, session) in conns.iter_mut() {
                            if let Some(ref mut sc) = session.character {
                                if sc.name.to_lowercase() == char_name.to_lowercase() {
                                    sc.gold = new_gold;
                                    break;
                                }
                            }
                        }
                    }
                }
                saved
            }
            _ => false,
        }
    });

    // add_character_gold(char_name, amount) -> bool (returns false if would go negative)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn("add_character_gold", move |char_name: String, amount: i64| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                let new_gold = char.gold as i64 + amount;
                if new_gold < 0 {
                    return false;
                }
                char.gold = new_gold as i32;
                let final_gold = char.gold;
                let saved = cloned_db.save_character_data(char).is_ok();
                if saved {
                    // Sync gold to session cache to prevent stale overwrites by ticks
                    if let Ok(mut conns) = cloned_conns.lock() {
                        for (_, session) in conns.iter_mut() {
                            if let Some(ref mut sc) = session.character {
                                if sc.name.to_lowercase() == char_name.to_lowercase() {
                                    sc.gold = final_gold;
                                    break;
                                }
                            }
                        }
                    }
                }
                saved
            }
            _ => false,
        }
    });

    // ========== Bank Functions ==========

    // get_bank_gold(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_bank_gold", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => char.bank_gold,
            _ => 0,
        }
    });

    // set_bank_gold(char_name, amount) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_bank_gold", move |char_name: String, amount: i64| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                char.bank_gold = amount;
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // add_bank_gold(char_name, amount) -> bool (returns false if would go negative)
    let cloned_db = db.clone();
    engine.register_fn("add_bank_gold", move |char_name: String, amount: i64| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                let new_gold = char.bank_gold + amount;
                if new_gold < 0 {
                    return false;
                }
                char.bank_gold = new_gold;
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // transfer_to_bank(char_name, amount) -> bool (moves gold from pocket to bank)
    let cloned_db = db.clone();
    engine.register_fn("transfer_to_bank", move |char_name: String, amount: i64| -> bool {
        if amount <= 0 {
            return false;
        }
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                if (char.gold as i64) < amount {
                    return false;
                }
                char.gold -= amount as i32;
                char.bank_gold += amount;
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // transfer_from_bank(char_name, amount) -> bool (moves gold from bank to pocket)
    let cloned_db = db.clone();
    engine.register_fn("transfer_from_bank", move |char_name: String, amount: i64| -> bool {
        if amount <= 0 {
            return false;
        }
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                if char.bank_gold < amount {
                    return false;
                }
                char.bank_gold -= amount;
                char.gold += amount as i32;
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // ========== Skill System Functions ==========

    // Helper function to get XP required for a level
    fn xp_for_level(level: i32) -> i32 {
        match level {
            0 => 100,  // 0->1
            1 => 200,  // 1->2
            2 => 350,  // 2->3
            3 => 550,  // 3->4
            4 => 800,  // 4->5
            5 => 1100, // 5->6
            6 => 1500, // 6->7
            7 => 2000, // 7->8
            8 => 2600, // 8->9
            9 => 3300, // 9->10
            _ => 0,    // Max level
        }
    }

    // get_xp_for_level(level) -> i64
    engine.register_fn("get_xp_for_level", move |level: i64| -> i64 {
        xp_for_level(level as i32) as i64
    });

    // get_skill_level(char_name, skill_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_skill_level", move |char_name: String, skill_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => char
                .skills
                .get(&skill_name.to_lowercase())
                .map(|s| s.level as i64)
                .unwrap_or(0),
            _ => 0,
        }
    });

    // get_skill_experience(char_name, skill_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn(
        "get_skill_experience",
        move |char_name: String, skill_name: String| -> i64 {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(char)) => char
                    .skills
                    .get(&skill_name.to_lowercase())
                    .map(|s| s.experience as i64)
                    .unwrap_or(0),
                _ => 0,
            }
        },
    );

    // get_all_skills(char_name) -> Map of skill_name -> {level, experience}
    let cloned_db = db.clone();
    engine.register_fn("get_all_skills", move |char_name: String| -> rhai::Map {
        let mut result = rhai::Map::new();
        if let Ok(Some(char)) = cloned_db.get_character_data(&char_name.to_lowercase()) {
            for (name, progress) in &char.skills {
                let mut skill_map = rhai::Map::new();
                skill_map.insert("level".into(), rhai::Dynamic::from(progress.level as i64));
                skill_map.insert("experience".into(), rhai::Dynamic::from(progress.experience as i64));
                skill_map.insert(
                    "xp_to_level".into(),
                    rhai::Dynamic::from(xp_for_level(progress.level) as i64),
                );
                result.insert(name.clone().into(), rhai::Dynamic::from(skill_map));
            }
        }
        result
    });

    // set_skill_level(char_name, skill_name, level) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_skill_level",
        move |char_name: String, skill_name: String, level: i64| -> bool {
            let level = level.clamp(0, 10) as i32;
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(mut char)) => {
                    let skill_key = skill_name.to_lowercase();
                    let entry = char.skills.entry(skill_key).or_insert(crate::SkillProgress::default());
                    entry.level = level;
                    entry.experience = 0; // Reset XP when setting level directly
                    cloned_db.save_character_data(char).is_ok()
                }
                _ => false,
            }
        },
    );

    // add_skill_experience(char_name, skill_name, amount) -> bool (returns true if leveled up)
    let cloned_db = db.clone();
    engine.register_fn(
        "add_skill_experience",
        move |char_name: String, skill_name: String, amount: i64| -> bool {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(mut char)) => {
                    let skill_key = skill_name.to_lowercase();
                    let entry = char.skills.entry(skill_key).or_insert(crate::SkillProgress::default());

                    // Don't add XP if already at max level
                    if entry.level >= 10 {
                        let _ = cloned_db.save_character_data(char);
                        return false;
                    }

                    // Apply skill XP trait modifiers
                    let has_prodigy = char.traits.iter().any(|t| t == "prodigy");
                    let has_quick_study = char.traits.iter().any(|t| t == "quick_study");
                    let has_slow_learner = char.traits.iter().any(|t| t == "slow_learner");
                    let mut xp = amount as i32;
                    if has_prodigy {
                        xp = xp * 150 / 100;
                    }
                    // +50%
                    else if has_quick_study {
                        xp = xp * 125 / 100;
                    } // +25%
                    if has_slow_learner {
                        xp = xp * 65 / 100;
                    } // -35%
                    xp = xp.max(1);
                    entry.experience += xp;

                    let mut leveled_up = false;
                    loop {
                        let xp_needed = xp_for_level(entry.level);
                        if xp_needed == 0 || entry.experience < xp_needed || entry.level >= 10 {
                            break;
                        }
                        entry.experience -= xp_needed;
                        entry.level += 1;
                        leveled_up = true;
                        if entry.level >= 10 {
                            entry.experience = 0; // No XP overflow at max
                            break;
                        }
                    }

                    let _ = cloned_db.save_character_data(char);
                    leveled_up
                }
                _ => false,
            }
        },
    );

    // ========== Trait Effect Sum Function ==========

    // get_trait_effect_sum(char_name, effect_key) -> i64
    // Sums the given effect key across all of a character's traits using trait_definitions
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn(
        "get_trait_effect_sum",
        move |char_name: String, effect_key: String| -> i64 {
            let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return 0,
            };
            let world = state_clone.lock().unwrap();
            char.traits
                .iter()
                .filter_map(|t| world.trait_definitions.get(t.as_str()))
                .filter_map(|def| def.effects.get(&effect_key))
                .copied()
                .sum::<i32>() as i64
        },
    );

    // ========== Forage Cooldown Functions ==========

    // get_forage_cooldown(char_name, room_id) -> i64 (seconds remaining, 0 = can forage)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_forage_cooldown",
        move |char_name: String, room_id: String| -> i64 {
            let cooldown_duration = 60; // 60 second cooldown per room
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(char)) => {
                    if let Some(timestamp) = char.foraged_rooms.get(&room_id) {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let elapsed = now - timestamp;
                        if elapsed < cooldown_duration {
                            return cooldown_duration - elapsed;
                        }
                    }
                    0 // No cooldown or cooldown expired
                }
                _ => 0,
            }
        },
    );

    // set_forage_timestamp(char_name, room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_forage_timestamp",
        move |char_name: String, room_id: String| -> bool {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(mut char)) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    char.foraged_rooms.insert(room_id, now);
                    cloned_db.save_character_data(char).is_ok()
                }
                _ => false,
            }
        },
    );

    // clear_forage_history(char_name) -> bool - Clear all forage timestamps
    let cloned_db = db.clone();
    engine.register_fn("clear_forage_history", move |char_name: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                char.foraged_rooms.clear();
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // clear_expired_forage_cooldowns(char_name) -> bool
    // Removes forage entries older than 1 hour
    let cloned_db = db.clone();
    engine.register_fn("clear_expired_forage_cooldowns", move |char_name: String| -> bool {
        let one_hour = 3600; // 1 hour in seconds
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let orig_len = char.foraged_rooms.len();
                char.foraged_rooms.retain(|_, timestamp| now - *timestamp < one_hour);
                if char.foraged_rooms.len() < orig_len {
                    cloned_db.save_character_data(char).is_ok()
                } else {
                    true // Nothing to clean
                }
            }
            _ => false,
        }
    });

    // ========== Secure Credential Functions ==========

    // change_password(connection_id, new_password) -> bool
    // Securely changes the logged-in user's password without exposing the hash to scripts
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "change_password",
        move |connection_id: String, new_password: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get_mut(&uuid) {
                    if let Some(ref mut character) = session.character {
                        match cloned_db.hash_password(&new_password) {
                            Ok(hash) => {
                                character.password_hash = hash;
                                character.must_change_password = false;
                                return cloned_db.save_character_data(character.clone()).is_ok();
                            }
                            Err(e) => {
                                tracing::error!("Failed to hash password: {}", e);
                                return false;
                            }
                        }
                    }
                }
            }
            false
        },
    );

    // set_first_character_privileges(character_name) -> bool
    // Grants admin+builder to a character only if they are the first character in the database.
    // Used during character creation; cannot be abused since it checks character count.
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "set_first_character_privileges",
        move |character_name: String| -> bool {
            let count = cloned_db.count_characters().unwrap_or(0);
            // Only grant if this is the very first character (count == 1 means we just created it)
            if count != 1 {
                return false;
            }
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&character_name) {
                character.is_admin = true;
                character.is_builder = true;
                if let Err(e) = cloned_db.save_character_data(character.clone()) {
                    tracing::error!("Failed to save first character privileges: {}", e);
                    return false;
                }
                // Update session if online
                let mut conns = conns.lock().unwrap();
                for (_id, session) in conns.iter_mut() {
                    if let Some(ref mut session_char) = session.character {
                        if session_char.name.eq_ignore_ascii_case(&character_name) {
                            session_char.is_admin = true;
                            session_char.is_builder = true;
                            break;
                        }
                    }
                }
                true
            } else {
                false
            }
        },
    );

    // ========== Security Logging Functions ==========

    // log_security_event(message) - Log security events to server console
    engine.register_fn("log_security_event", |message: String| {
        tracing::warn!("[SECURITY] {}", message);
    });

    // ========== Admin User Management Functions ==========

    // list_all_characters() -> Array of CharacterData
    let cloned_db = db.clone();
    engine.register_fn("list_all_characters", move || -> rhai::Array {
        match cloned_db.list_all_characters() {
            Ok(chars) => chars.into_iter().map(rhai::Dynamic::from).collect(),
            Err(_) => rhai::Array::new(),
        }
    });

    // ========== Character Stats Functions ==========

    // get_player_stat(connection_id, stat_name) -> i64 - Get a player's stat value
    // stat_name: str, dex, con, int, wis, cha
    let conns = connections.clone();
    engine.register_fn(
        "get_player_stat",
        move |connection_id: String, stat_name: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get(&uuid) {
                    if let Some(ref char) = session.character {
                        return match stat_name.to_lowercase().as_str() {
                            "str" | "strength" => char.stat_str as i64,
                            "dex" | "dexterity" => char.stat_dex as i64,
                            "con" | "constitution" => char.stat_con as i64,
                            "int" | "intelligence" => char.stat_int as i64,
                            "wis" | "wisdom" => char.stat_wis as i64,
                            "cha" | "charisma" => char.stat_cha as i64,
                            _ => 0,
                        };
                    }
                }
            }
            0
        },
    );

    // set_player_stat(connection_id, stat_name, value) -> bool
    // Updates both session and database
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "set_player_stat",
        move |connection_id: String, stat_name: String, value: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get_mut(&uuid) {
                    if let Some(ref mut char) = session.character {
                        let stat_val = value as i32;
                        match stat_name.to_lowercase().as_str() {
                            "str" | "strength" => char.stat_str = stat_val,
                            "dex" | "dexterity" => char.stat_dex = stat_val,
                            "con" | "constitution" => char.stat_con = stat_val,
                            "int" | "intelligence" => char.stat_int = stat_val,
                            "wis" | "wisdom" => char.stat_wis = stat_val,
                            "cha" | "charisma" => char.stat_cha = stat_val,
                            _ => return false,
                        };
                        return cloned_db.save_character_data(char.clone()).is_ok();
                    }
                }
            }
            false
        },
    );

    // get_player_max_carry_weight(connection_id) -> i64
    // Formula: 50 + (STR * 10)
    let conns = connections.clone();
    engine.register_fn("get_player_max_carry_weight", move |connection_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    return 50 + (char.stat_str as i64 * 10);
                }
            }
        }
        150 // Default for STR 10
    });

    // get_player_calculated_max_health(connection_id) -> i64
    // Formula: 20 + (CON * 5) + (level * 5)
    let conns = connections.clone();
    engine.register_fn(
        "get_player_calculated_max_health",
        move |connection_id: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get(&uuid) {
                    if let Some(ref char) = session.character {
                        return 20 + (char.stat_con as i64 * 5) + (char.level as i64 * 5);
                    }
                }
            }
            75 // Default for CON 10, level 1
        },
    );

    // get_player_calculated_max_stamina(connection_id) -> i64
    // Formula: 50 + (CON * 3) + (STR * 2)
    let conns = connections.clone();
    engine.register_fn(
        "get_player_calculated_max_stamina",
        move |connection_id: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let conns_guard = conns.lock().unwrap();
                if let Some(session) = conns_guard.get(&uuid) {
                    if let Some(ref char) = session.character {
                        return 50 + (char.stat_con as i64 * 3) + (char.stat_str as i64 * 2);
                    }
                }
            }
            100 // Default for CON 10, STR 10
        },
    );

    // ========== Effective Level Functions ==========

    // get_effective_level(char_name) -> i64
    // Sum of all skill levels (used for mail system level requirement)
    let cloned_db = db.clone();
    engine.register_fn("get_effective_level", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => char.skills.values().map(|progress| progress.level as i64).sum(),
            _ => 0,
        }
    });

    // get_combat_skill_names() -> Array
    // Returns the list of combat skill name strings
    engine.register_fn("get_combat_skill_names", || -> rhai::Array {
        vec![
            "unarmed".into(),
            "short_blades".into(),
            "long_blades".into(),
            "short_blunt".into(),
            "long_blunt".into(),
            "polearms".into(),
            "ranged".into(),
            "magic".into(),
        ]
    });

    // get_effective_combat_level(char_name) -> i64
    // Returns the player's highest combat skill level (0-10)
    let cloned_db = db.clone();
    engine.register_fn("get_effective_combat_level", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => {
                let weapon_max = char
                    .skills
                    .iter()
                    .filter_map(|(key, progress)| WeaponSkill::from_str(key).map(|_| progress.level as i64))
                    .max()
                    .unwrap_or(0);
                let magic_level = char.skills.get("magic").map(|s| s.level as i64).unwrap_or(0);
                weapon_max.max(magic_level)
            }
            _ => 0,
        }
    });

    // ========== Buff System Functions ==========

    // apply_buff(char_name, effect_type_str, magnitude, duration_secs, source) -> bool
    // Adds or replaces an ActiveBuff on a character. If same effect_type exists, replaces it.
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "apply_buff",
        move |char_name: String, effect_type_str: String, magnitude: i64, duration_secs: i64, source: String| -> bool {
            let effect_type = match EffectType::from_str(&effect_type_str) {
                Some(et) => et,
                None => return false,
            };
            let buff = ActiveBuff {
                effect_type,
                magnitude: magnitude as i32,
                remaining_secs: duration_secs as i32,
                source,
            };
            let name_lower = char_name.to_lowercase();
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&name_lower) {
                // Replace existing buff of same type, or add new
                if let Some(existing) = character.active_buffs.iter_mut().find(|b| b.effect_type == effect_type) {
                    existing.magnitude = existing.magnitude.max(buff.magnitude);
                    existing.remaining_secs = buff.remaining_secs;
                    existing.source = buff.source.clone();
                } else {
                    character.active_buffs.push(buff);
                }
                if cloned_db.save_character_data(character.clone()).is_err() {
                    return false;
                }
                // Update session
                let mut conns_guard = conns.lock().unwrap();
                for (_id, session) in conns_guard.iter_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.eq_ignore_ascii_case(&char_name) {
                            sc.active_buffs = character.active_buffs;
                            break;
                        }
                    }
                }
                true
            } else {
                false
            }
        },
    );

    // remove_buff(char_name, effect_type_str) -> bool
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "remove_buff",
        move |char_name: String, effect_type_str: String| -> bool {
            let effect_type = match EffectType::from_str(&effect_type_str) {
                Some(et) => et,
                None => return false,
            };
            let name_lower = char_name.to_lowercase();
            if let Ok(Some(mut character)) = cloned_db.get_character_data(&name_lower) {
                let before = character.active_buffs.len();
                character.active_buffs.retain(|b| b.effect_type != effect_type);
                if character.active_buffs.len() == before {
                    return false; // Buff not found
                }
                if cloned_db.save_character_data(character.clone()).is_err() {
                    return false;
                }
                // Update session
                let mut conns_guard = conns.lock().unwrap();
                for (_id, session) in conns_guard.iter_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.eq_ignore_ascii_case(&char_name) {
                            sc.active_buffs = character.active_buffs;
                            break;
                        }
                    }
                }
                true
            } else {
                false
            }
        },
    );

    // has_buff(char_name, effect_type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_buff", move |char_name: String, effect_type_str: String| -> bool {
        let effect_type = match EffectType::from_str(&effect_type_str) {
            Some(et) => et,
            None => return false,
        };
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.active_buffs.iter().any(|b| b.effect_type == effect_type),
            _ => false,
        }
    });

    // get_buff_magnitude(char_name, effect_type_str) -> i64
    let cloned_db = db.clone();
    engine.register_fn(
        "get_buff_magnitude",
        move |char_name: String, effect_type_str: String| -> i64 {
            let effect_type = match EffectType::from_str(&effect_type_str) {
                Some(et) => et,
                None => return 0,
            };
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(character)) => character
                    .active_buffs
                    .iter()
                    .find(|b| b.effect_type == effect_type)
                    .map(|b| b.magnitude as i64)
                    .unwrap_or(0),
                _ => 0,
            }
        },
    );

    // get_active_buffs(char_name) -> Array of maps with effect_type, magnitude, remaining_secs, source
    let cloned_db = db.clone();
    engine.register_fn("get_active_buffs", move |char_name: String| -> rhai::Array {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character
                .active_buffs
                .iter()
                .map(|b| {
                    let mut map = rhai::Map::new();
                    map.insert(
                        "effect_type".into(),
                        rhai::Dynamic::from(b.effect_type.to_display_string().to_string()),
                    );
                    map.insert("magnitude".into(), rhai::Dynamic::from(b.magnitude as i64));
                    map.insert("remaining_secs".into(), rhai::Dynamic::from(b.remaining_secs as i64));
                    map.insert("source".into(), rhai::Dynamic::from(b.source.clone()));
                    rhai::Dynamic::from(map)
                })
                .collect(),
            _ => rhai::Array::new(),
        }
    });

    // get_effective_stat(char_name, stat_name) -> i64
    // Returns base stat + buff bonuses + racial stat modifiers + racial passives
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn(
        "get_effective_stat",
        move |char_name: String, stat_name: String| -> i64 {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(character)) => {
                    let stat_key = stat_name.to_lowercase();
                    let (base, buff_type) = match stat_key.as_str() {
                        "strength" | "str" => (character.stat_str, EffectType::StrengthBoost),
                        "dexterity" | "dex" => (character.stat_dex, EffectType::DexterityBoost),
                        "constitution" | "con" => (character.stat_con, EffectType::ConstitutionBoost),
                        "intelligence" | "int" => (character.stat_int, EffectType::IntelligenceBoost),
                        "wisdom" | "wis" => (character.stat_wis, EffectType::WisdomBoost),
                        "charisma" | "cha" => (character.stat_cha, EffectType::CharismaBoost),
                        _ => return 10, // Unknown stat, return default
                    };
                    let buff_bonus: i32 = character
                        .active_buffs
                        .iter()
                        .filter(|b| b.effect_type == buff_type)
                        .map(|b| b.magnitude)
                        .sum();

                    // Look up racial bonuses (scoped lock, safe - no other lock held)
                    let racial_bonus = {
                        let race_id = character.race.to_lowercase();
                        let short_key = match stat_key.as_str() {
                            "strength" => "str",
                            "dexterity" => "dex",
                            "constitution" => "con",
                            "intelligence" => "int",
                            "wisdom" => "wis",
                            "charisma" => "cha",
                            other => other,
                        };
                        let world = state_clone.lock().unwrap();
                        if let Some(race) = world.race_definitions.get(&race_id) {
                            let stat_mod = *race.stat_modifiers.get(short_key).unwrap_or(&0);
                            let all_stats: i32 = race
                                .passive_abilities
                                .iter()
                                .flat_map(|p| p.effects.get("all_stats"))
                                .sum();
                            stat_mod + all_stats
                        } else {
                            0
                        }
                    };

                    let mut effective = (base + buff_bonus + racial_bonus) as i64;

                    // Head wound reduces intelligence (concussion effect)
                    if stat_key == "intelligence" || stat_key == "int" {
                        let head_penalty = character
                            .wounds
                            .iter()
                            .filter(|w| w.body_part == BodyPart::Head)
                            .map(|w| w.level.penalty())
                            .max()
                            .unwrap_or(0);
                        if head_penalty > 0 {
                            effective = (effective * (100 - head_penalty as i64) / 100).max(1);
                        }
                    }

                    effective
                }
                _ => 10, // Default stat value
            }
        },
    );

    // ========== Mana Functions ==========

    // restore_mana(connection_id, amount) -> bool
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("restore_mana", move |connection_id: String, amount: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get_mut(&uuid) {
                if let Some(ref mut char) = session.character {
                    if !char.mana_enabled {
                        return false;
                    }
                    let old_mana = char.mana;
                    char.mana = (char.mana + amount as i32).min(char.max_mana);
                    if char.mana != old_mana {
                        let _ = cloned_db.save_character_data(char.clone());
                    }
                    return true;
                }
            }
        }
        false
    });

    // get_character_mana(connection_id) -> Map with current/max/percent/enabled
    let conns = connections.clone();
    engine.register_fn("get_character_mana", move |connection_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get(&uuid) {
                if let Some(ref char) = session.character {
                    let mut map = rhai::Map::new();
                    map.insert("current".into(), rhai::Dynamic::from(char.mana as i64));
                    map.insert("max".into(), rhai::Dynamic::from(char.max_mana as i64));
                    map.insert("enabled".into(), rhai::Dynamic::from(char.mana_enabled));
                    let pct = if char.max_mana > 0 {
                        (char.mana * 100) / char.max_mana
                    } else {
                        0
                    };
                    map.insert("percent".into(), rhai::Dynamic::from(pct as i64));
                    return rhai::Dynamic::from(map);
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // ========== Drunk Functions ==========

    // set_drunk_level(char_name, level) -> bool
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("set_drunk_level", move |char_name: String, level: i64| -> bool {
        let name_lower = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&name_lower) {
            character.drunk_level = (level as i32).clamp(0, 100);
            if cloned_db.save_character_data(character.clone()).is_err() {
                return false;
            }
            let mut conns_guard = conns.lock().unwrap();
            for (_id, session) in conns_guard.iter_mut() {
                if let Some(ref mut sc) = session.character {
                    if sc.name.eq_ignore_ascii_case(&char_name) {
                        sc.drunk_level = character.drunk_level;
                        break;
                    }
                }
            }
            true
        } else {
            false
        }
    });

    // get_drunk_level(char_name) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_drunk_level", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(character)) => character.drunk_level as i64,
            _ => 0,
        }
    });

    // apply_room_death(connection_id) -> bool
    // Kills the connected player at their current room (drops corpse, respawns
    // at bound spawn point). Used by go.rhai when a player enters a ROOM_DEATH room.
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("apply_room_death", move |connection_id: String| -> bool {
        let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let (mut char, room_id) = {
            let conns_guard = conns.lock().unwrap();
            let session = match conns_guard.get(&conn_uuid) {
                Some(s) => s,
                None => return false,
            };
            let char = match session.character.as_ref() {
                Some(c) => c.clone(),
                None => return false,
            };
            let room_id = char.current_room_id;
            (char, room_id)
        };
        match crate::session::kill_player_at_room(&cloned_db, &conns, &mut char, &room_id, &connection_id) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("apply_room_death failed for {}: {}", char.name, e);
                false
            }
        }
    });

    // add_drunk(char_name, amount) -> i64 (returns new drunk_level)
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("add_drunk", move |char_name: String, amount: i64| -> i64 {
        let name_lower = char_name.to_lowercase();
        if let Ok(Some(mut character)) = cloned_db.get_character_data(&name_lower) {
            character.drunk_level = (character.drunk_level + amount as i32).clamp(0, 100);
            let new_level = character.drunk_level;
            if cloned_db.save_character_data(character.clone()).is_ok() {
                let mut conns_guard = conns.lock().unwrap();
                for (_id, session) in conns_guard.iter_mut() {
                    if let Some(ref mut sc) = session.character {
                        if sc.name.eq_ignore_ascii_case(&char_name) {
                            sc.drunk_level = new_level;
                            break;
                        }
                    }
                }
            }
            new_level as i64
        } else {
            0
        }
    });
}
