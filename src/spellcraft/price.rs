//! The price machinery (§6): pledges of vigour and divine marks.
//!
//! Pledges lower ceilings at read time (`CharacterData::pledged`,
//! `effective_max_hp`/`_stamina`/`_resolve`); max mana is kept net of the mana
//! pledge, so every change here adjusts `max_mana` by the difference. Marks
//! stamp their affects as permanent buffs sourced `mark:<id>`, like tattoos.
//!
//! Every write is session-first (`achievements::apply_to_character`).

use crate::SharedConnections;
use crate::db::Db;
use crate::script::achievements::apply_to_character;
use crate::types::{
    CharacterData, DivineMark, DivinePledge, ItemAffect, MarkState, PledgeAmounts, PledgeKind, PledgeStatus, regrow_pct,
};

/// Short ids players and admins can type: `p1`, `p2`, ... / `m1`, ...
fn next_id(prefix: &str, taken: impl Iterator<Item = String>) -> String {
    let max = taken
        .filter_map(|id| id.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", max + 1)
}

/// Run `change` with `max_mana` kept net of the mana pledge, then clamp the
/// current pools under their (possibly lower) ceilings.
fn with_ceilings(ch: &mut CharacterData, change: impl FnOnce(&mut CharacterData)) {
    let mana_before = ch.pledged(PledgeKind::Mana);
    change(ch);
    let mana_after = ch.pledged(PledgeKind::Mana);
    ch.max_mana = (ch.max_mana + mana_before - mana_after).max(0);
    clamp_pools(ch);
}

fn clamp_pools(ch: &mut CharacterData) {
    ch.hp = ch.hp.min(ch.effective_max_hp());
    ch.stamina = ch.stamina.min(ch.effective_max_stamina());
    ch.mana = ch.mana.min(ch.max_mana);
    let max_resolve = ch.effective_max_resolve();
    if let Some(r) = ch.replicant_state.as_mut() {
        r.resolve = r.resolve.min(max_resolve);
    }
}

/// The ceiling `kind` would have with no pledges at all.
fn pre_pledge(ch: &CharacterData, kind: PledgeKind) -> i64 {
    let mut bare = ch.clone();
    bare.divine_pledges.clear();
    match kind {
        PledgeKind::Hp => bare.effective_max_hp() as i64,
        PledgeKind::Stamina => bare.effective_max_stamina() as i64,
        PledgeKind::Mana => (ch.max_mana + ch.pledged(PledgeKind::Mana)) as i64,
        PledgeKind::Resolve => bare.effective_max_resolve() as i64,
    }
}

/// Replicants pay stamina in Resolve: their stamina is pinned.
pub fn convert_for(ch: &CharacterData, mut amounts: PledgeAmounts) -> PledgeAmounts {
    if ch.replicant_state.is_some() && amounts.stamina > 0 {
        amounts.resolve += amounts.stamina;
        amounts.stamina = 0;
    }
    amounts
}

/// Would `amounts` (already converted) take any ceiling below `floor_pct`
/// of its pre-pledge value? `Err` names the first one that would.
pub fn affordable(ch: &CharacterData, amounts: &PledgeAmounts, floor_pct: i64) -> Result<(), String> {
    for (kind, label) in [
        (PledgeKind::Hp, "health"),
        (PledgeKind::Stamina, "stamina"),
        (PledgeKind::Mana, "mana"),
        (PledgeKind::Resolve, "resolve"),
    ] {
        let more = amounts.get(kind) as i64;
        if more <= 0 {
            continue;
        }
        let base = pre_pledge(ch, kind);
        let held = ch.pledged(kind) as i64;
        if held + more > base * (100 - floor_pct) / 100 {
            return Err(format!("You have too little {label} left to pledge."));
        }
    }
    Ok(())
}

/// Shrink `amounts` until it fits above the floor (each kind on its own).
pub fn clamp_to_floor(ch: &CharacterData, mut amounts: PledgeAmounts, floor_pct: i64) -> PledgeAmounts {
    for kind in [
        PledgeKind::Hp,
        PledgeKind::Stamina,
        PledgeKind::Mana,
        PledgeKind::Resolve,
    ] {
        let room = (pre_pledge(ch, kind) * (100 - floor_pct) / 100 - ch.pledged(kind) as i64).max(0) as i32;
        let slot = match kind {
            PledgeKind::Hp => &mut amounts.hp,
            PledgeKind::Stamina => &mut amounts.stamina,
            PledgeKind::Mana => &mut amounts.mana,
            PledgeKind::Resolve => &mut amounts.resolve,
        };
        *slot = (*slot).clamp(0, room);
    }
    amounts
}

/// Add a pledge to `ch` (pure; the caller persists): converts stamina for
/// replicants, then either refuses (`clamp = false`) or shrinks (`clamp =
/// true`) anything past the floor, and clamps the pools. Returns the id, or
/// `Ok(None)` when nothing was left to pledge after clamping.
pub fn push_pledge(
    ch: &mut CharacterData,
    spell_id: &str,
    god_vnum: &str,
    amounts: PledgeAmounts,
    today: i64,
    floor_pct: i64,
    clamp: bool,
) -> Result<Option<String>, String> {
    let mut amounts = convert_for(ch, amounts);
    if clamp {
        amounts = clamp_to_floor(ch, amounts, floor_pct);
    } else {
        affordable(ch, &amounts, floor_pct)?;
    }
    if amounts.is_empty() {
        return Ok(None);
    }
    let id = next_id("p", ch.divine_pledges.iter().map(|p| p.id.clone()));
    let pledge = DivinePledge {
        id: id.clone(),
        spell_id: spell_id.to_string(),
        god_vnum: god_vnum.to_string(),
        amounts,
        made_day: today,
        status: PledgeStatus::Active,
        outstanding_pct: 100,
        floor_pct: floor_pct.clamp(0, 100) as u8,
    };
    with_ceilings(ch, |c| c.divine_pledges.push(pledge));
    Ok(Some(id))
}

/// Put `mark` on `ch` with a fresh id and stamp it (pure).
pub fn push_mark(ch: &mut CharacterData, mut mark: DivineMark) -> String {
    mark.id = next_id("m", ch.divine_marks.iter().map(|m| m.id.clone()));
    let id = mark.id.clone();
    with_ceilings(ch, |c| {
        restamp(c, &mark);
        c.divine_marks.push(mark);
    });
    id
}

/// Pledge `amounts` to `god_vnum` for `spell_id`. Converts stamina for
/// replicants, refuses anything that would breach the floor, and clamps the
/// current pools. Returns the new pledge's id.
pub fn take_pledge(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    spell_id: &str,
    god_vnum: &str,
    amounts: PledgeAmounts,
) -> Result<String, String> {
    let settings = super::Settings::load(db);
    let today = crate::script::worship::current_absolute_day(db);
    let mut out = Err("No such character.".to_string());
    apply_to_character(db, connections, char_name, |ch| {
        out = match push_pledge(ch, spell_id, god_vnum, amounts, today, settings.pledge_floor_pct, false) {
            Ok(Some(id)) => Ok(id),
            Ok(None) => Err("Nothing to pledge.".to_string()),
            Err(e) => Err(e),
        };
        out.is_ok()
    });
    out
}

/// Change one pledge's status. `Err` when it can't move that way.
fn set_status(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    pledge_id: &str,
    to: impl Fn(PledgeStatus) -> Option<PledgeStatus>,
) -> Result<(), String> {
    let mut out = Err("No such character.".to_string());
    apply_to_character(db, connections, char_name, |ch| {
        let Some(i) = ch
            .divine_pledges
            .iter()
            .position(|p| p.id.eq_ignore_ascii_case(pledge_id))
        else {
            out = Err(format!("No pledge '{pledge_id}'."));
            return false;
        };
        let Some(next) = to(ch.divine_pledges[i].status) else {
            out = Err("That pledge can't change that way.".to_string());
            return false;
        };
        with_ceilings(ch, |c| {
            let p = &mut c.divine_pledges[i];
            p.status = next;
            p.outstanding_pct = 100;
        });
        out = Ok(());
        true
    });
    out
}

/// Let a pledge grow back (the spell left the canon, or was renounced).
pub fn release_pledge(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    pledge_id: &str,
) -> Result<(), String> {
    let today = crate::script::worship::current_absolute_day(db);
    set_status(db, connections, char_name, pledge_id, |s| match s {
        PledgeStatus::Active | PledgeStatus::Forfeit => Some(PledgeStatus::Regrowing { from_day: today }),
        _ => None,
    })
}

/// Take a pledge for good (apostasy). Whatever had grown back is taken again.
pub fn forfeit_pledge(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    pledge_id: &str,
) -> Result<(), String> {
    set_status(db, connections, char_name, pledge_id, |s| match s {
        PledgeStatus::Active | PledgeStatus::Regrowing { .. } => Some(PledgeStatus::Forfeit),
        _ => None,
    })
}

/// Bring regrowing pledges up to `today`, flipping fully regrown ones to
/// `Repaid`. Pure; the regen tick calls it on the session copy. True when
/// anything changed.
pub fn refresh_pledges(ch: &mut CharacterData, today: i64, regrow_days: i64) -> bool {
    let stale = ch.divine_pledges.iter().any(|p| match p.status {
        PledgeStatus::Regrowing { from_day } => regrow_pct(from_day, today, regrow_days) != p.outstanding_pct,
        _ => false,
    });
    if !stale {
        return false;
    }
    with_ceilings(ch, |c| {
        for p in &mut c.divine_pledges {
            if let PledgeStatus::Regrowing { from_day } = p.status {
                p.outstanding_pct = regrow_pct(from_day, today, regrow_days);
                if p.outstanding_pct == 0 {
                    p.status = PledgeStatus::Repaid;
                }
            }
        }
    });
    true
}

/// A spell left the canon (or its author renounced it): its pledges start
/// growing back and its marks cool. Pure; true when anything changed.
pub fn release_spell(ch: &mut CharacterData, spell_id: &str, today: i64) -> bool {
    let pledged = ch
        .divine_pledges
        .iter()
        .any(|p| p.spell_id == spell_id && p.status == PledgeStatus::Active);
    let marked = ch
        .divine_marks
        .iter()
        .any(|m| m.spell_id.as_deref() == Some(spell_id) && m.state != MarkState::Faded);
    if !pledged && !marked {
        return false;
    }
    with_ceilings(ch, |c| {
        for p in &mut c.divine_pledges {
            if p.spell_id == spell_id && p.status == PledgeStatus::Active {
                p.status = PledgeStatus::Regrowing { from_day: today };
                p.outstanding_pct = 100;
            }
        }
        let faded: Vec<DivineMark> = c
            .divine_marks
            .iter_mut()
            .filter(|m| m.spell_id.as_deref() == Some(spell_id))
            .map(|m| {
                m.state = MarkState::Faded;
                m.clone()
            })
            .collect();
        for m in &faded {
            restamp(c, m);
        }
    });
    true
}

/// Apostasy (pure): every Active or Regrowing pledge to `god_vnum` is
/// forfeit, and every mark from it burns again with `worse`. Returns
/// (pledges forfeit, marks worsened).
pub fn forsake(ch: &mut CharacterData, god_vnum: &str, worse: &ItemAffect) -> (usize, usize) {
    let mut forfeit = 0;
    let mut worsened = 0;
    with_ceilings(ch, |c| {
        for p in &mut c.divine_pledges {
            if p.god_vnum == god_vnum && matches!(p.status, PledgeStatus::Active | PledgeStatus::Regrowing { .. }) {
                p.status = PledgeStatus::Forfeit;
                p.outstanding_pct = 100;
                forfeit += 1;
            }
        }
        let marks: Vec<DivineMark> = c
            .divine_marks
            .iter_mut()
            .filter(|m| m.god_vnum == god_vnum)
            .map(|m| {
                m.state = MarkState::Worsened;
                m.worse_affect = Some(worse.clone());
                m.clone()
            })
            .collect();
        worsened = marks.len();
        for m in &marks {
            restamp(c, m);
        }
    });
    (forfeit, worsened)
}

// ---------- marks ----------

/// Strip a mark's buffs and stamp what its state calls for. Idempotent.
fn restamp(ch: &mut CharacterData, mark: &DivineMark) {
    let source = mark.buff_source();
    ch.active_buffs.retain(|b| b.source != source);
    crate::script::tattoos::stamp_affects(ch, &source, &mark.live_affects());
}

/// Put `mark` on the character (a fresh id is assigned) and stamp it.
pub fn stamp_mark(db: &Db, connections: &SharedConnections, char_name: &str, mark: DivineMark) -> Option<String> {
    let mut id = None;
    apply_to_character(db, connections, char_name, |ch| {
        id = Some(push_mark(ch, mark.clone()));
        true
    })?;
    id
}

/// Change a mark (by id) and restamp it. `Err` when there's no such mark.
fn change_mark(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    mark_id: &str,
    f: impl FnOnce(&mut DivineMark),
) -> Result<(), String> {
    let mut out = Err("No such character.".to_string());
    apply_to_character(db, connections, char_name, |ch| {
        let Some(i) = ch.divine_marks.iter().position(|m| m.id.eq_ignore_ascii_case(mark_id)) else {
            out = Err(format!("No mark '{mark_id}'."));
            return false;
        };
        f(&mut ch.divine_marks[i]);
        let mark = ch.divine_marks[i].clone();
        with_ceilings(ch, |c| restamp(c, &mark));
        out = Ok(());
        true
    });
    out
}

/// The mark cools: it stays, but stamps nothing.
pub fn fade_mark(db: &Db, connections: &SharedConnections, char_name: &str, mark_id: &str) -> Result<(), String> {
    change_mark(db, connections, char_name, mark_id, |m| m.state = MarkState::Faded)
}

/// The mark burns again with a second, negative affect (apostasy).
pub fn worsen_mark(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    mark_id: &str,
    worse: ItemAffect,
) -> Result<(), String> {
    change_mark(db, connections, char_name, mark_id, |m| {
        m.state = MarkState::Worsened;
        m.worse_affect = Some(worse);
    })
}

/// Remove a mark and its buffs entirely (admin/support).
pub fn unstamp_mark(db: &Db, connections: &SharedConnections, char_name: &str, mark_id: &str) -> Result<(), String> {
    let mut out = Err("No such character.".to_string());
    apply_to_character(db, connections, char_name, |ch| {
        let Some(i) = ch.divine_marks.iter().position(|m| m.id.eq_ignore_ascii_case(mark_id)) else {
            out = Err(format!("No mark '{mark_id}'."));
            return false;
        };
        let mark = ch.divine_marks.remove(i);
        let source = mark.buff_source();
        with_ceilings(ch, |c| c.active_buffs.retain(|b| b.source != source));
        out = Ok(());
        true
    });
    out
}

// ---------- display ----------

pub(crate) fn god_name(db: &Db, vnum: &str) -> String {
    match crate::script::worship::god_display_name(db, vnum) {
        n if n.is_empty() => vnum.to_string(),
        n => n,
    }
}

fn amounts_line(a: &PledgeAmounts) -> String {
    let mut parts = Vec::new();
    for (n, label) in [
        (a.hp, "HP"),
        (a.stamina, "stamina"),
        (a.mana, "mana"),
        (a.resolve, "resolve"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {label}"));
        }
    }
    parts.join(", ")
}

/// The `status` block: what the gods hold of you. Empty when nothing.
pub fn status_lines(db: &Db, ch: &CharacterData) -> String {
    let open: Vec<&DivinePledge> = ch.divine_pledges.iter().filter(|p| p.is_open()).collect();
    if open.is_empty() {
        return String::new();
    }
    let mut out = String::from("Divine pledges:\n");
    for (kind, label, base) in [
        (PledgeKind::Hp, "Max HP", pre_pledge(ch, PledgeKind::Hp)),
        (PledgeKind::Stamina, "Max stamina", pre_pledge(ch, PledgeKind::Stamina)),
        (PledgeKind::Mana, "Max mana", pre_pledge(ch, PledgeKind::Mana)),
        (PledgeKind::Resolve, "Max resolve", pre_pledge(ch, PledgeKind::Resolve)),
    ] {
        let held = ch.pledged(kind);
        if held > 0 {
            let now = ch.after_pledges(base, kind);
            out.push_str(&format!("  {label} {now} (-{} pledged)\n", base - now));
        }
    }
    if ch.pledged(PledgeKind::Stamina) > 0 || ch.pledged(PledgeKind::Resolve) > 0 {
        let gods: Vec<String> = open
            .iter()
            .filter(|p| p.amounts.stamina > 0 || p.amounts.resolve > 0)
            .map(|p| god_name(db, &p.god_vnum))
            .collect();
        out.push_str(&format!("  Weary (pledged to {})\n", gods.join(", ")));
    }
    for p in open {
        let state = match p.status {
            PledgeStatus::Active => "held".to_string(),
            PledgeStatus::Forfeit => "taken for good".to_string(),
            PledgeStatus::Regrowing { .. } => format!("growing back, {}% still held", p.outstanding_pct),
            PledgeStatus::Repaid => "repaid".to_string(),
        };
        let what = if p.spell_id.is_empty() {
            String::new()
        } else {
            format!(" for {}", p.spell_id)
        };
        out.push_str(&format!(
            "  [{}] {} to {}{} ({})\n",
            p.id,
            amounts_line(&p.amounts),
            god_name(db, &p.god_vnum),
            what,
            state
        ));
    }
    out
}

/// One line per mark, for `examine`: "a forge-brand across the palm on their hand".
pub fn mark_lines(ch: &CharacterData) -> Vec<String> {
    ch.divine_marks
        .iter()
        .map(|m| {
            if m.body_part.is_empty() {
                m.display()
            } else {
                format!("{} on their {}", m.display(), m.body_part)
            }
        })
        .collect()
}

// ---------- admin ----------

pub(crate) fn parse_affect(spec: &str) -> Result<ItemAffect, String> {
    let (name, mag) = match spec.split_once(':') {
        Some((n, m)) => (n, m.parse::<i32>().map_err(|_| format!("Bad magnitude in '{spec}'."))?),
        None => (spec, 1),
    };
    let effect_type = crate::types::EffectType::from_str(name).ok_or_else(|| format!("Unknown effect '{name}'."))?;
    Ok(ItemAffect {
        effect_type,
        magnitude: mag,
        damage_type: None,
        vs_effect: None,
        skill_key: None,
    })
}

fn describe_affects(m: &DivineMark) -> String {
    let parts: Vec<String> = m
        .affects
        .iter()
        .chain(m.worse_affect.iter())
        .map(|a| format!("{} {:+}", a.effect_type.to_display_string(), a.magnitude))
        .collect();
    parts.join(", ")
}

pub const PLEDGE_USAGE: &str = "Usage: admin pledge <player> list\n       admin pledge <player> add [hp=N] [stamina=N] [mana=N] [god=<vnum>] [spell=<id>]\n       admin pledge <player> release|forfeit <id>";
pub const MARK_USAGE: &str = "Usage: admin mark <player> list\n       admin mark <player> add <god vnum> <body part> <effect[:mag]> <short desc...>\n       admin mark <player> fade|remove <id>\n       admin mark <player> worsen <id> <effect[:mag]>";

/// `admin pledge ...`. `args` are the words after "pledge".
pub fn admin_pledge(db: &Db, connections: &SharedConnections, args: &[&str]) -> String {
    let (Some(player), Some(action)) = (args.first(), args.get(1)) else {
        return PLEDGE_USAGE.to_string();
    };
    let Some(ch) = apply_to_character(db, connections, player, |_| false) else {
        return format!("No character '{player}'.");
    };
    match *action {
        "list" => {
            let text = status_lines(db, &ch);
            if text.is_empty() {
                format!("{} has no open pledges.", ch.name)
            } else {
                text
            }
        }
        "add" => {
            let mut amounts = PledgeAmounts::default();
            let (mut god, mut spell) = (String::new(), String::new());
            for arg in &args[2..] {
                let Some((k, v)) = arg.split_once('=') else {
                    return PLEDGE_USAGE.to_string();
                };
                let n = v.parse::<i32>().unwrap_or(-1);
                match k {
                    "hp" => amounts.hp = n,
                    "stamina" => amounts.stamina = n,
                    "mana" => amounts.mana = n,
                    "god" => god = v.to_string(),
                    "spell" => spell = v.to_string(),
                    _ => return PLEDGE_USAGE.to_string(),
                }
                if n < 0 && matches!(k, "hp" | "stamina" | "mana") {
                    return format!("Bad amount '{arg}'.");
                }
            }
            match take_pledge(db, connections, &ch.name, &spell, &god, amounts) {
                Ok(id) => format!("Pledge {id} taken from {}.", ch.name),
                Err(e) => e,
            }
        }
        "release" | "forfeit" => {
            let Some(id) = args.get(2) else {
                return PLEDGE_USAGE.to_string();
            };
            let done = if *action == "release" {
                release_pledge(db, connections, &ch.name, id)
            } else {
                forfeit_pledge(db, connections, &ch.name, id)
            };
            match done {
                Ok(()) => format!(
                    "Pledge {id} {}d.",
                    if *action == "release" { "release" } else { "forfeite" }
                ),
                Err(e) => e,
            }
        }
        _ => PLEDGE_USAGE.to_string(),
    }
}

/// `admin mark ...`. `args` are the words after "mark".
pub fn admin_mark(db: &Db, connections: &SharedConnections, args: &[&str]) -> String {
    let (Some(player), Some(action)) = (args.first(), args.get(1)) else {
        return MARK_USAGE.to_string();
    };
    let Some(ch) = apply_to_character(db, connections, player, |_| false) else {
        return format!("No character '{player}'.");
    };
    let result = match *action {
        "list" => {
            if ch.divine_marks.is_empty() {
                return format!("{} bears no divine marks.", ch.name);
            }
            let mut out = String::new();
            for m in &ch.divine_marks {
                out.push_str(&format!(
                    "  [{}] {} ({}) on {} from {} [{:?}] {}\n",
                    m.id,
                    m.short_desc,
                    describe_affects(m),
                    m.body_part,
                    god_name(db, &m.god_vnum),
                    m.state,
                    m.spell_id.as_deref().unwrap_or("")
                ));
            }
            return out;
        }
        "add" => {
            if args.len() < 6 {
                return MARK_USAGE.to_string();
            }
            let affect = match parse_affect(args[4]) {
                Ok(a) => a,
                Err(e) => return e,
            };
            let mark = DivineMark {
                id: String::new(),
                god_vnum: args[2].to_string(),
                spell_id: None,
                body_part: args[3].to_string(),
                short_desc: args[5..].join(" "),
                affects: vec![affect],
                worse_affect: None,
                state: MarkState::Live,
            };
            return match stamp_mark(db, connections, &ch.name, mark) {
                Some(id) => format!("Mark {id} stamped on {}.", ch.name),
                None => "The mark would not take.".to_string(),
            };
        }
        "fade" | "remove" => {
            let Some(id) = args.get(2) else {
                return MARK_USAGE.to_string();
            };
            if *action == "fade" {
                fade_mark(db, connections, &ch.name, id)
            } else {
                unstamp_mark(db, connections, &ch.name, id)
            }
        }
        "worsen" => {
            let (Some(id), Some(spec)) = (args.get(2), args.get(3)) else {
                return MARK_USAGE.to_string();
            };
            match parse_affect(spec) {
                Ok(a) => worsen_mark(db, connections, &ch.name, id, a),
                Err(e) => Err(e),
            }
        }
        _ => return MARK_USAGE.to_string(),
    };
    match result {
        Ok(()) => "Done.".to_string(),
        Err(e) => e,
    }
}
