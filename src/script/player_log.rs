// src/script/player_log.rs
// Read-side Rhai functions for the player activity log (`crate::player_log`).
// Used by the admin-only `playlog` command.

use crate::db::Db;
use crate::player_log::{PlayerLogEntry, PlayerSessionSummary};
use rhai::Engine;
use std::sync::Arc;

fn entry_map(e: PlayerLogEntry) -> rhai::Dynamic {
    let mut m = rhai::Map::new();
    m.insert("ts".into(), rhai::Dynamic::from(e.ts));
    m.insert("session_id".into(), rhai::Dynamic::from(e.session_id));
    m.insert("kind".into(), rhai::Dynamic::from(e.kind.as_str().to_string()));
    m.insert("room".into(), rhai::Dynamic::from(e.room.unwrap_or_default()));
    m.insert("text".into(), rhai::Dynamic::from(e.text));
    m.insert("outcome".into(), rhai::Dynamic::from(e.outcome.unwrap_or_default()));
    rhai::Dynamic::from(m)
}

fn session_map(s: PlayerSessionSummary) -> rhai::Dynamic {
    let mut m = rhai::Map::new();
    m.insert("name".into(), rhai::Dynamic::from(s.name));
    m.insert("session_id".into(), rhai::Dynamic::from(s.session_id));
    m.insert("ip".into(), rhai::Dynamic::from(s.ip));
    m.insert("client".into(), rhai::Dynamic::from(s.client.unwrap_or_default()));
    m.insert("started_at".into(), rhai::Dynamic::from(s.started_at));
    m.insert("ended_at".into(), rhai::Dynamic::from(s.ended_at));
    m.insert(
        "seconds".into(),
        rhai::Dynamic::from((s.ended_at - s.started_at).max(0)),
    );
    m.insert("end_reason".into(), rhai::Dynamic::from(s.end_reason));
    m.insert("commands".into(), rhai::Dynamic::from(s.commands as i64));
    m.insert(
        "unknown_commands".into(),
        rhai::Dynamic::from(s.unknown_commands as i64),
    );
    rhai::Dynamic::from(m)
}

/// The `index`-th most recent session (0 = latest, including one still in
/// progress) for a character, oldest → newest. Empty when out of range.
pub fn session_entries(db: &Db, char_name: &str, index: usize) -> Vec<PlayerLogEntry> {
    let all = db
        .list_player_log(char_name, 0, crate::player_log::PLAYER_LOG_MAX_PER_CHAR)
        .unwrap_or_default();
    let mut seen: Vec<&str> = Vec::new();
    for e in all.iter().rev() {
        if !seen.contains(&e.session_id.as_str()) {
            seen.push(&e.session_id);
        }
    }
    let Some(target) = seen.get(index).map(|s| s.to_string()) else {
        return Vec::new();
    };
    all.into_iter().filter(|e| e.session_id == target).collect()
}

pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // list_player_sessions(name, limit) -> Array of Maps, newest first.
    // name "" = every player. Only finished sessions appear here.
    let cloned_db = db.clone();
    engine.register_fn("list_player_sessions", move |name: String, limit: i64| -> rhai::Array {
        let cap = if limit <= 0 { 20 } else { limit as usize };
        let filter = if name.is_empty() { None } else { Some(name.as_str()) };
        cloned_db
            .list_player_sessions(filter, cap)
            .unwrap_or_default()
            .into_iter()
            .map(session_map)
            .collect()
    });

    // get_player_log(name, limit) -> Array of Maps, oldest → newest.
    let cloned_db = db.clone();
    engine.register_fn("get_player_log", move |name: String, limit: i64| -> rhai::Array {
        let cap = if limit <= 0 { 50 } else { limit as usize };
        cloned_db
            .list_player_log(&name, 0, cap)
            .unwrap_or_default()
            .into_iter()
            .map(entry_map)
            .collect()
    });

    // get_player_session_log(name, index) -> Array of Maps for the index-th
    // most recent session (0 = latest, live sessions included).
    let cloned_db = db.clone();
    engine.register_fn(
        "get_player_session_log",
        move |name: String, index: i64| -> rhai::Array {
            session_entries(&cloned_db, &name, index.max(0) as usize)
                .into_iter()
                .map(entry_map)
                .collect()
        },
    );
}
