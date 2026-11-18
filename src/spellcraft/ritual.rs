//! The sealing ritual (§5): the push-your-luck loop that turns an accepted
//! petition into a codex spell.
//!
//! `pray ritual` claims the components (and Fortune gold) first, then puts a
//! [`RitualState`] on the ritualist. The ritual tick advances it a stage at a
//! time, drawing mana; at each stage boundary the ritualist chooses `pray
//! push` (more power, more pledge, more risk) or `pray seal`. Disturbances
//! raise the risk; a backlash can collapse the rite.
//!
//! All rolls go through [`Dice`] so tests are deterministic. Every character
//! write is session-first (`achievements::apply_to_character`); messages are
//! collected inside the write and sent after the lock is released.

use std::collections::VecDeque;

use rand::{Rng, SeedableRng};
use uuid::Uuid;

use super::{Settings, draft, price};
use crate::db::Db;
use crate::script::achievements::{apply_to_character, send_to_player};
use crate::types::{
    AidRecord, CanonStatus, CharacterData, CharacterPosition, CodexEntry, CodexMeta, ComponentReq, DivineMark,
    EffectType, FrozenPetition, ItemAffect, ItemData, LineageEvent, LineageKind, MarkState, MarkTemplate,
    PledgeAmounts, PledgeStatus, Price, PriceStyle, RitualState, SpellDefinition, SpellOrigin,
};
use crate::{SharedConnections, SharedState};

/// How often the ritual tick runs.
pub const RITUAL_TICK_SECS: u64 = 2;
/// Spellcraft XP a helper earns per `pray aid`.
pub const AID_XP: i32 = 25;
pub const AUTHORED_COUNTER: &str = "spells.authored";
/// Achievement counter bumped per restoration sealed.
pub const RESTORED_COUNTER: &str = "spells.restored";
pub const AIDED_COUNTER: &str = "spells.aided";
/// Percent of magnitude (damage/heal) each kept push adds.
const PUSH_MAGNITUDE_PCT: i64 = 15;
/// Percent of duration (buff/debuff) each kept push adds.
const PUSH_DURATION_PCT: i64 = 25;
/// The regen penalty a Weariness price brands into the mark.
const WEARINESS_REGEN_PENALTY: i32 = 15;
/// How often the tick refreshes `last_seen_at` (it is only a linkdead clock).
const SEEN_REFRESH_SECS: i64 = 30;

/// Free while a ritual runs: talk, look at yourself, pray.
const FREE: &[&str] = &[
    "say",
    "gtell",
    "tell",
    "whisper",
    "emote",
    "ftalk",
    "clanchat",
    "speak",
    "score",
    "status",
    "look",
    "inventory",
    "inv",
    "eq",
    "equipment",
    "time",
    "who",
    "spells",
    "pray",
    "help",
    "skills",
    "exits",
    "weather",
    "quest",
    "quests",
    "prompt",
    "socials",
];
/// Major for the ritualist: moving, fighting, sleeping.
const MAJOR: &[&str] = &[
    "go", "enter", "flee", "retreat", "recall", "sleep", "attack", "kill", "backstab", "bash", "circle", "shoot",
    "snipe", "assist", "drag",
];
/// Another player's words in the room break the ritualist's focus.
const SPEECH: &[&str] = &["say", "shout", "emote", "yell"];

const MINOR_LINES: &[&str] = &[
    "The chant slips from your tongue as a voice cuts across the nave.",
    "Your focus wavers; the fire in your palms gutters.",
    "A footfall breaks the hush, and the words you were holding scatter.",
];

// ---------- dice ----------

/// Every roll the ritual makes.
pub trait Dice {
    fn d6(&mut self) -> u8;
    /// 1-100 (heresy rolls).
    fn d100(&mut self) -> u8;
}

/// The live dice.
pub struct ThreadDice;

impl Dice for ThreadDice {
    fn d6(&mut self) -> u8 {
        rand::thread_rng().gen_range(1..=6)
    }
    fn d100(&mut self) -> u8 {
        rand::thread_rng().gen_range(1..=100)
    }
}

/// Seeded dice, for distribution tests.
pub struct SeededDice(rand::rngs::StdRng);

impl SeededDice {
    pub fn new(seed: u64) -> Self {
        Self(rand::rngs::StdRng::seed_from_u64(seed))
    }
}

impl Dice for SeededDice {
    fn d6(&mut self) -> u8 {
        self.0.gen_range(1..=6)
    }
    fn d100(&mut self) -> u8 {
        self.0.gen_range(1..=100)
    }
}

/// Scripted dice: rolls the queue in order, then sixes... except that a six
/// on the misfire table is a collapse, so an empty queue rolls 2 (no misfire
/// on a risk die, "lose the gain" on the table).
pub struct FixedDice(pub VecDeque<u8>);

impl FixedDice {
    pub fn new(rolls: &[u8]) -> Self {
        Self(rolls.iter().copied().collect())
    }
}

impl Dice for FixedDice {
    fn d6(&mut self) -> u8 {
        self.0.pop_front().unwrap_or(2)
    }
    /// Shares the queue; an empty queue rolls 100 (every heresy roll misses).
    fn d100(&mut self) -> u8 {
        self.0.pop_front().unwrap_or(100)
    }
}

// ---------- notes ----------

/// A message produced inside a character write, delivered after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Me(String),
    Room(Uuid, String),
    All(String),
}

