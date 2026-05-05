//! Core data types for IronMUD
//!
//! This module contains all game data structures organized by domain.

// Domain-specific submodules
mod combat;
pub mod garden;
mod time;
mod trigger;

// Re-export all types from submodules
pub use combat::*;
pub use garden::*;
pub use time::*;
pub use trigger::*;

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Deserialize categories from either a single string (legacy), an array of strings, or null.
fn deserialize_categories<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de;

    struct CategoriesVisitor;

    impl<'de> de::Visitor<'de> for CategoriesVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, a string, or an array of strings")
        }

        fn visit_unit<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_none<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_str<E>(self, v: &str) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![v.to_string()])
            }
        }

        fn visit_string<E>(self, v: String) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            if v.is_empty() { Ok(Vec::new()) } else { Ok(vec![v]) }
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut categories = Vec::new();
            while let Some(val) = seq.next_element::<String>()? {
                categories.push(val);
            }
            Ok(categories)
        }
    }

    deserializer.deserialize_any(CategoriesVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    pub name: String,
    pub password_hash: String,
    pub current_room_id: Uuid,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(default)]
    pub is_builder: bool,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub god_mode: bool,
    #[serde(default)]
    pub build_mode: bool,
    #[serde(default = "default_level")]
    pub level: i32,
    #[serde(default)]
    pub gold: i32,
    #[serde(default)]
    pub bank_gold: i64, // Gold stored in bank account (accessible from any bank/ATM)
    // Character creation wizard fields
    #[serde(default)]
    pub race: String,
    #[serde(default)]
    pub short_description: String,
    #[serde(default = "default_class")]
    pub class_name: String,
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default = "default_trait_points")]
    pub trait_points: i32,
    #[serde(default)]
    pub creation_complete: bool,
    // Thirst system fields
    #[serde(default = "default_max_thirst")]
    pub thirst: i32,
    #[serde(default = "default_max_thirst")]
    pub max_thirst: i32,
    #[serde(default)]
    pub last_thirst_tick: i64,
    // Hunger system fields
    #[serde(default = "default_max_hunger")]
    pub hunger: i32,
    #[serde(default = "default_max_hunger")]
    pub max_hunger: i32,
    #[serde(default)]
    pub last_hunger_tick: i64,
    // HP system fields
    #[serde(default = "default_max_hp")]
    pub hp: i32,
    #[serde(default = "default_max_hp")]
    pub max_hp: i32,
    // Prompt settings
    #[serde(default)]
    pub prompt_mode: String, // "simple" (default) or "verbose"
    // Password management
    #[serde(default)]
    pub must_change_password: bool,
    // Builder mode: show room flags/vnum in room display (persisted)
    #[serde(default)]
    pub show_room_flags: bool,
    // Builder debug channel subscription
    #[serde(default)]
    pub builder_debug_enabled: bool,
    // Stamina system fields
    #[serde(default = "default_max_stamina")]
    pub stamina: i32,
    #[serde(default = "default_max_stamina")]
    pub max_stamina: i32,
    #[serde(default)]
    pub position: CharacterPosition,
    // Skill system
    #[serde(default)]
    pub skills: HashMap<String, SkillProgress>,
    // Learned recipes (recipe IDs learned from books/trainers, not auto-learned)
    #[serde(default)]
    pub learned_recipes: std::collections::HashSet<String>,
    // Foraging cooldown tracking (room_id -> timestamp of last forage)
    #[serde(default)]
    pub foraged_rooms: HashMap<String, i64>,
    // Group/Party system fields
    #[serde(default)]
    pub following: Option<String>, // Name of character being followed
    #[serde(default)]
    pub following_mobile_id: Option<Uuid>, // Mobile instance being followed (mutually exclusive with `following`)
    #[serde(default)]
    pub is_grouped: bool, // Whether actively grouped (not just following)
    // Character stats (affect carrying capacity, health, stamina, etc.)
    #[serde(default = "default_stat")]
    pub stat_str: i32, // Strength - affects carrying capacity
    #[serde(default = "default_stat")]
    pub stat_dex: i32, // Dexterity
    #[serde(default = "default_stat")]
    pub stat_con: i32, // Constitution - affects max health/stamina
    #[serde(default = "default_stat")]
    pub stat_int: i32, // Intelligence
    #[serde(default = "default_stat")]
    pub stat_wis: i32, // Wisdom
    #[serde(default = "default_stat")]
    pub stat_cha: i32, // Charisma
    // Combat system fields
    #[serde(default)]
    pub spawn_room_id: Option<Uuid>, // Respawn location on death
    #[serde(default)]
    pub combat: CombatState,
    #[serde(default)]
    pub wounds: Vec<Wound>,
    #[serde(default)]
    pub ongoing_effects: Vec<OngoingEffect>,
    #[serde(default)]
    pub scars: HashMap<String, i32>, // body_part display name -> scar count
    // Death/unconscious state (persisted, cleared on login)
    #[serde(default)]
    pub is_unconscious: bool,
    #[serde(default)]
    pub bleedout_rounds_remaining: i32,
    // Weather exposure status (transient, cleared on login)
    #[serde(skip)]
    pub is_wet: bool,
    #[serde(skip)]
    pub wet_level: i32, // 0-100, higher = more soaked
    #[serde(skip)]
    pub cold_exposure: i32, // 0-100, escalates to hypothermia at thresholds
    #[serde(skip)]
    pub heat_exposure: i32, // 0-100, escalates to heat exhaustion
    // Environmental conditions (persisted)
    #[serde(default)]
    pub illness_progress: i32, // 0-100, illness severity
    #[serde(default)]
    pub has_hypothermia: bool,
    #[serde(default)]
    pub has_frostbite: Vec<BodyPart>, // Body parts affected
    #[serde(default)]
    pub has_heat_exhaustion: bool,
    #[serde(default)]
    pub has_heat_stroke: bool,
    #[serde(default)]
    pub has_illness: bool,
    #[serde(default)]
    pub food_sick: bool,
    // Helpline channel subscription
    #[serde(default)]
    pub helpline_enabled: bool,
    // Consent flag: when true, this character can be summoned by the
    // `summon` spell (CircleMUD PRF_SUMMONABLE parity). Default off.
    #[serde(default)]
    pub summonable: bool,
    // Property rental system
    #[serde(default)]
    pub active_leases: HashMap<Uuid, Uuid>, // area_id -> lease_id (one per area)
    #[serde(default)]
    pub escrow_ids: Vec<Uuid>, // Escrow IDs for evicted items
    #[serde(default)]
    pub tour_origin_room: Option<Uuid>, // Return location after tour
    #[serde(default)]
    pub on_tour: bool, // Currently touring a template
    // Buff system fields
    #[serde(default)]
    pub active_buffs: Vec<ActiveBuff>,
    #[serde(default)]
    pub mana: i32,
    #[serde(default)]
    pub max_mana: i32,
    #[serde(default)]
    pub mana_enabled: bool, // Admin flag - hidden from stats/prompt unless true
    #[serde(default)]
    pub drunk_level: i32, // 0-100, decreases over time
    // Racial ability cooldowns (ability_id -> unix timestamp when usable)
    #[serde(default)]
    pub racial_cooldowns: HashMap<String, i64>,
    // Spell system fields
    #[serde(default)]
    pub learned_spells: Vec<String>,
    #[serde(default)]
    pub spell_cooldowns: HashMap<String, i64>,
    // Breath/drowning system
    #[serde(default = "default_max_breath")]
    pub breath: i32,
    #[serde(default = "default_max_breath")]
    pub max_breath: i32,
    // Stealth system fields (transient - cleared on login)
    #[serde(skip)]
    pub is_hidden: bool,
    #[serde(skip)]
    pub is_sneaking: bool,
    #[serde(skip)]
    pub is_camouflaged: bool,
    #[serde(skip)]
    pub hunting_target: String,
    #[serde(skip)]
    pub envenomed_charges: i32,
    #[serde(skip)]
    pub circle_cooldown: i64,
    // Stealth system fields (persisted)
    #[serde(default)]
    pub theft_cooldowns: HashMap<String, i64>,
}

fn default_level() -> i32 {
    1
}

fn default_stat() -> i32 {
    10 // Average stat value
}

fn default_max_thirst() -> i32 {
    100
}

fn default_max_hunger() -> i32 {
    100
}

fn default_max_hp() -> i32 {
    100
}

fn default_max_stamina() -> i32 {
    100
}

fn default_max_breath() -> i32 {
    100
}

fn default_class() -> String {
    "unemployed".to_string()
}

fn default_trait_points() -> i32 {
    10
}

// Character creation data structures (loaded from scripts/data/*.json)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CharacterPosition {
    #[default]
    Standing,
    Sitting,
    Sleeping,
    Swimming,
}

impl std::fmt::Display for CharacterPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterPosition::Standing => write!(f, "standing"),
            CharacterPosition::Sitting => write!(f, "sitting"),
            CharacterPosition::Sleeping => write!(f, "sleeping"),
            CharacterPosition::Swimming => write!(f, "swimming"),
        }
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

// Recipe/Crafting system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub skill: String, // "crafting" or "cooking"
    #[serde(default)]
    pub skill_required: i32, // Minimum skill level (0-10)
    #[serde(default)]
    pub auto_learn: bool, // Learned automatically at skill level?
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,
    #[serde(default)]
    pub tools: Vec<RecipeTool>,
    pub output_vnum: String, // Prototype vnum to spawn
    #[serde(default = "default_one")]
    pub output_quantity: i32, // How many to produce
    #[serde(default)]
    pub base_xp: i32, // XP awarded on success
    #[serde(default = "default_one")]
    pub difficulty: i32, // Affects quality roll (1-10)
}

