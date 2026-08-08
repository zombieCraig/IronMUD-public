//! Unified skill-progression chokepoint.
//!
//! Every path that awards skill XP routes through here. Before this module
//! existed there were four independent implementations — the Rhai
//! `add_skill_experience`, the combat tick, the quest `SkillXp` reward, and
//! the dialogue/DG `award_skill_xp` — which had drifted apart in three ways:
//!
//! 1. **Curve.** Combat and Rhai used the real `xp_for_level` table; quest
//!    and dialogue used a flat 100-per-level loop, so quest XP advanced a
//!    skill up to 33x faster at the top end.
//! 2. **Trait modifiers.** Only the Rhai path applied `prodigy` /
//!    `quick_study` / `slow_learner` / `linguist` / `tongue_tied`.
//! 3. **Achievement hooks.** Only the Rhai and `set_skill_level` paths fired
//!    `skill_reached` / `skills_maxed`. A player who reached foraging 3 via a
//!    quest reward silently failed to unlock `novice_forager`.
//!
//! The split between [`award_xp_to_character`] (pure mutation) and
//! [`report_xp`] / [`notify_xp_achievements`] (messaging and side effects) is
//! deliberate: the combat tick owns its own `&mut CharacterData` and persists
//! it at the end of the round, so it must report *after* that save. Firing
//! out-of-band writes mid-round lets the round-end save clobber them — the
//! same hazard the kill-credit notifications already work around.
//!
//! Callers that do not already hold the character use the [`award_xp`]
//! facade, which does load -> award -> save -> sync session -> report ->
//! notify in the correct order.
//!
//! Per-spell mastery (`CharacterData.spell_progress`) is intentionally NOT
//! routed through here. It already shares `xp_for_level` and already applies
//! the learning traits, and its level-up path carries spell-evolution
//! semantics that do not generalise. It was never one of the divergent paths.

use crate::db::Db;
use crate::script::characters::xp_for_level;
use crate::types::CharacterData;
use crate::{SharedConnections, SharedState, SkillProgress};

/// Highest attainable skill level.
pub const MAX_SKILL_LEVEL: i32 = 10;

/// The canonical core skill set — the eighteen skills every world ships with,
/// and the only ones a "N of M" denominator can be built from.
///
/// `CharacterData.skills` is a wider map than this: languages share it (see
/// `src/script/lang.rs`), `bash` is awarded into it by `bash.rhai`, and
/// builder worlds add their own. Summing that whole map against a fixed
/// eighteen-skill denominator is what let `status` print `Mastered: 19/18`.
///
/// Kept sorted so the parity guard against
/// `scripts/lib/progress_ui.rhai::core_skill_categories()` — which owns the
/// *display grouping* of the same set — can compare the two directly.
pub const CORE_SKILLS: &[&str] = &[
    "cooking",
    "crafting",
    "fishing",
    "foraging",
    "gardening",
    "long_blades",
    "long_blunt",
    "magic",
    "medical",
    "polearms",
    "ranged",
    "short_blades",
    "short_blunt",
    "stealth",
    "swimming",
    "thievery",
    "tracking",
    "unarmed",
];

/// Where an XP award came from. Not shown to the player — it exists so the
/// feedback layer can make per-source presentation choices (see
/// [`XpSource::batches`]) and so future telemetry has a dimension to group on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XpSource {
    Combat,
    Quest,
    Dialogue,
    Craft,
    Gather,
    Medical,
    Stealth,
    Language,
    Movement,
    Teach,
    /// Spell-adjacent rites that advance a mastery track — currently the
    /// necromancy raise.
    Ritual,
    Admin,
}

