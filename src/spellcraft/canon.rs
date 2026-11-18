//! Canon life (§7, §8, §12): devotion, displacement, omens, restoration,
//! temple teaching and the author's full form.
//!
//! Devotion: faithful casts are counted in memory (`World.codex_cast_counts`)
//! and drained into each god's canon by the leaderboard tick, decaying with a
//! half-life of [`DEVOTION_HALF_LIFE_DAYS`] game days and floored by how many
//! characters know the spell. Sealing into a full canon displaces the
//! least-devoted slot past its grace period: that spell fades, its author's
//! pledge starts growing back and their mark cools.

use std::collections::{HashMap, HashSet};

use super::{Settings, draft, gate, petition, price, text};
use crate::db::Db;
use crate::script::achievements::{apply_to_character, send_to_player};
use crate::types::{
    Canon, CanonStatus, CharacterData, CodexEntry, DeitySpellcraft, DraftStep, FrozenPetition, SpellDraft,
};
use crate::{SharedConnections, SharedState};

/// Game days for unrefreshed devotion to halve.
pub const DEVOTION_HALF_LIFE_DAYS: f64 = 15.0;
/// Devotion floor per character who knows the spell.
pub const KNOWN_BY_WEIGHT: f64 = 0.5;
/// Learners' mastery cap on a codex spell; its author reaches 10.
pub const LEARNER_MASTERY_CAP: i32 = 7;
/// Mana discount (percent) for the author's full form.
pub const FULL_FORM_DISCOUNT_PCT: i32 = 25;

/// `devotion` after `days` of decay.
pub fn decay(devotion: f64, days: i64) -> f64 {
    devotion * 0.5f64.powf(days.max(0) as f64 / DEVOTION_HALF_LIFE_DAYS)
}

fn god_cfg(db: &Db, god_vnum: &str) -> Option<DeitySpellcraft> {
    crate::script::worship::deity_config_by_vnum(db, god_vnum)?.spellcraft
}

/// Tell `name` now if online, else by mail from the god.
pub fn notify(db: &Db, connections: &SharedConnections, name: &str, from: &str, text: &str) {
    let online = connections
        .lock()
        .values()
        .filter_map(|s| s.character.as_ref())
        .any(|c| c.name.eq_ignore_ascii_case(name));
    if online {
        send_to_player(connections, name, &format!("{text}\n"));
    } else {
        let _ = db.store_mail(crate::types::MailMessage::new(
            from.to_string(),
            name.to_string(),
            text.to_string(),
        ));
    }
}

// ---------- devotion ----------

/// Casts of one codex spell since the last drain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CastTally {
    /// By worshippers of the spell's god while it is Canon: devotion.
    pub faithful: u32,
    /// By anyone but the current author: the author's fame.
    pub by_others: u32,
}

/// Count a cast of `spell_id` by `char_name`: toward devotion when the
/// caster worships the spell's god, and toward the author's fame when the
/// caster isn't the author. Locks Connections, then World, never together.
pub fn note_cast(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str, spell_id: &str) {
    if !spell_id.starts_with(crate::types::CODEX_ID_PREFIX) {
        return;
    }
    let Ok(Some(entry)) = db.get_codex(spell_id) else {
        return;
    };
    let god = connections
        .lock()
        .values()
        .filter_map(|s| s.character.as_ref())
        .find(|c| c.name.eq_ignore_ascii_case(char_name))
        .and_then(|c| c.worship.as_ref().map(|w| w.god_vnum.clone()));
    let faithful = god.as_deref() == Some(entry.meta.god_vnum.as_str()) && entry.meta.status == CanonStatus::Canon;
    let by_other = entry
        .meta
        .author
        .as_deref()
        .is_none_or(|a| !a.eq_ignore_ascii_case(char_name));
    if !faithful && !by_other {
        return;
    }
    let mut world = state.lock();
    let tally = world.codex_cast_counts.entry(spell_id.to_string()).or_default();
    if faithful {
        tally.faithful += 1;
    }
    if by_other {
        tally.by_others += 1;
    }
}