fn default_one() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeIngredient {
    #[serde(default)]
    pub vnum: Option<String>, // Exact vnum (if specified)
    #[serde(default)]
    pub category: Option<String>, // Category match (e.g., "flour", "meat")
    #[serde(default = "default_one")]
    pub quantity: i32, // How many needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeTool {
    #[serde(default)]
    pub vnum: Option<String>, // Exact tool vnum (if specified)
    #[serde(default)]
    pub category: Option<String>, // Tool category (e.g., "knife", "forge")
    #[serde(default)]
    pub location: ToolLocation, // Where to find it
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ToolLocation {
    #[default]
    Inventory, // Must be in player's inventory
    Room,   // Must be in current room
    Either, // Can be in inventory OR room
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

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomData {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub exits: RoomExits,
    #[serde(default)]
    pub flags: RoomFlags,
    #[serde(default)]
    pub extra_descs: Vec<ExtraDesc>,
    #[serde(default)]
    pub vnum: Option<String>,
    #[serde(default)]
    pub area_id: Option<Uuid>,
    #[serde(default)]
    pub triggers: Vec<RoomTrigger>,
    #[serde(default)]
    pub doors: std::collections::HashMap<String, DoorState>,
    // Seasonal descriptions - auto-display based on current game season
    #[serde(default)]
    pub spring_desc: Option<String>,
    #[serde(default)]
    pub summer_desc: Option<String>,
    #[serde(default)]
    pub autumn_desc: Option<String>,
    #[serde(default)]
    pub winter_desc: Option<String>,
    // Dynamic description - set by triggers for weather, events, etc.
    #[serde(default)]
    pub dynamic_desc: Option<String>,
    // Fishing system
    #[serde(default)]
    pub water_type: WaterType,
    #[serde(default)]
    pub catch_table: Vec<CatchEntry>,
    // Property template fields
    #[serde(default)]
    pub is_property_template: bool, // This is a template room
    #[serde(default)]
    pub property_template_id: Option<Uuid>, // Which template this belongs to
    #[serde(default)]
    pub is_template_entrance: bool, // Entry point for template
    // Property instance fields
    #[serde(default)]
    pub property_lease_id: Option<Uuid>, // Which lease owns this instance
    #[serde(default)]
    pub property_entrance: bool, // Entry point for rental
    // Stealth/tracking system
    #[serde(default)]
    pub recent_departures: Vec<DepartureRecord>,
    #[serde(default)]
    pub blood_trails: Vec<BloodTrail>,
    #[serde(default)]
    pub traps: Vec<RoomTrap>,
    // Migrant housing
    /// Maximum number of mobiles that can claim this room as their home.
    /// Only meaningful when the `liveable` flag is set.
    #[serde(default)]
    pub living_capacity: i32,
    /// UUIDs of mobiles currently claiming this room as their home.
    /// Populated by the migration system; drained on mobile death.
    #[serde(default)]
    pub residents: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomExits {
    pub north: Option<Uuid>,
    pub east: Option<Uuid>,
    pub south: Option<Uuid>,
    pub west: Option<Uuid>,
    pub up: Option<Uuid>,
    pub down: Option<Uuid>,
    #[serde(default)]
    pub out: Option<Uuid>, // Used for transport vehicles, buildings, etc.
    #[serde(default)]
    pub custom: std::collections::HashMap<String, Uuid>, // Custom exits like "elevator", "train", "portal"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoorState {
    pub name: String, // "door", "gate", "hatch", etc.
    pub is_closed: bool,
    pub is_locked: bool,
    #[serde(default)]
    pub key_vnum: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>, // Additional keywords like "wooden", "iron"
    #[serde(default)]
    pub pickproof: bool, // Locks that lockpick skill cannot defeat (Circle EX_PICKPROOF)
}

// === Fishing System - Water Types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaterType {
    #[default]
    None,
    Freshwater,
    Saltwater,
    Magical,
}

impl WaterType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(WaterType::None),
            "freshwater" | "fresh" => Some(WaterType::Freshwater),
            "saltwater" | "salt" | "ocean" => Some(WaterType::Saltwater),
            "magical" | "magic" => Some(WaterType::Magical),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            WaterType::None => "none",
            WaterType::Freshwater => "freshwater",
            WaterType::Saltwater => "saltwater",
            WaterType::Magical => "magical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatchEntry {
    pub vnum: String,   // Item vnum to spawn
    pub weight: i32,    // Relative spawn weight
    pub min_skill: i32, // Minimum skill level (0-10)
    pub rarity: String, // common/uncommon/rare/legendary
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForageEntry {
    pub vnum: String,   // Item vnum to spawn (weight comes from item prototype)
    pub min_skill: i32, // Minimum skill level (0-10)
    pub rarity: String, // common/uncommon/rare/legendary
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomFlags {
    #[serde(default)]
    pub dark: bool,
    /// Room-level combat zone override (None = inherit from area)
    #[serde(default)]
    pub combat_zone: Option<CombatZoneType>,
    #[serde(default)]
    pub no_mob: bool,
    #[serde(default)]
    pub indoors: bool,
    #[serde(default)]
    pub underwater: bool,
    // Climate/weather flags
    #[serde(default)]
    pub climate_controlled: bool, // Room ignores outside weather/temp
    #[serde(default)]
    pub always_hot: bool, // Forge, volcano, etc.
    #[serde(default)]
    pub always_cold: bool, // Ice cave, freezer, etc.
    #[serde(default)]
    pub city: bool, // City rooms stay lit at night
    #[serde(default)]
    pub no_windows: bool, // No day/night messages (caves, deep buildings)
    // Stamina system flag
    #[serde(default)]
    pub difficult_terrain: bool, // Costs 2 stamina to traverse instead of 1
    // Foraging flag
    #[serde(default)]
    pub dirt_floor: bool, // Allows wilderness foraging
    // Property rental flag
    #[serde(default)]
    pub property_storage: bool, // Items dropped here are safe (property rooms)
    // Mail system flag
    #[serde(default)]
    pub post_office: bool, // Players can send mail from this room
    // Banking system flag
    #[serde(default)]
    pub bank: bool, // Players can use banking commands here
    // Gardening system flag
    #[serde(default)]
    pub garden: bool, // Thematic flag for garden display in look output
    // Recall system flag
    #[serde(default)]
    pub spawn_point: bool, // Players can bind their spawn point here (inns, safe rooms)
    // Water system flags
    #[serde(default)]
    pub shallow_water: bool, // Surface water - extra stamina, gets wet
    #[serde(default)]
    pub deep_water: bool, // Deep water - requires boat or swimming 5+
    // Migrant housing flag
    #[serde(default)]
    pub liveable: bool, // Migrant NPCs can claim this room as a home
    // CircleMUD legacy parity flags
    // (`private_room` rather than `private` because `private` is a reserved
    // keyword in Rhai 1.x and can't be accessed via dot syntax in scripts.)
    #[serde(default, alias = "private")]
    pub private_room: bool, // Caps player occupancy at 2 (Circle ROOM_PRIVATE — inn rooms)
    #[serde(default)]
    pub tunnel: bool, // Caps player occupancy at 1 (Circle ROOM_TUNNEL — chokepoints)
    #[serde(default)]
    pub death: bool, // Instant-kill on player entry (Circle ROOM_DEATH — death traps)
    #[serde(default)]
    pub no_magic: bool, // Suppresses spellcasting from this room (Circle ROOM_NOMAGIC)
    #[serde(default)]
    pub soundproof: bool, // Blocks shouts from leaking in/out (Circle ROOM_SOUNDPROOF)
    #[serde(default)]
    pub notrack: bool, // Defeats the track skill (Circle ROOM_NOTRACK)
}

impl RoomFlags {
    /// OR each bool default on top of this RoomFlags. Only turns flags on —
    /// anything the caller already set stays on. Used to apply an area's
    /// `default_room_flags` to a newly-created room. `combat_zone` is left
    /// alone (room-level override; area has its own `combat_zone`).
    pub fn merge_area_defaults(&mut self, defaults: &RoomFlags) {
        self.dark |= defaults.dark;
        self.no_mob |= defaults.no_mob;
        self.indoors |= defaults.indoors;
        self.underwater |= defaults.underwater;
        self.climate_controlled |= defaults.climate_controlled;
        self.always_hot |= defaults.always_hot;
        self.always_cold |= defaults.always_cold;
        self.city |= defaults.city;
        self.no_windows |= defaults.no_windows;
        self.difficult_terrain |= defaults.difficult_terrain;
        self.dirt_floor |= defaults.dirt_floor;
        self.property_storage |= defaults.property_storage;
        self.post_office |= defaults.post_office;
        self.bank |= defaults.bank;
        self.garden |= defaults.garden;
        self.spawn_point |= defaults.spawn_point;
        self.shallow_water |= defaults.shallow_water;
        self.deep_water |= defaults.deep_water;
        self.liveable |= defaults.liveable;
        self.private_room |= defaults.private_room;
        self.tunnel |= defaults.tunnel;
        self.death |= defaults.death;
        self.no_magic |= defaults.no_magic;
        self.soundproof |= defaults.soundproof;
        self.notrack |= defaults.notrack;
    }
}

// === Stealth/Tracking System Structures ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartureRecord {
    pub name: String,
    pub direction: String,
    pub timestamp: i64,
    pub is_sneaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloodTrail {
    pub name: String,
    pub severity: i32,
    pub timestamp: i64,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTrap {
    pub trap_type: String, // "spike", "alarm", "snare", "poison_dart"
    pub owner_name: String,
    pub damage: i32,
    pub detect_difficulty: i32,
    pub disarm_difficulty: i32,
    pub charges: i32,
    pub effect: String, // "damage", "alarm", "slow", "poison"
    pub placed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraDesc {
    pub keywords: Vec<String>,
    pub description: String,
}

// === Transportation System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportType {
    Elevator,
    Bus,
    Train,
    Ferry,
    Airship,
}

impl Default for TransportType {
    fn default() -> Self {
        TransportType::Elevator
    }
}

impl TransportType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "elevator" => Some(TransportType::Elevator),
            "bus" => Some(TransportType::Bus),
            "train" => Some(TransportType::Train),
            "ferry" => Some(TransportType::Ferry),
            "airship" => Some(TransportType::Airship),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            TransportType::Elevator => "elevator",
            TransportType::Bus => "bus",
            TransportType::Train => "train",
            TransportType::Ferry => "ferry",
            TransportType::Airship => "airship",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    #[default]
    Stopped,
    Moving,
}

impl TransportState {
    pub fn to_display_string(&self) -> &'static str {
        match self {
            TransportState::Stopped => "stopped",
            TransportState::Moving => "moving",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportSchedule {
    OnDemand,
    GameTime {
        frequency_hours: i32,
        operating_start: u8,
        operating_end: u8,
        dwell_time_secs: i64,
    },
}

impl Default for TransportSchedule {
    fn default() -> Self {
        TransportSchedule::OnDemand
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportStop {
    pub room_id: Uuid,
    pub name: String,
    pub exit_direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportData {
    pub id: Uuid,
    #[serde(default)]
    pub vnum: Option<String>,
    pub name: String,
    #[serde(default)]
    pub transport_type: TransportType,
    pub interior_room_id: Uuid,
    #[serde(default)]
    pub stops: Vec<TransportStop>,
    #[serde(default)]
    pub current_stop_index: usize,
    #[serde(default)]
    pub state: TransportState,
    #[serde(default = "default_transport_direction")]
    pub direction: i8,
    #[serde(default)]
    pub schedule: TransportSchedule,
    #[serde(default = "default_travel_time")]
    pub travel_time_secs: i64,
    #[serde(default)]
    pub last_state_change: i64,
}

fn default_transport_direction() -> i8 {
    1
}

fn default_travel_time() -> i64 {
    30
}

/// Get opposite direction for bidirectional exits
/// Returns None for non-cardinal directions (fall back to "out")
pub fn get_opposite_direction(direction: &str) -> Option<&'static str> {
    match direction.to_lowercase().as_str() {
        "north" | "n" => Some("south"),
        "south" | "s" => Some("north"),
        "east" | "e" => Some("west"),
        "west" | "w" => Some("east"),
        "up" | "u" => Some("down"),
        "down" | "d" => Some("up"),
        _ => None,
    }
}

impl TransportData {
    pub fn new(name: String, interior_room_id: Uuid) -> Self {
        TransportData {
            id: Uuid::new_v4(),
            vnum: None,
            name,
            transport_type: TransportType::default(),
            interior_room_id,
            stops: Vec::new(),
            current_stop_index: 0,
            state: TransportState::Stopped,
            direction: 1,
            schedule: TransportSchedule::OnDemand,
            travel_time_secs: 30,
            last_state_change: 0,
        }
    }

    /// Check if the transport is within operating hours (for GameTime schedules)
    pub fn is_within_operating_hours(&self, hour: u8) -> bool {
        match &self.schedule {
            TransportSchedule::OnDemand => true,
            TransportSchedule::GameTime {
                operating_start,
                operating_end,
                ..
            } => {
                if operating_start <= operating_end {
                    // Normal range: e.g., 6 AM to 11 PM
                    hour >= *operating_start && hour <= *operating_end
                } else {
                    // Overnight range: e.g., 11 PM to 6 AM
                    hour >= *operating_start || hour <= *operating_end
                }
            }
        }
    }

    /// Get the current stop, if any
    pub fn current_stop(&self) -> Option<&TransportStop> {
        self.stops.get(self.current_stop_index)
    }

    /// Advance to the next stop, handling direction reversal for ping-pong routes
    pub fn advance_to_next_stop(&mut self) {
        if self.stops.is_empty() {
            return;
        }

        let next_index = self.current_stop_index as i64 + self.direction as i64;

        if next_index < 0 {
            // At start, reverse direction
            self.direction = 1;
            self.current_stop_index = 1.min(self.stops.len() - 1);
        } else if next_index >= self.stops.len() as i64 {
            // At end, reverse direction
            self.direction = -1;
            self.current_stop_index = self.stops.len().saturating_sub(2);
        } else {
            self.current_stop_index = next_index as usize;
        }
    }
}

// === NPC Transport Route System ===

/// Schedule for when an NPC travels via transport
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NPCTravelSchedule {
    /// NPC travels at fixed game hours (e.g., commuter going to work)
    FixedHours {
        depart_hour: u8, // Hour to leave home for destination
        return_hour: u8, // Hour to return home
    },
    /// NPC has a random chance to travel each game hour
    Random {
        chance_per_hour: i32, // 1-100 percent chance to travel per hour
    },
    /// NPC stays on the transport permanently (e.g., conductor)
    Permanent,
}

impl Default for NPCTravelSchedule {
    fn default() -> Self {
        NPCTravelSchedule::FixedHours {
            depart_hour: 8,
            return_hour: 17,
        }
    }
}

/// Configuration for an NPC that uses transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportRoute {
    pub transport_id: Uuid,            // Which transport to use
    pub home_stop_index: usize,        // Where NPC "lives" (boards from)
    pub destination_stop_index: usize, // Where NPC travels to
    pub schedule: NPCTravelSchedule,
    #[serde(default)]
    pub is_at_destination: bool, // Track if NPC is currently at destination
    #[serde(default)]
    pub is_on_transport: bool, // Track if NPC is currently riding
}

impl TransportRoute {
    pub fn new(transport_id: Uuid, home_stop_index: usize, destination_stop_index: usize) -> Self {
        TransportRoute {
            transport_id,
            home_stop_index,
            destination_stop_index,
            schedule: NPCTravelSchedule::default(),
            is_at_destination: false,
            is_on_transport: false,
        }
    }
}

// === Mobile Daily Routine System ===

/// Activity state for a mobile NPC during their daily routine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Working,
    Sleeping,
    Patrolling,
    OffDuty,
    Socializing,
    Eating,
    Custom(String),
}

impl Default for ActivityState {
    fn default() -> Self {
        ActivityState::Working
    }
}

impl ActivityState {
    pub fn from_str(s: &str) -> ActivityState {
        match s.to_lowercase().as_str() {
            "working" => ActivityState::Working,
            "sleeping" => ActivityState::Sleeping,
            "patrolling" => ActivityState::Patrolling,
            "off_duty" | "offduty" => ActivityState::OffDuty,
            "socializing" => ActivityState::Socializing,
            "eating" => ActivityState::Eating,
            other => ActivityState::Custom(other.to_string()),
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            ActivityState::Working => "working".to_string(),
            ActivityState::Sleeping => "sleeping".to_string(),
            ActivityState::Patrolling => "patrolling".to_string(),
            ActivityState::OffDuty => "off_duty".to_string(),
            ActivityState::Socializing => "socializing".to_string(),
            ActivityState::Eating => "eating".to_string(),
            ActivityState::Custom(s) => s.clone(),
        }
    }
}

/// A single entry in a mobile's daily routine schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineEntry {
    /// Game hour (0-23) when this entry becomes active
    pub start_hour: u8,
    /// Activity state for this period
    pub activity: ActivityState,
    /// Optional destination vnum - mobile will walk there when this entry activates
    #[serde(default)]
    pub destination_vnum: Option<String>,
    /// Message broadcast to room when transitioning to this entry
    #[serde(default)]
    pub transition_message: Option<String>,
    /// Whether to suppress random wandering during this entry
    #[serde(default)]
    pub suppress_wander: bool,
    /// Dialogue overrides active during this entry (keyword -> response)
    #[serde(default)]
    pub dialogue_overrides: HashMap<String, String>,
}

/// Find the active routine entry for a given game hour.
/// Entries are matched by finding the one with the highest start_hour <= current hour,
/// wrapping around midnight if needed.
pub fn find_active_entry(entries: &[RoutineEntry], hour: u8) -> Option<&RoutineEntry> {
    if entries.is_empty() {
        return None;
    }

    // Find the entry with the highest start_hour that is <= the current hour
    let mut best: Option<&RoutineEntry> = None;
    let mut best_wrap: Option<&RoutineEntry> = None;

    for entry in entries {
        if entry.start_hour <= hour {
            match best {
                None => best = Some(entry),
                Some(b) if entry.start_hour > b.start_hour => best = Some(entry),
                _ => {}
            }
        }
        // Track the highest start_hour overall (for midnight wrap)
        match best_wrap {
            None => best_wrap = Some(entry),
            Some(b) if entry.start_hour > b.start_hour => best_wrap = Some(entry),
            _ => {}
        }
    }

    // If we found an entry at or before current hour, use it
    // Otherwise wrap around to the latest entry (it started "yesterday" and is still active)
    best.or(best_wrap)
}

// === NPC Needs Simulation System ===

/// What goal the simulated NPC is currently pursuing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SimGoal {
    /// At home, needs satisfied, no urgent goal
    Idle,
    /// Need to eat - heading to shop or eating from inventory
    SeekFood,
    /// Need to sleep - heading home
    SeekSleep,
    /// Need comfort - heading home
    SeekComfort,
    /// Broke + jobless - heading to a bank room in the area for a handout
    SeekBank,
    /// At work, working
    Working,
    /// In transit to workplace
    GoingToWork,
    /// In transit to home (off duty or no urgent need)
    GoingHome,
}

impl Default for SimGoal {
    fn default() -> Self {
        SimGoal::Idle
    }
}

impl SimGoal {
    pub fn to_display_string(&self) -> String {
        match self {
            SimGoal::Idle => "idle".to_string(),
            SimGoal::SeekFood => "seek_food".to_string(),
            SimGoal::SeekSleep => "seek_sleep".to_string(),
            SimGoal::SeekComfort => "seek_comfort".to_string(),
            SimGoal::SeekBank => "seek_bank".to_string(),
            SimGoal::Working => "working".to_string(),
            SimGoal::GoingToWork => "going_to_work".to_string(),
            SimGoal::GoingHome => "going_home".to_string(),
        }
    }

    pub fn from_str(s: &str) -> SimGoal {
        match s.to_lowercase().as_str() {
            "idle" => SimGoal::Idle,
            "seek_food" => SimGoal::SeekFood,
            "seek_sleep" => SimGoal::SeekSleep,
            "seek_comfort" => SimGoal::SeekComfort,
            "seek_bank" => SimGoal::SeekBank,
            "working" => SimGoal::Working,
            "going_to_work" => SimGoal::GoingToWork,
            "going_home" => SimGoal::GoingHome,
            _ => SimGoal::Idle,
        }
    }
}

/// Runtime needs state for a simulated NPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedsState {
    /// 0-100, 100 = fully fed
    pub hunger: i32,
    /// 0-100, 100 = fully rested
    pub energy: i32,
    /// 0-100, 100 = comfortable
    pub comfort: i32,
    /// Current goal the NPC is pursuing
    #[serde(default)]
    pub current_goal: SimGoal,
    /// Whether the NPC has been paid for the current work shift
    #[serde(default)]
    pub paid_this_shift: bool,
    /// Track game hour of last tick to detect shift boundaries
    #[serde(default)]
    pub last_tick_hour: u8,
    /// Unix timestamp of last ambient emote (for cooldown)
    #[serde(default)]
    pub last_emote_tick: i64,
    /// Shop room ids already visited this hunger cycle without finding affordable food.
    /// Cleared whenever the NPC successfully eats.
    #[serde(default)]
    pub tried_shops_this_cycle: Vec<Uuid>,
    /// Unix timestamp of the last time the NPC tried home-relief (cohabitant
    /// charity or forage scraps). Throttles fallback attempts so a hungry,
    /// broke, friendless mobile doesn't burn the helper every tick.
    #[serde(default)]
    pub last_relief_attempt: i64,
    /// Unix timestamp of the last successful visit to a bank room for the
    /// broke-jobless handout. Throttles repeat visits.
    #[serde(default)]
    pub last_bank_visit_attempt: i64,
}

impl Default for NeedsState {
    fn default() -> Self {
        NeedsState {
            hunger: 100,
            energy: 100,
            comfort: 100,
            current_goal: SimGoal::Idle,
            paid_this_shift: false,
            last_tick_hour: 0,
            last_emote_tick: 0,
            tried_shops_this_cycle: Vec::new(),
            last_relief_attempt: 0,
            last_bank_visit_attempt: 0,
        }
    }
}

/// Builder-configurable simulation settings for an NPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Room vnum where this NPC lives (sleeps, recovers comfort)
    pub home_room_vnum: String,
    /// Room vnum where this NPC works
    pub work_room_vnum: String,
    /// Room vnum of preferred shop for food purchases
    #[serde(default)]
    pub shop_room_vnum: String,
    /// Preferred food item vnum to purchase
    #[serde(default)]
    pub preferred_food_vnum: String,
    /// Gold earned per completed work shift
    #[serde(default = "default_work_pay")]
    pub work_pay: i32,
    /// Game hour when work starts (0-23)
    #[serde(default = "default_work_start")]
    pub work_start_hour: u8,
    /// Game hour when work ends (0-23)
    #[serde(default = "default_work_end")]
    pub work_end_hour: u8,
    /// Hunger decay rate multiplier (100 = normal, 200 = 2x, 0 = use default)
    #[serde(default)]
    pub hunger_decay_rate: i32,
    /// Energy decay rate multiplier
    #[serde(default)]
    pub energy_decay_rate: i32,
    /// Comfort decay rate multiplier
    #[serde(default)]
    pub comfort_decay_rate: i32,
    /// Gold balance below which the NPC treats earning money as a need
    #[serde(default = "default_low_gold_threshold")]
    pub low_gold_threshold: i32,
}

fn default_work_pay() -> i32 {
    50
}
fn default_work_start() -> u8 {
    8
}
fn default_work_end() -> u8 {
    17
}
fn default_low_gold_threshold() -> i32 {
    10
}

// === Migrant Characteristics & Relationships ===

/// Visual/physical characteristics for a generated migrant (or any mobile).
/// Currently visual-only; traits, skills, and personality seeds will be added later.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Characteristics {
    #[serde(default)]
    pub gender: String, // "male" | "female"
    #[serde(default)]
    pub age: i32,
    #[serde(default)]
    pub age_label: String, // "young adult", "middle-aged", etc.
    /// Absolute game day the mobile was born. Source of truth for age; `age` and
    /// `age_label` are caches refreshed by the aging tick. Zero means unknown
    /// (back-compat for pre-aging saves — aging tick back-computes from `age`).
    #[serde(default)]
    pub birth_day: i64,
    #[serde(default)]
    pub height: String,
    #[serde(default)]
    pub build: String,
    #[serde(default)]
    pub hair_color: String,
    #[serde(default)]
    pub hair_style: String,
    #[serde(default)]
    pub eye_color: String,
    #[serde(default)]
    pub skin_tone: String,
    #[serde(default)]
    pub distinguishing_mark: Option<String>,
}

