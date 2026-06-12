//! Cyberware runtime state carried by `CharacterData`.
//!
//! Models Cyberpunk RED's chrome-versus-soul trade: any character (except
//! races whose `cyberware_affinity` is `Incompatible`) can have cyberware
//! installed, paying **Humanity** for every piece. Max humanity derives from
//! the character's *base* CHA (`base_cha * 10`) minus a per-piece reduction
//! (2, or 4 for borgware; 0-HL fashionware reduces nothing) — only removing
//! the piece restores that headroom. Current humanity is spent at install
//! time (the item's `cyber_humanity_loss` tier, discounted for `Adept`
//! races) and recovered only through therapy. Every full 10 points of
//! humanity lost erodes 1 effective CHA via a permanent negative
//! `CharismaBoost` buff. Low humanity risks **cyberpsychosis**: escalating
//! dissociative or violent episodes rolled on the psyche tick.
//!
//! Installed chrome follows the tattoo pattern (`db.apply_tattoo_to_character`):
//! the item instance is consumed and an `InstalledCyberware` snapshot is
//! pushed here, with the item's `affects` stamped as permanent `ActiveBuff`s
//! sourced `"cyberware:<install_id>"`. Uninstalling rebuilds an `ItemData`
//! from the snapshot. Chrome therefore never occupies wear slots and cannot
//! be dropped, stolen, or looted — it is inside you.
//!
//! The struct lives behind `Option<CyberwareState>` on `CharacterData`:
//! `None` means "no chrome, no humanity tracking", mirroring `VampireState`
//! and `ReplicantState`. NOTE: this humanity (0..CHA*10) is unrelated to the
//! vampire 0-10 humanity scale — UI strings label it "Humanity (chrome)".

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::effects::ItemAffect;

/// Cadence of the psyche tick (cyberpsychosis episode rolls + erosion recalc).
pub const PSYCHE_TICK_INTERVAL_SECS: u64 = 60;
/// Max-humanity reduction per installed piece with a nonzero humanity-loss
/// tier. Pieces with `cyber_humanity_loss == 0` (fashionware) reduce nothing.
pub const MAX_HUMANITY_REDUCTION_PER_PIECE: i32 = 2;
/// Borgware reduces max humanity by this much instead.
pub const BORGWARE_MAX_HUMANITY_REDUCTION: i32 = 4;
/// Humanity lost per point of effective-CHA erosion: -1 CHA per full 10 lost.
pub const HUMANITY_PER_CHA_POINT: i32 = 10;
/// At or above this humanity percentage the mind is stable — no episode rolls.
pub const PSYCHE_STABLE_PCT: i32 = 30;
/// Below this percentage episodes can turn violent.
pub const PSYCHE_VIOLENT_BAND_PCT: i32 = 15;
/// Episode chance per tick at exactly 0 humanity (always violent).
pub const ZERO_HUMANITY_EPISODE_CHANCE: i32 = 75;
/// Violent-band chance cap: 2 * (30 - pct), clamped to this.
pub const VIOLENT_BAND_CHANCE_CAP: i32 = 60;
/// Minimum seconds between episodes.
pub const EPISODE_COOLDOWN_SECS: i64 = 300;
/// Tighter cooldown once humanity is gone entirely.
pub const EPISODE_COOLDOWN_ZERO_SECS: i64 = 120;
pub const DISSOCIATION_DURATION_SECS: i32 = 60;
pub const VIOLENT_DURATION_SECS: i32 = 45;
pub const ZERO_VIOLENT_DURATION_SECS: i32 = 60;

pub const EPISODE_KIND_DISSOCIATION: &str = "dissociation";
pub const EPISODE_KIND_VIOLENT: &str = "violent";

/// Buff source for episode debuffs (Slow/Luck or Frenzy/Rage).
pub const CYBERPSYCHOSIS_BUFF_SOURCE: &str = "cyberpsychosis";
/// Buff source for the single permanent CHA-erosion buff.
pub const HUMANITY_EROSION_BUFF_SOURCE: &str = "cyberware:humanity";
/// Prefix for per-install affect buffs: `"cyberware:<install_id>"`.
pub const CYBERWARE_BUFF_SOURCE_PREFIX: &str = "cyberware:";

