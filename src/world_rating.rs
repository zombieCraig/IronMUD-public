//! How much of a world this world is.
//!
//! `build audit world` answers "what is broken". This answers the other
//! question a builder asks, the one nothing in the engine could answer before:
//! *how far along is this?* A room count cannot say — 400 rooms with no
//! quests, no shops and no NPCs is not further along than 120 rooms that
//! hold together — so the rating is a composite of five things, and it is
//! reported as a **named tier** rather than a number.
//!
//! The naming is the point, and it is the same rule `src/tiers.rs` was written
//! for: *a move inside a band says nothing, and a move across one announces
//! itself*. "Village" tells an operator where they are. `41` does not.
//!
//! ## Why sqrt
//!
//! Every count term scales as `sqrt(actual / target)`. Linear scaling would
//! leave a new world reading 3% forever and teach its builders that nothing
//! they do matters; log scaling would put a demo world past halfway and teach
//! them the opposite. Square root moves fast enough early that the first
//! week's work is visible, and slowly enough late that the last tier is
//! genuinely a lot of world.

use crate::audit::WorldFacts;
use crate::tiers::{Tier, TierLadder};

/// What a full-marks world looks like. These are the numbers the whole rating
/// is calibrated against, so they are the honest place to argue about scale.
pub struct Targets {
    pub rooms: f32,
    pub areas: f32,
    pub quests: f32,
    pub dialogue_trees: f32,
    pub recipes: f32,
    pub transports: f32,
    /// Mobile prototypes per room.
    pub mobiles_per_room: f32,
    /// Item prototypes per area.
    pub items_per_area: f32,
    /// Spawn points per room. Below this the world is furnished but empty.
    pub spawns_per_room: f32,
}

pub const TARGETS: Targets = Targets {
    rooms: 1500.0,
    areas: 20.0,
    quests: 60.0,
    dialogue_trees: 40.0,
    recipes: 30.0,
    transports: 5.0,
    mobiles_per_room: 0.30,
    items_per_area: 25.0,
    spawns_per_room: 0.50,
};

/// The ladder. Eight rungs from a place with nothing in it to a place someone
/// could live in.
pub const LADDER: TierLadder = TierLadder {
    tiers: &[
        Tier {
            key: "wilderness",
            label: "Wilderness",
            description: "There is almost nothing here yet. Everything you build is the first of its kind.",
            floor: 0,
        },
        Tier {
            key: "outpost",
            label: "Outpost",
            description: "A handful of rooms and something living in them. A place, barely.",
            floor: 10,
        },
        Tier {
            key: "hamlet",
            label: "Hamlet",
            description: "Small, and beginning to hold together. Somewhere to walk through.",
            floor: 22,
        },
        Tier {
            key: "village",
            label: "Village",
            description: "Furnished and populated. A player could spend an evening here.",
            floor: 35,
        },
        Tier {
            key: "town",
            label: "Town",
            description: "Several areas, things to do in them, and reasons to move between.",
            floor: 50,
        },
        Tier {
            key: "city",
            label: "City",
            description: "Deep as well as wide. Systems in use, not just present.",
            floor: 65,
        },
        Tier {
            key: "realm",
            label: "Realm",
            description: "Large, connected and finished. A world with its own weather.",
            floor: 80,
        },
        Tier {
            key: "world",
            label: "World",
            description: "Everything the engine offers, in use, at scale, and in good repair.",
            floor: 92,
        },
    ],
};

/// One weighted component of the rating.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub key: &'static str,
    pub label: &'static str,
    /// 0..=100.
    pub score: i32,
    pub weight: i32,
    /// The one sentence that says what would move it.
    pub hint: &'static str,
}

/// The whole verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldRating {
    /// 0..=100.
    pub score: i32,
    pub tier_key: &'static str,
    pub tier_label: &'static str,
    pub tier_description: &'static str,
    pub components: Vec<Component>,
    /// Points to the next rung, or `None` at the top.
    pub next_label: Option<&'static str>,
    pub next_at: i32,
    /// The rating before caps were applied. Equal to `score` when nothing
    /// capped it.
    pub uncapped_score: i32,
    /// Why the rating is being held down, if it is.
    pub cap: Option<Cap>,
}

/// A structural absence that holds the rating back regardless of everything
/// else.
///
/// Curves cannot express "a world with no quests is not a Town" — averaging
/// lets a strong showing everywhere else buy past a hole that ought to be
/// disqualifying. Caps say it outright, and they double as the clearest "next
/// rung" message the rating can produce: not *build more*, but *build this*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap {
    pub max: i32,
    pub reason: &'static str,
}

