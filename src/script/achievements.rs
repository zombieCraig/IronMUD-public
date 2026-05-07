//! Achievement system: types, core unlock pipeline, Rhai bindings.
//!
//! Two flows live here:
//!
//! 1. **Engine notify path** — gameplay sites (combat tick, skill setter,
//!    learn command, gold setter, room entry) call `notify_counter_core` /
//!    `notify_event_core`. These bump per-character counters or evaluate
//!    event-shaped criteria, and on first threshold crossing call
//!    `award_core`.
//!
//! 2. **Manual path** — DG triggers and admin tools call `award_manual`
//!    (or the registered Rhai fn `award_achievement`) directly with a
//!    key whose criterion is `Manual`. Engine-criterion achievements
//!    reject manual awards (so builders cannot shortcut canonical
//!    milestones via DG).
//!
//! Both paths funnel through `award_core`, which is idempotent, grants
//! the title, optionally delivers item/gold, and persists the character.

use rhai::{Array, Dynamic, Engine, Map};
use std::sync::Arc;

use crate::db::Db;
use crate::types::{AchievementCriterion, AchievementDef, CharacterData};
use crate::{SharedConnections, SharedState};

/// Read the admin toggle. Defaults to enabled when unset or unparseable.
pub fn enabled(db: &Db) -> bool {
    match db.get_setting("achievements_enabled") {
        Ok(Some(v)) => !matches!(v.to_lowercase().as_str(), "false" | "0" | "off" | "no"),
        _ => true,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn send_to_player(connections: &SharedConnections, name: &str, message: &str) {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref ch) = session.character {
                if ch.name.eq_ignore_ascii_case(name) {
                    let _ = session.sender.send(format!("{}\n", message));
                    return;
                }
            }
        }
    }
}

fn sync_to_session(connections: &SharedConnections, ch: &CharacterData) {
    if let Ok(mut conns) = connections.lock() {
        for (_, session) in conns.iter_mut() {
            if let Some(ref existing) = session.character {
                if existing.name.eq_ignore_ascii_case(&ch.name) {
                    session.character = Some(ch.clone());
                    return;
                }
            }
        }
    }
}

/// Core unlock pipeline. Idempotent: returns false if the achievement is
/// already unlocked, missing, the system is disabled, or the player can't
/// be loaded.
///
/// `manual` is true when called from the DG verb / admin tools; manual
/// awards are rejected for non-`Manual` criteria so DG cannot shortcut
/// canonical engine-detected milestones. Engine-criterion awards (`manual
/// = false`) are rejected for `Manual` criteria so notify paths don't
/// accidentally auto-trigger builder achievements.
pub fn award_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    key: &str,
    manual: bool,
) -> bool {
    if !enabled(db) {
        return false;
    }

    let key_lc = key.to_lowercase();

    let def: AchievementDef = {
        let world = match state.lock() {
            Ok(w) => w,
            Err(_) => return false,
        };
        match world.achievement_definitions.get(&key_lc) {
            Some(d) => d.clone(),
            None => {
                tracing::warn!("achievements: award for unknown key '{}'", key_lc);
                return false;
            }
        }
    };

    let is_manual_def = matches!(def.criterion, AchievementCriterion::Manual);
    if manual && !is_manual_def {
        tracing::warn!(
            "achievements: manual award refused for engine-criterion key '{}'",
            key_lc
        );
        return false;
    }
    if !manual && is_manual_def {
        return false;
    }

    let mut ch = match db.get_character_data(&player_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return false,
    };

    if ch.achievements_unlocked.contains_key(&key_lc) {
        return false;
    }

    ch.achievements_unlocked.insert(
        key_lc.clone(),
        crate::types::AchievementUnlock { unlocked_at: now_secs() },
    );

    if ch.active_title.is_none() {
        ch.active_title = Some(key_lc.clone());
    }

    if let Some(gold) = def.reward.gold {
        ch.gold = ch.gold.saturating_add(gold);
        if ch.gold > ch.gold_high_water {
            ch.gold_high_water = ch.gold;
        }
    }

    if def.reward.item_vnum.is_some() {
        // Slice 3 wires item delivery (with escrow on overflow). For now
        // log so future JSON entries surface as audit signals rather
        // than silent drops.
        tracing::debug!(
            "Achievement '{}' has item_vnum reward; item delivery is wired in slice 3",
            key_lc
        );
    }

    if db.save_character_data(ch.clone()).is_err() {
        return false;
    }
    sync_to_session(connections, &ch);

    let banner = format!(
        "\x1b[1;33m*** Achievement unlocked: {} ***\x1b[0m\n  {}",
        def.name, def.description
    );
    send_to_player(connections, player_name, &banner);
    if let Some(gold) = def.reward.gold {
        if gold > 0 {
            send_to_player(connections, player_name, &format!("You receive {} gold.", gold));
        }
    }

    true
}

