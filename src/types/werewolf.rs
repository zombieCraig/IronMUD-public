//! Werewolf-specific runtime state carried by `CharacterData`.
//!
//! Models the Werewolf: the Apocalypse Rage economy. Rage builds in combat
//! (taking hits, making kills, the deliberate `rage` build action) and is
//! spent on form shifts; run it high and the wolf takes the wheel — an
//! involuntary frenzy rolled on the rage tick (and forced when a gain slams
//! into the cap), riding the exact Frenzy+Rage buff pair the vampire hunger
//! frenzy uses. Out of combat, Rage cools one point per tick.
//!
//! Everything that maps onto existing IronMUD systems lives there:
//! - **Tribe** is a granted trait (`tribe_get_of_fenris`, …), read by the
//!   generic trait plumbing — tribe banes are `frenzy_dc_modifier` /
//!   `flee_bonus` trait effects, extracted the same way clan banes are.
//! - **Gifts** are spells in `spells_werewolf.json` gated by
//!   `requires_werewolf`; they cost mana, which the status screen relabels
//!   "Gnosis" for werewolves — gift use and class magic share one well.
//! - **Forms** are persistent stat buffs (source "form") re-asserted by the
//!   rage tick, the mutation passive-reassertion pattern.
//!
//! The struct lives behind `Option<WerewolfState>` on `CharacterData`:
//! `None` means "not a werewolf", mirroring `VampireState`. PC-only — mobs
//! don't carry it (deliberate scope cut vs. vampire's mob support).

use serde::{Deserialize, Serialize};

pub const WEREWOLF_DEFAULT_MAX_RAGE: i32 = 10;
/// Rage at the First Change.
pub const WEREWOLF_STARTING_RAGE: i32 = 2;
/// Cadence of the rage tick (decay, frenzy rolls, form-buff reassertion).
pub const RAGE_TICK_INTERVAL_SECS: u64 = 60;
/// Rage lost per tick while OUT of combat.
pub const RAGE_DECAY_PER_TICK: i32 = 1;
/// Rage gained when taking damage in a combat round (capped at 1/round).
pub const RAGE_GAIN_ON_DAMAGE: i32 = 1;
/// Rage gained when a kill is credited.
pub const RAGE_GAIN_ON_KILL: i32 = 2;
/// Rage gained by the deliberate `rage` build action (combat only).
pub const RAGE_BUILD_GAIN: i32 = 2;
pub const RAGE_BUILD_COOLDOWN_SECS: i64 = 60;
/// At or above this rage the wolf starts testing the cage (frenzy rolls on
/// every rage tick).
pub const RAGE_FRENZY_THRESHOLD: i32 = 8;
/// Shifting while rage is at or above this forces an immediate frenzy roll.
pub const SHIFT_FRENZY_THRESHOLD: i32 = 9;
/// Rage costs of the forms.
pub const CRINOS_SHIFT_COST: i32 = 3;
pub const LUPUS_SHIFT_COST: i32 = 1;
/// Buff source for all form buffs; shifting swaps everything by this source.
pub const WEREWOLF_FORM_SOURCE: &str = "form";

pub const FORM_HOMID: &str = "homid";
pub const FORM_CRINOS: &str = "crinos";
pub const FORM_LUPUS: &str = "lupus";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WerewolfState {
    /// Current Rage pool. High rage risks involuntary frenzy.
    #[serde(default)]
    pub rage: i32,
    #[serde(default = "default_max_rage")]
    pub max_rage: i32,
    /// "homid" | "crinos" | "lupus".
    #[serde(default = "default_form")]
    pub current_form: String,
    /// Unix timestamp at which active frenzy ends. `None` = in control.
    /// Informational mirror of the Frenzy+Rage buff pair (the buffs are the
    /// operative mechanism), matching `VampireState.frenzy_until`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frenzy_until: Option<i64>,
    /// Unix timestamp of the last rage tick.
    #[serde(default)]
    pub last_rage_tick: i64,
    /// Cooldown bookkeeping for the `rage` build action.
    #[serde(default)]
    pub last_rage_build: i64,
    /// Unix timestamp of the First Change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awakened_at: Option<i64>,
}

fn default_max_rage() -> i32 {
    WEREWOLF_DEFAULT_MAX_RAGE
}

fn default_form() -> String {
    FORM_HOMID.to_string()
}

impl Default for WerewolfState {
    fn default() -> Self {
        WerewolfState {
            rage: WEREWOLF_STARTING_RAGE,
            max_rage: WEREWOLF_DEFAULT_MAX_RAGE,
            current_form: FORM_HOMID.to_string(),
            frenzy_until: None,
            last_rage_tick: 0,
            last_rage_build: 0,
            awakened_at: None,
        }
    }
}

impl WerewolfState {
    /// Fresh state for a Garou at the First Change.
    pub fn newly_awakened(now: i64) -> Self {
        WerewolfState {
            last_rage_tick: now,
            awakened_at: Some(now),
            ..Default::default()
        }
    }

    /// Clamp rage to [0, max_rage], returning the new value.
    pub fn set_rage(&mut self, value: i32) -> i32 {
        self.rage = value.clamp(0, self.max_rage);
        self.rage
    }

    /// Add `delta` to rage, clamped. Returns true when the gain hit or
    /// exceeded the cap (callers force a frenzy roll on overflow).
    pub fn gain_rage(&mut self, delta: i32) -> bool {
        let raw = self.rage.saturating_add(delta);
        self.set_rage(raw);
        raw >= self.max_rage
    }

    /// True while a frenzy is active relative to `now`.
    pub fn is_frenzying(&self, now: i64) -> bool {
        self.frenzy_until.map(|t| t > now).unwrap_or(false)
    }
}
