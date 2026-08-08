//! What a builder's work is worth.
//!
//! `src/leaderboard.rs` ranks play. This ranks building, and the difference
//! between the two is the reason this file exists rather than being another
//! board in that one:
//!
//! > **Player metrics measure play. Builder metrics measure the artifact
//! > everyone else has to live in.**
//!
//! Reward room count and you get a thousand empty rooms and a worse game.
//! That is Goodhart's law with a save button, and it is the single largest
//! risk in scoring building at all. Three structural defences are built in
//! here rather than bolted on:
//!
//! 1. **The score is derived from a scan of what currently exists**, never
//!    incremented on an edit. There is no event to farm: re-saving a room
//!    earns nothing, and deleting junk *lowers* your score.
//! 2. **Every entity is weighted by its audit grade.** A room contributes its
//!    quality, not a 1.
//! 3. **Returns diminish per area, per kind.** The sixtieth room in one area
//!    is worth a fraction of the sixth, so breadth — a new well-formed area, a
//!    quest, a mobile with a dialogue tree — always outbids padding.
//!
//! Plus the guard that predates all of it: only `ContentOrigin::Builder`
//! content is counted, so the demo world and anything an importer produced are
//! worth nothing to anybody. See `src/attribution.rs`.
//!
//! Everything here is pure. The tick that runs it lives in
//! `src/ticks/build_score.rs`, and the counters it reconciles onto characters
//! are written by [`apply_to_characters`].

use std::collections::{BTreeMap, HashMap};

use crate::audit::scan::WorldSnapshot;
use crate::audit::{self, Grade};
use crate::types::{Authored, CharacterData, ContentKind};

/// How often the scan runs. Same cadence and same reasoning as the leaderboard
/// scan: it is an expensive full-world read, and a builder score that moved the
/// instant you saved would make the number the point.
pub const BUILD_SCORE_TICK_INTERVAL_SECS: u64 = 300;

/// Points a perfect (grade 100) entity of each kind is worth, before
/// diminishing returns.
///
/// The order is a claim about cost, and it is the one lever that decides what
/// building *for score* looks like: an area is a decision, a quest is a day, a
/// mobile with something to say is an afternoon, a room is an hour, an item is
/// minutes. Anyone tuning these is choosing what builders will make more of.
pub const fn kind_weight(kind: ContentKind) -> i32 {
    match kind {
        ContentKind::Area => 60,
        ContentKind::Quest => 40,
        ContentKind::Mobile => 20,
        ContentKind::Item => 12,
        ContentKind::Room => 10,
    }
}

/// Extra weight for a mobile carrying a branching dialogue tree.
///
/// A dialogue tree is the single most under-used system in the engine and by
/// far the most work per mobile. It is not its own entity kind, so it rides
/// here as a multiplier rather than being invisible.
const DIALOGUE_TREE_MULTIPLIER: f32 = 2.0;

/// Half-life constant for per-area, per-kind diminishing returns.
///
/// `factor(n) = DIMINISH_AT / (DIMINISH_AT + n)` over the n-th entity of a
/// kind in one area, so the first is worth 1.0, the fortieth 0.5, the
/// hundred-and-twentieth 0.25. It never reaches zero — padding is not
/// forbidden, it is just a bad way to spend an afternoon.
const DIMINISH_AT: f32 = 40.0;

fn diminish(index: usize) -> f32 {
    DIMINISH_AT / (DIMINISH_AT + index as f32)
}

/// One builder's showing for one kind of content.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KindTally {
    pub count: usize,
    pub points: i32,
    /// Entities graded C or better — what the readiness tracks read.
    pub good: usize,
    /// Entities graded A.
    pub excellent: usize,
    /// Entities carrying at least one blocker. A builder's own to-do list.
    pub broken: usize,
}

/// Everything one builder has to show for themselves.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BuilderScore {
    pub name: String,
    /// Derived from the content that currently exists.
    pub content_points: i32,
    /// The one stored addend: bounties accepted. See `src/bounty.rs`.
    pub bounty_points: i32,
    pub total: i32,
    pub tallies: BTreeMap<&'static str, KindTally>,
}

impl BuilderScore {
    pub fn entities(&self) -> usize {
        self.tallies.values().map(|t| t.count).sum()
    }

