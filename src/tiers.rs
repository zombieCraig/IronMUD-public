//! A named-tier ladder: thresholds, labels, and the "you crossed a boundary"
//! announcement, written once.
//!
//! Three systems in this codebase turn a signed integer into a standing the
//! player can read: morality (`crate::morality`), divine favor
//! (`crate::worship::favor`), and faction reputation
//! (`crate::reputation`). They were two hand-copied implementations of the
//! same shape before the third arrived; this module is that shape.
//!
//! The design rule they share is worth stating, because it is the whole point
//! of naming tiers at all: **a move inside a band says nothing, and a move
//! across one announces itself.** Raw numbers tell a player that something
//! changed without telling them what it bought — "Favored" means something,
//! `47` does not — and announcing every `+5` turns a combat round into a
//! ticker. Bands buy both: the number stays available for players who want it,
//! and the game only interrupts when the answer to "where do I stand?" has
//! actually changed.
//!
//! A ladder is a `&'static [Tier]` in ascending order. Each tier carries the
//! inclusive value at which it *starts*; the first tier's `floor` is ignored,
//! since it catches everything below the second. Consumers keep their own enum
//! so they can still pattern-match (`is_evil`, `Ord` comparisons), and derive
//! the thresholds and strings from here — with a test pinning the enum's
//! variant order to the ladder's, since the two must not drift.

/// One rung. `key` is the stable snake_case identifier scripts and conditions
/// use; `label` is what the player reads; `description` is the one-line
/// explanation, phrased so it reads correctly whether the player just rose
/// into this tier or fell into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Lowest value belonging to this tier, inclusive. Ignored for the first
    /// entry, which is the floor of the whole ladder.
    pub floor: i32,
}

/// An ordered set of tiers. Construct as a `const` with the rungs ascending.
#[derive(Debug, Clone, Copy)]
pub struct TierLadder {
    pub tiers: &'static [Tier],
}

impl TierLadder {
    /// Index of the tier `v` falls in. Saturates at both ends, so there is
    /// always an answer — an unclamped value simply reads as the extreme.
    pub fn index_of(&self, v: i32) -> usize {
        // Walk down from the top: the first rung whose floor we meet is ours.
        // Index 0 is the catch-all, so the loop can stop before it.
        for i in (1..self.tiers.len()).rev() {
            if v >= self.tiers[i].floor {
                return i;
            }
        }
        0
    }

    pub fn tier_of(&self, v: i32) -> &'static Tier {
        &self.tiers[self.index_of(v)]
    }

    /// The tier `after` lands in, but only if it differs from `before`'s.
    /// `None` means the move stayed inside one band and should stay silent.
    pub fn crossed(&self, before: i32, after: i32) -> Option<&'static Tier> {
        let from = self.index_of(before);
        let to = self.index_of(after);
        if from == to {
            return None;
        }
        Some(&self.tiers[to])
    }

    /// Did the move go up the ladder? Only meaningful alongside [`crossed`].
    pub fn rose(&self, before: i32, after: i32) -> bool {
        self.index_of(after) > self.index_of(before)
    }

    /// The standing as shown on a status surface: the tier name with the raw
    /// score in parentheses. The number stays because players who are
    /// optimising want it; the name leads because everyone else needs it.
    pub fn standing_line(&self, v: i32) -> String {
        format!("{} ({})", self.tier_of(v).label, v)
    }

    /// Announcement for a move that crossed a boundary, or `None` when it did
    /// not.
    ///
    /// `subject` is whoever is doing the judging ("Kaleth", "The Iron Guard").
    /// `rise` and `fall` are the verb phrases that join it to the new tier
    /// name — "raises you to" / "casts you down to". Green for a rise, red for
    /// a fall, with the tier's description on the following line.
    pub fn shift_message(&self, subject: &str, rise: &str, fall: &str, before: i32, after: i32) -> Option<String> {
        let to = self.crossed(before, after)?;
        let up = self.rose(before, after);
        let verb = if up { rise } else { fall };
        let color = if up { "\x1b[1;32m" } else { "\x1b[1;31m" };
        Some(format!(
            "{}{} {} {}.\x1b[0m\n  {}",
            color, subject, verb, to.label, to.description
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: TierLadder = TierLadder {
        tiers: &[
            Tier {
                key: "low",
                label: "Low",
                description: "At the bottom.",
                floor: i32::MIN,
            },
            Tier {
                key: "mid",
                label: "Mid",
                description: "In the middle.",
                floor: 0,
            },
            Tier {
                key: "high",
                label: "High",
                description: "At the top.",
                floor: 100,
            },
        ],
    };

    #[test]
    fn floors_are_inclusive_and_the_first_rung_catches_everything_below() {
        assert_eq!(L.tier_of(i32::MIN).key, "low");
        assert_eq!(L.tier_of(-1).key, "low");
        assert_eq!(L.tier_of(0).key, "mid");
        assert_eq!(L.tier_of(99).key, "mid");
        assert_eq!(L.tier_of(100).key, "high");
        assert_eq!(L.tier_of(i32::MAX).key, "high");
    }

    #[test]
    fn a_move_inside_one_band_is_silent() {
        assert!(L.crossed(0, 99).is_none());
        assert!(
            L.shift_message("The Guild", "raises you to", "drops you to", 0, 99)
                .is_none()
        );
    }

    #[test]
    fn crossing_up_and_down_pick_different_verbs_and_colors() {
        let up = L
            .shift_message("The Guild", "raises you to", "drops you to", 99, 100)
            .expect("crossed");
        assert!(up.contains("The Guild raises you to High."));
        assert!(up.contains("At the top."));
        assert!(up.starts_with("\x1b[1;32m"), "a rise reads green");

        let down = L
            .shift_message("The Guild", "raises you to", "drops you to", 100, -1)
            .expect("crossed");
        assert!(down.contains("The Guild drops you to Low."));
        assert!(down.starts_with("\x1b[1;31m"), "a fall reads red");
    }

    #[test]
    fn standing_keeps_the_number_for_optimisers() {
        assert_eq!(L.standing_line(150), "High (150)");
        assert_eq!(L.standing_line(-3), "Low (-3)");
    }
}