/// Checked in order; the lowest applicable cap wins.
const CAPS: &[(i32, &str)] = &[
    (34, "there is only one area — a world is somewhere you can leave"),
    (49, "no quests exist — nothing tells a player what to do next"),
    (
        64,
        "no NPC has a dialogue tree — every conversation is keyword-matching",
    ),
    (
        79,
        "there is no bulletin board — a playerbase has nowhere to talk to itself",
    ),
];

impl WorldRating {
    /// The component that would move the rating most for the least work: the
    /// lowest-scoring one, weighted. This is the "show the next rung" line,
    /// and it is why the components carry hints at all.
    pub fn weakest(&self) -> Option<&Component> {
        self.components
            .iter()
            .filter(|c| c.score < 100)
            .max_by_key(|c| (100 - c.score) * c.weight)
    }

    /// The one sentence to show an operator asking "what next?".
    ///
    /// A cap outranks the weakest component, always: while one is in force,
    /// improving anything else moves nothing, and pointing at a term that
    /// cannot help would be actively misleading.
    pub fn next_step(&self) -> String {
        if let Some(cap) = self.cap {
            return format!("Held at {} because {}.", self.tier_label, cap.reason);
        }
        match self.weakest() {
            Some(c) => format!("{}: {}", c.label, c.hint),
            None => "Nothing left to raise.".to_string(),
        }
    }
}

/// `sqrt(actual / target)`, capped at 1, as 0..=100.
fn term(actual: f32, target: f32) -> i32 {
    if target <= 0.0 {
        return 100;
    }
    let r = (actual / target).max(0.0);
    ((r.sqrt()).min(1.0) * 100.0).round() as i32
}

/// A plain ratio, capped, as 0..=100. For terms that are already densities.
fn linear(actual: f32, target: f32) -> i32 {
    if target <= 0.0 {
        return 100;
    }
    ((actual / target).clamp(0.0, 1.0) * 100.0).round() as i32
}

fn mean(parts: &[i32]) -> i32 {
    if parts.is_empty() {
        return 0;
    }
    parts.iter().sum::<i32>() / parts.len() as i32
}

/// Rate a world.
///
/// `quality_pct` is the share of graded entities at C or better, from the
/// auditor. It is passed in rather than recomputed because the caller has
/// already done the expensive part.
pub fn rate(facts: &WorldFacts, quality_pct: i32) -> WorldRating {
    let rooms = facts.room_count as f32;
    let areas = facts.area_count as f32;

    let size = mean(&[term(rooms, TARGETS.rooms), term(areas, TARGETS.areas)]);

    let density = mean(&[
        linear(
            if rooms > 0.0 {
                facts.mobile_count as f32 / rooms
            } else {
                0.0
            },
            TARGETS.mobiles_per_room,
        ),
        linear(
            if areas > 0.0 {
                facts.item_count as f32 / areas
            } else {
                0.0
            },
            TARGETS.items_per_area,
        ),
        linear(
            if rooms > 0.0 {
                facts.spawn_point_count as f32 / rooms
            } else {
                0.0
            },
            TARGETS.spawns_per_room,
        ),
    ]);

    let depth = mean(&[
        term(facts.quest_count as f32, TARGETS.quests),
        term(facts.dialogue_trees as f32, TARGETS.dialogue_trees),
        term(facts.recipe_count as f32, TARGETS.recipes),
        term(facts.transport_count as f32, TARGETS.transports),
    ]);

    // A world of one area is connected with itself by definition; the term
    // only starts asking questions once there is somewhere else to go. A world
    // of *no* areas scores zero rather than full marks — "vacuously true" is
    // the wrong answer to give a rating whose whole job is to say how far along
    // something is.
    let connectivity = match facts.area_count {
        0 => 0,
        1 => 100,
        n => linear(facts.connected_areas as f32, n as f32),
    };

    let components = vec![
        Component {
            key: "size",
            label: "Size",
            score: size,
            weight: 25,
            hint: "More rooms, and more areas to put them in.",
        },
        Component {
            key: "density",
            label: "Density",
            score: density,
            weight: 20,
            hint: "Mobiles, items and spawn points — a furnished world with nothing in it reads as abandoned.",
        },
        Component {
            key: "depth",
            label: "Depth",
            score: depth,
            weight: 25,
            hint: "Quests, dialogue trees, recipes, transports. The systems the engine has and the world does not use.",
        },
        Component {
            key: "quality",
            label: "Quality",
            score: quality_pct.clamp(0, 100),
            weight: 20,
            hint: "The share of content grading C or better. `build audit world` names the worst of it.",
        },
        Component {
            key: "connectivity",
            label: "Connectivity",
            score: connectivity,
            weight: 10,
            hint: "Areas players can walk between. An area with no way in is a world nobody visits.",
        },
    ];

    let total_weight: i32 = components.iter().map(|c| c.weight).sum();
    let uncapped_score = if total_weight == 0 {
        0
    } else {
        components.iter().map(|c| c.score * c.weight).sum::<i32>() / total_weight
    };

    let applicable = [
        facts.area_count < 2,
        facts.quest_count == 0,
        facts.dialogue_trees == 0,
        facts.board_items == 0,
    ];
    let cap = CAPS
        .iter()
        .zip(applicable)
        .filter(|(_, applies)| *applies)
        .map(|((max, reason), _)| Cap { max: *max, reason })
        .min_by_key(|c| c.max);

    let score = match cap {
        Some(c) => uncapped_score.min(c.max),
        None => uncapped_score,
    };

    let idx = LADDER.index_of(score);
    let tier = &LADDER.tiers[idx];
    let next = LADDER.tiers.get(idx + 1);

    WorldRating {
        score,
        tier_key: tier.key,
        tier_label: tier.label,
        tier_description: tier.description,
        components,
        next_label: next.map(|t| t.label),
        next_at: next.map(|t| t.floor).unwrap_or(100),
        uncapped_score,
        cap,
    }
}

