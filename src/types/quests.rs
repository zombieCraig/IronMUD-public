//! Quest system types: prototypes, objectives, rewards, and per-player
//! active progress.

use super::serde_defaults::default_qty_one;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A first-class quest prototype, stored in the `quests` sled tree keyed by
/// vnum. Per-player progress lives in `CharacterData.active_quests` /
/// `completed_quests` rather than alongside the prototype.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestData {
    pub vnum: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// One-line summary shown in the `quests` list view.
    #[serde(default)]
    pub summary: String,
    /// Long description shown when offering / detailing the quest.
    #[serde(default)]
    pub description: String,
    /// Text shown to the player on successful completion (right before the
    /// reward delivery line).
    #[serde(default)]
    pub completion_text: String,
    #[serde(default)]
    pub objectives: Vec<QuestObjective>,
    #[serde(default)]
    pub rewards: Vec<QuestReward>,
    #[serde(default)]
    pub repeatable: bool,
    /// Optional canonical questgiver. Used by builder tooling and by
    /// `find_quests_by_giver_mob_vnum` for surface integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub giver_mob_vnum: Option<String>,
    /// Reserved for slice 3 (quest chains). Carried through serde so authored
    /// data lands clean even though slice 1 doesn't gate on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prereq_quest_vnum: Option<String>,
    /// Reserved for slice 3 (soft level gate). Sum of skill levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_player_skill_total: Option<i32>,
    /// Slice 3b: optional expiry. When set, the quest expiry tick drops the
    /// quest from a player's `active_quests` if more than `duration_secs`
    /// have elapsed since `started_at`. None = no expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<i64>,
}

impl QuestData {
    pub fn new(vnum: String, name: String) -> Self {
        Self {
            vnum,
            name,
            keywords: Vec::new(),
            summary: String::new(),
            description: String::new(),
            completion_text: String::new(),
            objectives: Vec::new(),
            rewards: Vec::new(),
            repeatable: false,
            giver_mob_vnum: None,
            prereq_quest_vnum: None,
            min_player_skill_total: None,
            duration_secs: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuestObjective {
    /// Slay `count` instances of the named mob prototype vnum.
    KillMob { vnum: String, count: i32 },
    /// Acquire (and turn in) `qty` of the named item vnum. When
    /// `return_to_mob_vnum` is `Some`, handing the items to that mob via
    /// `give` consumes them and advances progress; auto-completion fires
    /// when every BringItem objective is fully delivered. When `None`, the
    /// objective advances on inventory presence and completion must be
    /// driven via a `CompleteQuest` dialogue effect.
    BringItem {
        vnum: String,
        qty: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        return_to_mob_vnum: Option<String>,
    },
    /// Visit a named room vnum. Listener defers to slice 2.
    VisitRoom { vnum: String },
    /// The named DG var on the player has reached `value`. Listener defers
    /// to slice 2.
    DgFlag { var: String, value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuestReward {
    Gold { amount: i64 },
    Item {
        vnum: String,
        #[serde(default = "default_qty_one")]
        qty: i32,
    },
    SkillXp { skill: String, amount: i32 },
    /// Triggers `award_achievement` against the named achievement key.
    Achievement { key: String },
    LearnRecipe { recipe_id: String },
}

/// Per-player quest progress carried on `CharacterData.active_quests`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveQuest {
    /// Unix epoch seconds when the quest was accepted.
    #[serde(default)]
    pub started_at: i64,
    /// Mob prototype vnum -> kills accumulated.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub kill_progress: HashMap<String, i32>,
    /// Item prototype vnum -> qty turned in (BringItem with
    /// `return_to_mob_vnum`) OR currently in inventory toward the goal.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub item_progress: HashMap<String, i32>,
    /// Room vnums visited toward `VisitRoom` objectives. Slice 2 listener.
    #[serde(default, skip_serializing_if = "std::collections::HashSet::is_empty")]
    pub rooms_visited: std::collections::HashSet<String>,
    /// DG flag keys that have hit their target value. Slice 2 listener.
    #[serde(default, skip_serializing_if = "std::collections::HashSet::is_empty")]
    pub flags_set: std::collections::HashSet<String>,
}
