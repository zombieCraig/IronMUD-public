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
    ItemLocation,
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

/// Send one line to a player by name. Public because `morality::apply_delta`
/// needs the same "message a player you only have the name of" primitive for
/// its tier-shift announcement.
pub fn send_to_player(connections: &SharedConnections, name: &str, message: &str) {
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

/// Apply a mutation to a player's character, preferring the live in-memory
/// session copy when the player is online.
///
/// For online players this mutates `session.character` in place while holding
/// the connections lock — the same lock the regen tick holds while flushing
/// `session.character` to the DB. Serializing on that lock prevents a
/// concurrent regen flush of a *stale* session from clobbering the write
/// (bug #14: a freshly-awarded achievement/counter vanishing from the DB),
/// and conversely avoids overwriting session-only state (HP/stamina regen)
/// that a fresh DB load would not reflect. Offline players fall back to a
/// plain DB load/save.
///
/// `mutate` returns `true` when the character was changed and should be
/// persisted. Returns the resulting character, or `None` if the player can't
/// be found online or in the DB (or the save failed).
/// Public because it is the general "write to a player, correctly" primitive,
/// not an achievement detail. `crate::purse` needs exactly this for every gold
/// and bank write: the session copy is authoritative for anyone online, and a
/// DB-only write to an online player is not merely racy but *reverted* — the
/// thirst/hunger/regen ticks flush `session.character` wholesale
/// (`src/ticks/character.rs`). Anything that mutates a character out of band
/// should come through here rather than growing a seventh hand-rolled
/// load/save/sync.
pub fn apply_to_character<F>(
    db: &Db,
    connections: &SharedConnections,
    player_name: &str,
    mutate: F,
) -> Option<CharacterData>
where
    F: FnOnce(&mut CharacterData) -> bool,
{
    if let Ok(mut conns) = connections.lock() {
        for (_, session) in conns.iter_mut() {
            if let Some(ref mut ch) = session.character {
                if ch.name.eq_ignore_ascii_case(player_name) {
                    if mutate(ch) {
                        let snapshot = ch.clone();
                        if db.save_character_data(snapshot.clone()).is_err() {
                            return None;
                        }
                        return Some(snapshot);
                    }
                    return Some(ch.clone());
                }
            }
        }
    }

    let mut ch = match db.get_character_data(&player_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return None,
    };
    if mutate(&mut ch) && db.save_character_data(ch.clone()).is_err() {
        return None;
    }
    Some(ch)
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
        tracing::warn!("achievements: DG award refused for engine-criterion key '{}'", key_lc);
        return false;
    }
    unlock_with_def(db, connections, &def, player_name)
}

/// Shared unlock pipeline: dedup, insert, title default, gold/item reward,
/// persist, session sync, banner. Caller is responsible for criterion
/// gating (manual-vs-engine) before reaching this point.
fn unlock_with_def(db: &Db, connections: &SharedConnections, def: &AchievementDef, player_name: &str) -> bool {
    let key_lc = def.key.to_lowercase();

    let mut newly_unlocked = false;
    let mut gold_awarded: Option<i32> = None;
    let mut morality_shift: Option<(i32, i32)> = None;
    let mut trait_points_awarded: Option<i32> = None;

    let applied = apply_to_character(db, connections, player_name, |ch| {
        if ch.achievements_unlocked.contains_key(&key_lc) {
            return false;
        }

        ch.achievements_unlocked.insert(
            key_lc.clone(),
            crate::types::AchievementUnlock {
                unlocked_at: now_secs(),
            },
        );

        if ch.active_title.is_none() {
            ch.active_title = Some(key_lc.clone());
        }

        if let Some(gold) = def.reward.gold {
            ch.gold = ch.gold.saturating_add(gold);
            if ch.gold > ch.gold_high_water {
                ch.gold_high_water = ch.gold;
            }
            gold_awarded = Some(gold);
        }

        // Item rewards are delivered after the character mutation commits
        // (they touch the item tree, not the character) — see below.

        if def.reward.morality_delta != 0 {
            let before = ch.morality;
            ch.morality = crate::morality::adjust(before, def.reward.morality_delta);
            morality_shift = Some((before, ch.morality));
        }

        // Only positive grants are honoured here. A definition carrying a
        // negative value would have to have bypassed the editors, and
        // silently deducting build currency on what the player just read as
        // a reward is worse than ignoring it.
        if def.reward.trait_points > 0 {
            ch.trait_points = ch.trait_points.saturating_add(def.reward.trait_points);
            trait_points_awarded = Some(def.reward.trait_points);
        }

        newly_unlocked = true;
        true
    });

    if applied.is_none() || !newly_unlocked {
        return false;
    }

    let banner = format!(
        "\x1b[1;33m*** Achievement unlocked: {} ***\x1b[0m\n  {}",
        def.name, def.description
    );
    send_to_player(connections, player_name, &banner);
    if let Some(gold) = gold_awarded {
        if gold > 0 {
            send_to_player(connections, player_name, &format!("You receive {} gold.", gold));
        }
    }
    if let Some((before, after)) = morality_shift {
        if let Some(line) = crate::morality::tier_shift_message(before, after) {
            send_to_player(connections, player_name, line);
        }
    }
    if let Some(points) = trait_points_awarded {
        send_to_player(
            connections,
            player_name,
            &format!(
                "\x1b[1;36mYou gain {} trait point{}.\x1b[0m  Spend them with 'traits'.",
                points,
                if points == 1 { "" } else { "s" }
            ),
        );
    }
    if let Some(ref vnum) = def.reward.item_vnum {
        deliver_item_reward(db, connections, player_name, vnum);
    }

    true
}

