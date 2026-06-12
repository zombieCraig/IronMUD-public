//! Rhai bindings for mutant state on player characters.
//!
//! PC-only surface (mobs don't carry `MutantState`). Connection-id keyed,
//! mirroring `src/script/replicant.rs`. Every mutator follows the
//! session-authoritative pattern: lock connections, mutate
//! `session.character`, then `db.save_character_data`.
//!
//! `activate_pc_mutation` is transactional (ownership/MP gates, the spend,
//! the misfire table, passive re-stamp for Overload-granted mutations) so
//! `mutate.rhai` stays a thin renderer of the returned map.
//!
//! Deadlock note: definitions live in World, characters in Connections.
//! Functions needing both clone the definitions out of a short World lock
//! BEFORE touching the connections lock — never hold both.

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::types::{MutantState, MutationDefinition};
use rhai::Engine;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn err_map(error: &str) -> rhai::Map {
    let mut m = rhai::Map::new();
    m.insert("success".into(), rhai::Dynamic::from(false));
    m.insert("error".into(), rhai::Dynamic::from(error.to_string()));
    m
}

fn clone_defs(state: &SharedState) -> HashMap<String, MutationDefinition> {
    state.lock().map(|w| w.mutation_definitions.clone()).unwrap_or_default()
}

fn def_to_map(def: &MutationDefinition) -> rhai::Map {
    let mut m = rhai::Map::new();
    m.insert("id".into(), rhai::Dynamic::from(def.id.clone()));
    m.insert("name".into(), rhai::Dynamic::from(def.name.clone()));
    m.insert("description".into(), rhai::Dynamic::from(def.description.clone()));
    m.insert("activation".into(), rhai::Dynamic::from(def.activation.clone()));
    m.insert("effect".into(), rhai::Dynamic::from(def.effect.clone()));
    m.insert(
        "damage_type".into(),
        rhai::Dynamic::from(def.damage_type.clone().unwrap_or_default()),
    );
    m.insert("base_power".into(), rhai::Dynamic::from(def.base_power as i64));
    m.insert("power_per_mp".into(), rhai::Dynamic::from(def.power_per_mp as i64));
    m.insert("duration_secs".into(), rhai::Dynamic::from(def.duration_secs));
    m.insert("duration_per_mp".into(), rhai::Dynamic::from(def.duration_per_mp));
    for key in ["self", "room", "target", "examine"] {
        m.insert(
            format!("msg_{}", key).into(),
            rhai::Dynamic::from(def.messages.get(key).cloned().unwrap_or_default()),
        );
    }
    m
}

