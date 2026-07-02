// src/script/worship.rs
// God-worship capability chokepoint. Every worship state change — pact
// creation, tribute, blessings, favor, smites, faith offenses — routes
// through these core fns so the one-god-at-a-time invariant, buff source
// tagging, and session/DB sync hold uniformly. Exposed to BOTH Rhai command
// scripts and DG trigger scripts (see src/script/dg/cmds.rs); per-god config
// lives on the builder layer in `MobileData.deity: Option<DeityConfig>`.
//
// Buff source conventions:
//   "worship:<god_vnum>" — blessings (stripped when the god turns on you)
//   "wrath:<god_vnum>"   — punishments (lifted only by atonement)

use crate::db::Db;
use crate::{ActiveBuff, CharacterData, DeityConfig, EffectType, GodRank, MobileData, SharedConnections, WorshipState};
use rhai::Engine;
use std::sync::Arc;
use uuid::Uuid;

/// One game day in real seconds (2 real minutes per game hour * 24).
pub const GAME_DAY_SECS: i32 = 48 * 60;

/// Favor earned for slaying an NPC sworn to an enemy god.
pub const FAVOR_PER_ENEMY_MINION: i32 = 5;
/// Favor earned for a PvP kill of an enemy god's worshiper.
pub const FAVOR_PER_ENEMY_WORSHIPER: i32 = 25;
/// PvP kill credits older than this many game days are pruned.
const PVP_CREDIT_RETENTION_DAYS: i64 = 30;
/// Minimum total gold (on-hand + bank) before a god accepts atonement.
pub const ATONEMENT_GOLD_FLOOR: i64 = 100;

// === Core lookups ===

/// Current absolute game day (monotonic since year 1).
pub fn current_absolute_day(db: &Db) -> i64 {
    db.get_game_time().map(|t| t.absolute_day()).unwrap_or(0)
}

/// Deity config for a god vnum, read from the prototype (source of truth).
pub fn deity_config_by_vnum(db: &Db, god_vnum: &str) -> Option<DeityConfig> {
    if god_vnum.is_empty() {
        return None;
    }
    db.get_mobile_by_vnum(god_vnum).ok()??.deity
}

/// Display name of the god's mobile ("" if the vnum resolves to nothing).
pub fn god_display_name(db: &Db, god_vnum: &str) -> String {
    db.get_mobile_by_vnum(god_vnum)
        .ok()
        .flatten()
        .map(|m| m.name)
        .unwrap_or_default()
}

// === Character access (session authoritative for online players) ===

/// Mutate a character wherever the live copy is: the session copy for online
/// players (the regen tick flushes session -> DB, so DB-only writes get
/// clobbered), the DB row for offline ones. Persists after the mutation.
fn mutate_character<T>(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    f: impl FnOnce(&mut CharacterData) -> T,
) -> Option<T> {
    {
        let mut guard = connections.lock().unwrap();
        for (_id, session) in guard.iter_mut() {
            if let Some(ref mut ch) = session.character {
                if ch.name.eq_ignore_ascii_case(char_name) {
                    let out = f(ch);
                    let _ = db.save_character_data(ch.clone());
                    return Some(out);
                }
            }
        }
    }
    let mut ch = db.get_character_data(&char_name.to_lowercase()).ok()??;
    let out = f(&mut ch);
    db.save_character_data(ch).ok()?;
    Some(out)
}

/// Read-only character snapshot (session copy preferred).
fn read_character(db: &Db, connections: &SharedConnections, char_name: &str) -> Option<CharacterData> {
    {
        let guard = connections.lock().unwrap();
        for (_id, session) in guard.iter() {
            if let Some(ref ch) = session.character {
                if ch.name.eq_ignore_ascii_case(char_name) {
                    return Some(ch.clone());
                }
            }
        }
    }
    db.get_character_data(&char_name.to_lowercase()).ok()?
}

/// Send a line to an online player by name (no-op if offline).
pub fn send_to_player(connections: &SharedConnections, char_name: &str, msg: &str) {
    let guard = connections.lock().unwrap();
    for (_id, session) in guard.iter() {
        if let Some(ref ch) = session.character {
            if ch.name.eq_ignore_ascii_case(char_name) {
                let _ = session.sender.send(format!("\n{}\n", msg));
                return;
            }
        }
    }
}

// === Buff plumbing ===

