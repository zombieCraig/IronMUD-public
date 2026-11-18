//! The petition mode: `pray petition` → shape a spell → `offer` → a trial.
//!
//! While a draft is open the session's `olc_mode` is [`MODE`], and every
//! line the player types comes here (via `pray.rhai`), so none of the drafting
//! words reach the global command parser. The draft lives on the session
//! (`PlayerSession.spell_draft`) until it is offered; the god's acceptance
//! freezes it onto the character as `pending_petition`.
//!
//! Lock discipline: session reads/writes take Connections briefly, the spell
//! table is read under World briefly, and never both at once.

use uuid::Uuid;

use crate::db::Db;
use crate::types::{CharacterData, DeitySpellcraft, DraftStep, FrozenPetition, SpellDraft};
use crate::{SharedConnections, SharedState};

use super::Settings;
use super::draft::{self, Design, Domain};
use super::gate::{Clock, petition_status};
use super::power::tier;
use super::text;

/// `olc_mode` while a petition is being drafted.
pub const MODE: &str = "petitioning";

/// Every step, in the order they are asked.
const ORDER: [DraftStep; 12] = [
    DraftStep::Type,
    DraftStep::Element,
    DraftStep::Effect,
    DraftStep::Target,
    DraftStep::Magnitude,
    DraftStep::Duration,
    DraftStep::Name,
    DraftStep::Description,
    DraftStep::FlavorCaster,
    DraftStep::FlavorTarget,
    DraftStep::FlavorRoom,
    DraftStep::Review,
];

/// What a line of input led to.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub text: String,
    /// The player said `offer` and the draft is complete and valid: the
    /// caller fires the god's `OnPray` trigger (action `petition`), then calls
    /// [`offer`] unless the trigger vetoed.
    pub offer: Option<OfferPreview>,
}

#[derive(Debug, Clone)]
pub struct OfferPreview {
    pub name: String,
    pub spell_type: String,
    pub power: u32,
    pub tier: u8,
}

fn reply(text: impl Into<String>) -> Reply {
    Reply {
        text: text.into(),
        offer: None,
    }
}

// ---------- session ----------

fn session_state(conns: &SharedConnections, conn_id: Uuid) -> Option<(CharacterData, Option<SpellDraft>)> {
    let conns = conns.lock();
    let s = conns.get(&conn_id)?;
    Some((s.character.clone()?, s.spell_draft.clone()))
}

/// Store the draft and set the mode (or clear both with `None`).
fn store(conns: &SharedConnections, conn_id: Uuid, draft: Option<SpellDraft>) {
    let mut conns = conns.lock();
    if let Some(s) = conns.get_mut(&conn_id) {
        s.olc_mode = draft.as_ref().map(|_| MODE.to_string());
        s.spell_draft = draft;
    }
}

/// The script that owns a petitioner's input line, or `None` when the
/// petition just broke off and the line should run as an ordinary command.
pub fn route(conns: &SharedConnections, conn_id: Uuid) -> Option<&'static str> {
    if release_if_fighting(conns, conn_id) {
        return None;
    }
    Some("scripts/commands/pray.rhai")
}

/// If a petitioner is attacked, the petition breaks off and the input goes
/// on to the normal command parser (so they can fight or flee). Returns true
/// when it broke off; the message is already sent.
pub fn release_if_fighting(conns: &SharedConnections, conn_id: Uuid) -> bool {
    let mut conns = conns.lock();
    let Some(s) = conns.get_mut(&conn_id) else {
        return false;
    };
    let fighting = s.character.as_ref().is_some_and(|c| c.combat.in_combat);
    if !fighting || s.olc_mode.as_deref() != Some(MODE) {
        return false;
    }
    s.olc_mode = None;
    s.spell_draft = None;
    let _ = s
        .sender
        .send("\x1b[1;31mYour petition breaks off as violence finds you.\x1b[0m\n".to_string());
    true
}

// ---------- the god and its domain ----------

struct God {
    vnum: String,
    name: String,
    cfg: DeitySpellcraft,
    settings: Settings,
    domain: Domain,
    designs: Vec<Design>,
}