/// One canon's devotion pass (pure): decay to `today`, add `casts`, apply
/// the known-by floor, recompute influence, and move the omen. Returns the
/// spell ids newly warned.
pub fn apply_devotion(
    c: &mut Canon,
    casts: &HashMap<String, u32>,
    known_by: &HashMap<String, u32>,
    today: i64,
    cap: u32,
) -> Vec<String> {
    for slot in &mut c.slots {
        let since = if slot.devotion_day > 0 {
            today - slot.devotion_day
        } else {
            0
        };
        let mut d = decay(slot.devotion, since) + *casts.get(&slot.spell_id).unwrap_or(&0) as f64;
        let floor = *known_by.get(&slot.spell_id).unwrap_or(&0) as f64 * KNOWN_BY_WEIGHT;
        if d < floor {
            d = floor;
        }
        slot.devotion = d;
        slot.devotion_day = today;
    }
    c.influence = c.slots.iter().map(|s| s.devotion).sum::<f64>().round().max(0.0) as u32;

    // The omen: only once the canon is full is the lowest slot next to go.
    let lowest = if c.slots.len() as u32 >= cap && c.slots.len() > 1 {
        c.slots
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.devotion.total_cmp(&b.1.devotion))
            .map(|(i, _)| i)
    } else {
        None
    };
    let mut warned = Vec::new();
    for (i, slot) in c.slots.iter_mut().enumerate() {
        let is_lowest = Some(i) == lowest;
        if is_lowest && !slot.warned {
            warned.push(slot.spell_id.clone());
        }
        slot.warned = is_lowest;
    }
    warned
}

/// Drain the cast counts into every canon. Called by the leaderboard tick
/// after it releases World; `chars` is the character list it loaded.
pub fn drain_devotion(db: &Db, connections: &SharedConnections, state: &SharedState, chars: &[CharacterData]) {
    let tallies = std::mem::take(&mut state.lock().codex_cast_counts);
    let casts: HashMap<String, u32> = tallies
        .iter()
        .filter(|(_, t)| t.faithful > 0)
        .map(|(id, t)| (id.clone(), t.faithful))
        .collect();
    for (id, t) in tallies.iter().filter(|(_, t)| t.by_others > 0) {
        let mut before = 0;
        let Ok(Some(e)) = db.update_codex(id, |e| {
            before = e.meta.casts_by_others;
            e.meta.casts_by_others += t.by_others as u64;
        }) else {
            continue;
        };
        if let (Some(m), Some(author)) = (
            super::fame::milestone_crossed(before, e.meta.casts_by_others),
            e.meta.author.as_deref(),
        ) {
            notify(
                db,
                connections,
                author,
                "the Codex",
                &format!(
                    "\x1b[1;33mYour {} has now been cast {m} times by other hands.\x1b[0m",
                    e.def.name
                ),
            );
        }
    }
    let mut known_by: HashMap<String, u32> = HashMap::new();
    for ch in chars {
        for s in ch
            .learned_spells
            .iter()
            .filter(|s| s.starts_with(crate::types::CODEX_ID_PREFIX))
        {
            *known_by.entry(s.clone()).or_insert(0) += 1;
        }
    }
    let today = crate::script::worship::current_absolute_day(db);
    for canon in db.list_canons().unwrap_or_default() {
        let god = canon.god_vnum.clone();
        let cap = god_cfg(db, &god).map(|c| c.canon_cap).unwrap_or(5);
        let mut warned = Vec::new();
        let _ = db.update_canon(&god, |c| {
            warned = apply_devotion(c, &casts, &known_by, today, cap);
            true
        });
        let god_name = price::god_name(db, &god);
        for spell_id in warned {
            if let Ok(Some(e)) = db.get_codex(&spell_id)
                && let Some(author) = e.meta.author.clone()
            {
                notify(
                    db,
                    connections,
                    &author,
                    &god_name,
                    &format!(
                        "\x1b[0;35m{god_name}'s attention drifts from your {}.\x1b[0m",
                        e.def.name
                    ),
                );
            }
        }
    }
}

// ---------- displacement ----------

/// The slot a full canon gives up: the least devoted past its grace period.
pub fn pick_displaced(c: &Canon, today: i64, grace_days: i64) -> Option<usize> {
    c.slots
        .iter()
        .enumerate()
        .filter(|(_, s)| today >= s.sealed_day + grace_days)
        .min_by(|a, b| a.1.devotion.total_cmp(&b.1.devotion))
        .map(|(i, _)| i)
}

/// A spell leaves the canon: Faded, its author's pledge regrows and mark
/// cools, and the author hears of it. Idempotent.
pub fn fade_spell(db: &Db, connections: &SharedConnections, state: &SharedState, spell_id: &str) {
    let Ok(entry) = super::codex::set_status(db, state, spell_id, CanonStatus::Faded) else {
        return;
    };
    let Some(author) = entry.meta.author.clone() else {
        return;
    };
    let today = crate::script::worship::current_absolute_day(db);
    let mut changed = false;
    apply_to_character(db, connections, &author, |c| {
        changed = price::release_spell(c, spell_id, today);
        changed
    });
    if changed {
        let god = price::god_name(db, &entry.meta.god_vnum);
        notify(
            db,
            connections,
            &author,
            &god,
            &format!(
                "\x1b[0;35m{god} lets your {} fall from the canon. Its brand cools, and what you pledged begins to return to you.\x1b[0m",
                entry.def.name
            ),
        );
    }
}

