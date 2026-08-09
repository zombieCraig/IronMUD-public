//! The content auditor: one place that judges builder-authored content.
//!
//! `src/progress.rs` exists because XP was awarded in four places with four
//! curves. This module exists for the same reason one layer up: "is this room
//! any good?" is a question six different features need to answer, and if each
//! one answers it for itself they will disagree.
//!
//! The consumers are:
//!
//! * the `build audit` command, which prints the findings verbatim;
//! * the builder score, which weights an entity by its [`Grade::score`];
//! * area and world ratings, which roll grades up;
//! * progress tracks, whose predicates ask questions like "do 80% of rooms
//!   grade C or better?";
//! * the bounty generator, which turns every `Blocker` and `Warn` into a job;
//! * the MCP `audit_*` tools, so agent-driven building can check its own work.
//!
//! Everything here is a pure function over data the caller already loaded. No
//! database, no locks, no I/O — which is what makes it callable from inside a
//! save path, a tick, and a test with equal ease.
//!
//! # The rule for adding a check
//!
//! **A check must be actionable and objective.** "The description is boring"
//! is not a check, because two builders will disagree and the grade stops
//! meaning anything. "The description is empty" is a check. Anything a
//! reasonable builder would argue with is a [`Severity::Polish`] finding at
//! most, and probably not a finding at all.
//!
//! The corollary is that checks are *structural*, never length-scored beyond a
//! single floor. A grade that rises with word count is a grade that rewards
//! padding, and padding is exactly the failure mode this whole tier is built
//! to avoid.

pub mod scan;

use crate::types::{AreaData, ItemData, ItemType, MobileData, QuestData, RoomData, RoomExits};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

// ===========================================================================
// Severity, findings, grades
// ===========================================================================

/// How badly a finding hurts. The three levels are deliberately coarse: a
/// finer scale invites arguing about whether something is a 4 or a 5, and the
/// only decision any consumer makes is "must fix / should fix / could fix".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The content is broken or unusable as shipped. A room with no
    /// description, an exit into the void, a weapon that deals no damage.
    Blocker,
    /// It works, but a player will notice something is missing.
    Warn,
    /// It is fine. It could be better.
    Polish,
}

impl Severity {
    pub fn key(self) -> &'static str {
        match self {
            Severity::Blocker => "blocker",
            Severity::Warn => "warn",
            Severity::Polish => "polish",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Severity::Blocker => "BLOCKER",
            Severity::Warn => "warn",
            Severity::Polish => "polish",
        }
    }

    /// Points deducted from a perfect 100.
    ///
    /// Tuned so that two blockers floor an entity, and a bare-but-valid entity
    /// — one that exists, is addressable, and does not lie to the engine, but
    /// has no depth — lands around a C. That is the honest reading of such an
    /// entity, and a scale that hands it a B is a scale nobody trusts.
    pub fn weight(self) -> i32 {
        match self {
            Severity::Blocker => 45,
            Severity::Warn => 15,
            Severity::Polish => 6,
        }
    }
}

/// One thing wrong with one entity.
///
/// `code` is the stable identifier — it is what the bounty board dedupes on
/// and auto-closes against, so it must not change once shipped. `message` is
/// what a builder reads, and is phrased as an instruction rather than a
/// complaint: the builder already knows they are looking at a problem, what
/// they need from us is the next action.
#[derive(Debug, Clone)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    fn new(code: &'static str, severity: Severity, message: impl Into<String>) -> Self {
        Finding {
            code,
            severity,
            message: message.into(),
        }
    }
}

fn blocker(code: &'static str, msg: impl Into<String>) -> Finding {
    Finding::new(code, Severity::Blocker, msg)
}
fn warn(code: &'static str, msg: impl Into<String>) -> Finding {
    Finding::new(code, Severity::Warn, msg)
}
fn polish(code: &'static str, msg: impl Into<String>) -> Finding {
    Finding::new(code, Severity::Polish, msg)
}

/// The verdict on one entity.
#[derive(Debug, Clone)]
pub struct Grade {
    /// 0..=100.
    pub score: i32,
    /// One of `LETTERS`, derived from `score` and nothing else.
    pub letter: char,
    /// Severity-ordered: blockers first.
    pub findings: Vec<Finding>,
}

impl Grade {
    /// Build a grade from findings — the constructor for a single entity, and
    /// the only place the score/letter relationship is decided.
    ///
    /// Containers use [`AuditReport::as_grade`] instead, because their score
    /// comes from a rollup rather than from their own findings.
    pub fn from_findings(mut findings: Vec<Finding>) -> Grade {
        findings.sort_by_key(|f| f.severity);
        let deducted: i32 = findings.iter().map(|f| f.severity.weight()).sum();
        let score = (100 - deducted).clamp(0, 100);
        Grade {
            score,
            letter: letter_for(score),
            findings,
        }
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == severity).count()
    }

    pub fn has(&self, code: &str) -> bool {
        self.findings.iter().any(|f| f.code == code)
    }

    /// True when nothing at or above `Warn` fires. "Clean" is deliberately not
    /// "perfect" — polish findings are suggestions, and a system that treats
    /// them as debt trains builders to ignore all findings equally.
    pub fn is_clean(&self) -> bool {
        !self.findings.iter().any(|f| f.severity <= Severity::Warn)
    }
}

/// The one threshold table. Descending, and the last entry must be the floor.
///
/// Every letter in the game comes from here — the per-entity toast, the area
/// rollup, the world rating's quality term, and the `grade_ratio` track
/// predicate. One table means "what counts as good" is a single decision.
pub const LETTERS: &[(i32, char)] = &[(90, 'A'), (78, 'B'), (62, 'C'), (45, 'D'), (0, 'F')];

pub fn letter_for(score: i32) -> char {
    for (floor, letter) in LETTERS {
        if score >= *floor {
            return *letter;
        }
    }
    'F'
}

/// Rank of a letter for comparison, 0 = A. `letter_at_least('B', 'C')` is
/// false; `letter_at_least('A', 'C')` is true.
pub fn letter_rank(letter: char) -> usize {
    LETTERS
        .iter()
        .position(|(_, l)| *l == letter)
        .unwrap_or(LETTERS.len() - 1)
}

pub fn letter_at_least(letter: char, floor: char) -> bool {
    letter_rank(letter) <= letter_rank(floor)
}

// ===========================================================================
// Context
// ===========================================================================

/// What a per-entity check cannot see for itself.
///
/// Dangling exits, one-way exits and duplicated descriptions are all
/// relationships between rooms, so they need the neighbourhood. A caller
/// grading a single room in isolation uses [`AuditCtx::empty`] and those
/// checks simply do not fire — silently, and on purpose: a false "this exit
/// goes nowhere" because we were not given the destination is worse than no
/// check at all.
#[derive(Debug, Default, Clone)]
pub struct AuditCtx {
    /// Set when the context was built from a real room set. Guards every
    /// relationship check.
    pub linked: bool,
    known_rooms: HashSet<Uuid>,
    /// Destination room -> rooms that exit into it.
    inbound: HashMap<Uuid, HashSet<Uuid>>,
    /// Normalised description hash -> how many rooms share it.
    desc_counts: HashMap<u64, usize>,
}

impl AuditCtx {
    /// An unlinked context. Relationship checks are skipped.
    pub fn empty() -> AuditCtx {
        AuditCtx::default()
    }