impl XpSource {
    /// Parse the string form used by the Rhai binding. Unknown values fall
    /// back to `Admin` rather than failing the award — a mistyped source in a
    /// script should not cost the player their XP.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "combat" => XpSource::Combat,
            "quest" => XpSource::Quest,
            "dialogue" => XpSource::Dialogue,
            "craft" => XpSource::Craft,
            "gather" => XpSource::Gather,
            "medical" => XpSource::Medical,
            "stealth" => XpSource::Stealth,
            "language" => XpSource::Language,
            "movement" => XpSource::Movement,
            "teach" => XpSource::Teach,
            "ritual" => XpSource::Ritual,
            _ => XpSource::Admin,
        }
    }

    /// Whether awards from this source collapse into the batched `brief` line
    /// rather than printing as they land.
    ///
    /// The split is by *cadence*, not by importance. High-frequency sources
    /// fire many times for one player decision — a combat round, a walk across
    /// a zone, a foraging spree, every sentence spoken in a foreign tongue —
    /// and a line each would bury the rest of the scroll. Those batch.
    ///
    /// Low-frequency sources fire once, as the direct result of a deliberate
    /// act, and the award *is* the event: finishing a quest, paying a mentor,
    /// completing a rite, landing a craft, saving someone's life. Folding those
    /// into a summary line severs the award from what caused it, so they print
    /// immediately even in `brief`.
    ///
    /// `Full` prints everything regardless; `Off` prints nothing. This only
    /// decides what `brief` does.
    pub fn batches(self) -> bool {
        match self {
            XpSource::Combat
            | XpSource::Gather
            | XpSource::Movement
            | XpSource::Stealth
            | XpSource::Language
            | XpSource::Admin => true,
            XpSource::Quest
            | XpSource::Dialogue
            | XpSource::Craft
            | XpSource::Medical
            | XpSource::Teach
            | XpSource::Ritual => false,
        }
    }
}

/// What actually happened to a skill track as a result of an award.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XpOutcome {
    /// XP actually credited after trait modifiers. Zero when the track was
    /// already at max level.
    pub applied: i32,
    pub before_level: i32,
    pub after_level: i32,
    /// XP banked toward the *next* level after the award resolved.
    pub experience: i32,
    /// XP needed to clear `after_level`. Zero at max.
    pub to_next: i32,
    pub leveled: bool,
    /// True only on the award that pushed the skill to `MAX_SKILL_LEVEL`.
    pub maxed: bool,
}

impl XpOutcome {
    /// Nothing happened — the caller had no character, or the skill was
    /// already mastered.
    pub fn none(level: i32) -> Self {
        XpOutcome {
            applied: 0,
            before_level: level,
            after_level: level,
            experience: 0,
            to_next: 0,
            leveled: false,
            maxed: false,
        }
    }
}

/// Apply the learning-rate traits. Mirrors the multiplicative stacking the
/// Rhai path has always used: the general learning traits pick one of
/// prodigy/quick_study (prodigy wins), `slow_learner` stacks on top of
/// either, and the language traits stack on top of that again.
///
/// Reads `ch.traits` rather than the class-merged effective set, matching
/// pre-existing behaviour — folding granted traits in here would silently
/// change XP rates for every class with `granted_traits`.
fn apply_trait_modifiers(ch: &CharacterData, amount: i32, is_language: bool) -> i32 {
    let has = |id: &str| ch.traits.iter().any(|t| t == id);

    let mut xp = amount;
    if has("prodigy") {
        xp = xp * 150 / 100;
    } else if has("quick_study") {
        xp = xp * 125 / 100;
    }
    if has("slow_learner") {
        xp = xp * 65 / 100;
    }
    if is_language {
        if has("linguist") {
            xp = xp * 150 / 100;
        }
        if has("tongue_tied") {
            xp = xp * 65 / 100;
        }
    }
    // A positive award never rounds down to nothing.
    if amount > 0 { xp.max(1) } else { xp }
}

/// Credit skill XP against a character in memory. Pure — no I/O, no
/// messaging, no achievement hooks. Safe to call from inside a tick that
/// already holds `&mut CharacterData`.
///
/// `is_language` is a parameter rather than something this function derives
/// because the language check needs the World lock, which the combat tick
/// must not take while it holds its own state.
pub fn award_xp_to_character(ch: &mut CharacterData, skill: &str, amount: i32, is_language: bool) -> XpOutcome {
    let skill_key = skill.to_lowercase();
    let before_level = ch.skills.get(&skill_key).map(|s| s.level).unwrap_or(0);

    if before_level >= MAX_SKILL_LEVEL {
        return XpOutcome::none(before_level);
    }
    if amount <= 0 {
        let entry = ch.skills.entry(skill_key).or_insert_with(SkillProgress::default);
        return XpOutcome {
            applied: 0,
            before_level,
            after_level: entry.level,
            experience: entry.experience,
            to_next: xp_for_level(entry.level),
            leveled: false,
            maxed: false,
        };
    }

    let xp = apply_trait_modifiers(ch, amount, is_language);

    let entry = ch.skills.entry(skill_key).or_insert_with(SkillProgress::default);
    entry.experience += xp;

    // May clear several levels at once on a large award.
    loop {
        let needed = xp_for_level(entry.level);
        if needed == 0 || entry.experience < needed || entry.level >= MAX_SKILL_LEVEL {
            break;
        }
        entry.experience -= needed;
        entry.level += 1;
        if entry.level >= MAX_SKILL_LEVEL {
            entry.experience = 0; // no overflow banked at mastery
            break;
        }
    }

    let after_level = entry.level;
    XpOutcome {
        applied: xp,
        before_level,
        after_level,
        experience: entry.experience,
        to_next: xp_for_level(after_level),
        leveled: after_level > before_level,
        maxed: after_level >= MAX_SKILL_LEVEL && before_level < MAX_SKILL_LEVEL,
    }
}

