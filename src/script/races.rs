// src/script/races.rs
// Rhai bindings for the `admin loadout race` starting-kit editor. Mutates
// in-memory RaceDefinition fields AND persists a RaceLoadout override row in
// the `race_loadouts` sled tree so edits survive restart. Direct mirror of
// src/script/classes.rs. Race ids are lowercase-normalized to match the
// race_definitions map keys (see get_race_info in characters.rs).

use crate::SharedState;
use crate::db::Db;
use crate::types::RaceLoadout;
use rhai::Engine;
use std::sync::Arc;

/// Persist the current `starting_items` + `starting_gold` for `race_id` to the
/// database. Called from every setter so the in-memory edit survives restart
/// via the overlay in `init::data::load_game_data`.
fn persist_loadout(db: &Db, state: &SharedState, race_id: &str) -> bool {
    let world = state.lock().unwrap();
    let Some(def) = world.race_definitions.get(race_id) else {
        return false;
    };
    let loadout = RaceLoadout {
        race_id: race_id.to_string(),
        starting_items: def.starting_items.clone(),
        starting_gold: def.starting_gold,
    };
    db.save_race_loadout(loadout).is_ok()
}

pub fn register(engine: &mut Engine, db: Arc<Db>, state: SharedState) {
    // race_exists(race_id) -> bool
    let state_clone = state.clone();
    engine.register_fn("race_exists", move |race_id: String| -> bool {
        let world = state_clone.lock().unwrap();
        world.race_definitions.contains_key(&race_id.to_lowercase())
    });

    // list_race_ids() -> Array<String>
    // All race ids regardless of `available` — the editor must reach hidden
    // races. (get_race_list filters on availability and is the wrong primitive.)
    let state_clone = state.clone();
    engine.register_fn("list_race_ids", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        let mut ids: Vec<String> = world.race_definitions.keys().cloned().collect();
        ids.sort();
        ids.into_iter().map(rhai::Dynamic::from).collect()
    });

    // get_race_starting_gold(race_id) -> i64
    let state_clone = state.clone();
    engine.register_fn("get_race_starting_gold", move |race_id: String| -> i64 {
        let world = state_clone.lock().unwrap();
        world
            .race_definitions
            .get(&race_id.to_lowercase())
            .map(|r| r.starting_gold as i64)
            .unwrap_or(0)
    });

    // get_race_starting_items(race_id) -> Array<String>
    let state_clone = state.clone();
    engine.register_fn("get_race_starting_items", move |race_id: String| -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .race_definitions
            .get(&race_id.to_lowercase())
            .map(|r| r.starting_items.iter().cloned().map(rhai::Dynamic::from).collect())
            .unwrap_or_default()
    });

    // set_race_starting_gold(race_id, amount) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_race_starting_gold", move |race_id: String, amount: i64| -> bool {
        if amount < 0 {
            return false;
        }
        let race_id = race_id.to_lowercase();
        {
            let mut world = cloned_state.lock().unwrap();
            let Some(def) = world.race_definitions.get_mut(&race_id) else {
                return false;
            };
            def.starting_gold = amount as i32;
        }
        persist_loadout(&cloned_db, &cloned_state, &race_id)
    });

    // add_race_starting_item(race_id, vnum) -> bool
    // Caller is expected to have validated the vnum (admin loadout does so via
    // item_vnum_exists). The setter still rejects empty strings and dupes.
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("add_race_starting_item", move |race_id: String, vnum: String| -> bool {
        let race_id = race_id.to_lowercase();
        let vnum = vnum.trim().to_string();
        if vnum.is_empty() {
            return false;
        }
        {
            let mut world = cloned_state.lock().unwrap();
            let Some(def) = world.race_definitions.get_mut(&race_id) else {
                return false;
            };
            if def.starting_items.iter().any(|v| v == &vnum) {
                return false; // already present
            }
            def.starting_items.push(vnum);
        }
        persist_loadout(&cloned_db, &cloned_state, &race_id)
    });

    // remove_race_starting_item(race_id, vnum) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "remove_race_starting_item",
        move |race_id: String, vnum: String| -> bool {
            let race_id = race_id.to_lowercase();
            let vnum_lower = vnum.to_lowercase();
            let removed = {
                let mut world = cloned_state.lock().unwrap();
                let Some(def) = world.race_definitions.get_mut(&race_id) else {
                    return false;
                };
                let before = def.starting_items.len();
                def.starting_items.retain(|v| v.to_lowercase() != vnum_lower);
                before != def.starting_items.len()
            };
            if !removed {
                return false;
            }
            persist_loadout(&cloned_db, &cloned_state, &race_id)
        },
    );

    // clear_race_starting_items(race_id) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("clear_race_starting_items", move |race_id: String| -> bool {
        let race_id = race_id.to_lowercase();
        {
            let mut world = cloned_state.lock().unwrap();
            let Some(def) = world.race_definitions.get_mut(&race_id) else {
                return false;
            };
            def.starting_items.clear();
        }
        persist_loadout(&cloned_db, &cloned_state, &race_id)
    });

    // item_vnum_exists(vnum) is registered by src/script/classes.rs and shared.
}