    /// Build from every room the checks should be able to see. Pass the whole
    /// world for a world audit, or one area's rooms for an area audit — an
    /// area-scoped context will flag exits leaving the area as dangling, so
    /// callers auditing one area should still pass the full room set.
    pub fn build(rooms: &[RoomData]) -> AuditCtx {
        let mut ctx = AuditCtx {
            linked: true,
            ..Default::default()
        };
        for room in rooms {
            ctx.known_rooms.insert(room.id);
            // Property instances are excluded from the duplicate map. They copy
            // their template's description verbatim by design, so counting them
            // stamps `room.duplicate_desc` on every instantiated player
            // property *and* on the builder's template — content the builder
            // did not write, dragging down the area that hosts it.
            if room.property_template_id.is_some() {
                continue;
            }
            let h = normalised_hash(&room.description);
            if !room.description.trim().is_empty() {
                *ctx.desc_counts.entry(h).or_insert(0) += 1;
            }
        }
        for room in rooms {
            for (_, dest) in exit_pairs(&room.exits) {
                ctx.inbound.entry(dest).or_default().insert(room.id);
            }
        }
        ctx
    }

    fn knows(&self, id: Uuid) -> bool {
        self.known_rooms.contains(&id)
    }

    /// Does `dest` exit back into `origin`? Reads the inbound map of `origin`,
    /// which is where an exit *from* `dest` would have been recorded.
    fn has_path_back(&self, origin: Uuid, dest: Uuid) -> bool {
        self.inbound.get(&origin).is_some_and(|s| s.contains(&dest))
    }

    fn desc_shared(&self, desc: &str) -> bool {
        self.desc_counts.get(&normalised_hash(desc)).copied().unwrap_or(0) > 1
    }
}

fn normalised_hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    let norm: String = s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    norm.hash(&mut h);
    h.finish()
}

/// Every set exit as (direction name, destination). Custom exits included.
pub fn exit_pairs(exits: &RoomExits) -> Vec<(String, Uuid)> {
    let mut out = Vec::new();
    let named: [(&str, Option<Uuid>); 7] = [
        ("north", exits.north),
        ("east", exits.east),
        ("south", exits.south),
        ("west", exits.west),
        ("up", exits.up),
        ("down", exits.down),
        ("out", exits.out),
    ];
    for (name, dest) in named {
        if let Some(d) = dest {
            out.push((name.to_string(), d));
        }
    }
    for (name, dest) in &exits.custom {
        out.push((name.clone(), *dest));
    }
    out
}

// ===========================================================================
// Shared checks
// ===========================================================================

/// Text that exists in the field but says nothing. Importers and hurried
/// builders both leave these behind, and they are worse than an empty string
/// because they defeat an `is_empty` check.
const PLACEHOLDERS: &[&str] = &[
    "",
    "n/a",
    "na",
    "tbd",
    "todo",
    "xxx",
    "...",
    "-",
    "undefined",
    "none",
    "nothing special",
    "you see nothing special",
    "it is a room",
    "a room",
    "an item",
    "a mobile",
    "new room",
    "new item",
    "new mobile",
    "unfinished room",
    "description",
    "no description",
    "no description set",
    "default room",
    "default description",
];

/// Minimum description length before a description reads as a stub. One floor,
/// not a curve — see the module header on why length must not score.
const DESC_FLOOR: usize = 80;

fn is_placeholder(s: &str) -> bool {
    let t = s.trim().trim_end_matches('.').to_lowercase();
    PLACEHOLDERS.contains(&t.as_str())
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty() || is_placeholder(s)
}

/// Raw angle brackets break MXP-bound text. The test suite already forbids
/// these in editor-facing strings; this promotes the same rule to something a
/// builder is told about at write time rather than at `cargo test` time.
fn has_mxp_hazard(s: &str) -> bool {
    s.contains('<') || s.contains('>')
}

/// Words that carry no addressing weight, so their absence from `keywords` is
/// not a defect.
const NOUN_STOPWORDS: &[&str] = &[
    "the", "and", "with", "here", "there", "his", "her", "its", "their", "you", "your", "this", "that", "these",
    "those", "from", "into", "onto", "over", "under", "near", "for", "who", "has", "have", "are", "was", "were",
    "been", "being", "stands", "stand", "sits", "sit", "lies", "lie", "leans", "lean", "rests", "rest", "waits",
    "wait", "walks", "walk", "moves", "move", "looks", "look", "seems", "seem", "appears", "appear", "wanders",
    "wander", "some", "very", "quite", "just", "about", "around", "against", "before", "after", "while", "when",
    "where", "what", "which", "hangs", "hang", "lying", "sitting", "standing", "nearby", "here.", "them", "they",
    "block", "blocks", "guarding", "watching", "watches", "watch", "holds", "hold", "holding", "carries", "carry",
    "carrying", "wears", "wear", "wearing", "paces", "pace", "tends", "tend", "works", "work", "working", "sells",
    "sell", "selling", "waiting", "leaning", "resting",
];

/// Salient nouns in a display string that a player would reasonably try to
/// address. Lowercased, stopwords and short words dropped.
fn salient_words(display: &str) -> Vec<String> {
    display
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_lowercase())
        .filter(|w| !NOUN_STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Shortest keyword allowed to match by containment rather than equality.
///
/// Without a floor the containment arms are a hole rather than a convenience:
/// `"anything".contains("")` is true, so **one empty keyword silences this
/// check for the entire entity**, and `["e","a","o"]` covers essentially every
/// English word. That is a lint a builder can delete with junk, which is worse
/// than not having it.
const KEYWORD_MATCH_FLOOR: usize = 3;

/// A keyword covers a word if either contains the other — cheap plural and
/// compound tolerance ("guards" covers "guard", "goblin" covers "goblins")
/// without a stemmer. Containment needs a keyword of at least
/// [`KEYWORD_MATCH_FLOOR`]; shorter ones must match exactly.
fn keywords_cover(keywords: &[String], word: &str) -> bool {
    keywords
        .iter()
        .map(|k| k.to_lowercase())
        .any(|k| k == word || (k.len() >= KEYWORD_MATCH_FLOOR && (k.contains(word) || word.contains(&k))))
}

/// Everything a player can actually type to reach an entity.
///
/// `item_matches_keyword` (`crate::script::items`) tests `name` by substring
/// *before* it consults `keywords`, and mobile lookup calls the same function
/// — so an item named "a bull whip" answers to `get whip` with no keywords set
/// at all. A lint that ignores `name` reports a blocker on content that works,
/// which is worse than no lint: it teaches builders that the auditor is wrong.
///
/// Name tokens skip the [`NOUN_STOPWORDS`] filter that [`salient_words`]
/// applies. That filter exists to decide which words a builder *ought* to have
/// covered; this list is what the engine will match, and the engine does not
/// consult a stopword table.
fn addressable_terms(name: &str, keywords: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = keywords.to_vec();
    terms.extend(
        name.split(|c: char| !c.is_alphabetic())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase()),
    );
    terms
}

/// The shared "can a player type this name?" check.
///
/// Every salient noun in the display string must be reachable through the
/// addressable terms, or the entity is on screen and unaddressable — the
/// single most common builder mistake in this codebase, and the reason it is a
/// lint rather than a convention.
fn check_keyword_coverage(findings: &mut Vec<Finding>, code: &'static str, display: &str, keywords: &[String]) {
    let missing: Vec<String> = salient_words(display)
        .into_iter()
        .filter(|w| !keywords_cover(keywords, w))
        .collect();
    if missing.is_empty() {
        return;
    }
    let mut shown: Vec<String> = missing.clone();
    shown.sort();
    shown.dedup();
    shown.truncate(3);
    findings.push(warn(
        code,
        format!(
            "Players cannot address this by {}. Add {} to keywords.",
            shown.join(", "),
            if shown.len() == 1 { "it" } else { "them" }
        ),
    ));
}

// ===========================================================================
// Rooms
// ===========================================================================

