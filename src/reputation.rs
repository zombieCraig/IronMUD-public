//! Faction reputation: where a player stands with each organised group in the
//! world, and what that standing changes.
//!
//! `MobileData.faction` has existed for a while as a tag the helper-aggro scan
//! reads to decide who rushes to whose defence. This module gives the same tag
//! a player-facing meaning: kill a faction's members and it turns on you,
//! while its enemies warm to you.
//!
//! # Why opposition is the point
//!
//! Reputation only *rises* through a faction's own quests and dialogue, or by
//! killing the members of a faction it opposes. Combat with a faction can only
//! lower standing with it. That asymmetry is deliberate: without opposed
//! factions, reputation is a ratchet every player eventually maxes, and a
//! number everyone has is not a standing. With them, the same kill that buys
//! you the Iron Guard's goodwill costs you the Ash Syndicate's, so where a
//! character stands is a record of what they chose.
//!
//! # Shape
//!
//! Two entry points, matching [`crate::morality`]:
//!
//! * [`apply_delta`] is the IO chokepoint — load, adjust, save, sync the live
//!   session, announce a tier crossing, propagate to opposed factions. Callers
//!   that don't already hold the character (kill credit, quest rewards) use
//!   it.
//! * [`adjust`] is pure. Callers that own a `&mut CharacterData` and persist it
//!   themselves — the dialogue effect layer — use that instead, because
//!   `apply_delta`'s write is out-of-band and their later save would overwrite
//!   it.
//!
//! # Locking
//!
//! Resolving opposed factions needs the definition registry, which lives in
//! `World`. Every function here that takes a [`crate::SharedState`] locks it
//! only to clone the definitions it needs and releases before touching
//! connections — `std::sync::Mutex` is not reentrant and holding both locks is
//! the standing deadlock hazard in this codebase.

use crate::tiers::{Tier, TierLadder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reputation is a bounded slider, like morality and unlike divine favor.
/// A bound makes the top of the ladder a real destination and stops a
/// long-lived character from banking so much standing that nothing they do
/// afterwards can move it.
pub const REPUTATION_MIN: i32 = -1000;
pub const REPUTATION_MAX: i32 = 1000;

/// Standing at or above which a faction counts you as a friend. The threshold
/// the `npcs.befriended` counter and the friendlier shop rates key off.
pub const ACCEPTED_FLOOR: i32 = 50;

/// Default standing at or below which a faction's mobs attack on sight.
/// Overridable per faction via `FactionDefinition.hostile_at`.
pub const DEFAULT_HOSTILE_AT: i32 = -200;

/// How much of a gain transfers, negated, to each opposed faction, in percent.
pub const DEFAULT_OPPOSITION_RATIO: i32 = 50;

/// Widest shop price swing at the ends of the ladder, in percent.
pub const DEFAULT_PRICE_SWING: i32 = 20;

/// Standing lost with a faction for killing one of its members.
///
/// Deliberately small against a 1000-point ladder: reaching Revered with a
/// faction's enemies is on the order of two hundred kills, which makes a
/// reputation a campaign rather than an afternoon. It is the same reasoning as
/// `morality::kill_delta`'s divide-by-fifty, at the same scale.
pub const KILL_STANDING_LOSS: i32 = -5;

/// The seven bands, ascending.
pub const LADDER: TierLadder = TierLadder {
    tiers: &[
        Tier {
            key: "hated",
            label: "Hated",
            description: "They will kill you on sight, and will not trade with you at any price.",
            floor: i32::MIN,
        },
        Tier {
            key: "hostile",
            label: "Hostile",
            description: "They consider you an enemy.",
            floor: -499,
        },
        Tier {
            key: "disliked",
            label: "Disliked",
            description: "They have not forgotten what you have done.",
            floor: -199,
        },
        Tier {
            key: "neutral",
            label: "Neutral",
            description: "They have no particular opinion of you.",
            floor: -49,
        },
        Tier {
            key: "accepted",
            label: "Accepted",
            description: "They count you a friend.",
            floor: ACCEPTED_FLOOR,
        },
        Tier {
            key: "honored",
            label: "Honored",
            description: "Your name is spoken well of among them.",
            floor: 200,
        },
        Tier {
            key: "revered",
            label: "Revered",
            description: "You are one of their own, and they will spare you nothing.",
            floor: 500,
        },
    ],
};

/// Where a reputation value sits. Variant order must match [`LADDER`] —
/// `ReputationTier as usize` indexes straight into it, and `Ord` is
/// load-bearing for hostility checks. Pinned by `enum_order_matches_the_ladder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReputationTier {
    Hated,
    Hostile,
    Disliked,
    Neutral,
    Accepted,
    Honored,
    Revered,
}