pub fn register_mutant_functions(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // is_pc_mutant(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_mutant", move |connection_id: String| -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        conns_lock
            .get(&conn_id)
            .and_then(|s| s.character.as_ref())
            .map(|c| c.mutant_state.is_some())
            .unwrap_or(false)
    });

    // init_pc_mutant(connection_id) -> String
    // Stamps a fresh MutantState if absent and rolls the ONE random starting
    // mutation, stamping its passive buffs/traits. Idempotent: an existing
    // mutant keeps their state. Returns the (new or existing first) mutation
    // id, or "" on failure. Shared by creation, login migration, admin.
    let conns = connections.clone();
    let cdb = db.clone();
    let cstate = state.clone();
    engine.register_fn("init_pc_mutant", move |connection_id: String| -> String {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let defs = clone_defs(&cstate);
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return String::new(),
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return String::new(),
        };
        if ch.mutant_state.is_none() {
            let mut s = MutantState::newly_mutated(now_secs());
            let all: Vec<String> = defs.keys().cloned().collect();
            use rand::Rng;
            let pick = rand::thread_rng().gen_range(0..usize::MAX);
            if let Some(rolled) = crate::mutant::roll_random_mutation(&all, &[], pick) {
                s.mutations.push(rolled);
            }
            ch.mutant_state = Some(s);
        }
        crate::mutant::ensure_passive_effects(ch, &defs);
        let first = ch
            .mutant_state
            .as_ref()
            .and_then(|s| s.mutations.first().cloned())
            .unwrap_or_default();
        if cdb.save_character_data(ch.clone()).is_ok() {
            first
        } else {
            String::new()
        }
    });

    // revoke_pc_mutant(connection_id) -> bool (admin/testing symmetry)
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("revoke_pc_mutant", move |connection_id: String| -> bool {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return false,
        };
        if ch.mutant_state.is_none() {
            return false;
        }
        ch.mutant_state = None;
        // Permanent mutation buffs go with the state; traits stay (scars).
        ch.active_buffs
            .retain(|b| b.source != crate::mutant::MUTATION_BUFF_SOURCE);
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // get_pc_mp(connection_id) -> i64. -1 for non-mutants.
    let conns = connections.clone();
    engine.register_fn("get_pc_mp", move |connection_id: String| -> i64 {
        read_mutant(&conns, &connection_id, |s| s.mp as i64).unwrap_or(-1)
    });

    // get_pc_max_mp(connection_id) -> i64. 0 for non-mutants.
    let conns = connections.clone();
    engine.register_fn("get_pc_max_mp", move |connection_id: String| -> i64 {
        read_mutant(&conns, &connection_id, |s| s.max_mp as i64).unwrap_or(0)
    });

    // change_pc_mp(connection_id, delta) -> i64
    // Clamped to [0, max_mp]; returns the new value (-1 if not a mutant).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("change_pc_mp", move |connection_id: String, delta: i64| -> i64 {
        mutate_mutant(&conns, &cdb, &connection_id, |_ch, s| s.change_mp(delta as i32) as i64).unwrap_or(-1)
    });

    // get_pc_mutations(connection_id) -> Array of mutation ids
    let conns = connections.clone();
    engine.register_fn("get_pc_mutations", move |connection_id: String| -> rhai::Array {
        read_mutant(&conns, &connection_id, |s| {
            s.mutations
                .iter()
                .map(|m| rhai::Dynamic::from(m.clone()))
                .collect::<rhai::Array>()
        })
        .unwrap_or_default()
    });

    // pc_has_mutation(connection_id, mutation_id) -> bool
    let conns = connections.clone();
    engine.register_fn(
        "pc_has_mutation",
        move |connection_id: String, mutation_id: String| -> bool {
            read_mutant(&conns, &connection_id, |s| s.has_mutation(&mutation_id)).unwrap_or(false)
        },
    );

    // get_pc_deformities(connection_id) -> Array of strings
    let conns = connections.clone();
    engine.register_fn("get_pc_deformities", move |connection_id: String| -> rhai::Array {
        read_mutant(&conns, &connection_id, |s| {
            s.deformities
                .iter()
                .map(|d| rhai::Dynamic::from(d.clone()))
                .collect::<rhai::Array>()
        })
        .unwrap_or_default()
    });

    // get_mutation_list() -> Array of Maps (every loaded definition)
    let lstate = state.clone();
    engine.register_fn("get_mutation_list", move || -> rhai::Array {
        let defs = clone_defs(&lstate);
        let mut ids: Vec<&String> = defs.keys().collect();
        ids.sort();
        ids.iter()
            .map(|id| rhai::Dynamic::from(def_to_map(&defs[*id])))
            .collect()
    });

    // get_mutation_info(mutation_id) -> Map ({} when unknown)
    let lstate = state.clone();
    engine.register_fn("get_mutation_info", move |mutation_id: String| -> rhai::Map {
        let defs = clone_defs(&lstate);
        defs.get(&mutation_id).map(|d| def_to_map(d)).unwrap_or_default()
    });

    // activate_pc_mutation(connection_id, mutation_id, mp) -> Map
    //   { success, error?, power, duration_secs, mp_left, misfire,
    //     misfire_kind, misfire_damage, deformity, new_mutation, stat_lost,
    //     max_mp_increased, + def fields (name, effect, msg_self, ...) }
    // The whole transaction: gates, MP spend, misfire table, passive
    // re-stamp when Overload grants a new passive mutation, save.
    let conns = connections.clone();
    let cdb = db.clone();
    let astate = state.clone();
    engine.register_fn(
        "activate_pc_mutation",
        move |connection_id: String, mutation_id: String, mp: i64| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            let defs = clone_defs(&astate);
            let def = match defs.get(&mutation_id) {
                Some(d) => d.clone(),
                None => return err_map("unknown mutation"),
            };
            if def.activation != "active" {
                return err_map("passive");
            }
            let mp_spent = mp.clamp(1, 10) as i32;
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            match ch.mutant_state.as_ref() {
                None => return err_map("not a mutant"),
                Some(s) if !s.has_mutation(&mutation_id) => return err_map("not owned"),
                Some(s) if s.mp < mp_spent => {
                    let mut m = err_map("not enough mp");
                    m.insert("mp".into(), rhai::Dynamic::from(s.mp as i64));
                    return m;
                }
                Some(_) => {}
            }
            let all: Vec<String> = defs.keys().cloned().collect();
            let dice = crate::mutant::activation_dice_rolled(mp_spent);
            let out = crate::mutant::activate_mutation(ch, &def, mp_spent, &all, &dice);
            // An Overload may have granted a passive mutation — stamp it now.
            if out.new_mutation.is_some() {
                crate::mutant::ensure_passive_effects(ch, &defs);
            }
            let _ = cdb.save_character_data(ch.clone());

            let mut m = def_to_map(&def);
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("power".into(), rhai::Dynamic::from(out.power as i64));
            m.insert("duration_secs".into(), rhai::Dynamic::from(out.duration_secs));
            m.insert("mp_left".into(), rhai::Dynamic::from(out.mp_left as i64));
            m.insert("mp_spent".into(), rhai::Dynamic::from(mp_spent as i64));
            m.insert("misfire".into(), rhai::Dynamic::from(out.misfire));
            m.insert(
                "misfire_kind".into(),
                rhai::Dynamic::from(out.misfire_kind.map(|k| k.to_string()).unwrap_or_default()),
            );
            m.insert("misfire_damage".into(), rhai::Dynamic::from(out.misfire_damage as i64));
            m.insert(
                "deformity".into(),
                rhai::Dynamic::from(out.deformity.unwrap_or_default()),
            );
            let new_mutation_name = out
                .new_mutation
                .as_ref()
                .and_then(|id| defs.get(id))
                .map(|d| d.name.clone())
                .unwrap_or_default();
            m.insert(
                "new_mutation".into(),
                rhai::Dynamic::from(out.new_mutation.unwrap_or_default()),
            );
            m.insert("new_mutation_name".into(), rhai::Dynamic::from(new_mutation_name));
            m.insert(
                "stat_lost".into(),
                rhai::Dynamic::from(out.stat_lost.unwrap_or_default()),
            );
            m.insert("max_mp_increased".into(), rhai::Dynamic::from(out.max_mp_increased));
            m
        },
    );

    // get_pc_rot(connection_id) -> i64 (any race; transient points)
    let conns = connections.clone();
    engine.register_fn("get_pc_rot", move |connection_id: String| -> i64 {
        read_character(&conns, &connection_id, |c| c.rot_points as i64).unwrap_or(0)
    });

    // get_pc_permanent_rot(connection_id) -> i64 (any race)
    let conns = connections.clone();
    engine.register_fn("get_pc_permanent_rot", move |connection_id: String| -> i64 {
        read_character(&conns, &connection_id, |c| c.permanent_rot_points as i64).unwrap_or(0)
    });
}

/// Read helper: run `f` on the session's character if present.
fn read_character<T>(
    conns: &SharedConnections,
    connection_id: &str,
    f: impl FnOnce(&crate::types::CharacterData) -> T,
) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock.get(&conn_id).and_then(|s| s.character.as_ref()).map(f)
}

/// Read helper: run `f` on the session's MutantState if present.
fn read_mutant<T>(conns: &SharedConnections, connection_id: &str, f: impl FnOnce(&MutantState) -> T) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock
        .get(&conn_id)
        .and_then(|s| s.character.as_ref())
        .and_then(|c| c.mutant_state.as_ref())
        .map(f)
}

/// Mutate helper: run `f` on (character, mutant_state) and save.
fn mutate_mutant<T>(
    conns: &SharedConnections,
    db: &Arc<Db>,
    connection_id: &str,
    f: impl FnOnce(&mut crate::types::CharacterData, &mut MutantState) -> T,
) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let mut conns_lock = conns.lock().ok()?;
    let ch = conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut())?;
    let mut state = ch.mutant_state.take()?;
    let out = f(ch, &mut state);
    ch.mutant_state = Some(state);
    let _ = db.save_character_data(ch.clone());
    Some(out)
}
