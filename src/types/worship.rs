//! God worship system types.
//!
//! Gods are real mobiles marked with a `DeityConfig` (presence = deity, mirroring
//! `SimulationConfig`). Only rank-`God` deities can be worshiped; Demigod/Ascended
//! are lesser pantheon figures. A player's pact and standing live in
//! `CharacterData.worship: Option<WorshipState>`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::effects::EffectType;

/// Divine rank. Only `God` is worshipable; lesser ranks appear in quests and
/// as pantheon members linked via `MobileData.patron_god_vnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GodRank {
    God,
    Demigod,
    Ascended,
}

impl Default for GodRank {
    fn default() -> Self {
        GodRank::God
    }
}

impl GodRank {
    pub fn from_str(s: &str) -> Option<GodRank> {
        match s.to_lowercase().as_str() {
            "god" => Some(GodRank::God),
            "demigod" => Some(GodRank::Demigod),
            "ascended" => Some(GodRank::Ascended),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            GodRank::God => "god",
            GodRank::Demigod => "demigod",
            GodRank::Ascended => "ascended",
        }
    }
}

/// One buff granted to a worshiper in good standing when they pray.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlessingGrant {
    pub effect: EffectType,
    pub magnitude: i32,
}

/// Builder-authored deity configuration. Presence on a mobile marks it as a
/// deity; declarative defaults here make a god fully functional with zero DG
/// scripts (gold-% tribute, blessing list, default anger ladder). DG triggers
/// (OnPray/OnWorshipPact/OnSmite) can override the defaults for complex gods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeityConfig {
    #[serde(default)]
    pub rank: GodRank,
    /// Short reusable lore line, e.g. "God of Wrath".
    #[serde(default)]
    pub epithet: String,
    /// Longer story paragraph surfaced on examine.
    #[serde(default)]
    pub lore: String,
    /// Vnums of enemy gods; killing their followers earns favor.
    #[serde(default)]
    pub enemy_god_vnums: Vec<String>,
    /// Artifact item vnums that gate the pact (any one; consumed on pact).
    #[serde(default)]
    pub pact_item_vnums: Vec<String>,
    /// Completed-quest vnums that gate the pact (any one).
    #[serde(default)]
    pub pact_quest_ids: Vec<String>,
    /// Game days between required tributes.
    #[serde(default = "default_tribute_interval_days")]
    pub tribute_interval_days: i32,
    /// Default tribute: percent of total gold (on-hand + bank).
    #[serde(default = "default_tribute_gold_percent")]
    pub tribute_gold_percent: i32,
    /// Buffs stamped on the worshiper at prayer while in good standing.
    #[serde(default)]
    pub blessing_effects: Vec<BlessingGrant>,
    /// Gates the stage-4 permanent smite (e.g. permanent Blind).
    #[serde(default)]
    pub allow_permanent_smite: bool,
}

impl Default for DeityConfig {
    fn default() -> Self {
        DeityConfig {
            rank: GodRank::God,
            epithet: String::new(),
            lore: String::new(),
            enemy_god_vnums: Vec::new(),
            pact_item_vnums: Vec::new(),
            pact_quest_ids: Vec::new(),
            tribute_interval_days: default_tribute_interval_days(),
            tribute_gold_percent: default_tribute_gold_percent(),
            blessing_effects: Vec::new(),
            allow_permanent_smite: false,
        }
    }
}

fn default_tribute_interval_days() -> i32 {
    3
}

fn default_tribute_gold_percent() -> i32 {
    5
}

/// A player's active pact with a god. Anger is derived, not stored: overdue
/// days come from `last_tribute_day` vs the current absolute game day, and
/// `anger_stage` only records the highest ladder stage already fired so each
/// stage triggers once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorshipState {
    pub god_vnum: String,
    /// Absolute game day the pact was formed.
    pub pact_day: i64,
    /// Absolute game day tribute was last paid.
    pub last_tribute_day: i64,
    /// Highest anger-ladder stage already fired; 0 = content. Reset on tribute.
    #[serde(default)]
    pub anger_stage: i32,
    /// Positive divine currency earned via enemy-god kills and DG awards.
    #[serde(default)]
    pub favor: i32,
    /// Escalating-punishment counter for attacking same-god targets.
    #[serde(default)]
    pub coworshiper_offenses: i32,
    /// Victim name -> absolute game day of last PvP kill credit
    /// (anti kill-trading: one credit per victim per game day).
    #[serde(default)]
    pub pvp_credit_days: HashMap<String, i64>,
}

impl WorshipState {
    pub fn new(god_vnum: &str, today: i64) -> Self {
        WorshipState {
            god_vnum: god_vnum.to_string(),
            pact_day: today,
            last_tribute_day: today,
            anger_stage: 0,
            favor: 0,
            coworshiper_offenses: 0,
            pvp_credit_days: HashMap::new(),
        }
    }

    /// Days past the tribute deadline (0 while current).
    pub fn overdue_days(&self, today: i64, interval_days: i32) -> i64 {
        (today - self.last_tribute_day - interval_days as i64).max(0)
    }
}
