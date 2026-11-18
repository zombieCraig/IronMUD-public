//! Heresy and apostasy (§10, §11).
//!
//! Casting or learning a codex spell of a god you do not worship is heresy.
//! The tier depends on how the gods stand: godless 1, neutral 2, enemies 3,
//! and +1 if you once worshipped the spell's god (apostate). A cast always
//! costs a backlash; then your own god and the spell's god each roll
//! independently, at a chance that grows with their grudge against you.
//! Grudges live outside `WorshipState` and never decay.
//!
//! `worship renounce` ends a pact: the god's grudge jumps, it smites at
//! once, its pledges are forfeit for good, its marks burn again, and the
//! codex spells you authored for it name you Faithless (they stay canon).

use super::ritual::Dice;
use super::{Settings, codex, price};
use crate::db::Db;
use crate::script::achievements::apply_to_character;
use crate::types::{CharacterData, EffectType, FormerFaith, ItemAffect};
use crate::{SharedConnections, SharedState};

/// Seconds a heresy or renunciation confirmation stays open.
pub const CONFIRM_SECS: i64 = 60;
/// Grudge a god takes at being renounced.
pub const RENOUNCE_GRUDGE: u32 = 5;
/// Smite severity at renunciation.
pub const RENOUNCE_SMITE: i32 = 2;

/// Heresy tier for `caster_god` using a spell of `spell_god`, or `None`
/// when it is no heresy (same god, or a spell of no god).
pub fn tier(caster_god: Option<&str>, spell_god: &str, enemies: bool, apostate: bool) -> Option<u8> {
    if spell_god.is_empty() || caster_god == Some(spell_god) {
        return None;
    }
    let base = match caster_god {
        None => 1,
        Some(_) if enemies => 3,
        Some(_) => 2,
    };
    Some(base + apostate as u8)
}

/// Percent chance a god acts on a heresy of `tier` given its `grudge`.
pub fn chance(tier: u8, grudge: u32) -> u32 {
    let base = match tier {
        0 => 0,
        1 => 10,
        2 => 20,
        3 => 35,
        _ => 50,
    };
    (base + 5 * grudge.min(20)).min(95)
}

/// Backlash damage for a cast of `power` at `tier`: 5% of power per tier.
pub fn backlash(power: u32, tier: u8) -> i32 {
    ((power as i64 * tier as i64 * 5 / 100) as i32).max(1)
}

/// Codex spells `name` authored for `god_vnum`.
pub fn authored_for(db: &Db, name: &str, god_vnum: &str) -> Vec<String> {
    db.list_codex()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            e.meta.god_vnum == god_vnum
                && e.meta.lineage.iter().any(|l| l.who.eq_ignore_ascii_case(name))
                && e.meta.author.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(name))
        })
        .map(|e| e.def.id)
        .collect()
}

/// A renunciation still too recent for a new pact.
pub fn repact_blocked(db: &Db, ch: &CharacterData) -> bool {
    let Some(last) = ch.former_faiths.iter().filter(|f| f.renounced).map(|f| f.to_day).max() else {
        return false;
    };
    let today = crate::script::worship::current_absolute_day(db);
    today < last + Settings::load(db).repact_cooldown_days
}

fn enemies(db: &Db, a: &str, b: &str) -> bool {
    let lists = |x: &str, y: &str| {
        crate::script::worship::deity_config_by_vnum(db, x).is_some_and(|c| c.enemy_god_vnums.iter().any(|v| v == y))
    };
    lists(a, b) || lists(b, a)
}

/// The heresy a character commits with `spell_id`: (tier, spell god,
/// spell power), or `None`.
fn assess(db: &Db, ch: &CharacterData, spell_id: &str) -> Option<(u8, String, u32)> {
    if !spell_id.starts_with(crate::types::CODEX_ID_PREFIX) || !Settings::load(db).enabled {
        return None;
    }
    let entry = db.get_codex(spell_id).ok().flatten()?;
    let spell_god = entry.meta.god_vnum.clone();
    // A god this world doesn't have: the spell is godless (§13).
    crate::script::worship::deity_config_by_vnum(db, &spell_god)?;
    let own = ch.worship.as_ref().map(|w| w.god_vnum.as_str());
    let foes = own.is_some_and(|o| enemies(db, o, &spell_god));
    let apostate = ch.former_faiths.iter().any(|f| f.renounced && f.god_vnum == spell_god);
    let t = tier(own, &spell_god, foes, apostate)?;
    Some((t, spell_god, super::power::power_score(&entry.def)))
}

fn read(db: &Db, conns: &SharedConnections, name: &str) -> Option<CharacterData> {
    apply_to_character(db, conns, name, |_| false)
}

