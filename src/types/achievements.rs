//! Achievement system types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AchievementCategory {
    Skill,
    Combat,
    Crafting,
    Exploration,
    Social,
    Wealth,
    Builder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AchievementCriterion {
    /// A flat dotted counter key (e.g. "kills.goblin", "kills.any") and
    /// the threshold the counter must reach.
    Counter { counter: String, threshold: u32 },
    /// Skill reaches at least the given level.
    SkillReached { skill: String, level: i32 },
    /// Specific recipe is learned by the player.
    LearnedRecipe { recipe_key: String },
    /// Player owns a lease (any area, or a specific one).
    OwnedLease {
        #[serde(default)]
        area_vnum: Option<String>,
    },
    /// Gold balance reached at least the given amount (high-water).
    GoldHeld { amount: i32 },
    /// Awarded only by DG `award_achievement` verb. The criterion never
    /// fires on its own.
    Manual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AchievementReward {
    /// Always granted on unlock; selectable via `title set <key>`.
    pub title: String,
    /// Optional cosmetic item vnum delivered to inventory or escrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_vnum: Option<String>,
    /// Optional gold lump-sum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold: Option<i32>,
    /// Optional morality shift applied at unlock. Positive pushes toward
    /// Good, negative toward Evil. Clamped into `[-200, 200]` by the
    /// unlock pipeline. Defaults to 0 (no shift).
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub morality_delta: i32,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AchievementSource {
    /// Loaded from a JSON file under scripts/data/achievements/.
    Json { file: String },
    /// Created via achedit/REST/MCP and stored in the sled `achievements` tree.
    Db {
        #[serde(default)]
        author: String,
    },
}

impl Default for AchievementSource {
    fn default() -> Self {
        AchievementSource::Db { author: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementDef {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub category: AchievementCategory,
    pub criterion: AchievementCriterion,
    #[serde(default)]
    pub reward: AchievementReward,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub source: AchievementSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementUnlock {
    /// Unix timestamp (seconds) at unlock time.
    pub unlocked_at: i64,
}
