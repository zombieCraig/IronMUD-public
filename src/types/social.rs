//! Social and demographic types for mobiles: visual characteristics,
//! relationships, mood, life stage, and per-mobile social preferences.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Visual/physical characteristics for a generated migrant (or any mobile).
/// Currently visual-only; traits, skills, and personality seeds will be added later.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Characteristics {
    #[serde(default)]
    pub gender: String, // "male" | "female"
    #[serde(default)]
    pub age: i32,
    #[serde(default)]
    pub age_label: String, // "young adult", "middle-aged", etc.
    /// Absolute game day the mobile was born. Source of truth for age; `age` and
    /// `age_label` are caches refreshed by the aging tick. Zero means unknown
    /// (back-compat for pre-aging saves — aging tick back-computes from `age`).
    #[serde(default)]
    pub birth_day: i64,
    #[serde(default)]
    pub height: String,
    #[serde(default)]
    pub build: String,
    #[serde(default)]
    pub hair_color: String,
    #[serde(default)]
    pub hair_style: String,
    #[serde(default)]
    pub eye_color: String,
    #[serde(default)]
    pub skin_tone: String,
    #[serde(default)]
    pub distinguishing_mark: Option<String>,
}

/// Kind of social relationship between two mobiles. Stored on MobileData.relationships.
/// Partner/Parent/Child/Sibling aren't used by any tick yet, but the data lives on the
/// mobile so builders can wire up families today and future systems can read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Partner,
    Parent,
    Child,
    Sibling,
    Friend,
    /// Two adult mobiles who have moved into the same liveable room together.
    Cohabitant,
}

impl RelationshipKind {
    pub fn from_str(s: &str) -> Option<RelationshipKind> {
        match s.to_lowercase().as_str() {
            "partner" | "spouse" => Some(RelationshipKind::Partner),
            "parent" => Some(RelationshipKind::Parent),
            "child" => Some(RelationshipKind::Child),
            "sibling" => Some(RelationshipKind::Sibling),
            "friend" => Some(RelationshipKind::Friend),
            "cohabitant" | "cohab" => Some(RelationshipKind::Cohabitant),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            RelationshipKind::Partner => "partner",
            RelationshipKind::Parent => "parent",
            RelationshipKind::Child => "child",
            RelationshipKind::Sibling => "sibling",
            RelationshipKind::Friend => "friend",
            RelationshipKind::Cohabitant => "cohabitant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub other_id: Uuid,
    pub kind: RelationshipKind,
    /// How the holder feels about `other_id`. Range: -100..=100. 0 is neutral.
    /// Positive affinity is grown via matched topics in conversation and triggers
    /// cohabitation at high thresholds; strongly negative affinity triggers breakup.
    #[serde(default)]
    pub affinity: i32,
    /// Game day of the most recent interaction, used for slow drift toward neutral.
    #[serde(default)]
    pub last_interaction_day: i32,
    /// Topics recently covered with this partner (most-recent first). Conversation
    /// logic halves the affinity/happiness delta when the chosen topic appears
    /// here, so repeating the same subject yields diminishing returns. Capped at
    /// `TOPIC_FATIGUE_WINDOW` entries.
    #[serde(default)]
    pub recent_topics: Vec<String>,
}

/// Maximum number of topics retained per `Relationship::recent_topics`. Once a
/// topic rolls off the window, it counts as "fresh" again.
pub const TOPIC_FATIGUE_WINDOW: usize = 5;

/// Derived emotional state bucket computed from SocialState::happiness.
/// Stored so buff/emote hooks can observe transitions without recomputing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MoodState {
    Content,
    #[default]
    Normal,
    Sad,
    Depressed,
    Breakdown,
}

impl MoodState {
    pub fn to_display_string(&self) -> &'static str {
        match self {
            MoodState::Content => "content",
            MoodState::Normal => "normal",
            MoodState::Sad => "sad",
            MoodState::Depressed => "depressed",
            MoodState::Breakdown => "breakdown",
        }
    }
}