/// Human-readable skill name for player-facing text.
pub fn skill_display_name(skill: &str) -> String {
    skill.replace('_', " ")
}

/// Ten-cell mastery bar, one cell per level.
fn level_bar(level: i32) -> String {
    let filled = level.clamp(0, MAX_SKILL_LEVEL) as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(MAX_SKILL_LEVEL as usize - filled)
    )
}

/// The single level-up announcement, replacing the six formats that used to
/// live in the combat tick, the quest reward, the dialogue effect, and the
/// craft / cook / fish / garden scripts.
pub fn format_level_up(skill: &str, outcome: &XpOutcome, colors: bool) -> String {
    let name = title_case(&skill_display_name(skill));
    let (bright, reset) = if colors { ("\x1b[1;33m", "\x1b[0m") } else { ("", "") };

    if outcome.maxed {
        format!(
            "{}*** MASTERED — {}. ***{}\n    {}",
            bright,
            name,
            reset,
            level_bar(outcome.after_level)
        )
    } else {
        format!(
            "{}*** Your {} skill rises to {}. ***{}\n    {}  next: {} xp",
            bright,
            name,
            outcome.after_level,
            reset,
            level_bar(outcome.after_level),
            outcome.to_next
        )
    }
}

/// Compact per-award line used by the `full` feed mode.
pub fn format_xp_tick(skill: &str, outcome: &XpOutcome) -> String {
    format!("(+{} {})", outcome.applied, skill_display_name(skill))
}

/// Batched line used by the `brief` feed mode, flushed at prompt time.
pub fn format_xp_batch(skill: &str, tally: &XpTally) -> String {
    if tally.to_next > 0 {
        format!(
            "[ +{} {}  {}/{} → {} ]",
            tally.applied,
            skill_display_name(skill),
            tally.experience,
            tally.to_next,
            tally.level
        )
    } else {
        format!("[ +{} {}  mastered ]", tally.applied, skill_display_name(skill))
    }
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Accumulated XP for one skill within a single command, pending flush.
#[derive(Clone, Debug, Default)]
pub struct XpTally {
    pub applied: i32,
    pub level: i32,
    pub experience: i32,
    pub to_next: i32,
}

/// Verbosity of the XP feed, from `CharacterData.xp_feed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XpFeed {
    Off,
    Brief,
    Full,
}

impl XpFeed {
    /// Empty / unrecognised persists as `Brief`, the default for new and
    /// pre-existing characters alike.
    pub fn from_field(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "off" | "none" => XpFeed::Off,
            "full" => XpFeed::Full,
            _ => XpFeed::Brief,
        }
    }

    pub fn as_field(&self) -> &'static str {
        match self {
            XpFeed::Off => "off",
            XpFeed::Brief => "brief",
            XpFeed::Full => "full",
        }
    }
}