    pub fn good(&self) -> usize {
        self.tallies.values().map(|t| t.good).sum()
    }

    pub fn excellent(&self) -> usize {
        self.tallies.values().map(|t| t.excellent).sum()
    }

    pub fn broken(&self) -> usize {
        self.tallies.values().map(|t| t.broken).sum()
    }

    pub fn tally(&self, kind: ContentKind) -> KindTally {
        self.tallies.get(kind.key()).cloned().unwrap_or_default()
    }

    /// The counters this score reconciles onto the builder's character.
    ///
    /// Returned as data rather than written here so the pure computation stays
    /// pure, and so a test can assert the exact set without a database. Every
    /// key that appears here becomes a leaderboard automatically — boards are
    /// discovered from character counter keys — and can be the target of a
    /// pure-JSON achievement.
    pub fn counters(&self) -> Vec<(&'static str, u32)> {
        vec![
            ("build.score", self.total.max(0) as u32),
            ("build.rooms", self.tally(ContentKind::Room).count as u32),
            ("build.items", self.tally(ContentKind::Item).count as u32),
            ("build.mobiles", self.tally(ContentKind::Mobile).count as u32),
            ("build.quests", self.tally(ContentKind::Quest).count as u32),
            ("build.areas", self.tally(ContentKind::Area).count as u32),
            ("build.excellent", self.excellent() as u32),
        ]
    }
}

/// The cache the `build` command and the boards read.
#[derive(Debug, Default, Clone)]
pub struct BuildScores {
    /// Unix seconds of the last scan. 0 = never run, which readers must treat
    /// as "not ready" rather than "nobody has built anything".
    pub generated_at: i64,
    /// Entities considered — Builder-origin, with an author.
    pub credited_entities: usize,
    /// Entities that exist but credit nobody: seed, import, or unclaimed.
    pub uncredited_entities: usize,
    pub builders: BTreeMap<String, BuilderScore>,
}

impl BuildScores {
    pub fn is_ready(&self) -> bool {
        self.generated_at > 0
    }

    pub fn get(&self, name: &str) -> Option<&BuilderScore> {
        let lc = name.to_lowercase();
        self.builders
            .iter()
            .find(|(k, _)| k.to_lowercase() == lc)
            .map(|(_, v)| v)
    }

    /// Every builder, best first. Ties break on name so the order is stable
    /// between scans.
    pub fn ranked(&self) -> Vec<&BuilderScore> {
        let mut v: Vec<&BuilderScore> = self.builders.values().collect();
        v.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
        v
    }

    /// 1-based placing, or `None` when the builder has no score.
    pub fn placing(&self, name: &str) -> Option<i32> {
        let lc = name.to_lowercase();
        self.ranked()
            .iter()
            .position(|b| b.name.to_lowercase() == lc)
            .map(|i| i as i32 + 1)
    }
}

/// An entity as the scorer sees it: who owns it, what it is, how good it is,
/// and which area it sits in (for diminishing returns).
struct Scored<'a> {
    author: &'a str,
    kind: ContentKind,
    area: Option<uuid::Uuid>,
    grade: Grade,
    multiplier: f32,
}

/// The order diminishing returns are handed out in: **best first**, ties broken
/// on author so a scan is stable between runs.
///
/// A named function rather than a closure because the direction is the whole
/// anti-padding property and it deserves a test of its own — an inverted
/// comparator here would silently pay full weight for rubbish.
fn best_first(a: &Scored, b: &Scored) -> std::cmp::Ordering {
    b.grade.score.cmp(&a.grade.score).then_with(|| a.author.cmp(b.author))
}