/// Fade any of `god_vnum`'s Canon-status spells that hold no slot and are
/// past grace (so not a seal in flight). Recovers a crash between the canon
/// write and [`fade_spell`].
pub fn reconcile(db: &Db, connections: &SharedConnections, state: &SharedState, god_vnum: &str) {
    let today = crate::script::worship::current_absolute_day(db);
    let grace = Settings::load(db).canon_grace_days;
    let held: HashSet<String> = db
        .get_canon(god_vnum)
        .map(|c| c.slots.into_iter().map(|s| s.spell_id).collect())
        .unwrap_or_default();
    for e in db.list_codex().unwrap_or_default() {
        let orphan = e.meta.god_vnum == god_vnum
            && e.meta.status == CanonStatus::Canon
            && e.meta.origin == crate::types::SpellOrigin::Authored
            && !held.contains(&e.def.id)
            && today >= e.meta.sealed_day + grace;
        if orphan {
            fade_spell(db, connections, state, &e.def.id);
        }
    }
}

/// Add `spell_id` to the canon, displacing if full. Returns the displaced
/// spell id (already faded) if any. Idempotent.
pub fn take_slot(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    god_vnum: &str,
    spell_id: &str,
    today: i64,
) -> Option<String> {
    let cap = god_cfg(db, god_vnum).map(|c| c.canon_cap).unwrap_or(5);
    let grace = Settings::load(db).canon_grace_days;
    let mut displaced = None;
    let _ = db.update_canon(god_vnum, |c| {
        displaced = None;
        if c.slots.iter().any(|s| s.spell_id == spell_id) {
            return false;
        }
        if c.slots.len() as u32 >= cap
            && let Some(i) = pick_displaced(c, today, grace)
        {
            displaced = Some(c.slots.remove(i).spell_id);
        }
        c.god_vnum = god_vnum.to_string();
        c.slots.push(crate::types::CanonSlot {
            spell_id: spell_id.to_string(),
            sealed_day: today,
            devotion: 0.0,
            warned: false,
            devotion_day: today,
        });
        true
    });
    if let Some(id) = &displaced {
        fade_spell(db, connections, state, id);
    }
    reconcile(db, connections, state, god_vnum);
    displaced
}

// ---------- restoration ----------

fn find_faded(db: &Db, god_vnum: &str, arg: &str) -> Option<CodexEntry> {
    let want = arg.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    db.list_codex()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.meta.god_vnum == god_vnum && e.meta.status == CanonStatus::Faded && !e.meta.vetoed)
        .find(|e| e.def.id == want || e.def.name.to_lowercase().starts_with(&want))
}

