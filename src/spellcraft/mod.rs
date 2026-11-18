//! Divine spellcraft: spells players seal through their god, which then
//! live permanently in the `codex` sled tree.
//!
//! This module owns the rules; `script::spellcraft` is the thin Rhai layer.
//! Stage 1 is plumbing only: codex storage and loading, the settings reader
//! and the single learning function every learn path goes through. The
//! system ships dark behind `spellcraft.enabled`.
//!
//! Rules every caller follows:
//! - Character writes go through `achievements::apply_to_character`
//!   (session first). Read World data before calling it; its closure runs
//!   under the Connections lock.
//! - Codex writes use `Db::update_codex` / `insert_codex_if_absent` and are
//!   then mirrored into `World.spell_definitions`.
//! - Thresholds come from [`Settings::load`], never literals.

pub mod canon;
pub mod codex;
pub mod deity;
pub mod draft;
pub mod fame;
pub mod gate;
pub mod heresy;
pub mod petition;
pub mod power;
pub mod price;
pub mod ritual;
pub mod scribe;
pub mod text;

use std::collections::HashMap;

use crate::db::Db;
use crate::script::achievements::apply_to_character;
use crate::types::{CodexEntry, SpellDefinition};
use crate::{SharedConnections, SharedState};

// ========== Settings ==========

/// Every spellcraft tunable, read from server settings (`admin config
/// spellcraft.<key> <value>`) with the defaults registered in
/// `crate::settings::KNOWN_SETTINGS`. Read at check time,
/// so changes need no restart. Per-god overrides on the deity config take
/// precedence where they exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Master switch. Off by default so the system can ship dark.
    pub enabled: bool,
    pub min_magic: i64,
    pub min_spellcraft: i64,
    /// Spells at mastery 10 required before petitioning.
    pub min_mastered_spells: i64,
    /// Favor with the god (500 = Exalted).
    pub min_favor: i64,
    /// Lowest power tier (1-5) a canon spell may have.
    pub min_power_tier: i64,
    /// Real days between one character's seals.
    pub char_cooldown_days: i64,
    /// Game days a god waits between petitions (per-god override wins).
    pub god_petition_cooldown_days: i64,
    /// Pledges never take max HP/stamina below this percent of base.
    pub pledge_floor_pct: i64,
    /// Game days for a released pledge to grow back.
    pub regrow_days: i64,
    /// Game days after renouncing a god before a new pact is possible.
    pub repact_cooldown_days: i64,
    /// Game days a newly sealed spell is safe from displacement.
    pub canon_grace_days: i64,
    /// Component quality needed: base + per_tier × power tier.
    pub component_quality_base: i64,
    pub component_quality_per_tier: i64,
    /// Strongest power score a drafted spell may reach (Sanctuary, the
    /// strongest built-in, scores 150).
    pub max_power: i64,
    /// Base max-HP pledge, as a percent of the spell's power score.
    pub hp_pledge_pct: i64,
    /// Real seconds per ritual stage.
    pub ritual_stage_secs: i64,
    /// Most times a ritual may be pushed.
    pub ritual_max_pushes: i64,
    /// A ritualist away (linkdead/offline) this long collapses the rite.
    pub ritual_linkdead_secs: i64,
    /// Spellcraft skill needed to inscribe a scroll (`craft scroll`).
    pub scribe_min_spellcraft: i64,
    /// Game days after sealing that only the author may inscribe a Canon
    /// spell.
    pub scribe_monopoly_days: i64,
}

impl Default for Settings {
    /// The defaults registered in `crate::settings::KNOWN_SETTINGS`.
    fn default() -> Self {
        Self::from_lookup(|_| None)
    }
}

/// Every spellcraft setting key, in `admin config` form: the
/// `spellcraft.*` entries of `crate::settings::KNOWN_SETTINGS`.
pub fn setting_keys() -> impl Iterator<Item = &'static str> {
    crate::settings::KNOWN_SETTINGS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| k.starts_with("spellcraft."))
}

