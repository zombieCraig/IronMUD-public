//! Transportation system types: vehicles, schedules, stops, and NPC travel routes.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
