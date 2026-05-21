//! Morality tiers and "feel" line text. Centralized so Rust, Rhai, and DG Scripts
//! all see one source of truth for the slider bounds, tier thresholds, and flavor strings.
//!
//! The slider is stored on `CharacterData.morality` as `i32` and clamped to
//! `[MORALITY_MIN, MORALITY_MAX]` (i.e. -200..=+200). Tier classification, however,
//! anchors at `±EVIL_PURE_THRESHOLD` / `±GOOD_PURE_THRESHOLD` (i.e. ±100). This gap is
//! deliberate: it gives extreme alignment a "reputation buffer" so a single contrary
//! deed can't immediately flip an entrenched Pure Evil player back to a milder tier.

pub const MORALITY_MIN: i32 = -200;
pub const MORALITY_MAX: i32 = 200;
pub const EVIL_PURE_THRESHOLD: i32 = -100;
pub const GOOD_PURE_THRESHOLD: i32 = 100;
pub const NEUTRAL_BAND: i32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoralityTier {
    EvilPure,
    Evil3,
    Evil2,
    Evil1,
    Neutral,
    Good1,
    Good2,
    Good3,
    GoodPure,
}

impl MoralityTier {
    pub fn from_value(v: i32) -> Self {
        if v <= EVIL_PURE_THRESHOLD {
            Self::EvilPure
        } else if v <= -75 {
            Self::Evil3
        } else if v <= -50 {
            Self::Evil2
        } else if v <= -25 {
            Self::Evil1
        } else if v < 25 {
            Self::Neutral
        } else if v < 50 {
            Self::Good1
        } else if v < 75 {
            Self::Good2
        } else if v < GOOD_PURE_THRESHOLD {
            Self::Good3
        } else {
            Self::GoodPure
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::EvilPure => "evil_pure",
            Self::Evil3 => "evil_3",
            Self::Evil2 => "evil_2",
            Self::Evil1 => "evil_1",
            Self::Neutral => "neutral",
            Self::Good1 => "good_1",
            Self::Good2 => "good_2",
            Self::Good3 => "good_3",
            Self::GoodPure => "good_pure",
        }
    }

    pub fn is_good(self) -> bool {
        matches!(self, Self::Good1 | Self::Good2 | Self::Good3 | Self::GoodPure)
    }

    pub fn is_evil(self) -> bool {
        matches!(self, Self::Evil1 | Self::Evil2 | Self::Evil3 | Self::EvilPure)
    }

    pub fn is_neutral(self) -> bool {
        matches!(self, Self::Neutral)
    }
}

/// Returns the status-line "feel" message, or None for the Neutral tier.
pub fn feel_message(v: i32) -> Option<&'static str> {
    match MoralityTier::from_value(v) {
        MoralityTier::EvilPure => Some("You feel pure evil radiating from your very soul."),
        MoralityTier::Evil3 => Some("Darkness has taken root within you."),
        MoralityTier::Evil2 => Some("Cruelty comes naturally to you now."),
        MoralityTier::Evil1 => Some("You feel wickedness creeping into your bones."),
        MoralityTier::Neutral => None,
        MoralityTier::Good1 => Some("You feel a quiet warmth in your heart."),
        MoralityTier::Good2 => Some("Compassion comes easily to you."),
        MoralityTier::Good3 => Some("A bright virtue shines from within you."),
        MoralityTier::GoodPure => Some("You feel utterly pure of spirit."),
    }
}

/// Clamp a value into the legal morality range.
pub fn clamp(v: i32) -> i32 {
    v.clamp(MORALITY_MIN, MORALITY_MAX)
}

/// Add `delta` to `current` morality, clamped into the legal range.
/// Returns the new value. Pure — caller is responsible for writing it
/// back onto the character and persisting.
pub fn adjust(current: i32, delta: i32) -> i32 {
    clamp(current.saturating_add(delta))
}

/// Returns the tier-shift announcement line for a morality move from
/// `before` to `after`, or `None` if the move didn't cross a tier
/// boundary. Used to surface dramatic shifts (e.g. crossing into Good3
/// after a virtuous achievement) without spamming on small nudges.
pub fn tier_shift_message(before: i32, after: i32) -> Option<&'static str> {
    let from = MoralityTier::from_value(before);
    let to = MoralityTier::from_value(after);
    if from == to {
        return None;
    }
    feel_message(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries() {
        assert_eq!(MoralityTier::from_value(0), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(24), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(-24), MoralityTier::Neutral);
        assert_eq!(MoralityTier::from_value(25), MoralityTier::Good1);
        assert_eq!(MoralityTier::from_value(-25), MoralityTier::Evil1);
        assert_eq!(MoralityTier::from_value(49), MoralityTier::Good1);
        assert_eq!(MoralityTier::from_value(50), MoralityTier::Good2);
        assert_eq!(MoralityTier::from_value(74), MoralityTier::Good2);
        assert_eq!(MoralityTier::from_value(75), MoralityTier::Good3);
        assert_eq!(MoralityTier::from_value(99), MoralityTier::Good3);
        assert_eq!(MoralityTier::from_value(100), MoralityTier::GoodPure);
        assert_eq!(MoralityTier::from_value(200), MoralityTier::GoodPure);
        assert_eq!(MoralityTier::from_value(-100), MoralityTier::EvilPure);
        assert_eq!(MoralityTier::from_value(-150), MoralityTier::EvilPure);
        assert_eq!(MoralityTier::from_value(-99), MoralityTier::Evil3);
        assert_eq!(MoralityTier::from_value(-50), MoralityTier::Evil2);
        assert_eq!(MoralityTier::from_value(-49), MoralityTier::Evil1);
    }

    #[test]
    fn neutral_has_no_feel_message() {
        assert!(feel_message(0).is_none());
        assert!(feel_message(24).is_none());
        assert!(feel_message(-24).is_none());
    }

    #[test]
    fn extremes_get_pure_feel() {
        assert!(feel_message(100).unwrap().contains("pure"));
        assert!(feel_message(-100).unwrap().contains("pure evil"));
    }

    #[test]
    fn clamp_respects_bounds() {
        assert_eq!(clamp(500), 200);
        assert_eq!(clamp(-500), -200);
        assert_eq!(clamp(50), 50);
        assert_eq!(clamp(-50), -50);
    }

    #[test]
    fn sticky_extreme_buffer() {
        // -150 is Pure Evil; +30 leaves us at -120 = still Pure Evil.
        assert_eq!(MoralityTier::from_value(-150 + 30), MoralityTier::EvilPure);
        // Another +30 puts us at -90 = Evil3.
        assert_eq!(MoralityTier::from_value(-90), MoralityTier::Evil3);
    }
}