impl Settings {
    /// Read every key, falling back to the registered default for any key
    /// that is unset or unparseable.
    pub fn load(db: &Db) -> Self {
        Self::from_lookup(|key| db.get_setting(key).ok().flatten())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let raw = |key: &str| -> String {
            lookup(key)
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| crate::settings::setting_default(key).unwrap_or_default().to_string())
        };
        let default_of = |key: &str| crate::settings::setting_default(key).unwrap_or_default().to_string();
        let int = |key: &str| -> i64 {
            raw(key)
                .parse::<i64>()
                .or_else(|_| default_of(key).parse::<i64>())
                .unwrap_or(0)
        };
        let enabled = parse_bool(&raw("spellcraft.enabled"))
            .or_else(|| parse_bool(&default_of("spellcraft.enabled")))
            .unwrap_or(false);
        Self {
            enabled,
            min_magic: int("spellcraft.min_magic"),
            min_spellcraft: int("spellcraft.min_spellcraft"),
            min_mastered_spells: int("spellcraft.min_mastered_spells"),
            min_favor: int("spellcraft.min_favor"),
            min_power_tier: int("spellcraft.min_power_tier"),
            char_cooldown_days: int("spellcraft.char_cooldown_days"),
            god_petition_cooldown_days: int("spellcraft.god_petition_cooldown_days"),
            pledge_floor_pct: int("spellcraft.pledge_floor_pct"),
            regrow_days: int("spellcraft.regrow_days"),
            repact_cooldown_days: int("spellcraft.repact_cooldown_days"),
            canon_grace_days: int("spellcraft.canon_grace_days"),
            component_quality_base: int("spellcraft.component_quality_base"),
            component_quality_per_tier: int("spellcraft.component_quality_per_tier"),
            max_power: int("spellcraft.max_power"),
            hp_pledge_pct: int("spellcraft.hp_pledge_pct"),
            ritual_stage_secs: int("spellcraft.ritual_stage_secs"),
            ritual_max_pushes: int("spellcraft.ritual_max_pushes"),
            ritual_linkdead_secs: int("spellcraft.ritual_linkdead_secs"),
            scribe_min_spellcraft: int("spellcraft.scribe_min_spellcraft"),
            scribe_monopoly_days: int("spellcraft.scribe_monopoly_days"),
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

// ========== Codex loading and mirroring ==========

/// The definition as it goes into `World.spell_definitions`: always marked
/// `codex` and always `scroll_only`. A spell that isn't `scroll_only` is
/// castable by anyone with the skill level, without learning it; codex
/// spells must be learned (scroll, temple, teaching), whatever the stored
/// data says.
fn world_def(entry: &CodexEntry) -> SpellDefinition {
    let mut def = entry.def.clone();
    def.codex = true;
    def.scroll_only = true;
    def
}

/// Overlay every codex spell onto `spells`. Called at the end of
/// `load_game_data`, after the preset JSON replaced the map, so `admin
/// preset` and `admin reload` keep codex spells. Returns how many loaded.
pub fn overlay_codex(db: &Db, spells: &mut HashMap<String, SpellDefinition>) -> usize {
    match db.list_codex() {
        Ok(entries) => {
            let n = entries.len();
            for entry in &entries {
                spells.insert(entry.def.id.clone(), world_def(entry));
            }
            n
        }
        Err(e) => {
            tracing::error!("Failed to load codex spells: {}", e);
            0
        }
    }
}

/// Mirror a stored codex entry into the World cache. Takes the World lock:
/// never call with Connections held.
pub fn mirror_into_world(state: &SharedState, entry: &CodexEntry) {
    state
        .lock()
        .spell_definitions
        .insert(entry.def.id.clone(), world_def(entry));
}

/// Store a new codex spell under an id nobody holds yet and make it
/// castable. Returns false (and changes nothing) if the id is taken.
pub fn insert_codex_spell(db: &Db, state: &SharedState, entry: &CodexEntry) -> anyhow::Result<bool> {
    if !db.insert_codex_if_absent(entry)? {
        return Ok(false);
    }
    mirror_into_world(state, entry);
    Ok(true)
}

// ========== Spellcraft skill ==========

/// Skill key of the spellcraft skill.
pub const SPELLCRAFT_SKILL: &str = "spellcraft";
/// Spellcraft XP when one of a character's spells reaches mastery 10 — the
/// skill's main source. Reaching the default gate of 8 (6600 XP) takes about
/// seven masteries, or five plus ritual aid and scroll work.
pub const XP_PER_MASTERY: i32 = 900;
/// Spellcraft XP for learning a codex spell.
pub const XP_PER_CODEX_LEARN: i32 = 50;

/// A spell reached mastery 10. Call after the character write, never under
/// the Connections lock.
pub fn on_spell_mastered(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str) {
    crate::progress::award_xp(
        db,
        connections,
        state,
        char_name,
        SPELLCRAFT_SKILL,
        XP_PER_MASTERY,
        crate::progress::XpSource::Ritual,
    );
}

// ========== Learning ==========

/// How a spell is being learned. Later stages hang their rules off this
/// (foreign-faith warnings for `Scroll`/`Temple`, canon checks for
/// `Temple`); for now every path learns the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnVia {
    Scroll,
    Temple,
    Evolution,
    Admin,
}

impl LearnVia {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "scroll" => Some(Self::Scroll),
            "temple" => Some(Self::Temple),
            "evolution" => Some(Self::Evolution),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnResult {
    Learned,
    AlreadyKnown,
    /// No such spell in `World.spell_definitions`.
    UnknownSpell,
    /// The character could not be found or saved.
    NoCharacter,
    /// A temple teaches only spells its god's canon holds.
    NotInCanon,
    /// Another god's spell: warned, and a confirmation is open. Learning
    /// again with `confirmed` inside the window goes through.
    NeedsConfirm,
}

impl LearnResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Learned => "learned",
            Self::AlreadyKnown => "already_known",
            Self::UnknownSpell => "unknown_spell",
            Self::NoCharacter => "no_character",
            Self::NotInCanon => "not_in_canon",
            Self::NeedsConfirm => "needs_confirm",
        }
    }
}