/// `pray restore <spell>`: petition to re-light a faded spell of your god.
/// Same gates as a petition, a restoration trial, a fixed rite.
pub fn restore(db: &Db, connections: &SharedConnections, char_name: &str, arg: &str) -> String {
    let Some(ch) = apply_to_character(db, connections, char_name, |_| false) else {
        return "You are nowhere.\n".to_string();
    };
    let Some(god_vnum) = ch.worship.as_ref().map(|w| w.god_vnum.clone()) else {
        return "You must first be bound to a god.\n".to_string();
    };
    let in_temple = db
        .get_room_data(&ch.current_room_id)
        .ok()
        .flatten()
        .is_some_and(|r| r.flags.temple);
    if !in_temple {
        return "Restorations are heard only in a temple.\n".to_string();
    }
    let Some(cfg) = god_cfg(db, &god_vnum) else {
        return "Your god grants no spells.\n".to_string();
    };
    let god_name = price::god_name(db, &god_vnum);
    let Some(entry) = find_faded(db, &god_vnum, arg) else {
        return format!("{god_name} holds no faded spell by that name.\n");
    };
    let settings = Settings::load(db);
    let clock = gate::Clock::now(db);
    let status = gate::petition_status(db, &ch, &god_vnum, &settings, clock);
    if !status.can_petition() {
        let unmet: Vec<String> = status
            .requirements
            .iter()
            .filter(|r| !r.met)
            .map(|r| r.label.clone())
            .collect();
        return format!("{god_name} will not hear you now ({}).\n", unmet.join("; "));
    }
    let design = draft::design_of(&entry.def);
    let Some(rule) = draft::pick_restoration_trial(&cfg.trial_rules, &design) else {
        return format!("{god_name} sets no task for restorations yet.\n");
    };
    let Some(quest) = db.get_quest_data(&rule.quest_id).ok().flatten() else {
        return format!("{god_name} sets no task for restorations yet.\n");
    };
    let mut p = draft::price(design.power, &cfg, &settings);
    // A smaller pledge: the spell is already shaped.
    p.hp = (p.hp / 2).max(1);
    p.stamina /= 2;
    p.mana /= 2;
    let floor = super::draft::Domain::new(&cfg, &settings).floor;
    let frozen = FrozenPetition {
        draft: SpellDraft {
            god_vnum: god_vnum.clone(),
            step: DraftStep::Review,
            spell_type: Some(entry.def.spell_type.clone()),
            name: Some(entry.def.name.clone()),
            description: Some(entry.def.description.clone()),
            flavor: entry.meta.flavor.clone(),
            ..SpellDraft::default()
        },
        spell: entry.def.clone(),
        power: design.power,
        tier: super::power::tier(design.power),
        price: p,
        components: draft::components_for(&design, &god_vnum, floor, &settings),
        trial_quest_id: quest.vnum.clone(),
        offered_day: clock.today,
        restoring: Some(entry.def.id.clone()),
    };
    match petition::claim_and_set_trial(
        db,
        connections,
        &god_vnum,
        &god_name,
        &cfg,
        &settings,
        &ch.name,
        &quest,
        clock,
        frozen,
    ) {
        Ok(_) => {
            let mut out = format!(
                "\x1b[1;36m{god_name} will hear you restore {}.\x1b[0m\nYour trial: \x1b[1m{}\x1b[0m\n",
                entry.def.name, quest.name
            );
            if !quest.description.is_empty() {
                out.push_str(&format!("{}\n", quest.description));
            }
            out.push_str("When it is done, return to a temple and begin the rite with 'pray ritual'.\n");
            out
        }
        Err(why) => why,
    }
}

/// Re-light a faded entry for `name` (the restoration seal's codex step).
/// Authorship moves to the restorer; their own spell is simply re-lit.
/// Idempotent: an entry already re-lit by them is returned as is.
pub(crate) fn relight(
    db: &Db,
    state: &SharedState,
    name: &str,
    spell_id: &str,
    witnesses: &[String],
    offered_day: i64,
    today: i64,
) -> Option<CodexEntry> {
    let entry = db
        .update_codex(spell_id, |e| {
            let done = e.meta.status == CanonStatus::Canon
                && e.meta.author.as_deref() == Some(name)
                && e.meta.sealed_day >= offered_day;
            if done {
                return;
            }
            let previous = e.meta.author.clone();
            e.meta.status = CanonStatus::Canon;
            e.meta.sealed_day = today;
            e.meta.author_disgraced = false;
            if previous.as_deref() != Some(name) {
                e.meta.author = Some(name.to_string());
                e.meta.lineage.push(crate::types::LineageEvent {
                    kind: crate::types::LineageKind::Restored,
                    who: name.to_string(),
                    day: today,
                });
            }
            for w in witnesses {
                if !e.meta.witnesses.iter().any(|x| x.eq_ignore_ascii_case(w)) {
                    e.meta.witnesses.push(w.clone());
                }
            }
            e.meta.version += 1;
        })
        .ok()
        .flatten()?;
    super::mirror_into_world(state, &entry);
    Some(entry)
}

// ---------- temple teaching ----------

