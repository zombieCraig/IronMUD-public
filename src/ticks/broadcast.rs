//! Shared broadcast functions for tick systems
//!
//! These functions handle messaging to players during tick processing.
//! They are internal to the ticks module and duplicate some functions
//! from lib.rs with slightly different signatures for efficiency.

use ironmud::{CharacterData, CharacterPosition, SharedConnections};
use tracing::debug;

/// Send a message to a specific character by name
pub fn send_message_to_character(connections: &SharedConnections, char_name: &str, message: &str) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == char_name.to_lowercase() {
                    let _ = session.sender.send(format!("{}\n", message));
                    return;
                }
            }
        }
    }
}

/// Sync character data from database to session (for automatic combat updates)
pub fn sync_character_to_session(connections: &SharedConnections, char_data: &CharacterData) {
    if let Ok(mut conns) = connections.lock() {
        for (_, session) in conns.iter_mut() {
            if let Some(ref existing_char) = session.character {
                if existing_char.name.to_lowercase() == char_data.name.to_lowercase() {
                    session.character = Some(char_data.clone());
                    return;
                }
            }
        }
    }
}

/// Broadcast to everyone in a room except a specific character
pub fn broadcast_to_room_except(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str, exclude: &str) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.name.to_lowercase() != exclude.to_lowercase() {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}

/// Broadcast to non-sleeping characters in a room (excludes one character by name)
pub fn broadcast_to_room_except_awake(
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
    message: &str,
    exclude: &str,
) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id
                    && char.name.to_lowercase() != exclude.to_lowercase()
                    && char.position != CharacterPosition::Sleeping
                {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}

/// Broadcast to non-sleeping characters in a room (excludes one character by name),
/// formatting the message per-recipient. Useful when message text depends on
/// the viewer (e.g. invisible-mob attribution showing "Something" to viewers
/// without DetectInvisible and the mob's name to viewers with it).
pub fn broadcast_to_room_except_awake_per_viewer<F>(
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
    exclude: &str,
    fmt: F,
) where
    F: Fn(&CharacterData) -> String,
{
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id
                    && char.name.to_lowercase() != exclude.to_lowercase()
                    && char.position != CharacterPosition::Sleeping
                {
                    let msg = fmt(char);
                    let _ = session.sender.send(format!("{}\n", msg));
                }
            }
        }
    }
}

/// Broadcast to everyone in a room
pub fn broadcast_to_room(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str) {
    debug!("broadcast_to_room: acquiring connections lock");
    if let Ok(conns) = connections.lock() {
        debug!("broadcast_to_room: lock acquired, iterating sessions");
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
        debug!("broadcast_to_room: done iterating, releasing lock");
    } else {
        debug!("broadcast_to_room: failed to acquire lock (poisoned?)");
    }
}

/// Broadcast to awake characters in a room (sleeping players don't see)
pub fn broadcast_to_room_awake(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.position != CharacterPosition::Sleeping {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}

/// Broadcast a message to all awake players in a room (for mobile movement)
/// Sleeping players don't notice mobiles coming and going
pub fn broadcast_to_room_mobiles(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.position != CharacterPosition::Sleeping {
                    let _ = session.sender.send(message.to_string());
                }
            }
        }
    }
}