/// Score every builder in the world.
///
/// `bounty_points` is the stored addend, keyed by builder name — the one term
/// that is not derived, because a bounty pays for work whose product may be
/// spread across content the claimant does not own.
pub fn compute(snapshot: &WorldSnapshot, bounty_points: &HashMap<String, i32>, generated_at: i64) -> BuildScores {
    let ctx = snapshot.ctx();
    let mut scored: Vec<Scored> = Vec::new();
    let mut uncredited = 0usize;

    // One closure per kind rather than a trait: the grade function and the
    // area accessor differ per type and there are exactly five of them.
    macro_rules! collect {
        ($iter:expr, $kind:expr, $area:expr, $grade:expr, $mult:expr) => {
            for e in $iter {
                if !e.origin.counts_for_score() {
                    uncredited += 1;
                    continue;
                }
                let Some(author) = e.authored_by() else {
                    uncredited += 1;
                    continue;
                };
                scored.push(Scored {
                    author,
                    kind: $kind,
                    area: $area(e),
                    grade: $grade(e),
                    multiplier: $mult(e),
                });
            }
        };
    }

    collect!(
        snapshot.rooms.iter(),
        ContentKind::Room,
        |r: &crate::types::RoomData| r.area_id,
        |r: &crate::types::RoomData| audit::audit_room(r, ctx),
        |_: &crate::types::RoomData| 1.0
    );
    collect!(
        snapshot.items.iter(),
        ContentKind::Item,
        |i: &crate::types::ItemData| i.area_id,
        |i: &crate::types::ItemData| audit::audit_item(i),
        |_: &crate::types::ItemData| 1.0
    );
    collect!(
        snapshot.mobiles.iter(),
        ContentKind::Mobile,
        |m: &crate::types::MobileData| m.area_id,
        |m: &crate::types::MobileData| audit::audit_mobile(m),
        |m: &crate::types::MobileData| {
            if m.dialogue_tree.is_some() {
                DIALOGUE_TREE_MULTIPLIER
            } else {
                1.0
            }
        }
    );
    collect!(
        snapshot.quests.iter(),
        ContentKind::Quest,
        |_: &crate::types::QuestData| None,
        |q: &crate::types::QuestData| audit::audit_quest(q),
        |_: &crate::types::QuestData| 1.0
    );
    // An area is graded on its composite — its own findings blended with the
    // contents — so a builder cannot claim sixty points for an empty shell.
    for a in &snapshot.areas {
        if !a.origin.counts_for_score() {
            uncredited += 1;
            continue;
        }
        let Some(author) = a.authored_by() else {
            uncredited += 1;
            continue;
        };
        let report = crate::audit::scan::scan_area(snapshot, a, ctx);
        scored.push(Scored {
            author,
            kind: ContentKind::Area,
            area: Some(a.id),
            grade: report.as_grade(),
            multiplier: 1.0,
        });
    }

    // Diminishing returns are per (author, area, kind), applied in a stable
    // order: **best first**, so the entities holding the undiminished slots are
    // the good ones. Sorting the other way would let a builder pad an area with
    // rubbish and have the padding claim full weight while their real work took
    // the tail. Pinned by `the_best_work_takes_the_undiminished_slots`.
    scored.sort_by(best_first);

    let mut seen: HashMap<(&str, Option<uuid::Uuid>, ContentKind), usize> = HashMap::new();
    let mut builders: BTreeMap<String, BuilderScore> = BTreeMap::new();
    let mut credited = 0usize;

    for s in &scored {
        credited += 1;
        let slot = seen.entry((s.author, s.area, s.kind)).or_insert(0);
        let factor = diminish(*slot);
        *slot += 1;

        let points = (s.grade.score as f32 * kind_weight(s.kind) as f32 * s.multiplier * factor / 100.0).round() as i32;

        let entry = builders.entry(s.author.to_string()).or_insert_with(|| BuilderScore {
            name: s.author.to_string(),
            ..Default::default()
        });
        entry.content_points += points;
        let tally = entry.tallies.entry(s.kind.key()).or_default();
        tally.count += 1;
        tally.points += points;
        if audit::letter_at_least(s.grade.letter, 'C') {
            tally.good += 1;
        }
        if s.grade.letter == 'A' {
            tally.excellent += 1;
        }
        if s.grade.count(audit::Severity::Blocker) > 0 {
            tally.broken += 1;
        }
    }

    // Bounty points stand alone: a builder who has only ever claimed bounties
    // still has a score, and one whose content was all deleted keeps what the
    // bounties paid, because that work was accepted at the time.
    for (name, pts) in bounty_points {
        if *pts == 0 {
            continue;
        }
        let entry = builders.entry(name.clone()).or_insert_with(|| BuilderScore {
            name: name.clone(),
            ..Default::default()
        });
        entry.bounty_points += *pts;
    }

    for b in builders.values_mut() {
        b.total = b.content_points + b.bounty_points;
    }

    BuildScores {
        generated_at,
        credited_entities: credited,
        uncredited_entities: uncredited,
        builders,
    }
}