impl God {
    fn load(db: &Db, vnum: &str) -> Option<God> {
        let cfg = crate::script::worship::deity_config_by_vnum(db, vnum)?.spellcraft?;
        let settings = Settings::load(db);
        let domain = Domain::new(&cfg, &settings);
        let designs = draft::designs(&domain);
        let name = match crate::script::worship::god_display_name(db, vnum) {
            n if n.is_empty() => vnum.to_string(),
            n => n,
        };
        Some(God {
            vnum: vnum.to_string(),
            name,
            cfg,
            settings,
            domain,
            designs,
        })
    }
}

// ---------- steps ----------

fn applies(step: DraftStep, d: &SpellDraft) -> bool {
    let t = d.spell_type.as_deref().unwrap_or("");
    match step {
        DraftStep::Element => t == "damage",
        DraftStep::Effect | DraftStep::Duration => t == "buff" || t == "debuff",
        DraftStep::FlavorTarget => d.target.as_deref() != Some("self"),
        _ => true,
    }
}

fn is_menu(step: DraftStep) -> bool {
    matches!(
        step,
        DraftStep::Type
            | DraftStep::Element
            | DraftStep::Effect
            | DraftStep::Target
            | DraftStep::Magnitude
            | DraftStep::Duration
    )
}

fn filled(step: DraftStep, d: &SpellDraft) -> bool {
    match step {
        DraftStep::Type => d.spell_type.is_some(),
        DraftStep::Element => d.element.is_some(),
        DraftStep::Effect => d.effect.is_some(),
        DraftStep::Target => d.target.is_some(),
        DraftStep::Magnitude => d.magnitude.is_some(),
        DraftStep::Duration => d.duration.is_some(),
        DraftStep::Name => d.name.is_some(),
        DraftStep::Description => d.description.is_some(),
        DraftStep::FlavorCaster | DraftStep::FlavorTarget | DraftStep::FlavorRoom => d.history.contains(&step),
        DraftStep::Review => false,
    }
}

fn clear(step: DraftStep, d: &mut SpellDraft) {
    match step {
        DraftStep::Type => d.spell_type = None,
        DraftStep::Element => d.element = None,
        DraftStep::Effect => d.effect = None,
        DraftStep::Target => d.target = None,
        DraftStep::Magnitude => d.magnitude = None,
        DraftStep::Duration => d.duration = None,
        DraftStep::Name => d.name = None,
        DraftStep::Description => d.description = None,
        DraftStep::FlavorCaster => d.flavor.caster.clear(),
        DraftStep::FlavorTarget => d.flavor.target.clear(),
        DraftStep::FlavorRoom => d.flavor.room.clear(),
        DraftStep::Review => {}
    }
}

/// One menu answer: the value stored, and how it reads.
struct Choice {
    value: String,
    label: String,
}

fn amount_label(design: &Design) -> String {
    match design.spell_type.as_str() {
        "damage" => format!("{} damage", design.magnitude),
        "heal" => format!("heals {}", design.magnitude),
        _ => format!("strength {}", design.magnitude),
    }
}

fn power_label(power: u32) -> String {
    format!("power {power}, tier {}", tier(power))
}

/// The answers to `step` that still lead to a design the god accepts.
fn choices(step: DraftStep, d: &SpellDraft, god: &God) -> Vec<Choice> {
    let open = draft::open_designs(&god.designs, d);
    let mut out: Vec<Choice> = Vec::new();
    let mut add = |value: String, label: String| {
        if !out.iter().any(|c| c.value == value) {
            out.push(Choice { value, label });
        }
    };
    for x in open {
        match step {
            DraftStep::Type => add(x.spell_type.clone(), x.spell_type.clone()),
            DraftStep::Element => {
                let e = x.element.clone().unwrap_or_default();
                add(e.clone(), e)
            }
            DraftStep::Effect => {
                let e = x.effect.clone().unwrap_or_default();
                add(e.clone(), e.replace('_', " "))
            }
            DraftStep::Target => {
                let label = match x.target.as_str() {
                    "self_or_friendly" => "yourself or an ally",
                    "self" => "yourself only",
                    _ => "an enemy",
                };
                add(x.target.clone(), label.to_string())
            }
            DraftStep::Magnitude => {
                // For blasts and heals the amount is the last choice, so show
                // what it makes; buffs still have a duration to pick.
                let label = if x.duration.is_none() {
                    format!("{} ({})", amount_label(x), power_label(x.power))
                } else {
                    amount_label(x)
                };
                add(x.magnitude.to_string(), label)
            }
            DraftStep::Duration => {
                let secs = x.duration.unwrap_or(0);
                add(secs.to_string(), format!("{secs} seconds ({})", power_label(x.power)))
            }
            _ => {}
        }
    }
    if step == DraftStep::Magnitude || step == DraftStep::Duration {
        out.sort_by_key(|c| c.value.parse::<i64>().unwrap_or(0));
    }
    out
}

