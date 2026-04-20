// src/script/healers.rs
// NPC healer system functions

use crate::SharedConnections;
use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Register healer-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Healer Identification Functions ==========

    // is_healer(mobile_id) -> bool
    // Checks if a mobile is a healer NPC
    let cloned_db = db.clone();
    engine.register_fn("is_healer", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.flags.healer;
            }
        }
        false
    });

    // get_healer_type(mobile_id) -> String
    // Returns the healer type: "medic", "herbalist", or "cleric"
    let cloned_db = db.clone();
    engine.register_fn("get_healer_type", move |mobile_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if mobile.flags.healer {
                    return mobile.healer_type.clone();
                }
            }
        }
        String::new()
    });

    // is_healing_free(mobile_id) -> bool
    // Checks if healing is free for this healer
    let cloned_db = db.clone();
    engine.register_fn("is_healing_free", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if mobile.flags.healer {
                    return mobile.healing_free;
                }
            }
        }
        false
    });

    // ========== Healer Finding Functions ==========

    // find_healer_in_room(room_id) -> MobileData or ()
    // Finds any healer in the room
    let cloned_db = db.clone();
    engine.register_fn("find_healer_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_mobiles_in_room(&uuid) {
                Ok(mobiles) => {
                    for mobile in mobiles {
                        if mobile.flags.healer && !mobile.is_prototype {
                            return rhai::Dynamic::from(mobile);
                        }
                    }
                    rhai::Dynamic::UNIT
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // find_healer_by_type(room_id, healer_type) -> MobileData or ()
    // Finds a healer of a specific type in the room
    let cloned_db = db.clone();
    engine.register_fn("find_healer_by_type", move |room_id: String, healer_type: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_mobiles_in_room(&uuid) {
                Ok(mobiles) => {
                    for mobile in mobiles {
                        if mobile.flags.healer
                            && !mobile.is_prototype
                            && mobile.healer_type.to_lowercase() == healer_type.to_lowercase()
                        {
                            return rhai::Dynamic::from(mobile);
                        }
                    }
                    rhai::Dynamic::UNIT
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // ========== Service Capability Functions ==========

    // can_healer_treat(mobile_id, service) -> bool
    // Checks if a healer can provide a specific service
    // Services: "minor_wound", "moderate_wound", "severe_wound", "bleeding",
    //           "poison", "illness", "restore_hp", "revive", "full_heal"
    let cloned_db = db.clone();
    engine.register_fn("can_healer_treat", move |mobile_id: String, service: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if !mobile.flags.healer {
                    return false;
                }

                let healer_type = mobile.healer_type.to_lowercase();
                let service_lower = service.to_lowercase();

                return match healer_type.as_str() {
                    "medic" => matches!(
                        service_lower.as_str(),
                        "minor_wound"
                            | "moderate_wound"
                            | "severe_wound"
                            | "bleeding"
                            | "broken_bone"
                            | "concussion"
                            | "severed_tendon"
                            | "impaled"
                            | "nerve_damage"
                            | "punctured_organ"
                    ),
                    "herbalist" => matches!(
                        service_lower.as_str(),
                        "minor_wound"
                            | "moderate_wound"
                            | "severe_wound"
                            | "poison"
                            | "illness"
                            | "burn"
                            | "frostbite"
                            | "severe_burn"
                            | "frozen_limb"
                            | "venom_surge"
                            | "acid_burn"
                            | "frostbitten"
                            | "charred"
                            | "toxic_shock"
                    ),
                    "cleric" => matches!(service_lower.as_str(), "restore_hp" | "revive" | "full_heal"),
                    _ => false,
                };
            }
        }
        false
    });

    // get_healer_services(mobile_id) -> Array of service names
    // Returns list of services this healer can provide
    let cloned_db = db.clone();
    engine.register_fn("get_healer_services", move |mobile_id: String| -> Vec<rhai::Dynamic> {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if !mobile.flags.healer {
                    return Vec::new();
                }

                let services: Vec<&str> = match mobile.healer_type.to_lowercase().as_str() {
                    "medic" => vec![
                        "minor_wound",
                        "moderate_wound",
                        "severe_wound",
                        "bleeding",
                        "broken_bone",
                        "concussion",
                        "severed_tendon",
                        "impaled",
                        "nerve_damage",
                        "punctured_organ",
                    ],
                    "herbalist" => vec![
                        "minor_wound",
                        "moderate_wound",
                        "severe_wound",
                        "poison",
                        "illness",
                        "burn",
                        "frostbite",
                        "severe_burn",
                        "frozen_limb",
                        "venom_surge",
                        "acid_burn",
                        "frostbitten",
                        "charred",
                        "toxic_shock",
                    ],
                    "cleric" => vec!["restore_hp", "revive", "full_heal"],
                    _ => vec![],
                };

                return services
                    .into_iter()
                    .map(|s| rhai::Dynamic::from(s.to_string()))
                    .collect();
            }
        }
        Vec::new()
    });

    // ========== Pricing Functions ==========

    // get_healing_base_cost(service) -> i64
    // Returns the base cost for a service before multiplier
    engine.register_fn("get_healing_base_cost", |service: String| -> i64 {
        match service.to_lowercase().as_str() {
            "minor_wound" => 25,
            "moderate_wound" => 75,
            "severe_wound" => 200,
            "bleeding" => 50,
            "poison" => 100,
            "illness" => 150,
            "burn" => 75,
            "frostbite" => 75,
            "restore_hp" => 20, // Per 10 HP
            "revive" => 500,
            "full_heal" => 300,
            // Physical trauma (medic)
            "broken_bone" => 150,
            "concussion" => 125,
            "severed_tendon" => 200,
            "impaled" => 200,
            "nerve_damage" => 175,
            "punctured_organ" => 175,
            // Elemental/poison (herbalist)
            "severe_burn" | "charred" => 100,
            "frozen_limb" | "frostbitten" => 100,
            "venom_surge" | "toxic_shock" => 125,
            "acid_burn" => 100,
            _ => 0,
        }
    });

    // get_healing_cost(mobile_id, service) -> i64
    // Returns the actual cost for a service from this healer
    let cloned_db = db.clone();
    engine.register_fn("get_healing_cost", move |mobile_id: String, service: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if !mobile.flags.healer {
                    return 0;
                }

                // Free healers charge nothing
                if mobile.healing_free {
                    return 0;
                }

                let base_cost = match service.to_lowercase().as_str() {
                    "minor_wound" => 25,
                    "moderate_wound" => 75,
                    "severe_wound" => 200,
                    "bleeding" => 50,
                    "poison" => 100,
                    "illness" => 150,
                    "burn" => 75,
                    "frostbite" => 75,
                    "restore_hp" => 20,
                    "revive" => 500,
                    "full_heal" => 300,
                    "broken_bone" => 150,
                    "concussion" => 125,
                    "severed_tendon" => 200,
                    "impaled" => 200,
                    "nerve_damage" => 175,
                    "punctured_organ" => 175,
                    "severe_burn" | "charred" => 100,
                    "frozen_limb" | "frostbitten" => 100,
                    "venom_surge" | "toxic_shock" => 125,
                    "acid_burn" => 100,
                    _ => 0,
                };

                // Apply cost multiplier (100 = 1.0x, 150 = 1.5x, etc.)
                return (base_cost * mobile.healing_cost_multiplier as i64) / 100;
            }
        }
        0
    });

    // ========== Healing Action Functions ==========

    // perform_wound_healing(mobile_id, connection_id, wound_index) -> bool
    // Has the healer treat a specific wound on the character
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "perform_wound_healing",
        move |mobile_id: String, connection_id: String, wound_index: i64| -> bool {
            // Get the healer mobile
            let _healer = if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    if !mobile.flags.healer {
                        return false;
                    }
                    mobile
                } else {
                    return false;
                }
            } else {
                return false;
            };

            // Get the character
            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    if let Some(ref mut char) = session.character {
                        let idx = wound_index as usize;
                        if idx < char.wounds.len() {
                            // Get the body part before removing the wound
                            let body_part = char.wounds[idx].body_part.to_display_string().to_string();
                            // Remove the wound
                            char.wounds.remove(idx);
                            // Also clear any ongoing effects on this body part
                            char.ongoing_effects.retain(|e| e.body_part != body_part);
                            // Add scar on healed body part
                            let count = char.scars.entry(body_part).or_insert(0);
                            *count += 1;
                            let _ = cloned_db.save_character_data(char.clone());
                            return true;
                        }
                    }
                }
            }
            false
        },
    );

    // heal_character_wound_by_part(connection_id, body_part) -> bool
    // Heals a wound on a specific body part - updates session AND saves to DB
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "heal_character_wound_by_part",
        move |connection_id: String, body_part: String| -> bool {
            let bp = match crate::BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    if let Some(ref mut char) = session.character {
                        let original_len = char.wounds.len();
                        let bp_name = bp.to_display_string().to_string();
                        char.wounds.retain(|w| w.body_part != bp);
                        if char.wounds.len() != original_len {
                            // Also clear any ongoing effects on this body part
                            char.ongoing_effects.retain(|e| e.body_part != bp_name);
                            // Add scar on healed body part
                            let count = char.scars.entry(bp_name).or_insert(0);
                            *count += 1;
                            let _ = cloned_db.save_character_data(char.clone());
                            return true;
                        }
                    }
                }
            }
            false
        },
    );

    // clear_frostbite_condition(connection_id, body_part) -> bool
    // Clears the frostbite condition for a specific body part
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "clear_frostbite_condition",
        move |connection_id: String, body_part: String| -> bool {
            let bp = match crate::BodyPart::from_str(&body_part) {
                Some(p) => p,
                None => return false,
            };

            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    if let Some(ref mut char) = session.character {
                        let original_len = char.has_frostbite.len();
                        char.has_frostbite.retain(|&p| p != bp);
                        if char.has_frostbite.len() != original_len {
                            let _ = cloned_db.save_character_data(char.clone());
                            return true;
                        }
                    }
                }
            }
            false
        },
    );

    // stop_character_bleeding(connection_id) -> bool
    // Stops all bleeding on a character
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("stop_character_bleeding", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    // Stop bleeding on all wounds
                    for wound in &mut char.wounds {
                        wound.bleeding_severity = 0;
                    }
                    let _ = cloned_db.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // stop_mobile_bleeding(mobile_id) -> bool
    // Stops all bleeding on a mobile
    let cloned_db = db.clone();
    engine.register_fn("stop_mobile_bleeding", move |mobile_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                // Stop bleeding on all wounds
                for wound in &mut mobile.wounds {
                    wound.bleeding_severity = 0;
                }
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // cure_character_poison(connection_id) -> bool
    // Removes poison condition from character
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("cure_character_poison", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    // Remove poisoned wounds
                    char.wounds.retain(|w| w.wound_type != crate::WoundType::Poisoned);
                    let _ = cloned_db.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // cure_character_illness(connection_id) -> bool
    // Removes illness from character
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("cure_character_illness", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    char.has_illness = false;
                    char.illness_progress = 0;
                    char.food_sick = false;
                    let _ = cloned_db.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // cure_hypothermia(connection_id) -> bool
    // Clears hypothermia and reduces cold exposure
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("cure_hypothermia", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    if char.has_hypothermia {
                        char.has_hypothermia = false;
                        // Reduce cold exposure below hypothermia threshold (50)
                        char.cold_exposure = char.cold_exposure.min(40);
                        let _ = cloned_db.save_character_data(char.clone());
                        return true;
                    }
                }
            }
        }
        false
    });

    // cure_heat_exhaustion(connection_id) -> bool
    // Clears heat exhaustion and reduces heat exposure
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("cure_heat_exhaustion", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    if char.has_heat_exhaustion {
                        char.has_heat_exhaustion = false;
                        // Reduce heat exposure below heat exhaustion threshold (50)
                        char.heat_exposure = char.heat_exposure.min(40);
                        let _ = cloned_db.save_character_data(char.clone());
                        return true;
                    }
                }
            }
        }
        false
    });

    // cure_heat_stroke(connection_id) -> bool
    // Clears heat stroke and heat exhaustion, reduces heat exposure
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("cure_heat_stroke", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    if char.has_heat_stroke {
                        char.has_heat_stroke = false;
                        char.has_heat_exhaustion = false;
                        // Reduce heat exposure below heat exhaustion threshold (50)
                        char.heat_exposure = char.heat_exposure.min(40);
                        let _ = cloned_db.save_character_data(char.clone());
                        return true;
                    }
                }
            }
        }
        false
    });

    // restore_character_hp(connection_id, amount) -> i64
    // Restores HP to character, returns actual amount healed
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn(
        "restore_character_hp",
        move |connection_id: String, amount: i64| -> i64 {
            if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns_lock = conns.lock().unwrap();
                if let Some(session) = conns_lock.get_mut(&conn_id) {
                    if let Some(ref mut char) = session.character {
                        let old_hp = char.hp;
                        char.hp = (char.hp + amount as i32).min(char.max_hp);
                        let healed = char.hp - old_hp;
                        let _ = cloned_db.save_character_data(char.clone());
                        return healed as i64;
                    }
                }
            }
            0
        },
    );

    // full_heal_character(connection_id) -> bool
    // Fully restores character to max HP
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("full_heal_character", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    char.hp = char.max_hp;
                    let _ = cloned_db.save_character_data(char.clone());
                    return true;
                }
            }
        }
        false
    });

    // revive_unconscious_character(connection_id) -> bool
    // Revives an unconscious character to 10% HP
    let cloned_db = db.clone();
    let conns = connections.clone();
    engine.register_fn("revive_unconscious_character", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if let Some(ref mut char) = session.character {
                    if char.is_unconscious {
                        char.is_unconscious = false;
                        char.hp = (char.max_hp / 10).max(1);
                        let _ = cloned_db.save_character_data(char.clone());
                        return true;
                    }
                }
            }
        }
        false
    });

    // ========== Healer Configuration Functions (for medit) ==========

    // set_healer_type(mobile_id, healer_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_healer_type",
        move |mobile_id: String, healer_type: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.healer_type = healer_type;
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // set_healing_free(mobile_id, free) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_healing_free", move |mobile_id: String, free: bool| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                mobile.healing_free = free;
                return cloned_db.save_mobile_data(mobile).is_ok();
            }
        }
        false
    });

    // set_healing_cost_multiplier(mobile_id, multiplier) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_healing_cost_multiplier",
        move |mobile_id: String, multiplier: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mut mobile)) = cloned_db.get_mobile_data(&uuid) {
                    mobile.healing_cost_multiplier = multiplier as i32;
                    return cloned_db.save_mobile_data(mobile).is_ok();
                }
            }
            false
        },
    );

    // ========== Dialogue Helper Functions ==========

    // get_healer_greeting(mobile_id) -> String
    // Returns an appropriate greeting based on healer type
    let cloned_db = db.clone();
    engine.register_fn("get_healer_greeting", move |mobile_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                if !mobile.flags.healer {
                    return String::new();
                }

                return match mobile.healer_type.to_lowercase().as_str() {
                    "medic" => "I can treat your wounds and stop bleeding. Say 'treat' to begin.".to_string(),
                    "herbalist" => "I specialize in curing poisons, illnesses, and elemental ailments. Say 'cure' to begin.".to_string(),
                    "cleric" => "May the divine light heal you. I can restore your health or revive the fallen. Say 'heal' to begin.".to_string(),
                    _ => "I can help with your ailments.".to_string(),
                };
            }
        }
        String::new()
    });

    // is_healer_keyword(word, healer_type) -> bool
    // Checks if a word is a trigger keyword for a healer type
    engine.register_fn("is_healer_keyword", |word: String, healer_type: String| -> bool {
        let word_lower = word.to_lowercase();
        match healer_type.to_lowercase().as_str() {
            "medic" => matches!(
                word_lower.as_str(),
                "heal" | "treat" | "wound" | "wounds" | "bleeding" | "bandage" | "cost" | "price" | "help"
            ),
            "herbalist" => matches!(
                word_lower.as_str(),
                "heal"
                    | "cure"
                    | "poison"
                    | "antidote"
                    | "illness"
                    | "sick"
                    | "cold"
                    | "burn"
                    | "frostbite"
                    | "cost"
                    | "price"
                    | "help"
            ),
            "cleric" => matches!(
                word_lower.as_str(),
                "heal" | "restore" | "revive" | "resurrection" | "bless" | "prayer" | "cost" | "price" | "help"
            ),
            _ => false,
        }
    });
}