/// The cached answer to "how is the world doing", installed by the
/// build-score tick.
///
/// Cached for the same reason the leaderboards are: computing it needs a full
/// world scan, and that must never happen on somebody's command.
#[derive(Debug, Clone, Default)]
pub struct WorldReport {
    /// Unix seconds of the last scan. 0 = never run.
    pub generated_at: i64,
    pub facts: WorldFacts,
    /// `None` until the first scan lands.
    pub rating: Option<WorldRating>,
    /// Share of graded entities at C or better.
    pub quality_pct: i32,
}

impl WorldReport {
    pub fn is_ready(&self) -> bool {
        self.generated_at > 0 && self.rating.is_some()
    }
}

/// A world-level milestone: something true of the world rather than of any one
/// builder.
///
/// Named in code rather than in data because each one reads a different field
/// — the same reason `src/leaderboard.rs` names its derived boards in code
/// while discovering counters from data. The *presentation* (name, blurb,
/// title) lives in `scripts/data/achievements/world.json`, so what a milestone
/// is called is still a content decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldGoal {
    /// The rating reached this tier key.
    Tier(&'static str),
    Rooms(usize),
    Areas(usize),
    Quests(usize),
    Mobiles(usize),
    Items(usize),
    DialogueTrees(usize),
    Recipes(usize),
    Transports(usize),
    Boards(usize),
}

/// The milestone table. Keys must match definitions in
/// `scripts/data/achievements/world.json`.
///
/// Every threshold sits **above** what the shipped demo world already has, so
/// a fresh install unlocks nothing. A wall of milestones that were already
/// achieved before anybody logged in is not a record of anything.
pub const WORLD_GOALS: &[(&str, WorldGoal)] = &[
    ("world_hundred_rooms", WorldGoal::Rooms(100)),
    ("world_five_hundred_rooms", WorldGoal::Rooms(500)),
    ("world_thousand_rooms", WorldGoal::Rooms(1000)),
    ("world_ten_areas", WorldGoal::Areas(10)),
    ("world_twenty_areas", WorldGoal::Areas(20)),
    ("world_ten_quests", WorldGoal::Quests(10)),
    ("world_fifty_quests", WorldGoal::Quests(50)),
    ("world_hundred_mobiles", WorldGoal::Mobiles(100)),
    ("world_two_hundred_items", WorldGoal::Items(200)),
    ("world_talkative", WorldGoal::DialogueTrees(20)),
    ("world_well_stocked", WorldGoal::Recipes(20)),
    ("world_transit", WorldGoal::Transports(3)),
    ("world_notice_board", WorldGoal::Boards(1)),
    // No Village rung: the shipped demo world is already one, so it is where
    // a world starts rather than something it reaches.
    ("world_tier_town", WorldGoal::Tier("town")),
    ("world_tier_city", WorldGoal::Tier("city")),
    ("world_tier_realm", WorldGoal::Tier("realm")),
    ("world_tier_world", WorldGoal::Tier("world")),
];