const REPUTATION_TIERS: [ReputationTier; 7] = [
    ReputationTier::Hated,
    ReputationTier::Hostile,
    ReputationTier::Disliked,
    ReputationTier::Neutral,
    ReputationTier::Accepted,
    ReputationTier::Honored,
    ReputationTier::Revered,
];

impl ReputationTier {
    pub fn from_value(v: i32) -> Self {
        REPUTATION_TIERS[LADDER.index_of(v)]
    }

    pub fn key(self) -> &'static str {
        LADDER.tiers[self as usize].key
    }

    pub fn label(self) -> &'static str {
        LADDER.tiers[self as usize].label
    }

    pub fn description(self) -> &'static str {
        LADDER.tiers[self as usize].description
    }

    /// Is this a friendly standing — Accepted or better?
    pub fn is_friendly(self) -> bool {
        self >= ReputationTier::Accepted
    }
}

/// A named group the world can hold an opinion on behalf of. Loaded from
/// `scripts/data/factions.json`, keyed by the same string builders put in
/// `MobileData.faction`.
///
/// A faction that no definition names still works — mobs tagged with it fight
/// alongside each other, and killing them still costs standing under the
/// faction's own key. It simply has no opposites, no display name beyond the
/// key, and default thresholds. That keeps the tag usable for ad-hoc groups
/// without forcing a registry entry for every warband in the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionDefinition {
    /// Stable lowercase key. Matched against `MobileData.faction`
    /// case-insensitively.
    pub key: String,
    /// Player-facing name, used as the subject of standing announcements:
    /// "The Iron Guard raises you to Honored."
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Keys of factions this one is at odds with. Gaining standing here loses
    /// it there, scaled by `opposition_ratio`.
    ///
    /// Opposition is applied one hop only, and is not assumed symmetric: if
    /// you want a mutual rivalry, list it on both. A one-way entry is a
    /// legitimate shape — a faction can resent a rival that does not think
    /// about it at all.
    #[serde(default)]
    pub opposed: Vec<String>,
    /// Percentage of a gain that transfers, negated, to each opposed faction.
    #[serde(default = "default_opposition_ratio")]
    pub opposition_ratio: i32,
    /// Standing at or below which this faction's mobs attack on sight.
    #[serde(default = "default_hostile_at")]
    pub hostile_at: i32,
    /// This faction attacks on sight at *any* standing, so there is no
    /// standing worth earning with it.
    ///
    /// A separate field rather than a `hostile_at` at the top of the ladder:
    /// that spelling worked only because [`REPUTATION_MAX`] happened to be the
    /// ceiling and `is_hostile_at` compares inclusively, which is a fact about
    /// two constants rather than a statement of intent. Builders reading the
    /// JSON could not tell the difference between "implacable" and "a number
    /// somebody typed".
    #[serde(default)]
    pub always_hostile: bool,
    /// Widest shop price swing this faction's merchants apply, in percent, at
    /// the ends of the ladder. 0 disables reputation pricing for them.
    #[serde(default = "default_price_swing")]
    pub price_swing: i32,
}

fn default_opposition_ratio() -> i32 {
    DEFAULT_OPPOSITION_RATIO
}

fn default_hostile_at() -> i32 {
    DEFAULT_HOSTILE_AT
}

fn default_price_swing() -> i32 {
    DEFAULT_PRICE_SWING
}

