//! The author's fame (§12, S9): author counters reconciled from the codex,
//! cast milestones, and the daily scan that lets forgotten spells go Lost.
//!
//! Counters are *reconciled*, like `build_score::apply_to_characters`: the
//! codex is the authority, so an admin rename, a veto or a restoration that
//! moves authorship is reflected on the next pass without anyone bumping
//! anything.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};

use super::ritual::{AUTHORED_COUNTER, RESTORED_COUNTER};
use crate::db::Db;
use crate::types::{CanonStatus, CharacterData, CodexEntry, ItemData, LineageKind};
use crate::{SharedConnections, SharedState};

/// Lifetime casts of a character's spells by other hands.
pub const CANON_CASTS_COUNTER: &str = "spells.canon_casts";
/// Scrolls a character has inscribed (`craft scroll`).
pub const SCRIBED_COUNTER: &str = "spells.scribed";
/// Casts by others at which a spell's author hears of it.
pub const MILESTONES: [u64; 3] = [100, 1_000, 10_000];

/// The highest milestone `before < m <= after`, if any.
pub fn milestone_crossed(before: u64, after: u64) -> Option<u64> {
    MILESTONES.iter().rev().copied().find(|m| before < *m && after >= *m)
}

/// One character's standing in the codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthorFame {
    /// Spells they first sealed (lineage `authored`).
    pub authored: u32,
    /// Faded spells they brought back (lineage `restored`).
    pub restored: u32,
    /// Casts by others of the spells they currently author.
    pub canon_casts: u32,
}

/// Fame per lowercased character name, from the codex alone (pure).
pub fn compute(entries: &[CodexEntry]) -> HashMap<String, AuthorFame> {
    let mut out: HashMap<String, AuthorFame> = HashMap::new();
    for e in entries {
        for l in &e.meta.lineage {
            let f = out.entry(l.who.to_lowercase()).or_default();
            match l.kind {
                LineageKind::Authored => f.authored += 1,
                LineageKind::Restored => f.restored += 1,
            }
        }
        if let Some(a) = &e.meta.author {
            let f = out.entry(a.to_lowercase()).or_default();
            f.canon_casts = f
                .canon_casts
                .saturating_add(e.meta.casts_by_others.min(u32::MAX as u64) as u32);
        }
    }
    out
}

/// Write each character's fame counters. Anyone with fame, or with a fame
/// counter already on them (so losing it moves the number), is written.
pub fn reconcile(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    chars: &[CharacterData],
    entries: &[CodexEntry],
) {
    let fame = compute(entries);
    let keys = [AUTHORED_COUNTER, RESTORED_COUNTER, CANON_CASTS_COUNTER];
    for ch in chars {
        let f = fame.get(&ch.name.to_lowercase()).copied();
        let had = keys.iter().any(|k| ch.achievement_counters.contains_key(*k));
        if f.is_none() && !had {
            continue;
        }
        let f = f.unwrap_or_default();
        crate::script::achievements::reconcile_counters_core(
            db,
            connections,
            state,
            &ch.name,
            &[
                (AUTHORED_COUNTER, f.authored),
                (RESTORED_COUNTER, f.restored),
                (CANON_CASTS_COUNTER, f.canon_casts),
            ],
        );
    }
}

// ---------- the Lost scan ----------

/// Faded spells nobody alive knows and no item teaches (pure). Canon spells
/// never go Lost: their god's priests still teach them.
pub fn lost_candidates(entries: &[CodexEntry], chars: &[CharacterData], items: &[ItemData]) -> Vec<String> {
    let known: HashSet<&str> = chars
        .iter()
        .flat_map(|c| c.learned_spells.iter().map(String::as_str))
        .collect();
    let written: HashSet<&str> = items.iter().filter_map(|i| i.teaches_spell.as_deref()).collect();
    entries
        .iter()
        .filter(|e| e.meta.status == CanonStatus::Faded)
        .map(|e| e.def.id.as_str())
        .filter(|id| !known.contains(id) && !written.contains(id))
        .map(str::to_string)
        .collect()
}

/// Game day of the last Lost scan in this process.
static LAST_LOST_SCAN: AtomicI64 = AtomicI64::new(i64::MIN);

/// Once per game day: Faded spells nobody knows and no scroll or prototype
/// teaches go Lost. Reads every item, so it stays daily. Returns the ids lost.
pub fn scan_lost_daily(db: &Db, state: &SharedState, chars: &[CharacterData], entries: &[CodexEntry]) -> Vec<String> {
    let today = crate::script::worship::current_absolute_day(db);
    if LAST_LOST_SCAN.swap(today, Ordering::Relaxed) == today {
        return Vec::new();
    }
    if !entries.iter().any(|e| e.meta.status == CanonStatus::Faded) {
        return Vec::new();
    }
    let items = db.list_all_items().unwrap_or_default();
    let lost = lost_candidates(entries, chars, &items);
    for id in &lost {
        if let Err(e) = super::codex::set_status(db, state, id, CanonStatus::Lost) {
            tracing::warn!("lost scan: {id}: {e}");
        }
    }
    if !lost.is_empty() {
        tracing::info!("codex: {} forgotten spell(s) went Lost", lost.len());
    }
    lost
}

/// The codex pass the leaderboard tick runs after the devotion drain: fame
/// counters, then the daily Lost scan.
pub fn process(db: &Db, connections: &SharedConnections, state: &SharedState, chars: &[CharacterData]) {
    let entries = db.list_codex().unwrap_or_default();
    if entries.is_empty() {
        return;
    }
    reconcile(db, connections, state, chars, &entries);
    scan_lost_daily(db, state, chars, &entries);
}
