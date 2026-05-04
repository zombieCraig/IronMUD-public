// src/script/triggers.rs
// Trigger system functions for rooms, items, and mobiles

use crate::SharedConnections;
use crate::db::Db;
use crate::session::broadcast_to_builders;
use crate::{
    CharacterData, ItemData, ItemTrigger, ItemTriggerType, MobileData, MobileTrigger, MobileTriggerType, RoomData,
    RoomTrigger, TriggerType,
};
use rhai::Engine;
use std::sync::Arc;

/// Validate a trigger script name to prevent path traversal.
/// Only allows alphanumeric characters, underscores, and hyphens.
/// Template names (starting with @) are validated the same way for the part after @.
fn is_valid_script_name(name: &str) -> bool {
    let check = if let Some(stripped) = name.strip_prefix('@') {
        stripped
    } else {
        name
    };
    !check.is_empty() && check.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Execute a built-in mobile trigger template
/// Returns "continue" or "cancel"
fn execute_mobile_template(
    template_name: &str,
    args: &[String],
    mobile: &MobileData,
    connection_id: &str,
    connections: &SharedConnections,
    db: &Db,
) -> String {
    // Helper to send message to connection
    let send_msg = |msg: &str| {
        if let Ok(uuid) = uuid::Uuid::parse_str(connection_id) {
            if let Ok(conns) = connections.lock() {
                if let Some(session) = conns.get(&uuid) {
                    let _ = session.sender.send(msg.to_string());
                }
            }
        }
    };

    match template_name {
        "say_greeting" => {
            // @say_greeting <message>
            if let Some(message) = args.first() {
                send_msg(&format!("{} says: \"{}\"\n", mobile.name, message));
            }
            "continue".to_string()
        }
        "say_random" => {
            // @say_random <msg1|msg2|msg3>
            if !args.is_empty() {
                use rand::Rng;
                let idx = rand::thread_rng().gen_range(0..args.len());
                send_msg(&format!("{} says: \"{}\"\n", mobile.name, args[idx]));
            }
            "continue".to_string()
        }
        "emote" => {
            // @emote <action>
            if let Some(action) = args.first() {
                send_msg(&format!("{} {}\n", mobile.name, action));
            }
            "continue".to_string()
        }
        "shout" => {
            // @shout <message>
            // Broadcasts to mobile's room and all adjacent rooms
            if let Some(message) = args.first() {
                // Broadcast to mobile's current room
                if let Some(room_id) = mobile.current_room_id {
                    let room_msg = format!("{} shouts: \"{}!\"\n", mobile.name, message);
                    if let Ok(conns) = connections.lock() {
                        for (_, session) in conns.iter() {
                            if let Some(ref char_data) = session.character {
                                if char_data.current_room_id == room_id {
                                    let _ = session.sender.send(room_msg.clone());
                                }
                            }
                        }
                    }

                    // Broadcast to adjacent rooms
                    let adjacent_msg = format!("Someone shouts: \"{}!\"\n", message);
                    if let Ok(Some(room)) = db.get_room_data(&room_id) {
                        let exits: [Option<uuid::Uuid>; 6] = [
                            room.exits.north,
                            room.exits.south,
                            room.exits.east,
                            room.exits.west,
                            room.exits.up,
                            room.exits.down,
                        ];
                        for exit_opt in &exits {
                            if let Some(target_room_id) = exit_opt {
                                if let Ok(conns) = connections.lock() {
                                    for (_, session) in conns.iter() {
                                        if let Some(ref char_data) = session.character {
                                            if char_data.current_room_id == *target_room_id {
                                                let _ = session.sender.send(adjacent_msg.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "continue".to_string()
        }
        _ => {
            tracing::warn!("Unknown mobile template: @{}", template_name);
            "continue".to_string()
        }
    }
}

/// Execute a built-in item trigger template
/// Returns "continue" or "cancel"
fn execute_item_template(
    template_name: &str,
    args: &[String],
    connection_id: &str,
    connections: &SharedConnections,
) -> String {
    // Helper to send message to connection
    let send_msg = |msg: &str| {
        if let Ok(uuid) = uuid::Uuid::parse_str(connection_id) {
            if let Ok(conns) = connections.lock() {
                if let Some(session) = conns.get(&uuid) {
                    let _ = session.sender.send(format!("{}\n", msg));
                }
            }
        }
    };

    match template_name {
        "message" => {
            // @message <message>
            if let Some(message) = args.first() {
                send_msg(message);
            }
            "continue".to_string()
        }
        "random_message" => {
            // @random_message "msg1|msg2|msg3" or @random_message "msg1" "msg2" "msg3"
            // Sends a random message to the player
            if !args.is_empty() {
                use rand::Rng;
                // If single arg with pipes, split on pipes; otherwise use args directly
                if args.len() == 1 && args[0].contains('|') {
                    let messages: Vec<&str> = args[0].split('|').collect();
                    if !messages.is_empty() {
                        let idx = rand::thread_rng().gen_range(0..messages.len());
                        send_msg(messages[idx]);
                    }
                } else {
                    let idx = rand::thread_rng().gen_range(0..args.len());
                    send_msg(&args[idx]);
                }
            }
            "continue".to_string()
        }
        "block_message" => {
            // @block_message <message>
            if let Some(message) = args.first() {
                send_msg(message);
            }
            "cancel".to_string()
        }
        _ => {
            tracing::warn!("Unknown item template: @{}", template_name);
            "continue".to_string()
        }
    }
}

/// Execute a built-in room trigger template
/// Returns "continue" or "cancel"
pub fn execute_room_template(
    template_name: &str,
    args: &[String],
    room_id: &uuid::Uuid,
    connections: &SharedConnections,
    context: &std::collections::HashMap<String, String>,
) -> String {
    // Helper to broadcast message to all players in the room
    let broadcast = |msg: &str| {
        if let Ok(conns) = connections.lock() {
            for (_, session) in conns.iter() {
                if let Some(ref char_data) = session.character {
                    if char_data.current_room_id == *room_id {
                        let _ = session.sender.send(format!("{}\n", msg));
                    }
                }
            }
        }
    };

    match template_name {
        "room_message" => {
            // @room_message <message>
            // Always broadcast the message to all players in the room
            if let Some(message) = args.first() {
                broadcast(message);
            }
            "continue".to_string()
        }
        "time_message" => {
            // @time_message <time_of_day> <message>
            // Only broadcast if the new time matches the specified time
            // args[0] = target time (dawn, dusk, morning, etc.)
            // args[1] = message to show
            if args.len() >= 2 {
                let target_time = &args[0].to_lowercase();
                let message = &args[1];

                // Check if the new time matches
                if let Some(new_time) = context.get("new_time") {
                    if new_time.to_lowercase() == *target_time {
                        broadcast(message);
                    }
                }
            }
            "continue".to_string()
        }
        "weather_message" => {
            // @weather_message <weather_condition> <message>
            // Only broadcast if the new weather matches the specified condition
            // args[0] = target weather (rain, snow, clear, etc.) or category (raining, snowing)
            // args[1] = message to show
            if args.len() >= 2 {
                let target_weather = args[0].to_lowercase();
                let message = &args[1];

                // Check if the new weather matches
                if let Some(new_weather) = context.get("new_weather") {
                    let weather_lower = new_weather.to_lowercase();
                    // Check exact match or category match
                    let matches = weather_lower == target_weather
                        || (target_weather == "raining"
                            && (weather_lower.contains("rain") || weather_lower == "thunderstorm"))
                        || (target_weather == "snowing" && weather_lower.contains("snow"))
                        || (target_weather == "stormy" && weather_lower == "thunderstorm")
                        || (target_weather == "precipitation"
                            && (weather_lower.contains("rain") || weather_lower.contains("snow")));

                    if matches {
                        broadcast(message);
                    }
                }
            }
            "continue".to_string()
        }
        "season_message" => {
            // @season_message <season> <message>
            // Only broadcast if the new season matches the specified season
            // args[0] = target season (spring, summer, autumn, winter)
            // args[1] = message to show
            if args.len() >= 2 {
                let target_season = args[0].to_lowercase();
                let message = &args[1];

                // Check if the new season matches
                if let Some(new_season) = context.get("new_season") {
                    if new_season.to_lowercase() == target_season {
                        broadcast(message);
                    }
                }
            }
            "continue".to_string()
        }
        "random_message" => {
            // @random_message "msg1|msg2|msg3" or @random_message "msg1" "msg2" "msg3"
            // Broadcasts a random message to all players in the room
            if !args.is_empty() {
                use rand::Rng;
                // If single arg with pipes, split on pipes; otherwise use args directly
                if args.len() == 1 && args[0].contains('|') {
                    let messages: Vec<&str> = args[0].split('|').collect();
                    if !messages.is_empty() {
                        let idx = rand::thread_rng().gen_range(0..messages.len());
                        broadcast(messages[idx]);
                    }
                } else {
                    let idx = rand::thread_rng().gen_range(0..args.len());
                    broadcast(&args[idx]);
                }
            }
            "continue".to_string()
        }
        _ => {
            tracing::warn!("Unknown room template: @{}", template_name);
            "continue".to_string()
        }
    }
}

/// Register trigger-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Room Trigger Functions ==========

    // Register TriggerType enum for Rhai
    engine.register_type_with_name::<TriggerType>("TriggerType");

    // Register RoomTrigger type with getters
    engine
        .register_type_with_name::<RoomTrigger>("RoomTrigger")
        .register_get("script_name", |t: &mut RoomTrigger| t.script_name.clone())
        .register_get("enabled", |t: &mut RoomTrigger| t.enabled)
        .register_get("interval_secs", |t: &mut RoomTrigger| t.interval_secs)
        .register_get("chance", |t: &mut RoomTrigger| t.chance as i64)
        .register_get("last_fired", |t: &mut RoomTrigger| t.last_fired)
        .register_get("args", |t: &mut RoomTrigger| {
            t.args
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<rhai::Array>()
        });

    // Helper to get trigger type as string
    engine.register_fn("get_trigger_type", |t: RoomTrigger| match t.trigger_type {
        TriggerType::OnEnter => "on_enter".to_string(),
        TriggerType::OnExit => "on_exit".to_string(),
        TriggerType::OnLook => "on_look".to_string(),
        TriggerType::Periodic => "periodic".to_string(),
        TriggerType::OnTimeChange => "on_time_change".to_string(),
        TriggerType::OnWeatherChange => "on_weather_change".to_string(),
        TriggerType::OnSeasonChange => "on_season_change".to_string(),
        TriggerType::OnMonthChange => "on_month_change".to_string(),
    });

    // get_room_triggers(room_id) -> Array of RoomTrigger
    let cloned_db = db.clone();
    engine.register_fn("get_room_triggers", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return rhai::Array::new(),
        };
        match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(room)) => room.triggers.into_iter().map(rhai::Dynamic::from).collect(),
            _ => rhai::Array::new(),
        }
    });

    // add_room_trigger(room_id, trigger_type, script_name) -> bool
    // trigger_type: "on_enter", "on_exit", "on_look", "periodic", "on_time_change", "on_weather_change", "on_season_change", "on_month_change"
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_trigger",
        move |room_id: String, trigger_type: String, script_name: String| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_enter" | "enter" => TriggerType::OnEnter,
                "on_exit" | "exit" => TriggerType::OnExit,
                "on_look" | "look" => TriggerType::OnLook,
                "periodic" => TriggerType::Periodic,
                "on_time_change" | "time_change" => TriggerType::OnTimeChange,
                "on_weather_change" | "weather_change" => TriggerType::OnWeatherChange,
                "on_season_change" | "season_change" => TriggerType::OnSeasonChange,
                "on_month_change" | "month_change" => TriggerType::OnMonthChange,
                _ => return false,
            };
            room.triggers.push(RoomTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 100,
                args: Vec::new(),
            });
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // add_room_trigger_with_args(room_id, trigger_type, script_name, args) -> bool
    // Used for templates like @say_greeting with arguments
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_trigger_with_args",
        move |room_id: String, trigger_type: String, script_name: String, args: rhai::Array| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_enter" | "enter" => TriggerType::OnEnter,
                "on_exit" | "exit" => TriggerType::OnExit,
                "on_look" | "look" => TriggerType::OnLook,
                "periodic" => TriggerType::Periodic,
                "on_time_change" | "time_change" => TriggerType::OnTimeChange,
                "on_weather_change" | "weather_change" => TriggerType::OnWeatherChange,
                "on_season_change" | "season_change" => TriggerType::OnSeasonChange,
                "on_month_change" | "month_change" => TriggerType::OnMonthChange,
                _ => return false,
            };
            let string_args: Vec<String> = args.into_iter().filter_map(|a| a.try_cast::<String>()).collect();
            room.triggers.push(RoomTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 100,
                args: string_args,
            });
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // add_room_trigger_with_args_from(room_id, trigger_type, script_name, parts_array, start_idx) -> bool
    // Extracts args from parts_array starting at start_idx
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_trigger_with_args_from",
        move |room_id: String, trigger_type: String, script_name: String, parts: rhai::Array, start_idx: i64| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_enter" | "enter" => TriggerType::OnEnter,
                "on_exit" | "exit" => TriggerType::OnExit,
                "on_look" | "look" => TriggerType::OnLook,
                "periodic" => TriggerType::Periodic,
                "on_time_change" | "time_change" => TriggerType::OnTimeChange,
                "on_weather_change" | "weather_change" => TriggerType::OnWeatherChange,
                "on_season_change" | "season_change" => TriggerType::OnSeasonChange,
                "on_month_change" | "month_change" => TriggerType::OnMonthChange,
                _ => return false,
            };
            // Extract args from parts starting at start_idx
            let string_args: Vec<String> = parts
                .into_iter()
                .skip(start_idx as usize)
                .filter_map(|a| a.try_cast::<String>())
                .collect();
            room.triggers.push(RoomTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 100,
                args: string_args,
            });
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // remove_room_trigger(room_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_room_trigger", move |room_id: String, index: i64| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return false,
        };
        let idx = index as usize;
        if idx >= room.triggers.len() {
            return false;
        }
        room.triggers.remove(idx);
        cloned_db.save_room_data(room).is_ok()
    });

    // set_trigger_enabled(room_id, index, enabled) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_trigger_enabled",
        move |room_id: String, index: i64, enabled: bool| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= room.triggers.len() {
                return false;
            }
            room.triggers[idx].enabled = enabled;
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // set_trigger_interval(room_id, index, interval_secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_trigger_interval",
        move |room_id: String, index: i64, interval_secs: i64| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= room.triggers.len() {
                return false;
            }
            room.triggers[idx].interval_secs = interval_secs;
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // set_trigger_chance(room_id, index, chance) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_trigger_chance", move |room_id: String, index: i64, chance: i64| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return false,
        };
        let idx = index as usize;
        if idx >= room.triggers.len() {
            return false;
        }
        room.triggers[idx].chance = chance.clamp(1, 100) as i32;
        cloned_db.save_room_data(room).is_ok()
    });

    // random_int(min, max) -> i64 - for use in trigger scripts
    engine.register_fn("random_int", |min: i64, max: i64| {
        use rand::Rng;
        if min >= max {
            return min;
        }
        rand::thread_rng().gen_range(min..=max)
    });

    // test_room_trigger(room_id, trigger_index, connection_id) -> Map { success: bool, result: String, error: String }
    // Manually fire a trigger for debugging (bypasses chance roll)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "test_room_trigger",
        move |room_id: String, trigger_index: i64, connection_id: String| {
            let mut result_map = rhai::Map::new();

            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Invalid room ID".into());
                    return result_map;
                }
            };

            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Room not found".into());
                    return result_map;
                }
            };

            let idx = trigger_index as usize;
            if idx >= room.triggers.len() {
                result_map.insert("success".into(), false.into());
                result_map.insert(
                    "error".into(),
                    format!(
                        "Trigger index {} out of range (0-{})",
                        idx,
                        room.triggers.len().saturating_sub(1)
                    )
                    .into(),
                );
                return result_map;
            }

            let trigger = &room.triggers[idx];

            // Handle built-in templates (script_name starts with @)
            if trigger.script_name.starts_with('@') {
                let template_name = &trigger.script_name[1..];
                let ctx_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                let result = execute_room_template(template_name, &trigger.args, &room_uuid, &cloned_conns, &ctx_map);
                result_map.insert("success".into(), true.into());
                result_map.insert(
                    "result".into(),
                    format!("Template @{} executed: {}", template_name, result).into(),
                );
                result_map.insert("error".into(), "".into());
                return result_map;
            }

            // Validate script name to prevent path traversal
            if !is_valid_script_name(&trigger.script_name) {
                tracing::warn!(
                    "[SECURITY] Blocked trigger with invalid script_name: {}",
                    trigger.script_name
                );
                result_map.insert("success".into(), false.into());
                result_map.insert("error".into(), "Invalid script name".into());
                return result_map;
            }

            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);

            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(c) => c,
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Failed to load script: {}", e).into());
                    return result_map;
                }
            };

            let mut trigger_engine = rhai::Engine::new();
            trigger_engine.set_max_expr_depths(128, 128);
            trigger_engine.set_max_operations(100_000);
            trigger_engine.set_max_string_size(100_000);
            trigger_engine.set_max_array_size(1_000);
            trigger_engine.set_max_map_size(1_000);

            let conns_for_trigger = cloned_conns.clone();
            trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_trigger.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            let _ = session.sender.send(message);
                        }
                    }
                }
            });

            let conns_for_char = cloned_conns.clone();
            trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_char.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            if let Some(ref char) = session.character {
                                return rhai::Dynamic::from(char.clone());
                            }
                        }
                    }
                }
                rhai::Dynamic::UNIT
            });

            let db_for_trigger = cloned_db.clone();
            trigger_engine.register_fn("get_room_data", move |rid: String| -> rhai::Dynamic {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&rid) {
                    if let Ok(Some(room)) = db_for_trigger.get_room_data(&room_uuid) {
                        return rhai::Dynamic::from(room);
                    }
                }
                rhai::Dynamic::UNIT
            });

            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                use rand::Rng;
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            trigger_engine
                .register_type_with_name::<CharacterData>("CharacterData")
                .register_get("name", |c: &mut CharacterData| c.name.clone())
                .register_get("level", |c: &mut CharacterData| c.level as i64)
                .register_get("gold", |c: &mut CharacterData| c.gold as i64);

            trigger_engine
                .register_type_with_name::<RoomData>("RoomData")
                .register_get("id", |r: &mut RoomData| r.id.to_string())
                .register_get("title", |r: &mut RoomData| r.title.clone())
                .register_get("description", |r: &mut RoomData| r.description.clone());

            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    let mut scope = rhai::Scope::new();
                    let context = rhai::Map::new();

                    match trigger_engine.call_fn::<rhai::Dynamic>(
                        &mut scope,
                        &ast,
                        "run_trigger",
                        (room_id.clone(), connection_id.clone(), context),
                    ) {
                        Ok(res) => {
                            result_map.insert("success".into(), true.into());
                            result_map.insert("result".into(), res.to_string().into());
                            result_map.insert("error".into(), "".into());
                        }
                        Err(e) => {
                            result_map.insert("success".into(), false.into());
                            result_map.insert("error".into(), format!("Runtime error: {}", e).into());
                        }
                    }
                }
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Compile error: {}", e).into());
                }
            }

            result_map
        },
    );

    // fire_room_trigger(room_id, trigger_type, connection_id, context) -> "continue" | "cancel"
    // This is the main trigger execution function called from command scripts
    // context is a Rhai map with event-specific data (direction, source_room, etc.)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "fire_room_trigger",
        move |room_id: String, trigger_type: String, connection_id: String, context: rhai::Map| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return "continue".to_string(),
            };

            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return "continue".to_string(),
            };

            let target_type = match trigger_type.to_lowercase().as_str() {
                "on_enter" | "enter" => TriggerType::OnEnter,
                "on_exit" | "exit" => TriggerType::OnExit,
                "on_look" | "look" => TriggerType::OnLook,
                "periodic" => TriggerType::Periodic,
                "on_time_change" | "time_change" => TriggerType::OnTimeChange,
                "on_weather_change" | "weather_change" => TriggerType::OnWeatherChange,
                "on_season_change" | "season_change" => TriggerType::OnSeasonChange,
                "on_month_change" | "month_change" => TriggerType::OnMonthChange,
                _ => return "continue".to_string(),
            };

            // Find all matching triggers
            for trigger in &room.triggers {
                if trigger.trigger_type != target_type || !trigger.enabled {
                    continue;
                }

                // Check chance
                if trigger.chance < 100 {
                    use rand::Rng;
                    let roll: i32 = rand::thread_rng().gen_range(1..=100);
                    if roll > trigger.chance {
                        continue;
                    }
                }

                // Handle built-in templates (script_name starts with @)
                if trigger.script_name.starts_with('@') {
                    let template_name = &trigger.script_name[1..];
                    // Convert Rhai context map to HashMap for template function
                    let mut ctx_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    for (k, v) in context.iter() {
                        let key = k.to_string();
                        if let Some(val) = v.clone().try_cast::<rhai::ImmutableString>() {
                            ctx_map.insert(key, val.to_string());
                        }
                    }
                    let result =
                        execute_room_template(template_name, &trigger.args, &room_uuid, &cloned_conns, &ctx_map);
                    if result == "cancel" {
                        return "cancel".to_string();
                    }
                    continue;
                }

                // Validate script name to prevent path traversal
                if !is_valid_script_name(&trigger.script_name) {
                    tracing::warn!(
                        "[SECURITY] Blocked trigger with invalid script_name: {}",
                        trigger.script_name
                    );
                    continue;
                }

                // Execute trigger script
                let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);

                // Try to read and execute the script
                let script_content = match std::fs::read_to_string(&script_path) {
                    Ok(content) => content,
                    Err(e) => {
                        tracing::warn!("Trigger script not found: {} - {}", script_path, e);
                        continue;
                    }
                };

                // Create a new engine for trigger execution to avoid deadlock
                let mut trigger_engine = rhai::Engine::new();
                trigger_engine.set_max_expr_depths(128, 128);
                trigger_engine.set_max_operations(100_000);
                trigger_engine.set_max_string_size(100_000);
                trigger_engine.set_max_array_size(1_000);
                trigger_engine.set_max_map_size(1_000);

                // Register basic functions the trigger might need
                let _conn_id = connection_id.clone();
                let conns = cloned_conns.clone();
                trigger_engine.register_fn("send_client_message", move |cid: String, message: String| {
                    if let Ok(conn_uuid) = uuid::Uuid::parse_str(&cid) {
                        if let Some(session) = conns.lock().unwrap().get(&conn_uuid) {
                            let _ = session.sender.send(message);
                        }
                    }
                });

                let conns2 = cloned_conns.clone();
                trigger_engine.register_fn(
                    "broadcast_to_room",
                    move |rid: String, message: String, exclude: String| {
                        if let Ok(room_uuid) = uuid::Uuid::parse_str(&rid) {
                            crate::broadcast_to_room(
                                &conns2,
                                room_uuid,
                                message,
                                if exclude.is_empty() { None } else { Some(&exclude) },
                            );
                        }
                    },
                );

                let conns3 = cloned_conns.clone();
                trigger_engine.register_fn("get_player_character", move |cid: String| -> rhai::Dynamic {
                    if let Ok(conn_uuid) = uuid::Uuid::parse_str(&cid) {
                        if let Some(session) = conns3.lock().unwrap().get(&conn_uuid) {
                            if let Some(ref char) = session.character {
                                return rhai::Dynamic::from(char.clone());
                            }
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                let db_for_trigger = cloned_db.clone();
                trigger_engine.register_fn("get_room_data", move |rid: String| -> rhai::Dynamic {
                    if let Ok(room_uuid) = uuid::Uuid::parse_str(&rid) {
                        if let Ok(Some(room)) = db_for_trigger.get_room_data(&room_uuid) {
                            return rhai::Dynamic::from(room);
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                // Register CharacterData type for triggers
                trigger_engine
                    .register_type_with_name::<CharacterData>("CharacterData")
                    .register_get("name", |c: &mut CharacterData| c.name.clone())
                    .register_get("level", |c: &mut CharacterData| c.level as i64)
                    .register_get("gold", |c: &mut CharacterData| c.gold as i64);

                trigger_engine
                    .register_type_with_name::<RoomData>("RoomData")
                    .register_get("id", |r: &mut RoomData| r.id.to_string())
                    .register_get("title", |r: &mut RoomData| r.title.clone())
                    .register_get("vnum", |r: &mut RoomData| r.vnum.clone().unwrap_or_default());

                // Register set_room_flag for triggers (to modify room flags dynamically)
                let db_for_flag = cloned_db.clone();
                trigger_engine.register_fn(
                    "set_room_flag",
                    move |room_id: String, flag_name: String, value: bool| {
                        if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                            if let Ok(Some(mut room)) = db_for_flag.get_room_data(&room_uuid) {
                                match flag_name.as_str() {
                                    "dark" => room.flags.dark = value,
                                    "no_mob" => room.flags.no_mob = value,
                                    "indoors" => room.flags.indoors = value,
                                    "underwater" => room.flags.underwater = value,
                                    "city" => room.flags.city = value,
                                    "no_windows" => room.flags.no_windows = value,
                                    "climate_controlled" => room.flags.climate_controlled = value,
                                    "always_hot" => room.flags.always_hot = value,
                                    "always_cold" => room.flags.always_cold = value,
                                    "difficult_terrain" => room.flags.difficult_terrain = value,
                                    "dirt_floor" => room.flags.dirt_floor = value,
                                    "spawn_point" => room.flags.spawn_point = value,
                                    "private" | "private_room" => room.flags.private_room = value,
                                    "tunnel" => room.flags.tunnel = value,
                                    "death" => room.flags.death = value,
                                    "no_magic" => room.flags.no_magic = value,
                                    "soundproof" => room.flags.soundproof = value,
                                    "notrack" | "no_track" => room.flags.notrack = value,
                                    _ => return,
                                }
                                let _ = db_for_flag.save_room_data(room);
                            }
                        }
                    },
                );

                // Register set_room_dynamic_desc for triggers
                let db_for_dynamic = cloned_db.clone();
                trigger_engine.register_fn("set_room_dynamic_desc", move |room_id: String, desc: String| {
                    if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                        if let Ok(Some(mut room)) = db_for_dynamic.get_room_data(&room_uuid) {
                            room.dynamic_desc = Some(desc);
                            let _ = db_for_dynamic.save_room_data(room);
                        }
                    }
                });

                // Register clear_room_dynamic_desc for triggers
                let db_for_clear = cloned_db.clone();
                trigger_engine.register_fn("clear_room_dynamic_desc", move |room_id: String| {
                    if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                        if let Ok(Some(mut room)) = db_for_clear.get_room_data(&room_uuid) {
                            room.dynamic_desc = None;
                            let _ = db_for_clear.save_room_data(room);
                        }
                    }
                });

                // Compile and run
                match trigger_engine.compile(&script_content) {
                    Ok(ast) => {
                        let mut scope = rhai::Scope::new();
                        scope.push("room_id", room_id.clone());
                        scope.push("connection_id", connection_id.clone());
                        scope.push("context", context.clone());

                        match trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (room_id.clone(), connection_id.clone(), context.clone()),
                        ) {
                            Ok(result) => {
                                let result_str = result.to_string();
                                if result_str == "cancel" {
                                    return "cancel".to_string();
                                }
                            }
                            Err(e) => {
                                let msg = format!("Trigger script error in {}: {}", script_path, e);
                                tracing::error!("{}", msg);
                                broadcast_to_builders(&cloned_conns, &msg);
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to compile trigger script {}: {}", script_path, e);
                        tracing::error!("{}", msg);
                        broadcast_to_builders(&cloned_conns, &msg);
                    }
                }
            }

            "continue".to_string()
        },
    );

    // ========== Item Trigger Functions ==========

    // Register ItemTriggerType enum for Rhai
    engine.register_type_with_name::<ItemTriggerType>("ItemTriggerType");

    // Register ItemTrigger type with getters
    engine
        .register_type_with_name::<ItemTrigger>("ItemTrigger")
        .register_get("script_name", |t: &mut ItemTrigger| t.script_name.clone())
        .register_get("enabled", |t: &mut ItemTrigger| t.enabled)
        .register_get("chance", |t: &mut ItemTrigger| t.chance as i64)
        .register_get("args", |t: &mut ItemTrigger| {
            t.args
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<rhai::Array>()
        });

    // Helper to get item trigger type as string
    engine.register_fn("get_item_trigger_type", |t: ItemTrigger| match t.trigger_type {
        ItemTriggerType::OnGet => "on_get".to_string(),
        ItemTriggerType::OnDrop => "on_drop".to_string(),
        ItemTriggerType::OnUse => "on_use".to_string(),
        ItemTriggerType::OnExamine => "on_examine".to_string(),
        ItemTriggerType::OnPrompt => "on_prompt".to_string(),
    });

    // get_item_triggers(item_id) -> Array of ItemTrigger
    let cloned_db = db.clone();
    engine.register_fn("get_item_triggers", move |item_id: String| {
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return rhai::Array::new(),
        };
        match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(item)) => item.triggers.into_iter().map(rhai::Dynamic::from).collect(),
            _ => rhai::Array::new(),
        }
    });

    // add_item_trigger(item_id, trigger_type, script_name) -> bool
    // trigger_type: "on_get", "on_drop", "on_use", "on_examine", "on_prompt"
    let cloned_db = db.clone();
    engine.register_fn(
        "add_item_trigger",
        move |item_id: String, trigger_type: String, script_name: String| {
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_get" | "get" => ItemTriggerType::OnGet,
                "on_drop" | "drop" => ItemTriggerType::OnDrop,
                "on_use" | "use" => ItemTriggerType::OnUse,
                "on_examine" | "examine" => ItemTriggerType::OnExamine,
                "on_prompt" | "prompt" => ItemTriggerType::OnPrompt,
                _ => return false,
            };
            item.triggers.push(ItemTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: Vec::new(),
            });
            cloned_db.save_item_data(item).is_ok()
        },
    );

    // add_item_trigger_with_args(item_id, trigger_type, script_name, args) -> bool
    // Used for templates like @say_greeting with arguments
    let cloned_db = db.clone();
    engine.register_fn(
        "add_item_trigger_with_args",
        move |item_id: String, trigger_type: String, script_name: String, args: rhai::Array| {
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_get" | "get" => ItemTriggerType::OnGet,
                "on_drop" | "drop" => ItemTriggerType::OnDrop,
                "on_use" | "use" => ItemTriggerType::OnUse,
                "on_examine" | "examine" => ItemTriggerType::OnExamine,
                "on_prompt" | "prompt" => ItemTriggerType::OnPrompt,
                _ => return false,
            };
            let string_args: Vec<String> = args.into_iter().filter_map(|a| a.try_cast::<String>()).collect();
            item.triggers.push(ItemTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: string_args,
            });
            cloned_db.save_item_data(item).is_ok()
        },
    );

    // remove_item_trigger(item_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_item_trigger", move |item_id: String, index: i64| {
        let item_uuid = match uuid::Uuid::parse_str(&item_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut item = match cloned_db.get_item_data(&item_uuid) {
            Ok(Some(i)) => i,
            _ => return false,
        };
        let idx = index as usize;
        if idx >= item.triggers.len() {
            return false;
        }
        item.triggers.remove(idx);
        cloned_db.save_item_data(item).is_ok()
    });

    // set_item_trigger_enabled(item_id, index, enabled) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_trigger_enabled",
        move |item_id: String, index: i64, enabled: bool| {
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= item.triggers.len() {
                return false;
            }
            item.triggers[idx].enabled = enabled;
            cloned_db.save_item_data(item).is_ok()
        },
    );

    // set_item_trigger_chance(item_id, index, chance) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_trigger_chance",
        move |item_id: String, index: i64, chance: i64| {
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= item.triggers.len() {
                return false;
            }
            item.triggers[idx].chance = chance.clamp(1, 100) as i32;
            cloned_db.save_item_data(item).is_ok()
        },
    );

    // test_item_trigger(item_id, trigger_index, connection_id) -> Map { success: bool, result: String, error: String }
    // Manually fire a trigger for debugging (bypasses chance roll)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "test_item_trigger",
        move |item_id: String, trigger_index: i64, connection_id: String| {
            let mut result_map = rhai::Map::new();

            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Invalid item ID".into());
                    return result_map;
                }
            };

            let item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Item not found".into());
                    return result_map;
                }
            };

            let idx = trigger_index as usize;
            if idx >= item.triggers.len() {
                result_map.insert("success".into(), false.into());
                result_map.insert(
                    "error".into(),
                    format!(
                        "Trigger index {} out of range (0-{})",
                        idx,
                        item.triggers.len().saturating_sub(1)
                    )
                    .into(),
                );
                return result_map;
            }

            let trigger = &item.triggers[idx];

            // Validate script name to prevent path traversal
            if !is_valid_script_name(&trigger.script_name) {
                tracing::warn!(
                    "[SECURITY] Blocked trigger with invalid script_name: {}",
                    trigger.script_name
                );
                result_map.insert("success".into(), false.into());
                result_map.insert("error".into(), "Invalid script name".into());
                return result_map;
            }

            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);

            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(c) => c,
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Failed to load script: {}", e).into());
                    return result_map;
                }
            };

            let mut trigger_engine = rhai::Engine::new();
            trigger_engine.set_max_expr_depths(128, 128);
            trigger_engine.set_max_operations(100_000);
            trigger_engine.set_max_string_size(100_000);
            trigger_engine.set_max_array_size(1_000);
            trigger_engine.set_max_map_size(1_000);

            let conns_for_trigger = cloned_conns.clone();
            trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_trigger.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            let _ = session.sender.send(message);
                        }
                    }
                }
            });

            let conns_for_char = cloned_conns.clone();
            trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_char.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            if let Some(ref char) = session.character {
                                return rhai::Dynamic::from(char.clone());
                            }
                        }
                    }
                }
                rhai::Dynamic::UNIT
            });

            let db_for_trigger = cloned_db.clone();
            trigger_engine.register_fn("get_item_data", move |iid: String| -> rhai::Dynamic {
                if let Ok(item_uuid) = uuid::Uuid::parse_str(&iid) {
                    if let Ok(Some(item)) = db_for_trigger.get_item_data(&item_uuid) {
                        return rhai::Dynamic::from(item);
                    }
                }
                rhai::Dynamic::UNIT
            });

            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                use rand::Rng;
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            trigger_engine
                .register_type_with_name::<CharacterData>("CharacterData")
                .register_get("name", |c: &mut CharacterData| c.name.clone())
                .register_get("level", |c: &mut CharacterData| c.level as i64)
                .register_get("gold", |c: &mut CharacterData| c.gold as i64);

            trigger_engine
                .register_type_with_name::<ItemData>("ItemData")
                .register_get("id", |i: &mut ItemData| i.id.to_string())
                .register_get("name", |i: &mut ItemData| i.name.clone())
                .register_get("short_desc", |i: &mut ItemData| i.short_desc.clone());

            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    let mut scope = rhai::Scope::new();
                    let context = rhai::Map::new();

                    match trigger_engine.call_fn::<rhai::Dynamic>(
                        &mut scope,
                        &ast,
                        "run_trigger",
                        (item_id.clone(), connection_id.clone(), context),
                    ) {
                        Ok(res) => {
                            result_map.insert("success".into(), true.into());
                            result_map.insert("result".into(), res.to_string().into());
                            result_map.insert("error".into(), "".into());
                        }
                        Err(e) => {
                            result_map.insert("success".into(), false.into());
                            result_map.insert("error".into(), format!("Runtime error: {}", e).into());
                        }
                    }
                }
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Compile error: {}", e).into());
                }
            }

            result_map
        },
    );

    // fire_item_trigger(item_id, trigger_type, connection_id, context) -> "continue" | "cancel"
    // Called from command scripts when item events occur
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "fire_item_trigger",
        move |item_id: String, trigger_type_str: String, connection_id: String, context: rhai::Map| {
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return "continue".to_string(),
            };
            let item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return "continue".to_string(),
            };

            let trigger_type = match trigger_type_str.to_lowercase().as_str() {
                "on_get" => ItemTriggerType::OnGet,
                "on_drop" => ItemTriggerType::OnDrop,
                "on_use" => ItemTriggerType::OnUse,
                "on_examine" => ItemTriggerType::OnExamine,
                "on_prompt" => ItemTriggerType::OnPrompt,
                _ => return "continue".to_string(),
            };

            // Find all matching triggers
            let matching_triggers: Vec<_> = item
                .triggers
                .iter()
                .filter(|t| t.trigger_type == trigger_type && t.enabled)
                .collect();

            if matching_triggers.is_empty() {
                return "continue".to_string();
            }

            // Execute each matching trigger
            for trigger in matching_triggers {
                // Check chance
                if trigger.chance < 100 {
                    use rand::Rng;
                    let roll = rand::thread_rng().gen_range(1..=100);
                    if roll > trigger.chance {
                        continue;
                    }
                }

                // Handle built-in templates (script_name starts with @)
                if trigger.script_name.starts_with('@') {
                    let template_name = &trigger.script_name[1..];
                    let result = execute_item_template(template_name, &trigger.args, &connection_id, &cloned_conns);
                    if result == "cancel" {
                        return "cancel".to_string();
                    }
                    continue;
                }

                // Validate script name to prevent path traversal
                if !is_valid_script_name(&trigger.script_name) {
                    tracing::warn!(
                        "[SECURITY] Blocked item trigger with invalid script_name: {}",
                        trigger.script_name
                    );
                    continue;
                }

                // Load and execute trigger script
                let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
                let script_content = match std::fs::read_to_string(&script_path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to load item trigger script {}: {}", script_path, e);
                        continue;
                    }
                };

                // Create a new engine for trigger execution to avoid deadlock
                let mut trigger_engine = rhai::Engine::new();
                trigger_engine.set_max_expr_depths(128, 128);
                trigger_engine.set_max_operations(100_000);
                trigger_engine.set_max_string_size(100_000);
                trigger_engine.set_max_array_size(1_000);
                trigger_engine.set_max_map_size(1_000);

                // Register functions needed by triggers
                let conns_for_trigger = cloned_conns.clone();
                trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                        if let Ok(conns) = conns_for_trigger.lock() {
                            if let Some(session) = conns.get(&uuid) {
                                let _ = session.sender.send(message);
                            }
                        }
                    }
                });

                let conns_for_char = cloned_conns.clone();
                trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                        if let Ok(conns) = conns_for_char.lock() {
                            if let Some(session) = conns.get(&uuid) {
                                if let Some(ref char) = session.character {
                                    return rhai::Dynamic::from(char.clone());
                                }
                            }
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                let db_for_trigger = cloned_db.clone();
                trigger_engine.register_fn("get_item_data", move |iid: String| -> rhai::Dynamic {
                    if let Ok(item_uuid) = uuid::Uuid::parse_str(&iid) {
                        if let Ok(Some(item)) = db_for_trigger.get_item_data(&item_uuid) {
                            return rhai::Dynamic::from(item);
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                    use rand::Rng;
                    if min >= max {
                        return min;
                    }
                    rand::thread_rng().gen_range(min..=max)
                });

                // Register get_game_time for trigger scripts (used by smart_watch etc)
                let db_for_time = cloned_db.clone();
                trigger_engine.register_fn("get_game_time", move || -> rhai::Dynamic {
                    match db_for_time.get_game_time() {
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
                                "temperature".into(),
                                rhai::Dynamic::from(gt.calculate_effective_temperature() as i64),
                            );
                            map.insert("weather_desc".into(), rhai::Dynamic::from(format!("{}", gt.weather)));
                            rhai::Dynamic::from(map)
                        }
                        Err(_) => rhai::Dynamic::UNIT,
                    }
                });

                // Register get_character_thirst for trigger scripts
                let conns_for_thirst = cloned_conns.clone();
                trigger_engine.register_fn("get_character_thirst", move |connection_id: String| -> rhai::Dynamic {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                        if let Ok(conns) = conns_for_thirst.lock() {
                            if let Some(session) = conns.get(&uuid) {
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
                    }
                    rhai::Dynamic::UNIT
                });

                // Register CharacterData type for triggers
                trigger_engine
                    .register_type_with_name::<CharacterData>("CharacterData")
                    .register_get("name", |c: &mut CharacterData| c.name.clone())
                    .register_get("level", |c: &mut CharacterData| c.level as i64)
                    .register_get("gold", |c: &mut CharacterData| c.gold as i64);

                trigger_engine
                    .register_type_with_name::<ItemData>("ItemData")
                    .register_get("id", |i: &mut ItemData| i.id.to_string())
                    .register_get("name", |i: &mut ItemData| i.name.clone())
                    .register_get("short_desc", |i: &mut ItemData| i.short_desc.clone());

                // Compile and run
                match trigger_engine.compile(&script_content) {
                    Ok(ast) => {
                        let mut scope = rhai::Scope::new();
                        scope.push("item_id", item_id.clone());
                        scope.push("connection_id", connection_id.clone());
                        scope.push("context", context.clone());

                        match trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (item_id.clone(), connection_id.clone(), context.clone()),
                        ) {
                            Ok(result) => {
                                let result_str = result.to_string();
                                if result_str == "cancel" {
                                    return "cancel".to_string();
                                }
                            }
                            Err(e) => {
                                let msg = format!("Item trigger script error in {}: {}", script_path, e);
                                tracing::error!("{}", msg);
                                broadcast_to_builders(&cloned_conns, &msg);
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to compile item trigger script {}: {}", script_path, e);
                        tracing::error!("{}", msg);
                        broadcast_to_builders(&cloned_conns, &msg);
                    }
                }
            }

            "continue".to_string()
        },
    );

    // ========== Mobile/NPC Trigger Functions ==========

    // Register MobileTriggerType enum for Rhai
    engine.register_type_with_name::<MobileTriggerType>("MobileTriggerType");

    // Register MobileTrigger type with getters
    engine
        .register_type_with_name::<MobileTrigger>("MobileTrigger")
        .register_get("script_name", |t: &mut MobileTrigger| t.script_name.clone())
        .register_get("enabled", |t: &mut MobileTrigger| t.enabled)
        .register_get("chance", |t: &mut MobileTrigger| t.chance as i64)
        .register_get("args", |t: &mut MobileTrigger| {
            t.args
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<rhai::Array>()
        })
        .register_get("interval_secs", |t: &mut MobileTrigger| t.interval_secs)
        .register_get("last_fired", |t: &mut MobileTrigger| t.last_fired);

    // Helper to get mobile trigger type as string
    engine.register_fn("get_mobile_trigger_type", |t: MobileTrigger| match t.trigger_type {
        MobileTriggerType::OnGreet => "on_greet".to_string(),
        MobileTriggerType::OnAttack => "on_attack".to_string(),
        MobileTriggerType::OnDeath => "on_death".to_string(),
        MobileTriggerType::OnSay => "on_say".to_string(),
        MobileTriggerType::OnIdle => "on_idle".to_string(),
        MobileTriggerType::OnAlways => "on_always".to_string(),
        MobileTriggerType::OnFlee => "on_flee".to_string(),
    });

    // get_mobile_triggers(mobile_id) -> Array of MobileTrigger
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_triggers", move |mobile_id: String| {
        let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return rhai::Array::new(),
        };
        match cloned_db.get_mobile_data(&mobile_uuid) {
            Ok(Some(mobile)) => mobile.triggers.into_iter().map(rhai::Dynamic::from).collect(),
            _ => rhai::Array::new(),
        }
    });

    // add_mobile_trigger(mobile_id, trigger_type, script_name) -> bool
    // trigger_type: "on_greet", "on_attack", "on_death", "on_say", "on_flee"
    let cloned_db = db.clone();
    engine.register_fn(
        "add_mobile_trigger",
        move |mobile_id: String, trigger_type: String, script_name: String| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_greet" | "greet" => MobileTriggerType::OnGreet,
                "on_attack" | "attack" => MobileTriggerType::OnAttack,
                "on_death" | "death" => MobileTriggerType::OnDeath,
                "on_say" | "say" => MobileTriggerType::OnSay,
                "on_idle" | "idle" => MobileTriggerType::OnIdle,
                "on_always" | "always" => MobileTriggerType::OnAlways,
                "on_flee" | "flee" => MobileTriggerType::OnFlee,
                _ => return false,
            };
            mobile.triggers.push(MobileTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: Vec::new(),
                interval_secs: 60,
                last_fired: 0,
            });
            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );

    // add_mobile_trigger_with_args(mobile_id, trigger_type, script_name, args) -> bool
    // Used for templates like @say_greeting with arguments
    let cloned_db = db.clone();
    engine.register_fn(
        "add_mobile_trigger_with_args",
        move |mobile_id: String, trigger_type: String, script_name: String, args: rhai::Array| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let ttype = match trigger_type.to_lowercase().as_str() {
                "on_greet" | "greet" => MobileTriggerType::OnGreet,
                "on_attack" | "attack" => MobileTriggerType::OnAttack,
                "on_death" | "death" => MobileTriggerType::OnDeath,
                "on_say" | "say" => MobileTriggerType::OnSay,
                "on_idle" | "idle" => MobileTriggerType::OnIdle,
                "on_always" | "always" => MobileTriggerType::OnAlways,
                "on_flee" | "flee" => MobileTriggerType::OnFlee,
                _ => return false,
            };
            let string_args: Vec<String> = args.into_iter().filter_map(|a| a.try_cast::<String>()).collect();
            mobile.triggers.push(MobileTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: string_args,
                interval_secs: 60,
                last_fired: 0,
            });
            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );

    // remove_mobile_trigger(mobile_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_mobile_trigger", move |mobile_id: String, index: i64| {
        let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        let idx = index as usize;
        if idx >= mobile.triggers.len() {
            return false;
        }
        mobile.triggers.remove(idx);
        cloned_db.save_mobile_data(mobile).is_ok()
    });

    // set_mobile_trigger_enabled(mobile_id, index, enabled) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_trigger_enabled",
        move |mobile_id: String, index: i64, enabled: bool| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= mobile.triggers.len() {
                return false;
            }
            mobile.triggers[idx].enabled = enabled;
            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );

    // set_mobile_trigger_chance(mobile_id, index, chance) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_trigger_chance",
        move |mobile_id: String, index: i64, chance: i64| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= mobile.triggers.len() {
                return false;
            }
            mobile.triggers[idx].chance = chance.clamp(1, 100) as i32;
            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );

    // set_mobile_trigger_interval(mobile_id, index, interval_secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_trigger_interval",
        move |mobile_id: String, index: i64, interval_secs: i64| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let idx = index as usize;
            if idx >= mobile.triggers.len() {
                return false;
            }
            mobile.triggers[idx].interval_secs = interval_secs.max(1); // Minimum 1 second
            cloned_db.save_mobile_data(mobile).is_ok()
        },
    );

    // list_available_trigger_scripts() -> Array of script names (without .rhai extension)
    engine.register_fn("list_available_trigger_scripts", || {
        let mut scripts: Vec<rhai::Dynamic> = Vec::new();
        if let Ok(entries) = std::fs::read_dir("scripts/triggers") {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem() {
                    if entry.path().extension().is_some_and(|ext| ext == "rhai") {
                        scripts.push(name.to_string_lossy().to_string().into());
                    }
                }
            }
        }
        scripts.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        scripts
    });

    // validate_trigger_script(script_name) -> Map { valid: bool, error: String }
    // Check if a trigger script exists and is valid before adding
    engine.register_fn("validate_trigger_script", |script_name: String| {
        let mut result = rhai::Map::new();

        // Validate script name to prevent path traversal
        if !is_valid_script_name(&script_name) {
            result.insert("valid".into(), false.into());
            result.insert("error".into(), "Invalid script name: only alphanumeric, underscore, and hyphen allowed".into());
            return result;
        }

        // Built-in templates (start with @) are always valid
        if script_name.starts_with('@') {
            let template_name = &script_name[1..];
            // Room templates: room_message, time_message, weather_message, season_message, random_message
            // Item templates: message, random_message, block_message
            // Mobile templates: say_greeting, say_random, emote
            let valid_templates = [
                // Room templates
                "room_message", "time_message", "weather_message", "season_message", "random_message",
                // Item templates
                "message", "block_message",
                // Mobile templates
                "say_greeting", "say_random", "emote", "shout",
            ];
            if valid_templates.contains(&template_name) {
                result.insert("valid".into(), true.into());
                result.insert("error".into(), "".into());
            } else {
                result.insert("valid".into(), false.into());
                result.insert("error".into(),
                    format!("Unknown template: @{}\nValid templates:\n  Room: @room_message, @time_message, @weather_message, @season_message, @random_message\n  Item: @message, @random_message, @block_message\n  Mobile: @say_greeting, @say_random, @emote, @shout", template_name).into());
            }
            return result;
        }

        let script_path = format!("scripts/triggers/{}.rhai", script_name);

        // Check if file exists
        match std::fs::read_to_string(&script_path) {
            Ok(content) => {
                // Try to compile
                let test_engine = rhai::Engine::new();
                match test_engine.compile(&content) {
                    Ok(ast) => {
                        // Check if run_trigger function exists with 3 parameters
                        let has_fn = ast.iter_functions()
                            .any(|f| f.name == "run_trigger" && f.params.len() == 3);
                        if has_fn {
                            result.insert("valid".into(), true.into());
                            result.insert("error".into(), "".into());
                        } else {
                            result.insert("valid".into(), false.into());
                            result.insert("error".into(),
                                "Script missing run_trigger(entity_id, connection_id, context) function".into());
                        }
                    }
                    Err(e) => {
                        result.insert("valid".into(), false.into());
                        result.insert("error".into(), format!("Compile error: {}", e).into());
                    }
                }
            }
            Err(_) => {
                result.insert("valid".into(), false.into());
                result.insert("error".into(), format!("Script not found: {}", script_path).into());
            }
        }
        result
    });

    // get_trigger_script_content(script_name) -> Map { success: bool, content: String, error: String }
    // Read and return the contents of a trigger script file
    engine.register_fn("get_trigger_script_content", |script_name: String| {
        let mut result = rhai::Map::new();

        if !is_valid_script_name(&script_name) {
            result.insert("success".into(), false.into());
            result.insert("content".into(), "".into());
            result.insert(
                "error".into(),
                "Invalid script name: only alphanumeric, underscore, and hyphen allowed".into(),
            );
            return result;
        }

        if script_name.starts_with('@') {
            result.insert("success".into(), false.into());
            result.insert("content".into(), "".into());
            result.insert(
                "error".into(),
                "Built-in templates (@) have no script file to view.".into(),
            );
            return result;
        }

        let script_path = format!("scripts/triggers/{}.rhai", script_name);
        match std::fs::read_to_string(&script_path) {
            Ok(content) => {
                result.insert("success".into(), true.into());
                result.insert("content".into(), content.into());
                result.insert("error".into(), "".into());
            }
            Err(_) => {
                result.insert("success".into(), false.into());
                result.insert("content".into(), "".into());
                result.insert("error".into(), format!("Script not found: {}", script_path).into());
            }
        }
        result
    });

    // test_mobile_trigger(mobile_id, trigger_index, connection_id) -> Map { success: bool, result: String, error: String }
    // Manually fire a trigger for debugging (bypasses chance roll)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "test_mobile_trigger",
        move |mobile_id: String, trigger_index: i64, connection_id: String| {
            let mut result_map = rhai::Map::new();

            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Invalid mobile ID".into());
                    return result_map;
                }
            };

            let mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), "Mobile not found".into());
                    return result_map;
                }
            };

            let idx = trigger_index as usize;
            if idx >= mobile.triggers.len() {
                result_map.insert("success".into(), false.into());
                result_map.insert(
                    "error".into(),
                    format!(
                        "Trigger index {} out of range (0-{})",
                        idx,
                        mobile.triggers.len().saturating_sub(1)
                    )
                    .into(),
                );
                return result_map;
            }

            let trigger = &mobile.triggers[idx];

            // Validate script name to prevent path traversal
            if !is_valid_script_name(&trigger.script_name) {
                tracing::warn!(
                    "[SECURITY] Blocked trigger with invalid script_name: {}",
                    trigger.script_name
                );
                result_map.insert("success".into(), false.into());
                result_map.insert("error".into(), "Invalid script name".into());
                return result_map;
            }

            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);

            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(c) => c,
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Failed to load script: {}", e).into());
                    return result_map;
                }
            };

            // Create trigger engine with all necessary functions
            let mut trigger_engine = rhai::Engine::new();
            trigger_engine.set_max_expr_depths(128, 128);
            trigger_engine.set_max_operations(100_000);
            trigger_engine.set_max_string_size(100_000);
            trigger_engine.set_max_array_size(1_000);
            trigger_engine.set_max_map_size(1_000);

            let conns_for_trigger = cloned_conns.clone();
            trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_trigger.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            let _ = session.sender.send(message);
                        }
                    }
                }
            });

            let conns_for_char = cloned_conns.clone();
            trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
                if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                    if let Ok(conns) = conns_for_char.lock() {
                        if let Some(session) = conns.get(&uuid) {
                            if let Some(ref char) = session.character {
                                return rhai::Dynamic::from(char.clone());
                            }
                        }
                    }
                }
                rhai::Dynamic::UNIT
            });

            let db_for_trigger = cloned_db.clone();
            trigger_engine.register_fn("get_mobile_data", move |mid: String| -> rhai::Dynamic {
                if let Ok(mobile_uuid) = uuid::Uuid::parse_str(&mid) {
                    if let Ok(Some(mobile)) = db_for_trigger.get_mobile_data(&mobile_uuid) {
                        return rhai::Dynamic::from(mobile);
                    }
                }
                rhai::Dynamic::UNIT
            });

            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                use rand::Rng;
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            // Register types
            trigger_engine
                .register_type_with_name::<CharacterData>("CharacterData")
                .register_get("name", |c: &mut CharacterData| c.name.clone())
                .register_get("level", |c: &mut CharacterData| c.level as i64)
                .register_get("gold", |c: &mut CharacterData| c.gold as i64);

            trigger_engine
                .register_type_with_name::<MobileData>("MobileData")
                .register_get("id", |m: &mut MobileData| m.id.to_string())
                .register_get("name", |m: &mut MobileData| m.name.clone())
                .register_get("short_desc", |m: &mut MobileData| m.short_desc.clone())
                .register_get("level", |m: &mut MobileData| m.level as i64)
                .register_get("current_hp", |m: &mut MobileData| m.current_hp as i64)
                .register_get("max_hp", |m: &mut MobileData| m.max_hp as i64);

            // Compile and execute
            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    let mut scope = rhai::Scope::new();
                    let context = rhai::Map::new(); // Empty context for test

                    match trigger_engine.call_fn::<rhai::Dynamic>(
                        &mut scope,
                        &ast,
                        "run_trigger",
                        (mobile_id.clone(), connection_id.clone(), context),
                    ) {
                        Ok(res) => {
                            result_map.insert("success".into(), true.into());
                            result_map.insert("result".into(), res.to_string().into());
                            result_map.insert("error".into(), "".into());
                        }
                        Err(e) => {
                            result_map.insert("success".into(), false.into());
                            result_map.insert("error".into(), format!("Runtime error: {}", e).into());
                        }
                    }
                }
                Err(e) => {
                    result_map.insert("success".into(), false.into());
                    result_map.insert("error".into(), format!("Compile error: {}", e).into());
                }
            }

            result_map
        },
    );

    // fire_mobile_trigger(mobile_id, trigger_type, connection_id, context) -> "continue" | "cancel"
    // Called from command scripts when mobile events occur
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "fire_mobile_trigger",
        move |mobile_id: String, trigger_type_str: String, connection_id: String, context: rhai::Map| {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return "continue".to_string(),
            };
            let mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return "continue".to_string(),
            };

            let trigger_type = match trigger_type_str.to_lowercase().as_str() {
                "on_greet" => MobileTriggerType::OnGreet,
                "on_attack" => MobileTriggerType::OnAttack,
                "on_death" => MobileTriggerType::OnDeath,
                "on_say" => MobileTriggerType::OnSay,
                "on_flee" => MobileTriggerType::OnFlee,
                _ => return "continue".to_string(),
            };

            // Find all matching triggers
            let matching_triggers: Vec<_> = mobile
                .triggers
                .iter()
                .filter(|t| t.trigger_type == trigger_type && t.enabled)
                .collect();

            if matching_triggers.is_empty() {
                return "continue".to_string();
            }

            // Execute each matching trigger
            for trigger in matching_triggers {
                // Check chance
                if trigger.chance < 100 {
                    use rand::Rng;
                    let roll = rand::thread_rng().gen_range(1..=100);
                    if roll > trigger.chance {
                        continue;
                    }
                }

                // Handle built-in templates (script_name starts with @)
                if trigger.script_name.starts_with('@') {
                    let template_name = &trigger.script_name[1..];
                    let result = execute_mobile_template(
                        template_name,
                        &trigger.args,
                        &mobile,
                        &connection_id,
                        &cloned_conns,
                        &cloned_db,
                    );
                    if result == "cancel" {
                        return "cancel".to_string();
                    }
                    continue;
                }

                // Validate script name to prevent path traversal
                if !is_valid_script_name(&trigger.script_name) {
                    tracing::warn!(
                        "[SECURITY] Blocked mobile trigger with invalid script_name: {}",
                        trigger.script_name
                    );
                    continue;
                }

                // Load and execute trigger script
                let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
                let script_content = match std::fs::read_to_string(&script_path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to load mobile trigger script {}: {}", script_path, e);
                        continue;
                    }
                };

                // Create a new engine for trigger execution to avoid deadlock
                let mut trigger_engine = rhai::Engine::new();
                trigger_engine.set_max_expr_depths(128, 128);
                trigger_engine.set_max_operations(100_000);
                trigger_engine.set_max_string_size(100_000);
                trigger_engine.set_max_array_size(1_000);
                trigger_engine.set_max_map_size(1_000);

                // Register functions needed by triggers
                let conns_for_trigger = cloned_conns.clone();
                trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                        if let Ok(conns) = conns_for_trigger.lock() {
                            if let Some(session) = conns.get(&uuid) {
                                let _ = session.sender.send(message);
                            }
                        }
                    }
                });

                let conns_for_char = cloned_conns.clone();
                trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                        if let Ok(conns) = conns_for_char.lock() {
                            if let Some(session) = conns.get(&uuid) {
                                if let Some(ref char) = session.character {
                                    return rhai::Dynamic::from(char.clone());
                                }
                            }
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                let db_for_trigger = cloned_db.clone();
                trigger_engine.register_fn("get_mobile_data", move |mid: String| -> rhai::Dynamic {
                    if let Ok(mobile_uuid) = uuid::Uuid::parse_str(&mid) {
                        if let Ok(Some(mobile)) = db_for_trigger.get_mobile_data(&mobile_uuid) {
                            return rhai::Dynamic::from(mobile);
                        }
                    }
                    rhai::Dynamic::UNIT
                });

                trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                    use rand::Rng;
                    if min >= max {
                        return min;
                    }
                    rand::thread_rng().gen_range(min..=max)
                });

                // Register CharacterData type for triggers
                trigger_engine
                    .register_type_with_name::<CharacterData>("CharacterData")
                    .register_get("name", |c: &mut CharacterData| c.name.clone())
                    .register_get("level", |c: &mut CharacterData| c.level as i64)
                    .register_get("gold", |c: &mut CharacterData| c.gold as i64);

                trigger_engine
                    .register_type_with_name::<MobileData>("MobileData")
                    .register_get("id", |m: &mut MobileData| m.id.to_string())
                    .register_get("name", |m: &mut MobileData| m.name.clone())
                    .register_get("short_desc", |m: &mut MobileData| m.short_desc.clone())
                    .register_get("level", |m: &mut MobileData| m.level as i64)
                    .register_get("current_hp", |m: &mut MobileData| m.current_hp as i64)
                    .register_get("max_hp", |m: &mut MobileData| m.max_hp as i64);

                // Compile and run
                match trigger_engine.compile(&script_content) {
                    Ok(ast) => {
                        let mut scope = rhai::Scope::new();
                        scope.push("mobile_id", mobile_id.clone());
                        scope.push("connection_id", connection_id.clone());
                        scope.push("context", context.clone());

                        match trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (mobile_id.clone(), connection_id.clone(), context.clone()),
                        ) {
                            Ok(result) => {
                                let result_str = result.to_string();
                                if result_str == "cancel" {
                                    return "cancel".to_string();
                                }
                            }
                            Err(e) => {
                                let msg = format!("Mobile trigger script error in {}: {}", script_path, e);
                                tracing::error!("{}", msg);
                                broadcast_to_builders(&cloned_conns, &msg);
                            }
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to compile mobile trigger script {}: {}", script_path, e);
                        tracing::error!("{}", msg);
                        broadcast_to_builders(&cloned_conns, &msg);
                    }
                }
            }

            "continue".to_string()
        },
    );
}

