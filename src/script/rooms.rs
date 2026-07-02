// src/script/rooms.rs
// Room system functions including OLC, doors, extra descriptions, vnums, and display

use super::utilities;
use crate::SharedConnections;
use crate::db::Db;
use crate::script::items::character_has_item_vnum;
use crate::{
    CharacterData, CombatZoneType, DoorState, RoomData, RoomEntryCondition, RoomEntryGate, RoomExits, RoomFlags,
};
use rhai::Engine;
use std::sync::Arc;

const DEFAULT_BLOCK_MESSAGE: &str = "You cannot pass that way.";

/// Pure evaluator: returns `None` if the character may enter the room,
/// or `Some(block_message)` if any condition fails. Empty `conditions`
/// list = allowed (vacuous true). god_mode / build_mode bypass lives
/// in `go.rhai`, not here — this function is also called from tests.
pub fn evaluate_entry_gate(db: &Db, character: &CharacterData, room: &RoomData) -> Option<String> {
    let gate = room.entry_gate.as_ref()?;
    if gate.conditions.is_empty() {
        return None;
    }
    let all_pass = gate.conditions.iter().all(|cond| condition_passes(db, character, cond));
    if all_pass {
        None
    } else {
        let msg = if gate.block_message.trim().is_empty() {
            DEFAULT_BLOCK_MESSAGE.to_string()
        } else {
            gate.block_message.clone()
        };
        Some(msg)
    }
}

fn condition_passes(db: &Db, ch: &CharacterData, cond: &RoomEntryCondition) -> bool {
    match cond {
        RoomEntryCondition::ClassIs { name } => ch.class_name.eq_ignore_ascii_case(name),
        RoomEntryCondition::HasSkill { name, min_level } => ch
            .skills
            .get(&name.to_lowercase())
            .map(|p| (p.level as i64) >= *min_level)
            .unwrap_or(false),
        RoomEntryCondition::HasItem { vnum } => character_has_item_vnum(db, &ch.name, vnum),
        RoomEntryCondition::HasTattoo { keyword } => {
            let kw = keyword.to_lowercase();
            ch.tattoos
                .iter()
                .any(|t| t.keywords.iter().any(|k| k.to_lowercase() == kw))
        }
        RoomEntryCondition::DgVarSet { key } => ch.dg_vars.contains_key(key),
        RoomEntryCondition::DgVarEquals { key, value } => ch.dg_vars.get(key).map(|v| v == value).unwrap_or(false),
        RoomEntryCondition::IsClanMember { tag } => ch
            .clan_tag
            .as_deref()
            .map(|t| t.eq_ignore_ascii_case(tag))
            .unwrap_or(false),
    }
}

fn summarize_condition(cond: &RoomEntryCondition) -> String {
    match cond {
        RoomEntryCondition::ClassIs { name } => format!("class={}", name),
        RoomEntryCondition::HasSkill { name, min_level } => {
            format!("skill {} >= {}", name, min_level)
        }
        RoomEntryCondition::HasItem { vnum } => format!("has item {}", vnum),
        RoomEntryCondition::HasTattoo { keyword } => format!("has tattoo {}", keyword),
        RoomEntryCondition::DgVarSet { key } => format!("dg_var {} set", key),
        RoomEntryCondition::DgVarEquals { key, value } => {
            format!("dg_var {} = {}", key, value)
        }
        RoomEntryCondition::IsClanMember { tag } => format!("clan={}", tag),
    }
}

fn condition_kind(cond: &RoomEntryCondition) -> &'static str {
    match cond {
        RoomEntryCondition::ClassIs { .. } => "class",
        RoomEntryCondition::HasSkill { .. } => "skill",
        RoomEntryCondition::HasItem { .. } => "item",
        RoomEntryCondition::HasTattoo { .. } => "tattoo",
        RoomEntryCondition::DgVarSet { .. } => "dgvar",
        RoomEntryCondition::DgVarEquals { .. } => "dgvar",
        RoomEntryCondition::IsClanMember { .. } => "clan",
    }
}

