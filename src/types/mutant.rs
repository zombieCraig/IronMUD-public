//! Mutant-specific runtime state carried by `CharacterData`.
//!
//! Models Mutant: Year Zero's push/mutation-point economy: mutant power runs
//! on self-harm. The `push` command trades immediate HP trauma for Mutation
//! Points (MP) plus a short exertion surge; mutation powers spend MP, and
//! every MP spent adds a misfire die — misfires range from losing the pool to
//! permanent attribute loss with a brand-new uncontrolled mutation (Overload).
//! MP has NO passive regeneration: pushing (and the Rot-Eater mutation) are
//! the only sources.
//!
//! Each mutant is rolled exactly ONE random mutation at creation; further
//! mutations arrive only through Overload misfires. The character degenerates
//! into something less human as they lean on their power — that arc is the
//! point.
//!
//! The Rot (zone contamination) is deliberately NOT stored here: rot points
//! live directly on `CharacterData` because every race accumulates them.
//! Mutants merely interact with the Rot on better terms (half gain rate, half
//! damage; Rot-Eaters metabolize it into MP). See `crate::mutant` for the
//! tick logic.
//!
//! The struct lives behind `Option<MutantState>` on `CharacterData`: `None`
//! means "not a mutant", mirroring `ReplicantState` / `VampireState`.

use serde::{Deserialize, Serialize};

pub const MUTANT_DEFAULT_MAX_MP: i32 = 10;
/// Cadence of the mutation tick (passive buff/trait re-assertion only — MP
/// never regenerates on its own).
pub const MUTATION_TICK_INTERVAL_SECS: u64 = 60;
/// Cadence of the world rot tick (gain/decay/damage for ALL races).
pub const ROT_TICK_INTERVAL_SECS: u64 = 60;
/// Mutant `push`: cooldown between pushes.
pub const PUSH_COOLDOWN_SECS: i64 = 60;
/// Duration of the "Pushing" exertion surge buffs.
pub const PUSH_BUFF_SECS: i64 = 30;
/// Pushing is refused below this % of max HP — the body has nothing left.
pub const PUSH_MIN_HP_PCT: i32 = 15;
/// Seconds between rot-point gains per room rot level (index 0 unused).
pub const ROT_GAIN_SECS_BY_LEVEL: [i64; 4] = [0, 300, 120, 60];
/// In a rot-free room, shed 1 rot point per this interval.
pub const ROT_DECAY_INTERVAL_SECS: i64 = 600;
/// Chance (percent) that a shed rot point becomes permanent instead.
pub const ROT_PERMANENT_CHANCE_PCT: i32 = 10;
/// Mutants gain rot at 1/Nth the rate (interval multiplied by this).
pub const MUTANT_ROT_GAIN_DIVISOR: i32 = 2;
/// Mutants take 1/Nth rot damage (rounded down, min 0).
pub const MUTANT_ROT_DAMAGE_DIVISOR: i32 = 2;
/// Highest room rot level (0 none / 1 weak / 2 heavy / 3 hotspot).
pub const ROT_LEVEL_MAX: i32 = 3;

pub const MISFIRE_KIND_POWER_LOSS: &str = "power_loss";
pub const MISFIRE_KIND_SELF_TRAUMA: &str = "self_trauma";
pub const MISFIRE_KIND_DEFORMITY: &str = "deformity";
pub const MISFIRE_KIND_OVERLOAD: &str = "overload";

