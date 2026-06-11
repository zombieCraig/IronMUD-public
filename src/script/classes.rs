// src/script/classes.rs
// Rhai bindings for the `admin loadout class` class loadout editor. Mutates
// in-memory ClassDefinition fields AND persists a ClassLoadout override row in
// the `class_loadouts` sled tree so edits survive restart.

use crate::SharedState;
use crate::db::Db;
use crate::types::ClassLoadout;
use rhai::Engine;
use std::sync::Arc;

/// Persist the current `starting_items` + `starting_gold` for `class_id` to
/// the database. Called from every setter so the in-memory edit survives
/// restart via the overlay in `init::data::load_game_data`.
fn persist_loadout(db: &Db, state: &SharedState, class_id: &str) -> bool {
    let world = state.lock().unwrap();
    let Some(def) = world.class_definitions.get(class_id) else {
        return false;
    };
    let loadout = ClassLoadout {
        class_id: class_id.to_string(),
        starting_items: def.starting_items.clone(),
        starting_gold: def.starting_gold,
    };
    db.save_class_loadout(loadout).is_ok()
}

pub fn register(engine: &mut Engine, db: Arc<Db>, state: SharedState) {
    // class_exists(class_id) -> bool
    let state_clone = state.clone();
    engine.register_fn("class_exists", move |class_id: String| -> bool {
        let world = state_clone.lock().unwrap();
        world.class_definitions.contains_key(&class_id)
    });

    // list_class_ids() -> Array<String>
    let state_clone = state.clone();
    engine.register_fn("list_class_ids", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        let mut ids: Vec<String> = world.class_definitions.keys().cloned().collect();
        ids.sort();
        ids.into_iter().map(rhai::Dynamic::from).collect()
    });

    // get_class_starting_gold(class_id) -> i64
    let state_clone = state.clone();
    engine.register_fn("get_class_starting_gold", move |class_id: String| -> i64 {
        let world = state_clone.lock().unwrap();
        world
            .class_definitions
            .get(&class_id)
            .map(|c| c.starting_gold as i64)
            .unwrap_or(0)
    });

    // get_class_starting_items(class_id) -> Array<String>
    let state_clone = state.clone();
    engine.register_fn("get_class_starting_items", move |class_id: String| -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .class_definitions
            .get(&class_id)
            .map(|c| c.starting_items.iter().cloned().map(rhai::Dynamic::from).collect())
            .unwrap_or_default()
    });

    // set_class_starting_gold(class_id, amount) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "set_class_starting_gold",
        move |class_id: String, amount: i64| -> bool {
            if amount < 0 {
                return false;
            }
            {
                let mut world = cloned_state.lock().unwrap();
                let Some(def) = world.class_definitions.get_mut(&class_id) else {
                    return false;
                };
                def.starting_gold = amount as i32;
            }
            persist_loadout(&cloned_db, &cloned_state, &class_id)
        },
    );

    // add_class_starting_item(class_id, vnum) -> bool
    // Caller is expected to have validated the vnum (admin loadout does so via
    // item_vnum_exists). The setter still rejects empty strings and dupes.
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "add_class_starting_item",
        move |class_id: String, vnum: String| -> bool {
            let vnum = vnum.trim().to_string();
            if vnum.is_empty() {
                return false;
            }
            {
                let mut world = cloned_state.lock().unwrap();
                let Some(def) = world.class_definitions.get_mut(&class_id) else {
                    return false;
                };
                if def.starting_items.iter().any(|v| v == &vnum) {
                    return false; // already present
                }
                def.starting_items.push(vnum);
            }
            persist_loadout(&cloned_db, &cloned_state, &class_id)
        },
    );

    // remove_class_starting_item(class_id, vnum) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn(
        "remove_class_starting_item",
        move |class_id: String, vnum: String| -> bool {
            let vnum_lower = vnum.to_lowercase();
            let removed = {
                let mut world = cloned_state.lock().unwrap();
                let Some(def) = world.class_definitions.get_mut(&class_id) else {
                    return false;
                };
                let before = def.starting_items.len();
                def.starting_items.retain(|v| v.to_lowercase() != vnum_lower);
                before != def.starting_items.len()
            };
            if !removed {
                return false;
            }
            persist_loadout(&cloned_db, &cloned_state, &class_id)
        },
    );

    // clear_class_starting_items(class_id) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("clear_class_starting_items", move |class_id: String| -> bool {
        {
            let mut world = cloned_state.lock().unwrap();
            let Some(def) = world.class_definitions.get_mut(&class_id) else {
                return false;
            };
            def.starting_items.clear();
        }
        persist_loadout(&cloned_db, &cloned_state, &class_id)
    });

    // item_vnum_exists(vnum) -> bool — `admin loadout` uses this to validate
    // before accepting an `items add` operation (shared by the race editor too).
    // Reads the items tree directly so we don't depend on world state.
    let cloned_db = db.clone();
    engine.register_fn("item_vnum_exists", move |vnum: String| -> bool {
        cloned_db
            .get_item_by_vnum(&vnum)
            .ok()
            .flatten()
            .map(|i| i.is_prototype)
            .unwrap_or(false)
    });
}
