//! NPC needs / daily-routine simulation types.
//!
//! Covers the data model for sim-driven mobiles: their daily routine
//! (`ActivityState`, `RoutineEntry`), what they're currently chasing
//! (`SimGoal`), their runtime needs (`NeedsState`), and builder-authored
//! configuration (`SimulationConfig`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
