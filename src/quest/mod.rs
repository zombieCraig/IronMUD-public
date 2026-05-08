//! Quest engine core. Lifecycle helpers (`offer`, `abandon`, `try_complete`)
//! and listener entry points (`handle_mob_kill`, `handle_item_to_mob`) called
//! from combat / give paths. Reward dispatch reuses existing primitives:
//! achievements, recipes, gold, item spawning, skill XP — quests don't ship a
//! parallel grant pipeline.
//!
//! The quest *prototypes* live in the `quests` sled tree (`db.list_all_quests`,
//! etc.); per-player state lives on `CharacterData.active_quests` /
//! `completed_quests` and rides the existing character save path.

use crate::SharedState;
use crate::db::Db;
use crate::types::{
    ActiveQuest, CharacterData, ItemData, ItemLocation, MobileData, QuestData, QuestObjective, QuestReward,
    SkillProgress,
};
use crate::SharedConnections;

/// Send a single line to the named character if they're online. No-op if
/// they're offline (quest progress just persists silently).
fn notify(connections: &SharedConnections, char_name: &str, line: &str) {
    if let Ok(conns) = connections.lock() {
        for session in conns.values() {
            if let Some(ref ch) = session.character {
                if ch.name.eq_ignore_ascii_case(char_name) {
                    let _ = session.sender.send(format!("{}\n", line));
                    return;
                }
            }
        }
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Public wrapper for tick code outside this module that needs a clock
/// source matching the one ActiveQuest.started_at uses.
pub fn now_secs_pub() -> i64 {
    now_secs()
}

/// Walk an online player's active quests and drop any whose
/// duration_secs has elapsed since started_at. Sends a "[ Quest expired:
/// X ]" line per drop. Saves once at the end if anything changed.
pub fn expire_quests_for(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    now: i64,
) {
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return,
    };
    if ch.active_quests.is_empty() {
        return;
    }
    let mut expired: Vec<(String, String)> = Vec::new();
    let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    for qvnum in &active_keys {
        let quest = match db.get_quest_data(qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        let duration = match quest.duration_secs {
            Some(d) if d > 0 => d,
            _ => continue,
        };
        let started = ch
            .active_quests
            .get(qvnum)
            .map(|aq| aq.started_at)
            .unwrap_or(0);
        if now.saturating_sub(started) >= duration {
            expired.push((quest.vnum.clone(), quest.name.clone()));
        }
    }
    if expired.is_empty() {
        return;
    }
    for (vnum, _) in &expired {
        ch.active_quests.remove(vnum);
    }
    let _ = db.save_character_data(ch);
    for (_, name) in expired {
        let line = format!("\x1b[1;31m[ Quest expired: {} ]\x1b[0m", name);
        notify(connections, char_name, &line);
    }
}

/// Resolve a `keyword_or_vnum` to a quest prototype. Tries vnum exact-match
/// first, then case-insensitive name contains, then keyword contains.
pub fn find_quest(db: &Db, keyword_or_vnum: &str) -> Option<QuestData> {
    let needle = keyword_or_vnum.trim();
    if needle.is_empty() {
        return None;
    }
    if let Ok(Some(q)) = db.get_quest_data(needle) {
        return Some(q);
    }
    let lc = needle.to_lowercase();
    if let Ok(all) = db.list_all_quests() {
        for q in &all {
            if q.keywords.iter().any(|k| k.to_lowercase() == lc) {
                return Some(q.clone());
            }
        }
        for q in &all {
            if q.name.to_lowercase().contains(&lc) {
                return Some(q.clone());
            }
        }
        for q in &all {
            if q.keywords.iter().any(|k| k.to_lowercase().contains(&lc)) {
                return Some(q.clone());
            }
        }
    }
    None
}

/// Offer a quest to a character. Returns "" on success, error string otherwise.
pub fn offer(db: &Db, char_name: &str, vnum: &str) -> String {
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return "no such character".to_string(),
    };
    let quest = match db.get_quest_data(vnum) {
        Ok(Some(q)) => q,
        _ => return format!("no such quest `{}`", vnum),
    };
    if ch.active_quests.contains_key(&quest.vnum) {
        return format!("You are already on quest `{}`.", quest.name);
    }
    if ch.completed_quests.contains(&quest.vnum) && !quest.repeatable {
        return format!("You have already completed `{}`.", quest.name);
    }
    // Slice 3a: prereq + skill gates.
    if let Some(prereq) = &quest.prereq_quest_vnum {
        if !ch.completed_quests.contains(prereq) {
            let pname = db
                .get_quest_data(prereq)
                .ok()
                .flatten()
                .map(|q| q.name)
                .unwrap_or_else(|| prereq.clone());
            return format!("You must first complete `{}`.", pname);
        }
    }
    if let Some(min_total) = quest.min_player_skill_total {
        let total: i32 = ch.skills.values().map(|sp| sp.level).sum();
        if total < min_total {
            return format!(
                "You're not skilled enough yet ({} / {}).",
                total, min_total
            );
        }
    }
    let aq = ActiveQuest {
        started_at: now_secs(),
        ..Default::default()
    };
    ch.active_quests.insert(quest.vnum.clone(), aq);
    if let Err(e) = db.save_character_data(ch) {
        return format!("save error: {}", e);
    }
    String::new()
}

/// Abandon an active quest. Returns "" on success, error string otherwise.
pub fn abandon(db: &Db, char_name: &str, vnum: &str) -> String {
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return "no such character".to_string(),
    };
    if ch.active_quests.remove(vnum).is_none() {
        return format!("you are not on quest `{}`", vnum);
    }
    if let Err(e) = db.save_character_data(ch) {
        return format!("save error: {}", e);
    }
    String::new()
}

