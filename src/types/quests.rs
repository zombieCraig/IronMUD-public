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
    /// Set-count prereq: require `min_count` of the listed achievement keys
    /// to be unlocked before the quest becomes offerable. Mirrors the
    /// `HasAchievement` dialogue condition's read path. Use this for endgame
    /// quests gated on "completed N of M investigation lines" — no fixed
    /// ordering, only a threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub achievement_set_prereq: Option<AchievementSetPrereq>,
}

/// Set-count achievement gate. The quest is offerable when at least
/// `min_count` keys in `keys` are present in the player's
/// `achievements_unlocked` map.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AchievementSetPrereq {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub min_count: i32,
}

impl AchievementSetPrereq {
    /// Returns the number of `keys` currently present in `unlocked`. Used by
    /// both prereq enforcement and offer-cue rendering.
    pub fn unlocked_count<V>(&self, unlocked: &HashMap<String, V>) -> i32 {
        self.keys
            .iter()
            .filter(|k| unlocked.contains_key(*k))
            .count() as i32
    }

    /// Is this prereq satisfied right now? Returns true when `keys` is empty
    /// or `min_count` is non-positive (treated as "no gate"), or when the
    /// unlocked count meets the threshold.
    pub fn is_satisfied<V>(&self, unlocked: &HashMap<String, V>) -> bool {
        if self.keys.is_empty() || self.min_count <= 0 {
            return true;
        }
        self.unlocked_count(unlocked) >= self.min_count
    }
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
            achievement_set_prereq: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuestObjective {
    /// Slay `count` instances of the named mob prototype vnum.
    KillMob { vnum: String, count: i32 },
    /// Slay `count` instances drawn from any of the listed prototype vnums.
    /// Use this when the questgiver wants "any hunter" or "any migrant
    /// arrival" semantics — every kill of any listed vnum increments a
    /// shared counter. Progress is stored under a stable key derived from
    /// the (sorted) vnum set so the same objective re-evaluates the same
    /// bucket across server restarts.
    KillAnyMob {
        #[serde(default)]
        vnums: Vec<String>,
        count: i32,
    },
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
    /// Grants the named clan to a thinblood vampire. Adds the matching
    /// `clan_<name>` trait, seeds 1 dot of the clan's first registered
    /// preferred discipline, and lifts the thinblood gates (max blood pool
    /// 6 -> 10, blood refilled, sun damage normal, humanity loss normal,
    /// tier-3 disciplines unlocked). Sire is taken from the quest's
    /// `giver_mob_vnum` prototype name when present. No-op for mortals
    /// or already-acknowledged kindred.
    EmbraceClan { clan: String },
    /// Anarch-path counterpart to `EmbraceClan`. Lifts the thinblood gates
    /// (blood pool 6 -> 10, refill, sun damage normal, humanity normal,
    /// tier-3 disciplines unlocked) without claiming a clan. Stamps the
    /// `anarch_unbound` trait, sets sire to the sentinel `"Anarch Unbound"`,
    /// and seeds 1 dot of the chosen discipline.
    ///
    /// `discipline = Some(name)` hardcodes the seeded discipline; `None`
    /// pulls it from the player's `ActiveQuest.choice_vars["discipline"]`
    /// (set by a `DialogueEffect::SetQuestChoice` earlier in the tree).
    /// No-op for mortals or already-acknowledged kindred (clan_* trait or
    /// existing `anarch_unbound` trait).
    EmbraceAnarch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discipline: Option<String>,
    },
}

/// Build the stable storage key for a `KillAnyMob` objective's progress
/// bucket. Sorts and joins the vnum set so equal sets (regardless of input
/// order) map to the same `kill_any_progress` key.
pub fn kill_any_key(vnums: &[String]) -> String {
    let mut sorted: Vec<&str> = vnums.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.join(",")
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
    /// KillAnyMob progress: kill_any_key(sorted vnums) -> kills accumulated.
    /// Independent from `kill_progress` so a quest may carry both a
    /// `KillMob` and a `KillAnyMob` objective without cross-talk.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub kill_any_progress: HashMap<String, i32>,
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
    /// Free-form per-quest choice vars set by `DialogueEffect::SetQuestChoice`.
    /// Consumed by reward handlers that need a runtime-chosen value (e.g. the
    /// Anarch path's player-picked discipline on Q10). Empty by default.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub choice_vars: HashMap<String, String>,
}
