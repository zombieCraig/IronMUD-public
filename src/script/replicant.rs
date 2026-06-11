//! Rhai bindings for replicant state on player characters.
//!
//! PC-only surface (mobs don't carry `ReplicantState`). Connection-id keyed,
//! mirroring `src/script/vampire.rs`. Every mutator follows the
//! session-authoritative pattern: lock connections, mutate
//! `session.character`, then `db.save_character_data` — the regen tick
//! flushes session→DB, so DB-only writes would be clobbered.
//!
//! Mechanics here are transactional (baseline test, attune, focus, comfort)
//! so command scripts stay thin renderers of the returned maps. Shared
//! breakdown/retirement logic lives in `crate::replicant` because the combat
//! tick needs it too.

use crate::SharedConnections;
use crate::db::Db;
use crate::replicant::{apply_retirement, trigger_breakdown_rolled};
use crate::types::{
    ATTUNE_GRIEF_RESOLVE_COST, BASELINE_FAIL_COOLDOWN_SECS, FOCUS_COOLDOWN_SECS, FOCUS_RESOLVE_RESTORE, ReplicantState,
    STRIKES_FOR_RETIREMENT,
};
use rhai::Engine;
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

pub fn register_replicant_functions(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // is_pc_replicant(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_replicant", move |connection_id: String| -> bool {
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
            .map(|c| c.replicant_state.is_some())
            .unwrap_or(false)
    });

    // init_pc_replicant(connection_id) -> bool
    // Stamps a fresh ReplicantState if absent (idempotent) and pins stamina.
    // Shared by character creation, login migration, and admin tooling.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("init_pc_replicant", move |connection_id: String| -> bool {
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
        if ch.replicant_state.is_none() {
            ch.replicant_state = Some(ReplicantState::newly_incepted(now_secs()));
        }
        ch.stamina = ch.max_stamina;
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // revoke_pc_replicantism(connection_id) -> bool (admin/testing symmetry)
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("revoke_pc_replicantism", move |connection_id: String| -> bool {
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
        if ch.replicant_state.is_none() {
            return false;
        }
        ch.replicant_state = None;
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // get_pc_resolve(connection_id) -> i64. -1 for non-replicants.
    let conns = connections.clone();
    engine.register_fn("get_pc_resolve", move |connection_id: String| -> i64 {
        read_replicant(&conns, &connection_id, |r| r.resolve as i64).unwrap_or(-1)
    });

    // get_pc_max_resolve(connection_id) -> i64. 0 for non-replicants.
    let conns = connections.clone();
    engine.register_fn("get_pc_max_resolve", move |connection_id: String| -> i64 {
        read_replicant(&conns, &connection_id, |r| r.max_resolve as i64).unwrap_or(0)
    });

    // get_pc_baseline_strikes(connection_id) -> i64. -1 for non-replicants.
    let conns = connections.clone();
    engine.register_fn("get_pc_baseline_strikes", move |connection_id: String| -> i64 {
        read_replicant(&conns, &connection_id, |r| r.baseline_strikes as i64).unwrap_or(-1)
    });

    // is_pc_in_breakdown(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_in_breakdown", move |connection_id: String| -> bool {
        let now = now_secs();
        read_replicant(&conns, &connection_id, |r| r.is_breaking_down(now)).unwrap_or(false)
    });

    // get_pc_breakdown_kind(connection_id) -> String ("" if stable)
    let conns = connections.clone();
    engine.register_fn("get_pc_breakdown_kind", move |connection_id: String| -> String {
        let now = now_secs();
        read_replicant(&conns, &connection_id, |r| {
            if r.is_breaking_down(now) {
                r.breakdown_kind.clone().unwrap_or_default()
            } else {
                String::new()
            }
        })
        .unwrap_or_default()
    });

    // get_pc_signature_item_id(connection_id) -> String ("" if none)
    let conns = connections.clone();
    engine.register_fn("get_pc_signature_item_id", move |connection_id: String| -> String {
        read_replicant(&conns, &connection_id, |r| {
            r.signature_item_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .unwrap_or_default()
    });

    // set_pc_max_resolve(connection_id, n) -> bool (admin/progression; n >= 1)
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("set_pc_max_resolve", move |connection_id: String, n: i64| -> bool {
        mutate_replicant(&conns, &cdb, &connection_id, |_ch, r| {
            r.max_resolve = (n as i32).max(1);
            r.resolve = r.resolve.min(r.max_resolve);
            true
        })
        .unwrap_or(false)
    });

    // change_pc_resolve(connection_id, delta) -> i64
    // Clamped to [0, max_resolve]; returns the new value (-1 if not a
    // replicant). Does NOT fire a breakdown — callers check for 0 and call
    // trigger_pc_breakdown so the messaging stays in their hands.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("change_pc_resolve", move |connection_id: String, delta: i64| -> i64 {
        mutate_replicant(&conns, &cdb, &connection_id, |_ch, r| {
            r.change_resolve(delta as i32) as i64
        })
        .unwrap_or(-1)
    });

    // trigger_pc_breakdown(connection_id) -> Map { success, kind, message, room_message }
    // Rolls the critical-stress table and applies effects. No-op (success:
    // false) while a breakdown is already active — no chain re-rolls.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("trigger_pc_breakdown", move |connection_id: String| -> rhai::Map {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return err_map("bad connection id"),
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return err_map("no character"),
        };
        let now = now_secs();
        match ch.replicant_state.as_ref() {
            None => return err_map("not a replicant"),
            Some(r) if r.is_breaking_down(now) => return err_map("already breaking down"),
            Some(_) => {}
        }
        let outcome = trigger_breakdown_rolled(ch, now);
        let _ = cdb.save_character_data(ch.clone());
        let mut m = rhai::Map::new();
        m.insert("success".into(), rhai::Dynamic::from(true));
        m.insert("kind".into(), rhai::Dynamic::from(outcome.kind.to_string()));
        m.insert("message".into(), rhai::Dynamic::from(outcome.message.to_string()));
        m.insert(
            "room_message".into(),
            rhai::Dynamic::from(outcome.room_message.to_string()),
        );
        m
    });

    // replicant_baseline_test(connection_id) -> Map
    //   { success, passed, roll, chance, strikes, retired, cooldown_remaining }
    // The full transaction: cooldown gate, chance roll, full restore on pass,
    // strike + lockout on fail, retirement at 3 strikes.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("replicant_baseline_test", move |connection_id: String| -> rhai::Map {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return err_map("bad connection id"),
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return err_map("no character"),
        };
        let now = now_secs();
        let (chance, cooldown_remaining) = match ch.replicant_state.as_ref() {
            None => return err_map("not a replicant"),
            Some(r) => (r.baseline_success_chance(), (r.baseline_cooldown_until - now).max(0)),
        };
        if cooldown_remaining > 0 {
            let mut m = err_map("cooldown");
            m.insert("cooldown_remaining".into(), rhai::Dynamic::from(cooldown_remaining));
            return m;
        }

        use rand::Rng;
        let roll = rand::thread_rng().gen_range(1..=100);
        let passed = roll <= chance;
        let mut retired = false;
        if let Some(r) = ch.replicant_state.as_mut() {
            if passed {
                r.resolve = r.max_resolve;
                r.baseline_strikes = (r.baseline_strikes - 1).max(0);
                r.breakdowns_since_baseline = 0;
            } else {
                r.baseline_strikes += 1;
                r.baseline_cooldown_until = now + BASELINE_FAIL_COOLDOWN_SECS;
                retired = r.baseline_strikes >= STRIKES_FOR_RETIREMENT;
            }
        }
        if retired {
            apply_retirement(ch);
        }
        let strikes = ch
            .replicant_state
            .as_ref()
            .map(|r| r.baseline_strikes as i64)
            .unwrap_or(0);
        let _ = cdb.save_character_data(ch.clone());

        let mut m = rhai::Map::new();
        m.insert("success".into(), rhai::Dynamic::from(true));
        m.insert("passed".into(), rhai::Dynamic::from(passed));
        m.insert("roll".into(), rhai::Dynamic::from(roll as i64));
        m.insert("chance".into(), rhai::Dynamic::from(chance as i64));
        m.insert("strikes".into(), rhai::Dynamic::from(strikes));
        m.insert("retired".into(), rhai::Dynamic::from(retired));
        m.insert("cooldown_remaining".into(), rhai::Dynamic::from(0_i64));
        m
    });

    // attune_pc_signature_item(connection_id, item_id, confirmed) -> Map
    //   { success, grief, needs_confirm, error }
    // Replacing a previous attunement costs grief resolve; `confirmed=false`
    // returns needs_confirm instead of applying it (the command warns first).
    // Grief can drop resolve to 0 — the CALLER checks and triggers breakdown.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "attune_pc_signature_item",
        move |connection_id: String, item_id: String, confirmed: bool| -> rhai::Map {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad item id"),
            };
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            let previous = match ch.replicant_state.as_ref() {
                None => return err_map("not a replicant"),
                Some(r) => r.signature_item_id,
            };
            if previous == Some(item_uuid) {
                return err_map("already attuned");
            }
            let grief = previous.is_some();
            if grief && !confirmed {
                let mut m = err_map("needs confirm");
                m.insert("needs_confirm".into(), rhai::Dynamic::from(true));
                return m;
            }
            if let Some(r) = ch.replicant_state.as_mut() {
                if grief {
                    r.change_resolve(-ATTUNE_GRIEF_RESOLVE_COST);
                }
                r.signature_item_id = Some(item_uuid);
                r.attuned_at = now_secs();
                r.last_focus_time = 0;
            }
            let _ = cdb.save_character_data(ch.clone());
            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("grief".into(), rhai::Dynamic::from(grief));
            m
        },
    );

    // replicant_focus(connection_id) -> Map
    //   { success, restored, resolve, error, cooldown_remaining }
    // Gates handled here: attuned, bonded (24h), item still carried,
    // cooldown. The SAFE-ZONE gate lives in focus.rhai (room context).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("replicant_focus", move |connection_id: String| -> rhai::Map {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return err_map("bad connection id"),
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return err_map("no character"),
        };
        let now = now_secs();
        let (item_id, bonded, cooldown_remaining, full) = match ch.replicant_state.as_ref() {
            None => return err_map("not a replicant"),
            Some(r) => (
                r.signature_item_id,
                r.is_signature_bonded(now),
                (r.last_focus_time + FOCUS_COOLDOWN_SECS - now).max(0),
                r.resolve >= r.max_resolve,
            ),
        };
        let item_id = match item_id {
            Some(i) => i,
            None => return err_map("no signature item"),
        };
        if !bonded {
            return err_map("not bonded");
        }
        if cooldown_remaining > 0 {
            let mut m = err_map("cooldown");
            m.insert("cooldown_remaining".into(), rhai::Dynamic::from(cooldown_remaining));
            return m;
        }
        if full {
            return err_map("resolve full");
        }
        // Item must still be on the character (inventory or equipped).
        let name = ch.name.clone();
        let carried = cdb
            .get_items_in_inventory(&name)
            .map(|items| items.iter().any(|i| i.id == item_id))
            .unwrap_or(false)
            || cdb
                .get_equipped_items(&name)
                .map(|items| items.iter().any(|i| i.id == item_id))
                .unwrap_or(false);
        if !carried {
            return err_map("item gone");
        }
        let new_resolve = ch
            .replicant_state
            .as_mut()
            .map(|r| {
                r.last_focus_time = now;
                r.change_resolve(FOCUS_RESOLVE_RESTORE) as i64
            })
            .unwrap_or(-1);
        let _ = cdb.save_character_data(ch.clone());
        let mut m = rhai::Map::new();
        m.insert("success".into(), rhai::Dynamic::from(true));
        m.insert("restored".into(), rhai::Dynamic::from(FOCUS_RESOLVE_RESTORE as i64));
        m.insert("resolve".into(), rhai::Dynamic::from(new_resolve));
        m
    });

    // replicant_comfort(target_name) -> Map { success, restored, resolve, error }
    // Same core the `comfort` social uses; exposed for quest/NPC scripting.
    // The recipient-side cooldown means N friends can't stack restores.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("replicant_comfort", move |target_name: String| -> rhai::Map {
        match crate::replicant::comfort_replicant_by_name(&cdb, &conns, &target_name) {
            crate::replicant::ComfortOutcome::Restored(restored, resolve, _max) => {
                let mut m = rhai::Map::new();
                m.insert("success".into(), rhai::Dynamic::from(true));
                m.insert("restored".into(), rhai::Dynamic::from(restored as i64));
                m.insert("resolve".into(), rhai::Dynamic::from(resolve as i64));
                m
            }
            crate::replicant::ComfortOutcome::TooRattled => err_map("too rattled"),
            crate::replicant::ComfortOutcome::Full => err_map("resolve full"),
            crate::replicant::ComfortOutcome::NotApplicable => err_map("not a replicant"),
        }
    });

    // is_signature_item_bonded(connection_id) -> bool (display helper)
    let conns = connections.clone();
    engine.register_fn("is_signature_item_bonded", move |connection_id: String| -> bool {
        let now = now_secs();
        read_replicant(&conns, &connection_id, |r| r.is_signature_bonded(now)).unwrap_or(false)
    });
}

/// Read helper: run `f` on the session's ReplicantState if present.
fn read_replicant<T>(
    conns: &SharedConnections,
    connection_id: &str,
    f: impl FnOnce(&ReplicantState) -> T,
) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock
        .get(&conn_id)
        .and_then(|s| s.character.as_ref())
        .and_then(|c| c.replicant_state.as_ref())
        .map(f)
}

/// Mutate helper: run `f` on (character, replicant_state) and save.
fn mutate_replicant<T>(
    conns: &SharedConnections,
    db: &Arc<Db>,
    connection_id: &str,
    f: impl FnOnce(&mut crate::types::CharacterData, &mut ReplicantState) -> T,
) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let mut conns_lock = conns.lock().ok()?;
    let ch = conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut())?;
    let mut state = ch.replicant_state.take()?;
    let out = f(ch, &mut state);
    ch.replicant_state = Some(state);
    let _ = db.save_character_data(ch.clone());
    Some(out)
}