/// Are all of the quest's objectives satisfied for this character right now?
pub fn is_completable(db: &Db, ch: &CharacterData, quest: &QuestData) -> bool {
    let progress = match ch.active_quests.get(&quest.vnum) {
        Some(p) => p,
        None => return false,
    };
    for obj in &quest.objectives {
        if !objective_done(db, ch, progress, obj) {
            return false;
        }
    }
    true
}

fn objective_done(db: &Db, ch: &CharacterData, progress: &ActiveQuest, obj: &QuestObjective) -> bool {
    match obj {
        QuestObjective::KillMob { vnum, count } => {
            progress.kill_progress.get(vnum).copied().unwrap_or(0) >= *count
        }
        QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } => {
            if return_to_mob_vnum.is_some() {
                // Items have already been consumed; check the recorded turn-in count.
                progress.item_progress.get(vnum).copied().unwrap_or(0) >= *qty
            } else {
                // No turn-in mob — check current inventory.
                count_inventory_vnum(db, &ch.name, vnum) >= *qty
            }
        }
        QuestObjective::VisitRoom { vnum } => progress.rooms_visited.contains(vnum),
        QuestObjective::DgFlag { var, .. } => progress.flags_set.contains(var),
    }
}

fn count_inventory_vnum(db: &Db, char_name: &str, vnum: &str) -> i32 {
    let mut n = 0;
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if item.is_prototype {
                continue;
            }
            if item.vnum.as_deref() != Some(vnum) {
                continue;
            }
            if let ItemLocation::Inventory(ref name) = item.location {
                if name.eq_ignore_ascii_case(char_name) {
                    n += 1;
                }
            }
        }
    }
    n
}

/// Try to complete an active quest. If all objectives are met, applies all
/// rewards, moves the vnum from `active_quests` to `completed_quests`, and
/// returns Ok(true). Returns Ok(false) when not yet completable.
pub fn try_complete(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    quest_vnum: &str,
) -> bool {
    let quest = match db.get_quest_data(quest_vnum) {
        Ok(Some(q)) => q,
        _ => return false,
    };
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return false,
    };
    if !is_completable(db, &ch, &quest) {
        return false;
    }

    // For BringItem objectives WITHOUT return_to_mob_vnum, we consume from
    // inventory now (turn-in completion via dialogue effect).
    for obj in &quest.objectives {
        if let QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } = obj
        {
            if return_to_mob_vnum.is_none() {
                consume_inventory_items(db, &ch.name, vnum, *qty);
            }
        }
    }

    // Move the quest from active to completed BEFORE granting rewards so a
    // reward that itself touches character state (gold, achievements) sees a
    // consistent quest state.
    ch.active_quests.remove(&quest.vnum);
    ch.completed_quests.insert(quest.vnum.clone());

    if !quest.completion_text.is_empty() {
        notify(connections, char_name, &quest.completion_text);
    }

    // Persist the active->completed transition before reward calls (some of
    // which load character independently).
    let _ = db.save_character_data(ch.clone());

    // Apply rewards.
    for reward in &quest.rewards {
        apply_reward(db, connections, state, &ch.name, reward);
    }

    let line = format!("\x1b[1;33m[ Quest complete: {} ]\x1b[0m", quest.name);
    notify(connections, char_name, &line);
    true
}