/// Report an award to the player, honouring their `set xpfeed` preference.
///
/// Level-ups always print immediately regardless of mode (short of `off`) —
/// they are the milestone beat and must not be swallowed by batching. Below
/// that, `brief` batches only the sources that repeat; see
/// [`XpSource::batches`].
///
/// Takes the connections lock once, and does not call back into anything that
/// takes it again.
pub fn report_xp(connections: &SharedConnections, char_name: &str, skill: &str, outcome: &XpOutcome, source: XpSource) {
    if outcome.applied <= 0 && !outcome.leveled {
        return;
    }
    let skill_key = skill.to_lowercase();

    let Ok(mut conns) = connections.lock() else {
        return;
    };
    for (_, session) in conns.iter_mut() {
        let matches = session
            .character
            .as_ref()
            .map(|c| c.name.eq_ignore_ascii_case(char_name))
            .unwrap_or(false);
        if !matches {
            continue;
        }

        let feed = session
            .character
            .as_ref()
            .map(|c| XpFeed::from_field(&c.xp_feed))
            .unwrap_or(XpFeed::Brief);
        if feed == XpFeed::Off {
            return;
        }
        let colors = session.colors_enabled;

        if outcome.leveled {
            // Drop any pending ticks for this skill — the level-up line
            // supersedes them and reprinting both reads as double-counting.
            session.xp_buffer.remove(&skill_key);
            let _ = session
                .sender
                .send(format!("{}\n", format_level_up(skill, outcome, colors)));
            return;
        }

        match feed {
            XpFeed::Full => {
                let _ = session.sender.send(format!("{}\n", format_xp_tick(skill, outcome)));
            }
            XpFeed::Brief if source.batches() => {
                let tally = session.xp_buffer.entry(skill_key).or_default();
                tally.applied += outcome.applied;
                tally.level = outcome.after_level;
                tally.experience = outcome.experience;
                tally.to_next = outcome.to_next;
            }
            XpFeed::Brief => {
                // A one-shot award. Print the batch form immediately — same
                // shape the flush would have produced, so the player sees one
                // consistent surface either way, but attached to the act that
                // earned it. Fold in anything already pending for this skill
                // so the pending ticks are not reported twice.
                let mut tally = session.xp_buffer.remove(&skill_key).unwrap_or_default();
                tally.applied += outcome.applied;
                tally.level = outcome.after_level;
                tally.experience = outcome.experience;
                tally.to_next = outcome.to_next;
                let _ = session.sender.send(format!("{}\n", format_xp_batch(skill, &tally)));
            }
            XpFeed::Off => {}
        }
        return;
    }
}

/// Drain a session's batched XP into display lines.
///
/// Called from `build_prompt` while the connections lock is already held, so
/// it takes the session rather than the map.
pub fn drain_xp_buffer(session: &mut crate::PlayerSession) -> Vec<String> {
    if session.xp_buffer.is_empty() {
        return Vec::new();
    }
    let drained = std::mem::take(&mut session.xp_buffer);
    drained
        .iter()
        .map(|(skill, tally)| format_xp_batch(skill, tally))
        .collect()
}

/// Fire the achievement hooks a skill gain implies. Idempotent and safe to
/// call after the caller's own save — which is exactly where it belongs, so a
/// round-end character save cannot clobber the award.
pub fn notify_xp_achievements(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    skill: &str,
    outcome: &XpOutcome,
) {
    if !outcome.leveled {
        return;
    }
    crate::script::achievements::notify_event_core(
        db,
        connections,
        state,
        char_name,
        "skill_reached",
        &format!("{}:{}", skill.to_lowercase(), outcome.after_level),
    );
    if outcome.maxed {
        crate::script::achievements::notify_counter_core(db, connections, state, char_name, "skills_maxed", 1);
    }
}

/// Every skill's level, for [`notify_skill_level_gains`] to diff against.
///
/// Cheap — a character carries on the order of twenty skills — and taken
/// before a block of work rather than threaded through it.
pub fn skill_level_snapshot(ch: &CharacterData) -> std::collections::HashMap<String, i32> {
    ch.skills.iter().map(|(k, sp)| (k.clone(), sp.level)).collect()
}

/// [`notify_xp_achievements`] for a caller that does not hold an [`XpOutcome`].
///
/// Some callers own a `&mut CharacterData` and persist it themselves, so they
/// cannot use the [`award_xp`] facade — the facade's own save would be
/// overwritten by theirs. The dialogue effect layer is the standing example:
/// it mutates the character across a whole walk of effects and saves once at
/// the end. Threading an outcome out of every effect would mean a signature
/// change on every function between the effect and the save; comparing a
/// snapshot does the same job at the one place that already knows both.
///
/// Fires the same two hooks `notify_xp_achievements` does, on the same rule
/// (a level rose), and must be called for the same reason **after** the
/// caller's save: the hooks write the character out-of-band.
///
/// Generic in the useful direction — it credits any route that raised a
/// skill, not just an XP award, so a future effect that sets a level directly
/// is covered without touching this.
pub fn notify_skill_level_gains(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    before: &std::collections::HashMap<String, i32>,
    after: &CharacterData,
) {
    for (skill, sp) in &after.skills {
        let was = before.get(skill).copied().unwrap_or(0);
        if sp.level <= was {
            continue;
        }
        crate::script::achievements::notify_event_core(
            db,
            connections,
            state,
            char_name,
            "skill_reached",
            &format!("{}:{}", skill.to_lowercase(), sp.level),
        );
        if sp.level >= MAX_SKILL_LEVEL && was < MAX_SKILL_LEVEL {
            crate::script::achievements::notify_counter_core(db, connections, state, char_name, "skills_maxed", 1);
        }
    }
}

