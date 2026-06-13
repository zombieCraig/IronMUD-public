//! Synth-specific runtime state carried by `CharacterData`.
//!
//! Models the Alien RPG synthetic: a polymer chassis that does not heal, does
//! not tire of fear, and does not fall unconscious. Organic medicine barely
//! works (heal spells/potions land at SYNTH_HEAL_EFFECT_PCT, `treat` refuses
//! outright, natural HP regen is zero); the real recovery path is repair —
//! field repair kits or a technician NPC.
//!
//! Where a baseline character at 0 HP collapses into the unconscious/bleedout
//! flow, a synth "runs broken": the first lethal hit floors HP at 1, stamps
//! escalating malfunction debuffs, and starts a SYNTH_SHUTDOWN_GRACE_SECS
//! countdown to System Shutdown. A second lethal hit while critical — or the
//! countdown expiring — is death (the SunlightBurning rescue-window rule).
//!
//! Malfunction stages 0-2 (NOMINAL/DEGRADED/FAILING) are derived from HP%
//! each chassis tick; stage 3 (CRITICAL) is sticky and only clears via
//! sufficient repair. The behavioral inhibitor (can't initiate violence
//! against idle mortals) lives in `crate::synth::directive_allows_attack`.
//!
//! The struct lives behind `Option<SynthState>` on `CharacterData`: `None`
//! means "not a synth", mirroring `ReplicantState`/`MutantState`.

use serde::{Deserialize, Serialize};

/// Cadence of the chassis tick (stage derivation + shutdown countdown).
pub const CHASSIS_TICK_INTERVAL_SECS: u64 = 30;
/// Heal spells / potions land at this percent on a synth (min 1 HP).
pub const SYNTH_HEAL_EFFECT_PCT: i32 = 25;
/// A field repair kit restores this percent of max HP.
pub const SYNTH_REPAIR_KIT_HP_PCT: i32 = 25;
/// Self-repair (`repair`) cooldown.
pub const SYNTH_REPAIR_COOLDOWN_SECS: i64 = 60;
/// Seconds of emergency reserve between going critical and System Shutdown.
pub const SYNTH_SHUTDOWN_GRACE_SECS: i64 = 300;
/// Below this HP% the chassis is DEGRADED (stage 1, cosmetic).
pub const SYNTH_STAGE_DEGRADED_HP_PCT: i32 = 50;
/// Below this HP% the chassis is FAILING (stage 2, Slow 1).
pub const SYNTH_STAGE_FAILING_HP_PCT: i32 = 25;
/// CRITICAL (stage 3) combat penalties, re-stamped each chassis tick.
pub const SYNTH_CRITICAL_HIT_PENALTY: i32 = -3;
pub const SYNTH_CRITICAL_DAMAGE_PENALTY: i32 = -2;
/// Buff source for all malfunction debuffs; repair clears by this source.
pub const SYNTH_MALFUNCTION_SOURCE: &str = "malfunction";
/// Behavioral inhibitor: a mortal that left combat within this window is
/// still a legitimate target (pursue a fleeing enemy).
pub const SYNTH_RECENT_COMBAT_WINDOW_SECS: i64 = 120;
/// Kits needed to claw back from CRITICAL without a technician.
pub const SYNTH_CRITICAL_REPAIR_KITS: i32 = 2;

pub const SYNTH_STAGE_NOMINAL: i32 = 0;
pub const SYNTH_STAGE_DEGRADED: i32 = 1;
pub const SYNTH_STAGE_FAILING: i32 = 2;
pub const SYNTH_STAGE_CRITICAL: i32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SynthState {
    /// 0 NOMINAL / 1 DEGRADED / 2 FAILING / 3 CRITICAL. 0-2 are re-derived
    /// from HP% each chassis tick; 3 is sticky until repaired.
    #[serde(default)]
    pub malfunction_stage: i32,
    /// Unix timestamp of System Shutdown while critical. `None` = no
    /// countdown running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutdown_at: Option<i64>,
    /// Unix timestamp of the last chassis tick.
    #[serde(default)]
    pub last_chassis_tick: i64,
    /// Self-repair cooldown bookkeeping (unix timestamp of last `repair`).
    #[serde(default)]
    pub last_repair_time: i64,
    /// Unix timestamp the chassis was activated (creation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<i64>,
}

impl SynthState {
    /// Fresh state for a newly-activated synth.
    pub fn newly_activated(now: i64) -> Self {
        SynthState {
            last_chassis_tick: now,
            activated_at: Some(now),
            ..Default::default()
        }
    }

    pub fn is_critical(&self) -> bool {
        self.malfunction_stage >= SYNTH_STAGE_CRITICAL
    }

    /// Seconds of emergency reserve left, if a countdown is running.
    pub fn shutdown_remaining(&self, now: i64) -> Option<i64> {
        self.shutdown_at.map(|t| (t - now).max(0))
    }

    /// Display name for a malfunction stage.
    pub fn stage_label(stage: i32) -> &'static str {
        match stage {
            SYNTH_STAGE_DEGRADED => "DEGRADED",
            SYNTH_STAGE_FAILING => "FAILING",
            s if s >= SYNTH_STAGE_CRITICAL => "CRITICAL",
            _ => "NOMINAL",
        }
    }
}

/// Derive the non-sticky malfunction stage (0-2) from current HP.
pub fn synth_stage_for_hp(hp: i32, max_hp: i32) -> i32 {
    if max_hp <= 0 {
        return SYNTH_STAGE_NOMINAL;
    }
    let pct = (hp.max(0) * 100) / max_hp;
    if pct < SYNTH_STAGE_FAILING_HP_PCT {
        SYNTH_STAGE_FAILING
    } else if pct < SYNTH_STAGE_DEGRADED_HP_PCT {
        SYNTH_STAGE_DEGRADED
    } else {
        SYNTH_STAGE_NOMINAL
    }
}
