// src/script/medical.rs
// Medical treatment system functions

use crate::SharedConnections;
use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Register medical-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Medical Tool Functions ==========

    // is_medical_tool(item_id) -> bool
    // Checks if an item is a medical tool
    let cloned_db = db.clone();
    engine.register_fn("is_medical_tool", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                return item.flags.medical_tool;
            }
        }
        false
    });

    // get_medical_tool_tier(item_id) -> i64
    // Returns the tier of a medical tool (1=basic, 2=intermediate, 3=advanced)
    let cloned_db = db.clone();
    engine.register_fn("get_medical_tool_tier", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if item.flags.medical_tool {
                    return item.medical_tier as i64;
                }
            }
        }
        0
    });

    // get_medical_tool_uses(item_id) -> i64
    // Returns remaining uses (0 = reusable/infinite)
    let cloned_db = db.clone();
    engine.register_fn("get_medical_tool_uses", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if item.flags.medical_tool {
                    return item.medical_uses as i64;
                }
            }
        }
        0
    });

    // consume_medical_tool(item_id) -> bool
    // Decrements uses, removes item if uses reach 0. Returns true if successful.
    let cloned_db = db.clone();
    engine.register_fn("consume_medical_tool", move |item_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                if item.flags.medical_tool {
                    if item.medical_uses == 0 {
                        // Reusable tool, don't consume
                        return true;
                    }
                    item.medical_uses -= 1;
                    if item.medical_uses <= 0 {
                        // Tool exhausted, delete it
                        let _ = cloned_db.delete_item(&uuid);
                        return true;
                    }
                    let _ = cloned_db.save_item_data(item);
                    return true;
                }
            }
        }
        false
    });

    // can_tool_treat_wound_type(item_id, wound_type) -> bool
    // Checks if a medical tool can treat a specific wound type
    let cloned_db = db.clone();
    engine.register_fn(
        "can_tool_treat_wound_type",
        move |item_id: String, wound_type: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                    if item.flags.medical_tool {
                        // Empty treats_wound_types means treats all types
                        if item.treats_wound_types.is_empty() {
                            return true;
                        }
                        return item
                            .treats_wound_types
                            .iter()
                            .any(|t| t.to_lowercase() == wound_type.to_lowercase());
                    }
                }
            }
            false
        },
    );

    // get_tool_max_wound_level(item_id) -> String
    // Returns the maximum wound level this tool can treat ("minor", "moderate", "severe", "critical")
    let cloned_db = db.clone();
    engine.register_fn("get_tool_max_wound_level", move |item_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if item.flags.medical_tool && !item.max_treatable_wound.is_empty() {
                    return item.max_treatable_wound.clone();
                }
            }
        }
        "critical".to_string() // Default to treating all wound levels
    });

    // get_tool_quality_bonus(item_id) -> i64
    // Returns the quality bonus for treatment success (quality / 20, so 0-5)
    let cloned_db = db.clone();
    engine.register_fn("get_tool_quality_bonus", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            if let Ok(Some(item)) = cloned_db.get_item_data(&uuid) {
                if item.flags.medical_tool {
                    return (item.quality / 20) as i64;
                }
            }
        }
        0
    });

    // ========== Helpline Functions ==========

    // is_helpline_enabled(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_helpline_enabled", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.helpline_enabled;
                }
            }
        }
        false
    });

    // set_helpline_enabled(connection_id, enabled) -> bool
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "set_helpline_enabled",
        move |connection_id: String, enabled: bool| -> bool {
            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    if let Some(ref mut char) = session.character {
                        char.helpline_enabled = enabled;
                        let _ = cloned_db.save_character_data(char.clone());
                        return true;
                    }
                }
            }
            false
        },
    );

    // broadcast_to_helpline(message, room_name, area_name) -> i64
    // Broadcasts a message to all players with helpline enabled. Returns count of recipients.
    let conns = connections.clone();
    engine.register_fn(
        "broadcast_to_helpline",
        move |message: String, room_name: String, area_name: String| -> i64 {
            let mut count = 0i64;
            let conns_lock = conns.lock().unwrap();

            let location = if area_name.is_empty() {
                room_name.clone()
            } else {
                format!("{} ({})", room_name, area_name)
            };

            let formatted = format!("[HELPLINE] {} Location: {}\n", message, location);

            for session in conns_lock.values() {
                if let Some(ref char) = session.character {
                    if char.helpline_enabled {
                        let _ = session.sender.send(formatted.clone());
                        count += 1;
                    }
                }
            }
            count
        },
    );

    // ========== Weather Exposure Functions ==========

    // is_character_wet(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_character_wet", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.is_wet;
                }
            }
        }
        false
    });

    // get_wet_level(connection_id) -> i64
    let conns = connections.clone();
    engine.register_fn("get_wet_level", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.wet_level as i64;
                }
            }
        }
        0
    });

    // apply_wet_status(connection_id, amount) -> bool
    // Adds wet_level to a character
    let conns = connections.clone();
    engine.register_fn("apply_wet_status", move |connection_id: String, amount: i64| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    char.wet_level = (char.wet_level + amount as i32).clamp(0, 100);
                    char.is_wet = char.wet_level > 0;
                    return true;
                }
            }
        }
        false
    });

    // dry_character(connection_id, amount) -> bool
    // Reduces wet_level
    let conns = connections.clone();
    engine.register_fn("dry_character", move |connection_id: String, amount: i64| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    char.wet_level = (char.wet_level - amount as i32).max(0);
                    char.is_wet = char.wet_level > 0;
                    return true;
                }
            }
        }
        false
    });

    // get_cold_exposure(connection_id) -> i64
    let conns = connections.clone();
    engine.register_fn("get_cold_exposure", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.cold_exposure as i64;
                }
            }
        }
        0
    });

    // get_heat_exposure(connection_id) -> i64
    let conns = connections.clone();
    engine.register_fn("get_heat_exposure", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.heat_exposure as i64;
                }
            }
        }
        0
    });

    // has_condition(connection_id, condition) -> bool
    // Check if character has a specific condition ("hypothermia", "frostbite", "heat_exhaustion", "heat_stroke", "illness")
    let conns = connections.clone();
    engine.register_fn(
        "has_condition",
        move |connection_id: String, condition: String| -> bool {
            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get(&conn_id) {
                    if let Some(ref char) = session.character {
                        return match condition.to_lowercase().as_str() {
                            "hypothermia" => char.has_hypothermia,
                            "frostbite" => !char.has_frostbite.is_empty(),
                            "heat_exhaustion" => char.has_heat_exhaustion,
                            "heat_stroke" => char.has_heat_stroke,
                            "illness" | "sick" | "cold" | "flu" => char.has_illness,
                            _ => false,
                        };
                    }
                }
            }
            false
        },
    );

    // get_illness_progress(connection_id) -> i64
    let conns = connections.clone();
    engine.register_fn("get_illness_progress", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    return char.illness_progress as i64;
                }
            }
        }
        0
    });

    // ========== Effective Insulation Calculation ==========

    // get_effective_insulation(connection_id) -> i64
    // Calculates total insulation from worn equipment, reduced by wet status
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("get_effective_insulation", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    let mut total_insulation = 0i32;

                    // Query database for equipped items (source of truth is ItemLocation::Equipped)
                    if let Ok(equipped_items) = cloned_db.get_equipped_items(&char.name) {
                        for item in equipped_items {
                            total_insulation += item.insulation;
                        }
                    }

                    // Reduce by wet level (wet_level / 2 percent reduction)
                    if char.is_wet && char.wet_level > 0 {
                        let reduction = (total_insulation * char.wet_level) / 200;
                        total_insulation -= reduction;
                    }

                    return total_insulation.max(0) as i64;
                }
            }
        }
        0
    });

    // has_waterproof_coverage(connection_id) -> bool
    // Checks if character has waterproof items equipped
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("has_waterproof_coverage", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref char) = session.character {
                    // Query database for equipped items (source of truth is ItemLocation::Equipped)
                    if let Ok(equipped_items) = cloned_db.get_equipped_items(&char.name) {
                        for item in equipped_items {
                            if item.flags.waterproof {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    });

    // ========== Treatment Functions ==========

    // calculate_treatment_success(skill_level, tool_tier, tool_quality, wound_level, is_self) -> i64
    // Returns success chance (0-100)
    engine.register_fn(
        "calculate_treatment_success",
        |skill_level: i64, tool_tier: i64, tool_quality: i64, wound_level: String, is_self: bool| -> i64 {
            // Base chance: 30 + (skill * 5)
            let mut chance = 30 + (skill_level * 5);

            // Tool tier bonus: +10 per tier
            chance += tool_tier * 10;

            // Quality bonus (quality / 20, so 0-5)
            chance += tool_quality;

            // Wound level penalty
            let wound_penalty = match wound_level.to_lowercase().as_str() {
                "minor" => 0,
                "moderate" => -10,
                "severe" => -20,
                "critical" => -30,
                "disabled" => -40,
                _ => 0,
            };
            chance += wound_penalty;

            // Self-treatment penalty
            if is_self {
                chance -= 20;
            }

            // Clamp to 5-95 range
            chance.clamp(5, 95)
        },
    );
}
