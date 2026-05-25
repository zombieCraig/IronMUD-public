//! Character (player) types: the persistent `CharacterData` aggregate and
//! its supporting position/command/fishing/property-access helpers.

use super::serde_defaults::default_stat;
use super::{
    AchievementUnlock, ActiveBuff, ActiveQuest, BodyPart, CombatState, DialoguePairState,
    ItemAffect, OngoingEffect, SkillProgress, WearLocation, Wound,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A permanent body mark applied via `apply <tattoo_item>`. The source item
/// is consumed on apply; this record persists on the character and its
/// `affects` are re-stamped as `ActiveBuff`s with source
/// `"tattoo:<vnum>:<location>"` (where `<vnum>` is `"-"` if unknown).
/// Visible to others via examine (short_desc grouped by location); the
/// wearer can `examine <keyword>` to see long_desc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterTattoo {
    pub location: WearLocation,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub short_desc: String,
    #[serde(default)]
    pub long_desc: String,
    #[serde(default)]
    pub source_vnum: Option<String>,
    #[serde(default)]
    pub affects: Vec<ItemAffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    pub name: String,
    pub password_hash: String,
    pub current_room_id: Uuid,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    /// DG Scripts per-character persistent vars. Set via `set name value` /
    /// `global name` from a trigger body, queried via `%actor.<name>%` and
    /// `%actor.varexists(<name>)%`. Empty for fresh characters; persists
    /// across logins by virtue of being on `CharacterData`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dg_vars: HashMap<String, String>,
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
    /// Pronoun gender for DG Scripts %actor.heshe%/%hisher%/%himher%/sex/gender.
    /// Empty / unrecognised resolves as "neuter" (it/its).
    #[serde(default)]
    pub gender: String,
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
    /// Cumulative seconds the character has been played, across all sessions.
    /// Incremented on quit and disconnect-timeout when a session-start stamp
    /// is in scope. Not retroactive: pre-feature characters start at 0.
    #[serde(default)]
    pub total_seconds_played: i64,
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
    // Social lists
    #[serde(default)]
    pub friends: Vec<String>,
    #[serde(default)]
    pub ignored: Vec<String>,
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
    // Clan membership mirror. None = no clan. Authoritative roster lives on
    // ClanData.members; this field exists so `who` and gate checks don't have
    // to scan every clan.
    #[serde(default)]
    pub clan_tag: Option<String>,
    // Active language for say/tell/whisper/shout. Defaults to "common".
    #[serde(default = "default_language")]
    pub current_language: String,
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
    // Morality slider (clamp -200..=+200; tier thresholds at +/-100; everyone starts at 0).
    // No class sets this; only explicit Rhai/DG script calls (kills, quests, dialogue) shift it.
    #[serde(default)]
    pub morality: i32,
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
    #[serde(default)]
    pub tattoos: Vec<CharacterTattoo>,
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
    /// Consent flag: when true, vampire PCs may `feed` on this mortal
    /// without taking a humanity hit. Default off — feeding on a
    /// non-consenting mortal carries the moral cost.
    #[serde(default)]
    pub bloodfeed_willing: bool,
    /// Beta opt-in for the modern full-screen multi-line editor.
    /// When off, OLC text composition uses the legacy line-oriented editor.
    #[serde(default)]
    pub new_editor_enabled: bool,
    /// Vampirism state. None = mortal (default for nearly every character).
    /// Some = kindred. Stamped by the embrace flow (admin / quest / class
    /// creation). See `crate::types::VampireState`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vampire_state: Option<crate::types::VampireState>,
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
    // Per-spell mastery (level + XP keyed by spell ID). Independent of the
    // unified `magic` skill — see SpellProgress.
    #[serde(default)]
    pub spell_progress: HashMap<String, super::definitions::SpellProgress>,
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
    // Achievement system fields
    #[serde(default)]
    pub achievement_counters: HashMap<String, u32>,
    #[serde(default)]
    pub achievements_unlocked: HashMap<String, AchievementUnlock>,
    /// Builder-defined custom skill values (key → integer). Keys must be
    /// registered via `lookup skill publish`. Missing key reads as 0.
    #[serde(default)]
    pub custom_skills: HashMap<String, i32>,
    // Dialogue tree state: per-mob-vnum conversation cursor
    #[serde(default)]
    pub dialogue_pair_state: HashMap<String, DialoguePairState>,
    // Dialogue flags: "<vnum>:<name>" for FlagScope::Local, "<name>" for Global
    #[serde(default)]
    pub dialogue_flags: HashMap<String, bool>,
    #[serde(default)]
    pub active_title: Option<String>,
    /// Highest gold balance the character has ever held; used to fire
    /// `gold_high_water` achievement events without re-firing on dips.
    #[serde(default)]
    pub gold_high_water: i32,
    // Map system: rooms the player has entered (drives fog-of-war).
    #[serde(default)]
    pub rooms_visited: std::collections::HashSet<Uuid>,
    // Map system: prepend ASCII map to every room display (look/move/login).
    // Default off — opt-in via `set automap on`. The map is screen-real-estate
    // heavy and many players prefer to invoke `map` on demand.
    #[serde(default)]
    pub automap_enabled: bool,
    #[serde(default = "default_automap_radius")]
    pub automap_radius: i32,
    // Fallback to ASCII connectors/arrows for restricted clients.
    #[serde(default)]
    pub ascii_map: bool,
    // Quest system: per-player active and completed quests. Quest prototypes
    // live in the `quests` sled tree; player progress rides on the character.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub active_quests: HashMap<String, ActiveQuest>,
    #[serde(default, skip_serializing_if = "std::collections::HashSet::is_empty")]
    pub completed_quests: std::collections::HashSet<String>,
    /// In-flight slow-move (set when stepping into a delayed exit). The
    /// player is locked in `source_room_id` until `complete_at` (unix
    /// seconds). The slow-move tick reads this, injects a `go <direction>`
    /// input event when the timer fires, and the go handler honors the
    /// pending move by skipping the delay check exactly once.
    #[serde(default)]
    pub pending_slow_move: Option<PendingSlowMove>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingSlowMove {
    pub direction: String,
    pub source_room_id: Uuid,
    pub complete_at: i64,
}

fn default_automap_radius() -> i32 {
    crate::script::map::AUTOMAP_DEFAULT_RADIUS
}

fn default_level() -> i32 {
    1
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

fn default_language() -> String {
    "common".to_string()
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
pub struct CommandMeta {
    pub access: String, // "guest", "any", "user", "admin"
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<CommandRequirements>,
    /// Optional category tag. `Some("social")` marks a virtual social-command
    /// entry so `help` can hide it behind the unified `socials` listing while
    /// the prefix-matching dispatcher still resolves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Optional gating beyond `access`. A command with skill requirements is
/// hidden from the help listing when the viewer doesn't meet them. Builders
/// and admins are not exempt — gates are about ability, not permission.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRequirements {
    /// Map of skill key (lowercase, e.g. "thievery") -> minimum level.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skill: HashMap<String, i32>,
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