/// The one function every learn path goes through. Session first; the
/// spell must exist in the World cache.
pub fn learn(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    spell_id: &str,
    via: LearnVia,
) -> LearnResult {
    learn_with(
        db,
        connections,
        state,
        char_name,
        spell_id,
        via,
        false,
        ritual::now(),
        &mut ritual::ThreadDice,
    )
}

/// [`learn`] with the heresy confirmation spelled out: a foreign god's
/// spell (scroll or temple) returns `NeedsConfirm` and opens a
/// confirmation unless `confirmed` arrives while one is open. Learning it
/// fires the learning offence once.
#[allow(clippy::too_many_arguments)]
pub fn learn_with(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    spell_id: &str,
    via: LearnVia,
    confirmed: bool,
    now: i64,
    dice: &mut dyn ritual::Dice,
) -> LearnResult {
    // World first, then drop it: `apply_to_character` takes Connections.
    let Some(is_codex) = state.lock().spell_definitions.get(spell_id).map(|d| d.codex) else {
        return LearnResult::UnknownSpell;
    };
    let foreign = is_codex
        && matches!(via, LearnVia::Scroll | LearnVia::Temple)
        && heresy::is_foreign(db, connections, char_name, spell_id);
    if foreign {
        let key = format!("learn:{spell_id}");
        let already = apply_to_character(db, connections, char_name, |_| false)
            .is_some_and(|c| c.learned_spells.iter().any(|s| s == spell_id));
        if already {
            return LearnResult::AlreadyKnown;
        }
        if !(confirmed && heresy::take_confirm(connections, char_name, &key, now)) {
            heresy::open_confirm(connections, char_name, &key, now);
            return LearnResult::NeedsConfirm;
        }
    }
    if via == LearnVia::Temple && is_codex {
        let in_canon = db
            .get_codex(spell_id)
            .ok()
            .flatten()
            .is_some_and(|e| e.meta.status == crate::types::CanonStatus::Canon && !e.meta.vetoed);
        if !in_canon {
            return LearnResult::NotInCanon;
        }
    }

    let result = add_learned_spell(db, connections, char_name, spell_id);
    if result == LearnResult::Learned && foreign {
        heresy::on_foreign_learn(db, connections, char_name, spell_id, dice);
    }
    if result == LearnResult::Learned && is_codex {
        crate::progress::award_xp(
            db,
            connections,
            state,
            char_name,
            SPELLCRAFT_SKILL,
            XP_PER_CODEX_LEARN,
            crate::progress::XpSource::Ritual,
        );
    }
    result
}

/// The raw write behind [`learn`]: add `spell_id` to the character's
/// learned spells, session first, with no rule checks. Only `learn` and the
/// legacy `learn_spell` Rhai binding call it.
pub fn add_learned_spell(db: &Db, connections: &SharedConnections, char_name: &str, spell_id: &str) -> LearnResult {
    let mut already = false;
    let saved = apply_to_character(db, connections, char_name, |c| {
        if c.learned_spells.iter().any(|s| s == spell_id) {
            already = true;
            return false;
        }
        c.learned_spells.push(spell_id.to_string());
        true
    });
    match (saved, already) {
        (None, _) => LearnResult::NoCharacter,
        (Some(_), true) => LearnResult::AlreadyKnown,
        (Some(_), false) => LearnResult::Learned,
    }
}

#[cfg(test)]
mod tests;
