//! Connection management functions for player sessions

use rhai::Position;
use uuid::Uuid;

use crate::{CharacterData, ConnectionId, SharedConnections};

/// Set the character data for a connection
pub fn set_character_for_connection(
    connections: &SharedConnections,
    connection_id_str: String,
    character_data: CharacterData,
) -> Result<(), Box<rhai::EvalAltResult>> {
    let connection_id = Uuid::parse_str(&connection_id_str).map_err(|e| {
        Box::new(rhai::EvalAltResult::ErrorRuntime(
            format!("Invalid Connection ID: {}", e).into(),
            Position::NONE,
        ))
    })?;
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        // Copy persisted settings from CharacterData to session
        session.show_room_flags = character_data.show_room_flags;
        // Stamp the play-time anchor when the character first enters the
        // world for this session. Idempotent — later `set_player_character`
        // calls during normal gameplay (refreshing the snapshot) keep the
        // original anchor. Cleared by `flush_play_time` on quit.
        if session.character.is_none() && session.session_started_at.is_none() {
            session.session_started_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            );
        }
        session.character = Some(character_data);
        Ok(())
    } else {
        Err(Box::new(rhai::EvalAltResult::ErrorRuntime(
            "Connection not found.".into(),
            Position::NONE,
        )))
    }
}

/// Get the character data for a connection
pub fn get_character_for_connection(
    connections: &SharedConnections,
    connection_id_str: String,
) -> Result<CharacterData, Box<rhai::EvalAltResult>> {
    let connection_id = Uuid::parse_str(&connection_id_str).map_err(|e| {
        Box::new(rhai::EvalAltResult::ErrorRuntime(
            format!("Invalid Connection ID: {}", e).into(),
            Position::NONE,
        ))
    })?;
    let conns = connections.lock().unwrap();
    if let Some(session) = conns.get(&connection_id) {
        session.character.clone().ok_or_else(|| {
            Box::new(rhai::EvalAltResult::ErrorRuntime(
                "No character logged in for this connection.".into(),
                Position::NONE,
            ))
        })
    } else {
        Err(Box::new(rhai::EvalAltResult::ErrorRuntime(
            "Connection not found.".into(),
            Position::NONE,
        )))
    }
}

/// Clear the character data for a connection (logout)
pub fn clear_player_character(
    connections: &SharedConnections,
    connection_id_str: String,
) -> Result<(), Box<rhai::EvalAltResult>> {
    let connection_id = Uuid::parse_str(&connection_id_str).map_err(|e| {
        Box::new(rhai::EvalAltResult::ErrorRuntime(
            format!("Invalid Connection ID: {}", e).into(),
            Position::NONE,
        ))
    })?;
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        session.character = None;
        Ok(())
    } else {
        Err(Box::new(rhai::EvalAltResult::ErrorRuntime(
            "Connection not found.".into(),
            Position::NONE,
        )))
    }
}

/// Disconnect a client by removing their session
pub fn disconnect_client(
    connections: &SharedConnections,
    connection_id_str: String,
) -> Result<(), Box<rhai::EvalAltResult>> {
    let connection_id = Uuid::parse_str(&connection_id_str).map_err(|e| {
        Box::new(rhai::EvalAltResult::ErrorRuntime(
            format!("Invalid Connection ID: {}", e).into(),
            Position::NONE,
        ))
    })?;
    let mut conns = connections.lock().unwrap();
    if conns.remove(&connection_id).is_some() {
        Ok(())
    } else {
        Err(Box::new(rhai::EvalAltResult::ErrorRuntime(
            "Connection not found.".into(),
            Position::NONE,
        )))
    }
}

/// Send a message to a specific client
pub fn send_client_message(connections: &SharedConnections, connection_id_str: String, message: String) {
    if let Ok(connection_id) = Uuid::parse_str(&connection_id_str) {
        let conns = connections.lock().unwrap();
        if let Some(session) = conns.get(&connection_id) {
            let _ = session.sender.send(message + "\n");
        }
    }
}

/// Find a player's connection ID by their character name (case-insensitive)
pub fn find_player_connection_by_name(connections: &SharedConnections, player_name: &str) -> Option<ConnectionId> {
    let conns = connections.lock().unwrap();
    for (id, session) in conns.iter() {
        if let Some(ref character) = session.character {
            if character.name.eq_ignore_ascii_case(player_name) {
                return Some(*id);
            }
        }
    }
    None
}

/// Lowercased names of every player currently connected with a character in
/// play. The source of truth for "who is actually online" — a character whose
/// `CharacterData.current_room_id` still points into a room (because it
/// persists across logout) is NOT present unless their name is in this set.
/// Used by the DG people rosters so they don't list logged-off characters.
pub fn online_character_names(connections: &SharedConnections) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    if let Ok(conns) = connections.lock() {
        for session in conns.values() {
            if let Some(ref character) = session.character {
                names.insert(character.name.to_ascii_lowercase());
            }
        }
    }
    names
}