fn consume_inventory_items(db: &Db, char_name: &str, vnum: &str, qty: i32) {
    let mut consumed = 0;
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if consumed >= qty {
                break;
            }
            if item.is_prototype {
                continue;
            }
            if item.vnum.as_deref() != Some(vnum) {
                continue;
            }
            if let ItemLocation::Inventory(ref name) = item.location {
                if name.eq_ignore_ascii_case(char_name) {
                    if db.delete_item(&item.id).is_ok() {
                        consumed += 1;
                    }
                }
            }
        }
    }
}

fn apply_reward(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    reward: &QuestReward,
) {
    match reward {
        QuestReward::Gold { amount } => {
            if let Ok(Some(mut ch)) = db.get_character_data(&char_name.to_lowercase()) {
                let next = (ch.gold as i64).saturating_add(*amount);
                ch.gold = next.clamp(0, i32::MAX as i64) as i32;
                let _ = db.save_character_data(ch);
                notify(connections, char_name, &format!("[ +{} gold ]", amount));
            }
        }
        QuestReward::Item { vnum, qty } => {
            let mut given = 0;
            for _ in 0..*qty {
                match db.spawn_item_from_prototype(vnum) {
                    Ok(Some(mut item)) => {
                        item.location = ItemLocation::Inventory(char_name.to_string());
                        if db.save_item_data(item.clone()).is_ok() {
                            given += 1;
                        }
                    }
                    _ => break,
                }
            }
            if given > 0 {
                let label = db
                    .get_item_by_vnum(vnum)
                    .ok()
                    .flatten()
                    .map(|i| i.short_desc)
                    .unwrap_or_else(|| format!("item {}", vnum));
                notify(connections, char_name, &format!("[ You receive: {} ]", label));
            }
        }
        QuestReward::SkillXp { skill, amount } => {
            let key = skill.to_lowercase();
            if let Ok(Some(mut ch)) = db.get_character_data(&char_name.to_lowercase()) {
                let entry = ch.skills.entry(key.clone()).or_insert(SkillProgress::default());
                if entry.level >= 10 {
                    let _ = db.save_character_data(ch);
                    return;
                }
                entry.experience += *amount;
                let mut leveled = false;
                while entry.experience >= 100 && entry.level < 10 {
                    entry.experience -= 100;
                    entry.level += 1;
                    leveled = true;
                }
                let _ = db.save_character_data(ch);
                if leveled {
                    notify(
                        connections,
                        char_name,
                        &format!("\x1b[1;33mYour {} skill has improved!\x1b[0m", key.replace('_', " ")),
                    );
                } else {
                    notify(
                        connections,
                        char_name,
                        &format!("[ +{} {} xp ]", amount, key.replace('_', " ")),
                    );
                }
            }
        }
        QuestReward::Achievement { key } => {
            crate::script::achievements::award_core(db, connections, state, char_name, key, true);
        }
        QuestReward::LearnRecipe { recipe_id } => {
            if let Ok(Some(mut ch)) = db.get_character_data(&char_name.to_lowercase()) {
                if ch.learned_recipes.insert(recipe_id.clone()) {
                    let _ = db.save_character_data(ch);
                    notify(
                        connections,
                        char_name,
                        &format!("[ You have learned a new recipe: {} ]", recipe_id),
                    );
                }
            }
        }
    }
}

/// Mob-death listener entry point. Increments `kill_progress` for every
/// active quest whose `KillMob` objective matches `mob_proto_vnum`. Slice 3c:
/// credits all players in `damaged_by` (any non-zero damage), plus the
/// killing-blow `killer_name`. Sends a progress line, saves, and auto-completes
/// kill-only quests.
pub fn handle_mob_kill(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    killer_name: &str,
    mob_proto_vnum: &str,
    damaged_by: &std::collections::HashMap<String, i32>,
) {
    if mob_proto_vnum.is_empty() {
        return;
    }
    // Build deduped recipient list: every name in damaged_by + the killer.
    let mut recipients: std::collections::HashSet<String> = damaged_by
        .iter()
        .filter(|(_, dmg)| **dmg > 0)
        .map(|(name, _)| name.to_lowercase())
        .collect();
    if !killer_name.is_empty() {
        recipients.insert(killer_name.to_lowercase());
    }
    if recipients.is_empty() {
        return;
    }
    for name in recipients {
        credit_one_kill(db, connections, state, &name, mob_proto_vnum);
    }
}