pub fn audit_room(room: &RoomData, ctx: &AuditCtx) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if is_blank(&room.title) {
        f.push(blocker("room.no_title", "Set a title — `redit <vnum> title <text>`."));
    }

    if is_blank(&room.description) {
        f.push(blocker("room.no_desc", "Write a description — `redit <vnum> desc`."));
    } else if room.description.trim().len() < DESC_FLOOR {
        f.push(warn(
            "room.thin_desc",
            format!(
                "The description is {} characters. Rooms read as unfinished below {}.",
                room.description.trim().len(),
                DESC_FLOOR
            ),
        ));
    }

    let exits = exit_pairs(&room.exits);

    // A property template room legitimately has no exits until it is
    // instantiated, so it is exempt rather than permanently failing.
    if exits.is_empty() && !room.is_property_template && !room.property_entrance {
        f.push(blocker(
            "room.no_exits",
            "This room has no exits. Link it with `dig` or `link`.",
        ));
    }

    if ctx.linked {
        let dangling: Vec<&String> = exits
            .iter()
            .filter(|(_, dest)| !ctx.knows(*dest))
            .map(|(dir, _)| dir)
            .collect();
        if !dangling.is_empty() {
            f.push(blocker(
                "room.dangling_exit",
                format!(
                    "Exit{} {} lead{} to a room that does not exist.",
                    if dangling.len() == 1 { "" } else { "s" },
                    dangling.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    if dangling.len() == 1 { "s" } else { "" }
                ),
            ));
        }

        let one_way: Vec<&String> = exits
            .iter()
            .filter(|(_, dest)| ctx.knows(*dest) && !ctx.has_path_back(room.id, *dest))
            .map(|(dir, _)| dir)
            .collect();
        if !one_way.is_empty() {
            f.push(warn(
                "room.one_way_exit",
                format!(
                    "Exit{} {} {} no way back. Intentional one-ways are fine; strand a player and they will `bug` you.",
                    if one_way.len() == 1 { "" } else { "s" },
                    one_way.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    if one_way.len() == 1 { "has" } else { "have" }
                ),
            ));
        }

        // Not on a room that already tripped `room.thin_desc`: every freshly
        // dug room carries the same placeholder, and charging twice for one
        // defect is how a builder mid-dig watches an area's grade fall further
        // than the work deserves. The thin finding already names the fix.
        let thin = room.description.trim().len() < DESC_FLOOR;
        if !is_blank(&room.description) && !thin && ctx.desc_shared(&room.description) {
            f.push(warn(
                "room.duplicate_desc",
                "Another room has this exact description. Copied rooms read as filler.",
            ));
        }
    }

    if has_mxp_hazard(&room.title) || has_mxp_hazard(&room.description) {
        f.push(warn(
            "room.mxp_hazard",
            "Raw < or > breaks MXP clients. Reword or escape them.",
        ));
    }

    // A room where every flag is off has had no sector decision made — it is
    // outdoors, lit, dry, ordinary ground, by omission rather than by choice.
    if !any_room_flag_set(room) {
        f.push(polish(
            "room.no_flags",
            "No room flags set. Consider indoors/dark/city/dirt_floor and the rest — flags are how the room joins the weather, forage and light systems.",
        ));
    }

    if room.extra_descs.is_empty() {
        f.push(polish(
            "room.no_extra_descs",
            "Nothing here can be examined. `redit <vnum> extradesc add <keywords>` rewards the players who look.",
        ));
    }

    let interactive = !room.triggers.is_empty()
        || !room.contextual_commands.is_empty()
        || !room.doors.is_empty()
        || !room.traps.is_empty()
        || !room.catch_table.is_empty()
        || room.entry_gate.is_some();
    if !interactive && room.extra_descs.is_empty() {
        f.push(polish(
            "room.inert",
            "Nothing in this room reacts: no triggers, doors, verbs or extra descriptions.",
        ));
    }

    if room.spring_desc.is_none()
        && room.summer_desc.is_none()
        && room.autumn_desc.is_none()
        && room.winter_desc.is_none()
        && !room.flags.indoors
    {
        f.push(polish(
            "room.no_seasonal_desc",
            "Outdoor room with no seasonal descriptions — the season system will never touch it.",
        ));
    }

    Grade::from_findings(f)
}

fn any_room_flag_set(room: &RoomData) -> bool {
    let fl = &room.flags;
    fl.dark
        || fl.combat_zone.is_some()
        || fl.no_mob
        || fl.indoors
        || fl.underwater
        || fl.climate_controlled
        || fl.always_hot
        || fl.always_cold
        || fl.city
        || fl.no_windows
        || fl.difficult_terrain
        || fl.dirt_floor
        || fl.property_storage
        || fl.post_office
        || fl.baseline_office
        || fl.bank
        || fl.garden
        || fl.spawn_point
        || fl.shallow_water
        || fl.deep_water
        || fl.liveable
        || fl.private_room
        || fl.tunnel
        || fl.death
        || fl.no_magic
        || fl.soundproof
        || fl.notrack
        || fl.no_recall
        || fl.temple
}

// ===========================================================================
// Items
// ===========================================================================

pub fn audit_item(item: &ItemData) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if is_blank(&item.name) {
        f.push(blocker("item.no_name", "Set a name — `oedit <vnum> name <text>`."));
    }
    if is_blank(&item.short_desc) {
        f.push(blocker(
            "item.no_short_desc",
            "Set a short description — it is what inventory and equipment lists print.",
        ));
    }
    if is_blank(&item.long_desc) {
        f.push(blocker(
            "item.no_long_desc",
            "Set a long description — it is what `look` prints when the item is on the ground.",
        ));
    }

    let item_terms = addressable_terms(&item.name, &item.keywords);
    if item_terms.is_empty() {
        f.push(blocker(
            "item.no_keywords",
            "No name and no keywords: nothing a player types can refer to this item.",
        ));
    } else {
        check_keyword_coverage(&mut f, "item.keywords_miss_nouns", &item.short_desc, &item_terms);
    }

    match item.item_type {
        ItemType::Weapon => {
            if item.damage_dice_count <= 0 || item.damage_dice_sides <= 0 {
                f.push(blocker(
                    "item.weapon_no_damage",
                    "Weapon with no damage dice — it will hit for nothing.",
                ));
            }
            if item.weapon_skill.is_none() {
                f.push(warn(
                    "item.weapon_no_skill",
                    "No weapon skill set, so wielding it trains nothing.",
                ));
            }
        }
        ItemType::Armor => {
            if item.armor_class.unwrap_or(0) == 0 && item.affects.is_empty() {
                f.push(blocker(
                    "item.armor_no_protection",
                    "Armor with no armor class and no affects does nothing when worn.",
                ));
            }
            if item.wear_locations.is_empty() {
                f.push(blocker(
                    "item.armor_no_wear_location",
                    "Armor with no wear location cannot be worn.",
                ));
            }
        }
        ItemType::Container => {
            if item.container_max_items <= 0 && item.container_max_weight <= 0 {
                f.push(blocker(
                    "item.container_no_capacity",
                    "Container holds nothing — set max items or max weight.",
                ));
            }
        }
        ItemType::LiquidContainer => {
            if item.liquid_max <= 0 {
                f.push(blocker(
                    "item.liquid_no_capacity",
                    "Liquid container with zero capacity can never be filled.",
                ));
            }
        }
        ItemType::Food => {
            if item.food_nutrition <= 0 {
                f.push(warn("item.food_no_nutrition", "Food with no nutrition value."));
            }
        }
        ItemType::Key => {
            if item.vnum.as_deref().unwrap_or("").is_empty() {
                f.push(blocker(
                    "item.key_no_vnum",
                    "A key without a vnum can never be referenced by a door.",
                ));
            }
        }
        ItemType::Note => {
            if item.note_content.as_deref().unwrap_or("").trim().is_empty() {
                f.push(warn("item.note_empty", "Note with no written content."));
            }
        }
        ItemType::Misc => {
            if item.affects.is_empty() && item.triggers.is_empty() && item.categories.is_empty() {
                f.push(warn(
                    "item.untyped",
                    "Type is `misc` with no affects, triggers or categories — it is scenery. Set a real item type if it should do something.",
                ));
            }
        }
        _ => {}
    }

    if !item.wear_locations.is_empty() && item.weight <= 0 {
        f.push(warn(
            "item.weightless",
            "Wearable with zero weight — encumbrance will never see it.",
        ));
    }

    if item.value <= 0 && !item.flags.no_sell && !item.flags.quest_item && item.item_type != ItemType::Misc {
        f.push(warn(
            "item.no_value",
            "Value is 0, so shops pay nothing for it. Set a value or flag it no_sell.",
        ));
    }

    if has_mxp_hazard(&item.short_desc) || has_mxp_hazard(&item.long_desc) {
        f.push(warn(
            "item.mxp_hazard",
            "Raw < or > breaks MXP clients. Reword or escape them.",
        ));
    }

    if item.extra_descs.is_empty() {
        f.push(polish(
            "item.no_extra_descs",
            "No extra descriptions — examining it tells the player nothing beyond the long description.",
        ));
    }

    Grade::from_findings(f)
}