/// Cyberpunk RED taxonomy. Foundations (neural link, cybereye, cyberaudio
/// suite, cyberarm, cyberleg) provide option slots; options of the same
/// category install into them. Fashionware / internal / external / borgware
/// pieces stand alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CyberwareCategory {
    Fashionware,
    Neuralware,
    Cyberoptic,
    Cyberaudio,
    Cyberarm,
    Cyberleg,
    InternalBody,
    ExternalBody,
    Borgware,
}

impl CyberwareCategory {
    pub fn from_str(s: &str) -> Option<CyberwareCategory> {
        match s.to_lowercase().as_str() {
            "fashionware" | "fashion" => Some(CyberwareCategory::Fashionware),
            "neuralware" | "neural" => Some(CyberwareCategory::Neuralware),
            "cyberoptic" | "cyberoptics" | "optic" | "eye" => Some(CyberwareCategory::Cyberoptic),
            "cyberaudio" | "audio" | "ear" => Some(CyberwareCategory::Cyberaudio),
            "cyberarm" | "arm" => Some(CyberwareCategory::Cyberarm),
            "cyberleg" | "leg" => Some(CyberwareCategory::Cyberleg),
            "internal_body" | "internal" => Some(CyberwareCategory::InternalBody),
            "external_body" | "external" => Some(CyberwareCategory::ExternalBody),
            "borgware" | "borg" => Some(CyberwareCategory::Borgware),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            CyberwareCategory::Fashionware => "fashionware",
            CyberwareCategory::Neuralware => "neuralware",
            CyberwareCategory::Cyberoptic => "cyberoptic",
            CyberwareCategory::Cyberaudio => "cyberaudio",
            CyberwareCategory::Cyberarm => "cyberarm",
            CyberwareCategory::Cyberleg => "cyberleg",
            CyberwareCategory::InternalBody => "internal_body",
            CyberwareCategory::ExternalBody => "external_body",
            CyberwareCategory::Borgware => "borgware",
        }
    }

    /// Option slots a foundation of this category provides when the item
    /// doesn't override via `cyber_option_slots` (RED defaults).
    pub fn default_option_slots(&self) -> i32 {
        match self {
            CyberwareCategory::Neuralware => 5,
            CyberwareCategory::Cyberoptic => 3,
            CyberwareCategory::Cyberaudio => 3,
            CyberwareCategory::Cyberarm => 4,
            CyberwareCategory::Cyberleg => 3,
            _ => 0,
        }
    }

    /// How many foundations of this category one body holds (one neural
    /// link / cyberaudio suite; two eyes, arms, legs). 0 = category has no
    /// foundations.
    pub fn foundation_max(&self) -> i32 {
        match self {
            CyberwareCategory::Neuralware => 1,
            CyberwareCategory::Cyberaudio => 1,
            CyberwareCategory::Cyberoptic => 2,
            CyberwareCategory::Cyberarm => 2,
            CyberwareCategory::Cyberleg => 2,
            _ => 0,
        }
    }

    /// Whether chrome of this category is externally visible on `examine`.
    /// Neuralware / cyberaudio / internal body ware is hidden under skin.
    pub fn is_visible(&self) -> bool {
        !matches!(
            self,
            CyberwareCategory::Neuralware | CyberwareCategory::Cyberaudio | CyberwareCategory::InternalBody
        )
    }

    /// Max-humanity reduction for an installed piece of this category with
    /// the given humanity-loss tier.
    pub fn max_humanity_reduction(&self, humanity_loss: i32) -> i32 {
        if *self == CyberwareCategory::Borgware {
            BORGWARE_MAX_HUMANITY_REDUCTION
        } else if humanity_loss <= 0 {
            0
        } else {
            MAX_HUMANITY_REDUCTION_PER_PIECE
        }
    }
}

/// How a race's biology takes to chrome. Data-driven via
/// `RaceDefinition.cyberware_affinity` in `races_*.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CyberwareAffinity {
    /// Grafts don't take (synth polymer chassis, revenant necrotic tissue).
    Incompatible,
    /// Installs at full humanity cost.
    #[default]
    Normal,
    /// Born chromed (augmented): pays ~75% of each humanity-loss tier.
    Adept,
}