fn credit_one_kill(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    mob_proto_vnum: &str,
) {
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return,
    };
    let mut dirty = false;
    let mut auto_complete: Vec<String> = Vec::new();
    let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    for qvnum in &active_keys {
        let quest = match db.get_quest_data(qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        let progress = match ch.active_quests.get_mut(qvnum) {
            Some(p) => p,
            None => continue,
        };
        for obj in &quest.objectives {
            if let QuestObjective::KillMob { vnum, count } = obj {
                if vnum == mob_proto_vnum {
                    let entry = progress.kill_progress.entry(vnum.clone()).or_insert(0);
                    if *entry < *count {
                        *entry += 1;
                        dirty = true;
                        let line = format!("[ {}: {}/{} ]", quest.name, *entry, count);
                        notify(connections, char_name, &line);
                    }
                }
            }
        }
        if has_only_kill_objectives(&quest) && is_completable_owned(db, &ch, &quest) {
            auto_complete.push(quest.vnum.clone());
        }
    }
    if dirty {
        let _ = db.save_character_data(ch);
    }
    for vnum in auto_complete {
        try_complete(db, connections, state, char_name, &vnum);
    }
}

fn has_only_kill_objectives(quest: &QuestData) -> bool {
    !quest.objectives.is_empty()
        && quest
            .objectives
            .iter()
            .all(|o| matches!(o, QuestObjective::KillMob { .. }))
}

/// Variant of `is_completable` that takes an owned character. Used after
/// mutable progress writes when the borrow on active_quests is still live.
fn is_completable_owned(db: &Db, ch: &CharacterData, quest: &QuestData) -> bool {
    let progress = match ch.active_quests.get(&quest.vnum) {
        Some(p) => p,
        None => return false,
    };
    for obj in &quest.objectives {
        if !objective_done(db, ch, progress, obj) {
            return false;
        }
    }
    true
}

