//! Quest engine core. Lifecycle helpers (`offer`, `abandon`, `try_complete`)
//! and listener entry points (`handle_mob_kill`, `handle_item_to_mob`) called
//! from combat / give paths. Reward dispatch reuses existing primitives:
//! achievements, recipes, gold, item spawning, skill XP — quests don't ship a
//! parallel grant pipeline.
//!
//! The quest *prototypes* live in the `quests` sled tree (`db.list_all_quests`,
//! etc.); per-player state lives on `CharacterData.active_quests` /
//! `completed_quests` and rides the existing character save path.

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::types::{
    ActiveQuest, CharacterData, ItemData, ItemLocation, MobileData, QuestData, QuestObjective, QuestReward,
    kill_any_key,
};

/// Send a single line to the named character if they're online. No-op if
/// they're offline (quest progress just persists silently).
fn notify(connections: &SharedConnections, char_name: &str, line: &str) {
    {
        let conns = connections.lock();
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

/// Change a character session-first and return what `f` produced.
///
/// An online player's session copy is authoritative: the regen tick flushes
/// it over the DB, so a DB-only load→mutate→save here is silently undone a few
/// seconds later (which is what used to happen to every quest offer, kill
/// credit and completion for a logged-in player). `f` returns
/// `(changed, output)`; the character is saved only when `changed`.
///
/// `f` runs under the Connections lock: no `notify`, no World lock, no call
/// back into anything that writes the character. DB reads are fine.
fn with_character<R>(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    f: impl FnOnce(&mut CharacterData) -> (bool, R),
) -> Option<R> {
    let mut out = None;
    crate::script::achievements::apply_to_character(db, connections, char_name, |ch| {
        let (changed, r) = f(ch);
        out = Some(r);
        changed
    })?;
    out
}

fn notify_all(connections: &SharedConnections, char_name: &str, lines: &[String]) {
    for line in lines {
        notify(connections, char_name, line);
    }
}

/// Walk an online player's active quests and drop any whose
/// duration_secs has elapsed since started_at. Sends a "[ Quest expired:
/// X ]" line per drop. Saves once at the end if anything changed.
pub fn expire_quests_for(db: &Db, connections: &SharedConnections, char_name: &str, now: i64) {
    let expired = with_character(db, connections, char_name, |ch| {
        let mut names = Vec::new();
        let keys: Vec<String> = ch.active_quests.keys().cloned().collect();
        for qvnum in keys {
            let Ok(Some(quest)) = db.get_quest_data(&qvnum) else {
                continue;
            };
            let duration = match quest.duration_secs {
                Some(d) if d > 0 => d,
                _ => continue,
            };
            let started = ch.active_quests.get(&qvnum).map(|aq| aq.started_at).unwrap_or(0);
            if now.saturating_sub(started) >= duration {
                ch.active_quests.remove(&qvnum);
                names.push(quest.name);
            }
        }
        (!names.is_empty(), names)
    })
    .unwrap_or_default();
    for name in expired {
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
pub fn offer(db: &Db, connections: &SharedConnections, char_name: &str, vnum: &str) -> String {
    let quest = match db.get_quest_data(vnum) {
        Ok(Some(q)) => q,
        _ => return format!("no such quest `{}`", vnum),
    };
    with_character(db, connections, char_name, |ch| match offer_refusal(db, ch, &quest) {
        Some(why) => (false, why),
        None => {
            let aq = ActiveQuest {
                started_at: now_secs(),
                ..Default::default()
            };
            ch.active_quests.insert(quest.vnum.clone(), aq);
            (true, String::new())
        }
    })
    .unwrap_or_else(|| "no such character".to_string())
}

/// Why `ch` can't take `quest` right now, or `None` if they can.
fn offer_refusal(db: &Db, ch: &CharacterData, quest: &QuestData) -> Option<String> {
    if ch.active_quests.contains_key(&quest.vnum) {
        return Some(format!("You are already on quest `{}`.", quest.name));
    }
    if ch.completed_quests.contains(&quest.vnum) && !quest.repeatable {
        return Some(format!("You have already completed `{}`.", quest.name));
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
            return Some(format!("You must first complete `{}`.", pname));
        }
    }
    if let Some(min_total) = quest.min_player_skill_total {
        let total: i32 = ch.skills.values().map(|sp| sp.level).sum();
        if total < min_total {
            return Some(format!("You're not skilled enough yet ({} / {}).", total, min_total));
        }
    }
    if let Some(set) = &quest.achievement_set_prereq {
        if !set.is_satisfied(&ch.achievements_unlocked) {
            return Some(format!(
                "You haven't proven enough yet ({} / {} pieces in place).",
                set.unlocked_count(&ch.achievements_unlocked),
                set.min_count,
            ));
        }
    }
    if let Some(rep) = &quest.reputation_prereq {
        if !rep.is_satisfied(&ch.reputation) {
            return Some(format!(
                "They don't trust you enough for that yet ({}).",
                crate::reputation::LADDER.standing_line(crate::reputation::standing(&ch.reputation, &rep.faction)),
            ));
        }
    }
    None
}

/// Abandon an active quest. Returns "" on success, error string otherwise.
pub fn abandon(db: &Db, connections: &SharedConnections, char_name: &str, vnum: &str) -> String {
    with_character(db, connections, char_name, |ch| {
        if ch.active_quests.remove(vnum).is_none() {
            (false, format!("you are not on quest `{}`", vnum))
        } else {
            (true, String::new())
        }
    })
    .unwrap_or_else(|| "no such character".to_string())
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
        QuestObjective::KillMob { vnum, count } => progress.kill_progress.get(vnum).copied().unwrap_or(0) >= *count,
        QuestObjective::KillAnyMob { vnums, count } => {
            let key = kill_any_key(vnums);
            progress.kill_any_progress.get(&key).copied().unwrap_or(0) >= *count
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
    // Claim the completion first: the active->completed move happens in one
    // session-first write, so two racing turn-ins can't both pay out.
    // Rewards that touch the character load it again through the same path.
    // Completability (which may scan inventory) is judged on a snapshot,
    // outside the lock; the claim only re-checks the quest is still active.
    let Some(snapshot) = with_character(db, connections, char_name, |ch| (false, ch.clone())) else {
        return false;
    };
    if !is_completable(db, &snapshot, &quest) {
        return false;
    }
    let claimed = with_character(db, connections, char_name, |ch| {
        if !ch.active_quests.contains_key(&quest.vnum) {
            return (false, None);
        }
        // Per-quest choice vars (set by `DialogueEffect::SetQuestChoice`) are
        // read before the active quest is removed — reward variants like
        // `EmbraceAnarch` consume them.
        let choice_vars = ch
            .active_quests
            .remove(&quest.vnum)
            .map(|aq| aq.choice_vars)
            .unwrap_or_default();
        ch.completed_quests.insert(quest.vnum.clone());
        (true, Some((ch.name.clone(), choice_vars)))
    })
    .flatten();
    let Some((name, choice_vars)) = claimed else {
        return false;
    };

    // BringItem objectives WITHOUT return_to_mob_vnum are consumed from
    // inventory now (turn-in completion via dialogue effect).
    for obj in &quest.objectives {
        if let QuestObjective::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } = obj
        {
            if return_to_mob_vnum.is_none() {
                consume_inventory_items(db, &name, vnum, *qty);
            }
        }
    }

    if !quest.completion_text.is_empty() {
        notify(connections, char_name, &quest.completion_text);
    }

    // Resolve the giver mob's display name once for reward variants that
    // need it (e.g. EmbraceClan stamps the sire). Falls back to None if the
    // quest has no canonical giver or the prototype isn't loaded.
    let giver_name = quest
        .giver_mob_vnum
        .as_ref()
        .and_then(|vn| db.get_mobile_by_vnum(vn).ok().flatten())
        .map(|m| m.name);

    // Apply rewards.
    for reward in &quest.rewards {
        apply_reward(
            db,
            connections,
            state,
            &name,
            reward,
            giver_name.as_deref(),
            &choice_vars,
        );
    }

    // After the rewards, which each load and save the character independently
    // — bumping first would be overwritten by the next reward's save.
    // Counts turn-ins rather than distinct quests: a repeatable quest is meant
    // to be repeated, and `completed_quests` already records the distinct set.
    crate::script::achievements::notify_counter_core(db, connections, state, &name, "quests.completed", 1);

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
    giver_name: Option<&str>,
    choice_vars: &std::collections::HashMap<String, String>,
) {
    match reward {
        QuestReward::Gold { amount } => {
            // Through the purse: session-first, and it keeps the gold
            // counters and high-water marks the other credit paths keep.
            if crate::purse::add_gold(db, connections, state, char_name, *amount) {
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
            // Was a hand-rolled flat-100-per-level loop that also skipped the
            // `skill_reached` achievement hook entirely — so a quest reward
            // could push a skill past an achievement threshold without ever
            // unlocking it. Both are fixed by routing through the chokepoint,
            // which also emits the player-facing feedback.
            crate::progress::award_xp(
                db,
                connections,
                state,
                char_name,
                skill,
                *amount,
                crate::progress::XpSource::Quest,
            );
        }
        QuestReward::Achievement { key } => {
            crate::script::achievements::award_core(db, connections, state, char_name, key, true);
        }
        QuestReward::Morality { delta } => {
            if let Some((_, after)) = crate::morality::apply_delta(db, connections, char_name, *delta) {
                // apply_delta already sent the tier line when one was crossed;
                // this is the receipt that the reward landed at all.
                notify(
                    connections,
                    char_name,
                    &format!(
                        "[ Morality {}{} \u{2192} {} ]",
                        if *delta > 0 { "+" } else { "" },
                        delta,
                        after
                    ),
                );
            }
        }
        QuestReward::Reputation { faction, delta } => {
            // Through the IO chokepoint: this runs outside any save of the
            // character, and `apply_delta` handles opposition, the session
            // sync and the band announcement in one place.
            let moved = crate::reputation::apply_delta(db, connections, state, char_name, faction, *delta);
            for (f, before, after) in &moved {
                let d = after - before;
                let label = crate::reputation::definition(state, f);
                notify(
                    connections,
                    char_name,
                    &format!(
                        "[ {} {}{} \u{2192} {} ]",
                        label.display(),
                        if d > 0 { "+" } else { "" },
                        d,
                        after
                    ),
                );
            }
        }
        QuestReward::LearnRecipe { recipe_id } => {
            let learned = with_character(db, connections, char_name, |ch| {
                let new = ch.learned_recipes.insert(recipe_id.clone());
                (new, new)
            })
            .unwrap_or(false);
            if learned {
                notify(
                    connections,
                    char_name,
                    &format!("[ You have learned a new recipe: {} ]", recipe_id),
                );
            }
        }
        QuestReward::EmbraceClan { clan } => {
            let sire = giver_name.map(str::to_string);
            let embraced = with_character(db, connections, char_name, |ch| {
                if ch.vampire_state.is_none() {
                    tracing::warn!(
                        char = %char_name,
                        clan = %clan,
                        "EmbraceClan reward fired on a mortal — no vampire state to upgrade",
                    );
                    return (false, false);
                }
                if crate::script::vampire::pc_clan_from_traits(ch).is_some() {
                    tracing::warn!(
                        char = %char_name,
                        clan = %clan,
                        "EmbraceClan reward fired on already-acknowledged kindred — skipping",
                    );
                    return (false, false);
                }
                let ok = crate::script::vampire::apply_clan_acknowledgment(ch, clan, sire);
                (ok, ok)
            })
            .unwrap_or(false);
            if embraced {
                notify(
                    connections,
                    char_name,
                    &format!(
                        "\x1b[1;31m[ Your blood thickens. You are kindred of clan {}. ]\x1b[0m",
                        clan
                    ),
                );
            }
        }
        QuestReward::EmbraceAnarch { discipline } => {
            // Resolve the discipline: hardcoded > runtime choice_var > bail.
            let resolved = discipline
                .as_ref()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    choice_vars
                        .get("discipline")
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                });
            let Some(resolved) = resolved else {
                tracing::warn!(
                    char = %char_name,
                    "EmbraceAnarch reward fired without a discipline (neither hardcoded nor choice_vars[\"discipline\"]) — skipping",
                );
                return;
            };
            // Validate against the canonical discipline allow-list.
            if !crate::script::vampire::known_disciplines()
                .iter()
                .any(|d| d == &resolved)
            {
                tracing::warn!(
                    char = %char_name,
                    discipline = %resolved,
                    "EmbraceAnarch reward rejected — discipline not in known set",
                );
                return;
            }
            let embraced = with_character(db, connections, char_name, |ch| {
                if ch.vampire_state.is_none() {
                    tracing::warn!(
                        char = %char_name,
                        "EmbraceAnarch reward fired on a mortal — no vampire state to upgrade",
                    );
                    return (false, false);
                }
                if crate::script::vampire::pc_clan_from_traits(ch).is_some() {
                    tracing::warn!(
                        char = %char_name,
                        "EmbraceAnarch reward fired on already-acknowledged kindred — skipping",
                    );
                    return (false, false);
                }
                if ch.traits.iter().any(|t| t == crate::script::vampire::ANARCH_TRAIT) {
                    tracing::warn!(
                        char = %char_name,
                        "EmbraceAnarch reward fired on a character already walking the Anarch path — skipping",
                    );
                    return (false, false);
                }
                let ok = crate::script::vampire::apply_anarch_acknowledgment(ch, &resolved);
                (ok, ok)
            })
            .unwrap_or(false);
            if embraced {
                notify(
                    connections,
                    char_name,
                    &format!(
                        "\x1b[1;31m[ Your blood thickens. You walk the Anarch road — your gift is {}. ]\x1b[0m",
                        resolved
                    ),
                );
            }
        }
    }
}

/// Mob-death listener entry point. Increments `kill_progress` for every
/// active quest whose `KillMob` objective matches `mob_proto_vnum`, for every
/// name in `participants`. Sends a progress line, saves, and auto-completes
/// kill-only quests.
///
/// Takes the recipient list rather than deriving it. This function used to
/// build the set itself from `damaged_by` plus the killer, which made quests
/// the only kill consumer that credited a party — the counter, worship,
/// morality and reputation all took the killing blow alone. The set now comes
/// from [`crate::group::kill_credit`] so every consumer agrees on who took
/// part, and this one is no longer the odd one out.
pub fn handle_mob_kill(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    participants: &[String],
    mob_proto_vnum: &str,
) {
    if mob_proto_vnum.is_empty() {
        return;
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for name in participants {
        let lower = name.to_lowercase();
        if !seen.insert(lower.clone()) {
            continue;
        }
        credit_one_kill(db, connections, state, &lower, mob_proto_vnum);
    }
}

fn credit_one_kill(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    mob_proto_vnum: &str,
) {
    let (lines, auto_complete) = with_character(db, connections, char_name, |ch| {
        let mut lines = Vec::new();
        let mut auto_complete = Vec::new();
        let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
        for qvnum in &active_keys {
            let Ok(Some(quest)) = db.get_quest_data(qvnum) else {
                continue;
            };
            let Some(progress) = ch.active_quests.get_mut(qvnum) else {
                continue;
            };
            for obj in &quest.objectives {
                match obj {
                    QuestObjective::KillMob { vnum, count } if vnum == mob_proto_vnum => {
                        let entry = progress.kill_progress.entry(vnum.clone()).or_insert(0);
                        if *entry < *count {
                            *entry += 1;
                            lines.push(format!("[ {}: {}/{} ]", quest.name, *entry, count));
                        }
                    }
                    QuestObjective::KillAnyMob { vnums, count } if vnums.iter().any(|v| v == mob_proto_vnum) => {
                        let entry = progress.kill_any_progress.entry(kill_any_key(vnums)).or_insert(0);
                        if *entry < *count {
                            *entry += 1;
                            lines.push(format!("[ {}: {}/{} ]", quest.name, *entry, count));
                        }
                    }
                    _ => {}
                }
            }
            if has_only_kill_objectives(&quest) && is_completable_owned(db, ch, &quest) {
                auto_complete.push(quest.vnum.clone());
            }
        }
        (!lines.is_empty(), (lines, auto_complete))
    })
    .unwrap_or_default();
    notify_all(connections, char_name, &lines);
    for vnum in auto_complete {
        try_complete(db, connections, state, char_name, &vnum);
    }
}

fn has_only_kill_objectives(quest: &QuestData) -> bool {
    !quest.objectives.is_empty()
        && quest
            .objectives
            .iter()
            .all(|o| matches!(o, QuestObjective::KillMob { .. } | QuestObjective::KillAnyMob { .. }))
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

    // Advance the FIRST active quest with an open BringItem objective for
    // this (item_vnum, mob_vnum) pair.
    let advanced = with_character(db, connections, giver_name, |ch| {
        let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
        for qvnum in &active_keys {
            let Ok(Some(quest)) = db.get_quest_data(qvnum) else {
                continue;
            };
            for obj in &quest.objectives {
                let QuestObjective::BringItem {
                    vnum,
                    qty,
                    return_to_mob_vnum,
                } = obj
                else {
                    continue;
                };
                if vnum != item_vnum || return_to_mob_vnum.as_deref() != Some(mob_vnum.as_str()) {
                    continue;
                }
                let Some(progress) = ch.active_quests.get_mut(qvnum) else {
                    break;
                };
                let entry = progress.item_progress.entry(vnum.clone()).or_insert(0);
                if *entry < *qty {
                    *entry += 1;
                    let (count, target) = (*entry, *qty);
                    let done = has_all_returnto_bringitem(&quest) && is_completable_owned(db, ch, &quest);
                    return (true, Some((quest, count, target, done)));
                }
                break;
            }
        }
        (false, None)
    })
    .flatten();
    let Some((quest, new_count, target_qty, done)) = advanced else {
        return false;
    };

    // Consume the item from the world (the give.rhai caller would otherwise
    // place it on the mob; we shortcut that).
    let _ = db.delete_item(&item.id);
    let line = format!("[ {}: {} ({}/{}) ]", quest.name, item.short_desc, new_count, target_qty);
    notify(connections, giver_name, &line);

    // If this was the LAST item in a turn-in-only quest, auto-complete.
    if done {
        try_complete(db, connections, state, giver_name, &quest.vnum);
    }
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
    let (lines, auto_complete) = with_character(db, connections, char_name, |ch| {
        let mut lines = Vec::new();
        let mut auto_complete = Vec::new();
        let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
        for qvnum in &active_keys {
            let Ok(Some(quest)) = db.get_quest_data(qvnum) else {
                continue;
            };
            let Some(progress) = ch.active_quests.get_mut(qvnum) else {
                continue;
            };
            let mut matched = false;
            for obj in &quest.objectives {
                if let QuestObjective::VisitRoom { vnum } = obj {
                    if vnum == room_vnum && progress.rooms_visited.insert(vnum.clone()) {
                        matched = true;
                        lines.push(format!("[ {}: visited {} ]", quest.name, vnum));
                    }
                }
            }
            if matched && has_only_visit_objectives(&quest) && is_completable_owned(db, ch, &quest) {
                auto_complete.push(quest.vnum.clone());
            }
        }
        (!lines.is_empty(), (lines, auto_complete))
    })
    .unwrap_or_default();
    notify_all(connections, char_name, &lines);
    let dirty = !lines.is_empty();
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

/// DG-var-set listener entry point. Called from `src/script/dg/eval.rs`
/// (`set_entity_var`) after a character-scoped DG var write, and from the
/// dialogue `SetDgVar` effect. Advances any `DgFlag` objective whose
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
    let (lines, auto_complete) = with_character(db, connections, char_name, |ch| {
        let mut lines = Vec::new();
        let mut auto_complete = Vec::new();
        let active_keys: Vec<String> = ch.active_quests.keys().cloned().collect();
        for qvnum in &active_keys {
            let Ok(Some(quest)) = db.get_quest_data(qvnum) else {
                continue;
            };
            let Some(progress) = ch.active_quests.get_mut(qvnum) else {
                continue;
            };
            let mut matched = false;
            for obj in &quest.objectives {
                if let QuestObjective::DgFlag { var: ovar, value: oval } = obj {
                    if ovar == var && oval == value && progress.flags_set.insert(ovar.clone()) {
                        matched = true;
                        lines.push(format!("[ {}: flag {}={} set ]", quest.name, ovar, oval));
                    }
                }
            }
            if matched && has_only_flag_objectives(&quest) && is_completable_owned(db, ch, &quest) {
                auto_complete.push(quest.vnum.clone());
            }
        }
        (!lines.is_empty(), (lines, auto_complete))
    })
    .unwrap_or_default();
    notify_all(connections, char_name, &lines);
    let dirty = !lines.is_empty();
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
            if let Some(set) = &q.achievement_set_prereq {
                if !set.is_satisfied(&ch.achievements_unlocked) {
                    continue;
                }
            }
            if let Some(rep) = &q.reputation_prereq {
                if !rep.is_satisfied(&ch.reputation) {
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
            QuestObjective::BringItem { return_to_mob_vnum, .. } => return_to_mob_vnum.is_some(),
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
        QuestObjective::KillAnyMob { vnums, count } => {
            let key = kill_any_key(vnums);
            let cur = progress.kill_any_progress.get(&key).copied().unwrap_or(0);
            let mut labels: Vec<String> = vnums
                .iter()
                .map(|v| {
                    db.get_mobile_by_vnum(v)
                        .ok()
                        .flatten()
                        .map(|m| m.short_desc)
                        .unwrap_or_else(|| format!("mob {}", v))
                })
                .collect();
            labels.sort();
            labels.dedup();
            let joined = labels.join(" / ");
            format!("  Slay any of [{}]: {}/{}", joined, cur, count)
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
            format!("  Set flag {}={}: {}", var, value, if set { "✓" } else { "·" })
        }
    }
}
