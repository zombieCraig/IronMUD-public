//! Trigger types for rooms, items, and mobiles

use serde::{Deserialize, Serialize};

// === Room Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    OnEnter,
    OnExit,
    OnLook,
    Periodic,
    OnTimeChange,    // Fires when time of day changes (dawn, dusk, etc.)
    OnWeatherChange, // Fires when weather changes (rain starts, clears up, etc.)
    OnSeasonChange,  // Fires when season changes (spring, summer, autumn, winter)
    OnMonthChange,   // Fires when month changes (1-12)
}

impl Default for TriggerType {
    fn default() -> Self {
        TriggerType::OnEnter
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTrigger {
    pub trigger_type: TriggerType,
    pub script_name: String,      // e.g., "forest_trap" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_interval")]
    pub interval_secs: i64,       // For periodic triggers (default 60)
    #[serde(default)]
    pub last_fired: i64,          // Unix timestamp of last execution
    #[serde(default = "default_trigger_chance")]
    pub chance: i32,              // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>,        // Template arguments
}

fn default_trigger_interval() -> i64 {
    60
}

fn default_trigger_chance() -> i32 {
    100
}

impl Default for RoomTrigger {
    fn default() -> Self {
        RoomTrigger {
            trigger_type: TriggerType::OnEnter,
            script_name: String::new(),
            enabled: true,
            interval_secs: 60,
            last_fired: 0,
            chance: 100,
            args: Vec::new(),
        }
    }
}

// === Item Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemTriggerType {
    OnGet,
    OnDrop,
    OnUse,
    OnExamine,
    OnPrompt,  // Fires when building prompt for equipped items
}

impl Default for ItemTriggerType {
    fn default() -> Self {
        ItemTriggerType::OnGet
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTrigger {
    pub trigger_type: ItemTriggerType,
    pub script_name: String,      // e.g., "cursed_item" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_chance")]
    pub chance: i32,              // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>,        // Template arguments
}

impl Default for ItemTrigger {
    fn default() -> Self {
        ItemTrigger {
            trigger_type: ItemTriggerType::OnGet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
        }
    }
}

// === Mobile/NPC Trigger System ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileTriggerType {
    OnGreet,    // Player enters room with NPC
    OnAttack,   // NPC is attacked
    OnDeath,    // NPC dies
    OnSay,      // Player says something in room (for advanced dialogue)
    OnIdle,     // Periodic when players present in room
    OnAlways,   // Periodic regardless of player presence
    OnFlee,     // Mobile flees from combat
}

impl Default for MobileTriggerType {
    fn default() -> Self {
        MobileTriggerType::OnGreet
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileTrigger {
    pub trigger_type: MobileTriggerType,
    pub script_name: String,      // e.g., "guard_greet" or "@say_greeting" for templates
    pub enabled: bool,
    #[serde(default = "default_trigger_chance")]
    pub chance: i32,              // 1-100 percent (100 = always)
    #[serde(default)]
    pub args: Vec<String>,        // Template arguments (e.g., greeting message)
    #[serde(default = "default_trigger_interval")]
    pub interval_secs: i64,       // For OnIdle triggers (default 60)
    #[serde(default)]
    pub last_fired: i64,          // Unix timestamp of last execution
}

impl Default for MobileTrigger {
    fn default() -> Self {
        MobileTrigger {
            trigger_type: MobileTriggerType::OnGreet,
            script_name: String::new(),
            enabled: true,
            chance: 100,
            args: Vec::new(),
            interval_secs: 60,
            last_fired: 0,
        }
    }
}
