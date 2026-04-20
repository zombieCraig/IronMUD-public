// src/script/fishing.rs
// Fishing system functions

use rhai::Engine;
use crate::SharedConnections;

/// Register fishing-related functions
pub fn register(engine: &mut Engine, connections: SharedConnections) {
    // ========== Fishing State Functions ==========

    // start_fishing(connection_id, rod_id, bait_id, room_id, bite_time) -> bool
    // Starts a fishing session for the player
    let conns = connections.clone();
    engine.register_fn("start_fishing", move |connection_id: String, rod_id: String, bait_id: String, room_id: String, bite_time: i64| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            if let Ok(rod_uuid) = uuid::Uuid::parse_str(&rod_id) {
                if let Ok(room_uuid) = uuid::Uuid::parse_str(&room_id) {
                    let bait_uuid = if bait_id.is_empty() {
                        None
                    } else {
                        uuid::Uuid::parse_str(&bait_id).ok()
                    };

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;

                    let fishing_state = crate::FishingState {
                        started_at: now,
                        bite_time,
                        rod_item_id: rod_uuid,
                        bait_item_id: bait_uuid,
                        room_id: room_uuid,
                        bite_notified: false,
                        warning_notified: false,
                    };

                    let mut conns_lock = conns.lock().unwrap();
                    if let Some(session) = conns_lock.get_mut(&conn_id) {
                        session.fishing_state = Some(fishing_state);
                        return true;
                    }
                }
            }
        }
        false
    });

    // is_fishing(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_fishing", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                return session.fishing_state.is_some();
            }
        }
        false
    });

    // cancel_fishing(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("cancel_fishing", move |connection_id: String| -> bool {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get_mut(&conn_id) {
                if session.fishing_state.is_some() {
                    session.fishing_state = None;
                    return true;
                }
            }
        }
        false
    });

    // get_fishing_bite_time(connection_id) -> i64
    // Returns the unix timestamp when the fish will bite, or 0 if not fishing
    let conns = connections.clone();
    engine.register_fn("get_fishing_bite_time", move |connection_id: String| -> i64 {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref state) = session.fishing_state {
                    return state.bite_time;
                }
            }
        }
        0
    });

    // get_fishing_room(connection_id) -> String
    // Returns the room ID where fishing started, or empty string if not fishing
    let conns = connections.clone();
    engine.register_fn("get_fishing_room", move |connection_id: String| -> String {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref state) = session.fishing_state {
                    return state.room_id.to_string();
                }
            }
        }
        String::new()
    });

    // get_fishing_rod(connection_id) -> String
    // Returns the rod item ID, or empty string if not fishing
    let conns = connections.clone();
    engine.register_fn("get_fishing_rod", move |connection_id: String| -> String {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref state) = session.fishing_state {
                    return state.rod_item_id.to_string();
                }
            }
        }
        String::new()
    });

    // get_fishing_bait(connection_id) -> String
    // Returns the bait item ID, or empty string if no bait
    let conns = connections.clone();
    engine.register_fn("get_fishing_bait", move |connection_id: String| -> String {
        if let Ok(conn_id) = uuid::Uuid::parse_str(&connection_id) {
            let conns_lock = conns.lock().unwrap();
            if let Some(session) = conns_lock.get(&conn_id) {
                if let Some(ref state) = session.fishing_state {
                    if let Some(bait_id) = state.bait_item_id {
                        return bait_id.to_string();
                    }
                }
            }
        }
        String::new()
    });

    // get_current_time() -> i64
    // Returns current unix timestamp
    engine.register_fn("get_current_time", || -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    });

    // random_int(min, max) -> i64
    // Returns a random integer between min and max (inclusive)
    engine.register_fn("random_int", |min: i64, max: i64| -> i64 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(min..=max)
    });

    // random_float() -> f64
    // Returns a random float between 0.0 and 1.0
    engine.register_fn("random_float", || -> f64 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.r#gen::<f64>()
    });

}
