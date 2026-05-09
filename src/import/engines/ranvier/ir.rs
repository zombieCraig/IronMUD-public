//! Ranvier-native IR types. These mirror the on-disk YAML shapes 1:1 so
//! the parser is a thin `serde_yaml::from_str` and the mapper has all the
//! fields it needs without re-parsing strings.
//!
//! Optional fields default — a Ranvier bundle may omit any of `rooms.yml`
//! / `npcs.yml` / `items.yml` / `quests.yml` / `loot-pools.yml`, and within
//! each list entry many fields are optional.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct IrBundle {
    pub areas: Vec<IrArea>,
}

#[derive(Debug, Clone)]
pub struct IrArea {
    /// Directory name under `<bundle>/areas/`. Used as the IronMUD area
    /// prefix and as the namespace component for `<area>:<id>` refs.
    pub name: String,
    pub manifest: IrManifest,
    pub rooms: Vec<IrRoom>,
    pub npcs: Vec<IrNpc>,
    pub items: Vec<IrItem>,
    pub quests: Vec<IrQuest>,
    pub loot_pools: HashMap<String, Vec<IrLootEntry>>,
    /// Source path to the area directory — used for warning provenance.
    pub source_dir: PathBuf,
    /// Names found under `<area>/scripts/` (rooms / npcs / items
    /// subdirs). Surfaced as `script_skipped` warnings during mapping.
    pub script_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IrManifest {
    #[serde(default)]
    pub title: String,
    /// Per-area respawn cadence (Ranvier `respawnInterval`, seconds).
    /// Falls back to 60s during mapping when absent.
    #[serde(default)]
    pub respawn_interval: Option<i64>,
    /// `behaviors.progressive-respawn.interval` is the only manifest
    /// behavior used in stock starter bundles. We capture the raw map
    /// and dig into it during mapping rather than typing it out.
    #[serde(default)]
    pub behaviors: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrRoom {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub npcs: Vec<IrSpawnRef>,
    #[serde(default)]
    pub items: Vec<IrSpawnRef>,
    #[serde(default)]
    pub exits: Vec<IrExit>,
    /// `<other_room_id>: { closed?, locked?, lockedBy? }`. Resolved against
    /// the matching exit during mapping.
    #[serde(default)]
    pub doors: HashMap<String, IrDoor>,
    #[serde(default)]
    pub coordinates: Option<[i32; 3]>,
    #[serde(default)]
    pub metadata: serde_yaml::Value,
}

/// Either a bare string id (`"limbo:rat"`) or a struct with respawn knobs.
#[derive(Debug, Clone)]
pub struct IrSpawnRef {
    pub id: String,
    pub respawn_chance: Option<i32>,
    pub max_load: Option<i32>,
    pub replace_on_respawn: bool,
}

impl<'de> Deserialize<'de> for IrSpawnRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Bare(String),
            Full {
                id: String,
                #[serde(default, rename = "respawnChance")]
                respawn_chance: Option<i32>,
                #[serde(default, rename = "maxLoad")]
                max_load: Option<i32>,
                #[serde(default, rename = "replaceOnRespawn")]
                replace_on_respawn: bool,
            },
        }
        Ok(match Form::deserialize(d)? {
            Form::Bare(id) => Self {
                id,
                respawn_chance: None,
                max_load: None,
                replace_on_respawn: false,
            },
            Form::Full {
                id,
                respawn_chance,
                max_load,
                replace_on_respawn,
            } => Self {
                id,
                respawn_chance,
                max_load,
                replace_on_respawn,
            },
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrExit {
    #[serde(rename = "roomId")]
    pub room_id: String,
    pub direction: String,
    #[serde(default, rename = "leaveMessage")]
    pub leave_message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IrDoor {
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default, rename = "lockedBy")]
    pub locked_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrNpc {
    pub id: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub quests: Vec<String>,
    #[serde(default)]
    pub attributes: HashMap<String, i32>,
    #[serde(default)]
    pub behaviors: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub metadata: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrItem {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub item_type: Option<String>,
    #[serde(default, rename = "roomDesc")]
    pub room_desc: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default, rename = "lockedBy")]
    pub locked_by: Option<String>,
    #[serde(default, rename = "maxItems")]
    pub max_items: Option<i32>,
    #[serde(default)]
    pub behaviors: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub metadata: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrQuest {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub level: i32,
    #[serde(default, rename = "autoComplete")]
    pub auto_complete: bool,
    #[serde(default)]
    pub repeatable: bool,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "completionMessage")]
    pub completion_message: String,
    #[serde(default)]
    pub goals: Vec<IrQuestGoal>,
    #[serde(default)]
    pub rewards: Vec<IrQuestReward>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrQuestGoal {
    #[serde(rename = "type")]
    pub goal_type: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IrQuestReward {
    #[serde(rename = "type")]
    pub reward_type: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

/// One entry in a `loot-pools.yml` table. The Ranvier on-disk shape is a
/// list of `{<itemId>: <weight>}` single-key maps, occasionally a bare id.
#[derive(Debug, Clone)]
pub struct IrLootEntry {
    pub item_id: String,
    pub weight: i32,
}

impl<'de> Deserialize<'de> for IrLootEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Bare(String),
            Map(HashMap<String, i32>),
        }
        match Form::deserialize(d)? {
            Form::Bare(item_id) => Ok(Self { item_id, weight: 1 }),
            Form::Map(m) => {
                let mut iter = m.into_iter();
                let (item_id, weight) = iter
                    .next()
                    .ok_or_else(|| serde::de::Error::custom("loot pool entry missing item:weight"))?;
                Ok(Self { item_id, weight })
            }
        }
    }
}