// ===========================================================================
// Mobiles
// ===========================================================================

pub fn audit_mobile(mob: &MobileData) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if is_blank(&mob.name) {
        f.push(blocker("mobile.no_name", "Set a name — `medit <vnum> name <text>`."));
    }
    if is_blank(&mob.short_desc) {
        f.push(blocker(
            "mobile.no_short_desc",
            "Set a short description — it is the line `look` prints for this mobile.",
        ));
    }
    if is_blank(&mob.long_desc) {
        f.push(blocker(
            "mobile.no_long_desc",
            "Set a long description — it is what `examine` prints.",
        ));
    }

    let mob_terms = addressable_terms(&mob.name, &mob.keywords);
    if mob_terms.is_empty() {
        f.push(blocker(
            "mobile.no_keywords",
            "No name and no keywords: a player can see this mobile but cannot target it.",
        ));
    } else {
        check_keyword_coverage(&mut f, "mobile.keywords_miss_nouns", &mob.short_desc, &mob_terms);
    }

    if mob.level <= 0 {
        f.push(blocker(
            "mobile.no_level",
            "Level 0 — `consider` cannot rank it and XP awards will be nil.",
        ));
    }

    let combatant = !mob.flags.no_attack;
    if combatant && mob.damage_dice.trim().is_empty() {
        f.push(warn(
            "mobile.no_damage_dice",
            "Attackable mobile with no damage dice — it will fight back for nothing.",
        ));
    }

    if mob.flags.shopkeeper && mob.shop_stock.is_empty() && mob.shop_preset_vnum.trim().is_empty() {
        f.push(warn(
            "mobile.shop_empty",
            "Shopkeeper with no stock and no shop preset — `list` shows an empty shelf.",
        ));
    }
    if mob.flags.healer && mob.healer_type.trim().is_empty() {
        f.push(warn(
            "mobile.healer_no_type",
            "Healer flag set with no healer type (medic/herbalist/cleric).",
        ));
    }
    if mob.flags.leasing_agent && mob.property_templates.is_empty() {
        f.push(warn(
            "mobile.agent_no_templates",
            "Leasing agent with no property templates to offer.",
        ));
    }

    if has_mxp_hazard(&mob.short_desc) || has_mxp_hazard(&mob.long_desc) {
        f.push(warn(
            "mobile.mxp_hazard",
            "Raw < or > breaks MXP clients. Reword or escape them.",
        ));
    }

    let has_behaviour = !mob.dialogue.is_empty()
        || mob.dialogue_tree.is_some()
        || !mob.triggers.is_empty()
        || !mob.daily_routine.is_empty()
        || mob.simulation.is_some();
    if !has_behaviour {
        f.push(polish(
            "mobile.inert",
            "No dialogue, triggers, routine or simulation — it stands still and can be killed. That is all it does.",
        ));
    }

    if combatant && mob.gold <= 0 && mob.level >= 3 {
        f.push(polish(
            "mobile.no_reward",
            "Carries no gold, so killing it drops nothing but XP.",
        ));
    }

    if mob.alignment == 0 && combatant {
        f.push(polish(
            "mobile.no_alignment",
            "Alignment 0: killing this carries no moral weight. Set one if it should move morality.",
        ));
    }

    Grade::from_findings(f)
}

// ===========================================================================
// Quests
// ===========================================================================

pub fn audit_quest(quest: &QuestData) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if is_blank(&quest.name) {
        f.push(blocker("quest.no_name", "Set a name — `quedit <vnum> name <text>`."));
    }
    if is_blank(&quest.summary) {
        f.push(blocker(
            "quest.no_summary",
            "Set a summary — it is the one line the quest log shows.",
        ));
    }
    if quest.objectives.is_empty() {
        f.push(blocker(
            "quest.no_objectives",
            "No objectives, so the quest can never be completed.",
        ));
    }
    if quest.rewards.is_empty() {
        f.push(blocker(
            "quest.no_rewards",
            "No rewards — completing it gives the player nothing.",
        ));
    }
    if quest.keywords.is_empty() {
        f.push(warn(
            "quest.no_keywords",
            "No keywords: `quest <name>` will only match on the full name.",
        ));
    }
    if quest.giver_mob_vnum.as_deref().unwrap_or("").trim().is_empty() {
        f.push(warn(
            "quest.no_giver",
            "No giver mobile, so nothing in the world offers this quest.",
        ));
    }
    if is_blank(&quest.description) {
        f.push(warn(
            "quest.no_description",
            "No description shown when the quest is offered.",
        ));
    }
    if is_blank(&quest.completion_text) {
        f.push(polish(
            "quest.no_completion_text",
            "No completion text — turning it in prints only the reward lines.",
        ));
    }

    Grade::from_findings(f)
}

// ===========================================================================
// Areas and rollups
// ===========================================================================

/// What an area contains, as counted by the caller. The auditor never reads
/// the database, so the caller does the gathering.
#[derive(Debug, Default, Clone)]
pub struct AreaContents<'a> {
    pub rooms: &'a [RoomData],
    pub items: &'a [ItemData],
    pub mobiles: &'a [MobileData],
    pub quests: &'a [QuestData],
    pub spawn_point_count: usize,
}

/// Rooms in this area that cannot be reached from its entrance by following
/// exits. An orphan is content the player will never see.
///
/// The walk starts at `starting_room_vnum` when the area declares one. Starting
/// at whichever room the database happened to yield first is how a single
/// disconnected room came to report *every other room in the area* as an
/// orphan — a 45-point blocker on a perfectly connected place.
///
/// Property template rooms are excluded from both ends: they have no exits
/// until they are instantiated, which is why `room.no_exits` already exempts
/// them, and an area is not broken for containing one.
fn orphan_rooms(rooms: &[RoomData], entrance_vnum: Option<&str>) -> Vec<Uuid> {
    let rooms: Vec<&RoomData> = rooms.iter().filter(|r| !r.is_property_template).collect();
    if rooms.is_empty() {
        return Vec::new();
    }
    let ids: HashSet<Uuid> = rooms.iter().map(|r| r.id).collect();
    // Undirected reachability: a room reachable only by walking backwards is
    // still reachable in practice, because the player got there somehow.
    let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for room in &rooms {
        for (_, dest) in exit_pairs(&room.exits) {
            if ids.contains(&dest) {
                adj.entry(room.id).or_default().push(dest);
                adj.entry(dest).or_default().push(room.id);
            }
        }
    }
    let start = entrance_vnum
        .and_then(|v| {
            rooms
                .iter()
                .find(|r| r.vnum.as_deref().is_some_and(|rv| rv.eq_ignore_ascii_case(v)))
        })
        .unwrap_or(&rooms[0])
        .id;
    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut stack = vec![start];
    seen.insert(start);
    while let Some(cur) = stack.pop() {
        for next in adj.get(&cur).into_iter().flatten() {
            if seen.insert(*next) {
                stack.push(*next);
            }
        }
    }
    rooms.iter().map(|r| r.id).filter(|id| !seen.contains(id)).collect()
}

