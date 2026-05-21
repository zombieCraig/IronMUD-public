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

/// Parse a room trigger type name into a [`TriggerType`]. Recognises both
/// `on_xxx` and bare `xxx` forms.
pub(crate) fn parse_room_trigger_type(s: &str) -> Option<TriggerType> {
    Some(match s.to_ascii_lowercase().as_str() {
        "on_enter" | "enter" => TriggerType::OnEnter,
        "on_exit" | "exit" => TriggerType::OnExit,
        "on_look" | "look" => TriggerType::OnLook,
        "periodic" => TriggerType::Periodic,
        "on_time_change" | "time_change" => TriggerType::OnTimeChange,
        "on_weather_change" | "weather_change" => TriggerType::OnWeatherChange,
        "on_season_change" | "season_change" => TriggerType::OnSeasonChange,
        "on_month_change" | "month_change" => TriggerType::OnMonthChange,
        "on_command" | "command" => TriggerType::OnCommand,
        _ => return None,
    })
}

/// Parse an item trigger type name into an [`ItemTriggerType`].
pub(crate) fn parse_item_trigger_type(s: &str) -> Option<ItemTriggerType> {
    Some(match s.to_ascii_lowercase().as_str() {
        "on_get" | "get" => ItemTriggerType::OnGet,
        "on_drop" | "drop" => ItemTriggerType::OnDrop,
        "on_use" | "use" => ItemTriggerType::OnUse,
        "on_examine" | "examine" => ItemTriggerType::OnExamine,
        "on_look" | "look" => ItemTriggerType::OnLook,
        "on_prompt" | "prompt" => ItemTriggerType::OnPrompt,
        "on_load" | "load" => ItemTriggerType::OnLoad,
        "on_command" | "command" => ItemTriggerType::OnCommand,
        "on_wear" | "wear" => ItemTriggerType::OnWear,
        "on_remove" | "remove" => ItemTriggerType::OnRemove,
        "on_wield" | "wield" => ItemTriggerType::OnWield,
        _ => return None,
    })
}

/// Parse a string like "on_greet" / "greet" / "on_hitprcnt" / "hitprcnt"
/// into a [`MobileTriggerType`]. Returns None for unrecognised input.
pub(crate) fn parse_mobile_trigger_type(s: &str) -> Option<MobileTriggerType> {
    Some(match s.to_ascii_lowercase().as_str() {
        "on_greet" | "greet" => MobileTriggerType::OnGreet,
        "on_attack" | "attack" => MobileTriggerType::OnAttack,
        "on_death" | "death" => MobileTriggerType::OnDeath,
        "on_say" | "say" => MobileTriggerType::OnSay,
        "on_idle" | "idle" => MobileTriggerType::OnIdle,
        "on_always" | "always" => MobileTriggerType::OnAlways,
        "on_flee" | "flee" => MobileTriggerType::OnFlee,
        "on_fight" | "fight" => MobileTriggerType::OnFight,
        "on_hitprcnt" | "hitprcnt" | "hitpercent" | "on_hitpercent" => MobileTriggerType::OnHitPercent,
        "on_receive" | "receive" => MobileTriggerType::OnReceive,
        "on_bribe" | "bribe" => MobileTriggerType::OnBribe,
        "on_load" | "load" => MobileTriggerType::OnLoad,
        "on_command" | "command" => MobileTriggerType::OnCommand,
        _ => return None,
    })
}

