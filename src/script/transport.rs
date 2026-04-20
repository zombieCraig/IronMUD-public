// src/script/transport.rs
// Transportation system functions (elevators, buses, trains, etc.)

use crate::db::Db;
use crate::{TransportData, TransportSchedule, TransportState, TransportStop, TransportType};
use rhai::Engine;
use std::sync::Arc;

/// Register transport-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Transport CRUD Functions ==========

    // get_transport_data(id) -> TransportData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_transport_data", move |id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            match cloned_db.get_transport(uuid) {
                Ok(Some(transport)) => rhai::Dynamic::from(transport),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // get_transport_by_vnum(vnum) -> TransportData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_transport_by_vnum", move |vnum: String| {
        match cloned_db.get_transport_by_vnum(&vnum) {
            Ok(Some(transport)) => rhai::Dynamic::from(transport),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // save_transport_data(transport) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_transport_data", move |transport: TransportData| -> bool {
        cloned_db.save_transport(&transport).is_ok()
    });

    // delete_transport(id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_transport", move |id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            cloned_db.delete_transport(uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // list_all_transports() -> Array of TransportData
    let cloned_db = db.clone();
    engine.register_fn("list_all_transports", move || -> Vec<rhai::Dynamic> {
        match cloned_db.list_all_transports() {
            Ok(transports) => transports.into_iter().map(rhai::Dynamic::from).collect(),
            Err(_) => Vec::new(),
        }
    });

    // search_transports(keyword) -> Array of TransportData
    let cloned_db = db.clone();
    engine.register_fn("search_transports", move |keyword: String| -> Vec<rhai::Dynamic> {
        match cloned_db.search_transports(&keyword) {
            Ok(transports) => transports.into_iter().map(rhai::Dynamic::from).collect(),
            Err(_) => Vec::new(),
        }
    });

    // ========== Transport Lookup Functions ==========

    // get_transport_by_interior_room(room_id) -> TransportData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_transport_by_interior_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_transport_by_interior_room(uuid) {
                Ok(Some(transport)) => rhai::Dynamic::from(transport),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // get_transports_at_stop(room_id) -> Array of TransportData
    let cloned_db = db.clone();
    engine.register_fn("get_transports_at_stop", move |room_id: String| -> Vec<rhai::Dynamic> {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_transports_with_stop_at(uuid) {
                Ok(transports) => transports.into_iter().map(rhai::Dynamic::from).collect(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        }
    });

    // ========== Transport Creation Functions ==========

    // new_transport(name, interior_room_id) -> TransportData
    engine.register_fn("new_transport", |name: String, interior_room_id: String| {
        if let Ok(room_uuid) = uuid::Uuid::parse_str(&interior_room_id) {
            rhai::Dynamic::from(TransportData::new(name, room_uuid))
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // new_transport_stop(room_id, name, exit_direction) -> TransportStop
    engine.register_fn(
        "new_transport_stop",
        |room_id: String, name: String, exit_direction: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
                rhai::Dynamic::from(TransportStop {
                    room_id: uuid,
                    name,
                    exit_direction,
                })
            } else {
                rhai::Dynamic::UNIT
            }
        },
    );

    // ========== Transport Modification Functions ==========

    // set_transport_vnum(transport_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_vnum",
        move |transport_id: String, vnum: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        transport.vnum = Some(vnum);
                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // set_transport_name(transport_id, name) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_name",
        move |transport_id: String, name: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        transport.name = name;
                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // set_transport_type(transport_id, type_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_type",
        move |transport_id: String, type_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Some(transport_type) = TransportType::from_str(&type_str) {
                    match cloned_db.get_transport(uuid) {
                        Ok(Some(mut transport)) => {
                            transport.transport_type = transport_type;
                            cloned_db.save_transport(&transport).is_ok()
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        },
    );

    // set_transport_interior_room(transport_id, room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_interior_room",
        move |transport_id: String, room_id: String| -> bool {
            if let Ok(transport_uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                    match cloned_db.get_transport(transport_uuid) {
                        Ok(Some(mut transport)) => {
                            transport.interior_room_id = room_uuid;
                            cloned_db.save_transport(&transport).is_ok()
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        },
    );

    // set_transport_travel_time(transport_id, seconds) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_travel_time",
        move |transport_id: String, seconds: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        transport.travel_time_secs = seconds;
                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // ========== Transport Stop Management ==========

    // add_transport_stop(transport_id, room_id, name, exit_direction) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_transport_stop",
        move |transport_id: String, room_id: String, name: String, exit_direction: String| -> bool {
            if let Ok(transport_uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                    match cloned_db.get_transport(transport_uuid) {
                        Ok(Some(mut transport)) => {
                            transport.stops.push(TransportStop {
                                room_id: room_uuid,
                                name,
                                exit_direction,
                            });
                            cloned_db.save_transport(&transport).is_ok()
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        },
    );

    // remove_transport_stop(transport_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_transport_stop",
        move |transport_id: String, index: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        let idx = index as usize;
                        if idx < transport.stops.len() {
                            transport.stops.remove(idx);
                            cloned_db.save_transport(&transport).is_ok()
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

    // get_transport_stops(transport_id) -> Array of TransportStop
    let cloned_db = db.clone();
    engine.register_fn(
        "get_transport_stops",
        move |transport_id: String| -> Vec<rhai::Dynamic> {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(transport)) => transport.stops.into_iter().map(rhai::Dynamic::from).collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            }
        },
    );

    // ========== Transport Schedule Functions ==========

    // set_transport_schedule_ondemand(transport_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_transport_schedule_ondemand", move |transport_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
            match cloned_db.get_transport(uuid) {
                Ok(Some(mut transport)) => {
                    transport.schedule = TransportSchedule::OnDemand;
                    cloned_db.save_transport(&transport).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // set_transport_schedule_gametime(transport_id, frequency_hours, operating_start, operating_end, dwell_time_secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_schedule_gametime",
        move |transport_id: String,
              frequency_hours: i64,
              operating_start: i64,
              operating_end: i64,
              dwell_time_secs: i64|
              -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        transport.schedule = TransportSchedule::GameTime {
                            frequency_hours: frequency_hours as i32,
                            operating_start: operating_start as u8,
                            operating_end: operating_end as u8,
                            dwell_time_secs,
                        };
                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // ========== Transport State Functions ==========

    // set_transport_state(transport_id, state_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_state",
        move |transport_id: String, state_str: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                let state = match state_str.to_lowercase().as_str() {
                    "stopped" => TransportState::Stopped,
                    "moving" => TransportState::Moving,
                    _ => return false,
                };
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        transport.state = state;
                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // set_transport_current_stop(transport_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_transport_current_stop",
        move |transport_id: String, index: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        let idx = index as usize;
                        if idx < transport.stops.len() {
                            transport.current_stop_index = idx;
                            cloned_db.save_transport(&transport).is_ok()
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

    // advance_transport_stop(transport_id) -> bool (advances to next stop, handling ping-pong)
    let cloned_db = db.clone();
    engine.register_fn("advance_transport_stop", move |transport_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
            match cloned_db.get_transport(uuid) {
                Ok(Some(mut transport)) => {
                    transport.advance_to_next_stop();
                    cloned_db.save_transport(&transport).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // ========== Transport Utility Functions ==========

    // get_all_transport_types() -> Array of strings
    engine.register_fn("get_all_transport_types", || -> Vec<rhai::Dynamic> {
        vec![
            rhai::Dynamic::from("elevator".to_string()),
            rhai::Dynamic::from("bus".to_string()),
            rhai::Dynamic::from("train".to_string()),
            rhai::Dynamic::from("ferry".to_string()),
            rhai::Dynamic::from("airship".to_string()),
        ]
    });

    // is_transport_ondemand(transport_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_transport_ondemand", move |transport_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
            match cloned_db.get_transport(uuid) {
                Ok(Some(transport)) => {
                    matches!(transport.schedule, TransportSchedule::OnDemand)
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // is_transport_within_operating_hours(transport_id, hour) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "is_transport_within_operating_hours",
        move |transport_id: String, hour: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(transport)) => transport.is_within_operating_hours(hour as u8),
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_transport_schedule_type(transport_id) -> String ("ondemand" or "gametime")
    let cloned_db = db.clone();
    engine.register_fn("get_transport_schedule_type", move |transport_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
            match cloned_db.get_transport(uuid) {
                Ok(Some(transport)) => match transport.schedule {
                    TransportSchedule::OnDemand => "ondemand".to_string(),
                    TransportSchedule::GameTime { .. } => "gametime".to_string(),
                },
                _ => "unknown".to_string(),
            }
        } else {
            "unknown".to_string()
        }
    });

    // get_transport_schedule_info(transport_id) -> Map with schedule details
    let cloned_db = db.clone();
    engine.register_fn(
        "get_transport_schedule_info",
        move |transport_id: String| -> rhai::Map {
            let mut map = rhai::Map::new();
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Ok(Some(transport)) = cloned_db.get_transport(uuid) {
                    match transport.schedule {
                        TransportSchedule::OnDemand => {
                            map.insert("type".into(), rhai::Dynamic::from("ondemand"));
                        }
                        TransportSchedule::GameTime {
                            frequency_hours,
                            operating_start,
                            operating_end,
                            dwell_time_secs,
                        } => {
                            map.insert("type".into(), rhai::Dynamic::from("gametime"));
                            map.insert("frequency_hours".into(), rhai::Dynamic::from(frequency_hours as i64));
                            map.insert("operating_start".into(), rhai::Dynamic::from(operating_start as i64));
                            map.insert("operating_end".into(), rhai::Dynamic::from(operating_end as i64));
                            map.insert("dwell_time_secs".into(), rhai::Dynamic::from(dwell_time_secs));
                        }
                    }
                }
            }
            map
        },
    );

    // get_transport_time_at_current_state(transport_id) -> i64 (seconds since last state change)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_transport_time_at_current_state",
        move |transport_id: String| -> i64 {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Ok(Some(transport)) = cloned_db.get_transport(uuid) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    return now - transport.last_state_change;
                }
            }
            0
        },
    );

    // ========== Transport Connection Functions ==========

    // connect_transport_to_stop(transport_id, stop_index) -> bool
    // Creates exits between stop room and vehicle interior, sets state to Stopped
    let cloned_db = db.clone();
    engine.register_fn(
        "connect_transport_to_stop",
        move |transport_id: String, stop_index: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        let idx = stop_index as usize;
                        if idx >= transport.stops.len() {
                            return false;
                        }
                        let stop = &transport.stops[idx];
                        let stop_room_id = stop.room_id;
                        let exit_direction = stop.exit_direction.clone();

                        // Create exit from stop room to vehicle interior
                        if cloned_db
                            .set_room_exit(&stop_room_id, &exit_direction, &transport.interior_room_id)
                            .is_err()
                        {
                            return false;
                        }
                        // Create exit from vehicle interior to stop room (use opposite direction for cardinal, "out" for custom)
                        let interior_exit = crate::types::get_opposite_direction(&exit_direction).unwrap_or("out");
                        if cloned_db
                            .set_room_exit(&transport.interior_room_id, interior_exit, &stop_room_id)
                            .is_err()
                        {
                            return false;
                        }

                        // Update transport state
                        transport.current_stop_index = idx;
                        transport.state = TransportState::Stopped;
                        transport.last_state_change = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);

                        // Set dynamic description on stop room
                        if let Ok(Some(mut room)) = cloned_db.get_room_data(&stop_room_id) {
                            room.dynamic_desc = Some(format!("The {} is here, doors open.", transport.name));
                            let _ = cloned_db.save_room_data(room);
                        }

                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // disconnect_transport_from_current_stop(transport_id) -> bool
    // Removes exits from current stop, sets state to Moving
    let cloned_db = db.clone();
    engine.register_fn(
        "disconnect_transport_from_current_stop",
        move |transport_id: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        // Handle transports with no stops
                        if transport.stops.is_empty() {
                            transport.state = TransportState::Moving;
                            transport.last_state_change = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0);
                            return cloned_db.save_transport(&transport).is_ok();
                        }

                        let stop = &transport.stops[transport.current_stop_index];
                        let stop_room_id = stop.room_id;
                        let exit_direction = stop.exit_direction.clone();

                        // Remove exit from stop room to vehicle
                        let _ = cloned_db.clear_room_exit(&stop_room_id, &exit_direction);
                        // Remove exit from vehicle to stop room (use opposite direction for cardinal, "out" for custom)
                        let interior_exit = crate::types::get_opposite_direction(&exit_direction).unwrap_or("out");
                        let _ = cloned_db.clear_room_exit(&transport.interior_room_id, interior_exit);

                        // Clear dynamic description from stop room
                        if let Ok(Some(mut room)) = cloned_db.get_room_data(&stop_room_id) {
                            room.dynamic_desc = None;
                            let _ = cloned_db.save_room_data(room);
                        }

                        // Update transport state
                        transport.state = TransportState::Moving;
                        transport.last_state_change = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);

                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // start_transport_travel(transport_id, destination_stop_index) -> bool
    // For on-demand transports: disconnects from current stop and sets destination
    let cloned_db = db.clone();
    engine.register_fn(
        "start_transport_travel",
        move |transport_id: String, destination_index: i64| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&transport_id) {
                match cloned_db.get_transport(uuid) {
                    Ok(Some(mut transport)) => {
                        let dest_idx = destination_index as usize;
                        if dest_idx >= transport.stops.len() {
                            return false;
                        }
                        // Also validate current_stop_index
                        if transport.current_stop_index >= transport.stops.len() {
                            return false;
                        }

                        // Disconnect from current stop
                        let current_stop = &transport.stops[transport.current_stop_index];
                        let stop_room_id = current_stop.room_id;
                        let exit_direction = current_stop.exit_direction.clone();

                        let _ = cloned_db.clear_room_exit(&stop_room_id, &exit_direction);
                        // Use opposite direction for cardinal, "out" for custom
                        let interior_exit = crate::types::get_opposite_direction(&exit_direction).unwrap_or("out");
                        let _ = cloned_db.clear_room_exit(&transport.interior_room_id, interior_exit);

                        // Clear dynamic description from stop room
                        if let Ok(Some(mut room)) = cloned_db.get_room_data(&stop_room_id) {
                            room.dynamic_desc = None;
                            let _ = cloned_db.save_room_data(room);
                        }

                        // Set destination and state
                        transport.current_stop_index = dest_idx;
                        transport.state = TransportState::Moving;
                        transport.last_state_change = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);

                        cloned_db.save_transport(&transport).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_transport_for_stop_room(room_id) -> TransportData or ()
    // Finds a transport that has this room as one of its stops
    let cloned_db = db.clone();
    engine.register_fn("get_transport_for_stop_room", move |room_id: String| {
        if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_transports_with_stop_at(room_uuid) {
                Ok(transports) => {
                    // Return the first on-demand transport found (prioritize elevators)
                    for transport in &transports {
                        if matches!(transport.schedule, TransportSchedule::OnDemand) {
                            return rhai::Dynamic::from(transport.clone());
                        }
                    }
                    // If no on-demand transport, return first scheduled one
                    if let Some(transport) = transports.into_iter().next() {
                        return rhai::Dynamic::from(transport);
                    }
                    rhai::Dynamic::UNIT
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // get_stop_index_for_room(transport_id, room_id) -> i64 or -1 if not found
    let cloned_db = db.clone();
    engine.register_fn(
        "get_stop_index_for_room",
        move |transport_id: String, room_id: String| -> i64 {
            if let Ok(transport_uuid) = uuid::Uuid::parse_str(&transport_id) {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                    match cloned_db.get_transport(transport_uuid) {
                        Ok(Some(transport)) => {
                            for (i, stop) in transport.stops.iter().enumerate() {
                                if stop.room_id == room_uuid {
                                    return i as i64;
                                }
                            }
                            -1
                        }
                        _ => -1,
                    }
                } else {
                    -1
                }
            } else {
                -1
            }
        },
    );

    // calculate_elevator_travel_time(from_index, to_index) -> i64 (seconds)
    // Returns appropriate travel time based on floor distance
    engine.register_fn(
        "calculate_elevator_travel_time",
        |from_index: i64, to_index: i64| -> i64 {
            let distance = (to_index - from_index).unsigned_abs() as i64;
            // Base 1 second + 0.5 seconds per floor, minimum 1 second
            (1 + distance / 2).max(1)
        },
    );
}

/// Register TransportData type and its getters
pub fn register_types(engine: &mut Engine) {
    // Register TransportData type
    engine
        .register_type_with_name::<TransportData>("TransportData")
        .register_get("id", |t: &mut TransportData| t.id.to_string())
        .register_get("vnum", |t: &mut TransportData| {
            t.vnum.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT)
        })
        .register_get("name", |t: &mut TransportData| t.name.clone())
        .register_get("transport_type", |t: &mut TransportData| {
            t.transport_type.to_display_string().to_string()
        })
        .register_get("interior_room_id", |t: &mut TransportData| {
            t.interior_room_id.to_string()
        })
        .register_get("stops", |t: &mut TransportData| {
            t.stops
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("current_stop_index", |t: &mut TransportData| {
            t.current_stop_index as i64
        })
        .register_get("state", |t: &mut TransportData| t.state.to_display_string().to_string())
        .register_get("direction", |t: &mut TransportData| t.direction as i64)
        .register_get("travel_time_secs", |t: &mut TransportData| t.travel_time_secs)
        .register_get("last_state_change", |t: &mut TransportData| t.last_state_change);

    // Register TransportStop type
    engine
        .register_type_with_name::<TransportStop>("TransportStop")
        .register_get("room_id", |s: &mut TransportStop| s.room_id.to_string())
        .register_get("name", |s: &mut TransportStop| s.name.clone())
        .register_get("exit_direction", |s: &mut TransportStop| s.exit_direction.clone());
}