/// An offence: both gods' grudges grow (with `backlash` HP taken, floor 1,
/// in the same write), then each god rolls. Returns lines for the caster.
fn offend(
    db: &Db,
    conns: &SharedConnections,
    name: &str,
    t: u8,
    spell_god: &str,
    backlash_hp: i32,
    dice: &mut dyn Dice,
) -> Vec<String> {
    let today = crate::script::worship::current_absolute_day(db);
    let mut own: Option<(String, u32)> = None;
    let mut theirs = 0u32;
    let mut taken = 0;
    apply_to_character(db, conns, name, |c| {
        if backlash_hp > 0 {
            let before = c.hp;
            c.hp = (c.hp - backlash_hp).max(1);
            taken = before - c.hp;
        }
        let g = c.divine_grudges.entry(spell_god.to_string()).or_default();
        theirs = g.count;
        g.count += 1;
        g.last_day = today;
        if let Some(w) = c.worship.as_ref() {
            let god = w.god_vnum.clone();
            let g = c.divine_grudges.entry(god.clone()).or_default();
            own = Some((god, g.count));
            g.count += 1;
            g.last_day = today;
        }
        true
    });
    let mut lines = Vec::new();
    if taken > 0 {
        lines.push(format!(
            "\x1b[0;31mThe power comes, but it does not want to come for you. It tears its way out (-{taken} HP).\x1b[0m"
        ));
    }
    if let Some((_, prev)) = own
        && (dice.d100() as u32) <= chance(t, prev)
    {
        crate::script::worship::punish_heresy(db, conns, name, prev + 1);
    }
    if (dice.d100() as u32) <= chance(t, theirs) {
        crate::script::worship::smite_character(db, conns, spell_god, name, (t as i32).min(3));
    }
    lines
}

/// Called from the cast tail for every codex spell. Empty for no heresy.
pub fn on_foreign_cast(db: &Db, conns: &SharedConnections, name: &str, spell_id: &str, dice: &mut dyn Dice) -> String {
    let Some(ch) = read(db, conns, name) else {
        return String::new();
    };
    let Some((t, god, power)) = assess(db, &ch, spell_id) else {
        return String::new();
    };
    offend(db, conns, name, t, &god, backlash(power, t), dice).join("\n")
}

/// The learning offence, fired once when a foreign spell is learned. No
/// backlash; the gods still roll.
pub fn on_foreign_learn(db: &Db, conns: &SharedConnections, name: &str, spell_id: &str, dice: &mut dyn Dice) {
    if let Some(ch) = read(db, conns, name)
        && let Some((t, god, _)) = assess(db, &ch, spell_id)
    {
        offend(db, conns, name, t, &god, 0, dice);
    }
}

/// Is learning `spell_id` heresy for `name`?
pub fn is_foreign(db: &Db, conns: &SharedConnections, name: &str, spell_id: &str) -> bool {
    read(db, conns, name).is_some_and(|ch| assess(db, &ch, spell_id).is_some())
}

/// The warning shown before a foreign spell is learned.
pub fn learn_warning(db: &Db, conns: &SharedConnections, name: &str, spell_id: &str) -> String {
    let Some(ch) = read(db, conns, name) else {
        return String::new();
    };
    let Some((t, god, _)) = assess(db, &ch, spell_id) else {
        return String::new();
    };
    let god_name = price::god_name(db, &god);
    let mut out = format!("\x1b[1;31mThis is a prayer of {god_name}, and you are not theirs.\x1b[0m\n");
    out.push_str(&format!(
        "{god_name} will remember the theft, and so will {}. Every cast will tear at you.\n",
        if ch.worship.is_some() {
            "your own god"
        } else {
            "every god who sees it"
        }
    ));
    if t >= 3 {
        out.push_str("The gods are enemies. Their wrath will be swift.\n");
    }
    out
}

// ---------- confirmations ----------

fn session_of<'a>(
    conns: &'a mut std::collections::HashMap<crate::ConnectionId, crate::PlayerSession>,
    name: &str,
) -> Option<&'a mut crate::PlayerSession> {
    conns
        .values_mut()
        .find(|s| s.character.as_ref().is_some_and(|c| c.name.eq_ignore_ascii_case(name)))
}

/// Open a confirmation for `name` (session-only).
pub fn open_confirm(conns: &SharedConnections, name: &str, key: &str, now: i64) {
    if let Some(s) = session_of(&mut conns.lock(), name) {
        s.spellcraft_confirm = Some((key.to_string(), now + CONFIRM_SECS));
    }
}

/// Consume a live confirmation for `key`. False if none, another, or lapsed.
pub fn take_confirm(conns: &SharedConnections, name: &str, key: &str, now: i64) -> bool {
    let mut guard = conns.lock();
    let Some(s) = session_of(&mut guard, name) else {
        return false;
    };
    match s.spellcraft_confirm.take() {
        Some((k, until)) if k == key && now <= until => true,
        other => {
            // A different pending confirmation stays open.
            if other.as_ref().is_some_and(|(k, until)| k != key && now <= *until) {
                s.spellcraft_confirm = other;
            }
            false
        }
    }
}

// ---------- apostasy ----------

