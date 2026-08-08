//! Divine favor tiers and their announcement text. Centralised so Rust, Rhai
//! and DG Scripts all read one ladder, the same way `crate::morality` serves
//! the alignment slider.
//!
//! Favor used to surface as a bare integer — `Favor: 47` — which tells a
//! player a number moved but not what it bought them. A named tier is
//! legible without a wiki: "Favored" means something, 47 does not.
//!
//! Unlike morality, favor is **not** clamped. Morality is a bounded slider by
//! design; favor is an earned currency with no ceiling in the fiction, and a
//! clamp would silently rewrite whatever DG scripts and builder worlds have
//! already accumulated. The ladder simply tops out at Exalted.
//!
//! The bands are asymmetric on purpose. Favor starts at 0 and normally only
//! rises (+5 per enemy minion, +25 per enemy worshiper); it goes negative
//! only through faith offences, which are deliberate acts. So the negative
//! side is short and steep, and the positive side is long.

/// The seven bands, ascending. Thresholds and text live here and nowhere
/// else; [`FavorTier`] derives both from this by index.
pub const LADDER: crate::tiers::TierLadder = crate::tiers::TierLadder {
    tiers: &[
        crate::tiers::Tier {
            key: "anathema",
            label: "Anathema",
            description: "You are anathema. No offering of yours will be accepted.",
            floor: i32::MIN,
        },
        crate::tiers::Tier {
            key: "disfavored",
            label: "Disfavored",
            description: "Your god's regard has soured against you.",
            floor: -99,
        },
        crate::tiers::Tier {
            key: "unproven",
            label: "Unproven",
            description: "You stand unproven before your god.",
            floor: -24,
        },
        crate::tiers::Tier {
            key: "noticed",
            label: "Noticed",
            description: "Your god has taken note of you.",
            floor: 25,
        },
        crate::tiers::Tier {
            key: "favored",
            label: "Favored",
            description: "Your god looks upon you with favor.",
            floor: 100,
        },
        crate::tiers::Tier {
            key: "blessed",
            label: "Blessed",
            description: "Your god's blessing rests upon you.",
            floor: 250,
        },
        crate::tiers::Tier {
            key: "exalted",
            label: "Exalted",
            description: "You stand among your god's exalted.",
            floor: 500,
        },
    ],
};

/// Where a favor value sits on the ladder. Variant order must match
/// [`LADDER`] — `FavorTier as usize` indexes straight into it, and `Ord` is
/// load-bearing for the rise/fall wording. Pinned by
/// `enum_order_matches_the_ladder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FavorTier {
    Anathema,
    Disfavored,
    Unproven,
    Noticed,
    Favored,
    Blessed,
    Exalted,
}

const FAVOR_TIERS: [FavorTier; 7] = [
    FavorTier::Anathema,
    FavorTier::Disfavored,
    FavorTier::Unproven,
    FavorTier::Noticed,
    FavorTier::Favored,
    FavorTier::Blessed,
    FavorTier::Exalted,
];

impl FavorTier {
    pub fn from_value(v: i32) -> Self {
        FAVOR_TIERS[LADDER.index_of(v)]
    }

    /// Stable snake_case identifier for scripts and conditions.
    pub fn key(self) -> &'static str {
        LADDER.tiers[self as usize].key
    }

    /// Player-facing name.
    pub fn label(self) -> &'static str {
        LADDER.tiers[self as usize].label
    }

    /// Parse a [`key`](Self::key). `None` for anything unrecognised, so a
    /// mistyped setting refuses the gate rather than silently opening it.
    pub fn from_key(key: &str) -> Option<Self> {
        let lc = key.trim().to_lowercase();
        FAVOR_TIERS.into_iter().find(|t| t.key() == lc)
    }

    /// One line describing the standing, phrased so it reads correctly
    /// whether the player just rose into the tier or fell into it.
    pub fn description(self) -> &'static str {
        LADDER.tiers[self as usize].description
    }
}

/// The standing as shown on `worship` and `examine`: the tier name with the
/// raw score in parentheses. The number stays because players who are
/// optimising want it; the name leads because everyone else needs it.
pub fn standing_line(favor: i32) -> String {
    LADDER.standing_line(favor)
}