/// Convert a Rhai `context` map (the 4th arg to `fire_*_trigger`) into the
/// `HashMap<String, String>` shape consumed by both the template branch
/// and the DG `EvalCtx.context_vars`. Strings, integers, floats, and bools
/// are stringified; other types (maps, arrays) are dropped — callers
/// should pass primitives only.
fn rhai_context_to_strmap(context: &rhai::Map) -> std::collections::HashMap<String, String> {
    let mut out: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (k, v) in context.iter() {
        let key = k.to_string();
        let val = if let Some(s) = v.clone().try_cast::<rhai::ImmutableString>() {
            s.to_string()
        } else if let Some(i) = v.clone().try_cast::<i64>() {
            i.to_string()
        } else if let Some(f) = v.clone().try_cast::<f64>() {
            f.to_string()
        } else if let Some(b) = v.clone().try_cast::<bool>() {
            b.to_string()
        } else {
            continue;
        };
        out.insert(key, val);
    }
    out
}

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
        TriggerType::OnCommand => "on_command".to_string(),
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
            let ttype = match parse_room_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
            };
            room.triggers.push(RoomTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                interval_secs: 60,
                last_fired: 0,
                chance: 100,
                args: Vec::new(),
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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
            let ttype = match parse_room_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
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
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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
            let ttype = match parse_room_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
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
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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

            let target_type = match parse_room_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return "continue".to_string(),
            };

            // Convert the rhai context map once — shared between DG bodies
            // (passed into EvalCtx.context_vars) and the legacy @template
            // branch.
            let ctx_strmap = rhai_context_to_strmap(&context);

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

                // DG Scripts body (imported from .trg). Routes through the
                // runtime DG interpreter; `return 0` from a command-shape
                // trigger cancels the host action.
                if let Some(body) = trigger.dg_body.as_deref() {
                    let outcome = super::dg::fire_room_dg(
                        body,
                        &room,
                        &connection_id,
                        cloned_db.clone(),
                        cloned_conns.clone(),
                        trigger.authored_by.clone(),
                        trigger.elevated,
                        ctx_strmap.clone(),
                    );
                    if matches!(outcome, super::dg::Outcome::Return(0)) {
                        return "cancel".to_string();
                    }
                    continue;
                }

                // Handle built-in templates (script_name starts with @)
                if trigger.script_name.starts_with('@') {
                    let template_name = &trigger.script_name[1..];
                    let result =
                        execute_room_template(template_name, &trigger.args, &room_uuid, &cloned_conns, &ctx_strmap);
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
        ItemTriggerType::OnLook => "on_look".to_string(),
        ItemTriggerType::OnPrompt => "on_prompt".to_string(),
        ItemTriggerType::OnLoad => "on_load".to_string(),
        ItemTriggerType::OnCommand => "on_command".to_string(),
        ItemTriggerType::OnWear => "on_wear".to_string(),
        ItemTriggerType::OnRemove => "on_remove".to_string(),
        ItemTriggerType::OnWield => "on_wield".to_string(),
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
    // trigger_type: "on_get", "on_drop", "on_use", "on_examine", "on_look", "on_prompt"
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
            let ttype = match parse_item_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
            };
            item.triggers.push(ItemTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: Vec::new(),
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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
            let ttype = match parse_item_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
            };
            let string_args: Vec<String> = args.into_iter().filter_map(|a| a.try_cast::<String>()).collect();
            item.triggers.push(ItemTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: string_args,
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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

            let trigger_type = match parse_item_trigger_type(&trigger_type_str) {
                Some(t) => t,
                None => return "continue".to_string(),
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

            // Convert the rhai context map once — used to seed
            // EvalCtx.context_vars on the DG branch.
            let ctx_strmap = rhai_context_to_strmap(&context);

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

                // DG Scripts body (imported from .trg).
                if let Some(body) = trigger.dg_body.as_deref() {
                    let outcome = super::dg::fire_item_dg(
                        body,
                        &item,
                        &connection_id,
                        cloned_db.clone(),
                        cloned_conns.clone(),
                        trigger.authored_by.clone(),
                        trigger.elevated,
                        ctx_strmap.clone(),
                    );
                    if matches!(outcome, super::dg::Outcome::Return(0)) {
                        return "cancel".to_string();
                    }
                    continue;
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
                        Ok(gt) => rhai::Dynamic::from(crate::script::characters::build_game_time_map(
                            &gt,
                            crate::types::ClimateProfile::Temperate,
                        )),
                        Err(_) => rhai::Dynamic::UNIT,
                    }
                });

                // get_local_game_time(connection_id): same map shape, but
                // weather + temperature projected through the player's
                // current-room area climate. Use from item triggers (watch
                // displays, etc) so a tropical-island player never sees the
                // global blizzard text.
                let db_for_local = cloned_db.clone();
                let conns_for_local = cloned_conns.clone();
                trigger_engine.register_fn(
                    "get_local_game_time",
                    move |connection_id: String| -> rhai::Dynamic {
                        let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                            Ok(u) => u,
                            Err(_) => return rhai::Dynamic::UNIT,
                        };
                        let room_id = {
                            let conns_guard = match conns_for_local.lock() {
                                Ok(g) => g,
                                Err(_) => return rhai::Dynamic::UNIT,
                            };
                            match conns_guard.get(&conn_uuid).and_then(|s| s.character.as_ref()) {
                                Some(c) => c.current_room_id,
                                None => return rhai::Dynamic::UNIT,
                            }
                        };
                        let climate = match db_for_local.get_room_data(&room_id) {
                            Ok(Some(room)) => db_for_local.room_climate(&room),
                            _ => crate::types::ClimateProfile::default(),
                        };
                        match db_for_local.get_game_time() {
                            Ok(gt) => rhai::Dynamic::from(
                                crate::script::characters::build_game_time_map(&gt, climate),
                            ),
                            Err(_) => rhai::Dynamic::UNIT,
                        }
                    },
                );

                // get_room_game_time(room_id): same map shape, projected
                // through the given room's area climate. Use from
                // environmental triggers (on_weather_change, on_time_change)
                // where a room_id is in scope but no specific player is.
                let db_for_room = cloned_db.clone();
                trigger_engine.register_fn(
                    "get_room_game_time",
                    move |room_id: String| -> rhai::Dynamic {
                        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                            Ok(u) => u,
                            Err(_) => return rhai::Dynamic::UNIT,
                        };
                        let climate = match db_for_room.get_room_data(&room_uuid) {
                            Ok(Some(room)) => db_for_room.room_climate(&room),
                            _ => crate::types::ClimateProfile::default(),
                        };
                        match db_for_room.get_game_time() {
                            Ok(gt) => rhai::Dynamic::from(
                                crate::script::characters::build_game_time_map(&gt, climate),
                            ),
                            Err(_) => rhai::Dynamic::UNIT,
                        }
                    },
                );

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
        MobileTriggerType::OnFight => "on_fight".to_string(),
        MobileTriggerType::OnHitPercent => "on_hitprcnt".to_string(),
        MobileTriggerType::OnReceive => "on_receive".to_string(),
        MobileTriggerType::OnBribe => "on_bribe".to_string(),
        MobileTriggerType::OnLoad => "on_load".to_string(),
        MobileTriggerType::OnCommand => "on_command".to_string(),
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
            let ttype = match parse_mobile_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
            };
            mobile.triggers.push(MobileTrigger {
                trigger_type: ttype,
                script_name,
                enabled: true,
                chance: 100,
                args: Vec::new(),
                interval_secs: 60,
                last_fired: 0,
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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
            let ttype = match parse_mobile_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return false,
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
                dg_body: None,
                dg_name: None,
                authored_by: None,
                elevated: false,
                source_proto_vnum: None,
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

            let trigger_type = match parse_mobile_trigger_type(&trigger_type_str) {
                Some(t) => t,
                None => return "continue".to_string(),
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

            // Convert the rhai context map once — shared between DG bodies
            // (passed into EvalCtx.context_vars) and the legacy @template
            // branch.
            let ctx_strmap = rhai_context_to_strmap(&context);

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

                // DG Scripts body (imported from .trg).
                if let Some(body) = trigger.dg_body.as_deref() {
                    let outcome = super::dg::fire_mobile_dg(
                        body,
                        &mobile,
                        &connection_id,
                        cloned_db.clone(),
                        cloned_conns.clone(),
                        trigger.authored_by.clone(),
                        trigger.elevated,
                        ctx_strmap.clone(),
                    );
                    if matches!(outcome, super::dg::Outcome::Return(0)) {
                        return "cancel".to_string();
                    }
                    continue;
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

    // ========== DG-trigger helpers (Phase 4 builder UX) ==========

    // get_mobile_trigger_dg_body(mobile_id, index) -> body string ("" when none).
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_trigger_dg_body", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_mobile_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_body.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_item_trigger_dg_body", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_item_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_body.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_room_trigger_dg_body", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_room_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_body.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_*_trigger_dg_name -> human-readable name (used in `trigger dg list`).
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_trigger_dg_name", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_mobile_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_name.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_item_trigger_dg_name", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_item_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_name.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_room_trigger_dg_name", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_room_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.dg_name.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_*_trigger_authored_by(host_id, index) -> author name ("" when None).
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_trigger_authored_by", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_mobile_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.authored_by.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_item_trigger_authored_by", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_item_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.authored_by.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_room_trigger_authored_by", move |id: String, index: i64| {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return String::new() };
        match cloned_db.get_room_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).and_then(|t| t.authored_by.clone()).unwrap_or_default(),
            _ => String::new(),
        }
    });

    // get_*_trigger_elevated -> bool. Reflects whether the trigger has
    // been admin-marked to bypass the per-author area gate on dangerous
    // DG opcodes (force/at/purge/load/teleport).
    let cloned_db = db.clone();
    engine.register_fn("get_mobile_trigger_elevated", move |id: String, index: i64| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        match cloned_db.get_mobile_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).map(|t| t.elevated).unwrap_or(false),
            _ => false,
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_item_trigger_elevated", move |id: String, index: i64| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        match cloned_db.get_item_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).map(|t| t.elevated).unwrap_or(false),
            _ => false,
        }
    });
    let cloned_db = db.clone();
    engine.register_fn("get_room_trigger_elevated", move |id: String, index: i64| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        match cloned_db.get_room_data(&uid) {
            Ok(Some(host)) => host.triggers.get(index as usize).map(|t| t.elevated).unwrap_or(false),
            _ => false,
        }
    });

    // set_*_trigger_elevated(host_id, index, on) -> bool (true on success).
    // Caller is responsible for the admin gate; these helpers just write
    // the field. The `trigger dg elevate` subcommand checks `is_admin`
    // before invoking these.
    let cloned_db = db.clone();
    engine.register_fn("set_mobile_trigger_elevated", move |id: String, index: i64, on: bool| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        let mut ok = false;
        let _ = cloned_db.update_mobile(&uid, |m| {
            if let Some(t) = m.triggers.get_mut(index as usize) {
                t.elevated = on;
                ok = true;
            }
        });
        ok
    });
    let cloned_db = db.clone();
    engine.register_fn("set_item_trigger_elevated", move |id: String, index: i64, on: bool| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        let mut ok = false;
        let _ = cloned_db.update_item(&uid, |it| {
            if let Some(t) = it.triggers.get_mut(index as usize) {
                t.elevated = on;
                ok = true;
            }
        });
        ok
    });
    let cloned_db = db.clone();
    engine.register_fn("set_room_trigger_elevated", move |id: String, index: i64, on: bool| -> bool {
        let uid = match uuid::Uuid::parse_str(&id) { Ok(u) => u, Err(_) => return false };
        let mut ok = false;
        let _ = cloned_db.update_room(&uid, |r| {
            if let Some(t) = r.triggers.get_mut(index as usize) {
                t.elevated = on;
                ok = true;
            }
        });
        ok
    });

    // Append a new dg-bodied trigger and return its index (-1 on failure).
    let cloned_db = db.clone();
    engine.register_fn(
        "add_mobile_dg_trigger",
        move |mobile_id: String, trigger_type: String, name: String| {
            let uid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return -1i64,
            };
            let ttype = match parse_mobile_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return -1,
            };
            let mut idx_out: i64 = -1;
            let _ = cloned_db.update_mobile(&uid, |m| {
                m.triggers.push(MobileTrigger {
                    trigger_type: ttype,
                    script_name: String::new(),
                    enabled: true,
                    chance: 100,
                    args: Vec::new(),
                    interval_secs: 60,
                    last_fired: 0,
                    dg_body: Some(String::new()),
                    dg_name: if name.is_empty() { None } else { Some(name.clone()) },
                    authored_by: None,
                    elevated: false,
                    source_proto_vnum: None,
                });
                idx_out = (m.triggers.len() as i64) - 1;
            });
            idx_out
        },
    );
    let cloned_db = db.clone();
    engine.register_fn(
        "add_item_dg_trigger",
        move |item_id: String, trigger_type: String, name: String| {
            let uid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return -1i64,
            };
            let ttype = match parse_item_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return -1,
            };
            let mut idx_out: i64 = -1;
            let _ = cloned_db.update_item(&uid, |it| {
                it.triggers.push(ItemTrigger {
                    trigger_type: ttype,
                    script_name: String::new(),
                    enabled: true,
                    chance: 100,
                    args: Vec::new(),
                    dg_body: Some(String::new()),
                    dg_name: if name.is_empty() { None } else { Some(name.clone()) },
                    authored_by: None,
                    elevated: false,
                    source_proto_vnum: None,
                });
                idx_out = (it.triggers.len() as i64) - 1;
            });
            idx_out
        },
    );
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_dg_trigger",
        move |room_id: String, trigger_type: String, name: String| {
            let uid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return -1i64,
            };
            let ttype = match parse_room_trigger_type(&trigger_type) {
                Some(t) => t,
                None => return -1,
            };
            let mut idx_out: i64 = -1;
            let _ = cloned_db.update_room(&uid, |r| {
                r.triggers.push(RoomTrigger {
                    trigger_type: ttype,
                    script_name: String::new(),
                    enabled: true,
                    interval_secs: 60,
                    last_fired: 0,
                    chance: 100,
                    args: Vec::new(),
                    dg_body: Some(String::new()),
                    dg_name: if name.is_empty() { None } else { Some(name.clone()) },
                    authored_by: None,
                    elevated: false,
                    source_proto_vnum: None,
                });
                idx_out = (r.triggers.len() as i64) - 1;
            });
            idx_out
        },
    );

    // set_*_dg_trigger_type(id, index, new_type) -> String
    // Change the trigger type without touching the body. Returns "" on
    // success, an error message on failure (bad UUID, out-of-range index,
    // unknown type, proto-attached instance, or non-DG trigger).
    let cloned_db = db.clone();
    engine.register_fn(
        "set_mobile_dg_trigger_type",
        move |mobile_id: String, index: i64, new_type: String| -> String {
            let uid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return "invalid mobile id".to_string(),
            };
            let ttype = match parse_mobile_trigger_type(&new_type) {
                Some(t) => t,
                None => return format!("unknown trigger type '{}'", new_type),
            };
            let mut err = String::new();
            let _ = cloned_db.update_mobile(&uid, |m| {
                let Some(t) = m.triggers.get_mut(index as usize) else {
                    err = format!("no trigger at index {}", index);
                    return;
                };
                if t.dg_body.is_none() {
                    err = "trigger at that index has no DG body (use trigger remove + re-add for template triggers)".to_string();
                    return;
                }
                if let Some(proto) = &t.source_proto_vnum {
                    err = format!(
                        "trigger is attached from proto '{}'; detach first or edit the proto",
                        proto
                    );
                    return;
                }
                t.trigger_type = ttype;
            });
            err
        },
    );
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_dg_trigger_type",
        move |item_id: String, index: i64, new_type: String| -> String {
            let uid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return "invalid item id".to_string(),
            };
            let ttype = match parse_item_trigger_type(&new_type) {
                Some(t) => t,
                None => return format!("unknown trigger type '{}'", new_type),
            };
            let mut err = String::new();
            let _ = cloned_db.update_item(&uid, |it| {
                let Some(t) = it.triggers.get_mut(index as usize) else {
                    err = format!("no trigger at index {}", index);
                    return;
                };
                if t.dg_body.is_none() {
                    err = "trigger at that index has no DG body (use trigger remove + re-add for template triggers)".to_string();
                    return;
                }
                if let Some(proto) = &t.source_proto_vnum {
                    err = format!(
                        "trigger is attached from proto '{}'; detach first or edit the proto",
                        proto
                    );
                    return;
                }
                t.trigger_type = ttype;
            });
            err
        },
    );
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_dg_trigger_type",
        move |room_id: String, index: i64, new_type: String| -> String {
            let uid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return "invalid room id".to_string(),
            };
            let ttype = match parse_room_trigger_type(&new_type) {
                Some(t) => t,
                None => return format!("unknown trigger type '{}'", new_type),
            };
            let mut err = String::new();
            let _ = cloned_db.update_room(&uid, |r| {
                let Some(t) = r.triggers.get_mut(index as usize) else {
                    err = format!("no trigger at index {}", index);
                    return;
                };
                if t.dg_body.is_none() {
                    err = "trigger at that index has no DG body (use trigger remove + re-add for template triggers)".to_string();
                    return;
                }
                if let Some(proto) = &t.source_proto_vnum {
                    err = format!(
                        "trigger is attached from proto '{}'; detach first or edit the proto",
                        proto
                    );
                    return;
                }
                t.trigger_type = ttype;
            });
            err
        },
    );

    // attach_dg_trigger_proto_to_<kind>(target_id, vnum) -> bool.
    // Resolves the prototype from the dg_trigger_protos sled tree and
    // pushes a fully-bodied trigger onto the target's triggers list.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    engine.register_fn(
        "attach_dg_trigger_proto",
        move |target_id: String, vnum: String| {
            let uid = match uuid::Uuid::parse_str(&target_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            // Build a minimal EvalCtx — host kind comes from the proto.
            let proto = match cloned_db.get_dg_trigger_proto(vnum.trim()) {
                Ok(Some(p)) => p,
                _ => return false,
            };
            let (kind, name) = (proto.attach_kind, target_id.clone());
            let self_kind = match kind {
                crate::types::DgAttachKind::Mob => crate::script::dg::SelfKind::Mob,
                crate::types::DgAttachKind::Obj => crate::script::dg::SelfKind::Obj,
                crate::types::DgAttachKind::Room => crate::script::dg::SelfKind::Room,
            };
            let ctx = crate::script::dg::EvalCtx {
                db: cloned_db.clone(),
                connections: cloned_conns.clone(),
                self_kind,
                self_id: uid,
                self_name: String::new(),
                self_vnum: String::new(),
                self_room: None,
                actor: None,
                victim: None,
                arg: String::new(),
                cmd: String::new(),
                cmd_canonical: String::new(),
                context_vars: std::collections::HashMap::new(),
                authored_by: None,
                elevated: false,
                #[cfg(test)]
                test_temp_dir: None,
            };
            let _ = name;
            crate::script::dg::cmds::attach_trigger_proto(&proto.vnum, &uid.to_string(), &ctx);
            true
        },
    );

    // list_dg_trigger_protos() -> Array of map { vnum, name, kind, flags }.
    let cloned_db = db.clone();
    engine.register_fn("list_dg_trigger_protos", move || {
        let mut out: rhai::Array = Vec::new();
        if let Ok(list) = cloned_db.list_dg_trigger_protos() {
            for p in list {
                let mut m = rhai::Map::new();
                m.insert("vnum".into(), p.vnum.clone().into());
                m.insert("name".into(), p.name.clone().into());
                m.insert(
                    "kind".into(),
                    match p.attach_kind {
                        crate::types::DgAttachKind::Mob => "mob",
                        crate::types::DgAttachKind::Obj => "obj",
                        crate::types::DgAttachKind::Room => "room",
                    }
                    .to_string()
                    .into(),
                );
                m.insert("flags".into(), p.flags.clone().into());
                out.push(rhai::Dynamic::from(m));
            }
        }
        out
    });

    // === DG trigger proto editor surface ===
    //
    // Powers the `trigger dg proto new/view/edit/delete` + `makeproto` /
    // `detach` subcommands in scripts/lib/dg_olc.rhai. Each helper is
    // intentionally small so the Rhai layer stays declarative.

    // dg_proto_get(vnum) -> Map { vnum, name, kind, flags, body, attached }
    //   or () if no such proto. `attached` is a count of live instances
    //   carrying this proto's source_proto_vnum across the matching kind.
    let cloned_db = db.clone();
    engine.register_fn("dg_proto_get", move |vnum: String| -> rhai::Dynamic {
        let proto = match cloned_db.get_dg_trigger_proto(vnum.trim()) {
            Ok(Some(p)) => p,
            _ => return rhai::Dynamic::UNIT,
        };
        let target = Some(proto.vnum.as_str());
        let attached: i64 = match proto.attach_kind {
            crate::types::DgAttachKind::Mob => cloned_db
                .list_all_mobiles()
                .unwrap_or_default()
                .iter()
                .flat_map(|m| m.triggers.iter())
                .filter(|t| t.source_proto_vnum.as_deref() == target)
                .count() as i64,
            crate::types::DgAttachKind::Obj => cloned_db
                .list_all_items()
                .unwrap_or_default()
                .iter()
                .flat_map(|i| i.triggers.iter())
                .filter(|t| t.source_proto_vnum.as_deref() == target)
                .count() as i64,
            crate::types::DgAttachKind::Room => cloned_db
                .list_all_rooms()
                .unwrap_or_default()
                .iter()
                .flat_map(|r| r.triggers.iter())
                .filter(|t| t.source_proto_vnum.as_deref() == target)
                .count() as i64,
        };
        let mut m = rhai::Map::new();
        m.insert("vnum".into(), proto.vnum.into());
        m.insert("name".into(), proto.name.into());
        m.insert(
            "kind".into(),
            match proto.attach_kind {
                crate::types::DgAttachKind::Mob => "mob",
                crate::types::DgAttachKind::Obj => "obj",
                crate::types::DgAttachKind::Room => "room",
            }
            .to_string()
            .into(),
        );
        m.insert("flags".into(), proto.flags.into());
        m.insert("body".into(), proto.body.into());
        m.insert("attached".into(), attached.into());
        rhai::Dynamic::from(m)
    });

    // dg_proto_new(vnum, kind, flags, name) -> bool
    //   Creates an empty-bodied proto in the registry. Refuses if the vnum
    //   already exists (use dg_proto_save_body to overwrite an existing
    //   proto). `kind` is "mob"|"obj"|"room"; `flags` is a letter string
    //   (e.g. "cw" for OnCommand+OnWear on an item).
    let cloned_db = db.clone();
    engine.register_fn(
        "dg_proto_new",
        move |vnum: String, kind: String, flags: String, name: String| -> bool {
            let v = vnum.trim();
            if v.is_empty() {
                return false;
            }
            if matches!(cloned_db.get_dg_trigger_proto(v), Ok(Some(_))) {
                return false;
            }
            let attach_kind = match kind.to_lowercase().as_str() {
                "mob" | "mobile" => crate::types::DgAttachKind::Mob,
                "obj" | "item" => crate::types::DgAttachKind::Obj,
                "room" => crate::types::DgAttachKind::Room,
                _ => return false,
            };
            let proto = crate::types::DgTriggerProto {
                vnum: v.to_string(),
                name: name.trim().to_string(),
                attach_kind,
                flags: flags.trim().to_string(),
                numeric_arg: 100,
                arglist: String::new(),
                body: String::new(),
            };
            cloned_db.save_dg_trigger_proto(&proto).is_ok()
        },
    );

    // dg_proto_save_body(vnum, body) -> Map { ok, refreshed, error, warnings }
    //   Run-the-analyzer + persist + refresh path. On ParseError, save is
    //   refused and attached instances are unchanged (ok=false, error set).
    //   Non-fatal issues come back as warnings. Used by the
    //   collecting_dg_proto_body OLC mode on .save.
    let cloned_db = db.clone();
    engine.register_fn(
        "dg_proto_save_body",
        move |vnum: String, body: String| -> rhai::Map {
            let mut result = rhai::Map::new();
            let mut proto = match cloned_db.get_dg_trigger_proto(vnum.trim()) {
                Ok(Some(p)) => p,
                _ => {
                    result.insert("ok".into(), false.into());
                    result.insert("error".into(), "unknown proto vnum".to_string().into());
                    result.insert("refreshed".into(), 0i64.into());
                    result.insert("warnings".into(), rhai::Array::new().into());
                    return result;
                }
            };
            proto.body = body;
            match cloned_db.save_dg_trigger_proto_with_refresh(&proto) {
                Ok((refreshed, warnings)) => {
                    let warn_arr: rhai::Array =
                        warnings.into_iter().map(rhai::Dynamic::from).collect();
                    result.insert("ok".into(), true.into());
                    result.insert("error".into(), "".to_string().into());
                    result.insert("refreshed".into(), (refreshed as i64).into());
                    result.insert("warnings".into(), warn_arr.into());
                }
                Err(e) => {
                    result.insert("ok".into(), false.into());
                    result.insert("error".into(), e.to_string().into());
                    result.insert("refreshed".into(), 0i64.into());
                    result.insert("warnings".into(), rhai::Array::new().into());
                }
            }
            result
        },
    );

    // dg_proto_set_meta(vnum, name, flags) -> bool
    //   Update proto name and/or flags without touching the body. Flag
    //   changes still trigger a refresh sweep (structural — adds/removes
    //   trigger types on attached instances). Pass "" to leave a field as-is.
    let cloned_db = db.clone();
    engine.register_fn(
        "dg_proto_set_meta",
        move |vnum: String, name: String, flags: String| -> bool {
            let mut proto = match cloned_db.get_dg_trigger_proto(vnum.trim()) {
                Ok(Some(p)) => p,
                _ => return false,
            };
            let mut changed = false;
            if !name.is_empty() {
                proto.name = name.trim().to_string();
                changed = true;
            }
            if !flags.is_empty() {
                proto.flags = flags.trim().to_string();
                changed = true;
            }
            if !changed {
                return true;
            }
            if cloned_db.save_dg_trigger_proto(&proto).is_err() {
                return false;
            }
            let _ = cloned_db.refresh_attached_dg_triggers(&proto);
            true
        },
    );

    // dg_proto_set_flags(vnum, new_flags) -> Map { ok, error, refreshed }
    //   Mutate a proto's letter-flag string and re-derive trigger types on
    //   all attached instances via the refresh sweep. Validates that the
    //   new flag string parses to at least one type for the proto's kind
    //   (rejects e.g. `proto_set_flags mob_proto "xyz"`). Returns `ok=true`
    //   with `refreshed` count on success; `ok=false` with `error` set on
    //   unknown vnum, empty/invalid flags, or save failure.
    let cloned_db = db.clone();
    engine.register_fn(
        "dg_proto_set_flags",
        move |vnum: String, new_flags: String| -> rhai::Map {
            let mut result = rhai::Map::new();
            let new_flags = new_flags.trim().to_string();
            if new_flags.is_empty() {
                result.insert("ok".into(), false.into());
                result.insert("error".into(), "flags cannot be empty".to_string().into());
                return result;
            }
            let mut proto = match cloned_db.get_dg_trigger_proto(vnum.trim()) {
                Ok(Some(p)) => p,
                _ => {
                    result.insert("ok".into(), false.into());
                    result.insert("error".into(), "unknown proto vnum".to_string().into());
                    return result;
                }
            };
            // Validate flags parse to at least one trigger type for this kind.
            use crate::import::engines::tba::trg_map;
            let valid = match proto.attach_kind {
                crate::types::DgAttachKind::Mob => {
                    !trg_map::mobile_trigger_types(&new_flags).is_empty()
                }
                crate::types::DgAttachKind::Obj => {
                    !trg_map::item_trigger_types(&new_flags).is_empty()
                }
                crate::types::DgAttachKind::Room => {
                    !trg_map::room_trigger_types(&new_flags).is_empty()
                }
            };
            if !valid {
                result.insert("ok".into(), false.into());
                result.insert(
                    "error".into(),
                    format!("flags '{}' don't map to any valid trigger type for this kind", new_flags).into(),
                );
                return result;
            }
            proto.flags = new_flags;
            if cloned_db.save_dg_trigger_proto(&proto).is_err() {
                result.insert("ok".into(), false.into());
                result.insert("error".into(), "save failed".to_string().into());
                return result;
            }
            let refreshed = cloned_db
                .refresh_attached_dg_triggers(&proto)
                .unwrap_or(0);
            result.insert("ok".into(), true.into());
            result.insert("refreshed".into(), (refreshed as i64).into());
            result
        },
    );

    // dg_proto_delete(vnum) -> int (count of orphaned trigger instances)
    //   Removes the proto from the registry and orphans every attached
    //   instance (clears source_proto_vnum, preserves body). Returns -1
    //   if the vnum is unknown.
    let cloned_db = db.clone();
    engine.register_fn("dg_proto_delete", move |vnum: String| -> i64 {
        if matches!(cloned_db.get_dg_trigger_proto(vnum.trim()), Ok(None)) {
            return -1;
        }
        let orphaned = cloned_db
            .orphan_attached_dg_triggers(vnum.trim())
            .unwrap_or(0);
        let _ = cloned_db.delete_dg_trigger_proto(vnum.trim());
        orphaned as i64
    });

    // dg_makeproto_from_mobile_trigger(mob_id, idx, vnum) -> bool
    // dg_makeproto_from_item_trigger(item_id, idx, vnum) -> bool
    // dg_makeproto_from_room_trigger(room_id, idx, vnum) -> bool
    //   Promote a host-local DG-bodied trigger to a registry proto with
    //   the given vnum. Sets source_proto_vnum on the original so future
    //   edits route through the proto's save+refresh path. Refuses if
    //   the trigger has no dg_body, or if the vnum already exists.

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_makeproto_from_mobile_trigger",
        move |mob_id: String, idx: i64, vnum: String| -> bool {
            let uid = match uuid::Uuid::parse_str(&mob_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let v = vnum.trim();
            if v.is_empty() || matches!(cloned_db.get_dg_trigger_proto(v), Ok(Some(_))) {
                return false;
            }
            let mob = match cloned_db.get_mobile_data(&uid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let i = idx as usize;
            let Some(t) = mob.triggers.get(i) else { return false; };
            let Some(body) = t.dg_body.clone() else { return false; };
            let flags = crate::import::engines::tba::trg_map::flags_for_mobile_trigger(t.trigger_type);
            let proto = crate::types::DgTriggerProto {
                vnum: v.to_string(),
                name: t.dg_name.clone().unwrap_or_default(),
                attach_kind: crate::types::DgAttachKind::Mob,
                flags,
                numeric_arg: t.chance,
                arglist: t.args.join(" "),
                body,
            };
            if cloned_db.save_dg_trigger_proto(&proto).is_err() {
                return false;
            }
            cloned_db
                .update_mobile(&uid, |m| {
                    if let Some(t) = m.triggers.get_mut(i) {
                        t.source_proto_vnum = Some(v.to_string());
                    }
                })
                .is_ok()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_makeproto_from_item_trigger",
        move |item_id: String, idx: i64, vnum: String| -> bool {
            let uid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let v = vnum.trim();
            if v.is_empty() || matches!(cloned_db.get_dg_trigger_proto(v), Ok(Some(_))) {
                return false;
            }
            let item = match cloned_db.get_item_data(&uid) {
                Ok(Some(it)) => it,
                _ => return false,
            };
            let i = idx as usize;
            let Some(t) = item.triggers.get(i) else { return false; };
            let Some(body) = t.dg_body.clone() else { return false; };
            let flags = crate::import::engines::tba::trg_map::flags_for_item_trigger(t.trigger_type);
            let proto = crate::types::DgTriggerProto {
                vnum: v.to_string(),
                name: t.dg_name.clone().unwrap_or_default(),
                attach_kind: crate::types::DgAttachKind::Obj,
                flags,
                numeric_arg: t.chance,
                arglist: t.args.join(" "),
                body,
            };
            if cloned_db.save_dg_trigger_proto(&proto).is_err() {
                return false;
            }
            cloned_db
                .update_item(&uid, |it| {
                    if let Some(t) = it.triggers.get_mut(i) {
                        t.source_proto_vnum = Some(v.to_string());
                    }
                })
                .is_ok()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_makeproto_from_room_trigger",
        move |room_id: String, idx: i64, vnum: String| -> bool {
            let uid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let v = vnum.trim();
            if v.is_empty() || matches!(cloned_db.get_dg_trigger_proto(v), Ok(Some(_))) {
                return false;
            }
            let room = match cloned_db.get_room_data(&uid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let i = idx as usize;
            let Some(t) = room.triggers.get(i) else { return false; };
            let Some(body) = t.dg_body.clone() else { return false; };
            let flags = crate::import::engines::tba::trg_map::flags_for_room_trigger(t.trigger_type);
            let proto = crate::types::DgTriggerProto {
                vnum: v.to_string(),
                name: t.dg_name.clone().unwrap_or_default(),
                attach_kind: crate::types::DgAttachKind::Room,
                flags,
                numeric_arg: t.chance,
                arglist: t.args.join(" "),
                body,
            };
            if cloned_db.save_dg_trigger_proto(&proto).is_err() {
                return false;
            }
            cloned_db
                .update_room(&uid, |r| {
                    if let Some(t) = r.triggers.get_mut(i) {
                        t.source_proto_vnum = Some(v.to_string());
                    }
                })
                .is_ok()
        },
    );

    // dg_detach_<kind>_trigger(host_id, idx) -> bool
    //   Clear source_proto_vnum on a single attached trigger so future
    //   edits stay local (and refresh sweeps skip this instance). Body
    //   preserved.

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_detach_mobile_trigger",
        move |mob_id: String, idx: i64| -> bool {
            let uid = match uuid::Uuid::parse_str(&mob_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .update_mobile(&uid, |m| {
                    if let Some(t) = m.triggers.get_mut(idx as usize) {
                        t.source_proto_vnum = None;
                    }
                })
                .is_ok()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_detach_item_trigger",
        move |item_id: String, idx: i64| -> bool {
            let uid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .update_item(&uid, |i| {
                    if let Some(t) = i.triggers.get_mut(idx as usize) {
                        t.source_proto_vnum = None;
                    }
                })
                .is_ok()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "dg_detach_room_trigger",
        move |room_id: String, idx: i64| -> bool {
            let uid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            cloned_db
                .update_room(&uid, |r| {
                    if let Some(t) = r.triggers.get_mut(idx as usize) {
                        t.source_proto_vnum = None;
                    }
                })
                .is_ok()
        },
    );

    // get_<kind>_trigger_source_proto(host_id, idx) -> String
    //   Returns the source_proto_vnum on an attached trigger (empty when
    //   the trigger is host-local). Used by the editor flow to detect
    //   edit-through cases on `trigger dg edit <idx>`.

    let cloned_db = db.clone();
    engine.register_fn(
        "get_mobile_trigger_source_proto",
        move |mob_id: String, idx: i64| -> String {
            let uid = match uuid::Uuid::parse_str(&mob_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            cloned_db
                .get_mobile_data(&uid)
                .ok()
                .flatten()
                .and_then(|m| m.triggers.get(idx as usize).and_then(|t| t.source_proto_vnum.clone()))
                .unwrap_or_default()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "get_item_trigger_source_proto",
        move |item_id: String, idx: i64| -> String {
            let uid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            cloned_db
                .get_item_data(&uid)
                .ok()
                .flatten()
                .and_then(|i| i.triggers.get(idx as usize).and_then(|t| t.source_proto_vnum.clone()))
                .unwrap_or_default()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "get_room_trigger_source_proto",
        move |room_id: String, idx: i64| -> String {
            let uid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            cloned_db
                .get_room_data(&uid)
                .ok()
                .flatten()
                .and_then(|r| r.triggers.get(idx as usize).and_then(|t| t.source_proto_vnum.clone()))
                .unwrap_or_default()
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

    let trigger_type = match parse_mobile_trigger_type(trigger_type_str) {
        Some(t) => t,
        None => return "continue".to_string(),
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
