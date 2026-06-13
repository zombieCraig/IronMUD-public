//! Rhai bindings for werewolf state on player characters.
//!
//! PC-only surface (mobs don't carry `WerewolfState`). Connection-id keyed,
//! mirroring `src/script/replicant.rs`. Every mutator follows the
//! session-authoritative pattern: lock connections, mutate
//! `session.character`, then `db.save_character_data`.
//!
//! The shift and rage-build transactions live here so the command scripts
//! stay thin renderers of the returned maps. Shared frenzy/form math lives
//! in `crate::werewolf` because the combat and rage ticks need it too.
//!
//! The First Change happens here in `awaken_pc` so creation, quest, and
//! admin paths share one definition (the embrace_pc pattern). Tribes are
//! granted traits (`tribe_*`) validated against
//! `scripts/data/werewolf_tribes.json`.

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::types::{
    CharacterData, FORM_HOMID, RAGE_BUILD_COOLDOWN_SECS, RAGE_BUILD_GAIN, SHIFT_FRENZY_THRESHOLD, SkillProgress,
    WerewolfState,
};
use crate::werewolf::{apply_form_buffs, is_known_form, maybe_rage_frenzy_rolled, shift_cost};
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

/// Returns the tribe id (e.g. "get_of_fenris") if any `tribe_*` trait is
/// present, else None. Mirrors `pc_clan_from_traits`.
pub fn pc_tribe_from_traits(ch: &CharacterData) -> Option<String> {
    ch.traits
        .iter()
        .find_map(|t| t.strip_prefix("tribe_").map(str::to_string))
}

/// Enumerate tribe ids known to `scripts/data/werewolf_tribes.json`. Skips
/// underscore-prefixed metadata keys. Returns the canonical three if the
/// file is missing or unparseable.
pub fn list_tribe_ids() -> Vec<String> {
    if let Ok(raw) = std::fs::read_to_string("scripts/data/werewolf_tribes.json") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(obj) = parsed.as_object() {
                let ids: Vec<String> = obj.keys().filter(|k| !k.starts_with('_')).cloned().collect();
                if !ids.is_empty() {
                    return ids;
                }
            }
        }
    }
    vec![
        "get_of_fenris".to_string(),
        "children_of_gaia".to_string(),
        "silent_striders".to_string(),
    ]
}

/// Apply tribe-acknowledgment side-effects to an already-awakened Garou.
/// Idempotent: trait pushed only if missing. Returns true when anything
/// changed (caller saves). Mirrors `apply_clan_acknowledgment`.
pub fn apply_tribe_acknowledgment(ch: &mut CharacterData, tribe: &str) -> bool {
    if ch.werewolf_state.is_none() {
        return false;
    }
    let tribe_trim = tribe.trim().to_lowercase();
    if tribe_trim.is_empty() || !list_tribe_ids().contains(&tribe_trim) {
        return false;
    }
    let trait_id = format!("tribe_{}", tribe_trim);
    if ch.traits.iter().any(|t| t == &trait_id) {
        return false;
    }
    ch.traits.push(trait_id);
    true
}

/// Summed `frenzy_dc_modifier` for the character's traits, read from the
/// global trait map. The map is fetched from World BEFORE the connections
/// lock is taken (deadlock rule).
fn frenzy_dc_map(state: &SharedState) -> std::collections::HashMap<String, i32> {
    let world = match state.lock() {
        Ok(w) => w,
        Err(_) => return Default::default(),
    };
    world
        .trait_definitions
        .iter()
        .filter_map(|(name, def)| def.effects.get("frenzy_dc_modifier").map(|m| (name.clone(), *m)))
        .collect()
}

