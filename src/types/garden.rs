//! Gardening system types for IronMUD
//!
//! Plant growth system where players find/buy seeds, plant them in dirt_floor
//! rooms (ground) or pots (anywhere), water and tend them over game-days,
//! and harvest food, herbs, or flowers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Season;

// === Growth Stages ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrowthStage {
    Seed,
    Sprout,
    Seedling,
    Growing,
    Mature,
    Flowering,
    Wilting,
    Dead,
}

impl Default for GrowthStage {
    fn default() -> Self {
        GrowthStage::Seed
    }
}

impl GrowthStage {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "seed" => Some(GrowthStage::Seed),
            "sprout" => Some(GrowthStage::Sprout),
            "seedling" => Some(GrowthStage::Seedling),
            "growing" => Some(GrowthStage::Growing),
            "mature" => Some(GrowthStage::Mature),
            "flowering" => Some(GrowthStage::Flowering),
            "wilting" => Some(GrowthStage::Wilting),
            "dead" => Some(GrowthStage::Dead),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            GrowthStage::Seed => "seed",
            GrowthStage::Sprout => "sprout",
            GrowthStage::Seedling => "seedling",
            GrowthStage::Growing => "growing",
            GrowthStage::Mature => "mature",
            GrowthStage::Flowering => "flowering",
            GrowthStage::Wilting => "wilting",
            GrowthStage::Dead => "dead",
        }
    }

    /// Returns the ordered list of living growth stages
    pub fn living_stages() -> &'static [GrowthStage] {
        &[
            GrowthStage::Seed,
            GrowthStage::Sprout,
            GrowthStage::Seedling,
            GrowthStage::Growing,
            GrowthStage::Mature,
            GrowthStage::Flowering,
        ]
    }

    /// Get the next stage in the growth sequence (None if Dead/Wilting)
    pub fn next(&self) -> Option<GrowthStage> {
        match self {
            GrowthStage::Seed => Some(GrowthStage::Sprout),
            GrowthStage::Sprout => Some(GrowthStage::Seedling),
            GrowthStage::Seedling => Some(GrowthStage::Growing),
            GrowthStage::Growing => Some(GrowthStage::Mature),
            GrowthStage::Mature => Some(GrowthStage::Flowering),
            GrowthStage::Flowering => None, // Must be harvested
            GrowthStage::Wilting => Some(GrowthStage::Dead),
            GrowthStage::Dead => None,
        }
    }

    /// All valid stage names for completion
    pub fn all_names() -> &'static [&'static str] {
        &[
            "seed",
            "sprout",
            "seedling",
            "growing",
            "mature",
            "flowering",
            "wilting",
            "dead",
        ]
    }
}

impl std::fmt::Display for GrowthStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// === Plant Categories ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlantCategory {
    Vegetable,
    Herb,
    Flower,
    Fruit,
    Grain,
}

impl Default for PlantCategory {
    fn default() -> Self {
        PlantCategory::Vegetable
    }
}

impl PlantCategory {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vegetable" => Some(PlantCategory::Vegetable),
            "herb" => Some(PlantCategory::Herb),
            "flower" => Some(PlantCategory::Flower),
            "fruit" => Some(PlantCategory::Fruit),
            "grain" => Some(PlantCategory::Grain),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            PlantCategory::Vegetable => "vegetable",
            PlantCategory::Herb => "herb",
            PlantCategory::Flower => "flower",
            PlantCategory::Fruit => "fruit",
            PlantCategory::Grain => "grain",
        }
    }

    pub fn all_names() -> &'static [&'static str] {
        &["vegetable", "herb", "flower", "fruit", "grain"]
    }
}

impl std::fmt::Display for PlantCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// === Infestation Types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfestationType {
    None,
    Aphids,
    Blight,
    RootRot,
    Frost,
}

impl Default for InfestationType {
    fn default() -> Self {
        InfestationType::None
    }
}

impl InfestationType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(InfestationType::None),
            "aphids" => Some(InfestationType::Aphids),
            "blight" => Some(InfestationType::Blight),
            "root_rot" | "rootrot" => Some(InfestationType::RootRot),
            "frost" => Some(InfestationType::Frost),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            InfestationType::None => "none",
            InfestationType::Aphids => "aphids",
            InfestationType::Blight => "blight",
            InfestationType::RootRot => "root_rot",
            InfestationType::Frost => "frost",
        }
    }

    pub fn all_names() -> &'static [&'static str] {
        &["none", "aphids", "blight", "root_rot", "frost"]
    }
}

impl std::fmt::Display for InfestationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

// === Growth Stage Definition (per-stage config in a prototype) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthStageDef {
    pub stage: GrowthStage,
    /// How long this stage lasts in game hours
    pub duration_game_hours: i64,
    /// Room display text when plant is at this stage
    pub description: String,
    /// Detail text shown on examine
    pub examine_desc: String,
}