/// Register room-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== OLC (Online Creation) Functions ==========

    // create_room(title, description) -> Creates new room with random UUID, returns RoomData
    let cloned_db = db.clone();
    engine.register_fn("create_room", move |title: String, description: String| {
        let room = RoomData {
            id: uuid::Uuid::new_v4(),
            title,
            description,
            exits: RoomExits::default(),
            flags: RoomFlags::default(),
            extra_descs: Vec::new(),
            vnum: None,
            area_id: None,
            triggers: Vec::new(),
            doors: std::collections::HashMap::new(),
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: crate::WaterType::None,
            rot_level: 0,
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
            dg_vars: std::collections::HashMap::new(),
            coordinates: None,
            contextual_commands: Vec::new(),
            exit_delays: std::collections::HashMap::new(),
            entry_gate: None,
        };
        if let Err(e) = cloned_db.save_room_data(room.clone()) {
            tracing::error!("Failed to save new room: {}", e);
        }
        room
    });

    // apply_area_default_room_flags(room_id) -> ORs the room's area's
    // default_room_flags into its RoomFlags and saves. Meant to be called
    // right after a freshly-created room is assigned to an area. Returns
    // false if the room/area can't be loaded or the room has no area.
    let cloned_db = db.clone();
    engine.register_fn("apply_area_default_room_flags", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return false,
        };
        let area_id = match room.area_id {
            Some(id) => id,
            None => return false,
        };
        let area = match cloned_db.get_area_data(&area_id) {
            Ok(Some(a)) => a,
            _ => return false,
        };
        room.flags.merge_area_defaults(&area.default_room_flags);
        if room.flags.liveable && room.living_capacity <= 0 {
            room.living_capacity = 1;
        }
        cloned_db.save_room_data(room).is_ok()
    });

    // set_room_exit(room_id, direction, target_room_id) -> Sets exit on a room
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_exit",
        move |room_id: String, direction: String, target_room_id: String| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let target_uuid = match uuid::Uuid::parse_str(&target_room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                match direction.to_lowercase().as_str() {
                    "north" | "n" => room.exits.north = Some(target_uuid),
                    "south" | "s" => room.exits.south = Some(target_uuid),
                    "east" | "e" => room.exits.east = Some(target_uuid),
                    "west" | "w" => room.exits.west = Some(target_uuid),
                    "up" | "u" => room.exits.up = Some(target_uuid),
                    "down" | "d" => room.exits.down = Some(target_uuid),
                    _ => return false,
                }
                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room exit: {}", e);
                    return false;
                }
                true
            } else {
                false
            }
        },
    );

    // clear_room_exit(room_id, direction) -> Removes exit from a room
    let cloned_db = db.clone();
    engine.register_fn("clear_room_exit", move |room_id: String, direction: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            match direction.to_lowercase().as_str() {
                "north" | "n" => room.exits.north = None,
                "south" | "s" => room.exits.south = None,
                "east" | "e" => room.exits.east = None,
                "west" | "w" => room.exits.west = None,
                "up" | "u" => room.exits.up = None,
                "down" | "d" => room.exits.down = None,
                _ => return false,
            }
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room exit removal: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // ========== Door Functions ==========

    // get_door(room_id, direction) -> DoorState or ()
    let cloned_db = db.clone();
    engine.register_fn("get_door", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                let dir = direction.to_lowercase();
                if let Some(door) = room.doors.get(&dir) {
                    return rhai::Dynamic::from(door.clone());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    // has_door(room_id, direction) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_door", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return room.doors.contains_key(&direction.to_lowercase());
            }
        }
        false
    });

    // get_exit_target(room_id, direction) -> target_room_id or ""
    let cloned_db = db.clone();
    engine.register_fn("get_exit_target", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                let target = match direction.to_lowercase().as_str() {
                    "north" | "n" => room.exits.north,
                    "south" | "s" => room.exits.south,
                    "east" | "e" => room.exits.east,
                    "west" | "w" => room.exits.west,
                    "up" | "u" => room.exits.up,
                    "down" | "d" => room.exits.down,
                    _ => None,
                };
                return target.map(|u| u.to_string()).unwrap_or_default();
            }
        }
        String::new()
    });

    // set_door_closed(room_id, direction, closed) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_closed",
        move |room_id: String, direction: String, closed: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.is_closed = closed;
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_door_locked(room_id, direction, locked) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_locked",
        move |room_id: String, direction: String, locked: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.is_locked = locked;
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // add_door(room_id, direction, name) -> bool (only if exit exists)
    let cloned_db = db.clone();
    engine.register_fn("add_door", move |room_id: String, direction: String, name: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                let dir = direction.to_lowercase();
                // Only add door if there's an exit in that direction
                let has_exit = match dir.as_str() {
                    "north" => room.exits.north.is_some(),
                    "south" => room.exits.south.is_some(),
                    "east" => room.exits.east.is_some(),
                    "west" => room.exits.west.is_some(),
                    "up" => room.exits.up.is_some(),
                    "down" => room.exits.down.is_some(),
                    _ => false,
                };
                if !has_exit {
                    return false;
                }
                room.doors.insert(
                    dir,
                    DoorState {
                        name,
                        is_closed: true, // Doors start closed by default
                        is_locked: false,
                        key_vnum: None,
                        description: None,
                        keywords: Vec::new(),
                        pickproof: false,
                    },
                );
                return cloned_db.save_room_data(room).is_ok();
            }
        }
        false
    });

    // remove_door(room_id, direction) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_door", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                if room.doors.remove(&direction.to_lowercase()).is_some() {
                    return cloned_db.save_room_data(room).is_ok();
                }
            }
        }
        false
    });

    // set_door_key(room_id, direction, key_vnum) -> bool
    // Empty / "clear" / "none" clears the key. Otherwise stores the vnum directly.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_key",
        move |room_id: String, direction: String, key_vnum: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        let trimmed = key_vnum.to_lowercase();
                        door.key_vnum = if key_vnum.is_empty() || trimmed == "clear" || trimmed == "none" {
                            None
                        } else {
                            Some(key_vnum)
                        };
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_door_description(room_id, direction, description) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_description",
        move |room_id: String, direction: String, description: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.description = if description.is_empty() {
                            None
                        } else {
                            Some(description)
                        };
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_door_name(room_id, direction, name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_name",
        move |room_id: String, direction: String, name: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.name = name;
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_door_keywords(room_id, direction, keywords_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_keywords",
        move |room_id: String, direction: String, keywords: rhai::Array| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.keywords = keywords.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_door_pickproof(room_id, direction, pickproof) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_door_pickproof",
        move |room_id: String, direction: String, pickproof: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let dir = direction.to_lowercase();
                    if let Some(door) = room.doors.get_mut(&dir) {
                        door.pickproof = pickproof;
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // list_all_rooms() -> Returns array of all RoomData
    let cloned_db = db.clone();
    engine.register_fn("list_all_rooms", move || {
        cloned_db
            .list_all_rooms()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // delete_room(room_id) -> Deletes a room from the database
    let cloned_db = db.clone();
    engine.register_fn("delete_room", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.delete_room(&room_uuid).unwrap_or(false)
    });

    // set_room_title(room_id, title) -> Sets room title. Capped at TITLE_MAX bytes.
    let cloned_db = db.clone();
    engine.register_fn("set_room_title", move |room_id: String, title: String| {
        if title.len() > crate::api::validate::TITLE_MAX {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.title = title;
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room title: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_description(room_id, description) -> Sets room description
    let cloned_db = db.clone();
    engine.register_fn("set_room_description", move |room_id: String, description: String| {
        if description.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.description = description;
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room description: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_spring_desc(room_id, desc) -> Sets spring seasonal description
    let cloned_db = db.clone();
    engine.register_fn("set_room_spring_desc", move |room_id: String, description: String| {
        if description.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.spring_desc = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room spring_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_summer_desc(room_id, desc) -> Sets summer seasonal description
    let cloned_db = db.clone();
    engine.register_fn("set_room_summer_desc", move |room_id: String, description: String| {
        if description.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.summer_desc = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room summer_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_autumn_desc(room_id, desc) -> Sets autumn seasonal description
    let cloned_db = db.clone();
    engine.register_fn("set_room_autumn_desc", move |room_id: String, description: String| {
        if description.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.autumn_desc = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room autumn_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_winter_desc(room_id, desc) -> Sets winter seasonal description
    let cloned_db = db.clone();
    engine.register_fn("set_room_winter_desc", move |room_id: String, description: String| {
        if description.len() > crate::MAX_DESC_BYTES {
            return false;
        }
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.winter_desc = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room winter_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_dynamic_desc(room_id, desc) -> Sets dynamic description (for triggers)
    let cloned_db = db.clone();
    engine.register_fn("set_room_dynamic_desc", move |room_id: String, description: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.dynamic_desc = if description.is_empty() {
                None
            } else {
                Some(description)
            };
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room dynamic_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // clear_room_dynamic_desc(room_id) -> Clears dynamic description
    let cloned_db = db.clone();
    engine.register_fn("clear_room_dynamic_desc", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            room.dynamic_desc = None;
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to clear room dynamic_desc: {}", e);
                return false;
            }
            true
        } else {
            false
        }
    });

    // set_room_flag(room_id, flag_name, value) -> Sets a room flag
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_flag",
        move |room_id: String, flag_name: String, value: bool| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                match flag_name.to_lowercase().as_str() {
                    "dark" => room.flags.dark = value,
                    "no_mob" | "nomob" => room.flags.no_mob = value,
                    "indoors" => room.flags.indoors = value,
                    "underwater" => room.flags.underwater = value,
                    "city" => room.flags.city = value,
                    "no_windows" | "nowindows" => room.flags.no_windows = value,
                    "difficult_terrain" => room.flags.difficult_terrain = value,
                    "dirt_floor" => room.flags.dirt_floor = value,
                    "post_office" => room.flags.post_office = value,
                    "baseline_office" => room.flags.baseline_office = value,
                    "bank" => room.flags.bank = value,
                    "garden" => room.flags.garden = value,
                    "spawn_point" => room.flags.spawn_point = value,
                    "shallow_water" => room.flags.shallow_water = value,
                    "deep_water" => room.flags.deep_water = value,
                    "liveable" | "livable" => room.flags.liveable = value,
                    "private" | "private_room" => room.flags.private_room = value,
                    "tunnel" => room.flags.tunnel = value,
                    "death" => room.flags.death = value,
                    "no_magic" | "nomagic" => room.flags.no_magic = value,
                    "soundproof" => room.flags.soundproof = value,
                    "notrack" | "no_track" => room.flags.notrack = value,
                    "no_recall" | "norecall" => room.flags.no_recall = value,
                    "temple" => room.flags.temple = value,
                    _ => return false,
                }
                if room.flags.liveable && room.living_capacity <= 0 {
                    room.living_capacity = 1;
                }
                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room flag: {}", e);
                    return false;
                }
                true
            } else {
                false
            }
        },
    );

    // get_room_flag(room_id, flag_name) -> Gets a room flag value
    let cloned_db = db.clone();
    engine.register_fn("get_room_flag", move |room_id: String, flag_name: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
            match flag_name.to_lowercase().as_str() {
                "dark" => room.flags.dark,
                "no_mob" | "nomob" => room.flags.no_mob,
                "indoors" => room.flags.indoors,
                "underwater" => room.flags.underwater,
                "city" => room.flags.city,
                "no_windows" | "nowindows" => room.flags.no_windows,
                "difficult_terrain" => room.flags.difficult_terrain,
                "dirt_floor" => room.flags.dirt_floor,
                "post_office" => room.flags.post_office,
                "baseline_office" => room.flags.baseline_office,
                "bank" => room.flags.bank,
                "garden" => room.flags.garden,
                "spawn_point" => room.flags.spawn_point,
                "shallow_water" => room.flags.shallow_water,
                "deep_water" => room.flags.deep_water,
                "liveable" | "livable" => room.flags.liveable,
                "private" | "private_room" => room.flags.private_room,
                "tunnel" => room.flags.tunnel,
                "death" => room.flags.death,
                "no_magic" | "nomagic" => room.flags.no_magic,
                "soundproof" => room.flags.soundproof,
                "notrack" | "no_track" => room.flags.notrack,
                "no_recall" | "norecall" => room.flags.no_recall,
                "temple" => room.flags.temple,
                _ => false,
            }
        } else {
            false
        }
    });

    // ========== Combat Zone Functions ==========

    // set_room_combat_zone(room_id, zone_type) -> Sets the room's combat zone ("pve", "safe", "pvp")
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_combat_zone",
        move |room_id: String, zone_type: String| -> bool {
            if let Some(zone) = CombatZoneType::from_str(&zone_type) {
                if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                    if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                        room.flags.combat_zone = Some(zone);
                        return cloned_db.save_room_data(room).is_ok();
                    }
                }
            }
            false
        },
    );

    // clear_room_combat_zone(room_id) -> Clears the room's combat zone (inherits from area)
    let cloned_db = db.clone();
    engine.register_fn("clear_room_combat_zone", move |room_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                room.flags.combat_zone = None;
                return cloned_db.save_room_data(room).is_ok();
            }
        }
        false
    });

    // get_room_combat_zone(room_id) -> Gets effective combat zone (with inheritance)
    // Returns "pve", "safe", or "pvp"
    let cloned_db = db.clone();
    engine.register_fn("get_room_combat_zone", move |room_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                // Room override takes precedence
                if let Some(zone) = room.flags.combat_zone {
                    return zone.to_display_string().to_string();
                }
                // Fall back to area zone
                if let Some(area_id) = room.area_id {
                    if let Ok(Some(area)) = cloned_db.get_area_data(&area_id) {
                        return area.combat_zone.to_display_string().to_string();
                    }
                }
            }
        }
        "pve".to_string() // Default
    });

    // get_room_combat_zone_raw(room_id) -> Gets raw room combat zone value
    // Returns "pve", "safe", "pvp", or "inherit"
    let cloned_db = db.clone();
    engine.register_fn("get_room_combat_zone_raw", move |room_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return match room.flags.combat_zone {
                    Some(zone) => zone.to_display_string().to_string(),
                    None => "inherit".to_string(),
                };
            }
        }
        "inherit".to_string()
    });

    // ========== Extra Description Functions ==========

    // get_room_extra_desc(room_id, keyword) -> Gets extra description by keyword
    let cloned_db = db.clone();
    engine.register_fn("get_room_extra_desc", move |room_id: String, keyword: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };

        if let Ok(Some(room)) = cloned_db.get_room_data(&room_uuid) {
            let keyword_lower = keyword.to_lowercase();
            for extra in &room.extra_descs {
                for kw in &extra.keywords {
                    if kw.to_lowercase() == keyword_lower {
                        return extra.description.clone();
                    }
                }
            }
        }
        String::new()
    });

    // add_room_extra_desc(room_id, keywords, description) -> Adds extra description to a room
    // keywords is a space-separated string of keywords
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_extra_desc",
        move |room_id: String, keywords: String, description: String| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let keyword_vec: Vec<String> = keywords.split_whitespace().map(|s| s.to_string()).collect();

                if keyword_vec.is_empty() {
                    return false;
                }

                room.extra_descs.push(crate::ExtraDesc {
                    keywords: keyword_vec,
                    description,
                });

                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room after adding extra desc: {}", e);
                    return false;
                }
                return true;
            }
            false
        },
    );

    // remove_room_extra_desc(room_id, keyword) -> Removes extra description by keyword
    let cloned_db = db.clone();
    engine.register_fn("remove_room_extra_desc", move |room_id: String, keyword: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };

        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            let keyword_lower = keyword.to_lowercase();
            let original_len = room.extra_descs.len();
            room.extra_descs
                .retain(|extra| !extra.keywords.iter().any(|kw| kw.to_lowercase() == keyword_lower));
            if room.extra_descs.len() < original_len {
                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room after removing extra desc: {}", e);
                    return false;
                }
                return true;
            }
        }
        false
    });

    // set_room_extra_desc(room_id, keyword, description) -> Upserts extra description
    // Updates existing if keyword matches, adds new if not found
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_extra_desc",
        move |room_id: String, keyword: String, description: String| {
            if description.len() > crate::MAX_DESC_BYTES {
                return false;
            }
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };

            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let keyword_lower = keyword.to_lowercase();

                // Try to find existing extra desc with this keyword
                let mut found = false;
                for extra in &mut room.extra_descs {
                    if extra.keywords.iter().any(|kw| kw.to_lowercase() == keyword_lower) {
                        extra.description = description.clone();
                        found = true;
                        break;
                    }
                }

                // If not found, add new extra desc
                if !found {
                    room.extra_descs.push(crate::ExtraDesc {
                        keywords: vec![keyword],
                        description,
                    });
                }

                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room after setting extra desc: {}", e);
                    return false;
                }
                return true;
            }
            false
        },
    );

    // ========== Contextual Commands ==========

    // add_room_contextual_command(room_id, verb, hint) -> bool
    // Upserts: replaces an existing entry with the same verb. Empty hint clears it.
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_contextual_command",
        move |room_id: String, verb: String, hint: String| {
            let verb = verb.trim().to_lowercase();
            if verb.is_empty() || verb.contains(char::is_whitespace) {
                return false;
            }
            let hint_trimmed = hint.trim();
            let hint_opt = if hint_trimmed.is_empty() {
                None
            } else {
                Some(hint_trimmed.to_string())
            };
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let mut found = false;
                for cc in room.contextual_commands.iter_mut() {
                    if cc.verb == verb {
                        cc.hint = hint_opt.clone();
                        found = true;
                        break;
                    }
                }
                if !found {
                    room.contextual_commands
                        .push(crate::ContextualCommand { verb, hint: hint_opt });
                }
                if let Err(e) = cloned_db.save_room_data(room) {
                    tracing::error!("Failed to save room after adding contextual command: {}", e);
                    return false;
                }
                return true;
            }
            false
        },
    );

    // remove_room_contextual_command(room_id, verb) -> bool (false if not found)
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_room_contextual_command",
        move |room_id: String, verb: String| {
            let verb = verb.trim().to_lowercase();
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
                let original_len = room.contextual_commands.len();
                room.contextual_commands.retain(|cc| cc.verb != verb);
                if room.contextual_commands.len() < original_len {
                    if let Err(e) = cloned_db.save_room_data(room) {
                        tracing::error!("Failed to save room after removing contextual command: {}", e);
                        return false;
                    }
                    return true;
                }
            }
            false
        },
    );

    // clear_room_contextual_commands(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_room_contextual_commands", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        if let Ok(Some(mut room)) = cloned_db.get_room_data(&room_uuid) {
            if room.contextual_commands.is_empty() {
                return false;
            }
            room.contextual_commands.clear();
            if let Err(e) = cloned_db.save_room_data(room) {
                tracing::error!("Failed to save room after clearing contextual commands: {}", e);
                return false;
            }
            return true;
        }
        false
    });

    // ========== Vnum Functions ==========

    // set_room_vnum(room_id, vnum) -> Sets vnum for a room (returns false if vnum already in use)
    let cloned_db = db.clone();
    engine.register_fn("set_room_vnum", move |room_id: String, vnum: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.set_room_vnum(&room_uuid, &vnum).unwrap_or(false)
    });

    // clear_room_vnum(room_id) -> Removes vnum from a room
    let cloned_db = db.clone();
    engine.register_fn("clear_room_vnum", move |room_id: String| {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        cloned_db.clear_room_vnum(&room_uuid).unwrap_or(false)
    });

    // get_room_by_vnum(vnum) -> Gets room data by vnum
    let cloned_db = db.clone();
    engine.register_fn("get_room_by_vnum", move |vnum: String| {
        match cloned_db.get_room_by_vnum(&vnum) {
            Ok(Some(room)) => rhai::Dynamic::from(room),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // resolve_room_id(identifier) -> Resolves vnum or UUID to UUID string
    // Returns empty string if not found
    let cloned_db = db.clone();
    engine.register_fn("resolve_room_id", move |identifier: String| {
        // First try to parse as UUID
        if let Ok(uuid) = uuid::Uuid::parse_str(&identifier) {
            if cloned_db.room_exists(&uuid).unwrap_or(false) {
                return uuid.to_string();
            }
        }
        // Try as vnum
        if let Ok(Some(room)) = cloned_db.get_room_by_vnum(&identifier) {
            return room.id.to_string();
        }
        String::new()
    });

    // ========== Room Search Functions ==========

    // search_rooms(keyword) -> Searches rooms by keyword in title/description
    let cloned_db = db.clone();
    engine.register_fn("search_rooms", move |keyword: String| {
        cloned_db
            .search_rooms(&keyword)
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // ========== Room Display Function ==========
    // display_room(room_id, connection_id, exclude_char_name) - Display room with colors/MXP
    // Consolidates room display logic used by look, go, and login commands
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "display_room",
        move |room_id: String, connection_id: String, exclude_char_name: String| {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return,
            };
            let conn_uuid = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return,
            };

            // Get room data
            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return,
            };

            // Map fog-of-war: record this room in the viewing character's
            // `rooms_visited`. Debounced: skip if already present in the
            // session's cached copy.
            {
                let already = {
                    let conns_guard = conns.lock().unwrap();
                    conns_guard.get(&conn_uuid).and_then(|s| {
                        s.character
                            .as_ref()
                            .map(|c| (c.name.clone(), c.rooms_visited.contains(&room_uuid)))
                    })
                };
                if let Some((player_name, already_visited)) = already {
                    if !already_visited {
                        if let Ok(Some(mut ch)) = cloned_db.get_character_data(&player_name) {
                            if ch.rooms_visited.insert(room_uuid) {
                                let _ = cloned_db.save_character_data(ch.clone());
                                if let Ok(mut conns_guard) = conns.lock() {
                                    if let Some(session) = conns_guard.get_mut(&conn_uuid) {
                                        session.character = Some(ch);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Determine darkness and blindness states
            let is_dark_room = if room.flags.dark {
                true // Always dark rooms (caves, dungeons)
            } else if !room.flags.indoors && !room.flags.city {
                // Outdoor non-city: dark at night
                cloned_db.get_game_time().map(|gt| !gt.is_daytime()).unwrap_or(false)
            } else {
                false // Indoor or city rooms are lit
            };

            // Get connection settings and character info for vision checks
            let (
                colors_enabled,
                mxp_enabled,
                term_width,
                show_room_flags,
                has_night_vision,
                has_light,
                is_blind,
                viewer_can_detect_invis,
                viewer_can_detect_magic,
                viewer_is_admin,
            ) = {
                let conns_guard = conns.lock().unwrap();
                match conns_guard.get(&conn_uuid) {
                    Some(session) => {
                        let (night_vision, light_source, blindness) = if let Some(ref char) = session.character {
                            let equipped = cloned_db.get_equipped_items(&char.name).unwrap_or_default();
                            let nv = char.traits.iter().any(|t| t == "night_vision")
                                || char
                                    .active_buffs
                                    .iter()
                                    .any(|b| b.effect_type == crate::EffectType::NightVision)
                                || equipped.iter().any(|item| item.flags.night_vision);
                            let blind = char.traits.iter().any(|t| t == "blindness")
                                || char
                                    .active_buffs
                                    .iter()
                                    .any(|b| b.effect_type == crate::EffectType::Blind);
                            let light = equipped.iter().any(|item| item.flags.provides_light);
                            (nv, light, blind)
                        } else {
                            (false, false, false)
                        };
                        let detect_invis = session
                            .character
                            .as_ref()
                            .map(|c| {
                                c.active_buffs
                                    .iter()
                                    .any(|b| b.effect_type == crate::EffectType::DetectInvisible)
                            })
                            .unwrap_or(false);
                        let detect_magic = session
                            .character
                            .as_ref()
                            .map(|c| {
                                c.active_buffs
                                    .iter()
                                    .any(|b| b.effect_type == crate::EffectType::DetectMagic)
                            })
                            .unwrap_or(false);
                        let is_admin = session.character.as_ref().map(|c| c.is_admin).unwrap_or(false);
                        (
                            session.colors_enabled,
                            session.mxp_enabled,
                            session.telnet_state.window_width as usize,
                            session.show_room_flags,
                            night_vision,
                            light_source,
                            blindness,
                            detect_invis,
                            detect_magic,
                            is_admin,
                        )
                    }
                    None => return,
                }
            };

            // Determine if room is effectively dark for this character
            // Build mode bypasses darkness in editable areas
            let in_build_mode = crate::script::check_build_mode(&cloned_db, &exclude_char_name, &room_uuid);
            let effectively_dark = is_dark_room && !has_night_vision && !has_light && !in_build_mode;

            // ANSI color codes
            const ANSI_RESET: &str = "\x1b[0m";
            const ANSI_GREEN: &str = "\x1b[32m";
            const ANSI_YELLOW: &str = "\x1b[33m";
            const ANSI_MAGENTA: &str = "\x1b[35m";
            const ANSI_CYAN: &str = "\x1b[36m";
            const ANSI_RED: &str = "\x1b[1;31m";
            const ANSI_BRIGHT_BLACK: &str = "\x1b[90m"; // Dark gray for builder info

            // Helper closures for coloring
            let color = |text: &str, code: &str| -> String {
                if colors_enabled {
                    format!("{}{}{}", code, text, ANSI_RESET)
                } else {
                    text.to_string()
                }
            };

            // Helper for MXP links
            let mxp_link = |cmd: &str, display: &str| -> String {
                if mxp_enabled {
                    format!("<send href=\"{}\">{}</send>", utilities::escape_mxp(cmd), display)
                } else {
                    display.to_string()
                }
            };

            // Word wrap helper
            let wrap = |text: &str, width: usize| -> String {
                let width = width.max(10);
                let mut result = String::new();
                for line in text.lines() {
                    if line.len() <= width {
                        result.push_str(line);
                        result.push('\n');
                        continue;
                    }
                    let mut current_line = String::new();
                    for word in line.split_whitespace() {
                        if current_line.is_empty() {
                            current_line = word.to_string();
                        } else if current_line.len() + 1 + word.len() <= width {
                            current_line.push(' ');
                            current_line.push_str(word);
                        } else {
                            result.push_str(&current_line);
                            result.push('\n');
                            current_line = word.to_string();
                        }
                    }
                    if !current_line.is_empty() {
                        result.push_str(&current_line);
                        result.push('\n');
                    }
                }
                if !text.ends_with('\n') && result.ends_with('\n') {
                    result.pop();
                }
                result
            };

            let mut output = String::new();

            // Title (cyan)
            output.push_str(&color(&room.title, ANSI_CYAN));

            // Show room flags/vnum for builders if enabled
            if show_room_flags {
                let mut info_parts = Vec::new();

                // Add vnum if set
                if let Some(ref vnum) = room.vnum {
                    info_parts.push(format!("vnum:{}", vnum));
                }

                // Add active flags
                if room.flags.dark {
                    info_parts.push("dark".to_string());
                }
                // Show combat zone if not inheriting (PvE default)
                if let Some(zone) = room.flags.combat_zone {
                    info_parts.push(format!("zone:{}", zone.to_display_string()));
                }
                if room.flags.no_mob {
                    info_parts.push("no_mob".to_string());
                }
                if room.flags.indoors {
                    info_parts.push("indoors".to_string());
                }
                if room.flags.underwater {
                    info_parts.push("underwater".to_string());
                }
                if room.flags.climate_controlled {
                    info_parts.push("climate".to_string());
                }
                if room.flags.always_hot {
                    info_parts.push("hot".to_string());
                }
                if room.flags.always_cold {
                    info_parts.push("cold".to_string());
                }
                if room.flags.city {
                    info_parts.push("city".to_string());
                }
                if room.flags.no_windows {
                    info_parts.push("no_windows".to_string());
                }
                if room.flags.difficult_terrain {
                    info_parts.push("difficult_terrain".to_string());
                }
                if room.flags.dirt_floor {
                    info_parts.push("dirt_floor".to_string());
                }
                if room.flags.property_storage {
                    info_parts.push("property_storage".to_string());
                }
                if room.flags.post_office {
                    info_parts.push("post_office".to_string());
                }
                if room.flags.baseline_office {
                    info_parts.push("baseline_office".to_string());
                }
                if room.flags.bank {
                    info_parts.push("bank".to_string());
                }
                if room.flags.garden {
                    info_parts.push("garden".to_string());
                }
                if room.flags.spawn_point {
                    info_parts.push("spawn_point".to_string());
                }
                if room.flags.shallow_water {
                    info_parts.push("shallow_water".to_string());
                }
                if room.flags.deep_water {
                    info_parts.push("deep_water".to_string());
                }
                if room.flags.liveable {
                    info_parts.push("liveable".to_string());
                }

                if !info_parts.is_empty() {
                    let info_str = format!(" [{}]", info_parts.join(", "));
                    output.push_str(&color(&info_str, ANSI_BRIGHT_BLACK));
                }
            }

            output.push('\n');
            output.push_str("--------------------\n");

            // Description (word-wrapped) - modified by darkness/blindness
            if effectively_dark {
                output.push_str("It is too dark to see.");
                output.push('\n');
            } else if is_blind {
                // Blind characters see no description (but still see title/exits)
                output.push('\n');
            } else {
                // Build full description: base + seasonal + dynamic
                let mut full_desc = room.description.clone();

                // Append seasonal description based on current game season
                if let Ok(game_time) = cloned_db.get_game_time() {
                    let seasonal_desc = match game_time.get_season() {
                        crate::Season::Spring => &room.spring_desc,
                        crate::Season::Summer => &room.summer_desc,
                        crate::Season::Autumn => &room.autumn_desc,
                        crate::Season::Winter => &room.winter_desc,
                    };
                    if let Some(desc) = seasonal_desc {
                        if !desc.is_empty() {
                            full_desc.push(' ');
                            full_desc.push_str(desc);
                        }
                    }
                }

                // Append dynamic description if set (from triggers/events)
                if let Some(ref dynamic) = room.dynamic_desc {
                    if !dynamic.is_empty() {
                        full_desc.push(' ');
                        full_desc.push_str(dynamic);
                    }
                }

                output.push_str(&wrap(&full_desc, term_width));
                output.push('\n');
            }

            // The Rot: ambient contamination warning (always sensed, even in
            // the dark — it's a taste, not a sight).
            if room.rot_level > 0 {
                let rot_line = match room.rot_level {
                    1 => "The air here tastes faintly of rust and rot.",
                    2 => "The Rot hangs heavy here; every breath stings going down.",
                    _ => "This is a Rot hotspot. Your skin prickles and weeps where the air touches it.",
                };
                output.push('\n');
                output.push_str(&color(rot_line, ANSI_YELLOW));
                output.push('\n');
            }

            // Mobiles in room (green) - show generic if dark/blind
            if let Ok(mobiles) = cloned_db.get_mobiles_in_room(&room_uuid) {
                let visible_mobiles: Vec<_> = mobiles
                    .into_iter()
                    .filter(|m| {
                        let is_invisible = m
                            .active_buffs
                            .iter()
                            .any(|b| b.effect_type == crate::EffectType::Invisibility);
                        if !is_invisible {
                            return true;
                        }
                        viewer_can_detect_invis || viewer_is_admin
                    })
                    .collect();
                if !visible_mobiles.is_empty() {
                    output.push('\n');
                    for mobile in visible_mobiles {
                        if effectively_dark || is_blind {
                            output.push_str(&color("Someone is here.", ANSI_GREEN));
                        } else {
                            let display = mobile.display_name();
                            let has_nickname = mobile.nickname.as_deref().filter(|s| !s.is_empty()).is_some();
                            let mut line = if mobile.current_activity == crate::ActivityState::Sleeping
                                || mobile.position == crate::types::MobilePosition::Sleeping
                            {
                                format!("{} is here, sleeping.", display)
                            } else if mobile.position == crate::types::MobilePosition::Sitting {
                                format!("{} is here, sitting.", display)
                            } else if has_nickname {
                                format!("{} is here.", display)
                            } else {
                                mobile.short_desc.clone()
                            };
                            if mobile
                                .active_buffs
                                .iter()
                                .any(|b| b.effect_type == crate::EffectType::DamageReduction)
                            {
                                line.push_str(" (glowing with a faint white aura)");
                            }
                            if mobile
                                .active_buffs
                                .iter()
                                .any(|b| b.effect_type == crate::EffectType::Invisibility)
                            {
                                line.push_str(" (invisible)");
                            }
                            if !mobile.vnum.is_empty() {
                                if let Some(cue) =
                                    crate::quest::describe_quest_offers(&cloned_db, &exclude_char_name, &mobile.vnum)
                                {
                                    line.push(' ');
                                    line.push_str(&cue);
                                }
                            }
                            output.push_str(&color(&line, ANSI_GREEN));
                        }
                        output.push('\n');
                    }
                }
            }

            // Items in room (yellow, skip invisible/buried) - show generic if dark/blind.
            // Builders (build_mode in editable area) and admins see buried items tagged "(buried)".
            let see_buried = viewer_is_admin || in_build_mode;
            if let Ok(items) = cloned_db.get_items_in_room(&room_uuid) {
                let visible_items: Vec<_> = items
                    .iter()
                    .filter(|i| !i.flags.invisible && (!i.flags.buried || see_buried))
                    .collect();
                if !visible_items.is_empty() {
                    output.push('\n');
                    for item in visible_items {
                        if effectively_dark || is_blind {
                            output.push_str(&color("Something is here.", ANSI_YELLOW));
                        } else {
                            let mut display_desc = item.short_desc.clone();
                            if item.flags.glow {
                                display_desc.push_str(" (glowing)");
                            }
                            if item.flags.hum {
                                display_desc.push_str(" (humming)");
                            }
                            if item.flags.magical && (viewer_can_detect_magic || viewer_is_admin) {
                                display_desc.push_str(" (magical aura)");
                            }
                            if item.flags.buried {
                                display_desc.push_str(" (buried)");
                            }
                            output.push_str(&color(&display_desc, ANSI_YELLOW));
                        }
                        output.push('\n');
                    }
                }
            }

            // Plants in room (green, skip Seed stage - underground)
            if let Ok(plants) = cloned_db.get_plants_in_room(&room_uuid) {
                let visible_plants: Vec<_> = plants
                    .iter()
                    .filter(|p| p.stage != crate::GrowthStage::Seed && p.stage != crate::GrowthStage::Dead)
                    .collect();
                if !visible_plants.is_empty() {
                    output.push('\n');
                    for plant in visible_plants {
                        if effectively_dark || is_blind {
                            output.push_str(&color("A plant grows here.", ANSI_GREEN));
                        } else {
                            let desc =
                                if let Ok(Some(proto)) = cloned_db.get_plant_prototype_by_vnum(&plant.prototype_vnum) {
                                    proto
                                        .get_stage_def(&plant.stage)
                                        .map(|s| s.description.clone())
                                        .unwrap_or_else(|| format!("A {} grows here.", proto.name))
                                } else {
                                    "A plant grows here.".to_string()
                                };
                            output.push_str(&color(&desc, ANSI_GREEN));
                        }
                        output.push('\n');
                    }
                }
            }

            // Blood trails (red) - anonymous in look, use track to identify
            {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Filter to non-expired trails
                let active_trails: Vec<_> = room.blood_trails.iter().filter(|t| now - t.timestamp < 300).collect();

                if !active_trails.is_empty() {
                    if effectively_dark || is_blind {
                        output.push('\n');
                        output.push_str(&color("You smell blood nearby.", ANSI_RED));
                        output.push('\n');
                    } else {
                        // Separate directional and non-directional trails
                        let directional: Vec<_> = active_trails.iter().filter(|t| t.direction.is_some()).collect();
                        let non_directional: Vec<_> = active_trails.iter().filter(|t| t.direction.is_none()).collect();

                        // For non-directional, show only the highest severity one
                        if !non_directional.is_empty() {
                            let max_sev = non_directional.iter().map(|t| t.severity).max().unwrap_or(1);
                            let desc = match max_sev {
                                1 => "A light spatter of blood stains the ground here.",
                                2 => "Drops of blood are spattered on the ground here.",
                                3 => "A trail of blood stains the ground here.",
                                _ => "A pool of blood stains the ground here.",
                            };
                            output.push('\n');
                            output.push_str(&color(desc, ANSI_RED));
                            output.push('\n');
                        }

                        // Show each directional trail separately
                        for trail in &directional {
                            if let Some(ref dir) = trail.direction {
                                output.push('\n');
                                output.push_str(&color(&format!("A trail of blood leads {}.", dir), ANSI_RED));
                                output.push('\n');
                            }
                        }
                    }
                }
            }

            // Exits (magenta, with MXP links)
            let mut exits: Vec<String> = Vec::new();
            if room.exits.north.is_some() {
                exits.push("north".to_string());
            }
            if room.exits.east.is_some() {
                exits.push("east".to_string());
            }
            if room.exits.south.is_some() {
                exits.push("south".to_string());
            }
            if room.exits.west.is_some() {
                exits.push("west".to_string());
            }
            if room.exits.up.is_some() {
                exits.push("up".to_string());
            }
            if room.exits.down.is_some() {
                exits.push("down".to_string());
            }
            if room.exits.out.is_some() {
                exits.push("out".to_string());
            }
            // Add custom exits (e.g., "elevator", "train", "portal")
            for custom_exit in room.exits.custom.keys() {
                exits.push(custom_exit.clone());
            }

            output.push('\n');
            if exits.is_empty() {
                output.push_str(&color("Exits: none", ANSI_MAGENTA));
            } else {
                output.push_str(&color("Exits: ", ANSI_MAGENTA));
                let exit_strs: Vec<String> = exits
                    .iter()
                    .map(|ex| {
                        let cmd = format!("go {}", ex);
                        let link = mxp_link(&cmd, ex);
                        color(&link, ANSI_MAGENTA)
                    })
                    .collect();
                output.push_str(&exit_strs.join(", "));
            }

            // Builder-declared verbs the room exposes (DG OnCommand triggers
            // typically back the runtime; this line just surfaces them).
            if !room.contextual_commands.is_empty() {
                output.push('\n');
                output.push_str(&color("Here you can: ", ANSI_MAGENTA));
                let parts: Vec<String> = room
                    .contextual_commands
                    .iter()
                    .map(|cc| {
                        let label = match cc.hint.as_deref() {
                            Some(h) if !h.is_empty() => format!("{} ({})", cc.verb, h),
                            _ => cc.verb.clone(),
                        };
                        let link = mxp_link(&cc.verb, &label);
                        color(&link, ANSI_MAGENTA)
                    })
                    .collect();
                output.push_str(&parts.join(", "));
            }

            // Weather/environment line for outdoor rooms (gray/dim)
            const ANSI_DIM: &str = "\x1b[2m";
            if room.flags.always_hot {
                output.push_str("\n");
                output.push_str(&color("The air here is oppressively hot.", ANSI_DIM));
            } else if room.flags.always_cold {
                output.push_str("\n");
                output.push_str(&color("The air here is bitterly cold.", ANSI_DIM));
            } else {
                // Check for climate_controlled (room or area inherited)
                let is_climate_controlled = room.flags.climate_controlled
                    || room
                        .area_id
                        .and_then(|aid| cloned_db.get_area_data(&aid).ok().flatten())
                        .map(|area| area.flags.climate_controlled)
                        .unwrap_or(false);
                if !room.flags.indoors && !is_climate_controlled {
                    // Outdoor room - show weather (projected through area climate).
                    if let Ok(game_time) = cloned_db.get_game_time() {
                        let climate = cloned_db.room_climate(&room);
                        let local_weather = game_time.weather_for_climate(climate);
                        let weather_desc = match local_weather {
                            crate::WeatherCondition::Clear => "clear",
                            crate::WeatherCondition::PartlyCloudy => "partly cloudy",
                            crate::WeatherCondition::Cloudy => "cloudy",
                            crate::WeatherCondition::Overcast => "overcast",
                            crate::WeatherCondition::LightRain => "light rain falling",
                            crate::WeatherCondition::Rain => "raining",
                            crate::WeatherCondition::HeavyRain => "heavy rain pouring down",
                            crate::WeatherCondition::Thunderstorm => "a thunderstorm raging",
                            crate::WeatherCondition::LightSnow => "light snow falling",
                            crate::WeatherCondition::Snow => "snowing",
                            crate::WeatherCondition::Blizzard => "a blizzard howling",
                            crate::WeatherCondition::Fog => "foggy",
                        };
                        let local_temp = game_time.effective_temperature_for_climate(climate);
                        let temp_desc = match local_temp {
                            t if t < 0 => "freezing cold",
                            t if t < 10 => "cold",
                            t if t < 15 => "cool",
                            t if t < 20 => "mild",
                            t if t < 25 => "warm",
                            t if t < 35 => "hot",
                            _ => "sweltering",
                        };
                        output.push_str("\n");
                        output.push_str(&color(&format!("It is {} and {}.", weather_desc, temp_desc), ANSI_DIM));
                    }
                }
            }
            // Indoor/climate_controlled rooms show nothing

            // Water environment description
            if room.flags.shallow_water {
                output.push_str("\n");
                output.push_str(&color("Shallow water ripples around your feet.", ANSI_DIM));
            }
            if room.flags.deep_water {
                output.push_str("\n");
                output.push_str(&color("Deep water stretches out before you.", ANSI_DIM));
            }
            if room.flags.underwater {
                output.push_str("\n");
                output.push_str(&color("You are submerged beneath the water's surface.", ANSI_DIM));
            }

            // Other characters in room (green) - show generic if dark/blind, with position
            let others_with_positions = crate::get_characters_in_room_with_positions(&conns, room_uuid);
            let idle_threshold: i64 = cloned_db
                .get_setting_or_default("idle_timeout_secs", "300")
                .unwrap_or_else(|_| "300".to_string())
                .parse::<i64>()
                .unwrap_or(300)
                .max(30);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let visible_others: Vec<String> = others_with_positions
                .into_iter()
                .filter(|(name, _)| name != &exclude_char_name)
                .filter(|(name, _)| {
                    // Skip invisible characters unless viewer has detect_invisible or is admin
                    if viewer_can_detect_invis || viewer_is_admin {
                        return true;
                    }
                    if let Ok(Some(other)) = cloned_db.get_character_data(name) {
                        // Check invisibility buff
                        if other
                            .active_buffs
                            .iter()
                            .any(|b| b.effect_type == crate::EffectType::Invisibility)
                        {
                            return false;
                        }
                        // Check stealth states (hidden, sneaking, camouflaged)
                        if other.is_hidden || other.is_sneaking || other.is_camouflaged {
                            // Viewer needs perception check to see stealthy characters
                            // Get viewer's perception level
                            if let Ok(Some(viewer)) = cloned_db.get_character_data(&exclude_char_name) {
                                let viewer_stealth = viewer.skills.get("stealth").map(|s| s.level as i64).unwrap_or(0);
                                let viewer_tracking =
                                    viewer.skills.get("tracking").map(|s| s.level as i64).unwrap_or(0);
                                let viewer_perception = viewer_stealth.max(viewer_tracking);
                                let viewer_wis_mod = (viewer.stat_wis as i64 - 10) / 2;
                                let mut perception_score = (viewer_perception * 8) + (viewer_wis_mod * 3);
                                if viewer
                                    .active_buffs
                                    .iter()
                                    .any(|b| b.effect_type == crate::EffectType::DetectInvisible)
                                {
                                    perception_score += 30;
                                }

                                let other_stealth = other.skills.get("stealth").map(|s| s.level as i64).unwrap_or(0);
                                let other_dex_mod = (other.stat_dex as i64 - 10) / 2;
                                let mut stealth_score = (other_stealth * 8) + (other_dex_mod * 3);
                                // Camouflage terrain bonus
                                if other.is_camouflaged
                                    && !room.flags.city
                                    && !room.flags.indoors
                                    && room.flags.dirt_floor
                                {
                                    stealth_score += 15;
                                }
                                if room.flags.dark {
                                    stealth_score += 20;
                                }

                                return perception_score > stealth_score;
                            }
                            return false; // Can't load viewer data, hide stealthy char
                        }
                    }
                    true
                })
                .map(|(name, position)| {
                    // Clone name for AFK lookup since display_name may consume it
                    let name_for_afk = name.clone();

                    // Check for god mode glow and dark vision
                    let (display_name, is_glowing) = if effectively_dark || is_blind {
                        // Check if the other character has night_vision trait (glowing eyes) or god_mode (divine glow)
                        if let Ok(Some(other_char)) = cloned_db.get_character_data(&name) {
                            if other_char.god_mode {
                                (name, true) // God mode players are visible even in darkness
                            } else if other_char.traits.iter().any(|t| t == "night_vision") {
                                ("A pair of glowing eyes".to_string(), false)
                            } else {
                                ("Someone".to_string(), false)
                            }
                        } else {
                            ("Someone".to_string(), false)
                        }
                    } else {
                        // In normal light, check for god mode glow
                        let glowing = cloned_db
                            .get_character_data(&name)
                            .map(|c| c.map(|ch| ch.god_mode).unwrap_or(false))
                            .unwrap_or(false);
                        (name, glowing)
                    };

                    let position_suffix = match position {
                        crate::CharacterPosition::Sitting => " (sitting)",
                        crate::CharacterPosition::Sleeping => " (sleeping)",
                        crate::CharacterPosition::Swimming => " (swimming)",
                        crate::CharacterPosition::Standing => "",
                    };

                    // Check player status tags (Disconnected, Writing, AFK, Idle)
                    let status_suffix = {
                        let conns_guard = conns.lock().unwrap();
                        let mut status = "";
                        for session in conns_guard.values() {
                            if let Some(ref char) = session.character {
                                if char.name == name_for_afk {
                                    if session.disconnected_at.is_some() {
                                        status = " [Disconnected]";
                                    } else if crate::session_is_writing(session) {
                                        status = " [Writing]";
                                    } else if session.afk {
                                        status = " [AFK]";
                                    } else if now.saturating_sub(session.last_activity_time) > idle_threshold {
                                        status = " [Idle]";
                                    }
                                    break;
                                }
                            }
                        }
                        status
                    };

                    // Add glowing indicator for god mode
                    let glow_suffix = if is_glowing { " (glowing)" } else { "" };

                    color(
                        &format!("{}{}{}{}", display_name, glow_suffix, position_suffix, status_suffix),
                        ANSI_GREEN,
                    )
                })
                .collect();
            if !visible_others.is_empty() {
                output.push_str("\n\n");
                output.push_str(&color("Also here: ", ANSI_GREEN));
                output.push_str(&visible_others.join(", "));
            }

            // Automap injection: if the world setting and the character's
            // `automap_enabled` are both on, render the map at the character's
            // configured radius and prepend it above the room block. Plain
            // ASCII (or Unicode box-drawing) sits before the MXP-mode escape so
            // the parser stays out of the map.
            let (map_prefix, map_legend_was_shown) = {
                let (player_name, automap_on, show_legend, want_colors, radius, ascii_only) = {
                    let conns_guard = conns.lock().unwrap();
                    match conns_guard.get(&conn_uuid) {
                        Some(s) => match s.character.as_ref() {
                            Some(c) => (
                                c.name.clone(),
                                c.automap_enabled,
                                !s.map_legend_shown,
                                s.colors_enabled,
                                c.automap_radius
                                    .clamp(crate::script::map::MIN_RADIUS, crate::script::map::MAX_RADIUS),
                                c.ascii_map,
                            ),
                            None => (
                                String::new(),
                                false,
                                false,
                                false,
                                crate::script::map::AUTOMAP_DEFAULT_RADIUS,
                                false,
                            ),
                        },
                        None => (
                            String::new(),
                            false,
                            false,
                            false,
                            crate::script::map::AUTOMAP_DEFAULT_RADIUS,
                            false,
                        ),
                    }
                };
                if automap_on && !player_name.is_empty() && crate::script::map::enabled(&cloned_db) {
                    let rendered = crate::script::map::render_map_for_player_with_options(
                        &cloned_db,
                        &player_name.to_lowercase(),
                        Some(radius),
                        show_legend,
                        want_colors,
                        ascii_only,
                    );
                    if rendered.is_empty() {
                        (String::new(), false)
                    } else {
                        // Trailing blank line between map and room title.
                        (format!("{}\n", rendered), show_legend)
                    }
                } else {
                    (String::new(), false)
                }
            };

            // Send the message (with MXP prefix if enabled, and terminal title update)
            let mut conns_guard = conns.lock().unwrap();
            if let Some(session) = conns_guard.get_mut(&conn_uuid) {
                // Build terminal title with character name if logged in
                let title = if let Some(ref char) = session.character {
                    format!("[{}] {}", char.name, room.title)
                } else {
                    room.title.clone()
                };
                let title_seq = crate::telnet::build_title_sequence(&session.telnet_state, &title);
                let title_str = String::from_utf8_lossy(&title_seq);

                let final_output = if session.mxp_enabled {
                    format!("{}{}\x1b[1z{}\n", title_str, map_prefix, output)
                } else {
                    format!("{}{}{}\n", title_str, map_prefix, utilities::strip_mxp_tags(&output))
                };
                let _ = session.sender.send(final_output);
                if map_legend_was_shown {
                    session.map_legend_shown = true;
                }
            }
        },
    );

    // set_room_living_capacity(room_id, capacity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_living_capacity",
        move |room_id: String, capacity: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    room.living_capacity = capacity.max(0).min(i32::MAX as i64) as i32;
                    return cloned_db.save_room_data(room).is_ok();
                }
            }
            false
        },
    );

    // set_room_rot_level(room_id, level) -> bool (clamped 0-3; The Rot)
    let cloned_db = db.clone();
    engine.register_fn("set_room_rot_level", move |room_id: String, level: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                room.rot_level = (level as i32).clamp(0, crate::types::ROT_LEVEL_MAX);
                return cloned_db.save_room_data(room).is_ok();
            }
        }
        false
    });

    // get_room_rot_level(room_id) -> i64 (0 = clean .. 3 = rot hotspot)
    let cloned_db = db.clone();
    engine.register_fn("get_room_rot_level", move |room_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return room.rot_level as i64;
            }
        }
        0
    });

    // get_room_living_capacity(room_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_room_living_capacity", move |room_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return room.living_capacity as i64;
            }
        }
        0
    });

    // get_room_resident_count(room_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_room_resident_count", move |room_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return room.residents.len() as i64;
            }
        }
        0
    });

    // get_room_resident_names(room_id) -> array of strings (names of mobiles residing here)
    let cloned_db = db.clone();
    engine.register_fn("get_room_resident_names", move |room_id: String| {
        let mut names: Vec<rhai::Dynamic> = Vec::new();
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                for rid in &room.residents {
                    if let Ok(Some(mob)) = cloned_db.get_mobile_data(rid) {
                        names.push(rhai::Dynamic::from(mob.name.clone()));
                    }
                }
            }
        }
        names
    });

    // get_exit_delay(room_id, direction) -> i64 (seconds; 0 = no delay)
    let cloned_db = db.clone();
    engine.register_fn("get_exit_delay", move |room_id: String, direction: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                return room.exit_delays.get(&direction.to_lowercase()).copied().unwrap_or(0);
            }
        }
        0
    });

    // set_exit_delay(room_id, direction, seconds) -> bool (0 clears the entry)
    let cloned_db = db.clone();
    engine.register_fn(
        "set_exit_delay",
        move |room_id: String, direction: String, seconds: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                if let Ok(Some(mut room)) = cloned_db.get_room_data(&uuid) {
                    let key = direction.to_lowercase();
                    if seconds <= 0 {
                        room.exit_delays.remove(&key);
                    } else {
                        room.exit_delays.insert(key, seconds);
                    }
                    return cloned_db.save_room_data(room).is_ok();
                }
            }
            false
        },
    );

    // list_exit_delays(room_id) -> Map<direction, seconds>
    let cloned_db = db.clone();
    engine.register_fn("list_exit_delays", move |room_id: String| -> rhai::Map {
        let mut out = rhai::Map::new();
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = cloned_db.get_room_data(&uuid) {
                for (dir, secs) in &room.exit_delays {
                    out.insert(dir.clone().into(), rhai::Dynamic::from(*secs));
                }
            }
        }
        out
    });

    // ========== Entry-gate functions (conditional room entry) ==========

    // check_room_entry_gate(room_id, char_name) -> "" if allowed, block message if blocked.
    // Fails open (returns "") if either room or character can't be loaded.
    let cloned_db = db.clone();
    engine.register_fn(
        "check_room_entry_gate",
        move |room_id: String, char_name: String| -> String {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            let room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return String::new(),
            };
            if room.entry_gate.is_none() {
                return String::new();
            }
            let character = match cloned_db.get_character_data(&char_name) {
                Ok(Some(c)) => c,
                _ => return String::new(),
            };
            evaluate_entry_gate(&cloned_db, &character, &room).unwrap_or_default()
        },
    );

    // clear_room_entry_gate(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_room_entry_gate", move |room_id: String| -> bool {
        with_room_mut(&cloned_db, &room_id, |room| {
            room.entry_gate = None;
        })
    });

    // set_room_entry_gate_message(room_id, msg) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_room_entry_gate_message",
        move |room_id: String, msg: String| -> bool {
            with_room_mut(&cloned_db, &room_id, |room| {
                let gate = room.entry_gate.get_or_insert_with(RoomEntryGate::default);
                gate.block_message = msg;
            })
        },
    );

    // get_room_entry_gate_message(room_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_room_entry_gate_message", move |room_id: String| -> String {
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r.entry_gate.map(|g| g.block_message).unwrap_or_default(),
            _ => String::new(),
        }
    });

    // add_room_entry_condition_class(room_id, name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_class",
        move |room_id: String, name: String| -> bool {
            if name.trim().is_empty() {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(RoomEntryCondition::ClassIs {
                        name: name.trim().to_lowercase(),
                    });
            })
        },
    );

    // add_room_entry_condition_skill(room_id, name, min_level) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_skill",
        move |room_id: String, name: String, min_level: i64| -> bool {
            if name.trim().is_empty() {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(RoomEntryCondition::HasSkill {
                        name: name.trim().to_lowercase(),
                        min_level,
                    });
            })
        },
    );

    // add_room_entry_condition_item(room_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_item",
        move |room_id: String, vnum: String| -> bool {
            if vnum.trim().is_empty() {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(RoomEntryCondition::HasItem {
                        vnum: vnum.trim().to_string(),
                    });
            })
        },
    );

    // add_room_entry_condition_tattoo(room_id, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_tattoo",
        move |room_id: String, keyword: String| -> bool {
            if keyword.trim().is_empty() {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(RoomEntryCondition::HasTattoo {
                        keyword: keyword.trim().to_lowercase(),
                    });
            })
        },
    );

    // add_room_entry_condition_dgvar(room_id, key, value) -> bool
    // Empty `value` => DgVarSet (presence-only); non-empty => DgVarEquals.
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_dgvar",
        move |room_id: String, key: String, value: String| -> bool {
            if key.trim().is_empty() {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                let key = key.trim().to_string();
                let cond = if value.is_empty() {
                    RoomEntryCondition::DgVarSet { key }
                } else {
                    RoomEntryCondition::DgVarEquals { key, value }
                };
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(cond);
            })
        },
    );

    // add_room_entry_condition_clan(room_id, tag) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_room_entry_condition_clan",
        move |room_id: String, tag: String| -> bool {
            let tag_up = tag.trim().to_ascii_uppercase();
            if !crate::ClanData::valid_tag(&tag_up) {
                return false;
            }
            with_room_mut(&cloned_db, &room_id, |room| {
                room.entry_gate
                    .get_or_insert_with(RoomEntryGate::default)
                    .conditions
                    .push(RoomEntryCondition::IsClanMember { tag: tag_up.clone() });
            })
        },
    );

    // remove_room_entry_condition(room_id, index) -> bool
    // 1-based index, matching the display order in `list_room_entry_conditions`.
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_room_entry_condition",
        move |room_id: String, index: i64| -> bool {
            let room_uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mut room = match cloned_db.get_room_data(&room_uuid) {
                Ok(Some(r)) => r,
                _ => return false,
            };
            let gate = match room.entry_gate.as_mut() {
                Some(g) => g,
                None => return false,
            };
            if index < 1 || (index as usize) > gate.conditions.len() {
                return false;
            }
            gate.conditions.remove((index as usize) - 1);
            cloned_db.save_room_data(room).is_ok()
        },
    );

    // list_room_entry_conditions(room_id) -> Array of #{kind, summary}
    let cloned_db = db.clone();
    engine.register_fn("list_room_entry_conditions", move |room_id: String| -> rhai::Array {
        let mut out = rhai::Array::new();
        let room_uuid = match uuid::Uuid::parse_str(&room_id) {
            Ok(u) => u,
            Err(_) => return out,
        };
        let room = match cloned_db.get_room_data(&room_uuid) {
            Ok(Some(r)) => r,
            _ => return out,
        };
        if let Some(gate) = room.entry_gate {
            for cond in &gate.conditions {
                let mut m = rhai::Map::new();
                m.insert("kind".into(), condition_kind(cond).to_string().into());
                m.insert("summary".into(), summarize_condition(cond).into());
                out.push(rhai::Dynamic::from(m));
            }
        }
        out
    });
}

/// Load → mutate → save helper for the entry-gate mutators.
fn with_room_mut<F: FnOnce(&mut RoomData)>(db: &Db, room_id: &str, f: F) -> bool {
    let uuid = match uuid::Uuid::parse_str(room_id) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let mut room = match db.get_room_data(&uuid) {
        Ok(Some(r)) => r,
        _ => return false,
    };
    f(&mut room);
    db.save_room_data(room).is_ok()
}