/// Minimum rooms before an area reads as a real place rather than a sketch.
const AREA_ROOM_FLOOR: usize = 8;

pub fn audit_area(area: &AreaData, contents: &AreaContents) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if is_blank(&area.name) {
        f.push(blocker("area.no_name", "Set a name — `aedit <id> name <text>`."));
    }
    if contents.rooms.is_empty() {
        f.push(blocker("area.no_rooms", "The area has no rooms."));
    } else if contents.rooms.len() < AREA_ROOM_FLOOR {
        f.push(warn(
            "area.thin",
            format!(
                "{} room{} — areas under {} read as a sketch.",
                contents.rooms.len(),
                if contents.rooms.len() == 1 { "" } else { "s" },
                AREA_ROOM_FLOOR
            ),
        ));
    }

    if contents.mobiles.is_empty() {
        f.push(warn(
            "area.no_mobiles",
            "No mobile prototypes — the area is uninhabited.",
        ));
    }
    if contents.items.is_empty() {
        f.push(warn(
            "area.no_items",
            "No item prototypes — there is nothing to find here.",
        ));
    }
    if contents.spawn_point_count == 0 && !contents.mobiles.is_empty() {
        f.push(blocker(
            "area.no_spawn_points",
            "Prototypes exist but nothing spawns them. Add spawn points with `spedit`.",
        ));
    }

    if is_blank(&area.description) {
        f.push(warn("area.no_description", "No area description."));
    }
    if area.level_min == 0 && area.level_max == 0 {
        f.push(warn(
            "area.no_level_range",
            "No level range set, so nothing can tell players who this area is for.",
        ));
    }

    let orphans = orphan_rooms(contents.rooms, area.starting_room_vnum.as_deref());
    if !orphans.is_empty() {
        f.push(blocker(
            "area.orphan_rooms",
            format!(
                "{} room{} unreachable from the rest of the area.",
                orphans.len(),
                if orphans.len() == 1 { " is" } else { "s are" }
            ),
        ));
    }

    if contents.quests.is_empty() {
        f.push(polish(
            "area.no_quests",
            "No quests set here — nothing gives players a reason to come.",
        ));
    }
    if area.theme.trim().is_empty() {
        f.push(polish("area.no_theme", "No theme set."));
    }
    // Owner is an ACL and `authored_by` is a credit — two findings, because
    // they are two different problems with two different fixes. The old single
    // finding said setting an owner would get you credited, which was never
    // true of any code path and sent builders to `aedit owner` for a problem
    // only `build claim` can solve.
    if area.owner.is_none() {
        f.push(polish(
            "area.no_owner",
            "Unowned: any builder can edit it. Set one with `aedit owner <name>`.",
        ));
    } else if area.authored_by.is_none() {
        f.push(polish(
            "area.unattributed",
            "Owned but uncredited: it counts toward nobody's score. `build claim`.",
        ));
    }

    Grade::from_findings(f)
}

// ===========================================================================
// Reports
// ===========================================================================

/// The five authored entity kinds.
///
/// Re-exported from `crate::types` rather than defined here: attribution
/// (`src/attribution.rs`) needs the same enum, and a module that stamps
/// authorship should not have to depend on the module that grades quality.
pub use crate::types::ContentKind as EntityKind;

/// One graded child inside a report.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub kind: EntityKind,
    /// vnum where one exists, otherwise the display name.
    pub label: String,
    pub name: String,
    pub grade: Grade,
}

/// A container's verdict: its own findings plus its graded children.
#[derive(Debug, Clone)]
pub struct AuditReport {
    pub label: String,
    pub own: Grade,
    pub entries: Vec<AuditEntry>,
    pub score: i32,
    pub letter: char,
}

/// How the container's own findings weigh against its contents.
///
/// An area whose own checks all pass but whose rooms are all F is not a good
/// area, and an area with twenty excellent rooms is not ruined by a missing
/// theme. 40/60 says the contents matter more, without letting a container
/// coast on volume.
const OWN_WEIGHT: i32 = 40;

impl AuditReport {
    pub fn build(label: impl Into<String>, own: Grade, entries: Vec<AuditEntry>) -> AuditReport {
        let score = if entries.is_empty() {
            own.score
        } else {
            let mean: i32 = entries.iter().map(|e| e.grade.score).sum::<i32>() / entries.len() as i32;
            (own.score * OWN_WEIGHT + mean * (100 - OWN_WEIGHT)) / 100
        };
        AuditReport {
            label: label.into(),
            own,
            entries,
            score,
            letter: letter_for(score),
        }
    }

    /// Entries of one kind, worst first — what `build audit` prints and what
    /// the bounty generator walks.
    pub fn worst(&self, kind: Option<EntityKind>, limit: usize) -> Vec<&AuditEntry> {
        let mut v: Vec<&AuditEntry> = self
            .entries
            .iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .filter(|e| !e.grade.findings.is_empty())
            .collect();
        v.sort_by(|a, b| a.grade.score.cmp(&b.grade.score).then_with(|| a.label.cmp(&b.label)));
        v.truncate(limit);
        v
    }

    /// Count of entries of a kind whose letter is at least `floor`.
    pub fn ratio_at_least(&self, kind: EntityKind, floor: char) -> Option<f32> {
        let of_kind: Vec<&AuditEntry> = self.entries.iter().filter(|e| e.kind == kind).collect();
        if of_kind.is_empty() {
            return None;
        }
        let good = of_kind
            .iter()
            .filter(|e| letter_at_least(e.grade.letter, floor))
            .count();
        Some(good as f32 / of_kind.len() as f32)
    }

    pub fn count_of(&self, kind: EntityKind) -> usize {
        self.entries.iter().filter(|e| e.kind == kind).count()
    }

    /// This report as a single [`Grade`].
    ///
    /// The score and letter are the **rollup** — they account for the
    /// children — while `findings` are the container's **own** checks, because
    /// a caller listing one area does not want every room's findings inlined
    /// underneath it; `worst()` and `all_findings()` exist for that.
    ///
    /// Three callers were building this pairing by struct literal, which made
    /// the "only constructor" promise on `Grade::from_findings` untrue and left
    /// `count()` and `is_clean()` answering about a different scope than the
    /// letter beside them. Naming it is what makes the mismatch deliberate
    /// instead of accidental.
    pub fn as_grade(&self) -> Grade {
        Grade {
            score: self.score,
            letter: self.letter,
            findings: self.own.findings.clone(),
        }
    }

    /// Every graded letter of one kind, for a caller that needs to ask about a
    /// floor it does not know yet — see `build_tracks::TrackFacts`.
    pub fn letters_of(&self, kind: EntityKind) -> Vec<char> {
        self.entries
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.grade.letter)
            .collect()
    }

    /// Every finding in the report, container-level first. The bounty
    /// generator's input.
    pub fn all_findings(&self) -> Vec<(Option<&AuditEntry>, &Finding)> {
        let mut out: Vec<(Option<&AuditEntry>, &Finding)> = self.own.findings.iter().map(|f| (None, f)).collect();
        for e in &self.entries {
            for f in &e.grade.findings {
                out.push((Some(e), f));
            }
        }
        out.sort_by_key(|(_, f)| f.severity);
        out
    }

    pub fn severity_counts(&self) -> (usize, usize, usize) {
        let mut b = 0;
        let mut w = 0;
        let mut p = 0;
        for (_, f) in self.all_findings() {
            match f.severity {
                Severity::Blocker => b += 1,
                Severity::Warn => w += 1,
                Severity::Polish => p += 1,
            }
        }
        (b, w, p)
    }
}

