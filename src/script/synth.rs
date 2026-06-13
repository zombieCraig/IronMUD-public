//! Rhai bindings for synth chassis state on player characters.
//!
//! PC-only surface (mobs don't carry `SynthState`). Connection-id keyed,
//! mirroring `src/script/replicant.rs`. Every mutator follows the
//! session-authoritative pattern: lock connections, mutate
//! `session.character`, then `db.save_character_data`.
//!
//! Repair is transactional here (cooldown, critical kit requirement) so
//! `repair.rhai` and technician NPC scripts stay thin renderers of the
//! returned maps. The chassis tick and down-transition logic live in
//! `crate::synth` because the combat/bleeding/drowning ticks need them too.

use crate::SharedConnections;
use crate::db::Db;
use crate::synth::{RepairOutcome, apply_kit_repair, apply_technician_repair, synth_scaled_heal};
use crate::types::{SYNTH_STAGE_CRITICAL, SynthState};
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

fn repair_map(outcome: RepairOutcome) -> rhai::Map {
    match outcome {
        RepairOutcome::Repaired(healed, hp, stage) => {
            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("healed".into(), rhai::Dynamic::from(healed as i64));
            m.insert("hp".into(), rhai::Dynamic::from(hp as i64));
            m.insert("stage".into(), rhai::Dynamic::from(stage as i64));
            m.insert(
                "stage_label".into(),
                rhai::Dynamic::from(SynthState::stage_label(stage).to_string()),
            );
            m
        }
        RepairOutcome::OnCooldown(secs) => {
            let mut m = err_map("cooldown");
            m.insert("cooldown_remaining".into(), rhai::Dynamic::from(secs));
            m
        }
        RepairOutcome::NeedsMoreKits(kits) => {
            let mut m = err_map("needs more kits");
            m.insert("kits_needed".into(), rhai::Dynamic::from(kits as i64));
            m
        }
        RepairOutcome::Full => err_map("already nominal"),
        RepairOutcome::NotApplicable => err_map("not a synth"),
    }
}