/// Deliver an achievement's item reward into the player's inventory.
///
/// Mirrors the quest `QuestReward::Item` path (`src/quest/mod.rs`): spawn one
/// instance from the prototype, relocate it to the player's inventory, and
/// persist. Items live in their own sled tree keyed by id (inventory is the
/// set of items whose `location` is `Inventory(name)`), so this never touches
/// the character record and needs no session sync. Honors the prototype's
/// `world_max_count` cap — `spawn_item_from_prototype` returns `Ok(None)` when
/// the vnum is unknown or the live cap is reached, which we surface as a warn
/// rather than a silent drop.
fn deliver_item_reward(db: &Db, connections: &SharedConnections, player_name: &str, vnum: &str) {
    match db.spawn_item_from_prototype(vnum) {
        Ok(Some(mut item)) => {
            item.location = ItemLocation::Inventory(player_name.to_string());
            if db.save_item_data(item).is_ok() {
                let label = db
                    .get_item_by_vnum(vnum)
                    .ok()
                    .flatten()
                    .map(|i| i.short_desc)
                    .unwrap_or_else(|| format!("item {}", vnum));
                send_to_player(connections, player_name, &format!("You receive {}.", label));
            } else {
                tracing::warn!("achievement item reward '{}' failed to save for {}", vnum, player_name);
            }
        }
        Ok(None) => tracing::warn!(
            "achievement item reward '{}' not delivered to {}: unknown prototype or world cap reached",
            vnum,
            player_name
        ),
        Err(e) => tracing::warn!(
            "achievement item reward '{}' spawn error for {}: {}",
            vnum,
            player_name,
            e
        ),
    }
}

/// An achievement's display name, from the registry. Falls back to `None` when
/// no definition is loaded, so callers can print the raw key rather than
/// nothing.
pub fn describe(state: &SharedState, key: &str) -> Option<String> {
    let world = state.lock().ok()?;
    world
        .achievement_definitions
        .get(&key.to_lowercase())
        .map(|d| d.name.clone())
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

    let mut new_value = 0u32;
    let applied = apply_to_character(db, connections, player_name, |ch| {
        let entry = ch.achievement_counters.entry(key_lc.clone()).or_insert(0);
        *entry = entry.saturating_add(increment);
        new_value = *entry;
        true
    });
    if applied.is_none() {
        return 0;
    }

    award_counter_thresholds(db, connections, state, player_name, &key_lc, new_value);
    new_value
}

/// Award every counter achievement whose threshold `value` now meets.
///
/// Split out of [`notify_counter_core`] because two callers need it and they
/// disagree about how the counter got there: one adds to it, the other sets it
/// outright from a scan. What happens once the number lands is the same, and
/// having it in one place is what stops the two paths drifting on which
/// achievements they check.
fn award_counter_thresholds(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    key_lc: &str,
    value: u32,
) {
    let candidates: Vec<(String, u32)> = match state.lock() {
        Ok(world) => world
            .achievement_index_by_counter
            .get(key_lc)
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
        Err(_) => return,
    };

    for (key, threshold) in candidates {
        if value >= threshold {
            award_core(db, connections, state, player_name, &key, false);
        }
    }
}

