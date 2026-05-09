//! Spawn-point and equipment-slot types: how entities respawn, the
//! per-spawn dependency manifest (extra items + where they go), and the
//! `WearLocation` enum referenced by both spawn destinations and item
//! equipment.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    /// When true, the respawn tick force-despawns existing tracked instances
    /// at each respawn cycle so the next spawn applies fresh dependencies.
    /// Mirrors Ranvier's `replaceOnRespawn` semantic — useful for chests
    /// whose contents should be fully refreshed rather than topped up.
    #[serde(default)]
    pub replace_on_respawn: bool,
}

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
