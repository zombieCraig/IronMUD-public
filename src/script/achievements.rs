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
use crate::types::{
    AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource, CharacterData,
};
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

    unlock_with_def(db, connections, &def, player_name)
}

/// DG-callable manual award path. Looks the definition up directly from
/// the sled `achievements` tree (so we don't need `SharedState`, which
/// `EvalCtx` doesn't carry), enforces the `Manual` criterion gate, and
/// funnels through the same [`unlock_with_def`] pipeline as `award_core`.
/// Returns `true` on first-time unlock; `false` on disabled system, missing
/// def, non-manual criterion, already-unlocked, or unloadable character.
pub fn award_manual_via_db(db: &Db, connections: &SharedConnections, player_name: &str, key: &str) -> bool {
    if !enabled(db) {
        return false;
    }
    let key_lc = key.to_lowercase();
    let def = match db.get_achievement(&key_lc) {
        Ok(Some(d)) => d,
        _ => {
            tracing::warn!("achievements: DG award for unknown key '{}'", key_lc);
            return false;
        }
    };
    if !matches!(def.criterion, AchievementCriterion::Manual) {
        tracing::warn!(
            "achievements: DG award refused for engine-criterion key '{}'",
            key_lc
        );
        return false;
    }
    unlock_with_def(db, connections, &def, player_name)
}