pub fn register_synth_functions(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // is_pc_synth(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_synth", move |connection_id: String| -> bool {
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
            .map(|c| c.synth_state.is_some())
            .unwrap_or(false)
    });

    // is_character_synth(name) -> bool — heal/treat target checks by name.
    // Online sessions first (authoritative), DB fallback for offline.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("is_character_synth", move |name: String| -> bool {
        let lower = name.to_lowercase();
        if let Ok(conns_lock) = conns.lock() {
            if let Some(c) = conns_lock
                .values()
                .filter_map(|s| s.character.as_ref())
                .find(|c| c.name.to_lowercase() == lower)
            {
                return c.synth_state.is_some();
            }
        }
        cdb.get_character_data(&name)
            .ok()
            .flatten()
            .map(|c| c.synth_state.is_some())
            .unwrap_or(false)
    });

    // init_pc_synth(connection_id) -> bool
    // Stamps a fresh SynthState if absent (idempotent). Shared by character
    // creation, login migration, and admin tooling.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("init_pc_synth", move |connection_id: String| -> bool {
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
        if ch.synth_state.is_none() {
            ch.synth_state = Some(SynthState::newly_activated(now_secs()));
        }
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // revoke_pc_synth(connection_id) -> bool (admin/testing symmetry)
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("revoke_pc_synth", move |connection_id: String| -> bool {
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
        if ch.synth_state.is_none() {
            return false;
        }
        ch.synth_state = None;
        ch.active_buffs
            .retain(|b| b.source != crate::types::SYNTH_MALFUNCTION_SOURCE);
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // get_pc_chassis_stage(connection_id) -> i64 (-1 for non-synths)
    let conns = connections.clone();
    engine.register_fn("get_pc_chassis_stage", move |connection_id: String| -> i64 {
        read_synth(&conns, &connection_id, |s| s.malfunction_stage as i64).unwrap_or(-1)
    });

    // get_pc_chassis_label(connection_id) -> String ("" for non-synths)
    let conns = connections.clone();
    engine.register_fn("get_pc_chassis_label", move |connection_id: String| -> String {
        read_synth(&conns, &connection_id, |s| {
            SynthState::stage_label(s.malfunction_stage).to_string()
        })
        .unwrap_or_default()
    });

    // get_pc_shutdown_remaining(connection_id) -> i64
    // Seconds of emergency reserve left; -1 when no countdown is running.
    let conns = connections.clone();
    engine.register_fn("get_pc_shutdown_remaining", move |connection_id: String| -> i64 {
        let now = now_secs();
        read_synth(&conns, &connection_id, |s| s.shutdown_remaining(now).unwrap_or(-1)).unwrap_or(-1)
    });

    // is_pc_chassis_critical(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_chassis_critical", move |connection_id: String| -> bool {
        read_synth(&conns, &connection_id, |s| s.is_critical()).unwrap_or(false)
    });

    // scale_heal_for_synth(amount) -> i64 — the 25%-effect rule, centralized.
    engine.register_fn("scale_heal_for_synth", move |amount: i64| -> i64 {
        synth_scaled_heal(amount as i32) as i64
    });

    // synth_kit_repair(connection_id, kits) -> Map
    //   { success, healed, hp, stage, stage_label } |
    //   { success: false, error, cooldown_remaining?, kits_needed? }
    // The CALLER validates and consumes the kits from inventory first.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "synth_kit_repair",
        move |connection_id: String, kits: i64| -> rhai::Map {
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
            let outcome = apply_kit_repair(ch, now_secs(), kits as i32);
            if matches!(outcome, RepairOutcome::Repaired(..)) {
                let _ = cdb.save_character_data(ch.clone());
            }
            repair_map(outcome)
        },
    );

    // synth_technician_repair(target_name) -> Map (same shape as kit repair)
    // Name-keyed so technician NPC dialogue / DG triggers can repair a synth
    // in the room. Payment is handled by the calling script.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("synth_technician_repair", move |target_name: String| -> rhai::Map {
        let lower = target_name.to_lowercase();
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock
            .values_mut()
            .filter_map(|s| s.character.as_mut())
            .find(|c| c.name.to_lowercase() == lower)
        {
            Some(c) => c,
            None => return err_map("not online"),
        };
        let outcome = apply_technician_repair(ch);
        if matches!(outcome, RepairOutcome::Repaired(..)) {
            let _ = cdb.save_character_data(ch.clone());
        }
        repair_map(outcome)
    });

    // synth_chassis_critical_stage() -> i64 (constant for scripts)
    engine.register_fn("synth_chassis_critical_stage", move || -> i64 {
        SYNTH_STAGE_CRITICAL as i64
    });

    // synth_directive_check(connection_id, mobile_id) -> Map { allowed, reason }
    // Behavioral inhibitor gate for attack initiation. Always allowed for
    // non-synths. Blocks initiating on a mortal that is neither in combat
    // nor recently fled one (the directive_allows_attack rule).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "synth_directive_check",
        move |connection_id: String, mobile_id: String| -> rhai::Map {
            let mut allowed = rhai::Map::new();
            allowed.insert("allowed".into(), rhai::Dynamic::from(true));
            allowed.insert("reason".into(), rhai::Dynamic::from(String::new()));

            // Non-synths (or unparseable ids) are never inhibited.
            let is_synth = uuid::Uuid::parse_str(&connection_id)
                .ok()
                .and_then(|conn_id| {
                    conns.lock().ok().map(|g| {
                        g.get(&conn_id)
                            .and_then(|s| s.character.as_ref())
                            .is_some_and(|c| c.synth_state.is_some())
                    })
                })
                .unwrap_or(false);
            if !is_synth {
                return allowed;
            }

            let mobile = match uuid::Uuid::parse_str(&mobile_id)
                .ok()
                .and_then(|id| cdb.get_mobile_data(&id).ok().flatten())
            {
                Some(m) => m,
                None => return allowed,
            };
            let now = now_secs();
            if crate::synth::directive_allows_attack(
                mobile.creature_type,
                mobile.combat.in_combat,
                mobile.last_combat_at,
                now,
            ) {
                return allowed;
            }
            let mut m = rhai::Map::new();
            m.insert("allowed".into(), rhai::Dynamic::from(false));
            m.insert(
                "reason".into(),
                rhai::Dynamic::from(format!(
                    "Your behavioral inhibitor refuses the target: {} is a non-combatant.",
                    mobile.name
                )),
            );
            m
        },
    );
}

/// Read helper: run `f` on the session's SynthState if present.
fn read_synth<T>(conns: &SharedConnections, connection_id: &str, f: impl FnOnce(&SynthState) -> T) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock
        .get(&conn_id)
        .and_then(|s| s.character.as_ref())
        .and_then(|c| c.synth_state.as_ref())
        .map(f)
}
