//! Rhai bindings for the leaderboard cache.
//!
//! Every function here is a read of `World.leaderboards` and nothing else —
//! no database access, no ranking, no character load. The scan that produces
//! the cache is a tick (`ironmud::leaderboard::process_leaderboard_tick`);
//! this is deliberately the cheap half, because `top` is a command and a
//! command must never pay for a full character sweep.
//!
//! Boards are returned as plain Rhai maps rather than registered types: the
//! shape is a table of strings and numbers with no behaviour, and a script
//! that wants to lay it out differently should not have to go through an API.

use rhai::{Array, Dynamic, Engine, Map};

use crate::SharedState;
use crate::leaderboard::Board;

/// One board as a script sees it. `entries` is already ranked and truncated.
fn board_to_map(board: &Board, include_entries: bool) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(true));
    m.insert("key".into(), Dynamic::from(board.key.clone()));
    m.insert("label".into(), Dynamic::from(board.label.clone()));
    m.insert("group".into(), Dynamic::from(board.group.clone()));
    m.insert("kind".into(), Dynamic::from(board.kind.key().to_string()));
    m.insert("ranked".into(), Dynamic::from(board.ranked as i64));

    if include_entries {
        let entries: Array = board
            .entries
            .iter()
            .map(|e| {
                let mut row = Map::new();
                row.insert("rank".into(), Dynamic::from(e.rank as i64));
                row.insert("name".into(), Dynamic::from(e.name.clone()));
                row.insert("value".into(), Dynamic::from(e.value));
                row.insert("display".into(), Dynamic::from(e.display.clone()));
                Dynamic::from(row)
            })
            .collect();
        m.insert("entries".into(), Dynamic::from(entries));
    }
    m
}

fn not_found(key: &str) -> Map {
    let mut m = Map::new();
    m.insert("found".into(), Dynamic::from(false));
    m.insert("key".into(), Dynamic::from(key.to_string()));
    m.insert("label".into(), Dynamic::from(String::new()));
    m.insert("group".into(), Dynamic::from(String::new()));
    m.insert("kind".into(), Dynamic::from(String::new()));
    m.insert("ranked".into(), Dynamic::from(0_i64));
    m.insert("entries".into(), Dynamic::from(Array::new()));
    m
}

pub fn register(engine: &mut Engine, state: SharedState) {
    // get_leaderboard(key) -> Map
    //
    // #{found, key, label, group, kind, ranked, entries: [#{rank, name,
    // value, display}]}. `key` is matched loosely — exact key, then label,
    // then a prefix of either — so "top kills" finds kills.any without an
    // alias table.
    let cloned_state = state.clone();
    engine.register_fn("get_leaderboard", move |key: String| -> Map {
        let Ok(world) = cloned_state.lock() else {
            return not_found(&key);
        };
        match world.leaderboards.resolve(&key) {
            Some(b) => board_to_map(b, true),
            None => not_found(&key),
        }
    });

    // get_leaderboard_index() -> Array of #{key, label, group, kind, ranked}
    //
    // Every board that exists, in display order (group, then label). No
    // entries — this is the index, and a world with hundreds of per-mobile
    // kill boards would otherwise return the whole cache to draw a menu.
    let cloned_state = state.clone();
    engine.register_fn("get_leaderboard_index", move || -> Array {
        let Ok(world) = cloned_state.lock() else {
            return Array::new();
        };
        let boards = &world.leaderboards;
        boards
            .ordered_keys()
            .into_iter()
            .filter_map(|k| boards.boards.get(k))
            .map(|b| Dynamic::from(board_to_map(b, false)))
            .collect()
    });

    // get_leaderboard_placings(char_name) -> Array of
    //   #{key, label, group, kind, ranked, rank}
    //
    // Every board this character ranks on, best placing first — including the
    // ones they are nowhere near the top of. This is the part a top-ten list
    // cannot do: a player 300th at killing may be 2nd at cooking, and nothing
    // else in the game will ever tell them so.
    let cloned_state = state.clone();
    engine.register_fn("get_leaderboard_placings", move |char_name: String| -> Array {
        let Ok(world) = cloned_state.lock() else {
            return Array::new();
        };
        world
            .leaderboards
            .placings_for(&char_name)
            .into_iter()
            .map(|(b, rank)| {
                let mut m = board_to_map(b, false);
                m.insert("rank".into(), Dynamic::from(rank as i64));
                Dynamic::from(m)
            })
            .collect()
    });

    // get_leaderboard_placing(char_name, key) -> i64
    //
    // This character's placing on one board, or 0 if they do not rank on it.
    let cloned_state = state.clone();
    engine.register_fn(
        "get_leaderboard_placing",
        move |char_name: String, key: String| -> i64 {
            let Ok(world) = cloned_state.lock() else {
                return 0;
            };
            match world.leaderboards.resolve(&key) {
                Some(b) => b.placing(&char_name).unwrap_or(0) as i64,
                None => 0,
            }
        },
    );

    // get_leaderboard_generated_at() -> i64
    //
    // Unix seconds of the last scan. 0 means the first scan has not landed
    // yet, which is the only "not ready" state a script has to handle.
    let cloned_state = state.clone();
    engine.register_fn("get_leaderboard_generated_at", move || -> i64 {
        match cloned_state.lock() {
            Ok(world) => world.leaderboards.generated_at,
            Err(_) => 0,
        }
    });

    // get_leaderboard_population() -> i64
    //
    // Characters the last scan considered, after admins were excluded.
    let cloned_state = state.clone();
    engine.register_fn("get_leaderboard_population", move || -> i64 {
        match cloned_state.lock() {
            Ok(world) => world.leaderboards.characters_scanned as i64,
            Err(_) => 0,
        }
    });
}