fn set_choice(step: DraftStep, d: &mut SpellDraft, value: &str) {
    let v = Some(value.to_string());
    match step {
        DraftStep::Type => d.spell_type = v,
        DraftStep::Element => d.element = v,
        DraftStep::Effect => d.effect = v,
        DraftStep::Target => d.target = v,
        DraftStep::Magnitude => d.magnitude = value.parse().ok(),
        DraftStep::Duration => d.duration = value.parse().ok(),
        _ => {}
    }
}

/// Move to the first unanswered step, filling single-answer menus on the
/// way. Returns notes about anything filled for the player.
fn advance(d: &mut SpellDraft, god: &God) -> Vec<String> {
    let mut notes = Vec::new();
    loop {
        let step = ORDER
            .iter()
            .copied()
            .find(|s| applies(*s, d) && !filled(*s, d))
            .unwrap_or(DraftStep::Review);
        d.step = step;
        if !is_menu(step) {
            return notes;
        }
        let opts = choices(step, d, god);
        if opts.len() != 1 {
            return notes;
        }
        notes.push(format!(
            "({}: {} is the only way {} allows.)",
            step_title(step),
            opts[0].label,
            god.name
        ));
        set_choice(step, d, &opts[0].value);
    }
}

/// Undo the last answer and every mechanical choice after it.
fn back(d: &mut SpellDraft) -> bool {
    let Some(last) = d.history.pop() else {
        return false;
    };
    let at = ORDER.iter().position(|s| *s == last).unwrap_or(0);
    clear(last, d);
    for s in &ORDER[at + 1..] {
        if is_menu(*s) {
            clear(*s, d);
        }
    }
    true
}

fn step_title(step: DraftStep) -> &'static str {
    match step {
        DraftStep::Type => "Shape",
        DraftStep::Element => "Element",
        DraftStep::Effect => "Effect",
        DraftStep::Target => "Reach",
        DraftStep::Magnitude => "Strength",
        DraftStep::Duration => "Duration",
        DraftStep::Name => "Name",
        DraftStep::Description => "Description",
        DraftStep::FlavorCaster => "Your words",
        DraftStep::FlavorTarget => "Its touch",
        DraftStep::FlavorRoom => "What others see",
        DraftStep::Review => "Review",
    }
}

fn question(step: DraftStep, god: &str) -> String {
    match step {
        DraftStep::Type => format!("{god} asks: What shape shall the gift take?"),
        DraftStep::Element => format!("{god} asks: Of what shall it burn?"),
        DraftStep::Effect => format!("{god} asks: What shall it work upon the body?"),
        DraftStep::Target => format!("{god} asks: Whom shall it reach?"),
        DraftStep::Magnitude => format!("{god} asks: How strong?"),
        DraftStep::Duration => format!("{god} asks: For how long?"),
        DraftStep::Name => format!(
            "{god} asks: By what name shall it be known? ({}-{} letters; spaces and apostrophes allowed)",
            text::NAME_MIN,
            text::NAME_MAX
        ),
        DraftStep::Description => format!(
            "{god} asks: Describe it, as the codex will. (one line, up to {} characters)",
            text::DESCRIPTION_MAX
        ),
        DraftStep::FlavorCaster => {
            "What do you see as you cast it? (one line, or 'none' for the plain message)".to_string()
        }
        DraftStep::FlavorTarget => "What does its target see? (one line, or 'none')".to_string(),
        DraftStep::FlavorRoom => {
            "What do onlookers see? Your name will be put before it. (one line, or 'none')".to_string()
        }
        DraftStep::Review => {
            format!("Type 'offer' to lay this before {god}, 'back' to change the last answer, or 'abort'.")
        }
    }
}

// ---------- rendering ----------

