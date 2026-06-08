//! Character creation and gameplay definition types loaded from
//! `scripts/data/*.json`: classes, traits, races (mechanical), languages,
//! and spells.

use super::serde_defaults::default_true;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub starting_skills: HashMap<String, i32>,
    #[serde(default)]
    pub stat_bonuses: HashMap<String, i32>,
    #[serde(default = "default_true")]
    pub available: bool,
    #[serde(default)]
    pub starting_languages: HashMap<String, i32>,
    #[serde(default)]
    pub starting_items: Vec<String>,
    #[serde(default)]
    pub starting_gold: i32,
    /// If non-empty, only these race ids may pick this class. Use to whitelist
    /// thematically narrow classes (e.g. lock a class to a single race).
    #[serde(default)]
    pub allowed_races: Vec<String>,
    /// Race ids that may NOT pick this class. Checked after `allowed_races`.
    /// Used by `vampire` in the modern theme to block synthetic races
    /// (synth, bioroid, clone) that can't be embraced.
    #[serde(default)]
    pub incompatible_races: Vec<String>,
}

impl ClassDefinition {
    /// True if this class is selectable by the given race id. Empty race id
    /// (character creation pre-race-pick) is treated as compatible so the
    /// list still renders. Comparison is case-insensitive.
    pub fn allowed_for_race(&self, race_id: &str) -> bool {
        if race_id.is_empty() {
            return true;
        }
        let race = race_id.to_lowercase();
        if !self.allowed_races.is_empty() && !self.allowed_races.iter().any(|r| r.to_lowercase() == race) {
            return false;
        }
        if self.incompatible_races.iter().any(|r| r.to_lowercase() == race) {
            return false;
        }
        true
    }
}

/// Builder-authored override for a class's starting kit. Persisted in the
/// `class_loadouts` sled tree (key = lowercase class id) and overlaid onto
/// `ClassDefinition` after JSON load. JSON files remain canonical for skills,
/// stat bonuses, and languages; only the kit fields are editable at runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassLoadout {
    pub class_id: String,
    #[serde(default)]
    pub starting_items: Vec<String>,
    #[serde(default)]
    pub starting_gold: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraitCategory {
    Positive,
    Negative,
    /// Neither positive nor negative — used for granted-only traits (clan,
    /// race-derived, etc.) that aren't selectable at character creation.
    /// Always paired with `available: false` and cost 0 in practice.
    Neutral,
}

impl Default for TraitCategory {
    fn default() -> Self {
        TraitCategory::Positive
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub cost: i32,
    #[serde(default)]
    pub category: TraitCategory,
    #[serde(default)]
    pub effects: HashMap<String, i32>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default = "default_true")]
    pub available: bool,
}

// Skill system
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillProgress {
    pub level: i32,      // 0-10
    pub experience: i32, // XP toward next level
}

// Per-spell mastery: every learned spell tracks its own level + XP
// independently of the unified `magic` skill. Same 0-10 cap and XP curve as
// SkillProgress. Higher levels boost damage/heal/buff scaling and can trigger
// evolution into a stronger spell ID (see SpellEvolution).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpellProgress {
    pub level: i32,
    pub experience: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellEvolution {
    pub level_required: i32,
    pub spell_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceSuggestion {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

// Race definition system (mechanical races with stat modifiers, resistances, abilities)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RacialPassive {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub effects: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RacialActive {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub script_name: String,
    #[serde(default)]
    pub cooldown_secs: i32,
    #[serde(default)]
    pub mana_cost: i32,
    #[serde(default)]
    pub stamina_cost: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub stat_modifiers: HashMap<String, i32>,
    #[serde(default)]
    pub granted_traits: Vec<String>,
    #[serde(default)]
    pub resistances: HashMap<String, i32>,
    #[serde(default)]
    pub passive_abilities: Vec<RacialPassive>,
    #[serde(default)]
    pub active_abilities: Vec<RacialActive>,
    #[serde(default = "default_true")]
    pub available: bool,
    #[serde(default)]
    pub starting_languages: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDefinition {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_lingua_franca: bool,
    #[serde(default)]
    pub phonetic_words: Vec<String>,
}

fn default_spell_xp() -> i32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub skill_required: i32,
    #[serde(default)]
    pub scroll_only: bool,
    #[serde(default)]
    pub mana_cost: i32,
    #[serde(default)]
    pub cooldown_secs: i32,
    #[serde(default)]
    pub spell_type: String, // "damage", "buff", "heal", "utility"
    #[serde(default)]
    pub damage_base: i32,
    #[serde(default)]
    pub damage_per_skill: i32,
    #[serde(default)]
    pub damage_int_scaling: i32,
    #[serde(default)]
    pub damage_type: String, // "arcane", "fire", "lightning"
    #[serde(default)]
    pub buff_effect: String, // EffectType string
    #[serde(default)]
    pub buff_magnitude: i32,
    #[serde(default)]
    pub buff_duration_secs: i32,
    #[serde(default)]
    pub heal_base: i32,
    #[serde(default)]
    pub heal_per_skill: i32,
    #[serde(default)]
    pub heal_int_scaling: i32,
    #[serde(default)]
    pub target_type: String, // "enemy", "self", "self_or_friendly", "room"
    #[serde(default)]
    pub requires_combat: bool,
    #[serde(default)]
    pub reagent_vnum: Option<String>,
    #[serde(default = "default_spell_xp")]
    pub xp_award: i32,
    /// Skill key this spell gates on. None / missing = "magic" (the default
    /// for fantasy spells). Vampire disciplines set this to "dominate",
    /// "auspex", "celerity", "potence", "obfuscate", … so each discipline
    /// scales independently of the magic skill.
    #[serde(default)]
    pub requires_skill: Option<String>,
    /// When true, only characters with a `vampire_state` may cast this
    /// spell. Used by every entry in `spells_vampire.json`.
    #[serde(default)]
    pub requires_vampire: bool,
    /// When non-empty, the caster must have one of these `clan_*` traits.
    /// Empty = any clan / any kindred can cast.
    #[serde(default)]
    pub requires_clan: Vec<String>,
    /// Per-spell-level scaling added to the magic-skill scaling. The cast
    /// formulas in cast.rhai add `per_spell_level * spell_level` to their
    /// damage/heal/magnitude/duration outputs. Default 0 = no per-spell
    /// scaling (current behavior).
    #[serde(default)]
    pub damage_per_spell_level: i32,
    #[serde(default)]
    pub heal_per_spell_level: i32,
    #[serde(default)]
    pub buff_magnitude_per_spell_level: i32,
    #[serde(default)]
    pub buff_duration_per_spell_level: i32,
    /// When set, hitting `level_required` swaps this spell's ID in the
    /// caster's `learned_spells` for `spell_id` (fresh SpellProgress at
    /// level 1).
    #[serde(default)]
    pub evolves_to: Option<SpellEvolution>,
}