/// Shared unlock pipeline: dedup, insert, title default, gold/item reward,
/// persist, session sync, banner. Caller is responsible for criterion
/// gating (manual-vs-engine) before reaching this point.
fn unlock_with_def(db: &Db, connections: &SharedConnections, def: &AchievementDef, player_name: &str) -> bool {
    let key_lc = def.key.to_lowercase();

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
                let mut m = achievement_to_map(def);
                m.insert("unlocked".into(), Dynamic::from(unlock.is_some()));
                m.insert(
                    "unlocked_at".into(),
                    Dynamic::from(unlock.map(|u| u.unlocked_at).unwrap_or(0)),
                );
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

    // list_achievement_defs() -> Array of Map
    {
        let state = state.clone();
        engine.register_fn("list_achievement_defs", move || -> Array {
            let world = match state.lock() {
                Ok(w) => w,
                Err(_) => return Array::new(),
            };
            let mut out: Vec<Map> = Vec::new();
            for def in world.achievement_definitions.values() {
                out.push(achievement_to_map(def));
            }
            out.sort_by(|a, b| {
                let ak = a.get("key").and_then(|d| d.clone().into_string().ok()).unwrap_or_default();
                let bk = b.get("key").and_then(|d| d.clone().into_string().ok()).unwrap_or_default();
                ak.cmp(&bk)
            });
            out.into_iter().map(Dynamic::from).collect()
        });
    }

    // === Builder Functions ===

    // create_achievement(key, name, author) -> String (empty on success)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "create_achievement",
            move |key: String, name: String, author: String| -> String {
                let key_lc = key.to_lowercase();
                if key_lc.is_empty() {
                    return "key required".into();
                }
                if name.is_empty() {
                    return "name required".into();
                }
                if let Ok(Some(_)) = db.get_achievement(&key_lc) {
                    return format!("achievement '{}' already exists", key_lc);
                }
                // Default hidden=true so an in-progress achievement doesn't
                // spoiler-leak through `achievements` / list_achievements until
                // the builder explicitly flips it visible.
                let def = AchievementDef {
                    key: key_lc,
                    name,
                    description: String::new(),
                    category: AchievementCategory::Builder,
                    criterion: AchievementCriterion::Manual,
                    reward: AchievementReward::default(),
                    hidden: true,
                    source: AchievementSource::Db { author },
                };
                match db.save_achievement(def.clone()) {
                    Ok(_) => {
                        sync_world_after_save(&state, def);
                        String::new()
                    }
                    Err(e) => format!("db error: {}", e),
                }
            },
        );
    }

    // delete_achievement(key) -> bool
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("delete_achievement", move |key: String| -> bool {
            let key_lc = key.to_lowercase();
            let ok = db.delete_achievement(&key_lc).unwrap_or(false);
            if ok {
                sync_world_after_delete(&state, &key_lc);
            }
            ok
        });
    }

    // set_achievement_name(key, name) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_name", move |key: String, name: String| -> String {
            update_def(&db, &state, &key, |d| d.name = name.clone())
        });
    }

    // set_achievement_description(key, desc) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_description", move |key: String, desc: String| -> String {
            update_def(&db, &state, &key, |d| d.description = desc.clone())
        });
    }

    // set_achievement_category(key, category) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_category", move |key: String, cat: String| -> String {
            let category = match cat.to_lowercase().as_str() {
                "skill" => AchievementCategory::Skill,
                "combat" => AchievementCategory::Combat,
                "crafting" => AchievementCategory::Crafting,
                "exploration" => AchievementCategory::Exploration,
                "social" => AchievementCategory::Social,
                "wealth" => AchievementCategory::Wealth,
                "builder" => AchievementCategory::Builder,
                _ => return format!("unknown category '{}'", cat),
            };
            update_def(&db, &state, &key, |d| d.category = category)
        });
    }

    // set_achievement_hidden(key, hidden) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_hidden", move |key: String, hidden: bool| -> String {
            update_def(&db, &state, &key, |d| d.hidden = hidden)
        });
    }

    // set_achievement_reward_title(key, title) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_reward_title", move |key: String, title: String| -> String {
            update_def(&db, &state, &key, |d| d.reward.title = title.clone())
        });
    }

    // set_achievement_reward_gold(key, gold) -> String (0 clears)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_reward_gold", move |key: String, gold: i64| -> String {
            update_def(&db, &state, &key, |d| d.reward.gold = if gold <= 0 { None } else { Some(gold as i32) })
        });
    }

    // set_achievement_reward_item(key, item_vnum) -> String (empty clears)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_reward_item", move |key: String, vnum: String| -> String {
            update_def(&db, &state, &key, |d| d.reward.item_vnum = if vnum.is_empty() { None } else { Some(vnum.clone()) })
        });
    }

    // set_achievement_criterion_manual(key) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_manual", move |key: String| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::Manual)
        });
    }

    // set_achievement_criterion_counter(key, counter, threshold) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_counter", move |key: String, counter: String, threshold: i64| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::Counter {
                counter: counter.clone(),
                threshold: threshold.max(1) as u32,
            })
        });
    }

    // set_achievement_criterion_skill(key, skill, level) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_skill", move |key: String, skill: String, level: i64| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::SkillReached {
                skill: skill.clone(),
                level: level as i32,
            })
        });
    }

    // set_achievement_criterion_recipe(key, recipe_key) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_recipe", move |key: String, recipe_key: String| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::LearnedRecipe {
                recipe_key: recipe_key.clone(),
            })
        });
    }

    // set_achievement_criterion_lease(key, area_vnum) -> String (empty area_vnum for any)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_lease", move |key: String, area_vnum: String| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::OwnedLease {
                area_vnum: if area_vnum.is_empty() { None } else { Some(area_vnum.clone()) },
            })
        });
    }

    // set_achievement_criterion_gold(key, amount) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_criterion_gold", move |key: String, amount: i64| -> String {
            update_def(&db, &state, &key, |d| d.criterion = AchievementCriterion::GoldHeld {
                amount: amount as i32,
            })
        });
    }
}

fn update_def<F>(db: &Db, state: &SharedState, key: &str, mutator: F) -> String
where
    F: FnOnce(&mut AchievementDef),
{
    let key_lc = key.to_lowercase();
    match db.get_achievement(&key_lc) {
        Ok(Some(mut def)) => {
            mutator(&mut def);
            if let Err(e) = db.save_achievement(def.clone()) {
                format!("db error: {}", e)
            } else {
                sync_world_after_save(state, def);
                String::new()
            }
        }
        Ok(None) => format!("achievement '{}' not found in database (or it's a JSON-only definition)", key_lc),
        Err(e) => format!("db error: {}", e),
    }
}

