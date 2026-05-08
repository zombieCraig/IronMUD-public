//! Bug-reporting system types: bug status, priority, captured context,
//! admin notes, and the report itself.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