/// Reconcile each builder's counters from a finished scan.
///
/// Separated from [`compute`] for the reason `src/progress.rs` separates
/// `report_xp` from `award_xp_to_character`: the computation is pure and
/// testable, and the write goes through `achievements::apply_to_character`,
/// which is the mandatory primitive for touching a character out of band —
/// a plain `save_character_data` on an online player is reverted by the regen
/// tick.
///
/// Counters are *reconciled*, not incremented. A builder who deletes an area
/// watches the numbers fall, which is the point.
/// Reconciled for anyone who **either** holds the builder bit **or** has
/// content credited to them. Filtering on the bit alone froze a demoted
/// builder's counters at their last reading: nothing wrote them again, so they
/// stayed ranked on the Building boards at a score no deletion could move,
/// which is exactly the guarantee this module exists to make.
pub fn apply_to_characters(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    state: &crate::SharedState,
    scores: &BuildScores,
    characters: &[CharacterData],
) {
    for ch in characters {
        let scored = scores.get(&ch.name);
        if !ch.is_builder && !ch.is_admin && scored.is_none() {
            continue;
        }
        // A builder with nothing scored still gets zeroes written, so deleting
        // your last room actually moves the number rather than leaving the
        // last non-zero reading in place forever.
        let empty = BuilderScore::default();
        let counters = scored.unwrap_or(&empty).counters();
        crate::script::achievements::reconcile_counters_core(db, connections, state, &ch.name, &counters);
    }
}