impl CyberwareAffinity {
    pub fn from_str(s: &str) -> Option<CyberwareAffinity> {
        match s.to_lowercase().as_str() {
            "incompatible" => Some(CyberwareAffinity::Incompatible),
            "normal" => Some(CyberwareAffinity::Normal),
            "adept" => Some(CyberwareAffinity::Adept),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> &'static str {
        match self {
            CyberwareAffinity::Incompatible => "incompatible",
            CyberwareAffinity::Normal => "normal",
            CyberwareAffinity::Adept => "adept",
        }
    }
}

/// Snapshot of a cyberware item at install time, frozen so prototype edits
/// or deletions can't mutate chrome already inside someone. Uninstall
/// rebuilds an `ItemData` from this record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledCyberware {
    /// Per-install identity; the affect-buff source is
    /// `"cyberware:<install_id>"` so two identical cybereyes don't collide.
    pub install_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_vnum: Option<String>,
    pub name: String,
    pub short_desc: String,
    #[serde(default)]
    pub long_desc: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub affects: Vec<ItemAffect>,
    pub cyber_category: CyberwareCategory,
    #[serde(default)]
    pub cyber_foundation: bool,
    /// Option slots this piece provides (foundations only; resolved from the
    /// category default when the item didn't override).
    #[serde(default)]
    pub cyber_option_slots: i32,
    /// Slots this piece consumes in its host foundation (options only;
    /// resolved to at least 1 at install time).
    #[serde(default)]
    pub cyber_slot_cost: i32,
    /// The item's humanity-loss tier (RED: 0/2/3/7/14).
    #[serde(default)]
    pub cyber_humanity_loss: i32,
    /// Humanity actually charged at install (0 for born-chromed kit,
    /// discounted for adepts). Informational/lore bookkeeping.
    #[serde(default)]
    pub humanity_paid: i32,
    #[serde(default)]
    pub cyber_paired: bool,
    #[serde(default)]
    pub cyber_exclusive_tag: String,
    /// install_ids of the foundation(s) hosting this option — two entries
    /// when paired. Empty for foundations and standalone pieces. A
    /// foundation cannot be uninstalled while referenced here.
    #[serde(default)]
    pub host_foundations: Vec<Uuid>,
    #[serde(default)]
    pub installed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CyberwareState {
    /// Current humanity, clamped to [0, computed max]. Max is never stored:
    /// `cyberware::max_humanity(base_cha, &installed)`.
    #[serde(default)]
    pub humanity: i32,
    #[serde(default)]
    pub installed: Vec<InstalledCyberware>,
    /// Unix timestamp of the last psyche tick processed for this character.
    #[serde(default)]
    pub last_psyche_tick: i64,
    /// Unix timestamp of the last cyberpsychotic episode (cooldown pacing).
    #[serde(default)]
    pub last_episode_at: i64,
    /// Unix timestamp at which the active episode ends. `None` = stable.
    /// Informational, like `VampireState.frenzy_until` — the buffs carry the
    /// mechanics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_until: Option<i64>,
    /// "dissociation" | "violent" while an episode is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_kind: Option<String>,
    /// Unix timestamp the character first chromed up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chromed_at: Option<i64>,
    /// Lifetime humanity points recovered through therapy (lore hook).
    #[serde(default)]
    pub lifetime_humanity_restored: i32,
}

impl Default for CyberwareState {
    fn default() -> Self {
        CyberwareState {
            humanity: 0,
            installed: Vec::new(),
            last_psyche_tick: 0,
            last_episode_at: 0,
            episode_until: None,
            episode_kind: None,
            chromed_at: None,
            lifetime_humanity_restored: 0,
        }
    }
}

impl CyberwareState {
    /// Fresh state for a character about to take their first chrome:
    /// humanity starts full at `base_cha * 10`.
    pub fn newly_chromed(base_cha: i32, now: i64) -> Self {
        CyberwareState {
            humanity: base_cha.max(1) * HUMANITY_PER_CHA_POINT,
            last_psyche_tick: now,
            chromed_at: Some(now),
            ..Default::default()
        }
    }

    /// True while an episode is active relative to `now`.
    pub fn is_in_episode(&self, now: i64) -> bool {
        self.episode_until.map(|t| t > now).unwrap_or(false)
    }
}