/// Item-given-to-mob listener entry point, called from `give.rhai` BEFORE the
/// existing `OnReceive` trigger fire. Returns true if the item was consumed by
/// a quest turn-in (caller must skip the OnReceive fire and not return the
/// item to inventory). The mob receiver is identified by its prototype vnum
/// (matches `BringItem.return_to_mob_vnum`).
pub fn handle_item_to_mob(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    giver_name: &str,
    mob: &MobileData,
    item: &ItemData,
) -> bool {
    let item_vnum = match item.vnum.as_deref() {
        Some(v) => v,
        None => return false,
    };
    let mob_vnum = mob.vnum.clone();
    if mob_vnum.is_empty() {
        return false;
    }
    let mut ch = match db.get_character_data(&giver_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return false,
    };

    // Find the FIRST active quest with a matching BringItem objective for
    // this (item_vnum, mob_vnum) pair that's still open.
    let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    let mut matched_quest: Option<QuestData> = None;
    let mut matched_advance: Option<(String, i32, i32)> = None; // (item_vnum, new_count, target_qty)
    for qvnum in &active_keys {
        let quest = match db.get_quest_data(qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        for obj in &quest.objectives {
            if let QuestObjective::BringItem {
                vnum,
                qty,
                return_to_mob_vnum,
            } = obj
            {
                if vnum == item_vnum && return_to_mob_vnum.as_deref() == Some(mob_vnum.as_str()) {
                    let progress = match ch.active_quests.get_mut(qvnum) {
                        Some(p) => p,
                        None => continue,
                    };
                    let entry = progress.item_progress.entry(vnum.clone()).or_insert(0);
                    if *entry < *qty {
                        *entry += 1;
                        matched_advance = Some((vnum.clone(), *entry, *qty));
                        matched_quest = Some(quest.clone());
                    }
                    break;
                }
            }
        }
        if matched_quest.is_some() {
            break;
        }
    }

    let Some(quest) = matched_quest else {
        return false;
    };
    let (item_vnum_advanced, new_count, target_qty) = matched_advance.expect("set with quest");

    // Consume the item from the world (the give.rhai caller would otherwise
    // place it on the mob; we shortcut that).
    let _ = db.delete_item(&item.id);

    let item_label = item.short_desc.clone();
    let line = format!(
        "[ {}: {} ({}/{}) ]",
        quest.name, item_label, new_count, target_qty
    );
    notify(connections, giver_name, &line);

    // Persist progress.
    let _ = db.save_character_data(ch);

    // If this was the LAST item in a turn-in-only quest, auto-complete.
    let _ = item_vnum_advanced; // tag-only; suppression for clarity
    if let Ok(Some(ch_now)) = db.get_character_data(&giver_name.to_lowercase()) {
        if has_all_returnto_bringitem(&quest) && is_completable_owned(db, &ch_now, &quest) {
            try_complete(db, connections, state, giver_name, &quest.vnum);
        }
    }

    // Optionally: surface "(quest available)" hook here later.
    true
}

/// Room-visit listener entry point. Called from `go.rhai` after the player
/// enters a new room. `room_vnum` is the room's prototype vnum (resilient
/// across instance respawns); empty / non-prototype rooms are no-ops.
pub fn handle_room_visit(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    room_vnum: &str,
) -> bool {
    if char_name.is_empty() || room_vnum.is_empty() {
        return false;
    }
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return false,
    };
    let mut dirty = false;
    let mut auto_complete: Vec<String> = Vec::new();
    let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    for qvnum in &active_keys {
        let quest = match db.get_quest_data(qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        let progress = match ch.active_quests.get_mut(qvnum) {
            Some(p) => p,
            None => continue,
        };
        let mut matched = false;
        for obj in &quest.objectives {
            if let QuestObjective::VisitRoom { vnum } = obj {
                if vnum == room_vnum && !progress.rooms_visited.contains(vnum) {
                    progress.rooms_visited.insert(vnum.clone());
                    matched = true;
                    dirty = true;
                    let line = format!("[ {}: visited {} ]", quest.name, vnum);
                    notify(connections, char_name, &line);
                }
            }
        }
        if matched && has_only_visit_objectives(&quest) && is_completable_owned(db, &ch, &quest) {
            auto_complete.push(quest.vnum.clone());
        }
    }
    if dirty {
        let _ = db.save_character_data(ch);
    }
    for vnum in auto_complete {
        try_complete(db, connections, state, char_name, &vnum);
    }
    dirty
}

fn has_only_visit_objectives(quest: &QuestData) -> bool {
    !quest.objectives.is_empty()
        && quest
            .objectives
            .iter()
            .all(|o| matches!(o, QuestObjective::VisitRoom { .. }))
}

/// DG-var-set listener entry point. Called from `src/script/dg/eval.rs` after
/// a character-scoped DG var write. Advances any `DgFlag` objective whose
/// (var, value) matches; auto-completes flag-only quests.
pub fn handle_dg_flag_set(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    var: &str,
    value: &str,
) -> bool {
    if char_name.is_empty() || var.is_empty() {
        return false;
    }
    let mut ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return false,
    };
    let mut dirty = false;
    let mut auto_complete: Vec<String> = Vec::new();
    let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    for qvnum in &active_keys {
        let quest = match db.get_quest_data(qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        let progress = match ch.active_quests.get_mut(qvnum) {
            Some(p) => p,
            None => continue,
        };
        let mut matched = false;
        for obj in &quest.objectives {
            if let QuestObjective::DgFlag { var: ovar, value: oval } = obj {
                if ovar == var && oval == value && !progress.flags_set.contains(ovar) {
                    progress.flags_set.insert(ovar.clone());
                    matched = true;
                    dirty = true;
                    let line = format!("[ {}: flag {}={} set ]", quest.name, ovar, oval);
                    notify(connections, char_name, &line);
                }
            }
        }
        if matched && has_only_flag_objectives(&quest) && is_completable_owned(db, &ch, &quest) {
            auto_complete.push(quest.vnum.clone());
        }
    }
    if dirty {
        let _ = db.save_character_data(ch);
    }
    for vnum in auto_complete {
        try_complete(db, connections, state, char_name, &vnum);
    }
    dirty
}

fn has_only_flag_objectives(quest: &QuestData) -> bool {
    !quest.objectives.is_empty()
        && quest
            .objectives
            .iter()
            .all(|o| matches!(o, QuestObjective::DgFlag { .. }))
}

/// Describe whether a viewer has a quest cue for a mob (questgiver). Returns
/// the bracketed cue line (or None when no cue applies). Used by look/examine
/// rendering to surface "(has a quest for you)" / "(awaits your return)".
pub fn describe_quest_offers(db: &Db, viewer_name: &str, mob_vnum: &str) -> Option<String> {
    if viewer_name.is_empty() || mob_vnum.is_empty() {
        return None;
    }
    let ch = db.get_character_data(&viewer_name.to_lowercase()).ok().flatten()?;
    let quests = db.find_quests_by_giver_mob_vnum(mob_vnum).ok()?;
    if quests.is_empty() {
        return None;
    }
    let mut has_completable = false;
    let mut has_offerable = false;
    for q in &quests {
        if ch.active_quests.contains_key(&q.vnum) {
            if is_completable(db, &ch, q) {
                has_completable = true;
            }
            continue;
        }
        if !ch.completed_quests.contains(&q.vnum) || q.repeatable {
            // Slice 3 gates: if a prereq exists, require it to be completed.
            // If a min skill total exists, require it.
            if let Some(prereq) = &q.prereq_quest_vnum {
                if !ch.completed_quests.contains(prereq) {
                    continue;
                }
            }
            if let Some(min_total) = q.min_player_skill_total {
                let total: i32 = ch.skills.values().map(|sp| sp.level).sum();
                if total < min_total {
                    continue;
                }
            }
            has_offerable = true;
        }
    }
    if has_completable {
        Some("(awaits your return)".to_string())
    } else if has_offerable {
        Some("(has a quest for you)".to_string())
    } else {
        None
    }
}

fn has_all_returnto_bringitem(quest: &QuestData) -> bool {
    !quest.objectives.is_empty()
        && quest.objectives.iter().all(|o| match o {
            QuestObjective::BringItem {
                return_to_mob_vnum, ..
            } => return_to_mob_vnum.is_some(),
            _ => false,
        })
}

/// Build a player-facing progress summary for the `quests` command. One entry
/// per active quest. Each entry includes objective lines.
pub fn format_progress(db: &Db, char_name: &str) -> Vec<QuestProgressView> {
    let ch = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(c)) => c,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut keys: Vec<String> = ch.active_quests.keys().cloned().collect();
    keys.sort();
    for qvnum in keys {
        let quest = match db.get_quest_data(&qvnum) {
            Ok(Some(q)) => q,
            _ => continue,
        };
        let progress = match ch.active_quests.get(&qvnum) {
            Some(p) => p,
            None => continue,
        };
        let mut lines = Vec::new();
        for obj in &quest.objectives {
            lines.push(format_objective_line(db, &ch, progress, obj));
        }
        out.push(QuestProgressView {
            vnum: quest.vnum,
            name: quest.name,
            summary: quest.summary,
            objective_lines: lines,
        });
    }
    out
}