/// Fire mobile triggers from Rust code (e.g., combat tick).
/// This avoids rhai dependency by accepting plain HashMap instead of rhai::Map.
pub fn fire_mobile_triggers_from_rust(
    db: &Db,
    connections: &SharedConnections,
    mobile_id_str: &str,
    trigger_type_str: &str,
    connection_id: &str,
    context: &std::collections::HashMap<String, String>,
) -> String {
    let mobile_uuid = match uuid::Uuid::parse_str(mobile_id_str) {
        Ok(u) => u,
        Err(_) => return "continue".to_string(),
    };
    let mobile = match db.get_mobile_data(&mobile_uuid) {
        Ok(Some(m)) => m,
        _ => return "continue".to_string(),
    };

    let trigger_type = match trigger_type_str.to_lowercase().as_str() {
        "on_greet" => MobileTriggerType::OnGreet,
        "on_attack" => MobileTriggerType::OnAttack,
        "on_death" => MobileTriggerType::OnDeath,
        "on_say" => MobileTriggerType::OnSay,
        "on_flee" => MobileTriggerType::OnFlee,
        _ => return "continue".to_string(),
    };

    // Find all matching triggers
    let matching_triggers: Vec<_> = mobile
        .triggers
        .iter()
        .filter(|t| t.trigger_type == trigger_type && t.enabled)
        .collect();

    if matching_triggers.is_empty() {
        return "continue".to_string();
    }

    // Execute each matching trigger
    for trigger in matching_triggers {
        // Check chance
        if trigger.chance < 100 {
            use rand::Rng;
            let roll = rand::thread_rng().gen_range(1..=100);
            if roll > trigger.chance {
                continue;
            }
        }

        // Handle built-in templates (script_name starts with @)
        if trigger.script_name.starts_with('@') {
            let template_name = &trigger.script_name[1..];
            let result = execute_mobile_template(template_name, &trigger.args, &mobile, connection_id, connections, db);
            if result == "cancel" {
                return "cancel".to_string();
            }
            continue;
        }

        // Validate script name to prevent path traversal
        if !is_valid_script_name(&trigger.script_name) {
            tracing::warn!(
                "[SECURITY] Blocked mobile trigger with invalid script_name: {}",
                trigger.script_name
            );
            continue;
        }

        // Load and execute trigger script
        let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
        let script_content = match std::fs::read_to_string(&script_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to load mobile trigger script {}: {}", script_path, e);
                continue;
            }
        };

        // Create a new engine for trigger execution to avoid deadlock
        let mut trigger_engine = rhai::Engine::new();
        trigger_engine.set_max_expr_depths(128, 128);
        trigger_engine.set_max_operations(100_000);
        trigger_engine.set_max_string_size(100_000);
        trigger_engine.set_max_array_size(1_000);
        trigger_engine.set_max_map_size(1_000);

        // Register functions needed by triggers
        let conns_for_trigger = connections.clone();
        trigger_engine.register_fn("send_client_message", move |conn_id: String, message: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                if let Ok(conns) = conns_for_trigger.lock() {
                    if let Some(session) = conns.get(&uuid) {
                        let _ = session.sender.send(message);
                    }
                }
            }
        });

        let conns_for_char = connections.clone();
        trigger_engine.register_fn("get_player_character", move |conn_id: String| -> rhai::Dynamic {
            if let Ok(uuid) = uuid::Uuid::parse_str(&conn_id) {
                if let Ok(conns) = conns_for_char.lock() {
                    if let Some(session) = conns.get(&uuid) {
                        if let Some(ref char) = session.character {
                            return rhai::Dynamic::from(char.clone());
                        }
                    }
                }
            }
            rhai::Dynamic::UNIT
        });

        let db_for_trigger = db.clone();
        trigger_engine.register_fn("get_mobile_data", move |mid: String| -> rhai::Dynamic {
            if let Ok(mobile_uuid) = uuid::Uuid::parse_str(&mid) {
                if let Ok(Some(mobile)) = db_for_trigger.get_mobile_data(&mobile_uuid) {
                    return rhai::Dynamic::from(mobile);
                }
            }
            rhai::Dynamic::UNIT
        });

        trigger_engine.register_fn("random_int", |min: i64, max: i64| {
            use rand::Rng;
            if min >= max {
                return min;
            }
            rand::thread_rng().gen_range(min..=max)
        });

        // Register CharacterData type for triggers
        trigger_engine
            .register_type_with_name::<CharacterData>("CharacterData")
            .register_get("name", |c: &mut CharacterData| c.name.clone())
            .register_get("level", |c: &mut CharacterData| c.level as i64)
            .register_get("gold", |c: &mut CharacterData| c.gold as i64);

        trigger_engine
            .register_type_with_name::<MobileData>("MobileData")
            .register_get("id", |m: &mut MobileData| m.id.to_string())
            .register_get("name", |m: &mut MobileData| m.name.clone())
            .register_get("short_desc", |m: &mut MobileData| m.short_desc.clone())
            .register_get("level", |m: &mut MobileData| m.level as i64)
            .register_get("current_hp", |m: &mut MobileData| m.current_hp as i64)
            .register_get("max_hp", |m: &mut MobileData| m.max_hp as i64);

        // Convert HashMap context to rhai::Map
        let mut rhai_context = rhai::Map::new();
        for (k, v) in context {
            rhai_context.insert(k.clone().into(), rhai::Dynamic::from(v.clone()));
        }

        // Compile and run
        let mobile_id_owned = mobile_id_str.to_string();
        let connection_id_owned = connection_id.to_string();
        match trigger_engine.compile(&script_content) {
            Ok(ast) => {
                let mut scope = rhai::Scope::new();
                scope.push("mobile_id", mobile_id_owned.clone());
                scope.push("connection_id", connection_id_owned.clone());
                scope.push("context", rhai_context.clone());

                match trigger_engine.call_fn::<rhai::Dynamic>(
                    &mut scope,
                    &ast,
                    "run_trigger",
                    (mobile_id_owned, connection_id_owned, rhai_context),
                ) {
                    Ok(result) => {
                        let result_str = result.to_string();
                        if result_str == "cancel" {
                            return "cancel".to_string();
                        }
                    }
                    Err(e) => {
                        let msg = format!("Mobile trigger script error in {}: {}", script_path, e);
                        tracing::error!("{}", msg);
                        broadcast_to_builders(connections, &msg);
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to compile mobile trigger script {}: {}", script_path, e);
                tracing::error!("{}", msg);
                broadcast_to_builders(connections, &msg);
            }
        }
    }

    "continue".to_string()
}