/// Set a counter to an exact value, and award anything the new value reaches.
///
/// The counterpart to [`notify_counter_core`] for figures that are *derived*
/// rather than accumulated. A builder's room count is not a tally of events —
/// it is a property of the world right now, recomputed by a scan — so it has
/// to be able to go **down** when content is deleted. That is the whole
/// anti-farm story in one method: there is no event to repeat, and undoing
/// work undoes the number.
///
/// Achievements already unlocked are never revoked when a counter falls. An
/// achievement records that you did the thing, and you did.
pub fn reconcile_counter_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    counter_key: &str,
    value: u32,
) -> u32 {
    if !enabled(db) {
        return 0;
    }
    let key_lc = counter_key.to_lowercase();

    let mut changed = false;
    let applied = apply_to_character(db, connections, player_name, |ch| {
        let entry = ch.achievement_counters.entry(key_lc.clone()).or_insert(0);
        changed = *entry != value;
        *entry = value;
        changed
    });
    if applied.is_none() {
        return 0;
    }
    if changed {
        award_counter_thresholds(db, connections, state, player_name, &key_lc, value);
    }
    value
}

/// [`reconcile_counter_core`] for a whole set of counters at once.
///
/// One `apply_to_character` instead of one per key. The build-score tick
/// reconciles seven counters for every builder on the server, and
/// `apply_to_character` takes the connections mutex — the hottest lock in the
/// process — and holds it across a character save and a sled flush. Seven times
/// per builder, every five minutes, is a lot of contention to buy nothing: the
/// values all come from the same scan and there is no reason to write them
/// separately.
///
/// Thresholds are still awarded per key, and still only for keys that moved.
pub fn reconcile_counters_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    counters: &[(&str, u32)],
) {
    if !enabled(db) || counters.is_empty() {
        return;
    }
    let pairs: Vec<(String, u32)> = counters.iter().map(|(k, v)| (k.to_lowercase(), *v)).collect();

    let mut moved: Vec<(String, u32)> = Vec::new();
    let applied = apply_to_character(db, connections, player_name, |ch| {
        moved.clear();
        for (key, value) in &pairs {
            // Read before inserting. `entry().or_insert(0)` would add a
            // zero-valued row for every counter a builder has never scored on,
            // and for an *online* builder that row survives in the session copy
            // even when nothing is persisted — the regen tick flushes it later.
            if ch.achievement_counters.get(key).copied().unwrap_or(0) == *value {
                continue;
            }
            ch.achievement_counters.insert(key.clone(), *value);
            moved.push((key.clone(), *value));
        }
        !moved.is_empty()
    });
    if applied.is_none() {
        return;
    }
    for (key, value) in moved {
        award_counter_thresholds(db, connections, state, player_name, &key, value);
    }
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

/// Record something in a persisted "distinct things seen" set on the character
/// and keep a counter in step with the set's size. Returns whether the value
/// was new.
///
/// `insert` receives the character and must add to the relevant set, returning
/// true only when the value was not already present. `size` reads that set's
/// length back — the counter is reconciled against it rather than blindly
/// incremented, which does two things:
///
///  * Characters predating a counter already have a full set and a counter of
///    zero. Without reconciliation they would have to start over.
///  * If a bump is ever lost (achievements disabled for a while, a crash
///    between the two writes) the next new value repairs the drift.
///
/// The set insert is deliberately outside the achievement `enabled` gate.
/// These sets have non-achievement consumers — `rooms_visited` drives map
/// fog-of-war — and must keep filling whether or not achievements are on. Only
/// the counter bump is gated, by `notify_counter_core`.
pub fn notify_distinct_core<I, S>(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    counter_key: &str,
    insert: I,
    size: S,
) -> bool
where
    I: FnOnce(&mut CharacterData) -> bool,
    S: Fn(&CharacterData) -> usize,
{
    let mut inserted = false;
    let mut shortfall = 0u32;
    let applied = apply_to_character(db, connections, player_name, |ch| {
        inserted = insert(ch);
        if inserted {
            let total = size(ch) as u32;
            let counted = ch.achievement_counters.get(counter_key).copied().unwrap_or(0);
            shortfall = total.saturating_sub(counted);
        }
        inserted
    });
    if applied.is_none() || !inserted {
        return false;
    }
    notify_counter_core(db, connections, state, player_name, counter_key, shortfall);
    true
}

/// Record a room as explored and bump `rooms.visited` if it was new.
///
/// This is the one place `rooms_visited` grows. It had two independent
/// insertion sites (the look/fog-of-war path and `mark_room_visited`), so a
/// counter added at either one alone would have undercounted; routing both
/// through here means the exploration category cannot silently miss rooms.
pub fn notify_room_visited_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    room_id: &uuid::Uuid,
) -> bool {
    notify_distinct_core(
        db,
        connections,
        state,
        player_name,
        "rooms.visited",
        |ch| ch.rooms_visited.insert(*room_id),
        |ch| ch.rooms_visited.len(),
    )
}