pub struct QuestProgressView {
    pub vnum: String,
    pub name: String,
    pub summary: String,
    pub objective_lines: Vec<String>,
}

fn format_objective_line(db: &Db, ch: &CharacterData, progress: &ActiveQuest, obj: &QuestObjective) -> String {
    match obj {
        QuestObjective::KillMob { vnum, count } => {
            let cur = progress.kill_progress.get(vnum).copied().unwrap_or(0);
            let label = db
                .get_mobile_by_vnum(vnum)
                .ok()
                .flatten()
                .map(|m| m.short_desc)
                .unwrap_or_else(|| format!("mob {}", vnum));
            format!("  Slay {}: {}/{}", label, cur, count)
        }
        QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } => {
            let label = db
                .get_item_by_vnum(vnum)
                .ok()
                .flatten()
                .map(|i| i.short_desc)
                .unwrap_or_else(|| format!("item {}", vnum));
            let cur = if return_to_mob_vnum.is_some() {
                progress.item_progress.get(vnum).copied().unwrap_or(0)
            } else {
                count_inventory_vnum(db, &ch.name, vnum).min(*qty)
            };
            let suffix = match return_to_mob_vnum {
                Some(mv) => {
                    let mname = db
                        .get_mobile_by_vnum(mv)
                        .ok()
                        .flatten()
                        .map(|m| m.short_desc)
                        .unwrap_or_else(|| format!("mob {}", mv));
                    format!(" (deliver to {})", mname)
                }
                None => String::new(),
            };
            format!("  Collect {}: {}/{}{}", label, cur, qty, suffix)
        }
        QuestObjective::VisitRoom { vnum } => {
            let visited = progress.rooms_visited.contains(vnum);
            format!("  Visit room {}: {}", vnum, if visited { "✓" } else { "·" })
        }
        QuestObjective::DgFlag { var, value } => {
            let set = progress.flags_set.contains(var);
            format!(
                "  Set flag {}={}: {}",
                var,
                value,
                if set { "✓" } else { "·" }
            )
        }
    }
}