/// Kind of social relationship between two mobiles. Stored on MobileData.relationships.
/// Partner/Parent/Child/Sibling aren't used by any tick yet, but the data lives on the
/// mobile so builders can wire up families today and future systems can read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Partner,
    Parent,
    Child,
    Sibling,
    Friend,
    /// Two adult mobiles who have moved into the same liveable room together.
    Cohabitant,
}

impl RelationshipKind {
    pub fn from_str(s: &str) -> Option<RelationshipKind> {
        match s.to_lowercase().as_str() {
            "partner" | "spouse" => Some(RelationshipKind::Partner),
            "parent" => Some(RelationshipKind::Parent),
            "child" => Some(RelationshipKind::Child),
            "sibling" => Some(RelationshipKind::Sibling),
            "friend" => Some(RelationshipKind::Friend),
            "cohabitant" | "cohab" => Some(RelationshipKind::Cohabitant),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            RelationshipKind::Partner => "partner",
            RelationshipKind::Parent => "parent",
            RelationshipKind::Child => "child",
            RelationshipKind::Sibling => "sibling",
            RelationshipKind::Friend => "friend",
            RelationshipKind::Cohabitant => "cohabitant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub other_id: Uuid,
    pub kind: RelationshipKind,
    /// How the holder feels about `other_id`. Range: -100..=100. 0 is neutral.
    /// Positive affinity is grown via matched topics in conversation and triggers
    /// cohabitation at high thresholds; strongly negative affinity triggers breakup.
    #[serde(default)]
    pub affinity: i32,
    /// Game day of the most recent interaction, used for slow drift toward neutral.
    #[serde(default)]
    pub last_interaction_day: i32,
    /// Topics recently covered with this partner (most-recent first). Conversation
    /// logic halves the affinity/happiness delta when the chosen topic appears
    /// here, so repeating the same subject yields diminishing returns. Capped at
    /// `TOPIC_FATIGUE_WINDOW` entries.
    #[serde(default)]
    pub recent_topics: Vec<String>,
}

/// Maximum number of topics retained per `Relationship::recent_topics`. Once a
/// topic rolls off the window, it counts as "fresh" again.
pub const TOPIC_FATIGUE_WINDOW: usize = 5;

/// Derived emotional state bucket computed from SocialState::happiness.
/// Stored so buff/emote hooks can observe transitions without recomputing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MoodState {
    Content,
    #[default]
    Normal,
    Sad,
    Depressed,
    Breakdown,
}

impl MoodState {
    pub fn to_display_string(&self) -> &'static str {
        match self {
            MoodState::Content => "content",
            MoodState::Normal => "normal",
            MoodState::Sad => "sad",
            MoodState::Depressed => "depressed",
            MoodState::Breakdown => "breakdown",
        }
    }
}

/// Derived life stage bucket computed from `Characteristics.age`. Stage
/// boundaries are the single source of truth for age-gated behaviour (migrant
/// exclusion of juveniles, pregnancy eligibility, old-age death rolls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LifeStage {
    Baby,
    Child,
    Adolescent,
    #[default]
    YoungAdult,
    Adult,
    MiddleAged,
    Elderly,
}

impl LifeStage {
    pub fn to_display_string(&self) -> &'static str {
        age_label_for_stage(*self)
    }
}

/// Number of game days in a game year. Keep in sync with
/// `GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR` in `src/types/time.rs`.
pub const GAME_DAYS_PER_YEAR: i64 = 360;

/// Map a numeric age (years) to its [`LifeStage`]. Single source of truth —
/// consulted by the aging tick, migration filters, and examine cues.
pub fn life_stage_for_age(age: i32) -> LifeStage {
    match age {
        i32::MIN..=2 => LifeStage::Baby,
        3..=12 => LifeStage::Child,
        13..=17 => LifeStage::Adolescent,
        18..=29 => LifeStage::YoungAdult,
        30..=49 => LifeStage::Adult,
        50..=64 => LifeStage::MiddleAged,
        _ => LifeStage::Elderly,
    }
}

/// Human-readable label for a life stage. These strings also appear in the
/// `age_ranges` entries in `scripts/data/visuals/*.json`, so keep them aligned.
pub fn age_label_for_stage(stage: LifeStage) -> &'static str {
    match stage {
        LifeStage::Baby => "baby",
        LifeStage::Child => "child",
        LifeStage::Adolescent => "adolescent",
        LifeStage::YoungAdult => "young adult",
        LifeStage::Adult => "adult",
        LifeStage::MiddleAged => "middle-aged",
        LifeStage::Elderly => "elderly",
    }
}

/// A record that this mobile is mourning a specific dead relation. Populated
/// by `db::delete_mobile` for every surviving family/cohabitant partner whose
/// affinity toward the deceased was not deeply negative. Cleared lazily by
/// the simulation tick once `until_day` has passed. Drives richer examine
/// cues than scanning broken Uuid references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BereavementNote {
    pub other_id: Uuid,
    pub other_name: String,
    pub kind: RelationshipKind,
    pub until_day: i32,
}

/// Social preferences + happiness tracking for simulated mobiles.
/// Seeded at migration time; never edited directly by builders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialState {
    #[serde(default)]
    pub likes: Vec<String>,
    #[serde(default)]
    pub dislikes: Vec<String>,
    /// 0..=100, default 50. Drives MoodState and buff application.
    #[serde(default = "default_happiness")]
    pub happiness: i32,
    #[serde(default)]
    pub mood: MoodState,
    /// Unix seconds of the last conversation; acts as a per-mobile cooldown.
    #[serde(default)]
    pub last_converse_secs: u64,
    /// Game day until which this mobile refuses new pair bonds after losing a cohabitant.
    #[serde(default)]
    pub bereaved_until_day: Option<i32>,
    /// Per-relation mourning notes used to surface "mourning their father"
    /// style cues. Written on death by `db::delete_mobile`, pruned by the
    /// simulation tick when `until_day` has passed.
    #[serde(default)]
    pub bereaved_for: Vec<BereavementNote>,
    /// Absolute game day a birth is due. `None` when not pregnant. Only
    /// females in YoungAdult/Adult stage carry this field; the aging tick
    /// checks it on birth day.
    #[serde(default)]
    pub pregnant_until_day: Option<i32>,
    /// Mobile id of the father. Set on conception; read at birth to wire
    /// reciprocal Parent/Child links. Cleared after birth alongside
    /// `pregnant_until_day`.
    #[serde(default)]
    pub pregnant_by: Option<Uuid>,
}

impl Default for SocialState {
    fn default() -> Self {
        SocialState {
            likes: Vec::new(),
            dislikes: Vec::new(),
            happiness: 50,
            mood: MoodState::Normal,
            last_converse_secs: 0,
            bereaved_until_day: None,
            bereaved_for: Vec::new(),
            pregnant_until_day: None,
            pregnant_by: None,
        }
    }
}

