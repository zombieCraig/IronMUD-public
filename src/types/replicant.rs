//! Replicant-specific runtime state carried by `CharacterData`.
//!
//! Models the Blade Runner RPG's two-track damage idea: the body is engineered
//! and tireless (replicants pay no stamina anywhere — see the pinned `stamina`
//! property setter in `src/script/mod.rs`), but the mind tracks **Resolve**, a
//! mental-stress pool drained by combat trauma, pushing past limits, and
//! grief. At 0 Resolve the replicant suffers a breakdown (berserk / lockup /
//! panic). Recovery is slow and deliberate: sleep trickle, being comforted by
//! others, focusing on a bonded signature item in a safe zone — and the only
//! full restore is passing a baseline test at a `baseline_office` room.
//!
//! Everything that maps onto existing IronMUD systems lives there:
//! - Breakdown side-effects ride the existing buff system (`Frenzy`, stat
//!   debuffs) and `combat.stun_rounds_remaining`.
//! - The retirement order is a regular trait (`retirement_order`) builders can
//!   key dialogue/hunters on, plus a 24h all-stats debuff buff.
//!
//! The struct lives behind `Option<ReplicantState>` on `CharacterData`: `None`
//! means "not a replicant", mirroring `VampireState`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const REPLICANT_DEFAULT_MAX_RESOLVE: i32 = 10;
/// Cadence of the resolve tick (breakdown expiry + sleep regen).
pub const RESOLVE_TICK_INTERVAL_SECS: u64 = 60;
/// Resolve regained per tick while sleeping (and not in breakdown).
pub const RESOLVE_REGEN_SLEEPING: i32 = 1;
/// Default resolve cost of the `push` command (overridable via the
/// `resolve_cost_push` setting).
pub const PUSH_RESOLVE_COST: i32 = 2;
/// Resolve restored by `focus` on a bonded signature item.
pub const FOCUS_RESOLVE_RESTORE: i32 = 2;
pub const FOCUS_COOLDOWN_SECS: i64 = 600;
/// Resolve restored when someone `comfort`s a replicant.
pub const COMFORT_RESOLVE_RESTORE: i32 = 1;
/// Per-RECIPIENT cooldown: N friends cannot stack comforts.
pub const COMFORT_RECEIVE_COOLDOWN_SECS: i64 = 300;
/// Real-time seconds before a newly attuned signature item is bonded enough
/// for `focus` to work.
pub const ATTUNE_BOND_SECS: i64 = 86_400;
/// Grief: resolve lost when replacing a previous attunement.
pub const ATTUNE_GRIEF_RESOLVE_COST: i32 = 2;
/// Retry lockout after a failed baseline test.
pub const BASELINE_FAIL_COOLDOWN_SECS: i64 = 14_400;
/// Resolve snaps back to this after a breakdown resolves so the player is not
/// chain-broken.
pub const BREAKDOWN_RESET_RESOLVE: i32 = 3;
pub const BREAKDOWN_DURATION_SECS: i64 = 60;
/// A single hit dealing at least this % of max HP drains 1 resolve.
pub const BIG_HIT_RESOLVE_THRESHOLD_PCT: i32 = 15;
/// Baseline success chance = clamp(BASE + resolve*PER_RESOLVE
/// - breakdowns_since_baseline*BREAKDOWN_PENALTY, MIN..=MAX).
pub const BASELINE_BASE_CHANCE: i32 = 40;
pub const BASELINE_PER_RESOLVE: i32 = 6;
pub const BASELINE_BREAKDOWN_PENALTY: i32 = 15;
pub const BASELINE_CHANCE_MIN: i32 = 5;
pub const BASELINE_CHANCE_MAX: i32 = 95;
pub const STRIKES_FOR_RETIREMENT: i32 = 3;
/// Engineered vigor: HP regen bonus (percent) on top of the normal regen.
pub const REPLICANT_HP_REGEN_BONUS_PCT: i32 = 50;
/// Duration of the recalibration debuff applied on retirement.
pub const RETIREMENT_DEBUFF_SECS: i64 = 86_400;
/// All six stats are debuffed by this much during recalibration.
pub const RETIREMENT_STAT_PENALTY: i32 = 2;
/// Trait stamped on retirement; builders key hunters/dialogue on it.
pub const RETIREMENT_TRAIT: &str = "retirement_order";