/// Is this skill key a spoken language? Needs the World lock, so callers
/// inside a tick that holds world state should pass `false` explicitly rather
/// than call this.
pub fn is_language_skill(state: &SharedState, skill: &str) -> bool {
    state
        .lock()
        .ok()
        .map(|w| w.language_definitions.contains_key(&skill.to_lowercase()))
        .unwrap_or(false)
}

/// Push the updated character into the live session so an online player's
/// in-memory copy does not go stale and get flushed back over the award by
/// the regen tick.
fn sync_to_session(connections: &SharedConnections, ch: &CharacterData) {
    if let Ok(mut conns) = connections.lock() {
        for (_, session) in conns.iter_mut() {
            let matches = session
                .character
                .as_ref()
                .map(|c| c.name.eq_ignore_ascii_case(&ch.name))
                .unwrap_or(false);
            if matches {
                session.character = Some(ch.clone());
                return;
            }
        }
    }
}

/// One-call facade for every caller that does not already hold the character:
/// Rhai scripts, quest rewards, dialogue effects, DG commands.
///
/// Order is load -> award -> save -> sync session -> report -> notify. The
/// notify step runs last so achievement writes land after the character save
/// rather than being overwritten by it.
pub fn award_xp(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    skill: &str,
    amount: i32,
    source: XpSource,
) -> XpOutcome {
    let Ok(Some(mut ch)) = db.get_character_data(&char_name.to_lowercase()) else {
        return XpOutcome::default();
    };

    let is_lang = is_language_skill(state, skill);
    let outcome = award_xp_to_character(&mut ch, skill, amount, is_lang);
    if outcome.applied == 0 && !outcome.leveled {
        return outcome;
    }

    if db.save_character_data(ch.clone()).is_err() {
        return XpOutcome::default();
    }
    sync_to_session(connections, &ch);
    report_xp(connections, char_name, skill, &outcome, source);
    notify_xp_achievements(db, connections, state, char_name, skill, &outcome);
    outcome
}

/// The components behind a renown score, kept separate so `status` can show
/// the player *why* their number is what it is rather than an opaque total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Renown {
    pub total: i32,
    /// Sum of every skill level, the same quantity `get_effective_level`
    /// reports. Includes languages, `bash` and any builder-defined skill,
    /// because breadth is exactly what Renown is measuring.
    pub skill_levels: i32,
    /// Skills standing at [`MAX_SKILL_LEVEL`], counted across the same wide
    /// set as `skill_levels`.
    pub mastered: i32,
    /// `skill_levels` restricted to [`CORE_SKILLS`]. This is the figure a
    /// display can safely put over a fixed `CORE_SKILLS.len() * 10`
    /// denominator; the wide one cannot, and used to overflow it.
    pub core_skill_levels: i32,
    /// `mastered` restricted to [`CORE_SKILLS`], bounded by
    /// `CORE_SKILLS.len()`.
    pub core_mastered: i32,
    pub achievements: i32,
    pub quests: i32,
    /// Sum of per-spell mastery levels.
    pub spell_levels: i32,
}

