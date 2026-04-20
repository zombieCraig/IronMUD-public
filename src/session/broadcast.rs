//! Broadcasting functions for sending messages to players

use uuid::Uuid;

use crate::db;
use crate::{CharacterPosition, SharedConnections};

/// Get all character names in a specific room
pub fn get_characters_in_room(connections: &SharedConnections, room_id: Uuid) -> Vec<String> {
    let conns = connections.lock().unwrap();
    conns
        .values()
        .filter_map(|session| {
            session.character.as_ref().and_then(|char| {
                if char.current_room_id == room_id {
                    Some(char.name.clone())
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Get all character names and positions in a specific room
pub fn get_characters_in_room_with_positions(
    connections: &SharedConnections,
    room_id: Uuid,
) -> Vec<(String, CharacterPosition)> {
    let conns = connections.lock().unwrap();
    conns
        .values()
        .filter_map(|session| {
            session.character.as_ref().and_then(|char| {
                if char.current_room_id == room_id {
                    Some((char.name.clone(), char.position))
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Broadcast a message to all players in a room
pub fn broadcast_to_room(connections: &SharedConnections, room_id: Uuid, message: String, exclude_name: Option<&str>) {
    let conns = connections.lock().unwrap();
    for (_id, session) in conns.iter() {
        if let Some(ref character) = session.character {
            if character.current_room_id == room_id {
                if let Some(exclude) = exclude_name {
                    if character.name == exclude {
                        continue;
                    }
                }
                let _ = session.sender.send(message.clone() + "\n");
            }
        }
    }
}

/// Broadcast a message to all awake players in a room (skips sleeping players)
pub fn broadcast_to_room_awake(
    connections: &SharedConnections,
    room_id: Uuid,
    message: String,
    exclude_name: Option<&str>,
) {
    let conns = connections.lock().unwrap();
    for (_id, session) in conns.iter() {
        if let Some(ref character) = session.character {
            if character.current_room_id == room_id {
                // Skip sleeping players
                if character.position == CharacterPosition::Sleeping {
                    continue;
                }
                if let Some(exclude) = exclude_name {
                    if character.name == exclude {
                        continue;
                    }
                }
                let _ = session.sender.send(message.clone() + "\n");
            }
        }
    }
}

/// Broadcast different messages to awake vs sleeping players
/// Sleeping players get a dream-like version of events
pub fn broadcast_to_room_dreaming(
    connections: &SharedConnections,
    room_id: Uuid,
    awake_message: String,
    sleeping_message: String,
    exclude_name: Option<&str>,
) {
    let conns = connections.lock().unwrap();
    for (_id, session) in conns.iter() {
        if let Some(ref character) = session.character {
            if character.current_room_id == room_id {
                if let Some(exclude) = exclude_name {
                    if character.name == exclude {
                        continue;
                    }
                }
                let msg = if character.position == CharacterPosition::Sleeping {
                    sleeping_message.clone() + "\n"
                } else {
                    awake_message.clone() + "\n"
                };
                let _ = session.sender.send(msg);
            }
        }
    }
}

/// Broadcast a message to all logged-in players
pub fn broadcast_to_all_players(connections: &SharedConnections, message: &str) {
    let conns = connections.lock().unwrap();
    for session in conns.values() {
        if session.character.is_some() {
            let _ = session.sender.send(message.to_string());
        }
    }
}

/// Broadcast a message to players in rooms that can see outside
/// (no no_windows flag, not climate controlled)
pub fn broadcast_to_outdoor_players(db: &db::Db, connections: &SharedConnections, message: &str) {
    let conns = connections.lock().unwrap();
    for session in conns.values() {
        if let Some(ref character) = session.character {
            if let Ok(Some(room)) = db.get_room_data(&character.current_room_id) {
                // Check for climate_controlled (room or area inherited)
                let is_climate_controlled = room.flags.climate_controlled
                    || room
                        .area_id
                        .and_then(|aid| db.get_area_data(&aid).ok().flatten())
                        .map(|area| area.flags.climate_controlled)
                        .unwrap_or(false);
                // Skip if room has no windows, is climate controlled, underwater, or deep water
                if !room.flags.no_windows && !is_climate_controlled && !room.flags.underwater && !room.flags.deep_water
                {
                    let _ = session.sender.send(message.to_string());
                }
            } else {
                // If we can't get room data, default to showing the message
                let _ = session.sender.send(message.to_string());
            }
        }
    }
}

/// Broadcast a message to builders/admins who have builder_debug_enabled
pub fn broadcast_to_builders(connections: &SharedConnections, message: &str) {
    let conns = connections.lock().unwrap();
    for session in conns.values() {
        if let Some(ref character) = session.character {
            if (character.is_builder || character.is_admin) && character.builder_debug_enabled {
                let _ = session.sender.send(format!("[Builder] {}\n", message));
            }
        }
    }
}