impl FactionDefinition {
    /// A definition for a faction tag nothing declared: no opposites, default
    /// thresholds, and the key itself as the display name.
    pub fn unregistered(key: &str) -> Self {
        Self {
            key: key.to_lowercase(),
            name: key.to_string(),
            description: String::new(),
            opposed: Vec::new(),
            opposition_ratio: DEFAULT_OPPOSITION_RATIO,
            hostile_at: DEFAULT_HOSTILE_AT,
            always_hostile: false,
            price_swing: DEFAULT_PRICE_SWING,
        }
    }

    /// The name to put in front of a standing announcement.
    pub fn display(&self) -> &str {
        if self.name.is_empty() { &self.key } else { &self.name }
    }
}

/// Normalise a faction tag. Returns `None` for absent or blank tags, which is
/// how an untagged mob opts out of the whole system.
pub fn normalize(faction: Option<&str>) -> Option<String> {
    let f = faction?.trim();
    if f.is_empty() {
        return None;
    }
    Some(f.to_lowercase())
}

/// Clamp a value into the legal reputation range.
pub fn clamp(v: i32) -> i32 {
    v.clamp(REPUTATION_MIN, REPUTATION_MAX)
}

/// Read a player's standing with a faction. Unknown factions read 0 — a
/// player who has never met a group is Neutral with it, which is the honest
/// default and means the map never has to be pre-populated.
pub fn standing(reputation: &HashMap<String, i32>, faction: &str) -> i32 {
    reputation.get(&faction.to_lowercase()).copied().unwrap_or(0)
}

pub fn tier(reputation: &HashMap<String, i32>, faction: &str) -> ReputationTier {
    ReputationTier::from_value(standing(reputation, faction))
}

/// Add `delta` to a character's standing with `faction`, clamped. Pure — the
/// caller writes it back and persists. Returns `(before, after)`.
///
/// Does **not** propagate to opposed factions: that needs the registry, so it
/// lives in [`apply_delta`] and [`opposition_deltas`].
pub fn adjust(reputation: &mut HashMap<String, i32>, faction: &str, delta: i32) -> (i32, i32) {
    let key = faction.to_lowercase();
    let before = reputation.get(&key).copied().unwrap_or(0);
    let after = clamp(before.saturating_add(delta));
    if after == 0 {
        // Keep the map sparse: a standing that returns to Neutral is
        // indistinguishable from never having met the faction, and leaving
        // zeroes behind would grow every long-lived character's record.
        reputation.remove(&key);
    } else {
        reputation.insert(key, after);
    }
    (before, after)
}

/// The knock-on changes a `delta` with `def`'s faction causes elsewhere:
/// each opposed faction moves the opposite way, scaled by
/// `opposition_ratio`.
///
/// Rounding is toward zero, so a ratio small enough to round a move away
/// produces no entry at all rather than a silent no-op write.
pub fn opposition_deltas(def: &FactionDefinition, delta: i32) -> Vec<(String, i32)> {
    if delta == 0 || def.opposition_ratio <= 0 {
        return Vec::new();
    }
    let transferred = -(delta * def.opposition_ratio) / 100;
    if transferred == 0 {
        return Vec::new();
    }
    def.opposed
        .iter()
        .filter_map(|k| normalize(Some(k)))
        .filter(|k| *k != def.key)
        .map(|k| (k, transferred))
        .collect()
}

/// Look a faction up in the world registry, falling back to
/// [`FactionDefinition::unregistered`] so callers never have to branch on
/// whether a builder declared the tag.
///
/// Locks `state` briefly and releases before returning. Never call while
/// holding the connections lock.
pub fn definition(state: &crate::SharedState, faction: &str) -> FactionDefinition {
    let key = faction.to_lowercase();
    match state.lock() {
        Ok(world) => world
            .faction_definitions
            .get(&key)
            .cloned()
            .unwrap_or_else(|| FactionDefinition::unregistered(&key)),
        Err(_) => FactionDefinition::unregistered(&key),
    }
}

/// Announcement for a standing move that crossed a band, or `None` when it
/// stayed inside one.
pub fn tier_shift_message(def: &FactionDefinition, before: i32, after: i32) -> Option<String> {
    LADDER.shift_message(def.display(), "now count you", "now count you", before, after)
}