fn price_line(p: &crate::types::Price) -> String {
    use crate::types::PriceStyle;
    let mut parts = vec![format!("{} max HP pledged", p.hp)];
    if p.mana > 0 {
        parts.push(format!("{} max mana pledged", p.mana));
    }
    if p.stamina > 0 {
        parts.push(format!("{} max stamina pledged, and slower recovery", p.stamina));
    }
    if p.attribute > 0 {
        parts.push(format!("{} point of a base attribute, for good", p.attribute));
    }
    if p.gold > 0 {
        parts.push(format!("{} gold", p.gold));
    }
    if p.style == PriceStyle::Fate {
        parts.push("and something more, which you learn only once it is taken".to_string());
    }
    parts.push(format!("{} favor", p.favor));
    parts.join(", ")
}

fn components_line(c: &[crate::types::ComponentReq]) -> String {
    c.iter()
        .map(|r| {
            if r.min_quality > 0 {
                format!("{} x{} (quality {}+)", r.category, r.qty, r.min_quality)
            } else {
                format!("{} x{}", r.category, r.qty)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn shape_line(d: &SpellDraft) -> String {
    let mut parts = Vec::new();
    if let Some(t) = &d.spell_type {
        parts.push(t.clone());
    }
    if let Some(e) = d.element.as_ref().or(d.effect.as_ref()) {
        parts.push(e.replace('_', " "));
    }
    if let Some(t) = &d.target {
        parts.push(format!("-> {}", t.replace('_', " ")));
    }
    if let Some(m) = d.magnitude {
        parts.push(format!("strength {m}"));
    }
    if let Some(s) = d.duration {
        parts.push(format!("{s}s"));
    }
    parts.join(" ")
}

/// The running summary: shape so far, and once the design is complete, its
/// power, cost to cast, price and components.
fn summary(d: &SpellDraft, god: &God) -> String {
    let mut out = String::new();
    if let Some(n) = &d.name {
        out.push_str(&format!("  \x1b[1m{n}\x1b[0m\n"));
    }
    let shape = shape_line(d);
    if !shape.is_empty() {
        out.push_str(&format!("  Shape:      {shape}\n"));
    }
    if let Some(design) = draft::chosen_design(&god.designs, d) {
        out.push_str(&format!(
            "  Power:      {} (tier {})   Mana {}   Cooldown {}s\n",
            design.power,
            tier(design.power),
            draft::mana_for(design.power),
            draft::cooldown_for(design.power, &design.spell_type)
        ));
        let price = draft::price(design.power, &god.cfg, &god.settings);
        out.push_str(&format!("  Price:      {}\n", price_line(&price)));
        let comps = draft::components_for(&design, &god.vnum, god.domain.floor, &god.settings);
        out.push_str(&format!("  Components: {}\n", components_line(&comps)));
    }
    if let Some(desc) = &d.description {
        out.push_str(&format!("  \"{desc}\"\n"));
    }
    for (label, line) in [
        ("You see", &d.flavor.caster),
        ("Target", &d.flavor.target),
        ("Others", &d.flavor.room),
    ] {
        if !line.is_empty() {
            out.push_str(&format!("  {label}: {line}\n"));
        }
    }
    out
}

fn prompt(d: &SpellDraft, god: &God) -> String {
    let mut out = format!("\x1b[1;36m{}\x1b[0m\n", question(d.step, &god.name));
    if is_menu(d.step) {
        for (i, c) in choices(d.step, d, god).iter().enumerate() {
            out.push_str(&format!("  {}) {}\n", i + 1, c.label));
        }
    }
    if d.step == DraftStep::Review {
        out = format!("{}{}", summary(d, god), out);
    }
    out
}

const HELP: &str = "While petitioning: answer with a number or a word; 'back' changes your last answer, \
'show' shows the draft, 'offer' (at the end) lays it before your god, 'abort' abandons it.\n";

// ---------- entry points ----------

fn spell_table(state: &SharedState) -> std::collections::HashMap<String, crate::types::SpellDefinition> {
    state.lock().spell_definitions.clone()
}

/// Open the petition mode for an eligible worshipper. `None` when they are
/// not eligible (the caller shows the checklist instead).
pub fn begin(db: &Db, conns: &SharedConnections, conn_id: Uuid) -> Option<String> {
    let (ch, _) = session_state(conns, conn_id)?;
    let god_vnum = ch.worship.as_ref()?.god_vnum.clone();
    let settings = Settings::load(db);
    if !petition_status(db, &ch, &god_vnum, &settings, Clock::now(db)).can_petition() {
        return None;
    }
    let god = God::load(db, &god_vnum)?;
    if god.designs.is_empty() {
        return Some(format!(
            "{} hears you, but has nothing to grant that is worthy of the canon.\n",
            god.name
        ));
    }
    let mut d = SpellDraft {
        god_vnum,
        ..SpellDraft::default()
    };
    let notes = advance(&mut d, &god);
    let mut out = format!(
        "\x1b[1;36m{} turns its regard upon you. Shape your gift.\x1b[0m\n{}",
        god.name, HELP
    );
    for n in notes {
        out.push_str(&format!("{n}\n"));
    }
    out.push_str(&prompt(&d, &god));
    store(conns, conn_id, Some(d));
    Some(out)
}

/// One line typed while petitioning.
pub fn input(db: &Db, conns: &SharedConnections, state: &SharedState, conn_id: Uuid, line: &str) -> Reply {
    let Some((_ch, Some(mut d))) = session_state(conns, conn_id) else {
        store(conns, conn_id, None);
        return reply("You are not petitioning.\n");
    };
    let Some(god) = God::load(db, &d.god_vnum) else {
        store(conns, conn_id, None);
        return reply("Your god no longer grants spells. The petition fades.\n");
    };
    let line = line.trim();
    let word = line.to_ascii_lowercase();

    match word.as_str() {
        "abort" => {
            store(conns, conn_id, None);
            return reply("You let the petition go unspoken.\n");
        }
        "help" | "?" => return reply(format!("{HELP}{}", prompt(&d, &god))),
        "show" | "summary" => return reply(format!("{}{}", summary(&d, &god), prompt(&d, &god))),
        "back" => {
            if !back(&mut d) {
                return reply(format!("There is nothing earlier to change.\n{}", prompt(&d, &god)));
            }
            let notes = advance(&mut d, &god);
            let out = notes.iter().map(|n| format!("{n}\n")).collect::<String>() + prompt(&d, &god).as_str();
            store(conns, conn_id, Some(d));
            return reply(out);
        }
        _ => {}
    }

    let step = d.step;
    if step == DraftStep::Review {
        if word != "offer" {
            return reply(prompt(&d, &god));
        }
        return match validate(&d, &god, state) {
            Err(why) => reply(format!("{why}\n{}", prompt(&d, &god))),
            Ok(design) => Reply {
                text: String::new(),
                offer: Some(OfferPreview {
                    name: d.name.clone().unwrap_or_default(),
                    spell_type: design.spell_type.clone(),
                    power: design.power,
                    tier: tier(design.power),
                }),
            },
        };
    }

    if let Err(why) = answer(step, &mut d, line, &god, state) {
        return reply(format!("{why}\n{}", prompt(&d, &god)));
    }
    d.history.push(step);
    let notes = advance(&mut d, &god);
    let mut out = String::new();
    if is_menu(step) {
        let shape = shape_line(&d);
        out.push_str(&format!("  So far: {shape}\n"));
    }
    for n in notes {
        out.push_str(&format!("{n}\n"));
    }
    out.push_str(&prompt(&d, &god));
    store(conns, conn_id, Some(d));
    reply(out)
}

/// Apply `line` as the answer to `step`.
fn answer(step: DraftStep, d: &mut SpellDraft, line: &str, god: &God, state: &SharedState) -> Result<(), String> {
    if is_menu(step) {
        let opts = choices(step, d, god);
        let pick = line
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| opts.get(i))
            .or_else(|| {
                opts.iter()
                    .find(|c| c.value.eq_ignore_ascii_case(line) || c.label.eq_ignore_ascii_case(line))
            });
        let Some(pick) = pick else {
            return Err("Choose one of the numbers below.".to_string());
        };
        let value = pick.value.clone();
        set_choice(step, d, &value);
        return Ok(());
    }
    match step {
        DraftStep::Name => {
            let spells = spell_table(state);
            text::validate_spell_name(line, &spells, None).map_err(|e| e.message())?;
            d.name = Some(line.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        DraftStep::Description => {
            let desc = text::sanitize_description(line);
            if desc.is_empty() {
                return Err("Say something of it.".to_string());
            }
            d.description = Some(desc);
        }
        DraftStep::FlavorCaster | DraftStep::FlavorTarget | DraftStep::FlavorRoom => {
            let text = if line.eq_ignore_ascii_case("none") || line == "-" {
                String::new()
            } else {
                text::sanitize_flavor(line)
            };
            match step {
                DraftStep::FlavorCaster => d.flavor.caster = text,
                DraftStep::FlavorTarget => d.flavor.target = text,
                _ => d.flavor.room = text,
            }
        }
        _ => {}
    }
    Ok(())
}

/// Everything an offer needs to be true of the draft itself.
fn validate(d: &SpellDraft, god: &God, state: &SharedState) -> Result<Design, String> {
    let design = draft::chosen_design(&god.designs, d)
        .ok_or_else(|| format!("{} will not grant that as it stands.", god.name))?;
    let name = d.name.as_deref().ok_or("Your gift needs a name.")?;
    text::validate_spell_name(name, &spell_table(state), None).map_err(|e| e.message())?;
    if d.description.as_deref().unwrap_or("").is_empty() {
        return Err("Your gift needs a description.".to_string());
    }
    Ok(design)
}

/// Lay the reviewed draft before the god (after its `OnPray` trigger, action
/// `petition`, did not veto). In order: re-check eligibility and the draft,
/// pick the trial, claim the god's cooldown and a canon slot (CAS), set the
/// trial quest, freeze the petition.
/// Claim the god's petition cooldown (with canon room), offer the trial
/// quest, then freeze `frozen` onto the character. Every failure releases
/// what was claimed. Shared by petitions and restorations.
#[allow(clippy::too_many_arguments)]
pub(crate) fn claim_and_set_trial(
    db: &Db,
    conns: &SharedConnections,
    god_vnum: &str,
    god_name: &str,
    cfg: &DeitySpellcraft,
    settings: &Settings,
    char_name: &str,
    quest: &crate::types::QuestData,
    clock: Clock,
    frozen: FrozenPetition,
) -> Result<FrozenPetition, String> {
    // Claim: the god hears one petition per cooldown, and only with room in
    // its canon. Exclusive under concurrent offers.
    let cooldown = cfg
        .petition_cooldown_days
        .unwrap_or(settings.god_petition_cooldown_days);
    let (today, grace, cap) = (clock.today, settings.canon_grace_days, cfg.canon_cap);
    let mut previous = None;
    let claimed = db.update_canon(god_vnum, |c| {
        let ready = c.last_petition_day.is_none_or(|day| today >= day + cooldown);
        if !ready || !c.has_room(cap, today, grace) {
            return false;
        }
        previous = Some(c.last_petition_day);
        c.god_vnum = god_vnum.to_string();
        c.last_petition_day = Some(today);
        true
    });
    if !matches!(claimed, Ok(Some(_))) {
        let canon = db.get_canon(god_vnum).unwrap_or_default();
        return Err(if !canon.has_room(cap, today, grace) {
            format!("{god_name}'s canon is full, and every gift in it is still new. Not now.\n")
        } else {
            format!("{god_name} is already weighing another's petition.\n")
        });
    }
    let release_claim = || {
        let _ = db.update_canon(god_vnum, |c| {
            if c.last_petition_day != Some(today) {
                return false;
            }
            c.last_petition_day = previous.flatten();
            true
        });
    };

    let refused = crate::quest::offer(db, conns, char_name, &quest.vnum);
    if !refused.is_empty() {
        release_claim();
        return Err(format!("{god_name} would set you a trial, but: {refused}\n"));
    }
    let saved = crate::script::achievements::apply_to_character(db, conns, char_name, |c| {
        c.pending_petition = Some(frozen.clone());
        c.last_petition_at = Some(clock.now_secs);
        true
    });
    if saved.is_none() {
        release_claim();
        let _ = crate::quest::abandon(db, conns, char_name, &quest.vnum);
        return Err("The petition slips away. Try again.\n".to_string());
    }
    Ok(frozen)
}

pub fn offer(db: &Db, conns: &SharedConnections, state: &SharedState, conn_id: Uuid) -> String {
    let Some((ch, Some(d))) = session_state(conns, conn_id) else {
        return "You are not petitioning.\n".to_string();
    };
    let Some(god) = God::load(db, &d.god_vnum) else {
        store(conns, conn_id, None);
        return "Your god no longer grants spells. The petition fades.\n".to_string();
    };
    if d.step != DraftStep::Review {
        return "Finish shaping the gift first.\n".to_string();
    }
    let in_temple = db
        .get_room_data(&ch.current_room_id)
        .ok()
        .flatten()
        .is_some_and(|r| r.flags.temple);
    if !in_temple {
        return "Petitions are heard only in a temple.\n".to_string();
    }
    let clock = Clock::now(db);
    let status = petition_status(db, &ch, &god.vnum, &god.settings, clock);
    if !status.can_petition() {
        store(conns, conn_id, None);
        let unmet: Vec<String> = status
            .requirements
            .iter()
            .filter(|r| !r.met)
            .map(|r| r.label.clone())
            .collect();
        return format!(
            "{} will not hear you now ({}). The petition fades.\n",
            god.name,
            unmet.join("; ")
        );
    }
    let design = match validate(&d, &god, state) {
        Ok(x) => x,
        Err(why) => return format!("{why}\n"),
    };
    let Some(rule) = draft::pick_trial(&god.cfg.trial_rules, &design) else {
        return format!("{} has no task for you yet. (Its trials are not written.)\n", god.name);
    };
    let Some(quest) = db.get_quest_data(&rule.quest_id).ok().flatten() else {
        tracing::warn!(god = %god.vnum, quest = %rule.quest_id, "spellcraft trial quest does not exist");
        return format!("{} has no task for you yet. (Its trials are not written.)\n", god.name);
    };

    let spell = draft::to_definition(&design, &d, &text::codex_id_for(d.name.as_deref().unwrap_or("")));
    let frozen = FrozenPetition {
        price: draft::price(design.power, &god.cfg, &god.settings),
        components: draft::components_for(&design, &god.vnum, god.domain.floor, &god.settings),
        power: design.power,
        tier: tier(design.power),
        draft: d,
        spell,
        trial_quest_id: quest.vnum.clone(),
        offered_day: clock.today,
        restoring: None,
    };
    let frozen = match claim_and_set_trial(
        db,
        conns,
        &god.vnum,
        &god.name,
        &god.cfg,
        &god.settings,
        &ch.name,
        &quest,
        clock,
        frozen,
    ) {
        Ok(f) => f,
        Err(why) => return why,
    };
    store(conns, conn_id, None);
    let mut out = format!(
        "\x1b[1;36m{} accepts your petition for {}.\x1b[0m\nYour trial: \x1b[1m{}\x1b[0m\n",
        god.name, frozen.spell.name, quest.name
    );
    if !quest.description.is_empty() {
        out.push_str(&format!("{}\n", quest.description));
    }
    out += "When it is done, return to a temple of your god.\n";
    out
}

/// A line about the character's offered petition, or `None` if they have
/// none. Drops a petition whose trial was abandoned or expired.
pub fn pending_status(db: &Db, conns: &SharedConnections, char_name: &str) -> Option<String> {
    let ch = crate::script::achievements::apply_to_character(db, conns, char_name, |_| false)?;
    if let Some(line) = super::ritual::status_line(&ch, &Settings::load(db)) {
        return Some(line);
    }
    let p = ch.pending_petition.as_ref()?;
    let quest_name = db
        .get_quest_data(&p.trial_quest_id)
        .ok()
        .flatten()
        .map(|q| q.name)
        .unwrap_or_else(|| p.trial_quest_id.clone());
    if ch.completed_quests.contains(&p.trial_quest_id) && !ch.active_quests.contains_key(&p.trial_quest_id) {
        return Some(format!(
            "You have passed the trial for {}. Begin the rite that seals it with 'pray ritual' in a temple.\n",
            p.spell.name
        ));
    }
    if ch.active_quests.contains_key(&p.trial_quest_id) {
        return Some(format!(
            "Your petition for {} awaits its trial: {}.\n",
            p.spell.name, quest_name
        ));
    }
    let name = p.spell.name.clone();
    crate::script::achievements::apply_to_character(db, conns, char_name, |c| c.pending_petition.take().is_some());
    Some(format!(
        "You let the trial for {name} lapse, and the petition with it. You may petition again once your god will hear you.\n"
    ))
}