/// Grade every entity in an area and roll it up.
pub fn report_area(area: &AreaData, contents: &AreaContents, ctx: &AuditCtx) -> AuditReport {
    let mut entries = Vec::new();
    for r in contents.rooms {
        entries.push(AuditEntry {
            kind: EntityKind::Room,
            label: r.vnum.clone().unwrap_or_else(|| short_id(r.id)),
            name: r.title.clone(),
            grade: audit_room(r, ctx),
        });
    }
    for m in contents.mobiles {
        entries.push(AuditEntry {
            kind: EntityKind::Mobile,
            label: if m.vnum.is_empty() {
                short_id(m.id)
            } else {
                m.vnum.clone()
            },
            name: m.short_desc.clone(),
            grade: audit_mobile(m),
        });
    }
    for i in contents.items {
        entries.push(AuditEntry {
            kind: EntityKind::Item,
            label: i.vnum.clone().unwrap_or_else(|| short_id(i.id)),
            name: i.short_desc.clone(),
            grade: audit_item(i),
        });
    }
    for q in contents.quests {
        entries.push(AuditEntry {
            kind: EntityKind::Quest,
            label: q.vnum.clone(),
            name: q.name.clone(),
            grade: audit_quest(q),
        });
    }
    AuditReport::build(area.name.clone(), audit_area(area, contents), entries)
}

fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// World-level checks: the things that are wrong with a world rather than with
/// any single area in it.
#[derive(Debug, Default, Clone)]
pub struct WorldFacts {
    pub area_count: usize,
    pub room_count: usize,
    pub item_count: usize,
    pub mobile_count: usize,
    pub quest_count: usize,
    pub spawn_point_count: usize,
    pub recipe_count: usize,
    pub transport_count: usize,
    /// Rooms carrying the `post_office` flag.
    pub post_office_rooms: usize,
    /// Item prototypes of type Board.
    pub board_items: usize,
    /// Rooms flagged `spawn_point` — where players may bind recall.
    pub recall_rooms: usize,
    /// Rooms flagged `bank`.
    pub bank_rooms: usize,
    /// Areas with at least one exit into a room belonging to another area.
    pub connected_areas: usize,
    /// Mobiles carrying a dialogue tree.
    pub dialogue_trees: usize,
    /// Prototypes with no `area_id`. They are real content, but no area audit
    /// will ever reach them and every area they logically belong to will
    /// report itself empty — so the world report has to own them.
    pub unfiled_mobiles: usize,
    pub unfiled_items: usize,
}