// === Plant Prototype (species template) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlantPrototype {
    pub id: Uuid,
    #[serde(default)]
    pub vnum: Option<String>,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Item vnum of the seed used to plant this
    #[serde(default)]
    pub seed_vnum: String,
    /// Item vnum of the produce when harvested
    #[serde(default)]
    pub harvest_vnum: String,
    /// Minimum harvest yield (based on care quality)
    #[serde(default = "default_harvest_min")]
    pub harvest_min: i32,
    /// Maximum harvest yield
    #[serde(default = "default_harvest_max")]
    pub harvest_max: i32,
    #[serde(default)]
    pub category: PlantCategory,
    /// Per-stage configuration
    #[serde(default)]
    pub stages: Vec<GrowthStageDef>,
    /// Seasons where growth is boosted (x1.25)
    #[serde(default)]
    pub preferred_seasons: Vec<Season>,
    /// Seasons where growth is blocked (x0.0)
    #[serde(default)]
    pub forbidden_seasons: Vec<Season>,
    /// Water consumed per game hour
    #[serde(default = "default_water_consumption")]
    pub water_consumption_per_hour: f64,
    /// Maximum water the plant can hold
    #[serde(default = "default_water_capacity")]
    pub water_capacity: f64,
    /// Can only be planted in pots (indoors)
    #[serde(default)]
    pub indoor_only: bool,
    /// Minimum gardening skill level to plant
    #[serde(default)]
    pub min_skill_to_plant: i32,
    /// Base XP awarded on harvest
    #[serde(default = "default_base_xp")]
    pub base_xp: i32,
    /// Resistance to pests (0-100)
    #[serde(default = "default_pest_resistance")]
    pub pest_resistance: i32,
    /// If true, resets to Growing after harvest instead of dying
    #[serde(default)]
    pub multi_harvest: bool,
    /// Whether this is a prototype template (always true for prototypes)
    #[serde(default)]
    pub is_prototype: bool,
}

fn default_harvest_min() -> i32 {
    1
}
fn default_harvest_max() -> i32 {
    3
}
fn default_water_consumption() -> f64 {
    1.0
}
fn default_water_capacity() -> f64 {
    100.0
}
fn default_base_xp() -> i32 {
    10
}
fn default_pest_resistance() -> i32 {
    30
}

impl PlantPrototype {
    pub fn new(name: String, vnum: String) -> Self {
        PlantPrototype {
            id: Uuid::new_v4(),
            vnum: Some(vnum),
            name,
            keywords: Vec::new(),
            seed_vnum: String::new(),
            harvest_vnum: String::new(),
            harvest_min: default_harvest_min(),
            harvest_max: default_harvest_max(),
            category: PlantCategory::default(),
            stages: Vec::new(),
            preferred_seasons: Vec::new(),
            forbidden_seasons: Vec::new(),
            water_consumption_per_hour: default_water_consumption(),
            water_capacity: default_water_capacity(),
            indoor_only: false,
            min_skill_to_plant: 0,
            base_xp: default_base_xp(),
            pest_resistance: default_pest_resistance(),
            multi_harvest: false,
            is_prototype: true,
        }
    }

    /// Get the stage definition for a given growth stage
    pub fn get_stage_def(&self, stage: &GrowthStage) -> Option<&GrowthStageDef> {
        self.stages.iter().find(|s| s.stage == *stage)
    }
}

// === Plant Instance (a specific planted plant) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlantInstance {
    pub id: Uuid,
    /// Vnum of the prototype this plant was created from
    pub prototype_vnum: String,
    /// Room where this plant is located
    pub room_id: Uuid,
    /// Name of the player who planted it
    #[serde(default)]
    pub planter_name: String,
    /// Group members who can also harvest
    #[serde(default)]
    pub group_members: Vec<String>,
    /// Current growth stage
    #[serde(default)]
    pub stage: GrowthStage,
    /// Hours of progress accumulated in current stage
    #[serde(default)]
    pub stage_progress_hours: f64,
    /// Water level (0-100)
    #[serde(default = "default_water_level")]
    pub water_level: f64,
    /// Plant health (0-100)
    #[serde(default = "default_health")]
    pub health: f64,
    /// Whether fertilizer is active
    #[serde(default)]
    pub fertilized: bool,
    /// Remaining hours of fertilizer effect
    #[serde(default)]
    pub fertilizer_hours_remaining: f64,
    /// Current infestation type
    #[serde(default)]
    pub infestation: InfestationType,
    /// Infestation severity (0.0-1.0)
    #[serde(default)]
    pub infestation_severity: f64,
    /// Whether this plant is in a pot
    #[serde(default)]
    pub is_potted: bool,
    /// UUID of the pot item (if potted)
    #[serde(default)]
    pub pot_item_id: Option<Uuid>,
    /// How many times this plant has been harvested (for multi-harvest)
    #[serde(default)]
    pub times_harvested: i32,
    /// Real-world Unix timestamp of last tick update (key to offline growth)
    #[serde(default)]
    pub last_update_timestamp: i64,
    /// Real-world Unix timestamp when planted
    #[serde(default)]
    pub planted_at: i64,
    /// Game month when planted
    #[serde(default)]
    pub planted_game_month: u8,
    /// Game year when planted
    #[serde(default)]
    pub planted_game_year: u32,
}

fn default_water_level() -> f64 {
    50.0
}
fn default_health() -> f64 {
    100.0
}

impl PlantInstance {
    pub fn new(
        prototype_vnum: String,
        room_id: Uuid,
        planter_name: String,
        is_potted: bool,
        pot_item_id: Option<Uuid>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        PlantInstance {
            id: Uuid::new_v4(),
            prototype_vnum,
            room_id,
            planter_name,
            group_members: Vec::new(),
            stage: GrowthStage::Seed,
            stage_progress_hours: 0.0,
            water_level: default_water_level(),
            health: default_health(),
            fertilized: false,
            fertilizer_hours_remaining: 0.0,
            infestation: InfestationType::None,
            infestation_severity: 0.0,
            is_potted,
            pot_item_id,
            times_harvested: 0,
            last_update_timestamp: now,
            planted_at: now,
            planted_game_month: 0,
            planted_game_year: 0,
        }
    }
}
