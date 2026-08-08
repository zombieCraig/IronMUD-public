//! Recording what the world has become, and crediting whoever built it.
//!
//! `crate::world_rating` decides *whether* a milestone is met. This does the
//! I/O: remembers that it happened, tells the builders, and hands each of them
//! the achievement.
//!
//! # Why two stores
//!
//! A world milestone is not a personal achievement, and forcing it to be one
//! would lose the fact that matters — *the world* passed a thousand rooms, and
//! it did so once, at a particular time, with a particular set of people
//! responsible. `achievement_counters` lives on `CharacterData` and has no way
//! to say that.
//!
//! So the global fact lives in its own sled tree, and each contributor is
//! *also* handed a normal achievement through the existing manual-award path.
//! That way the wall in `world` is the world's history, and `achievements`
//! still shows a builder everything they have to their name — without a second
//! achievement engine.
//!
//! The contributor list is "every builder with a score at the moment it
//! unlocked". It is deliberately not "everyone who has ever built anything":
//! a milestone is a snapshot of who was carrying the world when it crossed.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::audit::WorldFacts;
use crate::build_score::BuildScores;
use crate::db::Db;
use crate::world_rating::{WORLD_GOALS, WorldGoal, WorldRating};
use crate::{SharedConnections, SharedState};

/// One world milestone, once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMilestone {
    /// Matches an `AchievementDef` key in
    /// `scripts/data/achievements/world.json`.
    pub key: String,
    pub unlocked_at: i64,
    /// Builders carrying a score when it crossed.
    #[serde(default)]
    pub contributors: Vec<String>,
}

/// Evaluate every goal, record what is newly met, credit the builders, and
/// tell everyone who can hear it.
///
/// Returns the milestones unlocked by *this* call — empty on every run after
/// the first that crossed them, which is what makes it safe to run every tick.
pub fn evaluate(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    facts: &WorldFacts,
    rating: &WorldRating,
    scores: &BuildScores,
    now: i64,
) -> Result<Vec<String>> {
    let already = db.world_milestone_keys()?;

    let contributors: Vec<String> = scores
        .ranked()
        .into_iter()
        .filter(|b| b.total > 0)
        .map(|b| b.name.clone())
        .collect();

    // Nothing is recorded until somebody has built something.
    //
    // `WorldFacts` counts content of every origin, because the size of the
    // world is the size of the world. `contributors` is origin-gated. Import a
    // CircleMUD world and the two disagree completely: a dozen milestones are
    // met on the first tick, every one of them announced, permanently consumed,
    // and credited to nobody — which is the exact opposite of a record of
    // building. Holding them until at least one builder is on the board costs a
    // real world nothing (the first hand-built room releases them) and spares
    // an imported one a boot-time storm of achievements nobody earned.
    if contributors.is_empty() {
        return Ok(Vec::new());
    }

    let mut fresh = Vec::new();
    for (key, goal) in WORLD_GOALS {
        if already.contains(*key) || !goal.is_met(facts, rating) {
            continue;
        }
        db.save_world_milestone(&WorldMilestone {
            key: (*key).to_string(),
            unlocked_at: now,
            contributors: contributors.clone(),
        })?;
        fresh.push((*key).to_string());

        // The one interruption in the whole builder tier. A milestone is rare
        // by construction — every threshold sits above what the demo world
        // already has — so it is allowed to stop the room.
        //
        // `announce_to_builders`, not `broadcast_to_builders`: the latter is
        // the builder *debug* channel and only reaches people who opted into
        // debug output, which is nobody at the moment this fires.
        let label = crate::script::achievements::describe(state, key).unwrap_or_else(|| (*key).to_string());
        crate::session::broadcast::announce_to_builders(
            connections,
            &format!("\x1b[1;33m*** The world has reached: {label} ***\x1b[0m"),
        );

        // And each contributor gets it on their own sheet, through the normal
        // manual-award path — same banner, same title, same `achievements`
        // listing as everything else they have earned.
        for name in &contributors {
            crate::script::achievements::award_core(db, connections, state, name, key, true);
        }
    }

    Ok(fresh)
}

/// Every milestone, unlocked or not, for the `world` command's wall.
pub struct MilestoneRow {
    pub key: String,
    pub goal: WorldGoal,
    pub unlocked_at: Option<i64>,
    pub contributors: Vec<String>,
    pub have: i64,
    pub want: i64,
    /// Progress as a human reads it. Tier goals are rendered with their tier
    /// names — "Village / Town" says something, "3 / 4" says nothing.
    pub progress_text: String,
}

pub fn wall(db: &Db, facts: &WorldFacts, rating: &WorldRating) -> Result<Vec<MilestoneRow>> {
    let recorded: std::collections::HashMap<String, WorldMilestone> = db
        .list_world_milestones()?
        .into_iter()
        .map(|m| (m.key.clone(), m))
        .collect();

    Ok(WORLD_GOALS
        .iter()
        .map(|(key, goal)| {
            let (have, want) = goal.progress(facts, rating);
            let rec = recorded.get(*key);
            let progress_text = match goal {
                WorldGoal::Tier(_) => format!(
                    "{} / {}",
                    crate::world_rating::LADDER
                        .tiers
                        .get(have.max(0) as usize)
                        .map(|t| t.label)
                        .unwrap_or("?"),
                    crate::world_rating::LADDER
                        .tiers
                        .get(want.max(0) as usize)
                        .map(|t| t.label)
                        .unwrap_or("?"),
                ),
                _ => format!("{have} / {want}"),
            };
            MilestoneRow {
                key: (*key).to_string(),
                goal: *goal,
                unlocked_at: rec.map(|m| m.unlocked_at),
                contributors: rec.map(|m| m.contributors.clone()).unwrap_or_default(),
                have,
                want,
                progress_text,
            }
        })
        .collect())
}