/// Announcement for a favor move that crossed a tier boundary, or `None` when
/// it did not. Mirrors `morality::tier_shift_message`: small nudges stay
/// silent so the flow of combat isn't interrupted by every +5.
///
/// `god_name` may be empty; the caller does not have to resolve it.
pub fn tier_shift_message(god_name: &str, before: i32, after: i32) -> Option<String> {
    let god = if god_name.is_empty() { "Your god" } else { god_name };
    LADDER.shift_message(god, "raises you to", "casts you down to", before, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boundaries() {
        assert_eq!(FavorTier::from_value(0), FavorTier::Unproven);
        assert_eq!(FavorTier::from_value(24), FavorTier::Unproven);
        assert_eq!(FavorTier::from_value(-24), FavorTier::Unproven);
        assert_eq!(FavorTier::from_value(25), FavorTier::Noticed);
        assert_eq!(FavorTier::from_value(99), FavorTier::Noticed);
        assert_eq!(FavorTier::from_value(100), FavorTier::Favored);
        assert_eq!(FavorTier::from_value(249), FavorTier::Favored);
        assert_eq!(FavorTier::from_value(250), FavorTier::Blessed);
        assert_eq!(FavorTier::from_value(499), FavorTier::Blessed);
        assert_eq!(FavorTier::from_value(500), FavorTier::Exalted);
        assert_eq!(FavorTier::from_value(i32::MAX), FavorTier::Exalted);
        assert_eq!(FavorTier::from_value(-25), FavorTier::Disfavored);
        assert_eq!(FavorTier::from_value(-99), FavorTier::Disfavored);
        assert_eq!(FavorTier::from_value(-100), FavorTier::Anathema);
        assert_eq!(FavorTier::from_value(i32::MIN), FavorTier::Anathema);
    }

    #[test]
    fn enum_order_matches_the_ladder() {
        // `FavorTier as usize` indexes straight into LADDER, so a variant
        // inserted out of position would silently relabel every tier above it.
        assert_eq!(FAVOR_TIERS.len(), LADDER.tiers.len());
        for (i, t) in FAVOR_TIERS.iter().enumerate() {
            assert_eq!(*t as usize, i, "variant {:?} is out of position", t);
            assert_eq!(t.key(), LADDER.tiers[i].key);
        }
    }

    #[test]
    fn keys_and_labels_are_unique() {
        let tiers = [
            FavorTier::Anathema,
            FavorTier::Disfavored,
            FavorTier::Unproven,
            FavorTier::Noticed,
            FavorTier::Favored,
            FavorTier::Blessed,
            FavorTier::Exalted,
        ];
        let keys: std::collections::HashSet<_> = tiers.iter().map(|t| t.key()).collect();
        let labels: std::collections::HashSet<_> = tiers.iter().map(|t| t.label()).collect();
        assert_eq!(keys.len(), tiers.len());
        assert_eq!(labels.len(), tiers.len());
    }

    #[test]
    fn ordering_matches_the_ladder() {
        // The rise/fall wording depends on Ord, so the variant order and the
        // numeric order must not drift apart.
        assert!(FavorTier::from_value(-200) < FavorTier::from_value(0));
        assert!(FavorTier::from_value(0) < FavorTier::from_value(600));
    }

    #[test]
    fn a_move_inside_one_band_says_nothing() {
        // +5 per minion kill must not announce on every kill.
        assert!(tier_shift_message("Kaleth", 30, 35).is_none());
        assert!(tier_shift_message("Kaleth", 0, 24).is_none());
    }

    #[test]
    fn crossing_up_announces_a_rise() {
        let msg = tier_shift_message("Kaleth", 95, 105).expect("crossed into Favored");
        assert!(msg.contains("Kaleth raises you to Favored."));
        assert!(msg.contains("looks upon you with favor"));
    }

    #[test]
    fn crossing_down_announces_a_fall() {
        let msg = tier_shift_message("Kaleth", 30, -30).expect("crossed into Disfavored");
        assert!(msg.contains("Kaleth casts you down to Disfavored."));
    }

    #[test]
    fn a_nameless_god_still_reads() {
        let msg = tier_shift_message("", 0, 30).expect("crossed into Noticed");
        assert!(msg.starts_with("\x1b[1;32mYour god raises you to Noticed."));
    }

    #[test]
    fn standing_keeps_the_number_for_optimisers() {
        assert_eq!(standing_line(127), "Favored (127)");
        assert_eq!(standing_line(0), "Unproven (0)");
        assert_eq!(standing_line(-140), "Anathema (-140)");
    }

    #[test]
    fn every_key_round_trips() {
        for tier in FAVOR_TIERS {
            assert_eq!(FavorTier::from_key(tier.key()), Some(tier), "{}", tier.key());
        }
        assert_eq!(FavorTier::from_key(" Favored "), Some(FavorTier::Favored));
    }

    /// A gate whose threshold is misspelled must stay shut. `worship_favor_at_least`
    /// refuses on `None`, so this is what stops a typo in a script or a setting
    /// from quietly granting everyone the ability it was meant to gate.
    #[test]
    fn an_unknown_key_does_not_parse() {
        assert_eq!(FavorTier::from_key("beloved"), None);
        assert_eq!(FavorTier::from_key(""), None);
    }

    /// The gate comparison relies on `Ord` matching the ladder, which is
    /// already pinned; this is the property the gate itself depends on.
    #[test]
    fn tiers_order_from_anathema_up() {
        assert!(FavorTier::from_value(600) >= FavorTier::Noticed);
        assert!(FavorTier::from_value(30) >= FavorTier::Noticed);
        assert!(FavorTier::from_value(0) < FavorTier::Noticed);
        assert!(FavorTier::from_value(-500) < FavorTier::Noticed);
    }
}