fn default_happiness() -> i32 {
    50
}

// === Area Flags System ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AreaFlags {
    /// Area-wide climate control - rooms inherit unless they override
    #[serde(default)]
    pub climate_controlled: bool,
}

// === Area Permission System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AreaPermission {
    /// Only the owner can edit (strictest)
    OwnerOnly,
    /// Owner + trusted builders can edit
    Trusted,
    /// Any builder can edit (default, backwards compatible)
    #[default]
    AllBuilders,
}

/// Inclusive integer range used to roll a starting gold purse for new migrants.
/// Default `{0, 0}` preserves legacy "broke at spawn" behavior so areas without
/// the field set explicitly behave as before.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct GoldRange {
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
}

/// Per-role chances that a spawned migrant arrives as a specialized variation.
/// Each field is a probability in [0.0, 1.0] applied independently in priority
/// order (first match wins); 0.0 means "never". One field per role keeps the
/// builder UI grouped under `aedit immigration variations`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImmigrationVariationChances {
    #[serde(default)]
    pub guard: f32,
    #[serde(default)]
    pub healer: f32,
    #[serde(default)]
    pub scavenger: f32,
}

/// Per-form chances that a migration slot spawns as a pre-linked family group
/// instead of a single adult. Probabilities apply in order; first hit wins.
/// Defaults to 0.0 (migration keeps the existing single-adult shape). The
/// slot is only consumed if the target liveable room has enough free capacity
/// for the whole group.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImmigrationFamilyChance {
    /// Chance the slot spawns as one adult plus one juvenile child sharing a
    /// household. The parent claims the room; the child does not consume a
    /// separate liveable slot (treated as a dependent).
    #[serde(default)]
    pub parent_child: f32,
    /// Chance the slot spawns as two adult siblings moving in together.
    /// Requires 2 free slots in the room (both claim residency).
    #[serde(default)]
    pub sibling_pair: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaData {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub level_min: i32,
    #[serde(default)]
    pub level_max: i32,
    #[serde(default)]
    pub theme: String,
    /// Owner character name (None = no owner, any builder can edit)
    #[serde(default)]
    pub owner: Option<String>,
    /// Permission level for the area
    #[serde(default)]
    pub permission_level: AreaPermission,
    /// List of trusted builder names (used when permission_level = Trusted)
    #[serde(default)]
    pub trusted_builders: Vec<String>,
    /// Forageable items in city rooms (rooms with city flag)
    #[serde(default)]
    pub city_forage_table: Vec<ForageEntry>,
    /// Forageable items in wilderness rooms (rooms with dirt_floor flag)
    #[serde(default)]
    pub wilderness_forage_table: Vec<ForageEntry>,
    /// Forageable items in shallow water rooms
    #[serde(default)]
    pub shallow_water_forage_table: Vec<ForageEntry>,
    /// Forageable items in deep water rooms
    #[serde(default)]
    pub deep_water_forage_table: Vec<ForageEntry>,
    /// Forageable items in underwater rooms
    #[serde(default)]
    pub underwater_forage_table: Vec<ForageEntry>,
    /// Combat zone type (PvE/Safe/PvP) - area default, can be overridden at room level
    #[serde(default)]
    pub combat_zone: CombatZoneType,
    /// Area-wide flags that rooms can inherit
    #[serde(default)]
    pub flags: AreaFlags,
    /// Template RoomFlags copied into every newly-created room in this area.
    /// Applies at room-creation time only; existing rooms are not retroactively
    /// updated when this changes. Per-room flags still own runtime behavior,
    /// so builders can toggle a default off on a specific room.
    #[serde(default)]
    pub default_room_flags: RoomFlags,

    // === Migrant immigration system ===
    /// When true, the migration tick will attempt to spawn migrants in this area.
    #[serde(default)]
    pub immigration_enabled: bool,
    /// Room vnum where newly-arrived migrants spawn (e.g. town gate, train station).
    #[serde(default)]
    pub immigration_room_vnum: String,
    /// Name of a pool file under scripts/data/names/ (e.g. "generic", "japan").
    #[serde(default)]
    pub immigration_name_pool: String,
    /// Name of a visual profile under scripts/data/visuals/ (e.g. "human").
    #[serde(default)]
    pub immigration_visual_profile: String,
    /// Game days between migration checks (clamped 1..=30 on set).
    #[serde(default)]
    pub migration_interval_days: u8,
    /// Maximum migrants spawned in a single check.
    #[serde(default)]
    pub migration_max_per_check: u8,
    /// Default SimulationConfig values applied to each generated migrant
    /// (home_room_vnum is overridden per-migrant with the claimed liveable room).
    #[serde(default)]
    pub migrant_sim_defaults: Option<SimulationConfig>,
    /// Absolute game-day count at the time of the last migration check
    /// (None = never run; next tick will treat it as due).
    #[serde(default)]
    pub last_migration_check_day: Option<i64>,
    /// Per-role chances that a spawned migrant arrives as a specialized variation.
    #[serde(default)]
    pub immigration_variation_chances: ImmigrationVariationChances,
    /// Per-form chances that a spawn slot resolves to a pre-linked family.
    #[serde(default)]
    pub immigration_family_chance: ImmigrationFamilyChance,
    /// Inclusive range from which a new migrant's starting gold purse is rolled.
    /// `{0, 0}` (the default) keeps legacy behavior: migrants spawn broke. Set a
    /// realistic range (e.g. `{50, 150}`) so newcomers can buy a few meals before
    /// their first paycheck or relief fallback.
    #[serde(default)]
    pub migrant_starting_gold: GoldRange,
    /// Hourly area-treasury wages paid to migrant guards while in any room of
    /// this area. Decoupled from `SimulationConfig.work_pay` so guards earn
    /// without needing a configured `work_room_vnum` (they patrol everywhere).
    /// Default 0 means guards earn no passive wage.
    #[serde(default)]
    pub guard_wage_per_hour: i32,
    /// Hourly "patient visits" wage for migrant healers in any room of this area.
    /// Default 0 disables.
    #[serde(default)]
    pub healer_wage_per_hour: i32,
    /// Hourly scavenging wage for migrant scavengers, paid only while not at
    /// their home room (they have to actually be out scrounging). Default 0.
    #[serde(default)]
    pub scavenger_wage_per_hour: i32,
}

// === Spawn Point System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnEntityType {
    Mobile,
    Item,
}

/// Destination for spawned items in spawn dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnDestination {
    /// Item goes in mobile's inventory
    Inventory,
    /// Item is worn by mobile at specified location
    Equipped(WearLocation),
    /// Item goes inside spawned container (for item spawn points)
    Container,
}

/// Defines an item to spawn along with the main entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnDependency {
    pub item_vnum: String,
    pub destination: SpawnDestination,
    #[serde(default = "default_spawn_count")]
    pub count: i32,
    #[serde(default = "default_spawn_chance")]
    pub chance: i32,
}

fn default_spawn_count() -> i32 {
    1
}
fn default_spawn_chance() -> i32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPointData {
    pub id: Uuid,
    pub area_id: Uuid,
    pub room_id: Uuid,
    pub entity_type: SpawnEntityType,
    pub vnum: String,
    pub max_count: i32,
    pub respawn_interval_secs: i64,
    pub enabled: bool,
    #[serde(default)]
    pub last_spawn_time: i64,
    #[serde(default)]
    pub spawned_entities: Vec<Uuid>,
    /// Item dependencies to spawn alongside the main entity
    #[serde(default)]
    pub dependencies: Vec<SpawnDependency>,
    /// When true (and entity_type is Item), spawned items have flags.buried set.
    #[serde(default)]
    pub bury_on_spawn: bool,
}

// === Item System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WearLocation {
    // Single slots
    Head,
    Neck,
    Shoulders,
    Back,
    Torso,
    Waist,
    Ears,
    Wielded,
    OffHand,
    Ready,
    // Left/Right arm slots
    LeftArm,
    RightArm,
    WristLeft,
    WristRight,
    // Left/Right hand slots
    LeftHand,
    RightHand,
    FingerLeft,
    FingerRight,
    // Left/Right leg slots
    LeftLeg,
    RightLeg,
    LeftAnkle,
    RightAnkle,
    // Left/Right foot slots
    LeftFoot,
    RightFoot,
    // DEPRECATED - kept for data migration
    #[serde(rename = "arms")]
    Arms,
    #[serde(rename = "hands")]
    Hands,
    #[serde(rename = "legs")]
    Legs,
    #[serde(rename = "ankles")]
    Ankles,
    #[serde(rename = "feet")]
    Feet,
    #[serde(rename = "wrists")]
    Wrists,
}

impl WearLocation {
    pub fn from_str(s: &str) -> Option<WearLocation> {
        match s.to_lowercase().replace([' ', '_', '-'], "").as_str() {
            // Single slots
            "head" => Some(WearLocation::Head),
            "neck" => Some(WearLocation::Neck),
            "shoulders" => Some(WearLocation::Shoulders),
            "back" => Some(WearLocation::Back),
            "torso" => Some(WearLocation::Torso),
            "waist" => Some(WearLocation::Waist),
            "ears" => Some(WearLocation::Ears),
            "wielded" | "wield" => Some(WearLocation::Wielded),
            "offhand" => Some(WearLocation::OffHand),
            "ready" | "readied" | "quiver" => Some(WearLocation::Ready),
            // Left/Right arm
            "leftarm" | "larm" => Some(WearLocation::LeftArm),
            "rightarm" | "rarm" => Some(WearLocation::RightArm),
            "leftwrist" | "wristleft" | "lwrist" => Some(WearLocation::WristLeft),
            "rightwrist" | "wristright" | "rwrist" => Some(WearLocation::WristRight),
            // Left/Right hand
            "lefthand" | "lhand" => Some(WearLocation::LeftHand),
            "righthand" | "rhand" => Some(WearLocation::RightHand),
            "leftfinger" | "fingerleft" | "lfinger" => Some(WearLocation::FingerLeft),
            "rightfinger" | "fingerright" | "rfinger" => Some(WearLocation::FingerRight),
            // Left/Right leg
            "leftleg" | "lleg" => Some(WearLocation::LeftLeg),
            "rightleg" | "rleg" => Some(WearLocation::RightLeg),
            "leftankle" | "lankle" => Some(WearLocation::LeftAnkle),
            "rightankle" | "rankle" => Some(WearLocation::RightAnkle),
            // Left/Right foot
            "leftfoot" | "lfoot" => Some(WearLocation::LeftFoot),
            "rightfoot" | "rfoot" => Some(WearLocation::RightFoot),
            // Deprecated (map to L+R for backward compat)
            "arms" => Some(WearLocation::Arms),
            "hands" => Some(WearLocation::Hands),
            "legs" => Some(WearLocation::Legs),
            "ankles" => Some(WearLocation::Ankles),
            "feet" => Some(WearLocation::Feet),
            "wrists" => Some(WearLocation::Wrists),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            // Single slots
            WearLocation::Head => "head",
            WearLocation::Neck => "neck",
            WearLocation::Shoulders => "shoulders",
            WearLocation::Back => "back",
            WearLocation::Torso => "torso",
            WearLocation::Waist => "waist",
            WearLocation::Ears => "ears",
            WearLocation::Wielded => "wielded",
            WearLocation::OffHand => "off-hand",
            WearLocation::Ready => "ready",
            // Left/Right arm
            WearLocation::LeftArm => "left arm",
            WearLocation::RightArm => "right arm",
            WearLocation::WristLeft => "left wrist",
            WearLocation::WristRight => "right wrist",
            // Left/Right hand
            WearLocation::LeftHand => "left hand",
            WearLocation::RightHand => "right hand",
            WearLocation::FingerLeft => "left finger",
            WearLocation::FingerRight => "right finger",
            // Left/Right leg
            WearLocation::LeftLeg => "left leg",
            WearLocation::RightLeg => "right leg",
            WearLocation::LeftAnkle => "left ankle",
            WearLocation::RightAnkle => "right ankle",
            // Left/Right foot
            WearLocation::LeftFoot => "left foot",
            WearLocation::RightFoot => "right foot",
            // Deprecated
            WearLocation::Arms => "arms",
            WearLocation::Hands => "hands",
            WearLocation::Legs => "legs",
            WearLocation::Ankles => "ankles",
            WearLocation::Feet => "feet",
            WearLocation::Wrists => "wrists",
        }
    }

    /// Returns all active (non-deprecated) wear locations
    pub fn all() -> Vec<WearLocation> {
        vec![
            WearLocation::Head,
            WearLocation::Neck,
            WearLocation::Shoulders,
            WearLocation::Back,
            WearLocation::Torso,
            WearLocation::Waist,
            WearLocation::Ears,
            WearLocation::Wielded,
            WearLocation::OffHand,
            WearLocation::Ready,
            WearLocation::LeftArm,
            WearLocation::RightArm,
            WearLocation::WristLeft,
            WearLocation::WristRight,
            WearLocation::LeftHand,
            WearLocation::RightHand,
            WearLocation::FingerLeft,
            WearLocation::FingerRight,
            WearLocation::LeftLeg,
            WearLocation::RightLeg,
            WearLocation::LeftAnkle,
            WearLocation::RightAnkle,
            WearLocation::LeftFoot,
            WearLocation::RightFoot,
        ]
    }

    /// Returns true if this is a deprecated wear location
    pub fn is_deprecated(&self) -> bool {
        matches!(
            self,
            WearLocation::Arms
                | WearLocation::Hands
                | WearLocation::Legs
                | WearLocation::Ankles
                | WearLocation::Feet
                | WearLocation::Wrists
        )
    }

