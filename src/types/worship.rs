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
    /// Divine spellcraft: this god grants player-authored spells. None = it
    /// grants none.
    #[serde(default)]
    pub spellcraft: Option<DeitySpellcraft>,
}

/// How a god grants player-authored spells (`medit <god> deity spellcraft`).
/// The allow-lists bound what a petition may draft; an empty list allows
/// nothing of that kind. Override fields left `None` use the server setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeitySpellcraft {
    /// Subset of damage/debuff/buff/heal.
    #[serde(default)]
    pub allowed_types: Vec<String>,
    /// `damage_type` values damage spells may use.
    #[serde(default)]
    pub allowed_elements: Vec<String>,
    /// `buff_effect` (EffectType) values buffs/debuffs may use.
    #[serde(default)]
    pub allowed_effects: Vec<String>,
    /// Subset of self/self_or_friendly/enemy.
    #[serde(default)]
    pub allowed_targets: Vec<String>,
    /// Spells the god's canon holds at once.
    #[serde(default = "default_canon_cap")]
    pub canon_cap: u32,
    /// Overrides `spellcraft.god_petition_cooldown_days`.
    #[serde(default)]
    pub petition_cooldown_days: Option<i64>,
    /// Overrides `spellcraft.min_favor`.
    #[serde(default)]
    pub min_favor: Option<i64>,
    /// Overrides `spellcraft.min_power_tier`.
    #[serde(default)]
    pub min_power_tier: Option<i64>,
    /// Favor spent when a spell is sealed.
    #[serde(default = "default_spellcraft_favor_cost")]
    pub favor_cost: i64,
    /// Capstone quest that must be complete before any petition.
    #[serde(default)]
    pub unlock_quest_id: Option<String>,
    /// Priests of this god teach its canon spells.
    #[serde(default = "default_true")]
    pub priest_teaches: bool,
    /// What the god takes beyond the base HP pledge.
    #[serde(default)]
    pub price_style: super::PriceStyle,
    /// The tasks this god sets a petitioner, picked by the drafted spell's
    /// nature (`spellcraft::draft::pick_trial`). No matching rule = no petition.
    #[serde(default)]
    pub trial_rules: Vec<super::TrialRule>,
    /// Brands the god puts on a spell's author (one at sealing, more on a
    /// backlash). Empty = a plain sigil with no affects.
    #[serde(default)]
    pub mark_pool: Vec<super::MarkTemplate>,
}

impl Default for DeitySpellcraft {
    fn default() -> Self {
        DeitySpellcraft {
            allowed_types: Vec::new(),
            allowed_elements: Vec::new(),
            allowed_effects: Vec::new(),
            allowed_targets: Vec::new(),
            canon_cap: default_canon_cap(),
            petition_cooldown_days: None,
            min_favor: None,
            min_power_tier: None,
            favor_cost: default_spellcraft_favor_cost(),
            unlock_quest_id: None,
            priest_teaches: true,
            price_style: super::PriceStyle::default(),
            trial_rules: Vec::new(),
            mark_pool: Vec::new(),
        }
    }
}

fn default_canon_cap() -> u32 {
    5
}

fn default_spellcraft_favor_cost() -> i64 {
    250
}

fn default_true() -> bool {
    true
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
            spellcraft: None,
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
