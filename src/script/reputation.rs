//! Rhai bindings for faction reputation.
//!
//! Reads are cheap and per-faction so the `standing` command can render its
//! own layout; writes go through `crate::reputation::apply_delta`, the same IO
//! chokepoint kill credit and quest rewards use, so a script cannot move
//! standing without the opposition transfer, the session sync and the band
//! announcement coming with it.

use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: crate::SharedConnections, state: crate::SharedState) {
    // get_reputation(char_name, faction) -> i64
    // Raw standing. A faction the player has never dealt with reads 0.
    let cloned_db = db.clone();
    engine.register_fn("get_reputation", move |char_name: String, faction: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction) as i64,
            _ => 0,
        }
    });

    // get_reputation_tier(char_name, faction) -> String
    // The band key ("accepted", "hostile"). For display use
    // get_reputation_label; this is the stable identifier for script logic.
    let cloned_db = db.clone();
    engine.register_fn(
        "get_reputation_tier",
        move |char_name: String, faction: String| -> String {
            let v = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction),
                _ => 0,
            };
            crate::reputation::ReputationTier::from_value(v).key().to_string()
        },
    );

    // get_reputation_label(char_name, faction) -> String
    let cloned_db = db.clone();
    engine.register_fn(
        "get_reputation_label",
        move |char_name: String, faction: String| -> String {
            let v = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction),
                _ => 0,
            };
            crate::reputation::ReputationTier::from_value(v).label().to_string()
        },
    );

    // get_reputation_description(char_name, faction) -> String
    // The one-line "what this standing means" text for the current band.
    let cloned_db = db.clone();
    engine.register_fn(
        "get_reputation_description",
        move |char_name: String, faction: String| -> String {
            let v = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction),
                _ => 0,
            };
            crate::reputation::ReputationTier::from_value(v)
                .description()
                .to_string()
        },
    );

    // get_reputation_factions(char_name) -> Array of faction keys
    //
    // Only factions the character has actually dealt with, best standing
    // first. A world can carry hundreds of faction tags; listing every one at
    // Neutral would bury the handful that mean something.
    let cloned_db = db.clone();
    engine.register_fn("get_reputation_factions", move |char_name: String| -> rhai::Array {
        let Ok(Some(ch)) = cloned_db.get_character_data(&char_name.to_lowercase()) else {
            return rhai::Array::new();
        };
        let mut rows: Vec<(String, i32)> = ch.reputation.into_iter().collect();
        // Ties broken by name so the list is stable between renders.
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        rows.into_iter().map(|(k, _)| rhai::Dynamic::from(k)).collect()
    });

    // get_faction_name(faction) -> String
    // Display name, falling back to the key for tags no definition declares.
    let name_state = state.clone();
    engine.register_fn("get_faction_name", move |faction: String| -> String {
        crate::reputation::definition(&name_state, &faction)
            .display()
            .to_string()
    });

    // get_faction_description(faction) -> String
    let desc_state = state.clone();
    engine.register_fn("get_faction_description", move |faction: String| -> String {
        crate::reputation::definition(&desc_state, &faction).description.clone()
    });

    // get_faction_opposed(faction) -> Array of faction keys
    let opp_state = state.clone();
    engine.register_fn("get_faction_opposed", move |faction: String| -> rhai::Array {
        crate::reputation::definition(&opp_state, &faction)
            .opposed
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect()
    });

    // list_faction_keys() -> Array of every declared faction key, sorted.
    // For builder tooling and tab completion, not for the standing display.
    let list_state = state.clone();
    engine.register_fn("list_faction_keys", move || -> rhai::Array {
        let mut keys: Vec<String> = match list_state.lock() {
            Ok(w) => w.faction_definitions.keys().cloned().collect(),
            Err(_) => Vec::new(),
        };
        keys.sort();
        keys.into_iter().map(rhai::Dynamic::from).collect()
    });

    // adjust_reputation(char_name, faction, delta) -> i64
    //
    // The new standing with `faction`, or the unchanged one if nothing moved.
    // Routes through the IO chokepoint, so opposed factions move too and the
    // player is told when a band is crossed.
    let cloned_db = db.clone();
    let conns = connections.clone();
    let adj_state = state.clone();
    engine.register_fn(
        "adjust_reputation",
        move |char_name: String, faction: String, delta: i64| -> i64 {
            let delta = crate::reputation::clamp(delta.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
            crate::reputation::apply_delta(&cloned_db, &conns, &adj_state, &char_name, &faction, delta);
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction) as i64,
                _ => 0,
            }
        },
    );

    // set_reputation(char_name, faction, value) -> bool
    //
    // Admin-only in practice: assigns an absolute standing without the
    // opposition transfer, because "set this to exactly N" is a correction,
    // not an act in the world.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_reputation",
        move |char_name: String, faction: String, value: i64| -> bool {
            let Some(key) = crate::reputation::normalize(Some(&faction)) else {
                return false;
            };
            let Ok(Some(mut ch)) = cloned_db.get_character_data(&char_name.to_lowercase()) else {
                return false;
            };
            let v = crate::reputation::clamp(value.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
            if v == 0 {
                ch.reputation.remove(&key);
            } else {
                ch.reputation.insert(key, v);
            }
            cloned_db.save_character_data(ch).is_ok()
        },
    );
}