/// Mirror the just-saved (or just-created) definition into the in-memory
/// world map so subsequent reads (`get_achievement_def`, `list_achievement_defs`,
/// counter notify path) see the update without requiring a restart. Also
/// refreshes the counter index from scratch — the dataset is small.
fn sync_world_after_save(state: &SharedState, def: AchievementDef) {
    let Ok(mut world) = state.lock() else { return };
    let key = def.key.to_lowercase();
    world.achievement_definitions.insert(key, def);
    world.achievement_index_by_counter = rebuild_counter_index(&world.achievement_definitions);
}

fn sync_world_after_delete(state: &SharedState, key: &str) {
    let Ok(mut world) = state.lock() else { return };
    let key_lc = key.to_lowercase();
    world.achievement_definitions.remove(&key_lc);
    world.achievement_index_by_counter = rebuild_counter_index(&world.achievement_definitions);
}

fn rebuild_counter_index(
    defs: &std::collections::HashMap<String, AchievementDef>,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut index: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (key, def) in defs {
        if let AchievementCriterion::Counter { counter, .. } = &def.criterion {
            index.entry(counter.clone()).or_default().push(key.clone());
        }
    }
    index
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

    // Reward
    let mut r = Map::new();
    r.insert("title".into(), Dynamic::from(def.reward.title.clone()));
    r.insert("gold".into(), Dynamic::from(def.reward.gold.unwrap_or(0) as i64));
    r.insert("item_vnum".into(), Dynamic::from(def.reward.item_vnum.clone().unwrap_or_default()));
    m.insert("reward".into(), Dynamic::from(r));
    m.insert("title".into(), Dynamic::from(def.reward.title.clone())); // Legacy compat for achievements.rhai

    // Criterion
    let mut c = Map::new();
    match &def.criterion {
        AchievementCriterion::Counter { counter, threshold } => {
            c.insert("kind".into(), Dynamic::from("counter"));
            c.insert("counter".into(), Dynamic::from(counter.clone()));
            c.insert("threshold".into(), Dynamic::from(*threshold as i64));
        }
        AchievementCriterion::SkillReached { skill, level } => {
            c.insert("kind".into(), Dynamic::from("skill_reached"));
            c.insert("skill".into(), Dynamic::from(skill.clone()));
            c.insert("level".into(), Dynamic::from(*level as i64));
        }
        AchievementCriterion::LearnedRecipe { recipe_key } => {
            c.insert("kind".into(), Dynamic::from("recipe_learned"));
            c.insert("recipe_key".into(), Dynamic::from(recipe_key.clone()));
        }
        AchievementCriterion::OwnedLease { area_vnum } => {
            c.insert("kind".into(), Dynamic::from("lease_owned"));
            c.insert("area_vnum".into(), Dynamic::from(area_vnum.clone().unwrap_or_default()));
        }
        AchievementCriterion::GoldHeld { amount } => {
            c.insert("kind".into(), Dynamic::from("gold_held"));
            c.insert("amount".into(), Dynamic::from(*amount as i64));
        }
        AchievementCriterion::Manual => {
            c.insert("kind".into(), Dynamic::from("manual"));
        }
    }
    m.insert("criterion".into(), Dynamic::from(c));

    // Source
    let mut s = Map::new();
    match &def.source {
        AchievementSource::Json { file } => {
            s.insert("kind".into(), Dynamic::from("json"));
            s.insert("file".into(), Dynamic::from(file.clone()));
        }
        AchievementSource::Db { author } => {
            s.insert("kind".into(), Dynamic::from("db"));
            s.insert("author".into(), Dynamic::from(author.clone()));
        }
    }
    m.insert("source".into(), Dynamic::from(s));

    m
}