fn worse_affect() -> ItemAffect {
    ItemAffect {
        effect_type: EffectType::Curse,
        magnitude: 1,
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    }
}

/// `worship renounce [confirm]`. The first call warns and opens a
/// confirmation; `confirm` within [`CONFIRM_SECS`] carries it out.
pub fn renounce(
    db: &Db,
    conns: &SharedConnections,
    state: &SharedState,
    name: &str,
    confirm: bool,
    now: i64,
) -> String {
    let Some(ch) = read(db, conns, name) else {
        return "You are nowhere.\n".to_string();
    };
    let Some(w) = ch.worship.clone() else {
        return "You worship no god.\n".to_string();
    };
    let in_temple = db
        .get_room_data(&ch.current_room_id)
        .ok()
        .flatten()
        .is_some_and(|r| r.flags.temple);
    if !in_temple {
        return "A god is renounced only in a temple, before witnesses.\n".to_string();
    }
    if ch.pending_ritual.is_some() {
        return "Not in the middle of a rite.\n".to_string();
    }
    let god = w.god_vnum.clone();
    let god_name = price::god_name(db, &god);
    let key = format!("renounce:{god}");
    let authored = authored_for(db, name, &god);

    if !confirm || !take_confirm(conns, name, &key, now) {
        open_confirm(conns, name, &key, now);
        let pledges = ch
            .divine_pledges
            .iter()
            .filter(|p| p.god_vnum == god && p.is_open())
            .count();
        let marks = ch.divine_marks.iter().filter(|m| m.god_vnum == god).count();
        let mut out = format!("\x1b[1;31mYou mean to renounce {god_name}.\x1b[0m\n");
        out.push_str(&format!("{god_name} will strike you, and will never forget it.\n"));
        if pledges > 0 {
            out.push_str(&format!(
                "What you pledged ({pledges} pledge{}) is lost for good.\n",
                if pledges == 1 { "" } else { "s" }
            ));
        }
        if marks > 0 {
            out.push_str("Their marks on you will burn again.\n");
        }
        if !authored.is_empty() {
            out.push_str("The spells you brought stay in their canon, but you will be named Faithless.\n");
        }
        out.push_str("Type 'worship renounce confirm' within a minute to do it.\n");
        return out;
    }

    let today = crate::script::worship::current_absolute_day(db);
    let worse = worse_affect();
    let blessings = format!("worship:{god}");
    let mut dropped_quest = None;
    let mut done = false;
    let mut forsaken = (0, 0);
    apply_to_character(db, conns, name, |c| {
        let Some(w) = c.worship.take_if(|w| w.god_vnum == god) else {
            return false;
        };
        done = true;
        c.former_faiths.push(FormerFaith {
            god_vnum: god.clone(),
            from_day: w.pact_day,
            to_day: today,
            authored: authored.clone(),
            renounced: true,
        });
        let g = c.divine_grudges.entry(god.clone()).or_default();
        g.count += RENOUNCE_GRUDGE;
        g.last_day = today;
        c.active_buffs.retain(|b| b.source != blessings);
        forsaken = price::forsake(c, &god, &worse);
        if c.pending_petition.as_ref().is_some_and(|p| p.draft.god_vnum == god) {
            dropped_quest = c.pending_petition.take().map(|p| p.trial_quest_id);
        }
        true
    });
    if !done {
        return "You worship no god.\n".to_string();
    }
    for id in &authored {
        let _ = codex::set_disgraced(db, state, id, true);
    }
    if let Some(q) = dropped_quest {
        let _ = crate::quest::abandon(db, conns, name, &q);
    }
    crate::script::worship::smite_character(db, conns, &god, name, RENOUNCE_SMITE);
    let room = ch.current_room_id;
    crate::session::broadcast_to_room(conns, room, format!("{name} renounces {god_name}.\n"), Some(name));
    let mut out = format!(
        "\x1b[1;31mYou renounce {god_name}. The gods do not release their own, and they do not release you gently.\x1b[0m\n"
    );
    if forsaken.0 > 0 {
        out.push_str("What you pledged is gone for good.\n");
    }
    if forsaken.1 > 0 {
        out.push_str("Their marks burn on your skin.\n");
    }
    out
}

/// One line per god that holds a grudge, for `worship` and admin views.
pub fn grudge_lines(db: &Db, ch: &CharacterData) -> Vec<String> {
    let mut gods: Vec<_> = ch.divine_grudges.iter().filter(|(_, g)| g.count > 0).collect();
    gods.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
    gods.into_iter()
        .map(|(v, g)| format!("{} ({}, since day {})", price::god_name(db, v), g.count, g.last_day))
        .collect()
}

/// Admin: forget one god's grudge. True if there was one.
pub fn clear_grudge(db: &Db, conns: &SharedConnections, name: &str, god_vnum: &str) -> bool {
    let mut had = false;
    apply_to_character(db, conns, name, |c| {
        had = c.divine_grudges.remove(god_vnum).is_some();
        had
    });
    had
}