    /// For deprecated locations, returns the L/R equivalents
    pub fn to_lr_equivalents(&self) -> Vec<WearLocation> {
        match self {
            WearLocation::Arms => vec![WearLocation::LeftArm, WearLocation::RightArm],
            WearLocation::Hands => vec![WearLocation::LeftHand, WearLocation::RightHand],
            WearLocation::Legs => vec![WearLocation::LeftLeg, WearLocation::RightLeg],
            WearLocation::Ankles => vec![WearLocation::LeftAnkle, WearLocation::RightAnkle],
            WearLocation::Feet => vec![WearLocation::LeftFoot, WearLocation::RightFoot],
            WearLocation::Wrists => vec![WearLocation::WristLeft, WearLocation::WristRight],
            _ => vec![*self],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    #[default]
    Misc,
    Armor,
    Weapon,
    Container,
    LiquidContainer,
    Food,
    Key,
    Gold,
    Ammunition,
}

impl ItemType {
    pub fn from_str(s: &str) -> Option<ItemType> {
        match s.to_lowercase().as_str() {
            "misc" => Some(ItemType::Misc),
            "armor" => Some(ItemType::Armor),
            "weapon" => Some(ItemType::Weapon),
            "container" => Some(ItemType::Container),
            "liquid_container" | "liquidcontainer" | "drink" | "drinkcon" => Some(ItemType::LiquidContainer),
            "food" => Some(ItemType::Food),
            "key" => Some(ItemType::Key),
            "gold" => Some(ItemType::Gold),
            "ammunition" | "ammo" => Some(ItemType::Ammunition),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            ItemType::Misc => "misc",
            ItemType::Armor => "armor",
            ItemType::Weapon => "weapon",
            ItemType::Container => "container",
            ItemType::LiquidContainer => "liquid_container",
            ItemType::Food => "food",
            ItemType::Key => "key",
            ItemType::Gold => "gold",
            ItemType::Ammunition => "ammunition",
        }
    }
}

/// Returns the tier description for a gold amount
pub fn get_gold_tier_description(amount: i32) -> &'static str {
    match amount {
        1..=10 => "a few coins",
        11..=50 => "some gold",
        51..=200 => "a pile of gold",
        201..=1000 => "a large pile of gold",
        _ if amount > 1000 => "a fortune in gold",
        _ => "some gold", // fallback for 0 or negative
    }
}

/// Returns the short description for gold (shown in room listings)
pub fn get_gold_short_desc(amount: i32) -> String {
    let tier = get_gold_tier_description(amount);
    // Capitalize first letter
    let mut chars = tier.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str() + " lies here.",
    }
}

/// Returns the long description for gold (shown when examined)
pub fn get_gold_long_desc(amount: i32) -> String {
    match amount {
        1..=10 => format!("A small scattering of {} gold coins glints in the light.", amount),
        11..=50 => format!("A modest collection of {} gold coins is piled here.", amount),
        51..=200 => format!("An enticing pile of {} gold coins awaits collection.", amount),
        201..=1000 => format!("A large heap of {} gold coins gleams invitingly.", amount),
        _ => format!(
            "An absolutely staggering fortune of {} gold coins fills the area.",
            amount
        ),
    }
}

/// Creates a new gold item with the specified amount
pub fn create_gold_item(amount: i32) -> ItemData {
    let tier = get_gold_tier_description(amount);
    let mut item = ItemData::new(
        tier.to_string(),
        get_gold_short_desc(amount),
        get_gold_long_desc(amount),
    );
    item.item_type = ItemType::Gold;
    item.value = amount;
    item.keywords = vec!["gold".to_string(), "coins".to_string(), "coin".to_string()];
    item.weight = (amount / 100).max(1);
    item
}