/// Cosmetic deformities handed out by misfires. Pure flavor: surfaced in
/// `score` and available to builders for dialogue keying.
pub const DEFORMITIES: [&str; 12] = [
    "a patch of glistening scales across one cheek",
    "a milky, lidless third eye on the temple that never blinks",
    "knuckles split by short bone spurs",
    "skin that peels in grey, papery flakes",
    "one arm webbed to the ribs by a fan of translucent skin",
    "a cluster of vestigial fingers sprouting from one wrist",
    "hair replaced by fine, colorless quills",
    "veins that glow faintly green in the dark",
    "a lipless mouth of too many small teeth",
    "ears fused to ragged nubs",
    "fingernails grown into dark, curved talons",
    "a tail-stub that twitches when the Rot is near",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutantState {
    /// Mutation Point pool. Spent on powers; earned ONLY by pushing (and the
    /// Rot-Eater mutation). 0 means the power sleeps until you bleed for it.
    #[serde(default = "default_mp")]
    pub mp: i32,
    #[serde(default = "default_mp")]
    pub max_mp: i32,
    /// Owned mutation ids (see `scripts/data/mutations.json`). Exactly one at
    /// creation; Overload misfires append more.
    #[serde(default)]
    pub mutations: Vec<String>,
    /// Cosmetic deformities accumulated from misfires.
    #[serde(default)]
    pub deformities: Vec<String>,
    /// Unix timestamp the character became "of the People".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_time: Option<i64>,
}

fn default_mp() -> i32 {
    MUTANT_DEFAULT_MAX_MP
}

impl Default for MutantState {
    fn default() -> Self {
        MutantState {
            mp: MUTANT_DEFAULT_MAX_MP,
            max_mp: MUTANT_DEFAULT_MAX_MP,
            mutations: Vec::new(),
            deformities: Vec::new(),
            mutation_time: None,
        }
    }
}

impl MutantState {
    /// Fresh state for a newly-created mutant (mutation rolled separately).
    pub fn newly_mutated(now: i64) -> Self {
        MutantState {
            mutation_time: Some(now),
            ..Default::default()
        }
    }

    /// Clamp MP to [0, max_mp], returning the new value.
    pub fn set_mp(&mut self, value: i32) -> i32 {
        self.mp = value.clamp(0, self.max_mp);
        self.mp
    }

    /// Add `delta` to MP, clamped to [0, max_mp]. Returns the new value.
    pub fn change_mp(&mut self, delta: i32) -> i32 {
        self.set_mp(self.mp.saturating_add(delta))
    }

    pub fn has_mutation(&self, id: &str) -> bool {
        self.mutations.iter().any(|m| m == id)
    }
}

/// Push severity: how hard the body is strained, 1..=3 ("biohazard" scale).
/// HP cost and MP gain both scale with it. `roll_1d3` is the caller's die.
pub fn push_severity(roll_1d3: i32) -> i32 {
    roll_1d3.clamp(1, 3)
}

/// HP cost of a push at `severity` (1-3): 5/10/15% of max HP, floor 1.
pub fn push_hp_cost(max_hp: i32, severity: i32) -> i32 {
    (max_hp * 5 * severity.clamp(1, 3) / 100).max(1)
}

/// Misfire check for a mutation activation that spent `mp_spent` MP.
/// `dice` must hold `mp_spent` d6 rolls; any 1 misfires. The FIRST 1 found
/// triggers it (matching MYZ: one misfire per activation).
pub fn misfire_occurred(dice: &[i32]) -> bool {
    dice.iter().any(|&d| d == 1)
}

/// Map a 1d6 severity roll onto the misfire table.
/// 1-2 power loss, 3-4 self trauma, 5 deformity, 6 overload.
pub fn roll_misfire_kind(roll_1d6: i32) -> &'static str {
    match roll_1d6 {
        1 | 2 => MISFIRE_KIND_POWER_LOSS,
        3 | 4 => MISFIRE_KIND_SELF_TRAUMA,
        5 => MISFIRE_KIND_DEFORMITY,
        _ => MISFIRE_KIND_OVERLOAD,
    }
}

/// Rot damage on a gain: roll one d6 per total rot point; each 1 deals 1
/// damage. `dice` must hold `total_rot` d6 rolls. Snowballs with exposure.
pub fn rot_damage_from_dice(dice: &[i32]) -> i32 {
    dice.iter().filter(|&&d| d == 1).count() as i32
}

/// Effective seconds between rot gains for a character in a room of
/// `rot_level`, accounting for the mutant divisor. Returns None when the
/// room is rot-free or the level is out of range.
pub fn rot_gain_interval_secs(rot_level: i32, is_mutant: bool) -> Option<i64> {
    if !(1..=ROT_LEVEL_MAX).contains(&rot_level) {
        return None;
    }
    let base = ROT_GAIN_SECS_BY_LEVEL[rot_level as usize];
    Some(if is_mutant {
        base * MUTANT_ROT_GAIN_DIVISOR as i64
    } else {
        base
    })
}