/// Bump a counter on the character and award any matching achievements.
/// Public so tick-side hooks can call without going through the engine.
pub fn notify_counter_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    counter_key: &str,
    increment: u32,
) -> u32 {
    if !enabled(db) || increment == 0 {
        return 0;
    }
    let key_lc = counter_key.to_lowercase();

    let new_value = match db.get_character_data(&player_name.to_lowercase()) {
        Ok(Some(mut ch)) => {
            let entry = ch.achievement_counters.entry(key_lc.clone()).or_insert(0);
            *entry = entry.saturating_add(increment);
            let v = *entry;
            if db.save_character_data(ch).is_err() {
                return 0;
            }
            v
        }
        _ => return 0,
    };

    let candidates: Vec<(String, u32)> = match state.lock() {
        Ok(world) => world
            .achievement_index_by_counter
            .get(&key_lc)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|k| {
                world.achievement_definitions.get(&k).and_then(|d| {
                    if let AchievementCriterion::Counter { threshold, .. } = d.criterion {
                        Some((k, threshold))
                    } else {
                        None
                    }
                })
            })
            .collect(),
        Err(_) => return new_value,
    };

    for (key, threshold) in candidates {
        if new_value >= threshold {
            award_core(db, connections, state, player_name, &key, false);
        }
    }
    new_value
}

/// Notify an event-shaped criterion. `event_kind` selects the
/// `AchievementCriterion` variant: `skill_reached` (`arg = "<skill>:<level>"`),
/// `gold_high_water` (`arg = "<amount>"`), `recipe_learned` (`arg = key`),
/// `lease_bought` (`arg = area_vnum or ""`).
pub fn notify_event_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    event_kind: &str,
    event_arg: &str,
) {
    if !enabled(db) {
        return;
    }

    let candidate_keys: Vec<String> = match state.lock() {
        Ok(world) => world
            .achievement_definitions
            .iter()
            .filter_map(|(k, d)| match (&d.criterion, event_kind) {
                (AchievementCriterion::SkillReached { skill, level }, "skill_reached") => {
                    let mut parts = event_arg.splitn(2, ':');
                    let arg_skill = parts.next().unwrap_or("");
                    let arg_level: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    if arg_skill.eq_ignore_ascii_case(skill) && arg_level >= *level {
                        Some(k.clone())
                    } else {
                        None
                    }
                }
                (AchievementCriterion::LearnedRecipe { recipe_key }, "recipe_learned") => {
                    if event_arg.eq_ignore_ascii_case(recipe_key) {
                        Some(k.clone())
                    } else {
                        None
                    }
                }
                (AchievementCriterion::OwnedLease { area_vnum }, "lease_bought") => match area_vnum {
                    Some(v) if v.eq_ignore_ascii_case(event_arg) => Some(k.clone()),
                    None => Some(k.clone()),
                    _ => None,
                },
                (AchievementCriterion::GoldHeld { amount }, "gold_high_water") => {
                    let v: i32 = event_arg.parse().unwrap_or(0);
                    if v >= *amount { Some(k.clone()) } else { None }
                }
                _ => None,
            })
            .collect(),
        Err(_) => return,
    };

    for key in candidate_keys {
        award_core(db, connections, state, player_name, &key, false);
    }
}

