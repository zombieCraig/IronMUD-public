//! Room types: the `RoomData` aggregate, exits, doors, fishing/foraging
//! tables, room flags, and the small structs for stealth/tracking and
//! traps that live inside a room.

use super::{CombatZoneType, RoomTrigger};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
    pub doors: HashMap<String, DoorState>,
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
    /// DG Scripts persistent vars (see MobileData.dg_vars).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dg_vars: HashMap<String, String>,
    /// Optional grid coordinates `(x, y, z)`. Filled by importers (Ranvier
    /// `coordinates: [x, y, z]`); future ASCII-map / spatial features can
    /// build on it.
    #[serde(default)]
    pub coordinates: Option<(i32, i32, i32)>,
    /// Builder-declared verbs the room exposes (e.g. `pull` for a lever
    /// puzzle). Surfaced via TAB completion and the `look` "Here you can:"
    /// line. Runtime dispatch is still handled by DG OnCommand triggers.
    #[serde(default)]
    pub contextual_commands: Vec<ContextualCommand>,
    /// Per-direction travel delay in seconds. When a player issues a move
    /// command for a direction present in this map, the move is deferred —
    /// the player is locked in this room for N seconds before arriving at
    /// the destination. Used for one-person tunnels, climbing, swimming
    /// across rivers, etc. Direction key matches `RoomExits` field names
    /// ("north", "east", ..., "out") or a custom-exit name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub exit_delays: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ContextualCommand {
    /// Single keyword, lowercased on insert. Matches DG OnCommand `args[0]`.
    pub verb: String,
    /// Short flavor displayed alongside the verb in `look`. None / empty =
    /// bare verb only.
    #[serde(default)]
    pub hint: Option<String>,
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
    pub custom: HashMap<String, Uuid>, // Custom exits like "elevator", "train", "portal"
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