/// Updates gold item descriptions after the amount changes (e.g., after merging)
pub fn update_gold_descriptions(item: &mut ItemData) {
    if item.item_type == ItemType::Gold {
        item.name = get_gold_tier_description(item.value).to_string();
        item.short_desc = get_gold_short_desc(item.value);
        item.long_desc = get_gold_long_desc(item.value);
        item.weight = (item.value / 100).max(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    #[default]
    Bludgeoning,
    Slashing,
    Piercing,
    Fire,
    Cold,
    Lightning,
    Poison,
    Acid,
    Bite,
    Ballistic,
    Arcane,
}

impl DamageType {
    pub fn from_str(s: &str) -> Option<DamageType> {
        match s.to_lowercase().as_str() {
            "bludgeoning" | "bludgeon" => Some(DamageType::Bludgeoning),
            "slashing" | "slash" => Some(DamageType::Slashing),
            "piercing" | "pierce" => Some(DamageType::Piercing),
            "fire" => Some(DamageType::Fire),
            "cold" => Some(DamageType::Cold),
            "lightning" => Some(DamageType::Lightning),
            "poison" => Some(DamageType::Poison),
            "acid" => Some(DamageType::Acid),
            "bite" => Some(DamageType::Bite),
            "ballistic" | "bullet" | "projectile" => Some(DamageType::Ballistic),
            "arcane" | "magic" => Some(DamageType::Arcane),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            DamageType::Bludgeoning => "bludgeoning",
            DamageType::Slashing => "slashing",
            DamageType::Piercing => "piercing",
            DamageType::Fire => "fire",
            DamageType::Cold => "cold",
            DamageType::Lightning => "lightning",
            DamageType::Poison => "poison",
            DamageType::Acid => "acid",
            DamageType::Bite => "bite",
            DamageType::Ballistic => "ballistic",
            DamageType::Arcane => "arcane",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "bludgeoning",
            "slashing",
            "piercing",
            "fire",
            "cold",
            "lightning",
            "poison",
            "acid",
            "bite",
            "ballistic",
            "arcane",
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EffectType {
    #[default]
    None,
    Heal,
    Poison,
    ManaRestore,
    StaminaRestore,
    StrengthBoost,
    DexterityBoost,
    ConstitutionBoost,
    IntelligenceBoost,
    WisdomBoost,
    CharismaBoost,
    Haste,
    Slow,
    Sleep,
    Blind,
    Invisibility,
    DetectInvisible,
    DetectMagic,
    NightVision,
    Regeneration,
    Drunk,
    Satiated,
    Quenched,
    ArmorClassBoost,
    MagicLight,
    Disguise,
    WaterBreathing,
    DamageReduction,
    Charmed,
    Curse,
}

impl EffectType {
    pub fn from_str(s: &str) -> Option<EffectType> {
        match s.to_lowercase().as_str() {
            "none" => Some(EffectType::None),
            "heal" => Some(EffectType::Heal),
            "poison" => Some(EffectType::Poison),
            "mana_restore" | "manarestore" | "mana" => Some(EffectType::ManaRestore),
            "stamina_restore" | "staminarestore" | "stamina" => Some(EffectType::StaminaRestore),
            "strength_boost" | "strengthboost" | "str_boost" | "strength" => Some(EffectType::StrengthBoost),
            "dexterity_boost" | "dexterityboost" | "dex_boost" | "dexterity" => Some(EffectType::DexterityBoost),
            "constitution_boost" | "constitutionboost" | "con_boost" | "constitution" => {
                Some(EffectType::ConstitutionBoost)
            }
            "intelligence_boost" | "intelligenceboost" | "int_boost" | "intelligence" => {
                Some(EffectType::IntelligenceBoost)
            }
            "wisdom_boost" | "wisdomboost" | "wis_boost" | "wisdom" => Some(EffectType::WisdomBoost),
            "charisma_boost" | "charismaboost" | "cha_boost" | "charisma" => Some(EffectType::CharismaBoost),
            "haste" => Some(EffectType::Haste),
            "slow" => Some(EffectType::Slow),
            "sleep" => Some(EffectType::Sleep),
            "blind" | "blindness" => Some(EffectType::Blind),
            "invisibility" | "invis" => Some(EffectType::Invisibility),
            "detect_invisible" | "detectinvisible" | "detect_invis" => Some(EffectType::DetectInvisible),
            "detect_magic" | "detectmagic" => Some(EffectType::DetectMagic),
            "night_vision" | "nightvision" | "infravision" => Some(EffectType::NightVision),
            "regeneration" | "regen" => Some(EffectType::Regeneration),
            "drunk" => Some(EffectType::Drunk),
            "satiated" => Some(EffectType::Satiated),
            "quenched" => Some(EffectType::Quenched),
            "armor_class_boost" | "armorclassboost" | "ac_boost" | "arcane_shield" => Some(EffectType::ArmorClassBoost),
            "magic_light" | "magiclight" | "light" => Some(EffectType::MagicLight),
            "disguise" => Some(EffectType::Disguise),
            "water_breathing" | "waterbreathing" | "aqua_breath" => Some(EffectType::WaterBreathing),
            "damage_reduction" | "damagereduction" | "sanctuary" => Some(EffectType::DamageReduction),
            "charm" | "charmed" => Some(EffectType::Charmed),
            "curse" | "cursed" => Some(EffectType::Curse),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            EffectType::None => "none",
            EffectType::Heal => "heal",
            EffectType::Poison => "poison",
            EffectType::ManaRestore => "mana_restore",
            EffectType::StaminaRestore => "stamina_restore",
            EffectType::StrengthBoost => "strength_boost",
            EffectType::DexterityBoost => "dexterity_boost",
            EffectType::ConstitutionBoost => "constitution_boost",
            EffectType::IntelligenceBoost => "intelligence_boost",
            EffectType::WisdomBoost => "wisdom_boost",
            EffectType::CharismaBoost => "charisma_boost",
            EffectType::Haste => "haste",
            EffectType::Slow => "slow",
            EffectType::Sleep => "sleep",
            EffectType::Blind => "blind",
            EffectType::Invisibility => "invisibility",
            EffectType::DetectInvisible => "detect_invisible",
            EffectType::DetectMagic => "detect_magic",
            EffectType::NightVision => "night_vision",
            EffectType::Regeneration => "regeneration",
            EffectType::Drunk => "drunk",
            EffectType::Satiated => "satiated",
            EffectType::Quenched => "quenched",
            EffectType::ArmorClassBoost => "armor_class_boost",
            EffectType::MagicLight => "magic_light",
            EffectType::Disguise => "disguise",
            EffectType::WaterBreathing => "water_breathing",
            EffectType::DamageReduction => "damage_reduction",
            EffectType::Charmed => "charmed",
            EffectType::Curse => "curse",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "none",
            "heal",
            "poison",
            "mana_restore",
            "stamina_restore",
            "strength_boost",
            "dexterity_boost",
            "constitution_boost",
            "intelligence_boost",
            "wisdom_boost",
            "charisma_boost",
            "haste",
            "slow",
            "sleep",
            "blind",
            "invisibility",
            "detect_invisible",
            "detect_magic",
            "night_vision",
            "regeneration",
            "drunk",
            "satiated",
            "quenched",
            "armor_class_boost",
            "magic_light",
            "disguise",
            "water_breathing",
            "damage_reduction",
            "charmed",
            "curse",
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LiquidType {
    #[default]
    Water,
    Ale,
    Wine,
    Beer,
    Alcohol,
    Spirits,
    Milk,
    Juice,
    Tea,
    Coffee,
    Poison,
    HealingPotion,
    ManaPotion,
    Blood,
    Oil,
}

impl LiquidType {
    pub fn from_str(s: &str) -> Option<LiquidType> {
        match s.to_lowercase().as_str() {
            "water" => Some(LiquidType::Water),
            "ale" => Some(LiquidType::Ale),
            "wine" => Some(LiquidType::Wine),
            "beer" | "mead" => Some(LiquidType::Beer),
            "alcohol" => Some(LiquidType::Alcohol),
            "spirits" | "liquor" | "cocktail" => Some(LiquidType::Spirits),
            "milk" => Some(LiquidType::Milk),
            "juice" => Some(LiquidType::Juice),
            "tea" => Some(LiquidType::Tea),
            "coffee" => Some(LiquidType::Coffee),
            "poison" => Some(LiquidType::Poison),
            "healing_potion" | "healingpotion" | "heal_potion" => Some(LiquidType::HealingPotion),
            "mana_potion" | "manapotion" => Some(LiquidType::ManaPotion),
            "blood" => Some(LiquidType::Blood),
            "oil" => Some(LiquidType::Oil),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            LiquidType::Water => "water",
            LiquidType::Ale => "ale",
            LiquidType::Wine => "wine",
            LiquidType::Beer => "beer",
            LiquidType::Alcohol => "alcohol",
            LiquidType::Spirits => "spirits",
            LiquidType::Milk => "milk",
            LiquidType::Juice => "juice",
            LiquidType::Tea => "tea",
            LiquidType::Coffee => "coffee",
            LiquidType::Poison => "poison",
            LiquidType::HealingPotion => "healing_potion",
            LiquidType::ManaPotion => "mana_potion",
            LiquidType::Blood => "blood",
            LiquidType::Oil => "oil",
        }
    }

    pub fn all() -> Vec<&'static str> {
        vec![
            "water",
            "ale",
            "wine",
            "beer",
            "alcohol",
            "spirits",
            "milk",
            "juice",
            "tea",
            "coffee",
            "poison",
            "healing_potion",
            "mana_potion",
            "blood",
            "oil",
        ]
    }

    /// Returns default liquid effects for this liquid type, mirroring oedit auto_set_liquid_defaults.
    pub fn default_effects(&self) -> Vec<ItemEffect> {
        match self {
            LiquidType::Water => vec![ItemEffect {
                effect_type: EffectType::Quenched,
                magnitude: 100,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::Ale => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 2,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 50,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Wine => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 4,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Beer => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 2,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 50,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Satiated,
                    magnitude: 10,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Alcohol => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 6,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Spirits => vec![
                ItemEffect {
                    effect_type: EffectType::Drunk,
                    magnitude: 5,
                    duration: 300,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 25,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Milk => vec![
                ItemEffect {
                    effect_type: EffectType::Satiated,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 80,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Juice => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 5,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 80,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Tea => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 3,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 90,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Coffee => vec![
                ItemEffect {
                    effect_type: EffectType::StaminaRestore,
                    magnitude: 8,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 70,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Poison => vec![ItemEffect {
                effect_type: EffectType::Poison,
                magnitude: 10,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::HealingPotion => vec![
                ItemEffect {
                    effect_type: EffectType::Heal,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::ManaPotion => vec![
                ItemEffect {
                    effect_type: EffectType::ManaRestore,
                    magnitude: 20,
                    duration: 0,
                    script_callback: None,
                },
                ItemEffect {
                    effect_type: EffectType::Quenched,
                    magnitude: 30,
                    duration: 0,
                    script_callback: None,
                },
            ],
            LiquidType::Blood => vec![ItemEffect {
                effect_type: EffectType::Satiated,
                magnitude: 10,
                duration: 0,
                script_callback: None,
            }],
            LiquidType::Oil => vec![ItemEffect {
                effect_type: EffectType::Poison,
                magnitude: 3,
                duration: 0,
                script_callback: None,
            }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemEffect {
    pub effect_type: EffectType,
    pub magnitude: i32,
    pub duration: i32, // seconds, 0 = instant
    #[serde(default)]
    pub script_callback: Option<String>,
}

impl Default for ItemEffect {
    fn default() -> Self {
        ItemEffect {
            effect_type: EffectType::None,
            magnitude: 0,
            duration: 0,
            script_callback: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub effect_type: EffectType,
    pub magnitude: i32,
    pub remaining_secs: i32, // -1 = permanent until dispelled
    pub source: String,      // e.g. "coffee", "healing potion"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemFlags {
    #[serde(default)]
    pub no_drop: bool,
    #[serde(default)]
    pub no_get: bool,
    #[serde(default)]
    pub no_remove: bool,
    #[serde(default)]
    pub invisible: bool,
    #[serde(default)]
    pub glow: bool,
    #[serde(default)]
    pub hum: bool,
    #[serde(default)]
    pub magical: bool, // Reveals "(magical aura)" cue when viewer has DetectMagic buff
    #[serde(default)]
    pub no_sell: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub quest_item: bool,
    #[serde(default)]
    pub vending: bool, // Functions as a vending machine
    #[serde(default)]
    pub provides_light: bool, // Provides light when equipped/wielded
    #[serde(default)]
    pub night_vision: bool, // Grants the wearer night vision while equipped
    #[serde(default)]
    pub fishing_rod: bool, // Can be used for fishing when held
    #[serde(default)]
    pub bait: bool, // Can be used as fishing bait
    #[serde(default)]
    pub foraging_tool: bool, // Can be used as foraging tool (uses quality for bonus)
    #[serde(default)]
    pub waterproof: bool, // Protects from rain/water when worn
    #[serde(default)]
    pub provides_warmth: bool, // Radiates warmth to room (campfire, fireplace)
    #[serde(default)]
    pub reduces_glare: bool, // Reduces bright light penalty (sunglasses)
    #[serde(default)]
    pub medical_tool: bool, // Can be used for medical treatment
    #[serde(default)]
    pub preserves_contents: bool, // Container preserves food inside (fridge/freezer)
    #[serde(default)]
    pub death_only: bool, // Only visible in corpse after death
    #[serde(default)]
    pub atm: bool, // Functions as an ATM for banking
    // Corpse system fields
    #[serde(default)]
    pub is_corpse: bool, // This item is a corpse container
    #[serde(default)]
    pub corpse_owner: String, // Name of the dead character/mobile
    #[serde(default)]
    pub corpse_created_at: i64, // Unix timestamp when corpse was created
    #[serde(default)]
    pub corpse_is_player: bool, // true = 1hr decay, false = 10min decay
    #[serde(default)]
    pub corpse_gold: i64, // Gold carried by the corpse
    #[serde(default)]
    pub broken: bool, // Broken arrows/bolts cannot be used as ammo
    // Gardening system flags
    #[serde(default)]
    pub plant_pot: bool, // Can be used as a planting container
    // Stealth/thievery system flags
    #[serde(default)]
    pub lockpick: bool, // Can be used to pick locks
    #[serde(default)]
    pub is_skinned: bool, // Corpse has been butchered/skinned
    // Water system flags
    #[serde(default)]
    pub boat: bool, // Allows traversing deep_water rooms when in inventory
    // Buried treasure system
    #[serde(default)]
    pub buried: bool, // Hidden in a dirt_floor room until dug up
    #[serde(default)]
    pub can_dig: bool, // Held/equipped item allows player to dig
    #[serde(default)]
    pub detect_buried: bool, // Surfaces a hint when buried items are nearby
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "id")]
pub enum ItemLocation {
    #[default]
    Nowhere,
    Room(Uuid),
    Inventory(String),
    Equipped(String),
    Container(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemData {
    pub id: Uuid,
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub item_type: ItemType,
    // Categories for recipe ingredient/tool matching (e.g., "flour", "stick", "bamboo")
    #[serde(default, deserialize_with = "deserialize_categories", alias = "category")]
    pub categories: Vec<String>,
    // Recipe ID this item teaches when read/used (for recipe books/scrolls)
    #[serde(default)]
    pub teaches_recipe: Option<String>,
    // Spell ID this item teaches when read (for spell scrolls)
    #[serde(default)]
    pub teaches_spell: Option<String>,
    // Long-form readable body (ascii maps, tutorials, in-world documents).
    // Authored via `oedit <id> note` multi-line editor; surfaced by `read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_content: Option<String>,
    #[serde(default)]
    pub wear_locations: Vec<WearLocation>,
    #[serde(default)]
    pub armor_class: Option<i32>,
    /// Body parts this armor protects (for armor items)
    #[serde(default)]
    pub protects: Vec<BodyPart>,
    #[serde(default)]
    pub flags: ItemFlags,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub value: i32,
    #[serde(default)]
    pub location: ItemLocation,
    // Weapon fields
    #[serde(default)]
    pub damage_dice_count: i32,
    #[serde(default)]
    pub damage_dice_sides: i32,
    #[serde(default)]
    pub damage_type: DamageType,
    #[serde(default)]
    pub two_handed: bool,
    /// Weapon skill used by this weapon (for combat XP)
    #[serde(default)]
    pub weapon_skill: Option<WeaponSkill>,
    // Container fields (ItemType::Container)
    #[serde(default)]
    pub container_contents: Vec<Uuid>,
    #[serde(default)]
    pub container_max_items: i32,
    #[serde(default)]
    pub container_max_weight: i32,
    #[serde(default)]
    pub container_closed: bool,
    #[serde(default)]
    pub container_locked: bool,
    #[serde(default)]
    pub container_key_vnum: Option<String>,
    // Weight reduction when worn (0-100 percent, e.g., 50 = contents weigh 50% when worn)
    #[serde(default)]
    pub weight_reduction: i32,
    // Liquid Container fields (ItemType::LiquidContainer)
    #[serde(default)]
    pub liquid_type: LiquidType,
    #[serde(default)]
    pub liquid_current: i32,
    #[serde(default)]
    pub liquid_max: i32,
    #[serde(default)]
    pub liquid_poisoned: bool,
    #[serde(default)]
    pub liquid_effects: Vec<ItemEffect>,
    // Food fields (ItemType::Food)
    #[serde(default)]
    pub food_nutrition: i32,
    #[serde(default)]
    pub food_poisoned: bool,
    #[serde(default)]
    pub food_spoil_duration: i64,
    #[serde(default)]
    pub food_created_at: Option<i64>,
    #[serde(default)]
    pub food_effects: Vec<ItemEffect>,
    #[serde(default)]
    pub food_spoilage_points: f64, // 0.0 = fresh, 1.0 = spoiled
    #[serde(default)]
    pub preservation_level: i32, // 0=none, 1=fridge/cool, 2=freezer/frozen (for containers)
    // Level requirement and stat bonuses
    #[serde(default)]
    pub level_requirement: i32,
    #[serde(default)]
    pub stat_str: i32,
    #[serde(default)]
    pub stat_dex: i32,
    #[serde(default)]
    pub stat_con: i32,
    #[serde(default)]
    pub stat_int: i32,
    #[serde(default)]
    pub stat_wis: i32,
    #[serde(default)]
    pub stat_cha: i32,
    // Insulation for temperature/weather system
    #[serde(default)]
    pub insulation: i32, // 0-100 scale for warmth
    // Prototype fields
    #[serde(default)]
    pub is_prototype: bool,
    #[serde(default)]
    pub vnum: Option<String>,
    // World-wide cap on live (non-prototype) instances of this vnum.
    // None = unlimited. Some(n) = refuse spawn when count >= n.
    // `flags.unique` is sugar for Some(1).
    #[serde(default)]
    pub world_max_count: Option<i32>,
    // Item triggers
    #[serde(default)]
    pub triggers: Vec<ItemTrigger>,
    // Vending machine fields (requires flags.vending = true)
    #[serde(default)]
    pub vending_stock: Vec<String>, // Vnums for infinite stock
    #[serde(default = "default_vending_sell_rate")]
    pub vending_sell_rate: i32, // % charged when selling (default 150)
    // Generic quality field (0-100, used by fishing rods, bait, etc.)
    #[serde(default)]
    pub quality: i32,
    // Bait-specific fields (requires flags.bait = true)
    #[serde(default)]
    pub bait_uses: i32, // Uses remaining (0 = infinite)
    // Combat system - armor degradation
    /// Armor holes from combat damage (0-3, destroyed at 3)
    #[serde(default)]
    pub holes: i32,
    // Medical tool fields (requires flags.medical_tool = true)
    #[serde(default)]
    pub medical_tier: i32, // 1=basic, 2=intermediate, 3=advanced
    #[serde(default)]
    pub medical_uses: i32, // 0 = reusable, >0 = consumable uses
    #[serde(default)]
    pub treats_wound_types: Vec<String>, // ["cut", "puncture", "burn", etc.]
    #[serde(default)]
    pub max_treatable_wound: String, // "minor", "moderate", "severe", "critical"
    // Transport sign - links to a TransportData to show status when read
    #[serde(default)]
    pub transport_link: Option<Uuid>,
    // Ammunition fields (for both weapons with caliber and ammunition items)
    #[serde(default)]
    pub caliber: Option<String>, // "arrow", "bolt", "9mm", "5.56mm"
    #[serde(default)]
    pub ammo_count: i32, // Stack size for ammunition items
    #[serde(default)]
    pub ammo_damage_bonus: i32, // Quality bonus to damage
    // Crossbow/Firearm fields (internal magazine weapons)
    #[serde(default)]
    pub ranged_type: Option<String>, // "bow", "crossbow", "firearm"
    #[serde(default)]
    pub magazine_size: i32, // weapon capacity (crossbow=1, pistol=15, etc.)
    #[serde(default)]
    pub loaded_ammo: i32, // currently loaded rounds
    #[serde(default)]
    pub loaded_ammo_bonus: i32, // ammo_damage_bonus captured at reload time
    #[serde(default)]
    pub loaded_ammo_vnum: Option<String>, // vnum of loaded ammo prototype (for unload)
    #[serde(default)]
    pub fire_mode: String, // current: "single", "burst", "auto"
    #[serde(default)]
    pub supported_fire_modes: Vec<String>, // which modes this weapon supports
    #[serde(default)]
    pub noise_level: String, // "silent", "quiet", "normal", "loud" or "" for default
    // Special ammo effect payload (ammunition items)
    #[serde(default)]
    pub ammo_effect_type: String, // "fire", "cold", "poison", "acid", or ""
    #[serde(default)]
    pub ammo_effect_duration: i32,
    #[serde(default)]
    pub ammo_effect_damage: i32,
    // Captured at reload for magazine weapons
    #[serde(default)]
    pub loaded_ammo_effect_type: String,
    #[serde(default)]
    pub loaded_ammo_effect_duration: i32,
    #[serde(default)]
    pub loaded_ammo_effect_damage: i32,
    // Attachment properties (for attachment items)
    #[serde(default)]
    pub attachment_slot: String, // "scope", "suppressor", "magazine", "accessory"
    #[serde(default)]
    pub attachment_accuracy_bonus: i32,
    #[serde(default)]
    pub attachment_noise_reduction: i32,
    #[serde(default)]
    pub attachment_magazine_bonus: i32,
    #[serde(default)]
    pub attachment_compatible_types: Vec<String>,
    // Gardening system fields
    /// Plant prototype vnum this seed creates (for seed items)
    #[serde(default)]
    pub plant_prototype_vnum: String,
    /// Duration of fertilizer effect in game hours (for fertilizer items)
    #[serde(default)]
    pub fertilizer_duration: i64,
    /// Infestation type this item treats: "aphids", "blight", "root_rot", "frost", or "all"
    #[serde(default)]
    pub treats_infestation: String,
}

impl ItemData {
    pub fn new(name: String, short_desc: String, long_desc: String) -> Self {
        ItemData {
            id: Uuid::new_v4(),
            name,
            short_desc,
            long_desc,
            keywords: Vec::new(),
            item_type: ItemType::Misc,
            categories: Vec::new(),
            teaches_recipe: None,
            teaches_spell: None,
            note_content: None,
            wear_locations: Vec::new(),
            armor_class: None,
            protects: Vec::new(),
            holes: 0,
            flags: ItemFlags::default(),
            weight: 0,
            value: 0,
            location: ItemLocation::Nowhere,
            damage_dice_count: 0,
            damage_dice_sides: 0,
            damage_type: DamageType::default(),
            two_handed: false,
            weapon_skill: None,
            // Container fields
            container_contents: Vec::new(),
            container_max_items: 0,
            container_max_weight: 0,
            container_closed: false,
            container_locked: false,
            container_key_vnum: None,
            weight_reduction: 0,
            // Liquid container fields
            liquid_type: LiquidType::default(),
            liquid_current: 0,
            liquid_max: 0,
            liquid_poisoned: false,
            liquid_effects: Vec::new(),
            // Food fields
            food_nutrition: 0,
            food_poisoned: false,
            food_spoil_duration: 0,
            food_created_at: None,
            food_effects: Vec::new(),
            food_spoilage_points: 0.0,
            preservation_level: 0,
            level_requirement: 0,
            stat_str: 0,
            stat_dex: 0,
            stat_con: 0,
            stat_int: 0,
            stat_wis: 0,
            stat_cha: 0,
            insulation: 0,
            is_prototype: false,
            vnum: None,
            world_max_count: None,
            triggers: Vec::new(),
            // Vending machine fields
            vending_stock: Vec::new(),
            vending_sell_rate: 150,
            // Quality and bait fields
            quality: 0,
            bait_uses: 0,
            // Medical tool fields
            medical_tier: 0,
            medical_uses: 0,
            treats_wound_types: Vec::new(),
            max_treatable_wound: String::new(),
            // Transport sign
            transport_link: None,
            // Ammunition fields
            caliber: None,
            ammo_count: 0,
            ammo_damage_bonus: 0,
            // Crossbow/Firearm fields
            ranged_type: None,
            magazine_size: 0,
            loaded_ammo: 0,
            loaded_ammo_bonus: 0,
            loaded_ammo_vnum: None,
            fire_mode: String::new(),
            supported_fire_modes: Vec::new(),
            noise_level: String::new(),
            // Special ammo effect fields
            ammo_effect_type: String::new(),
            ammo_effect_duration: 0,
            ammo_effect_damage: 0,
            loaded_ammo_effect_type: String::new(),
            loaded_ammo_effect_duration: 0,
            loaded_ammo_effect_damage: 0,
            // Attachment fields
            attachment_slot: String::new(),
            attachment_accuracy_bonus: 0,
            attachment_noise_reduction: 0,
            attachment_magazine_bonus: 0,
            attachment_compatible_types: Vec::new(),
            // Gardening fields
            plant_prototype_vnum: String::new(),
            fertilizer_duration: 0,
            treats_infestation: String::new(),
        }
    }
}

// === Mobile/NPC System ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobileFlags {
    #[serde(default)]
    pub aggressive: bool, // Attacks players on sight
    #[serde(default)]
    pub sentinel: bool, // Never wanders from spawn room
    #[serde(default)]
    pub scavenger: bool, // Picks up items from ground
    #[serde(default)]
    pub shopkeeper: bool, // Can buy/sell items
    #[serde(default)]
    pub no_attack: bool, // Cannot be attacked
    #[serde(default)]
    pub healer: bool, // Can provide healing services
    #[serde(default)]
    pub leasing_agent: bool, // Can rent out property templates
    #[serde(default)]
    pub cowardly: bool, // Flees when sniped or HP < 25%
    #[serde(default)]
    pub can_open_doors: bool, // Can open/unlock doors during routine pathfinding
    #[serde(default)]
    pub guard: bool, // Enhanced perception, responds to nearby theft
    #[serde(default)]
    pub helper: bool, // Joins combat to defend faction allies (or any NPC if faction is empty) attacked by a player
    #[serde(default)]
    pub thief: bool, // Steals gold from players
    #[serde(default)]
    pub cant_swim: bool, // Cannot enter water rooms, takes damage if in water
    #[serde(default)]
    pub poisonous: bool, // Melee hits apply a poison DoT
    #[serde(default)]
    pub fiery: bool, // Melee hits apply a fire DoT
    #[serde(default)]
    pub chilling: bool, // Melee hits apply a cold DoT
    #[serde(default)]
    pub corrosive: bool, // Melee hits apply an acid DoT
    #[serde(default)]
    pub shocking: bool, // Melee hits apply a lightning DoT
    #[serde(default)]
    pub unique: bool, // Only one live (non-prototype) instance of this vnum allowed in the world
    #[serde(default)]
    pub stay_zone: bool, // Wandering / pursuit stays inside home_area_id
    #[serde(default)]
    pub aware: bool, // Sees through hidden / sneaking / invisibility
    #[serde(default)]
    pub memory: bool, // Remembers PC attackers and attacks on sight
    #[serde(default)]
    pub no_sleep: bool, // Immune to the sleep spell (CircleMUD MOB_NOSLEEP)
    #[serde(default)]
    pub no_blind: bool, // Immune to the blind spell (CircleMUD MOB_NOBLIND)
    #[serde(default)]
    pub no_bash: bool, // Immune to the bash skill's stun (CircleMUD MOB_NOBASH)
    #[serde(default)]
    pub no_summon: bool, // Immune to the summon spell (CircleMUD MOB_NOSUMMON)
    #[serde(default)]
    pub no_charm: bool, // Immune to the charm spell (CircleMUD MOB_NOCHARM)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RememberedEnemy {
    pub name: String,
    #[serde(default)]
    pub expires_at_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileData {
    // Identity
    pub id: Uuid,
    pub name: String,       // "a goblin guard"
    pub short_desc: String, // "A goblin guard stands here."
    pub long_desc: String,  // Full description
    #[serde(default)]
    pub keywords: Vec<String>, // ["goblin", "guard"]

    // Location
    #[serde(default)]
    pub current_room_id: Option<Uuid>,

    // Prototype system (same as items)
    #[serde(default)]
    pub is_prototype: bool,
    #[serde(default)]
    pub vnum: String, // "goblin_guard"
    // World-wide cap on live (non-prototype) instances of this vnum.
    // None = unlimited. Some(n) = refuse spawn when count >= n.
    // `flags.unique` is sugar for Some(1).
    #[serde(default)]
    pub world_max_count: Option<i32>,

    // Base stats (for future combat)
    #[serde(default)]
    pub level: i32,
    #[serde(default = "default_mobile_hp")]
    pub max_hp: i32,
    #[serde(default = "default_mobile_hp")]
    pub current_hp: i32,
    #[serde(default = "default_mobile_stamina")]
    pub max_stamina: i32,
    #[serde(default = "default_mobile_stamina")]
    pub current_stamina: i32,
    #[serde(default)]
    pub damage_dice: String, // "2d6+3"
    #[serde(default)]
    pub damage_type: DamageType,
    #[serde(default)]
    pub armor_class: i32,
    #[serde(default)]
    pub hit_modifier: i32, // Combat skill modifier based on level
    #[serde(default)]
    pub gold: i32,

    // Attributes
    #[serde(default = "default_stat")]
    pub stat_str: i32,
    #[serde(default = "default_stat")]
    pub stat_dex: i32,
    #[serde(default = "default_stat")]
    pub stat_con: i32,
    #[serde(default = "default_stat")]
    pub stat_int: i32,
    #[serde(default = "default_stat")]
    pub stat_wis: i32,
    #[serde(default = "default_stat")]
    pub stat_cha: i32,

    // AI/Behavior flags
    #[serde(default)]
    pub flags: MobileFlags,

    // Simple dialogue (keyword -> response)
    #[serde(default)]
    pub dialogue: HashMap<String, String>,

    // Shop system (requires shopkeeper flag)
    #[serde(default)]
    pub shop_stock: Vec<String>, // Vnums for infinite base stock
    #[serde(default)]
    pub shop_inventory: Vec<Uuid>, // Items bought from players (finite)
    #[serde(default = "default_shop_buy_rate")]
    pub shop_buy_rate: i32, // % paid when buying from player (e.g., 50)
    #[serde(default = "default_shop_sell_rate")]
    pub shop_sell_rate: i32, // % charged when selling (e.g., 150)
    #[serde(default = "default_shop_buys_types")]
    pub shop_buys_types: Vec<String>, // Item types this shop buys ("all" = any, empty = none)
    #[serde(default)]
    pub shop_buys_categories: Vec<String>, // Categories this shop buys directly
    #[serde(default)]
    pub shop_preset_vnum: String, // Preset reference (live lookup)
    #[serde(default)]
    pub shop_extra_types: Vec<String>, // Add types beyond preset
    #[serde(default)]
    pub shop_extra_categories: Vec<String>, // Add categories beyond preset
    #[serde(default)]
    pub shop_deny_types: Vec<String>, // Exclude types from preset
    #[serde(default)]
    pub shop_deny_categories: Vec<String>, // Exclude categories from preset
    #[serde(default)]
    pub shop_min_value: i32, // 0 = no minimum
    #[serde(default)]
    pub shop_max_value: i32, // 0 = no maximum

    // Healer system (requires healer flag)
    #[serde(default)]
    pub healer_type: String, // "medic", "herbalist", "cleric"
    #[serde(default)]
    pub healing_free: bool, // Free healing or charges gold?
    #[serde(default = "default_healing_cost_multiplier")]
    pub healing_cost_multiplier: i32, // 100 = base price, 200 = 2x, etc.

    // Trigger system
    #[serde(default)]
    pub triggers: Vec<MobileTrigger>,

    // Transport route (for NPCs that travel)
    #[serde(default)]
    pub transport_route: Option<TransportRoute>,

    // Leasing agent system (requires leasing_agent flag)
    #[serde(default)]
    pub property_templates: Vec<String>, // Vnums of available PropertyTemplates
    #[serde(default)]
    pub leasing_area_id: Option<Uuid>, // Area this agent manages

    // Combat system
    #[serde(default)]
    pub combat: CombatState,
    #[serde(default)]
    pub wounds: Vec<Wound>,
    #[serde(default)]
    pub ongoing_effects: Vec<OngoingEffect>,
    #[serde(default)]
    pub scars: HashMap<String, i32>, // body_part display name -> scar count
    // Death/unconscious state (not persisted)
    #[serde(skip)]
    pub is_unconscious: bool,
    #[serde(skip)]
    pub bleedout_rounds_remaining: i32,

    // Pursuit state (for cross-room sniping response)
    #[serde(default)]
    pub pursuit_target_name: String,
    #[serde(default)]
    pub pursuit_target_room: Option<Uuid>,
    #[serde(default)]
    pub pursuit_direction: String,
    #[serde(default)]
    pub pursuit_certain: bool,

    // Arrow recovery: projectile vnums embedded in this mobile
    #[serde(default)]
    pub embedded_projectiles: Vec<String>,

    // Daily routine system
    #[serde(default)]
    pub daily_routine: Vec<RoutineEntry>,
    #[serde(default)]
    pub schedule_visible: bool,
    #[serde(default)]
    pub current_activity: ActivityState,
    #[serde(default)]
    pub routine_destination_room: Option<Uuid>,
    // Stealth detection (0-10, builder-set)
    #[serde(default)]
    pub perception: i32,

    // NPC needs simulation system
    /// Simulation config - if Some, this mobile uses needs-based simulation instead of routines
    #[serde(default)]
    pub simulation: Option<SimulationConfig>,
    /// Runtime needs state (initialized on first simulation tick)
    #[serde(default)]
    pub needs: Option<NeedsState>,

    // Migrant / emergent population system
    /// Visual/physical characteristics (populated for generated migrants; optional for others).
    #[serde(default)]
    pub characteristics: Option<Characteristics>,
    /// Optional household grouping (reserved for future family/partner mechanics).
    #[serde(default)]
    pub household_id: Option<Uuid>,
    /// Free-form ally tag for the helper system. None/empty falls back to
    /// Circle-stock semantics (any NPC defends any other NPC against a PC).
    /// A tagged faction opts the mob *out* of that generic pool — only
    /// matching tags ally.
    #[serde(default)]
    pub faction: Option<String>,
    /// Declared relationships to other mobiles (future: families, partners, rivals).
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    /// Room vnum this mobile occupies as a claimed resident. Distinct from
    /// SimulationConfig.home_room_vnum so non-simulation mobiles can be housed too.
    #[serde(default)]
    pub resident_of: Option<String>,

    /// Social preferences + happiness. Populated for simulated migrants; None for
    /// static/prototype mobiles.
    #[serde(default)]
    pub social: Option<SocialState>,
    /// Time-limited buffs/debuffs applied to this mobile (mood, etc).
    #[serde(default)]
    pub active_buffs: Vec<ActiveBuff>,
    /// True when this (juvenile) mobile has lost its last living parent and
    /// is awaiting adoption. Set by `db::delete_mobile`; cleared by the
    /// adoption pass in the aging tick. Non-juveniles are never flagged.
    #[serde(default)]
    pub adoption_pending: bool,
    /// Home zone for `MobileFlags.stay_zone`. Stamped at spawn-time from the
    /// destination room's `area_id` when first None.
    #[serde(default)]
    pub home_area_id: Option<Uuid>,
    /// PC names this mobile remembers as enemies (`MobileFlags.memory`).
    /// Capped at MEMORY_CAP, FIFO eviction; entries expire after
    /// MEMORY_DURATION_SECS.
    #[serde(default)]
    pub remembered_enemies: Vec<RememberedEnemy>,
    /// Charmed-mob "stay" override. When true the mob ignores the master
    /// follow propagation and stays put. Set by `order <mob> stay`,
    /// cleared by `order <mob> follow [...]`. Reset on charm break.
    #[serde(default)]
    pub charm_stay: bool,
    /// Charmed-mob alternative leader. When `Some(name)`, the mob follows
    /// that player instead of its charm master. None = follow master
    /// (the default). Set by `order <mob> follow <player>`. Reset on
    /// charm break.
    #[serde(default)]
    pub charm_follow_player: Option<String>,
}

fn default_mobile_hp() -> i32 {
    10
}

fn default_mobile_stamina() -> i32 {
    50
}

fn default_shop_buy_rate() -> i32 {
    50
}

fn default_shop_sell_rate() -> i32 {
    150
}

fn default_shop_buys_types() -> Vec<String> {
    vec!["all".to_string()]
}

fn default_healing_cost_multiplier() -> i32 {
    100 // Base price, no markup
}

fn default_vending_sell_rate() -> i32 {
    150
}

impl MobileData {
    pub fn new(name: String) -> Self {
        MobileData {
            id: Uuid::new_v4(),
            name: name.clone(),
            short_desc: format!("{} is here.", name),
            long_desc: String::new(),
            keywords: Vec::new(),
            current_room_id: None,
            is_prototype: true,
            vnum: String::new(),
            world_max_count: None,
            level: 1,
            max_hp: 10,
            current_hp: 10,
            max_stamina: 50,
            current_stamina: 50,
            damage_dice: "1d4".to_string(),
            damage_type: DamageType::default(),
            armor_class: 10,
            hit_modifier: 0,
            stat_str: 10,
            stat_dex: 10,
            stat_con: 10,
            stat_int: 10,
            stat_wis: 10,
            stat_cha: 10,
            flags: MobileFlags::default(),
            dialogue: HashMap::new(),
            shop_stock: Vec::new(),
            shop_inventory: Vec::new(),
            shop_buy_rate: 50,
            shop_sell_rate: 150,
            shop_buys_types: vec!["all".to_string()],
            shop_buys_categories: Vec::new(),
            shop_preset_vnum: String::new(),
            shop_extra_types: Vec::new(),
            shop_extra_categories: Vec::new(),
            shop_deny_types: Vec::new(),
            shop_deny_categories: Vec::new(),
            shop_min_value: 0,
            shop_max_value: 0,
            healer_type: String::new(),
            healing_free: false,
            healing_cost_multiplier: 100,
            triggers: Vec::new(),
            transport_route: None,
            property_templates: Vec::new(),
            leasing_area_id: None,
            gold: 0,
            combat: CombatState::default(),
            wounds: Vec::new(),
            ongoing_effects: Vec::new(),
            scars: HashMap::new(),
            // Death/unconscious state (not persisted)
            is_unconscious: false,
            bleedout_rounds_remaining: 0,
            // Pursuit state
            pursuit_target_name: String::new(),
            pursuit_target_room: None,
            pursuit_direction: String::new(),
            pursuit_certain: false,
            // Arrow recovery
            embedded_projectiles: Vec::new(),
            // Daily routine system
            daily_routine: Vec::new(),
            schedule_visible: false,
            current_activity: ActivityState::default(),
            routine_destination_room: None,
            perception: 0,
            simulation: None,
            needs: None,
            characteristics: None,
            household_id: None,
            faction: None,
            relationships: Vec::new(),
            resident_of: None,
            social: None,
            active_buffs: Vec::new(),
            adoption_pending: false,
            home_area_id: None,
            remembered_enemies: Vec::new(),
            charm_stay: false,
            charm_follow_player: None,
        }
    }

    pub fn charm_master(&self) -> Option<&str> {
        self.active_buffs
            .iter()
            .find(|b| b.effect_type == EffectType::Charmed)
            .map(|b| b.source.as_str())
    }

    pub fn is_charmed_by(&self, player_name: &str) -> bool {
        self.charm_master()
            .map(|m| m.eq_ignore_ascii_case(player_name))
            .unwrap_or(false)
    }

    pub fn is_charmed_by_anyone(&self) -> bool {
        self.charm_master().is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMeta {
    pub access: String, // "guest", "any", "user", "admin"
    pub description: String,
}

/// State for an active fishing session
#[derive(Debug, Clone)]
pub struct FishingState {
    pub started_at: i64,            // When cast started (unix timestamp)
    pub bite_time: i64,             // When fish will bite (unix timestamp)
    pub rod_item_id: Uuid,          // The fishing rod being used
    pub bait_item_id: Option<Uuid>, // Optional bait being used
    pub room_id: Uuid,              // Room where fishing started
    pub bite_notified: bool,        // Whether bite notification was sent
    pub warning_notified: bool,     // Whether warning notification was sent
}

// === Property Rental System ===

/// Access level for party members visiting a player's property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyAccessLevel {
    #[default]
    None, // No party access
    VisitOnly,  // Can enter and look
    FullAccess, // Can use amenities, take items
}

impl PartyAccessLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(PartyAccessLevel::None),
            "visit" | "visit_only" | "visitonly" => Some(PartyAccessLevel::VisitOnly),
            "full" | "full_access" | "fullaccess" => Some(PartyAccessLevel::FullAccess),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            PartyAccessLevel::None => "none",
            PartyAccessLevel::VisitOnly => "visit only",
            PartyAccessLevel::FullAccess => "full access",
        }
    }
}

/// A reusable buy configuration for shopkeepers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopPreset {
    pub id: Uuid,
    pub vnum: String, // e.g., "weapons_dealer"
    pub name: String, // "Weapons Dealer"
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub buy_types: Vec<String>, // Item types: "weapon", "armor", etc.
    #[serde(default)]
    pub buy_categories: Vec<String>, // Item categories: "leather", "herbs", etc.
    #[serde(default)]
    pub min_value: i32, // 0 = no minimum
    #[serde(default)]
    pub max_value: i32, // 0 = no maximum
}

impl ShopPreset {
    pub fn new(vnum: String, name: String) -> Self {
        ShopPreset {
            id: Uuid::new_v4(),
            vnum,
            name,
            description: String::new(),
            buy_types: Vec::new(),
            buy_categories: Vec::new(),
            min_value: 0,
            max_value: 0,
        }
    }
}

/// A builder-defined blueprint for rentable properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyTemplate {
    pub id: Uuid,
    pub vnum: String,           // e.g., "cottage_small"
    pub name: String,           // "Small Cottage"
    pub description: String,    // Shown when listing properties
    pub monthly_rent: i32,      // Gold per game month
    pub entrance_room_id: Uuid, // Template entrance room
    #[serde(default)]
    pub max_instances: i32, // 0 = unlimited
    #[serde(default)]
    pub level_requirement: i32, // Minimum level to rent
    #[serde(default)]
    pub area_id: Option<Uuid>, // Which area this template belongs to
}

impl PropertyTemplate {
    pub fn new(vnum: String, name: String) -> Self {
        PropertyTemplate {
            id: Uuid::new_v4(),
            vnum,
            name,
            description: String::new(),
            monthly_rent: 0,
            entrance_room_id: Uuid::nil(),
            max_instances: 0,
            level_requirement: 0,
            area_id: None,
        }
    }
}

/// An active rental agreement between a player and a property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseData {
    pub id: Uuid,
    pub template_vnum: String,        // Which PropertyTemplate
    pub owner_name: String,           // Character name who rented
    pub leasing_agent_id: Uuid,       // Mobile who leased this
    pub leasing_office_room_id: Uuid, // Room to return to via "out"
    pub area_id: Uuid,                // Area where lease is active
    pub instanced_rooms: Vec<Uuid>,   // Actual room UUIDs created
    pub entrance_room_id: Uuid,       // Player's entrance room
    pub monthly_rent: i32,            // Locked rent amount
    pub rent_paid_until: i64,         // Unix timestamp
    pub created_at: i64,              // When lease started
    #[serde(default)]
    pub is_evicted: bool, // Ended due to non-payment
    #[serde(default)]
    pub eviction_time: Option<i64>, // When eviction occurred
    #[serde(default)]
    pub party_access: PartyAccessLevel, // Access for grouped players
    #[serde(default)]
    pub trusted_visitors: Vec<String>, // Names with full access
}

impl LeaseData {
    pub fn new(
        template_vnum: String,
        owner_name: String,
        leasing_agent_id: Uuid,
        leasing_office_room_id: Uuid,
        area_id: Uuid,
        monthly_rent: i32,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        LeaseData {
            id: Uuid::new_v4(),
            template_vnum,
            owner_name,
            leasing_agent_id,
            leasing_office_room_id,
            area_id,
            instanced_rooms: Vec::new(),
            entrance_room_id: Uuid::nil(),
            monthly_rent,
            rent_paid_until: now,
            created_at: now,
            is_evicted: false,
            eviction_time: None,
            party_access: PartyAccessLevel::None,
            trusted_visitors: Vec::new(),
        }
    }
}

// === Mail System ===

/// A mail message between players
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: Uuid,
    pub sender: String,
    pub recipient: String, // lowercase for lookup
    pub body: String,
    pub sent_at: i64, // Unix timestamp
    pub read: bool,
}

impl MailMessage {
    pub fn new(sender: String, recipient: String, body: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        MailMessage {
            id: Uuid::new_v4(),
            sender,
            recipient: recipient.to_lowercase(),
            body,
            sent_at: now,
            read: false,
        }
    }
}

// === Bug Reporting System ===

/// Status of a bug report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BugStatus {
    Open,
    InProgress,
    Resolved,
    Closed,
}

impl Default for BugStatus {
    fn default() -> Self {
        BugStatus::Open
    }
}

impl BugStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" => Some(BugStatus::Open),
            "inprogress" | "in_progress" | "in-progress" => Some(BugStatus::InProgress),
            "resolved" => Some(BugStatus::Resolved),
            "closed" => Some(BugStatus::Closed),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &str {
        match self {
            BugStatus::Open => "Open",
            BugStatus::InProgress => "InProgress",
            BugStatus::Resolved => "Resolved",
            BugStatus::Closed => "Closed",
        }
    }
}

/// Priority of a bug report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BugPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for BugPriority {
    fn default() -> Self {
        BugPriority::Normal
    }
}

impl BugPriority {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(BugPriority::Low),
            "normal" => Some(BugPriority::Normal),
            "high" => Some(BugPriority::High),
            "critical" => Some(BugPriority::Critical),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &str {
        match self {
            BugPriority::Low => "Low",
            BugPriority::Normal => "Normal",
            BugPriority::High => "High",
            BugPriority::Critical => "Critical",
        }
    }
}

/// Auto-captured game state at the time of a bug report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugContext {
    #[serde(default)]
    pub room_id: String,
    #[serde(default)]
    pub room_vnum: String,
    #[serde(default)]
    pub room_title: String,
    #[serde(default)]
    pub character_level: i32,
    #[serde(default)]
    pub character_class: String,
    #[serde(default)]
    pub character_race: String,
    #[serde(default)]
    pub character_position: String,
    #[serde(default)]
    pub hp: i32,
    #[serde(default)]
    pub max_hp: i32,
    #[serde(default)]
    pub mana: i32,
    #[serde(default)]
    pub max_mana: i32,
    #[serde(default)]
    pub in_combat: bool,
    #[serde(default)]
    pub game_time: String,
    #[serde(default)]
    pub season: String,
    #[serde(default)]
    pub weather: String,
    #[serde(default)]
    pub players_in_room: Vec<String>,
    #[serde(default)]
    pub mobiles_in_room: Vec<String>,
}

impl Default for BugContext {
    fn default() -> Self {
        BugContext {
            room_id: String::new(),
            room_vnum: String::new(),
            room_title: String::new(),
            character_level: 0,
            character_class: String::new(),
            character_race: String::new(),
            character_position: String::new(),
            hp: 0,
            max_hp: 0,
            mana: 0,
            max_mana: 0,
            in_combat: false,
            game_time: String::new(),
            season: String::new(),
            weather: String::new(),
            players_in_room: Vec::new(),
            mobiles_in_room: Vec::new(),
        }
    }
}

/// An admin note on a bug report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminNote {
    pub author: String,
    pub message: String,
    pub created_at: i64,
}

/// A bug report submitted by a player
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReport {
    pub id: Uuid,
    #[serde(default)]
    pub ticket_number: i64,
    pub reporter: String,
    pub description: String,
    #[serde(default)]
    pub status: BugStatus,
    #[serde(default)]
    pub priority: BugPriority,
    #[serde(default)]
    pub approved: bool,
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub resolved_at: Option<i64>,
    #[serde(default)]
    pub resolved_by: Option<String>,
    #[serde(default)]
    pub admin_notes: Vec<AdminNote>,
    #[serde(default)]
    pub context: BugContext,
}

/// Storage for items from evicted or ended leases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowData {
    pub id: Uuid,
    pub owner_name: String,    // Character who owned items
    pub items: Vec<Uuid>,      // Item IDs held in escrow
    pub source_lease_id: Uuid, // Original lease
    pub created_at: i64,       // When escrow started
    pub expires_at: i64,       // When items get deleted
    pub retrieval_fee: i32,    // Gold fee to retrieve
    #[serde(default)]
    pub destination_lease_id: Option<Uuid>, // Property to ship items to
}

impl EscrowData {
    pub fn new(
        owner_name: String,
        items: Vec<Uuid>,
        source_lease_id: Uuid,
        expires_days: i64,
        retrieval_fee: i32,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        EscrowData {
            id: Uuid::new_v4(),
            owner_name,
            items,
            source_lease_id,
            created_at: now,
            expires_at: now + (expires_days * 24 * 60 * 60),
            retrieval_fee,
            destination_lease_id: None,
        }
    }
}

// === API Key System ===

/// Permissions for an API key
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiPermissions {
    /// Can read data
    #[serde(default)]
    pub read: bool,
    /// Can modify data
    #[serde(default)]
    pub write: bool,
    /// Bypass area permission checks
    #[serde(default)]
    pub admin: bool,
}

/// API key for REST API authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    /// Argon2 hash of the key
    pub key_hash: String,
    /// Human-readable name
    pub name: String,
    /// Character name for permission checks
    pub owner_character: String,
    /// Permissions granted to this key
    #[serde(default)]
    pub permissions: ApiPermissions,
    /// Unix timestamp when key was created
    pub created_at: i64,
    /// Unix timestamp when key was last used
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Whether the key is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}
