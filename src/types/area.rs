//! Area-level types: builder permissions, area-wide flags, immigration
//! configuration, and the `AreaData` aggregate itself.

use super::{CombatZoneType, ClimateProfile, ForageEntry, RoomFlags, SimulationConfig};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    /// Climate preset filtering globally-rolled weather into a locally-permitted
    /// condition (e.g. Tropical converts snow to rain) and shifting effective
    /// temperature. Defaults to Temperate, which preserves global behavior.
    #[serde(default)]
    pub climate: ClimateProfile,

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

    /// Optional vnum of a room in this area that accepts player donations
    /// (`donate <item>`). `None` = donations refused with "Donations are not
    /// accepted here." Items teleported here decay after `donation_decay_secs`
    /// (default 1800) via the donation-decay tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub donation_room_vnum: Option<String>,
    /// Hourly scavenging wage for migrant scavengers, paid only while not at
    /// their home room (they have to actually be out scrounging). Default 0.
    #[serde(default)]
    pub scavenger_wage_per_hour: i32,

    /// Soft caps on the number of prototypes/rooms attributed to this area.
    /// `None` = unlimited (default). Enforced at create-time at the API and
    /// OLC boundaries; existing entities are never retroactively deleted if
    /// the cap is later set lower than the current count. Mainly a guard
    /// against runaway create loops by a hostile builder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rooms: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mobiles: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_spawn_points: Option<i32>,
}