impl WorldGoal {
    /// Current value and target, for a progress line.
    pub fn progress(&self, facts: &WorldFacts, rating: &WorldRating) -> (i64, i64) {
        match self {
            WorldGoal::Tier(key) => {
                // An unknown key must be **unreachable**, not already reached.
                // `unwrap_or(0)` would make `have >= want` true on a fresh
                // install, so one typo in a new goal would unlock, announce and
                // award itself on the first tick.
                let want = LADDER
                    .tiers
                    .iter()
                    .position(|t| t.key == *key)
                    .map(|i| i as i64)
                    .unwrap_or(i64::MAX);
                let have = LADDER.index_of(rating.score);
                (have as i64, want)
            }
            WorldGoal::Rooms(n) => (facts.room_count as i64, *n as i64),
            WorldGoal::Areas(n) => (facts.area_count as i64, *n as i64),
            WorldGoal::Quests(n) => (facts.quest_count as i64, *n as i64),
            WorldGoal::Mobiles(n) => (facts.mobile_count as i64, *n as i64),
            WorldGoal::Items(n) => (facts.item_count as i64, *n as i64),
            WorldGoal::DialogueTrees(n) => (facts.dialogue_trees as i64, *n as i64),
            WorldGoal::Recipes(n) => (facts.recipe_count as i64, *n as i64),
            WorldGoal::Transports(n) => (facts.transport_count as i64, *n as i64),
            WorldGoal::Boards(n) => (facts.board_items as i64, *n as i64),
        }
    }

    pub fn is_met(&self, facts: &WorldFacts, rating: &WorldRating) -> bool {
        let (have, want) = self.progress(facts, rating);
        have >= want
    }
}