/// Push or refresh a buff matched by (effect_type, source). Magnitude
/// collapses via max; a permanent duration (-1) always wins.
fn stamp_buff(buffs: &mut Vec<ActiveBuff>, effect: EffectType, magnitude: i32, duration_secs: i32, source: &str) {
    if let Some(existing) = buffs.iter_mut().find(|b| b.effect_type == effect && b.source == source) {
        existing.magnitude = existing.magnitude.max(magnitude);
        existing.remaining_secs = if existing.remaining_secs == -1 || duration_secs == -1 {
            -1
        } else {
            existing.remaining_secs.max(duration_secs)
        };
    } else {
        buffs.push(ActiveBuff {
            effect_type: effect,
            magnitude,
            remaining_secs: duration_secs,
            source: source.to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
    }
}

fn blessing_source(god_vnum: &str) -> String {
    format!("worship:{}", god_vnum)
}

fn wrath_source(god_vnum: &str) -> String {
    format!("wrath:{}", god_vnum)
}

// === Capabilities ===

/// Form a pact with a god. Gates: not already worshiping, the vnum is a
/// rank-`God` deity, and the character holds a pact artifact (consumed) or
/// has completed a pact quest. A god with no gates configured accepts anyone.
pub fn create_worship_pact(db: &Db, connections: &SharedConnections, char_name: &str, god_vnum: &str) -> String {
    let config = match deity_config_by_vnum(db, god_vnum) {
        Some(c) => c,
        None => return "not_a_god".to_string(),
    };
    if config.rank != GodRank::God {
        return "not_a_god".to_string();
    }
    let character = match read_character(db, connections, char_name) {
        Some(c) => c,
        None => return "not_found".to_string(),
    };
    if character.worship.is_some() {
        return "already_worshiping".to_string();
    }

    let gated = !config.pact_item_vnums.is_empty() || !config.pact_quest_ids.is_empty();
    let mut consume_item: Option<Uuid> = None;
    if gated {
        let quest_ok = config
            .pact_quest_ids
            .iter()
            .any(|q| character.completed_quests.contains(q));
        if !quest_ok {
            if let Ok(inv) = db.get_items_in_inventory(char_name) {
                consume_item = inv
                    .iter()
                    .find(|i| {
                        !i.is_prototype
                            && i.vnum
                                .as_deref()
                                .map(|v| config.pact_item_vnums.iter().any(|p| p == v))
                                .unwrap_or(false)
                    })
                    .map(|i| i.id);
            }
            if consume_item.is_none() {
                return "gate_failed".to_string();
            }
        }
    }
    if let Some(item_id) = consume_item {
        let _ = db.delete_item_recursive(&item_id);
    }

    let today = current_absolute_day(db);
    let stamped = mutate_character(db, connections, char_name, |ch| {
        if ch.worship.is_some() {
            return false;
        }
        ch.worship = Some(WorshipState::new(god_vnum, today));
        true
    });
    match stamped {
        Some(true) => "ok".to_string(),
        Some(false) => "already_worshiping".to_string(),
        None => "not_found".to_string(),
    }
}

/// Admin/testing pact set: validates only that the vnum is a rank-`God`
/// deity, bypassing the artifact/quest gates. Replaces any existing pact.
pub fn force_worship_pact(db: &Db, connections: &SharedConnections, char_name: &str, god_vnum: &str) -> String {
    let config = match deity_config_by_vnum(db, god_vnum) {
        Some(c) => c,
        None => return "not_a_god".to_string(),
    };
    if config.rank != GodRank::God {
        return "not_a_god".to_string();
    }
    let today = current_absolute_day(db);
    let stamped = mutate_character(db, connections, char_name, |ch| {
        if let Some(ref w) = ch.worship {
            let src = blessing_source(&w.god_vnum);
            ch.active_buffs.retain(|b| b.source != src);
        }
        ch.worship = Some(WorshipState::new(god_vnum, today));
    });
    match stamped {
        Some(_) => "ok".to_string(),
        None => "not_found".to_string(),
    }
}

/// Remove a character's pact entirely (admin/testing). Strips blessings but
/// leaves wrath afflictions — the god's parting gift.
pub fn clear_worship(db: &Db, connections: &SharedConnections, char_name: &str) -> bool {
    mutate_character(db, connections, char_name, |ch| {
        let had = ch.worship.is_some();
        if let Some(ref w) = ch.worship {
            let src = blessing_source(&w.god_vnum);
            ch.active_buffs.retain(|b| b.source != src);
        }
        ch.worship = None;
        had
    })
    .unwrap_or(false)
}

/// Stamp the god's blessing buffs on a worshiper. Duration = one tribute
/// interval, so blessings lapse naturally without prayer.
pub fn apply_worship_blessing(db: &Db, connections: &SharedConnections, char_name: &str) -> String {
    let character = match read_character(db, connections, char_name) {
        Some(c) => c,
        None => return "not_found".to_string(),
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return "no_pact".to_string(),
    };
    let config = match deity_config_by_vnum(db, &god_vnum) {
        Some(c) => c,
        None => return "no_god".to_string(),
    };
    if config.blessing_effects.is_empty() {
        return "no_blessings".to_string();
    }
    let duration = config.tribute_interval_days.max(1) * GAME_DAY_SECS;
    let source = blessing_source(&god_vnum);
    mutate_character(db, connections, char_name, |ch| {
        for grant in &config.blessing_effects {
            stamp_buff(&mut ch.active_buffs, grant.effect, grant.magnitude, duration, &source);
        }
    });
    "blessed".to_string()
}

/// Take the default gold tribute: `tribute_gold_percent` of total wealth
/// (on-hand + bank), deducted on-hand first. Marks tribute paid and resets
/// the anger ladder. Returns the amount taken, or -1 if the character has
/// no pact / doesn't exist.
pub fn take_tribute_gold_percent(db: &Db, connections: &SharedConnections, char_name: &str) -> i64 {
    let character = match read_character(db, connections, char_name) {
        Some(c) => c,
        None => return -1,
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return -1,
    };
    let percent = deity_config_by_vnum(db, &god_vnum)
        .map(|c| c.tribute_gold_percent)
        .unwrap_or(5)
        .clamp(0, 100) as i64;
    let today = current_absolute_day(db);
    mutate_character(db, connections, char_name, |ch| {
        let total = ch.gold as i64 + ch.bank_gold;
        let cost = total * percent / 100;
        deduct_gold(ch, cost);
        if let Some(ref mut w) = ch.worship {
            w.last_tribute_day = today;
            w.anger_stage = 0;
        }
        cost
    })
    .unwrap_or(-1)
}

/// Mark tribute paid without touching gold. This is how builder DG scripts
/// implement exotic tribute (blood, mob sacrifice, tasks).
pub fn record_tribute(db: &Db, connections: &SharedConnections, char_name: &str) -> bool {
    let today = current_absolute_day(db);
    mutate_character(db, connections, char_name, |ch| match ch.worship {
        Some(ref mut w) => {
            w.last_tribute_day = today;
            w.anger_stage = 0;
            true
        }
        None => false,
    })
    .unwrap_or(false)
}

/// Adjust favor (positive or negative). Returns false without a pact.
pub fn add_worship_favor(db: &Db, connections: &SharedConnections, char_name: &str, amount: i32) -> bool {
    mutate_character(db, connections, char_name, |ch| match ch.worship {
        Some(ref mut w) => {
            w.favor = w.favor.saturating_add(amount);
            true
        }
        None => false,
    })
    .unwrap_or(false)
}

/// Record that an anger-ladder stage has fired (worship tick bookkeeping).
pub fn set_worship_anger_stage(db: &Db, connections: &SharedConnections, char_name: &str, stage: i32) -> bool {
    mutate_character(db, connections, char_name, |ch| match ch.worship {
        Some(ref mut w) => {
            w.anger_stage = stage;
            true
        }
        None => false,
    })
    .unwrap_or(false)
}

/// Remove every blessing buff from the character's current god.
pub fn strip_worship_buffs(db: &Db, connections: &SharedConnections, char_name: &str) -> bool {
    mutate_character(db, connections, char_name, |ch| {
        let src = match ch.worship {
            Some(ref w) => blessing_source(&w.god_vnum),
            None => return false,
        };
        let before = ch.active_buffs.len();
        ch.active_buffs.retain(|b| b.source != src);
        before != ch.active_buffs.len()
    })
    .unwrap_or(false)
}

/// Divine punishment ladder. Severity 1-4; 4 is the permanent smite and only
/// lands when the god's `allow_permanent_smite` is set (else it repeats 3).
/// Sends flavor messages itself so tick, DG, and command callers all read
/// the same wrath. Returns "cursed" | "forsaken" | "smitten" |
/// "smitten_permanent" | "no_pact" | "not_found".
pub fn smite_worshiper(db: &Db, connections: &SharedConnections, char_name: &str, severity: i32) -> String {
    let character = match read_character(db, connections, char_name) {
        Some(c) => c,
        None => return "not_found".to_string(),
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return "no_pact".to_string(),
    };
    let config = deity_config_by_vnum(db, &god_vnum).unwrap_or_default();
    let god_name = {
        let n = god_display_name(db, &god_vnum);
        if n.is_empty() { "your god".to_string() } else { n }
    };
    let severity = if severity >= 4 && !config.allow_permanent_smite {
        3
    } else {
        severity.clamp(1, 4)
    };
    let wrath = wrath_source(&god_vnum);
    let blessings = blessing_source(&god_vnum);

    let outcome = mutate_character(db, connections, char_name, |ch| match severity {
        1 => {
            stamp_buff(&mut ch.active_buffs, EffectType::Curse, 1, GAME_DAY_SECS, &wrath);
            "cursed"
        }
        2 => {
            ch.active_buffs.retain(|b| b.source != blessings);
            stamp_buff(&mut ch.active_buffs, EffectType::Curse, 2, 2 * GAME_DAY_SECS, &wrath);
            "forsaken"
        }
        3 => {
            ch.active_buffs.retain(|b| b.source != blessings);
            ch.hp = (ch.hp - ch.max_hp / 4).max(1);
            stamp_buff(&mut ch.active_buffs, EffectType::Blind, 1, GAME_DAY_SECS, &wrath);
            stamp_buff(&mut ch.active_buffs, EffectType::Curse, 3, 2 * GAME_DAY_SECS, &wrath);
            "smitten"
        }
        _ => {
            ch.active_buffs.retain(|b| b.source != blessings);
            ch.hp = (ch.hp - ch.max_hp / 2).max(1);
            stamp_buff(&mut ch.active_buffs, EffectType::Blind, 1, -1, &wrath);
            stamp_buff(&mut ch.active_buffs, EffectType::Curse, 5, -1, &wrath);
            "smitten_permanent"
        }
    });
    let outcome = match outcome {
        Some(o) => o,
        None => return "not_found".to_string(),
    };
    let msg = match outcome {
        "cursed" => format!(
            "\x1b[33m{} marks your neglect. A shadow of ill luck settles over you.\x1b[0m",
            god_name
        ),
        "forsaken" => format!(
            "\x1b[1;33m{} withdraws all blessing. The curse upon you deepens.\x1b[0m",
            god_name
        ),
        "smitten" => format!(
            "\x1b[1;31m{} SMITES you! Divine fire sears your flesh and scorches your sight.\x1b[0m",
            god_name
        ),
        _ => format!(
            "\x1b[1;31m{} turns their face from you forever. Your eyes go dark. This will not heal.\x1b[0m",
            god_name
        ),
    };
    send_to_player(connections, char_name, &msg);
    outcome.to_string()
}

/// Massive atonement payment: half of total wealth, counts as tribute, lifts
/// wrath afflictions (including permanent smites) and clears the offense
/// counter. Returns gold paid, -1 if no pact, -2 if too poor for the god to
/// take the gesture seriously.
pub fn atone_worship(db: &Db, connections: &SharedConnections, char_name: &str) -> i64 {
    let character = match read_character(db, connections, char_name) {
        Some(c) => c,
        None => return -1,
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return -1,
    };
    let total = character.gold as i64 + character.bank_gold;
    if total < ATONEMENT_GOLD_FLOOR {
        return -2;
    }
    let today = current_absolute_day(db);
    let wrath = wrath_source(&god_vnum);
    mutate_character(db, connections, char_name, |ch| {
        let cost = (ch.gold as i64 + ch.bank_gold) / 2;
        deduct_gold(ch, cost);
        ch.active_buffs.retain(|b| b.source != wrath);
        if let Some(ref mut w) = ch.worship {
            w.last_tribute_day = today;
            w.anger_stage = 0;
            w.coworshiper_offenses = 0;
        }
        cost
    })
    .unwrap_or(-1)
}

/// Punish an attack on a target sworn to the attacker's own god (player
/// co-worshiper or patron mob). Escalates per offense — curse, deeper curse,
/// then a full smite every time after — but never breaks the pact.
/// Returns "" (not applicable) | "first" | "second" | "smite".
pub fn punish_faith_offense(
    db: &Db,
    connections: &SharedConnections,
    attacker_name: &str,
    target_god_vnum: &str,
) -> String {
    if target_god_vnum.is_empty() {
        return String::new();
    }
    let character = match read_character(db, connections, attacker_name) {
        Some(c) => c,
        None => return String::new(),
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return String::new(),
    };
    if god_vnum != target_god_vnum {
        return String::new();
    }
    let offenses = mutate_character(db, connections, attacker_name, |ch| match ch.worship {
        Some(ref mut w) => {
            w.coworshiper_offenses = w.coworshiper_offenses.saturating_add(1);
            w.favor = w.favor.saturating_sub(if w.coworshiper_offenses >= 2 { 15 } else { 5 });
            w.coworshiper_offenses
        }
        None => 0,
    })
    .unwrap_or(0);
    if offenses == 0 {
        return String::new();
    }
    let god_name = {
        let n = god_display_name(db, &god_vnum);
        if n.is_empty() { "your god".to_string() } else { n }
    };
    let wrath = wrath_source(&god_vnum);
    match offenses {
        1 => {
            mutate_character(db, connections, attacker_name, |ch| {
                stamp_buff(&mut ch.active_buffs, EffectType::Curse, 1, GAME_DAY_SECS, &wrath);
            });
            send_to_player(
                connections,
                attacker_name,
                &format!(
                    "\x1b[33mYou raise your hand against the faithful of {}. A cold displeasure settles on you.\x1b[0m",
                    god_name
                ),
            );
            "first".to_string()
        }
        2 => {
            mutate_character(db, connections, attacker_name, |ch| {
                stamp_buff(&mut ch.active_buffs, EffectType::Curse, 2, 2 * GAME_DAY_SECS, &wrath);
            });
            send_to_player(
                connections,
                attacker_name,
                &format!(
                    "\x1b[1;33m{} has seen this before. The curse upon you thickens.\x1b[0m",
                    god_name
                ),
            );
            "second".to_string()
        }
        _ => {
            smite_worshiper(db, connections, attacker_name, 3);
            "smite".to_string()
        }
    }
}

/// Award favor for slaying an NPC sworn to an enemy of the killer's god.
pub fn handle_npc_kill_credit(db: &Db, connections: &SharedConnections, killer_name: &str, killed_vnum: &str) {
    if killed_vnum.is_empty() {
        return;
    }
    let character = match read_character(db, connections, killer_name) {
        Some(c) => c,
        None => return,
    };
    let god_vnum = match character.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return,
    };
    let config = match deity_config_by_vnum(db, &god_vnum) {
        Some(c) => c,
        None => return,
    };
    let patron = db
        .get_mobile_by_vnum(killed_vnum)
        .ok()
        .flatten()
        .and_then(|m| m.patron_god_vnum);
    let patron = match patron {
        Some(p) if config.enemy_god_vnums.contains(&p) => p,
        _ => return,
    };
    add_worship_favor(db, connections, killer_name, FAVOR_PER_ENEMY_MINION);
    let god_name = god_display_name(db, &god_vnum);
    let enemy_name = god_display_name(db, &patron);
    send_to_player(
        connections,
        killer_name,
        &format!(
            "\x1b[1;32m{} savors the fall of {}'s servant. Your favor grows.\x1b[0m",
            if god_name.is_empty() { "Your god" } else { &god_name },
            if enemy_name.is_empty() {
                "the enemy"
            } else {
                &enemy_name
            },
        ),
    );
}

/// Award favor for a PvP kill of an enemy god's worshiper. Capped at one
/// credit per victim per game day to shut down kill-trading favor farms.
pub fn handle_pvp_kill_credit(db: &Db, connections: &SharedConnections, killer_name: &str, victim_name: &str) {
    if killer_name.eq_ignore_ascii_case(victim_name) {
        return;
    }
    let killer = match read_character(db, connections, killer_name) {
        Some(c) => c,
        None => return,
    };
    let god_vnum = match killer.worship {
        Some(ref w) => w.god_vnum.clone(),
        None => return,
    };
    let config = match deity_config_by_vnum(db, &god_vnum) {
        Some(c) => c,
        None => return,
    };
    let victim_god = match read_character(db, connections, victim_name).and_then(|v| v.worship) {
        Some(w) => w.god_vnum,
        None => return,
    };
    if !config.enemy_god_vnums.contains(&victim_god) {
        return;
    }
    let today = current_absolute_day(db);
    let victim_key = victim_name.to_lowercase();
    let credited = mutate_character(db, connections, killer_name, |ch| match ch.worship {
        Some(ref mut w) => {
            if w.pvp_credit_days.get(&victim_key) == Some(&today) {
                return false;
            }
            w.pvp_credit_days.insert(victim_key.clone(), today);
            w.pvp_credit_days.retain(|_, d| today - *d <= PVP_CREDIT_RETENTION_DAYS);
            w.favor = w.favor.saturating_add(FAVOR_PER_ENEMY_WORSHIPER);
            true
        }
        None => false,
    })
    .unwrap_or(false);
    if !credited {
        return;
    }
    let god_name = god_display_name(db, &god_vnum);
    send_to_player(
        connections,
        killer_name,
        &format!(
            "\x1b[1;32m{} exults! An enemy of the faith lies broken. Your favor swells.\x1b[0m",
            if god_name.is_empty() { "Your god" } else { &god_name },
        ),
    );
}

fn deduct_gold(ch: &mut CharacterData, mut cost: i64) {
    let from_hand = cost.min(ch.gold as i64).max(0);
    ch.gold -= from_hand as i32;
    cost -= from_hand;
    if cost > 0 {
        ch.bank_gold = (ch.bank_gold - cost).max(0);
    }
}

// === Rhai registration ===

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // MobileData deity/patron surface for scripts (examine, worship, attack).
    engine.register_get("is_deity", |m: &mut MobileData| m.deity.is_some());
    engine.register_get("deity_rank", |m: &mut MobileData| {
        m.deity
            .as_ref()
            .map(|d| d.rank.to_display_string().to_string())
            .unwrap_or_default()
    });
    engine.register_get("deity_epithet", |m: &mut MobileData| {
        m.deity.as_ref().map(|d| d.epithet.clone()).unwrap_or_default()
    });
    engine.register_get("deity_lore", |m: &mut MobileData| {
        m.deity.as_ref().map(|d| d.lore.clone()).unwrap_or_default()
    });
    engine.register_get("patron_god_vnum", |m: &mut MobileData| {
        m.patron_god_vnum.clone().unwrap_or_default()
    });

    // CharacterData worship surface.
    engine.register_get("worship_god_vnum", |c: &mut CharacterData| {
        c.worship.as_ref().map(|w| w.god_vnum.clone()).unwrap_or_default()
    });
    engine.register_get("worship_favor", |c: &mut CharacterData| {
        c.worship.as_ref().map(|w| w.favor as i64).unwrap_or(0)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn(
        "create_worship_pact",
        move |char_name: String, god_vnum: String| -> String { create_worship_pact(&d, &co, &char_name, &god_vnum) },
    );

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("clear_worship", move |char_name: String| -> bool {
        clear_worship(&d, &co, &char_name)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn(
        "force_worship_pact",
        move |char_name: String, god_vnum: String| -> String { force_worship_pact(&d, &co, &char_name, &god_vnum) },
    );

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("get_worship_god", move |char_name: String| -> String {
        read_character(&d, &co, &char_name)
            .and_then(|c| c.worship)
            .map(|w| w.god_vnum)
            .unwrap_or_default()
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("get_worship_overdue_days", move |char_name: String| -> i64 {
        let character = match read_character(&d, &co, &char_name) {
            Some(c) => c,
            None => return 0,
        };
        let w = match character.worship {
            Some(w) => w,
            None => return 0,
        };
        let interval = deity_config_by_vnum(&d, &w.god_vnum)
            .map(|c| c.tribute_interval_days)
            .unwrap_or(3);
        w.overdue_days(current_absolute_day(&d), interval)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("get_worship_favor", move |char_name: String| -> i64 {
        read_character(&d, &co, &char_name)
            .and_then(|c| c.worship)
            .map(|w| w.favor as i64)
            .unwrap_or(0)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("apply_worship_blessing", move |char_name: String| -> String {
        apply_worship_blessing(&d, &co, &char_name)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("take_tribute_gold_percent", move |char_name: String| -> i64 {
        take_tribute_gold_percent(&d, &co, &char_name)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("record_worship_tribute", move |char_name: String| -> bool {
        record_tribute(&d, &co, &char_name)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("add_worship_favor", move |char_name: String, amount: i64| -> bool {
        add_worship_favor(&d, &co, &char_name, amount as i32)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("smite_worshiper", move |char_name: String, severity: i64| -> String {
        smite_worshiper(&d, &co, &char_name, severity as i32)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("atone_worship", move |char_name: String| -> i64 {
        atone_worship(&d, &co, &char_name)
    });

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn(
        "punish_faith_offense",
        move |attacker_name: String, target_god_vnum: String| -> String {
            punish_faith_offense(&d, &co, &attacker_name, &target_god_vnum)
        },
    );

    let d = db.clone();
    let co = connections.clone();
    engine.register_fn("strip_worship_buffs", move |char_name: String| -> bool {
        strip_worship_buffs(&d, &co, &char_name)
    });

    let d = db.clone();
    engine.register_fn("get_deity_epithet", move |god_vnum: String| -> String {
        deity_config_by_vnum(&d, &god_vnum)
            .map(|c| c.epithet)
            .unwrap_or_default()
    });

    let d = db.clone();
    engine.register_fn("get_deity_lore", move |god_vnum: String| -> String {
        deity_config_by_vnum(&d, &god_vnum).map(|c| c.lore).unwrap_or_default()
    });

    let d = db.clone();
    engine.register_fn("is_worshipable_god", move |god_vnum: String| -> bool {
        deity_config_by_vnum(&d, &god_vnum)
            .map(|c| c.rank == GodRank::God)
            .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn("get_god_display_name", move |god_vnum: String| -> String {
        god_display_name(&d, &god_vnum)
    });

    // Find a worshipable god prototype by keyword (name-contains or keyword
    // prefix). Gods are no_attack mobs, so the summon-oriented world finder
    // can't be reused. Returns the god's vnum or "".
    let d = db.clone();
    engine.register_fn("find_deity_by_keyword", move |keyword: String| -> String {
        let kw = keyword.to_lowercase();
        if kw.is_empty() {
            return String::new();
        }
        let mobiles = match d.list_all_mobiles() {
            Ok(m) => m,
            Err(_) => return String::new(),
        };
        for m in mobiles {
            if !m.is_prototype || m.deity.is_none() || m.vnum.is_empty() {
                continue;
            }
            let name_hit = m.name.to_lowercase().contains(&kw);
            let kw_hit = m.keywords.iter().any(|k| k.to_lowercase().starts_with(&kw));
            if name_hit || kw_hit {
                return m.vnum.clone();
            }
        }
        String::new()
    });

    // Resolve a god vnum to the mobile UUID that trigger fires should target:
    // a live instance when one is spawned (room-scoped %echo% works there),
    // else the prototype. "" when the vnum resolves to nothing.
    let d = db.clone();
    engine.register_fn("find_god_mobile_id", move |god_vnum: String| -> String {
        if god_vnum.is_empty() {
            return String::new();
        }
        if let Ok(instances) = d.get_mobile_instances_by_vnum(&god_vnum) {
            if let Some(live) = instances.iter().find(|m| m.current_hp > 0) {
                return live.id.to_string();
            }
        }
        d.get_mobile_by_vnum(&god_vnum)
            .ok()
            .flatten()
            .map(|m| m.id.to_string())
            .unwrap_or_default()
    });

    register_builder_fns(engine, db, connections);
}

/// medit-facing deity config editing. All take the mobile UUID string.
fn register_builder_fns(engine: &mut Engine, db: Arc<Db>, _connections: SharedConnections) {
    fn with_mobile<T>(db: &Db, mobile_id: &str, f: impl FnOnce(&mut MobileData) -> T) -> Option<T> {
        let id = Uuid::parse_str(mobile_id).ok()?;
        let mut mobile = db.get_mobile_data(&id).ok()??;
        let out = f(&mut mobile);
        db.save_mobile_data(mobile).ok()?;
        Some(out)
    }

    let d = db.clone();
    engine.register_fn(
        "set_mobile_deity_rank",
        move |mobile_id: String, rank: String| -> bool {
            let rank = match GodRank::from_str(&rank) {
                Some(r) => r,
                None => return false,
            };
            with_mobile(&d, &mobile_id, |m| {
                m.deity.get_or_insert_with(DeityConfig::default).rank = rank;
            })
            .is_some()
        },
    );

    let d = db.clone();
    engine.register_fn("remove_mobile_deity", move |mobile_id: String| -> bool {
        with_mobile(&d, &mobile_id, |m| {
            let had = m.deity.is_some();
            m.deity = None;
            had
        })
        .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn("set_deity_epithet", move |mobile_id: String, text: String| -> bool {
        with_mobile(&d, &mobile_id, |m| match m.deity {
            Some(ref mut cfg) => {
                cfg.epithet = text;
                true
            }
            None => false,
        })
        .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn("set_deity_lore", move |mobile_id: String, text: String| -> bool {
        with_mobile(&d, &mobile_id, |m| match m.deity {
            Some(ref mut cfg) => {
                cfg.lore = text;
                true
            }
            None => false,
        })
        .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn(
        "set_deity_tribute_interval",
        move |mobile_id: String, days: i64| -> bool {
            with_mobile(&d, &mobile_id, |m| match m.deity {
                Some(ref mut cfg) => {
                    cfg.tribute_interval_days = (days as i32).clamp(1, 30);
                    true
                }
                None => false,
            })
            .unwrap_or(false)
        },
    );

    let d = db.clone();
    engine.register_fn("set_deity_tribute_gold", move |mobile_id: String, pct: i64| -> bool {
        with_mobile(&d, &mobile_id, |m| match m.deity {
            Some(ref mut cfg) => {
                cfg.tribute_gold_percent = (pct as i32).clamp(0, 100);
                true
            }
            None => false,
        })
        .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn(
        "set_deity_permanent_smite",
        move |mobile_id: String, enabled: bool| -> bool {
            with_mobile(&d, &mobile_id, |m| match m.deity {
                Some(ref mut cfg) => {
                    cfg.allow_permanent_smite = enabled;
                    true
                }
                None => false,
            })
            .unwrap_or(false)
        },
    );

    // list membership editing: kind is "enemy" | "pact_item" | "pact_quest"
    let d = db.clone();
    engine.register_fn(
        "deity_list_add",
        move |mobile_id: String, kind: String, value: String| -> bool {
            if value.is_empty() {
                return false;
            }
            with_mobile(&d, &mobile_id, |m| {
                let cfg = match m.deity {
                    Some(ref mut c) => c,
                    None => return false,
                };
                let list = match kind.as_str() {
                    "enemy" => &mut cfg.enemy_god_vnums,
                    "pact_item" => &mut cfg.pact_item_vnums,
                    "pact_quest" => &mut cfg.pact_quest_ids,
                    _ => return false,
                };
                if !list.contains(&value) {
                    list.push(value);
                }
                true
            })
            .unwrap_or(false)
        },
    );

    let d = db.clone();
    engine.register_fn(
        "deity_list_remove",
        move |mobile_id: String, kind: String, value: String| -> bool {
            with_mobile(&d, &mobile_id, |m| {
                let cfg = match m.deity {
                    Some(ref mut c) => c,
                    None => return false,
                };
                let list = match kind.as_str() {
                    "enemy" => &mut cfg.enemy_god_vnums,
                    "pact_item" => &mut cfg.pact_item_vnums,
                    "pact_quest" => &mut cfg.pact_quest_ids,
                    _ => return false,
                };
                let before = list.len();
                list.retain(|v| v != &value);
                before != list.len()
            })
            .unwrap_or(false)
        },
    );

    let d = db.clone();
    engine.register_fn(
        "deity_bless_add",
        move |mobile_id: String, effect_name: String, magnitude: i64| -> bool {
            let effect = match EffectType::from_str(&effect_name) {
                Some(e) => e,
                None => return false,
            };
            with_mobile(&d, &mobile_id, |m| match m.deity {
                Some(ref mut cfg) => {
                    cfg.blessing_effects.push(crate::BlessingGrant {
                        effect,
                        magnitude: magnitude as i32,
                    });
                    true
                }
                None => false,
            })
            .unwrap_or(false)
        },
    );

    let d = db.clone();
    engine.register_fn("deity_bless_remove", move |mobile_id: String, index: i64| -> bool {
        with_mobile(&d, &mobile_id, |m| match m.deity {
            Some(ref mut cfg) => {
                let idx = index as usize;
                if idx < cfg.blessing_effects.len() {
                    cfg.blessing_effects.remove(idx);
                    true
                } else {
                    false
                }
            }
            None => false,
        })
        .unwrap_or(false)
    });

    let d = db.clone();
    engine.register_fn("get_deity_config", move |mobile_id: String| -> rhai::Dynamic {
        let id = match Uuid::parse_str(&mobile_id) {
            Ok(i) => i,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let mobile = match d.get_mobile_data(&id) {
            Ok(Some(m)) => m,
            _ => return rhai::Dynamic::UNIT,
        };
        let cfg = match mobile.deity {
            Some(c) => c,
            None => return rhai::Dynamic::UNIT,
        };
        let mut map = rhai::Map::new();
        map.insert("rank".into(), cfg.rank.to_display_string().into());
        map.insert("epithet".into(), cfg.epithet.into());
        map.insert("lore".into(), cfg.lore.into());
        map.insert(
            "enemy_god_vnums".into(),
            cfg.enemy_god_vnums
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<rhai::Array>()
                .into(),
        );
        map.insert(
            "pact_item_vnums".into(),
            cfg.pact_item_vnums
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<rhai::Array>()
                .into(),
        );
        map.insert(
            "pact_quest_ids".into(),
            cfg.pact_quest_ids
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<rhai::Array>()
                .into(),
        );
        map.insert(
            "tribute_interval_days".into(),
            (cfg.tribute_interval_days as i64).into(),
        );
        map.insert("tribute_gold_percent".into(), (cfg.tribute_gold_percent as i64).into());
        map.insert(
            "blessing_effects".into(),
            cfg.blessing_effects
                .into_iter()
                .map(|g| {
                    let mut b = rhai::Map::new();
                    b.insert("effect".into(), g.effect.to_display_string().into());
                    b.insert("magnitude".into(), (g.magnitude as i64).into());
                    rhai::Dynamic::from(b)
                })
                .collect::<rhai::Array>()
                .into(),
        );
        map.insert("allow_permanent_smite".into(), cfg.allow_permanent_smite.into());
        rhai::Dynamic::from(map)
    });

    let d = db.clone();
    engine.register_fn(
        "set_mobile_patron_god",
        move |mobile_id: String, god_vnum: String| -> bool {
            with_mobile(&d, &mobile_id, |m| {
                m.patron_god_vnum = if god_vnum.is_empty() { None } else { Some(god_vnum) };
            })
            .is_some()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlessingGrant, MobileData};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn base_char(name: &str) -> CharacterData {
        let mut c: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        c.name = name.to_string();
        c
    }

    fn open_temp() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn empty_connections() -> SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn god_proto(db: &Db, vnum: &str, cfg: DeityConfig) {
        let mut m = MobileData::new(format!("god-{}", vnum));
        m.vnum = vnum.to_string();
        m.is_prototype = true;
        m.deity = Some(cfg);
        db.save_mobile_data(m).expect("save god");
    }

    fn default_god_cfg() -> DeityConfig {
        DeityConfig::default()
    }

    #[test]
    fn pact_requires_rank_god() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.rank = GodRank::Demigod;
        god_proto(&db, "pantheon:lesser", cfg);
        db.save_character_data(base_char("pilgrim")).unwrap();
        assert_eq!(
            create_worship_pact(&db, &conns, "pilgrim", "pantheon:lesser"),
            "not_a_god"
        );
    }

    #[test]
    fn pact_gate_quest_and_item_paths() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.pact_quest_ids = vec!["trial_of_flame".to_string()];
        god_proto(&db, "pantheon:wrath", cfg);

        // No quest, no item -> gate fails.
        db.save_character_data(base_char("novice")).unwrap();
        assert_eq!(
            create_worship_pact(&db, &conns, "novice", "pantheon:wrath"),
            "gate_failed"
        );

        // Completed quest -> ok.
        let mut c = base_char("zealot");
        c.completed_quests.insert("trial_of_flame".to_string());
        db.save_character_data(c).unwrap();
        assert_eq!(create_worship_pact(&db, &conns, "zealot", "pantheon:wrath"), "ok");
        let c = db.get_character_data("zealot").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().map(|w| w.god_vnum.as_str()), Some("pantheon:wrath"));

        // Already worshiping -> rejected.
        assert_eq!(
            create_worship_pact(&db, &conns, "zealot", "pantheon:wrath"),
            "already_worshiping"
        );
    }

    #[test]
    fn ungated_god_accepts_anyone() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:open", default_god_cfg());
        db.save_character_data(base_char("walkin")).unwrap();
        assert_eq!(create_worship_pact(&db, &conns, "walkin", "pantheon:open"), "ok");
    }

    #[test]
    fn tribute_takes_percent_of_total_and_resets_anger() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.tribute_gold_percent = 10;
        god_proto(&db, "pantheon:coin", cfg);
        let mut c = base_char("merchant");
        c.gold = 50;
        c.bank_gold = 950;
        c.worship = Some(WorshipState::new("pantheon:coin", 0));
        c.worship.as_mut().unwrap().anger_stage = 2;
        db.save_character_data(c).unwrap();

        let taken = take_tribute_gold_percent(&db, &conns, "merchant");
        assert_eq!(taken, 100); // 10% of 1000
        let c = db.get_character_data("merchant").unwrap().unwrap();
        assert_eq!(c.gold, 0); // on-hand drained first
        assert_eq!(c.bank_gold, 900); // remainder from bank
        assert_eq!(c.worship.as_ref().unwrap().anger_stage, 0);
    }

    #[test]
    fn blessing_stamps_tagged_buffs_and_strip_removes_only_those() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.tribute_interval_days = 2;
        cfg.blessing_effects = vec![BlessingGrant {
            effect: EffectType::StrengthBoost,
            magnitude: 2,
        }];
        god_proto(&db, "pantheon:might", cfg);
        let mut c = base_char("warrior");
        c.worship = Some(WorshipState::new("pantheon:might", 0));
        c.active_buffs.push(ActiveBuff {
            effect_type: EffectType::Haste,
            magnitude: 1,
            remaining_secs: 60,
            source: "potion".to_string(),
            damage_type: None,
            vs_effect: None,
            skill_key: None,
        });
        db.save_character_data(c).unwrap();

        assert_eq!(apply_worship_blessing(&db, &conns, "warrior"), "blessed");
        let c = db.get_character_data("warrior").unwrap().unwrap();
        let bless = c
            .active_buffs
            .iter()
            .find(|b| b.source == "worship:pantheon:might")
            .expect("blessing stamped");
        assert_eq!(bless.effect_type, EffectType::StrengthBoost);
        assert_eq!(bless.remaining_secs, 2 * GAME_DAY_SECS);

        assert!(strip_worship_buffs(&db, &conns, "warrior"));
        let c = db.get_character_data("warrior").unwrap().unwrap();
        assert!(!c.active_buffs.iter().any(|b| b.source.starts_with("worship:")));
        assert!(c.active_buffs.iter().any(|b| b.source == "potion")); // untouched
    }

    #[test]
    fn permanent_smite_gated_by_config() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:mercy", default_god_cfg()); // allow_permanent_smite = false
        let mut c = base_char("lapsed");
        c.hp = 100;
        c.max_hp = 100;
        c.worship = Some(WorshipState::new("pantheon:mercy", 0));
        db.save_character_data(c).unwrap();

        // Severity 4 downgrades to 3 without the opt-in.
        assert_eq!(smite_worshiper(&db, &conns, "lapsed", 4), "smitten");
        let c = db.get_character_data("lapsed").unwrap().unwrap();
        let blind = c
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Blind)
            .expect("blinded");
        assert_ne!(blind.remaining_secs, -1);

        let mut cfg = default_god_cfg();
        cfg.allow_permanent_smite = true;
        god_proto(&db, "pantheon:iron", cfg);
        let mut c = base_char("doomed");
        c.hp = 100;
        c.max_hp = 100;
        c.worship = Some(WorshipState::new("pantheon:iron", 0));
        db.save_character_data(c).unwrap();
        assert_eq!(smite_worshiper(&db, &conns, "doomed", 4), "smitten_permanent");
        let c = db.get_character_data("doomed").unwrap().unwrap();
        let blind = c
            .active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Blind)
            .expect("blinded");
        assert_eq!(blind.remaining_secs, -1);
        assert!(c.hp >= 1);
    }

    #[test]
    fn atonement_lifts_wrath_and_resets_counters() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.allow_permanent_smite = true;
        god_proto(&db, "pantheon:iron", cfg);
        let mut c = base_char("penitent");
        c.hp = 100;
        c.max_hp = 100;
        c.gold = 300;
        c.bank_gold = 700;
        let mut w = WorshipState::new("pantheon:iron", 0);
        w.anger_stage = 4;
        w.coworshiper_offenses = 2;
        c.worship = Some(w);
        db.save_character_data(c).unwrap();
        smite_worshiper(&db, &conns, "penitent", 4);

        let paid = atone_worship(&db, &conns, "penitent");
        assert_eq!(paid, 500);
        let c = db.get_character_data("penitent").unwrap().unwrap();
        assert!(!c.active_buffs.iter().any(|b| b.source.starts_with("wrath:")));
        let w = c.worship.as_ref().unwrap();
        assert_eq!(w.anger_stage, 0);
        assert_eq!(w.coworshiper_offenses, 0);
        assert_eq!(c.gold as i64 + c.bank_gold, 500);
    }

    #[test]
    fn atonement_needs_meaningful_gold() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:iron", default_god_cfg());
        let mut c = base_char("pauper");
        c.gold = 10;
        c.worship = Some(WorshipState::new("pantheon:iron", 0));
        db.save_character_data(c).unwrap();
        assert_eq!(atone_worship(&db, &conns, "pauper"), -2);
    }

    #[test]
    fn faith_offense_escalates_without_breaking_pact() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        god_proto(&db, "pantheon:oath", default_god_cfg());
        let mut c = base_char("traitor");
        c.hp = 100;
        c.max_hp = 100;
        c.worship = Some(WorshipState::new("pantheon:oath", 0));
        db.save_character_data(c).unwrap();

        // Different god's target: no punishment.
        assert_eq!(punish_faith_offense(&db, &conns, "traitor", "pantheon:other"), "");

        assert_eq!(punish_faith_offense(&db, &conns, "traitor", "pantheon:oath"), "first");
        assert_eq!(punish_faith_offense(&db, &conns, "traitor", "pantheon:oath"), "second");
        assert_eq!(punish_faith_offense(&db, &conns, "traitor", "pantheon:oath"), "smite");
        assert_eq!(punish_faith_offense(&db, &conns, "traitor", "pantheon:oath"), "smite");

        // Pact survives every offense.
        let c = db.get_character_data("traitor").unwrap().unwrap();
        assert!(c.worship.is_some());
        assert_eq!(c.worship.as_ref().unwrap().coworshiper_offenses, 4);
    }

    #[test]
    fn npc_kill_credit_requires_enemy_patron() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.enemy_god_vnums = vec!["pantheon:dark".to_string()];
        god_proto(&db, "pantheon:light", cfg);
        god_proto(&db, "pantheon:dark", default_god_cfg());

        let mut minion = MobileData::new("dark acolyte".to_string());
        minion.vnum = "dark:acolyte".to_string();
        minion.is_prototype = true;
        minion.patron_god_vnum = Some("pantheon:dark".to_string());
        db.save_mobile_data(minion).unwrap();

        let mut neutral = MobileData::new("rabbit".to_string());
        neutral.vnum = "wild:rabbit".to_string();
        neutral.is_prototype = true;
        db.save_mobile_data(neutral).unwrap();

        let mut c = base_char("paladin");
        c.worship = Some(WorshipState::new("pantheon:light", 0));
        db.save_character_data(c).unwrap();

        handle_npc_kill_credit(&db, &conns, "paladin", "wild:rabbit");
        let c = db.get_character_data("paladin").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().favor, 0);

        handle_npc_kill_credit(&db, &conns, "paladin", "dark:acolyte");
        let c = db.get_character_data("paladin").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().favor, FAVOR_PER_ENEMY_MINION);
    }

    #[test]
    fn pvp_kill_credit_once_per_victim_per_day() {
        let (db, _t) = open_temp();
        let conns = empty_connections();
        let mut cfg = default_god_cfg();
        cfg.enemy_god_vnums = vec!["pantheon:dark".to_string()];
        god_proto(&db, "pantheon:light", cfg);
        god_proto(&db, "pantheon:dark", default_god_cfg());

        let mut killer = base_char("crusader");
        killer.worship = Some(WorshipState::new("pantheon:light", 0));
        db.save_character_data(killer).unwrap();
        let mut victim = base_char("heretic");
        victim.worship = Some(WorshipState::new("pantheon:dark", 0));
        db.save_character_data(victim).unwrap();

        handle_pvp_kill_credit(&db, &conns, "crusader", "heretic");
        handle_pvp_kill_credit(&db, &conns, "crusader", "heretic");
        let c = db.get_character_data("crusader").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().favor, FAVOR_PER_ENEMY_WORSHIPER); // second kill same day: no credit

        // Non-enemy victim: nothing.
        let mut bystander = base_char("pilgrim");
        bystander.worship = Some(WorshipState::new("pantheon:light", 0));
        db.save_character_data(bystander).unwrap();
        handle_pvp_kill_credit(&db, &conns, "crusader", "pilgrim");
        let c = db.get_character_data("crusader").unwrap().unwrap();
        assert_eq!(c.worship.as_ref().unwrap().favor, FAVOR_PER_ENEMY_WORSHIPER);
    }
}
