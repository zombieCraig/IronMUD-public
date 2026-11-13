//! Shared broadcast functions for tick systems
//!
//! These functions handle messaging to players during tick processing.
//! They are internal to the ticks module and duplicate some functions
//! from lib.rs with slightly different signatures for efficiency.

use ironmud::{CharacterData, CharacterPosition, SharedConnections};
use tracing::debug;

/// Send a message to a specific character by name
pub fn send_message_to_character(connections: &SharedConnections, char_name: &str, message: &str) {
    {
        let conns = connections.lock();
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

/// The current copy of a character: the live session's when they're online
/// (the session is authoritative and may be ahead of the DB), else the DB's.
///
/// Tick code that loads a character, mutates it, saves it and then calls
/// [`sync_character_to_session`] must load through here. Loading from the DB
/// and pushing that copy into the session reverts whatever the session had
/// that the DB didn't yet (the regen tick only flushes periodically).
/// Must not be called with the Connections lock held.
pub fn live_character(
    connections: &SharedConnections,
    db: &ironmud::db::Db,
    name: &str,
) -> anyhow::Result<Option<CharacterData>> {
    {
        let conns = connections.lock();
        if let Some(ch) = conns
            .values()
            .filter_map(|s| s.character.as_ref())
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            return Ok(Some(ch.clone()));
        }
    }
    db.get_character_data(name)
}

/// Sync character data from database to session (for automatic combat updates)
pub fn sync_character_to_session(connections: &SharedConnections, char_data: &CharacterData, db: &ironmud::db::Db) {
    {
        let mut conns = connections.lock();
        for (_, session) in conns.iter_mut() {
            if let Some(ref existing_char) = session.character {
                if existing_char.name.to_lowercase() == char_data.name.to_lowercase() {
                    session.character = Some(char_data.clone());
                    session.sync_msdp_vitals(db);
                    return;
                }
            }
        }
    }
}

/// Broadcast to everyone in a room except a specific character
pub fn broadcast_to_room_except(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str, exclude: &str) {
    {
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.name.to_lowercase() != exclude.to_lowercase() {
                    // Writers are deafened to ambient room broadcasts.
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
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
    {
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id
                    && char.name.to_lowercase() != exclude.to_lowercase()
                    && char.position != CharacterPosition::Sleeping
                {
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
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
    {
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id
                    && char.name.to_lowercase() != exclude.to_lowercase()
                    && char.position != CharacterPosition::Sleeping
                {
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
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
    {
        let conns = connections.lock();
        debug!("broadcast_to_room: lock acquired, iterating sessions");
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id {
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
        debug!("broadcast_to_room: done iterating, releasing lock");
    }
}

/// Broadcast to awake characters in a room (sleeping players don't see)
pub fn broadcast_to_room_awake(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str) {
    {
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.position != CharacterPosition::Sleeping {
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}

/// Broadcast a message to all awake players in a room (for mobile movement)
/// Sleeping players don't notice mobiles coming and going
pub fn broadcast_to_room_mobiles(connections: &SharedConnections, room_id: &uuid::Uuid, message: &str) {
    {
        let conns = connections.lock();
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.position != CharacterPosition::Sleeping {
                    if ironmud::session_is_writing(session) {
                        continue;
                    }
                    let _ = session.sender.send(message.to_string());
                }
            }
        }
    }
}