/// Record a social verb the character has performed and bump
/// `socials.distinct` if it is one they had not used before.
pub fn notify_social_used_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    verb: &str,
) -> bool {
    let verb = verb.to_lowercase();
    notify_distinct_core(
        db,
        connections,
        state,
        player_name,
        "socials.distinct",
        |ch| ch.socials_used.insert(verb),
        |ch| ch.socials_used.len(),
    )
}

/// Record an NPC the character has opened a dialogue with and bump
/// `npcs.talked_to` if they had not spoken to that prototype before.
///
/// Keyed on the prototype vnum, not the instance id: two spawns of the same
/// shopkeeper are the same character to the player, and instance ids churn on
/// every area reset.
pub fn notify_npc_talked_to_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    player_name: &str,
    mob_vnum: &str,
) -> bool {
    if mob_vnum.is_empty() {
        return false;
    }
    let vnum = mob_vnum.to_lowercase();
    notify_distinct_core(
        db,
        connections,
        state,
        player_name,
        "npcs.talked_to",
        |ch| ch.npcs_talked_to.insert(vnum),
        |ch| ch.npcs_talked_to.len(),
    )
}

/// Bump kill counters and evaluate matching achievements. Convenience
/// wrapper over `notify_counter_core` for the combat-tick hook.
/// Returns the killer's new lifetime `kills.any` total, so the caller can
/// drive milestone feedback without re-reading the character. Zero when the
/// achievement system is disabled.
pub fn notify_kill_core(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    killer_name: &str,
    mob_vnum: &str,
) -> u32 {
    let total = notify_counter_core(db, connections, state, killer_name, "kills.any", 1);
    notify_counter_core(
        db,
        connections,
        state,
        killer_name,
        &format!("kills.{}", mob_vnum.to_lowercase()),
        1,
    );
    total
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

    // get_achievement_counter(player_name, counter_key) -> i64
    // Read-only. Unknown players and unwritten counters both read 0, so a
    // display can ask for a counter that no chokepoint has fired yet.
    {
        let db = db.clone();
        engine.register_fn(
            "get_achievement_counter",
            move |player_name: String, counter_key: String| -> i64 {
                match db.get_character_data(&player_name.to_lowercase()) {
                    Ok(Some(c)) => c.achievement_counters.get(&counter_key).copied().unwrap_or(0) as i64,
                    _ => 0,
                }
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
        engine.register_fn("award_achievement", move |player_name: String, key: String| -> bool {
            award_core(&db, &connections, &state, &player_name, &key, true)
        });
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
            // Category first, then key. Sorting by key alone interleaved the
            // categories, which was tolerable at a dozen definitions and reads
            // as a wall of mixed text now there are dozens. The command already
            // prints a category legend, so grouping matches what it promises.
            // The builder listing (`list_achievement_defs`) stays key-sorted:
            // there you are looking up a key you already know.
            let field = |m: &Map, name: &str| -> String {
                m.get(name)
                    .and_then(|d| d.clone().into_string().ok())
                    .unwrap_or_default()
            };
            out.sort_by(|a, b| {
                field(a, "category")
                    .cmp(&field(b, "category"))
                    .then_with(|| field(a, "key").cmp(&field(b, "key")))
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
        engine.register_fn("set_active_title", move |player_name: String, key: String| -> bool {
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
        });
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
                let ak = a
                    .get("key")
                    .and_then(|d| d.clone().into_string().ok())
                    .unwrap_or_default();
                let bk = b
                    .get("key")
                    .and_then(|d| d.clone().into_string().ok())
                    .unwrap_or_default();
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
        engine.register_fn(
            "set_achievement_description",
            move |key: String, desc: String| -> String {
                update_def(&db, &state, &key, |d| d.description = desc.clone())
            },
        );
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
        engine.register_fn(
            "set_achievement_reward_title",
            move |key: String, title: String| -> String {
                update_def(&db, &state, &key, |d| d.reward.title = title.clone())
            },
        );
    }

    // set_achievement_reward_gold(key, gold) -> String (0 clears)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn("set_achievement_reward_gold", move |key: String, gold: i64| -> String {
            update_def(&db, &state, &key, |d| {
                d.reward.gold = if gold <= 0 { None } else { Some(gold as i32) }
            })
        });
    }

    // set_achievement_reward_item(key, item_vnum) -> String (empty clears)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_reward_item",
            move |key: String, vnum: String| -> String {
                update_def(&db, &state, &key, |d| {
                    d.reward.item_vnum = if vnum.is_empty() { None } else { Some(vnum.clone()) }
                })
            },
        );
    }

    // set_achievement_reward_morality(key, delta) -> String
    // Positive pushes toward Good, negative toward Evil; clamped at unlock.
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_reward_morality",
            move |key: String, delta: i64| -> String {
                update_def(&db, &state, &key, |d| {
                    d.reward.morality_delta =
                        (delta as i32).clamp(crate::morality::MORALITY_MIN, crate::morality::MORALITY_MAX);
                })
            },
        );
    }

    // set_achievement_reward_trait_points(key, points) -> String
    // 0 clears. Negatives are refused rather than clamped so the builder
    // learns the field is a grant, not a levy.
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_reward_trait_points",
            move |key: String, points: i64| -> String {
                if points < 0 {
                    return "Trait point rewards cannot be negative.".to_string();
                }
                update_def(&db, &state, &key, |d| d.reward.trait_points = points as i32)
            },
        );
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
        engine.register_fn(
            "set_achievement_criterion_counter",
            move |key: String, counter: String, threshold: i64| -> String {
                update_def(&db, &state, &key, |d| {
                    d.criterion = AchievementCriterion::Counter {
                        counter: counter.clone(),
                        threshold: threshold.max(1) as u32,
                    }
                })
            },
        );
    }

    // set_achievement_criterion_skill(key, skill, level) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_criterion_skill",
            move |key: String, skill: String, level: i64| -> String {
                update_def(&db, &state, &key, |d| {
                    d.criterion = AchievementCriterion::SkillReached {
                        skill: skill.clone(),
                        level: level as i32,
                    }
                })
            },
        );
    }

    // set_achievement_criterion_recipe(key, recipe_key) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_criterion_recipe",
            move |key: String, recipe_key: String| -> String {
                update_def(&db, &state, &key, |d| {
                    d.criterion = AchievementCriterion::LearnedRecipe {
                        recipe_key: recipe_key.clone(),
                    }
                })
            },
        );
    }

    // set_achievement_criterion_lease(key, area_vnum) -> String (empty area_vnum for any)
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_criterion_lease",
            move |key: String, area_vnum: String| -> String {
                update_def(&db, &state, &key, |d| {
                    d.criterion = AchievementCriterion::OwnedLease {
                        area_vnum: if area_vnum.is_empty() {
                            None
                        } else {
                            Some(area_vnum.clone())
                        },
                    }
                })
            },
        );
    }

    // set_achievement_criterion_gold(key, amount) -> String
    {
        let db = db.clone();
        let state = state.clone();
        engine.register_fn(
            "set_achievement_criterion_gold",
            move |key: String, amount: i64| -> String {
                update_def(&db, &state, &key, |d| {
                    d.criterion = AchievementCriterion::GoldHeld { amount: amount as i32 }
                })
            },
        );
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
        Ok(None) => format!(
            "achievement '{}' not found in database (or it's a JSON-only definition)",
            key_lc
        ),
        Err(e) => format!("db error: {}", e),
    }
}

/// Mirror the just-saved (or just-created) definition into the in-memory
/// world map so subsequent reads (`get_achievement_def`, `list_achievement_defs`,
/// counter notify path) see the update without requiring a restart. Also
/// refreshes the counter index from scratch — the dataset is small.
pub fn sync_world_after_save(state: &SharedState, def: AchievementDef) {
    let Ok(mut world) = state.lock() else { return };
    let key = def.key.to_lowercase();
    world.achievement_definitions.insert(key, def);
    world.achievement_index_by_counter = rebuild_counter_index(&world.achievement_definitions);
}

pub fn sync_world_after_delete(state: &SharedState, key: &str) {
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
    r.insert(
        "item_vnum".into(),
        Dynamic::from(def.reward.item_vnum.clone().unwrap_or_default()),
    );
    r.insert("morality_delta".into(), Dynamic::from(def.reward.morality_delta as i64));
    r.insert("trait_points".into(), Dynamic::from(def.reward.trait_points as i64));
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