/// A derived breadth score. Deliberately *not* a stored field and not a
/// character level: it gates nothing, is never persisted, and needs no
/// migration or balance pass. It exists so a classless skill MUD has one
/// legible number to show on `status`, compare on `who`, and rank on a
/// leaderboard.
///
/// Mastery, achievements and quests are weighted above raw skill levels
/// because a specialist and a generalist can reach the same skill sum by
/// very different routes, and the raw sum alone cannot tell them apart.
/// Spell mastery is halved because spells are numerous and each individual
/// track is shallow relative to a skill.
///
/// If players eventually ask for level gates, this is the field to promote —
/// with real usage data in hand. Do not build the stored version first.
pub fn renown(ch: &CharacterData) -> Renown {
    let skill_levels: i32 = ch.skills.values().map(|s| s.level).sum();
    let mastered = ch.skills.values().filter(|s| s.level >= MAX_SKILL_LEVEL).count() as i32;
    let achievements = ch.achievements_unlocked.len() as i32;
    let quests = ch.completed_quests.len() as i32;
    let spell_levels: i32 = ch.spell_progress.values().map(|s| s.level).sum();

    // Scored on the wide totals — a polyglot's languages are real progression
    // and should raise their Renown. The core figures below exist only so a
    // display has something with a stable denominator.
    let total = skill_levels + 3 * mastered + 2 * achievements + quests + spell_levels / 2;

    let core: Vec<i32> = CORE_SKILLS
        .iter()
        .filter_map(|k| ch.skills.get(*k).map(|s| s.level))
        .collect();
    let core_skill_levels: i32 = core.iter().sum();
    let core_mastered = core.iter().filter(|l| **l >= MAX_SKILL_LEVEL).count() as i32;

    Renown {
        total,
        skill_levels,
        mastered,
        core_skill_levels,
        core_mastered,
        achievements,
        quests,
        spell_levels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CharacterData has no `Default`, so build it from JSON and let serde's
    /// field defaults fill the rest — same approach the dialogue tests use.
    fn char_with(skill: &str, level: i32, experience: i32) -> CharacterData {
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "Tester",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.skills.insert(skill.to_string(), SkillProgress { level, experience });
        ch
    }

    #[test]
    fn uses_the_real_curve_not_flat_hundred() {
        // The quest and dialogue paths used to level every 100 XP. At level 3
        // the real cost is 550, so 100 XP must NOT level.
        let mut ch = char_with("foraging", 3, 0);
        let out = award_xp_to_character(&mut ch, "foraging", 100, false);
        assert!(!out.leveled);
        assert_eq!(out.after_level, 3);
        assert_eq!(out.experience, 100);
        assert_eq!(out.to_next, 550);
    }

    #[test]
    fn levels_when_the_curve_is_cleared() {
        let mut ch = char_with("foraging", 3, 500);
        let out = award_xp_to_character(&mut ch, "foraging", 60, false);
        assert!(out.leveled);
        assert_eq!(out.before_level, 3);
        assert_eq!(out.after_level, 4);
        assert_eq!(out.experience, 10);
        assert_eq!(out.to_next, 800);
    }

    #[test]
    fn a_large_award_clears_several_levels() {
        let mut ch = char_with("foraging", 0, 0);
        let out = award_xp_to_character(&mut ch, "foraging", 100 + 200 + 350, false);
        assert_eq!(out.after_level, 3);
        assert_eq!(out.experience, 0);
        assert!(out.leveled);
        assert!(!out.maxed);
    }

    #[test]
    fn mastery_caps_and_flags_once() {
        let mut ch = char_with("foraging", 9, 3290);
        let out = award_xp_to_character(&mut ch, "foraging", 100, false);
        assert_eq!(out.after_level, 10);
        assert!(out.maxed);
        assert_eq!(out.experience, 0, "no XP is banked past mastery");
        assert_eq!(out.to_next, 0);

        // A second award against a mastered skill is inert and must not
        // re-fire the maxed flag (which would double-count skills_maxed).
        let again = award_xp_to_character(&mut ch, "foraging", 100, false);
        assert_eq!(again.applied, 0);
        assert!(!again.maxed);
        assert!(!again.leveled);
    }

    #[test]
    fn unknown_skill_is_created_at_zero() {
        let mut ch = char_with("foraging", 0, 0);
        let out = award_xp_to_character(&mut ch, "thievery", 10, false);
        assert_eq!(out.applied, 10);
        assert_eq!(out.after_level, 0);
        assert_eq!(ch.skills.get("thievery").unwrap().experience, 10);
    }

    #[test]
    fn skill_name_is_lowercased_on_the_way_in() {
        let mut ch = char_with("short_blades", 0, 0);
        award_xp_to_character(&mut ch, "Short_Blades", 10, false);
        assert_eq!(ch.skills.get("short_blades").unwrap().experience, 10);
        assert_eq!(ch.skills.len(), 1, "must not create a second cased entry");
    }

    #[test]
    fn learning_traits_apply_on_every_path() {
        let mut ch = char_with("foraging", 0, 0);
        ch.traits.push("prodigy".to_string());
        let out = award_xp_to_character(&mut ch, "foraging", 10, false);
        assert_eq!(out.applied, 15);
    }

    #[test]
    fn language_traits_stack_on_top_of_learning_traits() {
        let mut ch = char_with("elvish", 0, 0);
        ch.traits.push("prodigy".to_string());
        ch.traits.push("linguist".to_string());
        // 10 -> 15 (prodigy) -> 22 (linguist, integer division)
        let out = award_xp_to_character(&mut ch, "elvish", 10, true);
        assert_eq!(out.applied, 22);

        // ...and do nothing on a non-language skill.
        let mut ch2 = char_with("foraging", 0, 0);
        ch2.traits.push("linguist".to_string());
        assert_eq!(award_xp_to_character(&mut ch2, "foraging", 10, false).applied, 10);
    }

    #[test]
    fn a_positive_award_never_rounds_to_zero() {
        let mut ch = char_with("foraging", 0, 0);
        ch.traits.push("slow_learner".to_string());
        let out = award_xp_to_character(&mut ch, "foraging", 1, false);
        assert_eq!(out.applied, 1);
    }

    #[test]
    fn level_up_text_is_one_format() {
        let out = XpOutcome {
            applied: 40,
            before_level: 3,
            after_level: 4,
            experience: 10,
            to_next: 800,
            leveled: true,
            maxed: false,
        };
        let msg = format_level_up("short_blades", &out, false);
        assert!(msg.contains("Your Short Blades skill rises to 4."));
        assert!(msg.contains("[####------]"));
        assert!(msg.contains("next: 800 xp"));
    }

    #[test]
    fn mastery_text_replaces_the_level_line() {
        let out = XpOutcome {
            applied: 10,
            before_level: 9,
            after_level: 10,
            experience: 0,
            to_next: 0,
            leveled: true,
            maxed: true,
        };
        let msg = format_level_up("foraging", &out, false);
        assert!(msg.contains("MASTERED — Foraging."));
        assert!(msg.contains("[##########]"));
        assert!(!msg.contains("next:"));
    }

    /// Put `name` on a fresh connections map with a real client channel, so
    /// what `report_xp` sends can be asserted on. Mirrors the helper the
    /// worship favor tests use.
    fn online(ch: &CharacterData) -> (SharedConnections, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let (tx_client, rx_client) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(ch.clone());
        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);
        (conns, rx_client)
    }

    fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> String {
        let mut out = String::new();
        while let Ok(msg) = rx.try_recv() {
            out.push_str(&msg);
        }
        out
    }

    /// The pending tally for `skill`, or None if nothing is buffered.
    fn pending(conns: &SharedConnections, skill: &str) -> Option<XpTally> {
        conns
            .lock()
            .unwrap()
            .values()
            .next()
            .and_then(|s| s.xp_buffer.get(skill).cloned())
    }

    #[test]
    fn every_source_is_classified_by_cadence() {
        // Repeating sources collapse; one-shot sources land where they happened.
        for s in [
            XpSource::Combat,
            XpSource::Gather,
            XpSource::Movement,
            XpSource::Stealth,
            XpSource::Language,
            XpSource::Admin,
        ] {
            assert!(s.batches(), "{s:?} repeats and must batch");
        }
        for s in [
            XpSource::Quest,
            XpSource::Dialogue,
            XpSource::Craft,
            XpSource::Medical,
            XpSource::Teach,
            XpSource::Ritual,
        ] {
            assert!(!s.batches(), "{s:?} is one-shot and must print immediately");
        }
    }

    #[test]
    fn brief_batches_a_repeating_source_instead_of_printing() {
        let ch = char_with("short_blades", 3, 0);
        let (conns, mut rx) = online(&ch);

        let out = XpOutcome {
            applied: 10,
            before_level: 3,
            after_level: 3,
            experience: 10,
            to_next: 550,
            leveled: false,
            maxed: false,
        };
        report_xp(&conns, "Tester", "short_blades", &out, XpSource::Combat);

        assert_eq!(drain(&mut rx), "", "a combat tick must not print on its own");
        assert_eq!(pending(&conns, "short_blades").expect("buffered").applied, 10);
    }

    #[test]
    fn brief_prints_a_one_shot_source_where_it_happened() {
        let ch = char_with("foraging", 2, 0);
        let (conns, mut rx) = online(&ch);

        let out = XpOutcome {
            applied: 200,
            before_level: 2,
            after_level: 2,
            experience: 200,
            to_next: 350,
            leveled: false,
            maxed: false,
        };
        report_xp(&conns, "Tester", "foraging", &out, XpSource::Quest);

        assert_eq!(drain(&mut rx), "[ +200 foraging  200/350 → 2 ]\n");
        assert!(
            pending(&conns, "foraging").is_none(),
            "an immediate report must leave nothing pending to reprint at the prompt"
        );
    }

    #[test]
    fn a_one_shot_award_absorbs_the_ticks_already_pending_for_that_skill() {
        // Forage twice (batched), then hand in the quest that rewards the same
        // skill. The player must see one line totalling all three, not a quest
        // line now and the forage ticks again at the next prompt.
        let ch = char_with("foraging", 2, 0);
        let (conns, mut rx) = online(&ch);

        for _ in 0..2 {
            report_xp(
                &conns,
                "Tester",
                "foraging",
                &XpOutcome {
                    applied: 5,
                    before_level: 2,
                    after_level: 2,
                    experience: 5,
                    to_next: 350,
                    leveled: false,
                    maxed: false,
                },
                XpSource::Gather,
            );
        }
        report_xp(
            &conns,
            "Tester",
            "foraging",
            &XpOutcome {
                applied: 100,
                before_level: 2,
                after_level: 2,
                experience: 110,
                to_next: 350,
                leveled: false,
                maxed: false,
            },
            XpSource::Quest,
        );

        assert_eq!(drain(&mut rx), "[ +110 foraging  110/350 → 2 ]\n");
        assert!(pending(&conns, "foraging").is_none());
    }

    #[test]
    fn a_level_up_still_prints_immediately_whatever_the_source() {
        let ch = char_with("short_blades", 3, 540);
        let (conns, mut rx) = online(&ch);

        let out = XpOutcome {
            applied: 10,
            before_level: 3,
            after_level: 4,
            experience: 0,
            to_next: 800,
            leveled: true,
            maxed: false,
        };
        report_xp(&conns, "Tester", "short_blades", &out, XpSource::Combat);

        assert!(drain(&mut rx).contains("Your Short Blades skill rises to 4."));
        assert!(pending(&conns, "short_blades").is_none());
    }

    #[test]
    fn xpfeed_off_silences_one_shot_sources_too() {
        let mut ch = char_with("foraging", 2, 0);
        ch.xp_feed = "off".into();
        let (conns, mut rx) = online(&ch);

        report_xp(
            &conns,
            "Tester",
            "foraging",
            &XpOutcome {
                applied: 200,
                before_level: 2,
                after_level: 2,
                experience: 200,
                to_next: 350,
                leveled: false,
                maxed: false,
            },
            XpSource::Quest,
        );

        assert_eq!(drain(&mut rx), "");
        assert!(pending(&conns, "foraging").is_none());
    }

    #[test]
    fn feed_mode_parses_with_brief_as_the_default() {
        assert_eq!(XpFeed::from_field(""), XpFeed::Brief);
        assert_eq!(XpFeed::from_field("garbage"), XpFeed::Brief);
        assert_eq!(XpFeed::from_field("OFF"), XpFeed::Off);
        assert_eq!(XpFeed::from_field("Full"), XpFeed::Full);
    }

    #[test]
    fn a_fresh_character_has_no_renown() {
        let ch = char_with("foraging", 0, 0);
        assert_eq!(renown(&ch), Renown::default());
    }

    #[test]
    fn renown_sums_its_components_with_their_weights() {
        let mut ch = char_with("foraging", 10, 0);
        ch.skills.insert(
            "cooking".into(),
            SkillProgress {
                level: 4,
                experience: 0,
            },
        );
        ch.spell_progress.insert(
            "magic_missile".into(),
            crate::SpellProgress {
                level: 5,
                experience: 0,
            },
        );
        ch.completed_quests.insert("errand".into());
        ch.completed_quests.insert("delivery".into());

        let r = renown(&ch);
        assert_eq!(r.skill_levels, 14);
        assert_eq!(r.mastered, 1);
        assert_eq!(r.quests, 2);
        assert_eq!(r.spell_levels, 5);
        // 14 skill + 3*1 mastered + 2*0 achievements + 2 quests + 5/2 spells
        assert_eq!(r.total, 14 + 3 + 2 + 2);
    }

    #[test]
    fn mastery_is_worth_more_than_the_skill_levels_alone() {
        // Two characters with the same skill sum: one specialist at 10, one
        // generalist spread across five skills. The specialist's mastery is
        // what the raw sum cannot see.
        let specialist = char_with("foraging", 10, 0);
        let mut generalist = char_with("foraging", 2, 0);
        for skill in ["cooking", "fishing", "stealth", "medical"] {
            generalist.skills.insert(
                skill.into(),
                SkillProgress {
                    level: 2,
                    experience: 0,
                },
            );
        }
        assert_eq!(renown(&specialist).skill_levels, renown(&generalist).skill_levels);
        assert_eq!(renown(&specialist).total, renown(&generalist).total + 3);
    }

    #[test]
    fn spell_mastery_is_halved_and_rounds_down() {
        let mut ch = char_with("foraging", 0, 0);
        ch.spell_progress.insert(
            "spark".into(),
            crate::SpellProgress {
                level: 3,
                experience: 0,
            },
        );
        assert_eq!(renown(&ch).spell_levels, 3);
        assert_eq!(renown(&ch).total, 1);
    }
}