/// Every milestone the world currently satisfies.
pub fn met_goals(facts: &WorldFacts, rating: &WorldRating) -> Vec<&'static str> {
    WORLD_GOALS
        .iter()
        .filter(|(_, goal)| goal.is_met(facts, rating))
        .map(|(key, _)| *key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> WorldFacts {
        WorldFacts::default()
    }

    fn demo() -> WorldFacts {
        // The world the repo ships.
        WorldFacts {
            area_count: 5,
            room_count: 56,
            item_count: 40,
            mobile_count: 15,
            quest_count: 0,
            spawn_point_count: 20,
            recipe_count: 4,
            transport_count: 1,
            connected_areas: 5,
            ..Default::default()
        }
    }

    fn finished() -> WorldFacts {
        WorldFacts {
            area_count: 20,
            room_count: 1500,
            item_count: 500,
            mobile_count: 450,
            quest_count: 60,
            spawn_point_count: 750,
            recipe_count: 30,
            transport_count: 5,
            dialogue_trees: 40,
            connected_areas: 20,
            board_items: 3,
            post_office_rooms: 2,
            recall_rooms: 4,
            bank_rooms: 2,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_world_is_wilderness() {
        let r = rate(&empty(), 0);
        assert_eq!(r.tier_key, "wilderness");
        assert!(r.score < 20, "empty world scored {}", r.score);
    }

    #[test]
    fn the_demo_world_is_early_but_not_zero() {
        // It has to read as *started*. A rating that says 0 after five areas
        // and fifty-six rooms teaches its builders that nothing counts.
        let r = rate(&demo(), 80);
        assert!(r.score > 15, "the demo world scored {}", r.score);
        assert!(
            r.score < 55,
            "the demo world scored {} — the scale is too generous",
            r.score
        );
    }

    #[test]
    fn a_finished_world_reaches_the_top_rung() {
        let r = rate(&finished(), 95);
        assert_eq!(r.tier_key, "world", "a full world scored {}", r.score);
        assert!(r.next_label.is_none());
    }

    #[test]
    fn every_score_lands_on_a_rung() {
        for s in 0..=100 {
            let idx = LADDER.index_of(s);
            assert!(idx < LADDER.tiers.len());
        }
    }

    #[test]
    fn the_ladder_ascends() {
        let mut last = i32::MIN;
        for t in LADDER.tiers {
            assert!(t.floor > last || t.floor == 0, "{} breaks the order", t.key);
            last = t.floor;
        }
    }

    #[test]
    fn a_world_of_one_area_is_not_penalised_for_connectivity() {
        let facts = WorldFacts {
            area_count: 1,
            room_count: 30,
            connected_areas: 0,
            ..Default::default()
        };
        let r = rate(&facts, 50);
        let c = r.components.iter().find(|c| c.key == "connectivity").unwrap();
        assert_eq!(c.score, 100, "a lone area was marked down for being alone");
    }

    #[test]
    fn islands_are_marked_down() {
        let facts = WorldFacts {
            area_count: 4,
            room_count: 100,
            connected_areas: 2,
            ..Default::default()
        };
        let r = rate(&facts, 50);
        let c = r.components.iter().find(|c| c.key == "connectivity").unwrap();
        assert_eq!(c.score, 50);
    }

    #[test]
    fn the_weakest_component_is_the_one_worth_fixing() {
        // Weighted, not raw: a 0 on a 10-weight term is less urgent than a 40
        // on a 25-weight one, and the hint has to point at the second.
        let facts = WorldFacts {
            area_count: 4,
            room_count: 400,
            mobile_count: 130,
            item_count: 110,
            spawn_point_count: 210,
            connected_areas: 3,
            ..Default::default()
        };
        let r = rate(&facts, 90);
        let weakest = r.weakest().expect("something is imperfect here");
        assert_eq!(weakest.key, "depth", "pointed at {} instead", weakest.key);
    }

    #[test]
    fn quality_is_carried_straight_through() {
        let a = rate(&demo(), 20);
        let b = rate(&demo(), 100);
        assert!(b.score > a.score, "quality did not move the rating");
        let c = b.components.iter().find(|c| c.key == "quality").unwrap();
        assert_eq!(c.score, 100);
    }

    #[test]
    fn the_shipped_demo_world_unlocks_no_milestone() {
        // The guard for the whole wall. A fresh install that arrives with
        // several milestones already lit is not recording anything, and it
        // takes the first real one away from whoever earns it.
        let facts = demo();
        let rating = rate(&facts, 80);
        let met = met_goals(&facts, &rating);
        assert!(met.is_empty(), "the demo world already satisfies {met:?}");
    }

    #[test]
    fn milestones_fire_once_the_world_grows() {
        let facts = finished();
        let rating = rate(&facts, 95);
        let met = met_goals(&facts, &rating);
        assert!(met.contains(&"world_thousand_rooms"));
        assert!(met.contains(&"world_tier_world"));
        assert_eq!(
            met.len(),
            WORLD_GOALS.len(),
            "a finished world should satisfy all of them"
        );
    }

    #[test]
    fn a_tier_goal_is_met_by_anything_at_or_above_it() {
        let facts = finished();
        let rating = rate(&facts, 95);
        assert!(
            WorldGoal::Tier("town").is_met(&facts, &rating),
            "reaching World did not satisfy Town"
        );
    }

    #[test]
    fn a_world_with_no_quests_cannot_be_a_town() {
        // The demo world scores like a Town on the curves alone. It has no
        // quests, no dialogue trees and no boards, and calling that a Town
        // would make the whole ladder mean nothing.
        let facts = demo();
        let r = rate(&facts, 80);
        assert!(r.uncapped_score >= 50, "fixture no longer exercises the cap");
        assert!(r.cap.is_some());
        assert!(r.score <= 49, "the cap did not bite: {}", r.score);
        assert!(r.next_step().contains("quests"), "{}", r.next_step());
    }

    #[test]
    fn the_lowest_applicable_cap_wins() {
        let facts = WorldFacts {
            area_count: 1,
            room_count: 400,
            quest_count: 0,
            ..Default::default()
        };
        let r = rate(&facts, 100);
        assert_eq!(r.cap.map(|c| c.max), Some(34), "expected the one-area cap");
    }

    #[test]
    fn a_complete_world_is_not_capped() {
        let r = rate(&finished(), 95);
        assert!(r.cap.is_none(), "{:?}", r.cap);
        assert_eq!(r.score, r.uncapped_score);
    }

    #[test]
    fn a_cap_outranks_the_weakest_component_in_advice() {
        let facts = demo();
        let r = rate(&facts, 80);
        // While a cap is in force, improving the weakest term moves nothing,
        // so pointing at it would be misleading.
        assert!(r.next_step().starts_with("Held at"), "{}", r.next_step());
    }

    #[test]
    fn sqrt_scaling_makes_early_work_visible() {
        // Ten percent of the way to the room target must read as clearly more
        // than ten percent of the way there, or the first month feels like
        // standing still.
        assert!(term(150.0, 1500.0) > 25);
        // ...and must not read as most of the way there.
        assert!(term(150.0, 1500.0) < 50);
    }
}
