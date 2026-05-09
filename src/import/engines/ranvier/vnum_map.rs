//! Stable `<area>:<kind>:<id>` ↔ vnum index, persisted to `imports/<bundle>.vnum-map.json`.
//!
//! Each area gets a 1000-vnum window starting at the CLI `--vnum-base`,
//! split into sub-ranges:
//!
//! | Kind   | Sub-range within window |
//! |--------|-------------------------|
//! | room   | `+0..+399`              |
//! | mobile | `+400..+699`            |
//! | item   | `+700..+999`            |
//!
//! Quests are bundle-global and start at `--quest-vnum-base` (one slot per
//! quest, no sub-range bookkeeping).
//!
//! Re-imports load the existing map first; only fresh `<area>:<kind>:<id>`
//! triples consume new vnums, so applied IronMUD entities keep their vnum
//! across re-runs.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Room,
    Mobile,
    Item,
    Quest,
}

impl Kind {
    fn token(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Mobile => "mobile",
            Self::Item => "item",
            Self::Quest => "quest",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VnumMap {
    /// Key is `<area>:<kind>:<id>`. Value is the assigned source vnum.
    /// `area` is empty for `Quest` (quests are bundle-global).
    #[serde(default)]
    pub assignments: HashMap<String, i32>,
    /// Highest vnum used per area within each sub-range — drives next-id
    /// allocation when a new id appears on re-import.
    #[serde(default)]
    pub area_high_water: HashMap<String, AreaWindowState>,
    /// Highest quest vnum used so far in this bundle.
    #[serde(default)]
    pub quest_high_water: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaWindowState {
    /// Window base (the vnum corresponding to room sub-range start).
    pub base: i32,
    pub last_room: i32,
    pub last_mobile: i32,
    pub last_item: i32,
}

impl VnumMap {
    pub fn load_for_bundle(bundle_name: &str) -> Result<Self> {
        let path = path_for_bundle(bundle_name);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let map: VnumMap =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(map)
    }

    pub fn save_for_bundle(&self, bundle_name: &str) -> Result<()> {
        let path = path_for_bundle(bundle_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    fn lookup_key(area: &str, kind: Kind, id: &str) -> String {
        format!("{area}:{}:{id}", kind.token())
    }

    /// Resolve (or assign) a vnum for the given `(area, kind, id)`.
    /// `area_base` is the window base for this area (`vnum_base + N*1000`).
    /// Returns `None` if the area sub-range is exhausted.
    pub fn resolve(&mut self, area: &str, kind: Kind, id: &str, area_base: i32) -> Option<i32> {
        let key = Self::lookup_key(area, kind, id);
        if let Some(v) = self.assignments.get(&key) {
            return Some(*v);
        }
        let state = self
            .area_high_water
            .entry(area.to_string())
            .or_insert_with(|| AreaWindowState {
                base: area_base,
                last_room: area_base - 1,
                last_mobile: area_base + 400 - 1,
                last_item: area_base + 700 - 1,
            });
        // Defensive: if a saved sidecar disagrees with current --vnum-base,
        // keep the saved base. The high-water marks are the source of truth.
        let (slot_high_max, current) = match kind {
            Kind::Room => (state.base + 400, &mut state.last_room),
            Kind::Mobile => (state.base + 700, &mut state.last_mobile),
            Kind::Item => (state.base + 1000, &mut state.last_item),
            Kind::Quest => unreachable!("Quest kind goes through resolve_quest"),
        };
        let next = *current + 1;
        if next >= slot_high_max {
            return None;
        }
        *current = next;
        self.assignments.insert(key, next);
        Some(next)
    }

    pub fn resolve_quest(&mut self, id: &str, quest_base: i32) -> i32 {
        let key = Self::lookup_key("", Kind::Quest, id);
        if let Some(v) = self.assignments.get(&key) {
            return *v;
        }
        let next = match self.quest_high_water {
            Some(prev) => prev + 1,
            None => quest_base,
        };
        self.quest_high_water = Some(next);
        self.assignments.insert(key, next);
        next
    }

    pub fn get(&self, area: &str, kind: Kind, id: &str) -> Option<i32> {
        self.assignments.get(&Self::lookup_key(area, kind, id)).copied()
    }
}

fn path_for_bundle(bundle_name: &str) -> PathBuf {
    PathBuf::from("imports").join(format!("{bundle_name}.vnum-map.json"))
}