/// Load, compute and install. The tick body, kept lib-side so integration
/// tests can drive it without a tokio runtime — the same split
/// `src/leaderboard.rs` uses and for the same reason.
pub fn process_build_score_tick(
    db: &crate::db::Db,
    connections: &crate::SharedConnections,
    state: &crate::SharedState,
    now: i64,
) -> anyhow::Result<()> {
    let snapshot = WorldSnapshot::load(db)?;
    let characters = db.list_all_characters()?;

    // Bounty points ride on the character, so they come from the same read.
    let bounty: HashMap<String, i32> = characters
        .iter()
        .filter(|c| c.builder_bounty_points != 0)
        .map(|c| (c.name.clone(), c.builder_bounty_points))
        .collect();

    let scores = compute(&snapshot, &bounty, now);

    // Everyone, not just current builders — `apply_to_characters` decides who
    // is eligible, and it has to be able to reach a demoted builder to zero
    // them out.
    apply_to_characters(db, connections, state, &scores, &characters);

    // The world's own verdict rides on the same scan. Computing it separately
    // would mean loading five trees twice to answer two halves of one
    // question.
    let facts = snapshot.facts();
    let quality_pct = crate::audit::scan::world_quality_pct(&snapshot);
    let rating = crate::world_rating::rate(&facts, quality_pct);

    // Milestones are evaluated before the cache is installed, so the
    // announcement and the number a builder then reads cannot disagree.
    if let Err(e) = crate::world_milestones::evaluate(db, connections, state, &facts, &rating, &scores, now) {
        tracing::warn!("world milestone evaluation failed: {e}");
    }

    // The bounty board rides the same scan. Its generated half is the auditor's
    // findings, so recomputing it anywhere else would mean grading the world
    // twice to answer the same question.
    match crate::bounty::expire_claims(db, now) {
        Ok(n) if n > 0 => tracing::info!("{n} bounty claim(s) expired and returned to the board"),
        Err(e) => tracing::warn!("bounty claim expiry failed: {e}"),
        _ => {}
    }
    match crate::bounty::regenerate(db, &snapshot, now) {
        Ok((created, closed)) if created > 0 || closed > 0 => {
            tracing::info!("bounty board: {created} raised, {closed} resolved");
        }
        Err(e) => tracing::warn!("bounty regeneration failed: {e}"),
        _ => {}
    }

    // An error, not a silent skip: a swallowed poisoned lock means the scan
    // ran, the counters and milestones were written, and nothing was installed
    // — so `build` and `world` report "not surveyed yet" forever while the tick
    // logs success every five minutes. `process_leaderboard_tick` treats the
    // same failure as fatal.
    let mut world = state
        .lock()
        .map_err(|_| anyhow::anyhow!("world lock poisoned installing builder scores"))?;
    world.build_scores = scores;
    world.world_report = crate::world_rating::WorldReport {
        generated_at: now,
        facts,
        rating: Some(rating),
        quality_pct,
    };
    // Hand the context the scan already built to the OLC grade toast, so an
    // editor keystroke never has to build its own. See `World::audit_ctx`.
    world.audit_ctx = std::sync::Arc::new(snapshot.ctx().clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ContentOrigin;

    fn ctx_free_grade(score: i32) -> Grade {
        // A grade with the right score, built from nothing — the scoring
        // arithmetic does not care how the findings got there.
        Grade {
            score,
            letter: audit::letter_for(score),
            findings: Vec::new(),
        }
    }

    #[test]
    fn diminishing_returns_never_reach_zero_but_fall_fast_enough_to_matter() {
        assert!((diminish(0) - 1.0).abs() < f32::EPSILON);
        assert!((diminish(40) - 0.5).abs() < 0.01);
        assert!(diminish(1000) > 0.0);
        // The sixtieth room in an area is worth well under half the first.
        assert!(diminish(60) < 0.45);
    }

    #[test]
    fn the_best_work_takes_the_undiminished_slots() {
        // Direction matters and nothing else pins it. If this comparator is
        // ever inverted, a builder can pad an area with rubbish and have the
        // padding collect full weight while their real work takes the tail.
        let mk = |score: i32, author: &'static str| Scored {
            author,
            kind: ContentKind::Room,
            area: None,
            grade: ctx_free_grade(score),
            multiplier: 1.0,
        };
        let mut v = [mk(10, "ana"), mk(95, "ana"), mk(50, "ana")];
        v.sort_by(best_first);
        assert_eq!(v.iter().map(|s| s.grade.score).collect::<Vec<_>>(), vec![95, 50, 10]);
    }

    #[test]
    fn kind_weights_are_ordered_by_cost() {
        assert!(kind_weight(ContentKind::Area) > kind_weight(ContentKind::Quest));
        assert!(kind_weight(ContentKind::Quest) > kind_weight(ContentKind::Mobile));
        assert!(kind_weight(ContentKind::Mobile) > kind_weight(ContentKind::Item));
        assert!(kind_weight(ContentKind::Item) > kind_weight(ContentKind::Room));
    }

    #[test]
    fn a_perfect_room_is_worth_its_full_weight() {
        let g = ctx_free_grade(100);
        let pts = (g.score as f32 * kind_weight(ContentKind::Room) as f32 * 1.0 * diminish(0) / 100.0).round();
        assert_eq!(pts as i32, kind_weight(ContentKind::Room));
    }

    #[test]
    fn an_f_grade_room_is_worth_almost_nothing() {
        let g = ctx_free_grade(10);
        let pts = (g.score as f32 * kind_weight(ContentKind::Room) as f32 * diminish(0) / 100.0).round() as i32;
        assert_eq!(pts, 1);
    }

    #[test]
    fn the_counter_set_is_stable() {
        // Every key here becomes a leaderboard and can be an achievement
        // target, so adding or renaming one is a public change.
        let s = BuilderScore::default();
        let keys: Vec<&str> = s.counters().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec![
                "build.score",
                "build.rooms",
                "build.items",
                "build.mobiles",
                "build.quests",
                "build.areas",
                "build.excellent",
            ]
        );
    }

    #[test]
    fn ranking_is_stable_across_ties() {
        let mut scores = BuildScores::default();
        for name in ["Bo", "Ana", "Cy"] {
            scores.builders.insert(
                name.to_string(),
                BuilderScore {
                    name: name.to_string(),
                    total: 100,
                    ..Default::default()
                },
            );
        }
        let order: Vec<&str> = scores.ranked().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(order, vec!["Ana", "Bo", "Cy"]);
        assert_eq!(scores.placing("bo"), Some(2), "lookup must be case-insensitive");
        assert_eq!(scores.placing("nobody"), None);
    }

    #[test]
    fn origins_that_do_not_count_are_counted_as_uncredited() {
        for o in [ContentOrigin::Seed, ContentOrigin::Import, ContentOrigin::Unknown] {
            assert!(!o.counts_for_score(), "{o:?} must not be scored");
        }
        assert!(ContentOrigin::Builder.counts_for_score());
    }
}