fn dc_for(ch: &CharacterData, map: &std::collections::HashMap<String, i32>) -> i32 {
    ch.traits.iter().filter_map(|t| map.get(t)).sum()
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // is_pc_werewolf(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_werewolf", move |connection_id: String| -> bool {
        read_pc(&conns, &connection_id, |c| c.werewolf_state.is_some()).unwrap_or(false)
    });

    // awaken_pc(connection_id, tribe_id) -> bool
    // The First Change. Stamps a fresh WerewolfState if absent (idempotent),
    // seeds the primal_urge skill, and applies the tribe acknowledgment when
    // a tribe is supplied. Empty tribe = tribeless cub (the thinblood
    // analog) who can claim a tribe later via claim_tribe_for_pc.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("awaken_pc", move |connection_id: String, tribe: String| -> bool {
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
        let mut changed = false;
        if ch.werewolf_state.is_none() {
            ch.werewolf_state = Some(WerewolfState::newly_awakened(now_secs()));
            changed = true;
        }
        let entry = ch
            .skills
            .entry("primal_urge".to_string())
            .or_insert(SkillProgress::default());
        if entry.level < 1 {
            entry.level = 1;
            changed = true;
        }
        if !tribe.trim().is_empty() {
            if pc_tribe_from_traits(ch).is_some() {
                // Already tribe-acknowledged — refuse to overwrite.
                return false;
            }
            if apply_tribe_acknowledgment(ch, &tribe) {
                changed = true;
            }
        }
        if !changed {
            return false;
        }
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // claim_tribe_for_pc(connection_id, tribe) -> bool
    // Quest-reward path for a tribeless cub (mirrors claim_clan_for_pc).
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn(
        "claim_tribe_for_pc",
        move |connection_id: String, tribe: String| -> bool {
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
            if ch.werewolf_state.is_none() || pc_tribe_from_traits(ch).is_some() {
                return false;
            }
            if !apply_tribe_acknowledgment(ch, &tribe) {
                return false;
            }
            cdb.save_character_data(ch.clone()).is_ok()
        },
    );

    // revoke_pc_lycanthropy(connection_id) -> bool (admin/testing symmetry).
    // Clears state, form buffs, and any tribe trait.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("revoke_pc_lycanthropy", move |connection_id: String| -> bool {
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
        if ch.werewolf_state.is_none() {
            return false;
        }
        ch.werewolf_state = None;
        ch.active_buffs
            .retain(|b| b.source != crate::types::WEREWOLF_FORM_SOURCE);
        ch.traits.retain(|t| !t.starts_with("tribe_"));
        cdb.save_character_data(ch.clone()).is_ok()
    });

    // get_pc_rage(connection_id) -> i64. -1 for non-werewolves.
    let conns = connections.clone();
    engine.register_fn("get_pc_rage", move |connection_id: String| -> i64 {
        read_werewolf(&conns, &connection_id, |w| w.rage as i64).unwrap_or(-1)
    });

    // get_pc_max_rage(connection_id) -> i64. 0 for non-werewolves.
    let conns = connections.clone();
    engine.register_fn("get_pc_max_rage", move |connection_id: String| -> i64 {
        read_werewolf(&conns, &connection_id, |w| w.max_rage as i64).unwrap_or(0)
    });

    // change_pc_rage(connection_id, delta) -> i64
    // Clamped to [0, max_rage]; returns the new value (-1 if not a
    // werewolf). Does NOT roll frenzy — admin/quest tuning knob.
    let conns = connections.clone();
    let cdb = db.clone();
    engine.register_fn("change_pc_rage", move |connection_id: String, delta: i64| -> i64 {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return -1,
        };
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return -1,
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return -1,
        };
        let Some(w) = ch.werewolf_state.as_mut() else {
            return -1;
        };
        let new_val = w.set_rage(w.rage.saturating_add(delta as i32)) as i64;
        let _ = cdb.save_character_data(ch.clone());
        new_val
    });

    // get_pc_form(connection_id) -> String ("" for non-werewolves)
    let conns = connections.clone();
    engine.register_fn("get_pc_form", move |connection_id: String| -> String {
        read_werewolf(&conns, &connection_id, |w| w.current_form.clone()).unwrap_or_default()
    });

    // get_pc_tribe(connection_id) -> String ("" when tribeless / not Garou)
    let conns = connections.clone();
    engine.register_fn("get_pc_tribe", move |connection_id: String| -> String {
        read_pc(&conns, &connection_id, |c| pc_tribe_from_traits(c).unwrap_or_default()).unwrap_or_default()
    });

    // is_pc_werewolf_frenzying(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_pc_werewolf_frenzying", move |connection_id: String| -> bool {
        let now = now_secs();
        read_werewolf(&conns, &connection_id, |w| w.is_frenzying(now)).unwrap_or(false)
    });

    // list_werewolf_tribes() -> Array of tribe id strings
    engine.register_fn("list_werewolf_tribes", move || -> rhai::Array {
        list_tribe_ids().into_iter().map(rhai::Dynamic::from).collect()
    });

    // werewolf_shift(connection_id, form) -> Map
    //   { success, form, cost, rage, frenzied } |
    //   { success: false, error }
    // The whole transaction: form validation, frenzy lock, rage cost, buff
    // swap, and the forced frenzy roll when shifting at high rage.
    let conns = connections.clone();
    let cdb = db.clone();
    let cstate = state.clone();
    engine.register_fn(
        "werewolf_shift",
        move |connection_id: String, form: String| -> rhai::Map {
            let form = form.trim().to_lowercase();
            if !is_known_form(&form) {
                return err_map("unknown form");
            }
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(u) => u,
                Err(_) => return err_map("bad connection id"),
            };
            // Trait map read BEFORE the connections lock (deadlock rule).
            let dc_map = frenzy_dc_map(&cstate);
            let mut conns_lock = match conns.lock() {
                Ok(g) => g,
                Err(_) => return err_map("lock"),
            };
            let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
                Some(c) => c,
                None => return err_map("no character"),
            };
            let now = now_secs();
            let dc = dc_for(ch, &dc_map);
            let Some(mut w) = ch.werewolf_state.take() else {
                return err_map("not a werewolf");
            };
            if w.is_frenzying(now) {
                ch.werewolf_state = Some(w);
                return err_map("frenzying");
            }
            if w.current_form == form {
                ch.werewolf_state = Some(w);
                return err_map("already in form");
            }
            let cost = shift_cost(&form);
            if w.rage < cost {
                ch.werewolf_state = Some(w);
                let mut m = err_map("not enough rage");
                m.insert("cost".into(), rhai::Dynamic::from(cost as i64));
                return m;
            }
            w.set_rage(w.rage - cost);
            w.current_form = form.clone();
            apply_form_buffs(&mut ch.active_buffs, &form);

            // The change itself stirs the wolf: shifting while rage runs this
            // hot forces a frenzy roll on the spot.
            let mut frenzied = false;
            if form != FORM_HOMID && w.rage >= SHIFT_FRENZY_THRESHOLD {
                frenzied = maybe_rage_frenzy_rolled(&mut w, &mut ch.active_buffs, dc, now);
            }
            let rage = w.rage;
            ch.werewolf_state = Some(w);
            let _ = cdb.save_character_data(ch.clone());

            let mut m = rhai::Map::new();
            m.insert("success".into(), rhai::Dynamic::from(true));
            m.insert("form".into(), rhai::Dynamic::from(form));
            m.insert("cost".into(), rhai::Dynamic::from(cost as i64));
            m.insert("rage".into(), rhai::Dynamic::from(rage as i64));
            m.insert("frenzied".into(), rhai::Dynamic::from(frenzied));
            m
        },
    );

    // werewolf_build_rage(connection_id) -> Map
    //   { success, rage, frenzied } |
    //   { success: false, error, cooldown_remaining? }
    // The deliberate `rage` build action. The COMBAT gate lives in
    // rage.rhai (it needs room context); cooldown and the overflow frenzy
    // roll live here.
    let conns = connections.clone();
    let cdb = db.clone();
    let cstate = state.clone();
    engine.register_fn("werewolf_build_rage", move |connection_id: String| -> rhai::Map {
        let conn_id = match uuid::Uuid::parse_str(&connection_id) {
            Ok(u) => u,
            Err(_) => return err_map("bad connection id"),
        };
        let dc_map = frenzy_dc_map(&cstate);
        let mut conns_lock = match conns.lock() {
            Ok(g) => g,
            Err(_) => return err_map("lock"),
        };
        let ch = match conns_lock.get_mut(&conn_id).and_then(|s| s.character.as_mut()) {
            Some(c) => c,
            None => return err_map("no character"),
        };
        let now = now_secs();
        let dc = dc_for(ch, &dc_map);
        let Some(mut w) = ch.werewolf_state.take() else {
            return err_map("not a werewolf");
        };
        let cooldown_left = w.last_rage_build + RAGE_BUILD_COOLDOWN_SECS - now;
        if cooldown_left > 0 {
            ch.werewolf_state = Some(w);
            let mut m = err_map("cooldown");
            m.insert("cooldown_remaining".into(), rhai::Dynamic::from(cooldown_left));
            return m;
        }
        w.last_rage_build = now;
        let hit_max = w.gain_rage(RAGE_BUILD_GAIN);
        let mut frenzied = false;
        if hit_max {
            frenzied = maybe_rage_frenzy_rolled(&mut w, &mut ch.active_buffs, dc, now);
        }
        let rage = w.rage;
        ch.werewolf_state = Some(w);
        let _ = cdb.save_character_data(ch.clone());

        let mut m = rhai::Map::new();
        m.insert("success".into(), rhai::Dynamic::from(true));
        m.insert("rage".into(), rhai::Dynamic::from(rage as i64));
        m.insert("frenzied".into(), rhai::Dynamic::from(frenzied));
        m
    });
}

/// Read helper: run `f` on the session's character if present.
fn read_pc<T>(conns: &SharedConnections, connection_id: &str, f: impl FnOnce(&CharacterData) -> T) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock.get(&conn_id).and_then(|s| s.character.as_ref()).map(f)
}

/// Read helper: run `f` on the session's WerewolfState if present.
fn read_werewolf<T>(conns: &SharedConnections, connection_id: &str, f: impl FnOnce(&WerewolfState) -> T) -> Option<T> {
    let conn_id = uuid::Uuid::parse_str(connection_id).ok()?;
    let conns_lock = conns.lock().ok()?;
    conns_lock
        .get(&conn_id)
        .and_then(|s| s.character.as_ref())
        .and_then(|c| c.werewolf_state.as_ref())
        .map(f)
}
