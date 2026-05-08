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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraitCategory {
    Positive,
    Negative,
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
}