pub fn audit_world(facts: &WorldFacts) -> Grade {
    let mut f: Vec<Finding> = Vec::new();

    if facts.room_count == 0 {
        f.push(blocker("world.empty", "The world has no rooms."));
    }
    if facts.quest_count == 0 {
        f.push(blocker(
            "world.no_quests",
            "No quests exist. Quests are the only system that tells a player what to do next.",
        ));
    }
    if facts.spawn_point_count == 0 {
        f.push(blocker(
            "world.no_spawns",
            "No spawn points anywhere — the world is permanently empty.",
        ));
    }
    if facts.area_count > 1 && facts.connected_areas < facts.area_count {
        f.push(warn(
            "world.isolated_areas",
            format!(
                "{} area(s) have no exit into another area — players cannot walk between them.",
                facts.area_count - facts.connected_areas
            ),
        ));
    }
    if facts.recall_rooms == 0 {
        f.push(warn(
            "world.no_recall_point",
            "No room carries the `spawn_point` flag, so players cannot bind a recall point.",
        ));
    }
    if facts.post_office_rooms == 0 {
        f.push(warn(
            "world.no_post_office",
            "No post office room — the mail system is unreachable.",
        ));
    }
    if facts.board_items == 0 {
        f.push(warn(
            "world.no_boards",
            "No bulletin boards exist. Boards are where a playerbase talks to itself.",
        ));
    }
    if facts.unfiled_mobiles > 0 || facts.unfiled_items > 0 {
        f.push(warn(
            "world.unfiled_prototypes",
            format!(
                "{} mobile and {} item prototype(s) belong to no area. Every area they should be in reports itself empty until they are stamped.",
                facts.unfiled_mobiles, facts.unfiled_items
            ),
        ));
    }
    if facts.bank_rooms == 0 {
        f.push(polish(
            "world.no_bank",
            "No bank room — banking commands are unreachable.",
        ));
    }
    if facts.dialogue_trees == 0 {
        f.push(polish(
            "world.no_dialogue_trees",
            "No mobile has a dialogue tree. Flat keyword dialogue is the shallow end of the NPC system.",
        ));
    }
    if facts.recipe_count == 0 {
        f.push(polish("world.no_recipes", "No crafting recipes exist."));
    }
    if facts.transport_count == 0 {
        f.push(polish(
            "world.no_transports",
            "No transports — every journey is on foot.",
        ));
    }

    Grade::from_findings(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn room(patch: serde_json::Value) -> RoomData {
        let mut base = json!({
            "id": Uuid::new_v4(),
            "title": "The Iron Gate",
            "description": "A heavy iron gate stands here, its bars pitted with rust and \
                            streaked where rain has run down them for a hundred winters.",
            "exits": {"north": null, "east": null, "south": null, "west": null, "up": null, "down": null},
        });
        let obj = base.as_object_mut().unwrap();
        for (k, v) in patch.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).expect("room fixture")
    }

    fn mobile(patch: serde_json::Value) -> MobileData {
        let mut base = json!({
            "id": Uuid::new_v4(),
            "name": "a goblin guard",
            "short_desc": "A goblin guard leans on a rusted pike.",
            "long_desc": "Squat and thick-shouldered, the goblin watches the road with flat yellow eyes.",
            "keywords": ["goblin", "guard", "pike"],
            "level": 4,
            "damage_dice": "2d6+1",
            "gold": 20,
        });
        let obj = base.as_object_mut().unwrap();
        for (k, v) in patch.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).expect("mobile fixture")
    }

    fn item(patch: serde_json::Value) -> ItemData {
        let mut base = json!({
            "id": Uuid::new_v4(),
            "name": "a rusted pike",
            "short_desc": "a rusted pike",
            "long_desc": "A rusted pike lies here, its haft split near the head.",
            "keywords": ["rusted", "pike"],
            "item_type": "weapon",
            "damage_dice_count": 1,
            "damage_dice_sides": 8,
            "weapon_skill": "polearms",
            "value": 30,
            "weight": 12,
        });
        let obj = base.as_object_mut().unwrap();
        for (k, v) in patch.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).expect("item fixture")
    }

    // --- the threshold table -------------------------------------------------

    #[test]
    fn the_letter_table_covers_every_score() {
        for s in 0..=100 {
            let l = letter_for(s);
            assert!(LETTERS.iter().any(|(_, x)| *x == l), "score {s} produced {l}");
        }
    }

    #[test]
    fn letters_are_monotonic_in_score() {
        let mut prev = letter_rank(letter_for(0));
        for s in 0..=100 {
            let r = letter_rank(letter_for(s));
            assert!(r <= prev, "score {s} graded worse than {}", s - 1);
            prev = r;
        }
    }

    #[test]
    fn grading_is_deterministic() {
        let r = room(json!({}));
        let a = audit_room(&r, &AuditCtx::empty());
        let b = audit_room(&r, &AuditCtx::empty());
        assert_eq!(a.score, b.score);
        assert_eq!(a.letter, b.letter);
    }

    #[test]
    fn two_blockers_floor_an_entity() {
        assert_eq!(
            Grade::from_findings(vec![blocker("a", "x"), blocker("b", "y")]).letter,
            'F'
        );
    }

    // --- rooms ---------------------------------------------------------------

    #[test]
    fn a_room_with_no_description_is_an_f_and_says_so() {
        let g = audit_room(&room(json!({"description": ""})), &AuditCtx::empty());
        assert!(g.has("room.no_desc"));
        assert_eq!(g.letter, 'F', "no desc + no exits must floor the room");
    }

    #[test]
    fn writing_the_description_fixes_the_finding() {
        let bare = audit_room(&room(json!({"description": ""})), &AuditCtx::empty());
        let filled = audit_room(&room(json!({})), &AuditCtx::empty());
        assert!(bare.has("room.no_desc"));
        assert!(!filled.has("room.no_desc"));
        assert!(filled.score > bare.score);
    }

    #[test]
    fn a_placeholder_description_does_not_count_as_a_description() {
        for text in ["TODO", "n/a", "You see nothing special.", "  "] {
            let g = audit_room(&room(json!({"description": text})), &AuditCtx::empty());
            assert!(g.has("room.no_desc"), "{text:?} was accepted as a description");
        }
    }

    #[test]
    fn a_short_description_warns_but_does_not_block() {
        let g = audit_room(&room(json!({"description": "A small room."})), &AuditCtx::empty());
        assert!(g.has("room.thin_desc"));
        assert!(!g.has("room.no_desc"));
    }

    #[test]
    fn a_room_with_no_exits_is_blocked_unless_it_is_a_property_template() {
        assert!(audit_room(&room(json!({})), &AuditCtx::empty()).has("room.no_exits"));
        assert!(!audit_room(&room(json!({"is_property_template": true})), &AuditCtx::empty()).has("room.no_exits"));
    }

    #[test]
    fn a_dangling_exit_is_only_reported_when_we_can_see_the_neighbourhood() {
        let a = room(json!({"exits": {"north": Uuid::new_v4()}}));
        // Unlinked: we were not given the destination, so we must not guess.
        assert!(!audit_room(&a, &AuditCtx::empty()).has("room.dangling_exit"));
        // Linked, and the destination genuinely is not there.
        let ctx = AuditCtx::build(std::slice::from_ref(&a));
        assert!(audit_room(&a, &ctx).has("room.dangling_exit"));
    }

    #[test]
    fn a_one_way_exit_warns_and_a_reciprocal_pair_does_not() {
        let b_id = Uuid::new_v4();
        let a = room(json!({"exits": {"north": b_id}}));
        let one_way = room(json!({"id": b_id, "exits": {}}));
        let ctx = AuditCtx::build(&[a.clone(), one_way]);
        assert!(audit_room(&a, &ctx).has("room.one_way_exit"));

        let paired = room(json!({"id": b_id, "exits": {"south": a.id}}));
        let ctx = AuditCtx::build(&[a.clone(), paired]);
        assert!(!audit_room(&a, &ctx).has("room.one_way_exit"));
    }

    #[test]
    fn copied_descriptions_are_caught() {
        let a = room(json!({"exits": {"north": Uuid::new_v4()}}));
        let mut b = a.clone();
        b.id = Uuid::new_v4();
        let ctx = AuditCtx::build(&[a.clone(), b]);
        assert!(audit_room(&a, &ctx).has("room.duplicate_desc"));
    }

    #[test]
    fn mxp_hazards_are_flagged() {
        let g = audit_room(&room(json!({"title": "The <Iron> Gate"})), &AuditCtx::empty());
        assert!(g.has("room.mxp_hazard"));
    }

    #[test]
    fn a_bare_but_valid_room_lands_around_a_c() {
        // Title, real description, an exit, and nothing else. This is the
        // calibration case for the whole scale: if this reads as an A the
        // grade means nothing.
        let dest = Uuid::new_v4();
        let r = room(json!({"exits": {"north": dest}}));
        let other = room(json!({"id": dest, "exits": {"south": r.id}, "description":
            "A different stretch of road, rutted deep by carts and softened at the edges by moss."}));
        let ctx = AuditCtx::build(&[r.clone(), other]);
        let g = audit_room(&r, &ctx);
        assert!(g.count(Severity::Blocker) == 0, "{:?}", g.findings);
        assert!(
            g.letter == 'C' || g.letter == 'B',
            "bare valid room graded {} ({})",
            g.letter,
            g.score
        );
    }

    // --- mobiles -------------------------------------------------------------

    #[test]
    fn a_noun_missing_from_keywords_is_reported() {
        // Fixture name is "a goblin guard", short_desc "A goblin guard leans on
        // a rusted pike." With only "goblin" in keywords, "pike" is genuinely
        // unreachable — but "guard" is not, because the name matches it.
        let g = audit_mobile(&mobile(json!({"keywords": ["goblin"]})));
        assert!(g.has("mobile.keywords_miss_nouns"));
        let msg = &g
            .findings
            .iter()
            .find(|f| f.code == "mobile.keywords_miss_nouns")
            .unwrap()
            .message;
        assert!(msg.contains("pike"), "{msg}");
        assert!(!msg.contains("guard"), "the name already reaches it: {msg}");
    }

    #[test]
    fn plurals_do_not_trip_the_keyword_check() {
        let g = audit_mobile(&mobile(json!({
            "short_desc": "Two goblin guards block the road.",
            "keywords": ["goblin", "guard", "road"],
        })));
        assert!(!g.has("mobile.keywords_miss_nouns"), "{:?}", g.findings);
    }

    #[test]
    fn junk_keywords_cannot_buy_off_the_coverage_check() {
        // An empty keyword used to satisfy every word, because
        // "anything".contains("") is true, and one-letter keywords came close.
        // A check a builder can delete with junk is worse than no check.
        for junk in [json!([""]), json!(["e", "a", "o"]), json!(["x", ""])] {
            let g = audit_mobile(&mobile(json!({"keywords": junk})));
            assert!(
                g.has("mobile.keywords_miss_nouns"),
                "junk keywords {junk} silenced the check"
            );
        }
    }

    #[test]
    fn the_name_is_addressable_so_empty_keywords_are_not_a_blocker() {
        // `item_matches_keyword` tests `name` by substring before it looks at
        // `keywords`, and mobile lookup calls it — so "a goblin guard" answers
        // to `kill guard` with no keywords at all. Blocking that is the auditor
        // reporting a fault in working content.
        let g = audit_mobile(&mobile(json!({"keywords": []})));
        assert!(!g.has("mobile.no_keywords"), "{:?}", g.findings);
        // The nouns the name does *not* reach are still the real signal.
        assert!(g.has("mobile.keywords_miss_nouns"));
    }

    #[test]
    fn nothing_addressable_at_all_is_still_a_blocker() {
        let g = audit_mobile(&mobile(json!({"name": "", "keywords": []})));
        assert!(g.has("mobile.no_keywords"), "{:?}", g.findings);
    }

    #[test]
    fn a_name_covering_every_noun_clears_the_check() {
        let g = audit_mobile(&mobile(json!({
            "name": "a goblin guard with a rusted pike",
            "keywords": [],
        })));
        assert!(!g.has("mobile.no_keywords"), "{:?}", g.findings);
        assert!(!g.has("mobile.keywords_miss_nouns"), "{:?}", g.findings);
    }

    #[test]
    fn level_zero_blocks() {
        assert!(audit_mobile(&mobile(json!({"level": 0}))).has("mobile.no_level"));
        assert!(!audit_mobile(&mobile(json!({}))).has("mobile.no_level"));
    }

    #[test]
    fn a_shopkeeper_with_an_empty_shelf_is_reported() {
        let g = audit_mobile(&mobile(json!({"flags": {"shopkeeper": true}})));
        assert!(g.has("mobile.shop_empty"));
        let stocked = audit_mobile(&mobile(json!({
            "flags": {"shopkeeper": true},
            "shop_stock": ["bread"],
        })));
        assert!(!stocked.has("mobile.shop_empty"));
    }

    #[test]
    fn an_inert_mobile_is_polish_not_a_blocker() {
        let g = audit_mobile(&mobile(json!({})));
        assert!(g.has("mobile.inert"));
        assert_eq!(g.count(Severity::Blocker), 0);
    }

    // --- items ---------------------------------------------------------------

    #[test]
    fn an_item_named_for_itself_needs_no_keywords() {
        // The reported case: a real area full of F-graded items whose names
        // matched their short descs, all of them perfectly targetable in game.
        let g = audit_item(&item(json!({
            "name": "a bull whip",
            "short_desc": "A bull whip lies here.",
            "keywords": [],
        })));
        assert!(!g.has("item.no_keywords"), "{:?}", g.findings);
        assert!(!g.has("item.keywords_miss_nouns"), "{:?}", g.findings);
    }

    #[test]
    fn an_item_whose_name_shares_nothing_with_its_short_desc_is_warned() {
        let g = audit_item(&item(json!({
            "name": "widget",
            "short_desc": "A bull whip lies here.",
            "keywords": [],
        })));
        assert!(!g.has("item.no_keywords"), "{:?}", g.findings);
        assert!(g.has("item.keywords_miss_nouns"), "{:?}", g.findings);
    }

    #[test]
    fn an_item_with_neither_name_nor_keywords_is_blocked() {
        let g = audit_item(&item(json!({"name": "", "keywords": []})));
        assert!(g.has("item.no_keywords"), "{:?}", g.findings);
    }

    #[test]
    fn a_weapon_with_no_dice_is_blocked() {
        let g = audit_item(&item(json!({"damage_dice_count": 0})));
        assert!(g.has("item.weapon_no_damage"));
    }

    #[test]
    fn armor_needs_protection_and_a_slot() {
        let g = audit_item(&item(json!({
            "item_type": "armor",
            "damage_dice_count": 0,
            "damage_dice_sides": 0,
            "weapon_skill": null,
        })));
        assert!(g.has("item.armor_no_protection"));
        assert!(g.has("item.armor_no_wear_location"));
        assert_eq!(g.letter, 'F');
    }

    #[test]
    fn a_container_that_holds_nothing_is_blocked() {
        let g = audit_item(&item(json!({
            "item_type": "container",
            "damage_dice_count": 0,
            "weapon_skill": null,
        })));
        assert!(g.has("item.container_no_capacity"));
    }

    #[test]
    fn a_complete_weapon_grades_well() {
        let g = audit_item(&item(json!({
            "extra_descs": [{"keywords": ["pike", "haft"], "description": "The haft is split."}],
        })));
        assert_eq!(g.count(Severity::Blocker), 0, "{:?}", g.findings);
        assert!(letter_at_least(g.letter, 'B'), "graded {} ({})", g.letter, g.score);
    }

    // --- quests --------------------------------------------------------------

    #[test]
    fn a_quest_with_no_objectives_or_rewards_is_floored() {
        let q: QuestData = serde_json::from_value(json!({
            "vnum": "q1", "name": "Test", "summary": "A test.",
            "description": "", "completion_text": "",
            "objectives": [], "rewards": [],
        }))
        .unwrap();
        let g = audit_quest(&q);
        assert!(g.has("quest.no_objectives"));
        assert!(g.has("quest.no_rewards"));
        assert_eq!(g.letter, 'F');
    }

    // --- rollups -------------------------------------------------------------

    #[test]
    fn a_container_score_blends_its_own_findings_with_its_children() {
        let own = Grade::from_findings(vec![]);
        let good = AuditEntry {
            kind: EntityKind::Room,
            label: "1".into(),
            name: "x".into(),
            grade: Grade::from_findings(vec![]),
        };
        let bad = AuditEntry {
            kind: EntityKind::Room,
            label: "2".into(),
            name: "y".into(),
            grade: Grade::from_findings(vec![blocker("a", "x"), blocker("b", "y")]),
        };
        let r = AuditReport::build("area", own, vec![good, bad]);
        // Own 100, children (100 + 10) / 2 = 55 -> 40 + 33 = 73.
        assert_eq!(r.score, 73);
    }

    #[test]
    fn an_empty_container_scores_on_its_own_findings_alone() {
        let r = AuditReport::build("area", Grade::from_findings(vec![warn("a", "x")]), vec![]);
        assert_eq!(r.score, 85);
    }

    #[test]
    fn ratio_at_least_is_none_when_there_is_nothing_of_that_kind() {
        let r = AuditReport::build("area", Grade::from_findings(vec![]), vec![]);
        assert!(r.ratio_at_least(EntityKind::Room, 'C').is_none());
    }

    #[test]
    fn orphan_rooms_are_found() {
        let a = room(json!({}));
        let b = room(json!({"exits": {"south": a.id}}));
        let island = room(json!({}));
        let linked = vec![a.clone(), b, island.clone()];
        let orphans = orphan_rooms(&linked, None);
        assert_eq!(orphans, vec![island.id]);
    }

    #[test]
    fn reachability_is_undirected_so_a_one_way_entrance_is_not_an_orphan() {
        let a = room(json!({}));
        let b = room(json!({"exits": {"south": a.id}}));
        assert!(orphan_rooms(&[a, b], None).is_empty());
    }

    #[test]
    fn the_walk_starts_at_the_declared_entrance_not_at_whatever_sorted_first() {
        // The disconnected room sorts first. Walking from it would report the
        // two connected rooms as orphans — the whole area inverted.
        let island = room(json!({"vnum": "isle"}));
        let hall = room(json!({"vnum": "hall"}));
        let yard = room(json!({"vnum": "yard", "exits": {"south": hall.id}}));
        let rooms = vec![island.clone(), hall, yard];

        assert_eq!(
            orphan_rooms(&rooms, Some("hall")),
            vec![island.id],
            "with an entrance declared, only the island is orphaned"
        );
        assert_eq!(
            orphan_rooms(&rooms, Some("isle")).len(),
            2,
            "and this is the failure the entrance exists to prevent"
        );
    }

    #[test]
    fn a_property_template_is_not_an_orphan() {
        // Templates have no exits until instantiated — `room.no_exits` already
        // exempts them, and an area is not broken for holding one.
        let hall = room(json!({"vnum": "hall"}));
        let template = room(json!({"vnum": "tpl", "is_property_template": true}));
        assert!(orphan_rooms(&[hall, template], Some("hall")).is_empty());
    }

    // --- world ---------------------------------------------------------------

    #[test]
    fn the_demo_shaped_world_produces_real_findings() {
        // Five areas, 56 rooms, no quests, no boards — the shape the repo
        // ships. If this comes back clean the auditor is not earning its keep.
        let facts = WorldFacts {
            area_count: 5,
            room_count: 56,
            item_count: 48,
            mobile_count: 15,
            quest_count: 0,
            spawn_point_count: 22,
            connected_areas: 5,
            recall_rooms: 1,
            ..Default::default()
        };
        let g = audit_world(&facts);
        assert!(g.has("world.no_quests"));
        assert!(g.has("world.no_boards"));
        assert!(g.has("world.no_post_office"));
        assert!(g.count(Severity::Blocker) > 0);
    }

    #[test]
    fn a_developed_world_passes() {
        let facts = WorldFacts {
            area_count: 12,
            room_count: 400,
            item_count: 300,
            mobile_count: 150,
            quest_count: 30,
            spawn_point_count: 260,
            recipe_count: 20,
            transport_count: 3,
            post_office_rooms: 2,
            board_items: 3,
            recall_rooms: 4,
            bank_rooms: 2,
            connected_areas: 12,
            dialogue_trees: 25,
            unfiled_mobiles: 0,
            unfiled_items: 0,
        };
        let g = audit_world(&facts);
        assert!(g.findings.is_empty(), "{:?}", g.findings);
        assert_eq!(g.letter, 'A');
    }
}