/// Apply `delta` to `faction` **and** its declared opposites, mutating the map
/// in place. Pure apart from a brief registry read: the caller still owns the
/// character and is responsible for persisting it.
///
/// This is the shape the dialogue effect layer needs — it holds a
/// `&mut CharacterData` that the framework saves after effects run, so it
/// cannot use [`apply_delta`]'s out-of-band write. [`apply_delta`] is the same
/// arithmetic with the IO wrapped around it.
///
/// Returns the `(faction, before, after)` triples that actually moved,
/// primary first.
pub fn adjust_with_opposition(
    state: &crate::SharedState,
    reputation: &mut HashMap<String, i32>,
    faction: &str,
    delta: i32,
) -> Vec<(String, i32, i32)> {
    let Some(key) = normalize(Some(faction)) else {
        return Vec::new();
    };
    if delta == 0 {
        return Vec::new();
    }
    let def = definition(state, &key);
    let mut plan: Vec<(String, i32)> = vec![(key, delta)];
    plan.extend(opposition_deltas(&def, delta));

    let mut moved = Vec::new();
    for (f, d) in &plan {
        let (before, after) = adjust(reputation, f, *d);
        if after != before {
            moved.push((f.clone(), before, after));
        }
    }
    moved
}

/// Render a set of moves for a caller that reports inline rather than through
/// the connections table — the dialogue effect layer. One line per faction
/// that changed, with the band announcement appended where one crossed.
pub fn describe_moves(state: &crate::SharedState, moved: &[(String, i32, i32)]) -> String {
    let mut out = String::new();
    for (f, before, after) in moved {
        let def = definition(state, f);
        if !out.is_empty() {
            out.push('\n');
        }
        let delta = after - before;
        out.push_str(&format!(
            "[ {} {}{} \u{2192} {} ]",
            def.display(),
            if delta > 0 { "+" } else { "" },
            delta,
            after
        ));
        if let Some(line) = tier_shift_message(&def, *before, *after) {
            out.push('\n');
            out.push_str(&line);
        }
    }
    out
}

/// Move a character's standing with `faction` by `delta`, propagate to opposed
/// factions, persist once, keep the live session copy coherent, and announce
/// any band the move crossed.
///
/// The one place reputation changes for callers that do *not* already hold the
/// character. Callers that own a `&mut CharacterData` must use the pure
/// [`adjust`] instead — this function's write is out-of-band and their later
/// save would overwrite it.
///
/// Returns the `(faction, before, after)` triples that actually moved,
/// primary first.
pub fn apply_delta(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    state: &crate::SharedState,
    char_name: &str,
    faction: &str,
    delta: i32,
) -> Vec<(String, i32, i32)> {
    let Ok(Some(mut ch)) = db.get_character_data(&char_name.to_lowercase()) else {
        return Vec::new();
    };
    // Registry reads all happen inside this call and are finished with before
    // the connections lock below is taken.
    let moved = adjust_with_opposition(state, &mut ch.reputation, faction, delta);
    if moved.is_empty() {
        return Vec::new();
    }
    let synced = ch.reputation.clone();
    if db.save_character_data(ch).is_err() {
        return Vec::new();
    }

    if let Ok(mut conns) = connections.lock() {
        for session in conns.values_mut() {
            let is_them = session
                .character
                .as_ref()
                .map(|c| c.name.eq_ignore_ascii_case(char_name))
                .unwrap_or(false);
            if is_them {
                if let Some(c) = session.character.as_mut() {
                    c.reputation = synced;
                }
                break;
            }
        }
    }

    for (f, before, after) in &moved {
        let fdef = definition(state, f);
        if let Some(msg) = tier_shift_message(&fdef, *before, *after) {
            crate::script::achievements::send_to_player(connections, char_name, &msg);
        }
        // "Befriended" is the act, not the state: crossing up into Accepted
        // counts, and a faction you fall out with and win back counts twice,
        // because the second reconciliation was also work.
        let crossed_up = *after >= ACCEPTED_FLOOR && *before < ACCEPTED_FLOOR;
        if crossed_up {
            crate::script::achievements::notify_counter_core(db, connections, state, char_name, "npcs.befriended", 1);
        }
    }
    moved
}

