//! Vampire-specific runtime state shared by `CharacterData` and `MobileData`.
//!
//! Carries scalar resources and timers that are unique to vampirism (blood
//! pool, humanity, masquerade, frenzy timer, sire). Everything that maps onto
//! existing IronMUD systems lives there:
//!
//! - **Clan** is a granted trait (`clan_brujah`, `clan_toreador`, …) on the
//!   character/mobile, read by the existing trait-checking utilities.
//! - **Disciplines** (Dominate, Auspex, Celerity, Potence, Obfuscate) are
//!   regular skills in `CharacterData.skills`. Each discipline-spell gates on
//!   `skill_required` in the same way Magic gates `magic_missile`.
//! - **Sun damage** and **blood drain** ride on `EffectType::SunlightBurn`,
//!   `EffectType::SunlightBurning`, and the existing `OnHitEffect` pipeline.
//!
//! The struct lives behind `Option<VampireState>` on the host record: `None`
//! means "mortal", `Some` means "kindred". This avoids paying any cost on the
//! 99% of records that don't care about vampirism.
//!
//! See `docs/vampire-implementation.md` (TODO) and the design plan at
//! `~/.claude/plans/i-would-like-to-precious-stardust.md` for the broader
//! architecture and rationale.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEFAULT_BLOOD_POOL: i32 = 10;
pub const DEFAULT_HUMANITY: i32 = 7;
pub const HUMANITY_MIN: i32 = 0;
pub const HUMANITY_MAX: i32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VampireState {
    /// Current blood points. Spent by disciplines, restored by feeding,
    /// decays slowly each blood tick.
    #[serde(default = "default_blood")]
    pub blood_pool: i32,
    #[serde(default = "default_blood")]
    pub max_blood_pool: i32,
    /// Humanity 0–10. Dropped by lethal feeds and masquerade breaks; gained
    /// from humanity quests (Phase 2). Below 3 locks compassion-anchored
    /// disciplines; below 1 forces frenzy on low blood.
    #[serde(default = "default_humanity")]
    pub humanity: i32,
    /// Set true when the kindred has publicly broken the masquerade. Mortal
    /// NPCs in-area react with hostility/flight; the cue surfaces in
    /// `examine`. Per-area witness ledger is Phase 2.
    #[serde(default)]
    pub masquerade_broken: bool,
    /// Unix timestamp of the last blood-tick decay. Mirrors the
    /// `last_thirst_tick` pattern.
    #[serde(default)]
    pub last_blood_tick: i64,
    /// Unix timestamp at which active frenzy ends. `None` = not currently
    /// frenzying. Set by `apply_frenzy`, read by combat tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frenzy_until: Option<i64>,
    /// Unix timestamp of the embrace itself, used for "newly embraced"
    /// dialogue cues and future age-of-kindred mechanics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embrace_time: Option<i64>,
    /// PC name or mob UUID of the sire. Free-form string so quest scripts
    /// can store either. `None` = unknown sire (admin-embraced testing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sire_id: Option<String>,
}

fn default_blood() -> i32 {
    DEFAULT_BLOOD_POOL
}

fn default_humanity() -> i32 {
    DEFAULT_HUMANITY
}

impl Default for VampireState {
    fn default() -> Self {
        VampireState {
            blood_pool: DEFAULT_BLOOD_POOL,
            max_blood_pool: DEFAULT_BLOOD_POOL,
            humanity: DEFAULT_HUMANITY,
            masquerade_broken: false,
            last_blood_tick: 0,
            frenzy_until: None,
            embrace_time: None,
            sire_id: None,
        }
    }
}

impl VampireState {
    /// Fresh state for a newly-embraced kindred. `now` is the unix timestamp
    /// of the embrace; `sire` identifies the sire (PC name or mob UUID
    /// stringified) and is optional for admin/testing flows.
    pub fn newly_embraced(now: i64, sire: Option<String>) -> Self {
        VampireState {
            blood_pool: DEFAULT_BLOOD_POOL,
            max_blood_pool: DEFAULT_BLOOD_POOL,
            humanity: DEFAULT_HUMANITY,
            masquerade_broken: false,
            last_blood_tick: now,
            frenzy_until: None,
            embrace_time: Some(now),
            sire_id: sire,
        }
    }

    /// Clamp humanity to the [0, 10] range, returning the new value.
    pub fn set_humanity(&mut self, value: i32) -> i32 {
        self.humanity = value.clamp(HUMANITY_MIN, HUMANITY_MAX);
        self.humanity
    }

    /// Add `delta` to humanity, clamped to [0, 10]. Returns the new value.
    pub fn change_humanity(&mut self, delta: i32) -> i32 {
        self.set_humanity(self.humanity.saturating_add(delta))
    }

    /// True when frenzy_until is in the future relative to `now`. Convenience
    /// for combat-tick gates.
    pub fn is_frenzying(&self, now: i64) -> bool {
        self.frenzy_until.map(|t| t > now).unwrap_or(false)
    }
}

/// Type-fence helper: convert a sire UUID into the stringified form stored
/// on `VampireState.sire_id`. Round-trips through `parse_sire_uuid`.
pub fn stringify_sire_uuid(id: Uuid) -> String {
    id.to_string()
}

/// Best-effort parse of a stringified sire id back into a UUID. Returns None
/// for free-form names (in which case callers should treat the value as a PC
/// name lookup key).
pub fn parse_sire_uuid(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}