/// Bump kill counters and evaluate matching achievements. Convenience
/// wrapper over `notify_counter_core` for the combat-tick hook.
pub fn notify_kill_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    killer_name: &str,
    mob_vnum: &str,
) {
    notify_counter_core(db, connections, state, killer_name, "kills.any", 1);
    notify_counter_core(
        db,
        connections,
        state,
        killer_name,
        &format!("kills.{}", mob_vnum.to_lowercase()),
        1,
    );
}

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // notify_achievement_counter(player_name, counter_key, increment) -> i64 (new value)
    {
        let db = db.clone();
        let connections = connections.clone();
        let state = state.clone();
        engine.register_fn(
            "notify_achievement_counter",
            move |player_name: String, counter_key: String, increment: i64| -> i64 {
                let inc = increment.max(0) as u32;
                notify_counter_core(&db, &connections, &state, &player_name, &counter_key, inc) as i64
            },
        );
    }

    // notify_achievement_event(player_name, event_kind, event_arg)
    {
        let db = db.clone();
        let connections = connections.clone();
        let state = state.clone();
        engine.register_fn(
            "notify_achievement_event",
            move |player_name: String, event_kind: String, event_arg: String| {
                notify_event_core(&db, &connections, &state, &player_name, &event_kind, &event_arg);
            },
        );
    }

    // award_achievement(player_name, key) -> bool (Manual criteria only)
    {
        let db = db.clone();
        let connections = connections.clone();
        let state = state.clone();
        engine.register_fn(
            "award_achievement",
            move |player_name: String, key: String| -> bool {
                award_core(&db, &connections, &state, &player_name, &key, true)
            },
        );
    }

    // get_achievement_def(key) -> Map | ()
    {
        let state = state.clone();
        engine.register_fn("get_achievement_def", move |key: String| -> Dynamic {
            let world = match state.lock() {
                Ok(w) => w,
                Err(_) => return Dynamic::UNIT,
            };
            match world.achievement_definitions.get(&key.to_lowercase()) {
                Some(def) => Dynamic::from(achievement_to_map(def)),
                None => Dynamic::UNIT,
            }
        });
    }

    // list_achievements(player_name) -> Array of Map
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("list_achievements", move |player_name: String| -> Array {
            let ch = match db.get_character_data(&player_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return Array::new(),
            };
            let world = match state.lock() {
                Ok(w) => w,
                Err(_) => return Array::new(),
            };

            let mut out: Vec<Map> = Vec::new();
            for (key, def) in &world.achievement_definitions {
                let unlock = ch.achievements_unlocked.get(key);
                if def.hidden && unlock.is_none() {
                    continue;
                }
                let mut m = Map::new();
                m.insert("key".into(), Dynamic::from(def.key.clone()));
                m.insert("name".into(), Dynamic::from(def.name.clone()));
                m.insert("description".into(), Dynamic::from(def.description.clone()));
                m.insert(
                    "category".into(),
                    Dynamic::from(format!("{:?}", def.category).to_lowercase()),
                );
                m.insert("hidden".into(), Dynamic::from(def.hidden));
                m.insert("unlocked".into(), Dynamic::from(unlock.is_some()));
                m.insert(
                    "unlocked_at".into(),
                    Dynamic::from(unlock.map(|u| u.unlocked_at).unwrap_or(0)),
                );
                m.insert("title".into(), Dynamic::from(def.reward.title.clone()));
                m.insert(
                    "active".into(),
                    Dynamic::from(ch.active_title.as_deref() == Some(key.as_str())),
                );
                out.push(m);
            }
            out.sort_by(|a, b| {
                let ak = a.get("key").and_then(|d| d.clone().into_string().ok()).unwrap_or_default();
                let bk = b.get("key").and_then(|d| d.clone().into_string().ok()).unwrap_or_default();
                ak.cmp(&bk)
            });
            out.into_iter().map(Dynamic::from).collect()
        });
    }

    // get_active_title(player_name) -> String (display text; empty when none)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("get_active_title", move |player_name: String| -> String {
            let ch = match db.get_character_data(&player_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return String::new(),
            };
            let key = match ch.active_title {
                Some(k) => k,
                None => return String::new(),
            };
            let world = match state.lock() {
                Ok(w) => w,
                Err(_) => return String::new(),
            };
            world
                .achievement_definitions
                .get(&key)
                .map(|d| d.reward.title.clone())
                .unwrap_or_default()
        });
    }

    // set_active_title(player_name, key_or_empty) -> bool
    {
        let db = db.clone();
        let connections = connections.clone();
        engine.register_fn(
            "set_active_title",
            move |player_name: String, key: String| -> bool {
                let mut ch = match db.get_character_data(&player_name.to_lowercase()) {
                    Ok(Some(c)) => c,
                    _ => return false,
                };
                if key.is_empty() {
                    ch.active_title = None;
                    let ok = db.save_character_data(ch.clone()).is_ok();
                    if ok {
                        sync_to_session(&connections, &ch);
                    }
                    return ok;
                }
                let key_lc = key.to_lowercase();
                if !ch.achievements_unlocked.contains_key(&key_lc) {
                    return false;
                }
                ch.active_title = Some(key_lc);
                let ok = db.save_character_data(ch.clone()).is_ok();
                if ok {
                    sync_to_session(&connections, &ch);
                }
                ok
            },
        );
    }

    // achievements_enabled() -> bool
    {
        let db = db.clone();
        engine.register_fn("achievements_enabled", move || -> bool { enabled(&db) });
    }
}

fn achievement_to_map(def: &AchievementDef) -> Map {
    let mut m = Map::new();
    m.insert("key".into(), Dynamic::from(def.key.clone()));
    m.insert("name".into(), Dynamic::from(def.name.clone()));
    m.insert("description".into(), Dynamic::from(def.description.clone()));
    m.insert(
        "category".into(),
        Dynamic::from(format!("{:?}", def.category).to_lowercase()),
    );
    m.insert("hidden".into(), Dynamic::from(def.hidden));
    m.insert("title".into(), Dynamic::from(def.reward.title.clone()));
    m
}