/// Teach one of `god_vnum`'s canon spells to `ch` (pure on `ch`; the caller
/// saves it). `spell` picks one by id or name; `None` picks the oldest canon
/// spell `ch` doesn't know yet. Costs `gold_cost` gold and `favor_cost`
/// favor. Faded spells are never taught.
pub fn teach_canon(
    db: &Db,
    ch: &mut CharacterData,
    god_vnum: &str,
    spell: Option<&str>,
    gold_cost: i32,
    favor_cost: i32,
) -> Result<String, String> {
    let worships = ch.worship.as_ref().is_some_and(|w| w.god_vnum == god_vnum);
    if !worships {
        return Err("I teach only the faithful.".to_string());
    }
    let favor = ch.worship.as_ref().map(|w| w.favor).unwrap_or(0);
    if crate::worship::favor::FavorTier::from_value(favor) < crate::worship::favor::FavorTier::Favored {
        return Err("Our god does not yet favor you enough.".to_string());
    }
    let canon = db.get_canon(god_vnum).unwrap_or_default();
    let mut slots = canon.slots.clone();
    slots.sort_by_key(|s| s.sealed_day);
    let entries: Vec<CodexEntry> = slots
        .iter()
        .filter_map(|s| db.get_codex(&s.spell_id).ok().flatten())
        .filter(|e| e.meta.status == CanonStatus::Canon && !e.meta.vetoed)
        .collect();
    let chosen = match spell {
        Some(want) => {
            let want = want.to_lowercase();
            let faded = db
                .list_codex()
                .unwrap_or_default()
                .into_iter()
                .any(|e| e.def.name.to_lowercase() == want && e.meta.status != CanonStatus::Canon);
            match entries
                .iter()
                .find(|e| e.def.id == want || e.def.name.to_lowercase() == want)
            {
                Some(e) => e.clone(),
                None if faded => return Err("That prayer has faded from the canon; I cannot teach it.".to_string()),
                None => return Err("That is not one of our canon's prayers.".to_string()),
            }
        }
        None => match entries.iter().find(|e| !ch.learned_spells.contains(&e.def.id)) {
            Some(e) => e.clone(),
            None => return Err("You know every prayer our canon holds.".to_string()),
        },
    };
    if ch.learned_spells.contains(&chosen.def.id) {
        return Err(format!("You already know {}.", chosen.def.name));
    }
    let magic = ch.skills.get("magic").map(|s| s.level).unwrap_or(0);
    if magic < chosen.def.skill_required {
        return Err(format!(
            "{} needs magic {}; you have {magic}.",
            chosen.def.name, chosen.def.skill_required
        ));
    }
    if ch.gold < gold_cost {
        return Err(format!("The teaching asks {gold_cost} gold; you have {}.", ch.gold));
    }
    if favor < favor_cost {
        return Err(format!("The teaching asks {favor_cost} favor; you have {favor}."));
    }
    ch.gold -= gold_cost;
    if let Some(w) = ch.worship.as_mut() {
        w.favor -= favor_cost.max(0);
    }
    ch.learned_spells.push(chosen.def.id.clone());
    crate::progress::award_xp_to_character(ch, super::SPELLCRAFT_SKILL, super::XP_PER_CODEX_LEARN, false);
    let mut paid = Vec::new();
    if gold_cost > 0 {
        paid.push(format!("{gold_cost} gold"));
    }
    if favor_cost > 0 {
        paid.push(format!("{favor_cost} favor"));
    }
    let cost = if paid.is_empty() {
        String::new()
    } else {
        format!(" ({})", paid.join(", "))
    };
    Ok(format!("You learn {}{cost}.", chosen.def.name))
}

// ---------- casting ----------

/// What `cast` needs to know about a codex spell for this caster.
#[derive(Debug, Clone, Default)]
pub struct CastInfo {
    pub codex: bool,
    /// The author casting their own canon spell, in good standing.
    pub true_form: bool,
    /// How observers see the spell's name.
    pub seen_as: String,
    pub caster: String,
    pub room: String,
    pub target: String,
}

pub fn cast_info(db: &Db, char_name: &str, spell_id: &str, spell_name: &str) -> CastInfo {
    let mut info = CastInfo {
        seen_as: spell_name.to_string(),
        ..CastInfo::default()
    };
    if !spell_id.starts_with(crate::types::CODEX_ID_PREFIX) {
        return info;
    }
    let Ok(Some(e)) = db.get_codex(spell_id) else {
        return info;
    };
    info.codex = true;
    info.true_form = is_full_form(&e, char_name);
    let god = price::god_name(db, &e.meta.god_vnum);
    if !god.is_empty() {
        info.seen_as = format!("{spell_name}, gift of {god}");
    }
    info.caster = text::sanitize_flavor(&e.meta.flavor.caster);
    info.room = text::sanitize_flavor(&e.meta.flavor.room);
    info.target = text::sanitize_flavor(&e.meta.flavor.target);
    info
}

fn is_full_form(e: &CodexEntry, char_name: &str) -> bool {
    e.meta.status == CanonStatus::Canon
        && !e.meta.author_disgraced
        && e.meta
            .author
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(char_name))
}

/// Mastery cap for `spell_id`: 10, except codex spells cap learners at
/// [`LEARNER_MASTERY_CAP`] (the author in full form reaches 10).
pub fn mastery_cap(db: &Db, char_name: &str, spell_id: &str) -> i32 {
    if !spell_id.starts_with(crate::types::CODEX_ID_PREFIX) {
        return 10;
    }
    match db.get_codex(spell_id) {
        Ok(Some(e)) if is_full_form(&e, char_name) => 10,
        Ok(Some(_)) => LEARNER_MASTERY_CAP,
        _ => 10,
    }
}