pub const BREAKDOWN_KIND_PANIC: &str = "panic";
pub const BREAKDOWN_KIND_LOCKUP: &str = "lockup";
pub const BREAKDOWN_KIND_BERSERK: &str = "berserk";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicantState {
    /// Current mental-stress pool. 0 triggers a breakdown.
    #[serde(default = "default_resolve")]
    pub resolve: i32,
    #[serde(default = "default_resolve")]
    pub max_resolve: i32,
    /// Failed baseline tests. Hitting STRIKES_FOR_RETIREMENT triggers a
    /// retirement order (24h recalibration debuff + trait), then resets.
    #[serde(default)]
    pub baseline_strikes: i32,
    /// Breakdowns suffered since the last PASSED baseline test; penalizes the
    /// next test's success chance.
    #[serde(default)]
    pub breakdowns_since_baseline: i32,
    /// Lifetime count of retirement orders issued (lore/progression hook).
    #[serde(default)]
    pub retirement_count: i32,
    /// The bonded memento. `focus` requires it present and bonded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_item_id: Option<Uuid>,
    /// Unix timestamp of the current attunement; the bond matures
    /// ATTUNE_BOND_SECS later.
    #[serde(default)]
    pub attuned_at: i64,
    #[serde(default)]
    pub last_focus_time: i64,
    /// Recipient-side comfort cooldown (unix timestamp).
    #[serde(default)]
    pub comfort_cooldown_until: i64,
    /// Failed-baseline retry lockout (unix timestamp).
    #[serde(default)]
    pub baseline_cooldown_until: i64,
    /// Unix timestamp at which the active breakdown ends. `None` = stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breakdown_until: Option<i64>,
    /// "panic" | "lockup" | "berserk" while a breakdown is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breakdown_kind: Option<String>,
    /// Unix timestamp of the last resolve tick (sleep regen pacing).
    #[serde(default)]
    pub last_resolve_tick: i64,
    /// Unix timestamp the character became a replicant ("inception").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inception_time: Option<i64>,
}

fn default_resolve() -> i32 {
    REPLICANT_DEFAULT_MAX_RESOLVE
}

impl Default for ReplicantState {
    fn default() -> Self {
        ReplicantState {
            resolve: REPLICANT_DEFAULT_MAX_RESOLVE,
            max_resolve: REPLICANT_DEFAULT_MAX_RESOLVE,
            baseline_strikes: 0,
            breakdowns_since_baseline: 0,
            retirement_count: 0,
            signature_item_id: None,
            attuned_at: 0,
            last_focus_time: 0,
            comfort_cooldown_until: 0,
            baseline_cooldown_until: 0,
            breakdown_until: None,
            breakdown_kind: None,
            last_resolve_tick: 0,
            inception_time: None,
        }
    }
}

impl ReplicantState {
    /// Fresh state for a newly-incepted replicant.
    pub fn newly_incepted(now: i64) -> Self {
        ReplicantState {
            last_resolve_tick: now,
            inception_time: Some(now),
            ..Default::default()
        }
    }

    /// Clamp resolve to [0, max_resolve], returning the new value.
    pub fn set_resolve(&mut self, value: i32) -> i32 {
        self.resolve = value.clamp(0, self.max_resolve);
        self.resolve
    }

    /// Add `delta` to resolve, clamped to [0, max_resolve]. Returns the new
    /// value. Callers decide whether hitting 0 triggers a breakdown.
    pub fn change_resolve(&mut self, delta: i32) -> i32 {
        self.set_resolve(self.resolve.saturating_add(delta))
    }

    /// True while a breakdown is active relative to `now`.
    pub fn is_breaking_down(&self, now: i64) -> bool {
        self.breakdown_until.map(|t| t > now).unwrap_or(false)
    }

    /// True when a signature item is attuned and the bond has matured.
    pub fn is_signature_bonded(&self, now: i64) -> bool {
        self.signature_item_id.is_some() && now >= self.attuned_at + ATTUNE_BOND_SECS
    }

    /// Success chance (percent) for a baseline test at the current state.
    pub fn baseline_success_chance(&self) -> i32 {
        (BASELINE_BASE_CHANCE + self.resolve * BASELINE_PER_RESOLVE
            - self.breakdowns_since_baseline * BASELINE_BREAKDOWN_PENALTY)
            .clamp(BASELINE_CHANCE_MIN, BASELINE_CHANCE_MAX)
    }
}

/// Map a 1d6 roll onto the replicant critical-stress table.
/// 1-2 panic, 3-4 lockup, 5-6 berserk ("loses its mind and starts killing").
pub fn roll_breakdown(roll_1d6: i32) -> &'static str {
    match roll_1d6 {
        1 | 2 => BREAKDOWN_KIND_PANIC,
        3 | 4 => BREAKDOWN_KIND_LOCKUP,
        _ => BREAKDOWN_KIND_BERSERK,
    }
}