fn deliver(connections: &SharedConnections, name: &str, notes: Vec<Note>) {
    for n in notes {
        match n {
            Note::Me(t) => send_to_player(connections, name, &format!("{t}\n")),
            Note::Room(room, t) => crate::session::broadcast_to_room(connections, room, format!("{t}\n"), Some(name)),
            Note::All(t) => crate::session::broadcast_to_all_players(connections, &format!("{t}\n"), None),
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------- shared pure pieces ----------

/// Mana one stage draws.
pub fn stage_mana(p: &FrozenPetition) -> i32 {
    (p.spell.mana_cost / 2).max(10)
}

/// The spell with its kept pushes applied: stronger for the same mana.
pub fn empowered(def: &SpellDefinition, power_bonus: u8) -> SpellDefinition {
    let mut d = def.clone();
    let b = power_bonus as i64;
    let scale = |v: i32, pct: i64| ((v as i64 * (100 + pct * b)) / 100) as i32;
    d.damage_base = scale(d.damage_base, PUSH_MAGNITUDE_PCT);
    d.heal_base = scale(d.heal_base, PUSH_MAGNITUDE_PCT);
    d.buff_duration_secs = scale(d.buff_duration_secs, PUSH_DURATION_PCT);
    d
}

fn trial_passed(ch: &CharacterData, p: &FrozenPetition) -> bool {
    ch.completed_quests.contains(&p.trial_quest_id) && !ch.active_quests.contains_key(&p.trial_quest_id)
}

/// What the backlash table did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backlash {
    LoseGain,
    Trauma(i32),
    ExtraMark,
    Collapse,
}

/// Roll the misfire table (§5) and apply everything but a collapse.
pub(crate) fn roll_backlash(ch: &mut CharacterData, dice: &mut dyn Dice) -> Backlash {
    let roll = dice.d6();
    let max_hp = ch.effective_max_hp();
    let Some(r) = ch.pending_ritual.as_mut() else {
        return Backlash::Collapse;
    };
    match roll {
        1 | 2 => {
            r.power_bonus = r.power_bonus.saturating_sub(1);
            Backlash::LoseGain
        }
        3 | 4 => {
            let dmg = (max_hp / 5).max(1);
            ch.hp = (ch.hp - dmg).max(1);
            Backlash::Trauma(dmg)
        }
        5 => {
            r.extra_marks = r.extra_marks.saturating_add(1);
            Backlash::ExtraMark
        }
        _ => Backlash::Collapse,
    }
}

fn backlash_note(b: Backlash) -> String {
    match b {
        Backlash::LoseGain => "\x1b[1;31mBacklash!\x1b[0m The power you won slips back out of reach.".to_string(),
        Backlash::Trauma(n) => format!("\x1b[1;31mBacklash!\x1b[0m Holy fire turns inward, searing you for {n}."),
        Backlash::ExtraMark => {
            "\x1b[1;31mBacklash!\x1b[0m You feel the god's brand settle deeper; it will mark you twice.".to_string()
        }
        Backlash::Collapse => "\x1b[1;31mBacklash!\x1b[0m The rite tears apart.".to_string(),
    }
}

/// End the rite as a failure: the components are already gone, half the
/// HP pledge is taken anyway (growing back from today), the petition is
/// void and the god's cooldown stays spent. Pure.
pub(crate) fn collapse(ch: &mut CharacterData, today: i64, s: &Settings, why: &str) -> Vec<Note> {
    let Some(r) = ch.pending_ritual.take() else {
        return Vec::new();
    };
    let spell = ch
        .pending_petition
        .take()
        .map(|p| p.spell.name)
        .unwrap_or_else(|| "the gift".to_string());
    let half = (r.price.hp + r.pledge_accum) / 2;
    let mut taken = 0;
    if half > 0 {
        let amounts = PledgeAmounts {
            hp: half,
            ..Default::default()
        };
        if let Ok(Some(id)) = price::push_pledge(ch, "", &r.god_vnum, amounts, today, s.pledge_floor_pct, true)
            && let Some(p) = ch.divine_pledges.iter_mut().find(|p| p.id == id)
        {
            taken = p.amounts.hp;
            p.status = PledgeStatus::Regrowing { from_day: today };
        }
    }
    let mut me = format!("\x1b[1;31m{why} The rite collapses, and {spell} is lost with it.\x1b[0m");
    if taken > 0 {
        me.push_str(&format!(
            "\nThe god takes {taken} of your vigour for the failure; it will grow back in time."
        ));
    }
    vec![
        Note::Me(me),
        Note::Room(
            r.room_id,
            format!("The light over {} gutters out. The rite has failed.", ch.name),
        ),
    ]
}

/// A minor disturbance: at most one per stage, paid at the boundary.
pub(crate) fn minor(ch: &mut CharacterData) -> Vec<Note> {
    match ch.pending_ritual.as_mut() {
        Some(r) if !r.disturbed_minor => {
            r.disturbed_minor = true;
            vec![Note::Me("Your concentration flickers.".to_string())]
        }
        _ => Vec::new(),
    }
}

/// A major disturbance: +2 risk and an instant backlash roll.
pub(crate) fn major(ch: &mut CharacterData, today: i64, s: &Settings, dice: &mut dyn Dice, what: &str) -> Vec<Note> {
    let Some(r) = ch.pending_ritual.as_mut() else {
        return Vec::new();
    };
    r.risk = r.risk.saturating_add(2);
    let mut notes = vec![Note::Me(format!("\x1b[1;33m{what} The rite shudders.\x1b[0m"))];
    let b = roll_backlash(ch, dice);
    notes.push(Note::Me(backlash_note(b)));
    if b == Backlash::Collapse {
        notes.extend(collapse(ch, today, s, "Your concentration breaks."));
    }
    notes
}

fn prompt_line(r: &RitualState, s: &Settings) -> String {
    let push = if (r.pushes as i64) < s.ritual_max_pushes {
        format!("'pray push' to deepen it (risk {}d6)", r.risk + 1)
    } else {
        "the god will give no more".to_string()
    };
    format!(
        "\x1b[1;36mThe rite reaches a pause (stage {}).\x1b[0m 'pray seal' to inscribe the spell, or {push}.",
        r.stage
    )
}

/// One tick for one ritualist (pure). Returns (changed, notes).
pub fn tick_one(ch: &mut CharacterData, now: i64, today: i64, s: &Settings, dice: &mut dyn Dice) -> (bool, Vec<Note>) {
    let Some(r) = ch.pending_ritual.as_ref() else {
        return (false, Vec::new());
    };
    if ch.current_room_id != r.room_id {
        return (true, collapse(ch, today, s, "You walk out of the rite."));
    }
    if ch.hp <= 0 {
        return (true, collapse(ch, today, s, "Your life gutters."));
    }
    if r.last_seen_at > 0 && now - r.last_seen_at > s.ritual_linkdead_secs {
        return (true, collapse(ch, today, s, "The rite cooled while you were away."));
    }
    let mut changed = false;
    let mut notes = Vec::new();
    let r = ch.pending_ritual.as_mut().expect("checked");
    if now - r.last_seen_at >= SEEN_REFRESH_SECS {
        r.last_seen_at = now;
        changed = true;
    }

    // Fighting and sleeping are major, once per bout.
    let fighting = ch.combat.in_combat;
    let asleep = ch.position == CharacterPosition::Sleeping;
    let r = ch.pending_ritual.as_mut().expect("checked");
    if r.noted_combat != fighting || r.noted_sleep != asleep {
        let start_fight = fighting && !r.noted_combat;
        let fall_asleep = asleep && !r.noted_sleep;
        r.noted_combat = fighting;
        r.noted_sleep = asleep;
        changed = true;
        if start_fight {
            notes.extend(major(ch, today, s, dice, "Violence breaks over the altar!"));
        }
        if fall_asleep && ch.pending_ritual.is_some() {
            notes.extend(major(ch, today, s, dice, "Sleep drags at you mid-chant!"));
        }
        if ch.pending_ritual.is_none() {
            return (true, notes);
        }
    }

    let Some(p) = ch.pending_petition.clone() else {
        return (true, collapse(ch, today, s, "The petition is gone."));
    };
    let r = ch.pending_ritual.as_ref().expect("checked");
    if now < r.next_stage_at {
        return (changed, notes);
    }

    // The stage ends: draw its mana, helpers' first.
    let cost = stage_mana(&p);
    let from_pool = r.mana_pool.min(cost);
    let from_me = cost - from_pool;
    if ch.mana < from_me {
        notes.extend(collapse(ch, today, s, "Your mana runs dry."));
        return (true, notes);
    }
    ch.mana -= from_me;
    let r = ch.pending_ritual.as_mut().expect("checked");
    r.mana_pool -= from_pool;
    r.next_stage_at = now + s.ritual_stage_secs.max(1);
    if r.at_boundary {
        notes.push(Note::Me(format!(
            "The rite holds, drinking {cost} mana. 'pray seal' or 'pray push'."
        )));
        return (true, notes);
    }
    if r.disturbed_minor {
        r.disturbed_minor = false;
        r.risk = r.risk.saturating_add(1);
        let line = MINOR_LINES[(r.stage as usize) % MINOR_LINES.len()];
        notes.push(Note::Me(format!("\x1b[0;33m{line}\x1b[0m (+1 risk)")));
    }
    r.stage = r.stage.saturating_add(1);
    r.at_boundary = true;
    notes.push(Note::Me(prompt_line(r, s)));
    notes.push(Note::Room(
        r.room_id,
        format!("The air above {} brightens; the rite has reached a pause.", ch.name),
    ));
    (true, notes)
}

// ---------- starting the rite ----------

fn components_text(reqs: &[ComponentReq]) -> String {
    reqs.iter()
        .map(|c| {
            if c.min_quality > 0 {
                format!("{} x{} (quality {}+)", c.category, c.qty, c.min_quality)
            } else {
                format!("{} x{}", c.category, c.qty)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Pick items from `items` for every requirement, poorest sufficient first.
/// `Err` lists what is missing.
pub fn plan_components(items: &[ItemData], reqs: &[ComponentReq]) -> Result<Vec<Uuid>, String> {
    let mut sorted: Vec<&ItemData> = items.iter().collect();
    sorted.sort_by_key(|i| i.quality);
    let mut picked: Vec<Uuid> = Vec::new();
    let mut missing = Vec::new();
    for req in reqs {
        let mut got = 0;
        for item in &sorted {
            if got >= req.qty {
                break;
            }
            if picked.contains(&item.id) || item.quality < req.min_quality {
                continue;
            }
            if item.categories.iter().any(|c| c.eq_ignore_ascii_case(&req.category)) {
                picked.push(item.id);
                got += 1;
            }
        }
        if got < req.qty {
            missing.push(ComponentReq {
                qty: req.qty - got,
                ..req.clone()
            });
        }
    }
    if missing.is_empty() {
        Ok(picked)
    } else {
        Err(components_text(&missing))
    }
}

/// Claim (delete) every planned item. Exactly-once: only the deletes this
/// call made count; on any failure the claimed ones are put back.
fn claim_components(db: &Db, ids: &[Uuid]) -> Result<Vec<ItemData>, ()> {
    let mut claimed = Vec::new();
    for id in ids {
        let item = db.get_item_data(id).ok().flatten();
        match (item, db.delete_item(id)) {
            (Some(item), Ok(true)) => claimed.push(item),
            _ => {
                restore_components(db, &claimed);
                return Err(());
            }
        }
    }
    Ok(claimed)
}

fn restore_components(db: &Db, items: &[ItemData]) {
    for item in items {
        let _ = db.save_item_data(item.clone());
    }
}

/// The price as it will be paid: `Fate` picks a style now (kept hidden
/// until sealing).
fn resolve_price(db: &Db, p: &FrozenPetition, dice: &mut dyn Dice) -> Price {
    if p.price.style != PriceStyle::Fate {
        return p.price.clone();
    }
    let styles = [
        PriceStyle::Flesh,
        PriceStyle::Mind,
        PriceStyle::Essence,
        PriceStyle::Fortune,
        PriceStyle::Weariness,
    ];
    let style = styles[(dice.d6() as usize - 1) % styles.len()];
    let mut cfg = crate::script::worship::deity_config_by_vnum(db, &p.draft.god_vnum)
        .and_then(|d| d.spellcraft)
        .unwrap_or_default();
    cfg.price_style = style;
    cfg.favor_cost = p.price.favor;
    draft::price(p.power, &cfg, &Settings::load(db))
}

/// `pray ritual`: begin the sealing rite.
pub fn begin(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    name: &str,
    now: i64,
    dice: &mut dyn Dice,
) -> String {
    let Some(ch) = apply_to_character(db, connections, name, |_| false) else {
        return "You are nowhere.\n".to_string();
    };
    if ch.pending_ritual.is_some() {
        return "Your rite is already under way.\n".to_string();
    }
    let Some(p) = ch.pending_petition.clone() else {
        return "You have no accepted petition to seal.\n".to_string();
    };
    if !trial_passed(&ch, &p) {
        return "Your god has not seen your trial done.\n".to_string();
    }
    if ch.worship.as_ref().map(|w| w.god_vnum.as_str()) != Some(p.draft.god_vnum.as_str()) {
        return "You no longer serve the god who heard this petition.\n".to_string();
    }
    let in_temple = db
        .get_room_data(&ch.current_room_id)
        .ok()
        .flatten()
        .filter(|r| r.flags.temple);
    let Some(room) = in_temple else {
        return "The rite can be held only in a temple.\n".to_string();
    };
    if ch.combat.in_combat || ch.position == CharacterPosition::Sleeping {
        return "You cannot begin a rite like this.\n".to_string();
    }
    let s = Settings::load(db);
    let mut price = resolve_price(db, &p, dice);

    // Claim first: the components, then any gold. Nothing starts on failure.
    let items = db.get_items_in_inventory(name).unwrap_or_default();
    let ids = match plan_components(&items, &p.components) {
        Ok(ids) => ids,
        Err(missing) => return format!("The rite needs what you do not carry: {missing}.\n"),
    };
    let Ok(claimed) = claim_components(db, &ids) else {
        return "Your components shift in your pack; try again.\n".to_string();
    };
    if price.gold > 0 && !crate::purse::debit_gold(db, connections, name, price.gold) {
        if p.price.style == PriceStyle::Fate {
            // Fate's hidden choice can't be Fortune for a pauper.
            let mut cfg = crate::script::worship::deity_config_by_vnum(db, &p.draft.god_vnum)
                .and_then(|d| d.spellcraft)
                .unwrap_or_default();
            cfg.price_style = PriceStyle::Flesh;
            cfg.favor_cost = p.price.favor;
            price = draft::price(p.power, &cfg, &s);
        } else {
            restore_components(db, &claimed);
            return format!("The rite asks a tribute of {} gold, and you lack it.\n", price.gold);
        }
    }

    let ritual = RitualState {
        god_vnum: p.draft.god_vnum.clone(),
        room_id: ch.current_room_id,
        stage: 0,
        at_boundary: false,
        power_bonus: 0,
        pushes: 0,
        risk: 0,
        next_stage_at: now + s.ritual_stage_secs.max(1),
        last_seen_at: now,
        disturbed_minor: false,
        pledge_accum: 0,
        extra_marks: 0,
        mana_pool: 0,
        aid: Vec::new(),
        witnesses: Vec::new(),
        claimed_components: claimed.clone(),
        price: price.clone(),
        noted_combat: false,
        noted_sleep: false,
    };
    let spell_id = p.spell.id.clone();
    let mut started = false;
    apply_to_character(db, connections, name, |c| {
        let same = c.pending_petition.as_ref().is_some_and(|q| q.spell.id == spell_id);
        if c.pending_ritual.is_some() || !same {
            return false;
        }
        c.pending_ritual = Some(ritual.clone());
        started = true;
        true
    });
    if !started {
        restore_components(db, &claimed);
        if price.gold > 0 {
            crate::purse::add_gold(db, connections, state, name, price.gold);
        }
        return "The rite will not begin. Try again.\n".to_string();
    }

    let god = price::god_name(db, &p.draft.god_vnum);
    crate::session::broadcast_to_all_players(
        connections,
        &format!(
            "\x1b[1;36mThe heavens stir over {}: {} begins a rite to {}.\x1b[0m\n",
            room.title, name, god
        ),
        None,
    );
    let mut out = format!(
        "You lay out {} and begin the rite of {}.\n",
        components_text(&p.components),
        p.spell.name
    );
    out.push_str(&format!(
        "Each stage takes {} seconds and draws {} mana. Stay in the temple, keep your focus, and do not fight or sleep.\n",
        s.ritual_stage_secs,
        stage_mana(&p)
    ));
    out
}

// ---------- push / seal / aid ----------

/// `pray push`: deepen the gift at a stage boundary.
pub fn push(db: &Db, connections: &SharedConnections, name: &str, now: i64, dice: &mut dyn Dice) -> String {
    let s = Settings::load(db);
    let today = crate::script::worship::current_absolute_day(db);
    let mut out = String::new();
    let mut notes = Vec::new();
    apply_to_character(db, connections, name, |ch| {
        let Some(r) = ch.pending_ritual.as_mut() else {
            out = "You are not performing a rite.\n".to_string();
            return false;
        };
        if !r.at_boundary {
            out = "Wait for the rite to reach a pause.\n".to_string();
            return false;
        }
        if ch.pending_petition.as_ref().is_some_and(|p| p.restoring.is_some()) {
            out = "A restoration re-lights what was; it cannot be pushed. Seal it.\n".to_string();
            return false;
        }
        let Some(r) = ch.pending_ritual.as_mut() else {
            return false;
        };
        if r.pushes as i64 >= s.ritual_max_pushes {
            out = "The god will give no more. Seal the spell.\n".to_string();
            return false;
        }
        let base_hp = r.price.hp;
        r.pushes += 1;
        r.power_bonus += 1;
        r.pledge_accum += (base_hp / 2).max(1);
        r.risk = r.risk.saturating_add(1);
        r.at_boundary = false;
        r.next_stage_at = now + s.ritual_stage_secs.max(1);
        let risk = r.risk;
        let room = r.room_id;
        notes.push(Note::Me(format!(
            "You press the god for more. The gift deepens (+{}), and so does your debt. Rolling {risk}d6...",
            r.power_bonus
        )));
        notes.push(Note::Room(
            room,
            format!("{} cries out, and the light over the altar flares.", ch.name),
        ));
        let misfired = (0..risk).map(|_| dice.d6()).any(|d| d == 1);
        if misfired {
            let b = roll_backlash(ch, dice);
            notes.push(Note::Me(backlash_note(b)));
            if b == Backlash::Collapse {
                notes.extend(collapse(ch, today, &s, "The god's power overruns you."));
            }
        } else {
            notes.push(Note::Me("The power holds.".to_string()));
        }
        true
    });
    deliver(connections, name, notes);
    out
}

/// Where a [`seal`] stops; tests use the earlier ones to simulate a crash
/// between steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SealStep {
    Codex,
    Canon,
    Character,
    Done,
}

/// `pray seal`: inscribe the spell. Safe to rerun after a crash at any step.
pub fn seal(db: &Db, connections: &SharedConnections, state: &SharedState, name: &str) -> String {
    seal_upto(db, connections, state, name, SealStep::Done)
}

fn roman(n: usize) -> &'static str {
    ["", "", " II", " III", " IV", " V", " VI", " VII", " VIII", " IX"]
        .get(n)
        .copied()
        .unwrap_or(" X")
}

/// The codex entry this author's rite wrote, or a fresh claim of one.
/// Each candidate id is claimed with an insert-if-absent; an id held by
/// someone else moves on to `<id>_2` and the name gains a numeral.
fn claim_codex(
    db: &Db,
    state: &SharedState,
    name: &str,
    p: &FrozenPetition,
    r: &RitualState,
    today: i64,
) -> Option<CodexEntry> {
    for n in 1..10usize {
        let id = if n == 1 {
            p.spell.id.clone()
        } else {
            format!("{}_{n}", p.spell.id)
        };
        if let Ok(Some(existing)) = db.get_codex(&id) {
            if existing.meta.author.as_deref() == Some(name)
                && existing.meta.god_vnum == p.draft.god_vnum
                && existing.meta.sealed_day >= p.offered_day
            {
                super::mirror_into_world(state, &existing);
                return Some(existing);
            }
            continue;
        }
        let mut def = empowered(&p.spell, r.power_bonus);
        def.id = id.clone();
        def.name = format!("{}{}", p.spell.name, roman(n));
        let taken = state
            .lock()
            .spell_definitions
            .values()
            .any(|d| d.id != id && d.name.eq_ignore_ascii_case(&def.name));
        if taken {
            continue;
        }
        let entry = CodexEntry {
            def,
            meta: CodexMeta {
                origin: SpellOrigin::Authored,
                author: Some(name.to_string()),
                god_vnum: p.draft.god_vnum.clone(),
                status: CanonStatus::Canon,
                sealed_day: today,
                lineage: vec![LineageEvent {
                    kind: LineageKind::Authored,
                    who: name.to_string(),
                    day: today,
                }],
                witnesses: r.witnesses.clone(),
                author_disgraced: false,
                flavor: p.draft.flavor.clone(),
                version: 1,
                vetoed: false,
                casts_by_others: 0,
            },
        };
        match super::insert_codex_spell(db, state, &entry) {
            Ok(true) => return Some(entry),
            Ok(false) => continue,
            Err(e) => {
                tracing::error!("codex insert failed for {}: {}", id, e);
                return None;
            }
        }
    }
    None
}

fn marks_for(db: &Db, p: &FrozenPetition, r: &RitualState, spell_id: &str) -> Vec<DivineMark> {
    let pool = crate::script::worship::deity_config_by_vnum(db, &p.draft.god_vnum)
        .and_then(|d| d.spellcraft)
        .map(|c| c.mark_pool)
        .unwrap_or_default();
    let god = price::god_name(db, &p.draft.god_vnum);
    let fallback = MarkTemplate {
        short_desc: format!("the sigil of {god}"),
        body_part: "brow".to_string(),
        affects: Vec::new(),
    };
    let seed: usize = spell_id.bytes().map(|b| b as usize).sum();
    (0..1 + r.extra_marks as usize)
        .map(|i| {
            let t = if pool.is_empty() {
                fallback.clone()
            } else {
                pool[(seed + i) % pool.len()].clone()
            };
            let mut affects = t.affects;
            if i == 0 && r.price.style == PriceStyle::Weariness {
                affects.push(ItemAffect {
                    effect_type: EffectType::RegenPenalty,
                    magnitude: WEARINESS_REGEN_PENALTY,
                    damage_type: None,
                    vs_effect: None,
                    skill_key: None,
                });
            }
            DivineMark {
                id: String::new(),
                god_vnum: p.draft.god_vnum.clone(),
                spell_id: Some(spell_id.to_string()),
                body_part: t.body_part,
                short_desc: t.short_desc,
                affects,
                worse_affect: None,
                state: MarkState::Live,
            }
        })
        .collect()
}

/// Essence: the highest base attribute loses a point, never below 3.
fn take_essence(ch: &mut CharacterData, points: i32) -> Option<&'static str> {
    if points <= 0 {
        return None;
    }
    let stats: [(&'static str, i32); 6] = [
        ("strength", ch.stat_str),
        ("dexterity", ch.stat_dex),
        ("constitution", ch.stat_con),
        ("intelligence", ch.stat_int),
        ("wisdom", ch.stat_wis),
        ("charisma", ch.stat_cha),
    ];
    let (label, value) = stats.iter().copied().max_by_key(|(_, v)| *v)?;
    let new = (value - points).max(3);
    if new == value {
        return None;
    }
    match label {
        "strength" => ch.stat_str = new,
        "dexterity" => ch.stat_dex = new,
        "constitution" => ch.stat_con = new,
        "intelligence" => ch.stat_int = new,
        "wisdom" => ch.stat_wis = new,
        _ => ch.stat_cha = new,
    }
    Some(label)
}

pub(crate) fn seal_upto(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    name: &str,
    upto: SealStep,
) -> String {
    let Some(ch) = apply_to_character(db, connections, name, |_| false) else {
        return "You are nowhere.\n".to_string();
    };
    let (Some(r), Some(p)) = (ch.pending_ritual.clone(), ch.pending_petition.clone()) else {
        return "You are not performing a rite.\n".to_string();
    };
    if !r.at_boundary {
        return "Wait for the rite to reach a pause.\n".to_string();
    }
    let s = Settings::load(db);
    let today = crate::script::worship::current_absolute_day(db);
    let god_vnum = p.draft.god_vnum.clone();

    // 1. The codex id and name, and the entry itself (one insert), or the
    // faded entry re-lit.
    let entry = match &p.restoring {
        Some(id) => super::canon::relight(db, state, name, id, &r.witnesses, p.offered_day, today),
        None => claim_codex(db, state, name, &p, &r, today),
    };
    let Some(entry) = entry else {
        return "The god's codex will not take the spell. Try again.\n".to_string();
    };
    let spell_id = entry.def.id.clone();
    if upto == SealStep::Codex {
        return String::new();
    }

    // 2. The canon slot; a full canon gives up its least-devoted spell.
    super::canon::take_slot(db, connections, state, &god_vnum, &spell_id, today);
    if upto == SealStep::Canon {
        return String::new();
    }

    // 3. Everything on the author, in one write: pledge, essence, favor,
    // marks, the spell itself, and the end of the rite.
    let marks = marks_for(db, &p, &r, &spell_id);
    let mut favor_shift = None;
    let mut essence = None;
    let mut pledged = PledgeAmounts::default();
    let petition_id = p.spell.id.clone();
    let mut wrote = false;
    apply_to_character(db, connections, name, |c| {
        let ours = c.pending_ritual.is_some() && c.pending_petition.as_ref().is_some_and(|q| q.spell.id == petition_id);
        if !ours {
            return false;
        }
        wrote = true;
        let amounts = PledgeAmounts {
            hp: r.price.hp + r.pledge_accum,
            stamina: r.price.stamina,
            mana: r.price.mana,
            resolve: 0,
        };
        if let Ok(Some(id)) = price::push_pledge(c, &spell_id, &god_vnum, amounts, today, s.pledge_floor_pct, true) {
            pledged = c
                .divine_pledges
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.amounts)
                .unwrap_or_default();
        }
        essence = take_essence(c, r.price.attribute);
        if let Some(w) = c.worship.as_mut()
            && r.price.favor > 0
        {
            let before = w.favor;
            w.favor = w.favor.saturating_sub(r.price.favor.clamp(0, i32::MAX as i64) as i32);
            favor_shift = Some((before, w.favor));
        }
        for m in &marks {
            price::push_mark(c, m.clone());
        }
        if !c.learned_spells.iter().any(|x| x == &spell_id) {
            c.learned_spells.push(spell_id.clone());
        }
        c.pending_ritual = None;
        c.pending_petition = None;
        true
    });
    if !wrote {
        return "The rite is already over.\n".to_string();
    }
    if upto == SealStep::Character {
        return String::new();
    }

    // 4. After the fact: announcements and credit.
    if let Some((before, after)) = favor_shift {
        crate::script::worship::announce_favor_shift(db, connections, name, &god_vnum, before, after);
    }
    let restored = p.restoring.is_some();
    let counter = if restored { RESTORED_COUNTER } else { AUTHORED_COUNTER };
    crate::script::achievements::notify_counter_core(db, connections, state, name, counter, 1);
    let god = price::god_name(db, &god_vnum);
    let verb = if restored { "restored" } else { "sealed" };
    crate::session::broadcast_to_all_players(
        connections,
        &format!(
            "\x1b[1;33m{name} has {verb} {} into the canon of {god}.\x1b[0m\n",
            entry.def.name
        ),
        None,
    );
    let mut out = format!(
        "\x1b[1;33mThe rite ends in a blaze of light. {} is written into {god}'s canon, and into you.\x1b[0m\n",
        entry.def.name
    );
    if p.price.style == PriceStyle::Fate {
        out.push_str(&format!("Fate chose the price: {}.\n", r.price.style.as_str()));
    }
    let mut paid = Vec::new();
    for (n, label) in [
        (pledged.hp, "max HP"),
        (pledged.stamina, "max stamina"),
        (pledged.mana, "max mana"),
        (pledged.resolve, "max resolve"),
    ] {
        if n > 0 {
            paid.push(format!("{n} {label}"));
        }
    }
    if !paid.is_empty() {
        out.push_str(&format!("You have pledged {} to {god}.\n", paid.join(", ")));
    }
    if let Some(stat) = essence {
        out.push_str(&format!("Something of your {stat} goes with it, for good.\n"));
    }
    if r.price.gold > 0 {
        out.push_str(&format!("Your tribute of {} gold is accepted.\n", r.price.gold));
    }
    for m in &marks {
        out.push_str(&format!(
            "You bear a new mark: {} on your {}.\n",
            m.short_desc, m.body_part
        ));
    }
    out
}

/// `pray aid <name>`: lend mana and steady a rite in your room.
pub fn aid(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    helper: &str,
    target: &str,
    dice: &mut dyn Dice,
) -> String {
    let Some(me) = apply_to_character(db, connections, helper, |_| false) else {
        return "You are nowhere.\n".to_string();
    };
    let wanted = target.to_lowercase();
    let found = {
        let conns = connections.lock();
        conns.values().find_map(|sess| {
            let c = sess.character.as_ref()?;
            let r = c.pending_ritual.as_ref()?;
            let p = c.pending_petition.as_ref()?;
            let matches = !wanted.is_empty() && c.name.to_lowercase().starts_with(&wanted);
            (matches && r.room_id == me.current_room_id && c.current_room_id == me.current_room_id)
                .then(|| (c.name.clone(), r.god_vnum.clone(), r.stage, stage_mana(p)))
        })
    };
    let Some((ritualist, god_vnum, stage, give)) = found else {
        return "No one here is performing a rite by that name.\n".to_string();
    };
    if ritualist.eq_ignore_ascii_case(helper) {
        return "You cannot aid your own rite; that is the rite.\n".to_string();
    }
    let same_faith = me.worship.as_ref().is_some_and(|w| w.god_vnum == god_vnum);

    // Claim the helper's mana first.
    let mut paid = false;
    apply_to_character(db, connections, helper, |c| {
        if c.mana < give {
            return false;
        }
        c.mana -= give;
        paid = true;
        true
    });
    if !paid {
        return format!("You need {give} mana to aid the rite.\n");
    }
    let contribution = if same_faith { give } else { give / 2 };
    let steady_roll = if same_faith { true } else { dice.d6() <= 3 };
    let mut steadied = false;
    let mut outcome = None;
    apply_to_character(db, connections, &ritualist, |c| {
        let Some(r) = c.pending_ritual.as_mut() else {
            return false;
        };
        if r.stage != stage
            || r.aid
                .iter()
                .any(|a| a.helper.eq_ignore_ascii_case(helper) && a.stage == stage)
        {
            outcome = Some(false);
            return false;
        }
        r.aid.push(AidRecord {
            helper: helper.to_string(),
            stage,
        });
        r.mana_pool += contribution;
        if r.disturbed_minor && steady_roll {
            r.disturbed_minor = false;
            steadied = true;
        }
        if same_faith && !r.witnesses.iter().any(|w| w.eq_ignore_ascii_case(helper)) {
            r.witnesses.push(helper.to_string());
        }
        outcome = Some(true);
        true
    });
    if outcome != Some(true) {
        apply_to_character(db, connections, helper, |c| {
            c.mana += give;
            true
        });
        return if outcome == Some(false) {
            "You have already aided this stage of the rite.\n".to_string()
        } else {
            "The rite ends before your aid reaches it.\n".to_string()
        };
    }
    crate::progress::award_xp(
        db,
        connections,
        state,
        helper,
        super::SPELLCRAFT_SKILL,
        AID_XP,
        crate::progress::XpSource::Ritual,
    );
    crate::script::achievements::notify_counter_core(db, connections, state, helper, AIDED_COUNTER, 1);
    let mut theirs = format!("{helper} lends you {contribution} mana for the rite.");
    if steadied {
        theirs.push_str(" Your focus steadies.");
    }
    deliver(connections, &ritualist, vec![Note::Me(theirs)]);
    crate::session::broadcast_to_room(
        connections,
        me.current_room_id,
        format!("{helper} kneels beside {ritualist} and joins the chant.\n"),
        Some(helper),
    );
    let mut out = format!("You give {give} mana to {ritualist}'s rite");
    if contribution < give {
        out.push_str(&format!(" (it takes only {contribution}: you serve another god)"));
    }
    out.push('.');
    if steadied {
        out.push_str(" You steady their focus.");
    }
    if same_faith {
        out.push_str(" You will be named a witness.");
    }
    out.push('\n');
    out
}

// ---------- disturbance hooks ----------

/// Ritualists in `room`, other than `except`.
fn ritualists_in(connections: &SharedConnections, room: Uuid, except: &str) -> Vec<String> {
    connections
        .lock()
        .values()
        .filter_map(|s| s.character.as_ref())
        .filter(|c| c.pending_ritual.is_some() && c.current_room_id == room && !c.name.eq_ignore_ascii_case(except))
        .map(|c| c.name.clone())
        .collect()
}

fn disturb_minor(db: &Db, connections: &SharedConnections, names: &[String]) {
    for n in names {
        let mut notes = Vec::new();
        apply_to_character(db, connections, n, |c| {
            notes = minor(c);
            !notes.is_empty()
        });
        deliver(connections, n, notes);
    }
}

/// Called by the dispatcher before a command runs. The ritualist's own
/// commands are free, minor or major; another's speech or social in the
/// room is minor for any ritualist there. Returns the actor's room, for
/// [`after_command`].
pub fn on_command(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    conn_id: Uuid,
    cmd: &str,
) -> Option<Uuid> {
    let (name, room, mine) = {
        let conns = connections.lock();
        let c = conns.get(&conn_id)?.character.as_ref()?;
        (c.name.clone(), c.current_room_id, c.pending_ritual.is_some())
    };
    let is_social = |cmd: &str| state.lock().socials.get(cmd).is_some();
    if mine {
        if FREE.contains(&cmd) || is_social(cmd) {
            return Some(room);
        }
        if MAJOR.contains(&cmd) {
            let s = Settings::load(db);
            let today = crate::script::worship::current_absolute_day(db);
            let mut notes = Vec::new();
            apply_to_character(db, connections, &name, |c| {
                notes = major(c, today, &s, &mut ThreadDice, "You break from the chant!");
                true
            });
            deliver(connections, &name, notes);
        } else {
            disturb_minor(db, connections, std::slice::from_ref(&name));
        }
        return Some(room);
    }
    let others = ritualists_in(connections, room, &name);
    if !others.is_empty() && (SPEECH.contains(&cmd) || is_social(cmd)) {
        disturb_minor(db, connections, &others);
    }
    Some(room)
}

/// Called after a command ran: anyone entering or leaving a room disturbs
/// the ritualists there.
pub fn after_command(db: &Db, connections: &SharedConnections, conn_id: Uuid, room_before: Uuid) {
    let (name, room_now) = {
        let conns = connections.lock();
        let Some(c) = conns.get(&conn_id).and_then(|s| s.character.as_ref()) else {
            return;
        };
        (c.name.clone(), c.current_room_id)
    };
    if room_now == room_before {
        return;
    }
    let mut names = ritualists_in(connections, room_before, &name);
    names.extend(ritualists_in(connections, room_now, &name));
    disturb_minor(db, connections, &names);
}

/// Is a rite under way for this player, or in their room? (Help lines.)
pub fn in_view(connections: &SharedConnections, conn_id: Uuid) -> bool {
    let conns = connections.lock();
    let Some(me) = conns.get(&conn_id).and_then(|s| s.character.as_ref()) else {
        return false;
    };
    let room = me.current_room_id;
    conns
        .values()
        .filter_map(|s| s.character.as_ref())
        .any(|c| c.current_room_id == room && c.pending_ritual.is_some())
}

/// A line about the player's rite for `pray petition`, if one is under way.
pub fn status_line(ch: &CharacterData, s: &Settings) -> Option<String> {
    let r = ch.pending_ritual.as_ref()?;
    let name = ch
        .pending_petition
        .as_ref()
        .map(|p| p.spell.name.clone())
        .unwrap_or_default();
    Some(if r.at_boundary {
        format!("Your rite for {name} waits on you. {}\n", prompt_line(r, s))
    } else {
        format!(
            "Your rite for {name} is at stage {} (power +{}, risk {}d6 on a push).\n",
            r.stage,
            r.power_bonus,
            r.risk + 1
        )
    })
}

// ---------- the tick ----------

/// Advance every online ritualist's rite.
pub fn process_ritual_tick(
    db: &Db,
    connections: &SharedConnections,
    _state: &SharedState,
    now: i64,
    dice: &mut dyn Dice,
) {
    let names: Vec<String> = connections
        .lock()
        .values()
        .filter_map(|s| s.character.as_ref())
        .filter(|c| c.pending_ritual.is_some())
        .map(|c| c.name.clone())
        .collect();
    if names.is_empty() {
        return;
    }
    let s = Settings::load(db);
    let today = crate::script::worship::current_absolute_day(db);
    for name in names {
        let mut notes = Vec::new();
        apply_to_character(db, connections, &name, |c| {
            let (changed, n) = tick_one(c, now, today, &s, dice);
            notes = n;
            changed
        });
        deliver(connections, &name, notes);
    }
}

/// The live tick entry point.
pub fn run_tick_once(db: &Db, connections: &SharedConnections, state: &SharedState) {
    process_ritual_tick(db, connections, state, now_secs(), &mut ThreadDice);
}

/// Now, for the Rhai bindings.
pub fn now() -> i64 {
    now_secs()
}