/// Credit a kill against the victim's faction: standing with it drops, and its
/// enemies take note. A no-op for untagged mobs, which is the default and
/// keeps the world's unaffiliated wildlife out of the system entirely.
pub fn apply_kill(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    state: &crate::SharedState,
    char_name: &str,
    victim_faction: Option<&str>,
) -> Vec<(String, i32, i32)> {
    match normalize(victim_faction) {
        Some(f) => apply_delta(db, connections, state, char_name, &f, KILL_STANDING_LOSS),
        None => Vec::new(),
    }
}

/// Should this faction's mobs attack `standing` on sight?
pub fn is_hostile_at(def: &FactionDefinition, standing: i32) -> bool {
    def.always_hostile || standing <= def.hostile_at
}

/// Percentage adjustment to a shop rate, given the shopkeeper's faction
/// standing. Positive means the player pays more / is offered less.
///
/// Scales linearly across the ladder from `+price_swing` at the bottom to
/// `-price_swing` at the top, so a merchant's regard is a smooth discount
/// rather than a cliff at one threshold. Neutral is exactly zero.
pub fn price_adjustment_percent(def: &FactionDefinition, standing: i32) -> i32 {
    if def.price_swing <= 0 {
        return 0;
    }
    let s = clamp(standing);
    -(s * def.price_swing) / REPUTATION_MAX
}