/// Derived life stage bucket computed from `Characteristics.age`. Stage
/// boundaries are the single source of truth for age-gated behaviour (migrant
/// exclusion of juveniles, pregnancy eligibility, old-age death rolls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LifeStage {
    Baby,
    Child,
    Adolescent,
    #[default]
    YoungAdult,
    Adult,
    MiddleAged,
    Elderly,
}

impl LifeStage {
    pub fn to_display_string(&self) -> &'static str {
        age_label_for_stage(*self)
    }
}

/// Number of game days in a game year. Keep in sync with
/// `GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR` in `src/types/time.rs`.
pub const GAME_DAYS_PER_YEAR: i64 = 360;

/// Map a numeric age (years) to its [`LifeStage`]. Single source of truth —
/// consulted by the aging tick, migration filters, and examine cues.
pub fn life_stage_for_age(age: i32) -> LifeStage {
    match age {
        i32::MIN..=2 => LifeStage::Baby,
        3..=12 => LifeStage::Child,
        13..=17 => LifeStage::Adolescent,
        18..=29 => LifeStage::YoungAdult,
        30..=49 => LifeStage::Adult,
        50..=64 => LifeStage::MiddleAged,
        _ => LifeStage::Elderly,
    }
}

/// Human-readable label for a life stage. These strings also appear in the
/// `age_ranges` entries in `scripts/data/visuals/*.json`, so keep them aligned.
pub fn age_label_for_stage(stage: LifeStage) -> &'static str {
    match stage {
        LifeStage::Baby => "baby",
        LifeStage::Child => "child",
        LifeStage::Adolescent => "adolescent",
        LifeStage::YoungAdult => "young adult",
        LifeStage::Adult => "adult",
        LifeStage::MiddleAged => "middle-aged",
        LifeStage::Elderly => "elderly",
    }
}

/// A record that this mobile is mourning a specific dead relation. Populated
/// by `db::delete_mobile` for every surviving family/cohabitant partner whose
/// affinity toward the deceased was not deeply negative. Cleared lazily by
/// the simulation tick once `until_day` has passed. Drives richer examine
/// cues than scanning broken Uuid references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BereavementNote {
    pub other_id: Uuid,
    pub other_name: String,
    pub kind: RelationshipKind,
    pub until_day: i32,
}

/// Social preferences + happiness tracking for simulated mobiles.
/// Seeded at migration time; never edited directly by builders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialState {
    #[serde(default)]
    pub likes: Vec<String>,
    #[serde(default)]
    pub dislikes: Vec<String>,
    /// 0..=100, default 50. Drives MoodState and buff application.
    #[serde(default = "default_happiness")]
    pub happiness: i32,
    #[serde(default)]
    pub mood: MoodState,
    /// Unix seconds of the last conversation; acts as a per-mobile cooldown.
    #[serde(default)]
    pub last_converse_secs: u64,
    /// Game day until which this mobile refuses new pair bonds after losing a cohabitant.
    #[serde(default)]
    pub bereaved_until_day: Option<i32>,
    /// Per-relation mourning notes used to surface "mourning their father"
    /// style cues. Written on death by `db::delete_mobile`, pruned by the
    /// simulation tick when `until_day` has passed.
    #[serde(default)]
    pub bereaved_for: Vec<BereavementNote>,
    /// Absolute game day a birth is due. `None` when not pregnant. Only
    /// females in YoungAdult/Adult stage carry this field; the aging tick
    /// checks it on birth day.
    #[serde(default)]
    pub pregnant_until_day: Option<i32>,
    /// Mobile id of the father. Set on conception; read at birth to wire
    /// reciprocal Parent/Child links. Cleared after birth alongside
    /// `pregnant_until_day`.
    #[serde(default)]
    pub pregnant_by: Option<Uuid>,
}

impl Default for SocialState {
    fn default() -> Self {
        SocialState {
            likes: Vec::new(),
            dislikes: Vec::new(),
            happiness: 50,
            mood: MoodState::Normal,
            last_converse_secs: 0,
            bereaved_until_day: None,
            bereaved_for: Vec::new(),
            pregnant_until_day: None,
            pregnant_by: None,
        }
    }
}

fn default_happiness() -> i32 {
    50
}