/// Apply a reputation-driven adjustment to a shop rate.
///
/// `player_buying` flips the sense: a friendly merchant charges a friend less
/// but also pays them more, so the same standing lowers the sell rate and
/// raises the buy rate. The result is floored at 1 so a rate can never reach
/// zero and make goods free.
pub fn adjusted_shop_rate(def: &FactionDefinition, standing: i32, base_rate: i64, player_buying: bool) -> i64 {
    let pct = price_adjustment_percent(def, standing) as i64;
    let signed = if player_buying { pct } else { -pct };
    (base_rate + (base_rate * signed) / 100).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def_with(opposed: &[&str], ratio: i32) -> FactionDefinition {
        FactionDefinition {
            opposed: opposed.iter().map(|s| s.to_string()).collect(),
            opposition_ratio: ratio,
            ..FactionDefinition::unregistered("iron_guard")
        }
    }

    #[test]
    fn enum_order_matches_the_ladder() {
        assert_eq!(REPUTATION_TIERS.len(), LADDER.tiers.len());
        for (i, t) in REPUTATION_TIERS.iter().enumerate() {
            assert_eq!(*t as usize, i, "variant {:?} is out of position", t);
            assert_eq!(t.key(), LADDER.tiers[i].key);
        }
    }

    #[test]
    fn a_faction_you_have_never_met_reads_neutral() {
        let rep = HashMap::new();
        assert_eq!(standing(&rep, "iron_guard"), 0);
        assert_eq!(tier(&rep, "iron_guard"), ReputationTier::Neutral);
    }

    #[test]
    fn band_boundaries() {
        assert_eq!(ReputationTier::from_value(0), ReputationTier::Neutral);
        assert_eq!(ReputationTier::from_value(49), ReputationTier::Neutral);
        assert_eq!(ReputationTier::from_value(50), ReputationTier::Accepted);
        assert_eq!(ReputationTier::from_value(199), ReputationTier::Accepted);
        assert_eq!(ReputationTier::from_value(200), ReputationTier::Honored);
        assert_eq!(ReputationTier::from_value(500), ReputationTier::Revered);
        // The middle band is symmetric: Accepted opens at +50, Disliked at -50.
        assert_eq!(ReputationTier::from_value(-49), ReputationTier::Neutral);
        assert_eq!(ReputationTier::from_value(-50), ReputationTier::Disliked);
        assert_eq!(ReputationTier::from_value(-199), ReputationTier::Disliked);
        assert_eq!(ReputationTier::from_value(-200), ReputationTier::Hostile);
        assert_eq!(ReputationTier::from_value(-499), ReputationTier::Hostile);
        assert_eq!(ReputationTier::from_value(-500), ReputationTier::Hated);
        assert_eq!(ReputationTier::from_value(REPUTATION_MIN), ReputationTier::Hated);
    }

    #[test]
    fn friendly_starts_at_accepted() {
        assert!(!ReputationTier::from_value(ACCEPTED_FLOOR - 1).is_friendly());
        assert!(ReputationTier::from_value(ACCEPTED_FLOOR).is_friendly());
        assert!(ReputationTier::Revered.is_friendly());
    }

    #[test]
    fn adjust_clamps_and_keeps_the_map_sparse() {
        let mut rep = HashMap::new();
        assert_eq!(adjust(&mut rep, "Iron_Guard", 60), (0, 60));
        assert_eq!(rep.get("iron_guard"), Some(&60), "keys normalise to lowercase");

        assert_eq!(adjust(&mut rep, "iron_guard", 10_000), (60, REPUTATION_MAX));
        assert_eq!(
            adjust(&mut rep, "iron_guard", -10_000),
            (REPUTATION_MAX, REPUTATION_MIN)
        );

        // Back to exactly Neutral drops the row rather than storing a zero.
        assert_eq!(adjust(&mut rep, "iron_guard", 1000), (REPUTATION_MIN, 0));
        assert!(!rep.contains_key("iron_guard"));
    }

    #[test]
    fn opposition_transfers_a_fraction_the_other_way() {
        let def = def_with(&["ash_syndicate", "reavers"], 50);
        let out = opposition_deltas(&def, 100);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, d)| *d == -50), "a gain here is a loss there");

        // ...and the reverse: losing standing with the Guard warms its rivals.
        assert!(opposition_deltas(&def, -100).iter().all(|(_, d)| *d == 50));
    }

    #[test]
    fn opposition_is_skipped_when_it_would_round_to_nothing() {
        // 5% of the -5 kill delta rounds to zero. Writing nothing is better
        // than writing a no-op and announcing a move that didn't happen.
        let def = def_with(&["ash_syndicate"], 5);
        assert!(opposition_deltas(&def, KILL_STANDING_LOSS).is_empty());
        assert!(opposition_deltas(&def_with(&["ash_syndicate"], 0), 100).is_empty());
    }

    #[test]
    fn a_faction_cannot_oppose_itself() {
        let def = def_with(&["iron_guard", "ash_syndicate"], 50);
        let out = opposition_deltas(&def, 100);
        assert_eq!(out.len(), 1, "a self-reference would undo the primary move");
        assert_eq!(out[0].0, "ash_syndicate");
    }

    #[test]
    fn hostility_uses_the_factions_own_threshold() {
        let mut def = FactionDefinition::unregistered("reavers");
        assert!(!is_hostile_at(&def, -199));
        assert!(is_hostile_at(&def, DEFAULT_HOSTILE_AT));
        // A faction can be quicker to take offence than the default.
        def.hostile_at = -50;
        assert!(is_hostile_at(&def, -50));
        assert!(!is_hostile_at(&def, -49));
    }

    /// `always_hostile` is the implacable case, and it is a flag rather than a
    /// threshold on purpose.
    ///
    /// The shipped `undead` faction used to spell it `hostile_at: 1000`, which
    /// worked only because `REPUTATION_MAX` happens to be 1000 and the
    /// comparison is inclusive — a fact about two constants rather than a
    /// statement of intent, and one that would go quietly wrong the moment
    /// either moved. This test pins the meaning to the flag.
    #[test]
    fn an_always_hostile_faction_is_hostile_even_at_the_top_of_the_ladder() {
        let mut def = FactionDefinition::unregistered("undead");
        assert!(!is_hostile_at(&def, REPUTATION_MAX), "an ordinary faction is not");

        def.always_hostile = true;
        assert!(is_hostile_at(&def, REPUTATION_MAX));
        assert!(is_hostile_at(&def, 0));
        assert!(is_hostile_at(&def, REPUTATION_MIN));
    }

    /// The shipped registry has to name factions the shipped content actually
    /// uses. It did not: `factions.json` declared `town_guard` while the only
    /// tags in `mobile_presets.json` were `town_watch`, `camarilla` and
    /// `vampire_hunters`, so every declared faction had no mobs and every used
    /// faction fell back to `unregistered` — no display name, no opposites.
    #[test]
    fn the_shipped_registry_declares_the_tags_the_shipped_presets_use() {
        let registry: Vec<FactionDefinition> =
            serde_json::from_str(&std::fs::read_to_string("scripts/data/factions.json").expect("read factions.json"))
                .expect("parse factions.json");
        let declared: Vec<String> = registry.iter().map(|d| d.key.to_lowercase()).collect();

        let presets = std::fs::read_to_string("scripts/data/mobile_presets.json").expect("read mobile_presets.json");
        let presets: serde_json::Value = serde_json::from_str(&presets).expect("parse mobile_presets.json");
        let used: Vec<String> = presets
            .as_array()
            .expect("preset list")
            .iter()
            .filter_map(|p| p.get("faction").and_then(|f| f.as_str()))
            .filter_map(|f| normalize(Some(f)))
            .collect();
        assert!(!used.is_empty(), "presets should still tag some factions");

        for tag in &used {
            assert!(
                declared.contains(tag),
                "preset faction '{}' is not declared in factions.json (declared: {:?})",
                tag,
                declared
            );
        }

        // And every `opposed` entry names something real — a typo there is
        // silent, and hands the player standing with a faction that has no
        // mobs, no display name and its own leaderboard.
        for def in &registry {
            for other in &def.opposed {
                let other = other.to_lowercase();
                assert!(
                    declared.contains(&other),
                    "faction '{}' opposes '{}', which nothing declares",
                    def.key,
                    other
                );
            }
        }
    }

    #[test]
    fn prices_swing_smoothly_and_neutral_costs_the_listed_rate() {
        let def = FactionDefinition::unregistered("iron_guard");
        assert_eq!(price_adjustment_percent(&def, 0), 0);
        assert_eq!(price_adjustment_percent(&def, REPUTATION_MAX), -DEFAULT_PRICE_SWING);
        assert_eq!(price_adjustment_percent(&def, REPUTATION_MIN), DEFAULT_PRICE_SWING);
        // Halfway up the ladder is halfway to the discount.
        assert_eq!(
            price_adjustment_percent(&def, REPUTATION_MAX / 2),
            -DEFAULT_PRICE_SWING / 2
        );
    }

    #[test]
    fn a_merchant_who_likes_you_charges_less_and_pays_more() {
        let def = FactionDefinition::unregistered("iron_guard");
        // Selling to the player: 100% list price becomes 80% at Revered.
        assert_eq!(adjusted_shop_rate(&def, REPUTATION_MAX, 100, true), 80);
        // Buying from the player: 50% of value becomes 60%.
        assert_eq!(adjusted_shop_rate(&def, REPUTATION_MAX, 50, false), 60);
        // And an enemy pays the penalty in both directions.
        assert_eq!(adjusted_shop_rate(&def, REPUTATION_MIN, 100, true), 120);
        assert_eq!(adjusted_shop_rate(&def, REPUTATION_MIN, 50, false), 40);
    }

    #[test]
    fn a_shop_rate_never_reaches_zero() {
        let mut def = FactionDefinition::unregistered("iron_guard");
        def.price_swing = 500;
        assert!(
            adjusted_shop_rate(&def, REPUTATION_MAX, 100, true) >= 1,
            "goods are never free"
        );
    }

    #[test]
    fn a_zero_swing_faction_opts_out_of_reputation_pricing() {
        let mut def = FactionDefinition::unregistered("iron_guard");
        def.price_swing = 0;
        assert_eq!(adjusted_shop_rate(&def, REPUTATION_MAX, 100, true), 100);
    }

    #[test]
    fn an_untagged_or_blank_faction_is_out_of_the_system() {
        assert_eq!(normalize(None), None);
        assert_eq!(normalize(Some("")), None);
        assert_eq!(normalize(Some("   ")), None);
        assert_eq!(normalize(Some(" Iron_Guard ")), Some("iron_guard".to_string()));
    }

    fn temp_db() -> (crate::db::Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = crate::db::Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn char_named(db: &crate::db::Db, name: &str) {
        let ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        db.save_character_data(ch).expect("save");
    }

    fn world_with(db: &crate::db::Db, defs: Vec<FactionDefinition>) -> (crate::SharedConnections, crate::SharedState) {
        let conns: crate::SharedConnections =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let state = crate::World::minimal_shared(db.clone(), conns.clone());
        {
            let mut w = state.lock().unwrap();
            for d in defs {
                w.faction_definitions.insert(d.key.clone(), d);
            }
        }
        (conns, state)
    }

    #[test]
    fn a_kill_costs_standing_with_the_victims_faction_and_buys_it_with_their_enemies() {
        let (db, _t) = temp_db();
        char_named(&db, "rook");
        let (conns, state) = world_with(
            &db,
            vec![FactionDefinition {
                opposed: vec!["ash_syndicate".into()],
                ..FactionDefinition::unregistered("iron_guard")
            }],
        );

        // Twenty guards, so the opposition transfer clears the rounding floor.
        for _ in 0..20 {
            apply_kill(&db, &conns, &state, "rook", Some("Iron_Guard"));
        }

        let ch = db.get_character_data("rook").unwrap().unwrap();
        assert_eq!(standing(&ch.reputation, "iron_guard"), 20 * KILL_STANDING_LOSS);
        assert_eq!(
            standing(&ch.reputation, "ash_syndicate"),
            20 * 2,
            "half of each -5 lands on the rival, the other way up"
        );
    }

    #[test]
    fn an_untagged_victim_moves_nothing() {
        let (db, _t) = temp_db();
        char_named(&db, "rook");
        let (conns, state) = world_with(&db, vec![]);

        assert!(apply_kill(&db, &conns, &state, "rook", None).is_empty());
        assert!(apply_kill(&db, &conns, &state, "rook", Some("  ")).is_empty());
        assert!(db.get_character_data("rook").unwrap().unwrap().reputation.is_empty());
    }

    #[test]
    fn apply_delta_persists_and_syncs_the_live_session() {
        let (db, _t) = temp_db();
        char_named(&db, "rook");
        let (conns, state) = world_with(&db, vec![]);

        let (tx_client, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = db.get_character_data("rook").expect("read");
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);

        apply_delta(&db, &conns, &state, "rook", "iron_guard", 60);

        assert_eq!(
            standing(
                &db.get_character_data("rook").unwrap().unwrap().reputation,
                "iron_guard"
            ),
            60
        );
        let live = conns
            .lock()
            .unwrap()
            .values()
            .next()
            .and_then(|s| s.character.as_ref().map(|c| standing(&c.reputation, "iron_guard")));
        assert_eq!(
            live,
            Some(60),
            "the live copy must not go stale, or the regen tick flushes it back"
        );
    }

    #[test]
    fn crossing_into_accepted_announces_and_a_nudge_inside_a_band_does_not() {
        let (db, _t) = temp_db();
        char_named(&db, "rook");
        let (conns, state) = world_with(
            &db,
            vec![FactionDefinition {
                name: "The Iron Guard".into(),
                ..FactionDefinition::unregistered("iron_guard")
            }],
        );

        let (tx_client, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = db.get_character_data("rook").expect("read");
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);

        let drain = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>| {
            let mut out = String::new();
            while let Ok(m) = rx.try_recv() {
                out.push_str(&m);
            }
            out
        };

        apply_delta(&db, &conns, &state, "rook", "iron_guard", ACCEPTED_FLOOR);
        let msg = drain(&mut rx);
        assert!(msg.contains("The Iron Guard now count you Accepted."), "got {:?}", msg);

        apply_delta(&db, &conns, &state, "rook", "iron_guard", 10);
        assert_eq!(drain(&mut rx), "", "moves inside a band stay silent");
    }
}
