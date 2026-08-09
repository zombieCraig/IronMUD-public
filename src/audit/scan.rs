//! Turning a database into audit reports.
//!
//! The checks in the parent module are pure by design — they take data and
//! return findings, and they never learn where the data came from. This is the
//! other half: the gathering. It lives beside them rather than inside a script
//! binding because three different callers need it (the `build audit` command,
//! the score tick, and the bounty generator) and each one loading the world its
//! own way is how the counts start disagreeing.
//!
//! Everything here is a read. Nothing in this file writes.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::{
    AreaContents, AuditCtx, AuditEntry, AuditReport, EntityKind, Grade, WorldFacts, audit_item, audit_mobile,
    audit_room, audit_world, report_area,
};
use crate::db::Db;
use crate::types::{AreaData, Authored, ItemData, ItemType, MobileData, Provenance, QuestData, RoomData};

/// Everything the auditor needs, loaded once.
///
/// A world audit touches the room, item, mobile, quest and spawn-point trees.
/// Loading them per-area would re-read all five once per area, so callers load
/// this and slice it.
pub struct WorldSnapshot {
    pub areas: Vec<AreaData>,
    pub rooms: Vec<RoomData>,
    /// Prototypes only — instances are spawned content, not authored content.
    pub items: Vec<ItemData>,
    pub mobiles: Vec<MobileData>,
    pub quests: Vec<QuestData>,
    /// Spawn points per area id.
    pub spawn_counts: HashMap<Uuid, usize>,
    pub spawn_total: usize,
    pub recipe_count: usize,
    pub transport_count: usize,
    /// Built once, on first ask. Three callers want a context over one snapshot
    /// — the scorer, the quality figure and the bounty generator — and each
    /// rebuild hashes every room description and rebuilds the inbound map for
    /// no new information.
    ctx: std::sync::OnceLock<AuditCtx>,
}

/// Does this area answer to `needle`? Matches uuid, prefix or name.
///
/// Case- and whitespace-insensitive on *both* sides. Every call site trimmed
/// the needle and none of them trimmed the stored value, which is not a
/// symmetry anyone can be relied on to remember: an area name arrives from the
/// REST/MCP path length-checked but otherwise verbatim
/// (`crate::api::areas` create/update, and `set_area_name`), so a name with a
/// leading space is a name no lookup could reach. One predicate, five callers.
pub fn area_matches(area: &AreaData, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    let lower = needle.to_lowercase();
    area.id.to_string() == lower
        || area.prefix.trim().to_lowercase() == lower
        || area.name.trim().to_lowercase() == lower
}

impl WorldSnapshot {
    pub fn load(db: &Db) -> Result<WorldSnapshot> {
        let areas = db.list_all_areas()?;
        let rooms = db.list_all_rooms()?;
        let items: Vec<ItemData> = db.list_all_items()?.into_iter().filter(|i| i.is_prototype).collect();
        let mobiles: Vec<MobileData> = db.list_all_mobiles()?.into_iter().filter(|m| m.is_prototype).collect();
        let quests = db.list_all_quests()?;

        let mut spawn_counts: HashMap<Uuid, usize> = HashMap::new();
        let spawns = db.list_all_spawn_points()?;
        let spawn_total = spawns.len();
        // A spawn point belongs to the area of the room it fires in.
        let room_area: HashMap<Uuid, Option<Uuid>> = rooms.iter().map(|r| (r.id, r.area_id)).collect();
        for sp in &spawns {
            if let Some(Some(area_id)) = room_area.get(&sp.room_id) {
                *spawn_counts.entry(*area_id).or_insert(0) += 1;
            }
        }

        Ok(WorldSnapshot {
            areas,
            rooms,
            items,
            mobiles,
            quests,
            spawn_counts,
            spawn_total,
            recipe_count: db.list_all_recipes().map(|v| v.len()).unwrap_or(0),
            transport_count: db.list_all_transports().map(|v| v.len()).unwrap_or(0),
            ctx: std::sync::OnceLock::new(),
        })
    }

    /// A linked context over every room in the world.
    ///
    /// Always the full room set, even when auditing one area: an area-scoped
    /// context would call every exit leading out of the area dangling, which is
    /// the exact opposite of the truth.
    pub fn ctx(&self) -> &AuditCtx {
        self.ctx.get_or_init(|| AuditCtx::build(&self.rooms))
    }

    pub fn area_contents(&self, area_id: Uuid) -> AreaContentsOwned {
        AreaContentsOwned {
            rooms: self
                .rooms
                .iter()
                .filter(|r| r.area_id == Some(area_id))
                .cloned()
                .collect(),
            items: self
                .items
                .iter()
                .filter(|i| i.area_id == Some(area_id))
                .cloned()
                .collect(),
            mobiles: self
                .mobiles
                .iter()
                .filter(|m| m.area_id == Some(area_id))
                .cloned()
                .collect(),
            quests: self.quests_for_area(area_id),
            spawn_point_count: self.spawn_counts.get(&area_id).copied().unwrap_or(0),
        }
    }

    /// A quest belongs to an area when its giver mobile does. Quests carry no
    /// `area_id` of their own, and inventing one here would be a schema change
    /// wearing a helper's clothes.
    fn quests_for_area(&self, area_id: Uuid) -> Vec<QuestData> {
        let givers: HashSet<&str> = self
            .mobiles
            .iter()
            .filter(|m| m.area_id == Some(area_id))
            .map(|m| m.vnum.as_str())
            .collect();
        self.quests
            .iter()
            .filter(|q| q.giver_mob_vnum.as_deref().is_some_and(|v| givers.contains(v)))
            .cloned()
            .collect()
    }

    /// Rooms whose area is unset. They are real content that no area audit will
    /// ever reach, so the world report has to account for them itself.
    pub fn orphan_room_count(&self) -> usize {
        self.rooms.iter().filter(|r| r.area_id.is_none()).count()
    }

    pub fn facts(&self) -> WorldFacts {
        let room_area: HashMap<Uuid, Option<Uuid>> = self.rooms.iter().map(|r| (r.id, r.area_id)).collect();
        let mut connected: HashSet<Uuid> = HashSet::new();
        for room in &self.rooms {
            let Some(home) = room.area_id else { continue };
            for (_, dest) in super::exit_pairs(&room.exits) {
                if let Some(Some(other)) = room_area.get(&dest)
                    && *other != home
                {
                    connected.insert(home);
                    connected.insert(*other);
                }
            }
        }

        WorldFacts {
            area_count: self.areas.len(),
            room_count: self.rooms.len(),
            item_count: self.items.len(),
            mobile_count: self.mobiles.len(),
            quest_count: self.quests.len(),
            spawn_point_count: self.spawn_total,
            recipe_count: self.recipe_count,
            transport_count: self.transport_count,
            post_office_rooms: self.rooms.iter().filter(|r| r.flags.post_office).count(),
            board_items: self.items.iter().filter(|i| i.item_type == ItemType::Board).count(),
            recall_rooms: self.rooms.iter().filter(|r| r.flags.spawn_point).count(),
            bank_rooms: self.rooms.iter().filter(|r| r.flags.bank).count(),
            connected_areas: connected.len(),
            dialogue_trees: self.mobiles.iter().filter(|m| m.dialogue_tree.is_some()).count(),
            unfiled_mobiles: self.mobiles.iter().filter(|m| m.area_id.is_none()).count(),
            unfiled_items: self.items.iter().filter(|i| i.area_id.is_none()).count(),
        }
    }
}

/// Owned mirror of [`AreaContents`], which borrows. The snapshot filters into
/// this, then hands out a borrowing view.
pub struct AreaContentsOwned {
    pub rooms: Vec<RoomData>,
    pub items: Vec<ItemData>,
    pub mobiles: Vec<MobileData>,
    pub quests: Vec<QuestData>,
    pub spawn_point_count: usize,
}

impl AreaContentsOwned {
    pub fn view(&self) -> AreaContents<'_> {
        AreaContents {
            rooms: &self.rooms,
            items: &self.items,
            mobiles: &self.mobiles,
            quests: &self.quests,
            spawn_point_count: self.spawn_point_count,
        }
    }
}

/// Grade one area, with the world as context.
pub fn scan_area(snapshot: &WorldSnapshot, area: &AreaData, ctx: &AuditCtx) -> AuditReport {
    let contents = snapshot.area_contents(area.id);
    report_area(area, &contents.view(), ctx)
}

/// Grade the whole world: world-level checks plus one entry per area.
///
/// The entries are *areas*, not rooms, so the composite answers "how good are
/// this world's areas on average" rather than being dominated by whichever area
/// happens to have the most rooms. Rooms with no area are folded in as their own
/// entries so they cannot hide.
pub fn scan_world(snapshot: &WorldSnapshot) -> AuditReport {
    let ctx = snapshot.ctx();
    let mut entries: Vec<AuditEntry> = Vec::new();

    for area in &snapshot.areas {
        let report = scan_area(snapshot, area, ctx);
        entries.push(AuditEntry {
            kind: EntityKind::Area,
            label: area.prefix.clone(),
            name: area.name.clone(),
            grade: report.as_grade(),
        });
    }

    for room in snapshot.rooms.iter().filter(|r| r.area_id.is_none()) {
        entries.push(AuditEntry {
            kind: EntityKind::Room,
            label: room.vnum.clone().unwrap_or_default(),
            name: room.title.clone(),
            grade: audit_room(room, ctx),
        });
    }

    AuditReport::build("World", audit_world(&snapshot.facts()), entries)
}

/// One entity's verdict, plus who is responsible for it.
///
/// Grade and credit are read together because they are asked together: the
/// first thing a builder wants after "how good is this?" is "whose is it?".
pub struct EntityAudit {
    /// vnum where one exists, otherwise a uuid string.
    pub label: String,
    pub name: String,
    pub grade: Grade,
    pub provenance: Provenance,
}

/// Grade one entity by kind and key. `key` is a vnum, or a uuid string.
///
/// Returns `None` when nothing matches, which callers render as "not found"
/// rather than as a failing grade — an entity that does not exist has no
/// quality.
pub fn scan_entity(db: &Db, kind: EntityKind, key: &str) -> Result<Option<EntityAudit>> {
    scan_entity_with_ctx(db, kind, key, None)
}

/// [`scan_entity`] against a context the caller already has.
///
/// Only rooms read the context, and only for the exit-relationship checks.
/// Passing one turns a full-world read into a single keyed fetch, which is why
/// the grade toast holds a cached context on `World` rather than building one
/// per keystroke.
pub fn scan_entity_with_ctx(
    db: &Db,
    kind: EntityKind,
    key: &str,
    ctx: Option<&AuditCtx>,
) -> Result<Option<EntityAudit>> {
    let key_lower = key.trim().to_lowercase();
    let as_uuid = Uuid::parse_str(key.trim()).ok();

    match kind {
        EntityKind::Room => {
            // Fetch the one room by key rather than scanning the tree, and take
            // the context from the caller when it has one. `list_all_rooms` +
            // `AuditCtx::build` here is what made the grade toast the most
            // expensive thing an OLC editor does.
            let found = match as_uuid {
                Some(id) => db.get_room_data(&id)?,
                // `get_room_by_vnum` reads `vnum_index`, which only
                // `set_room_vnum` and the boot-time rebuild maintain — a room
                // saved with its vnum field set directly is not in it. The
                // scan is the fallback, so an unindexed room still grades.
                None => match db.get_room_by_vnum(&key_lower)? {
                    Some(r) => Some(r),
                    None => db
                        .list_all_rooms()?
                        .into_iter()
                        .find(|r| r.vnum.as_deref().map(str::to_lowercase) == Some(key_lower.clone())),
                },
            };
            let owned;
            let ctx = match ctx {
                Some(c) => c,
                None => {
                    owned = AuditCtx::build(&db.list_all_rooms()?);
                    &owned
                }
            };
            Ok(found.map(|r| EntityAudit {
                label: r.vnum.clone().unwrap_or_else(|| r.id.to_string()),
                name: r.title.clone(),
                grade: audit_room(&r, ctx),
                provenance: r.provenance(),
            }))
        }
        EntityKind::Item => {
            let found = match as_uuid {
                Some(id) => db.get_item_data(&id)?,
                None => db.get_item_by_vnum(&key_lower)?,
            }
            .filter(|i| i.is_prototype);
            Ok(found.map(|i| EntityAudit {
                label: i.vnum.clone().unwrap_or_else(|| i.id.to_string()),
                name: i.short_desc.clone(),
                grade: audit_item(&i),
                provenance: i.provenance(),
            }))
        }
        EntityKind::Mobile => {
            let found = match as_uuid {
                Some(id) => db.get_mobile_data(&id)?,
                None => db.get_mobile_by_vnum(&key_lower)?,
            }
            .filter(|m| m.is_prototype);
            Ok(found.map(|m| EntityAudit {
                label: m.vnum.clone(),
                name: m.short_desc.clone(),
                grade: audit_mobile(&m),
                provenance: m.provenance(),
            }))
        }
        EntityKind::Quest => {
            let found = db
                .list_all_quests()?
                .into_iter()
                .find(|q| q.vnum.to_lowercase() == key_lower);
            Ok(found.map(|q| EntityAudit {
                label: q.vnum.clone(),
                name: q.name.clone(),
                grade: super::audit_quest(&q),
                provenance: q.provenance(),
            }))
        }
        EntityKind::Area => {
            let snapshot = WorldSnapshot::load(db)?;
            let ctx = snapshot.ctx();
            let found = snapshot
                .areas
                .iter()
                .find(|a| as_uuid == Some(a.id) || area_matches(a, key));
            Ok(found.map(|a| {
                let report = scan_area(&snapshot, a, ctx);
                EntityAudit {
                    label: a.prefix.clone(),
                    name: a.name.clone(),
                    grade: report.as_grade(),
                    provenance: a.provenance(),
                }
            }))
        }
    }
}

/// Share of every graded entity in the world at C or better.
///
/// The world rating's quality term. Deliberately covers *all* content, not
/// just builder-authored content: this measures the world a player walks
/// through, and a player does not care who wrote the room.
///
/// A world with nothing in it reads as 0 rather than 100 — "vacuously
/// perfect" is the wrong answer from a figure whose job is to say how far
/// along something is.
pub fn world_quality_pct(snapshot: &WorldSnapshot) -> i32 {
    let ctx = snapshot.ctx();
    let mut good = 0usize;
    let mut total = 0usize;
    let mut tally = |g: super::Grade| {
        total += 1;
        if super::letter_at_least(g.letter, 'C') {
            good += 1;
        }
    };

    for r in &snapshot.rooms {
        tally(audit_room(r, ctx));
    }
    for i in &snapshot.items {
        tally(audit_item(i));
    }
    for m in &snapshot.mobiles {
        tally(audit_mobile(m));
    }
    for q in &snapshot.quests {
        tally(super::audit_quest(q));
    }

    if total == 0 {
        return 0;
    }
    (good * 100 / total) as i32
}

// ===========================================================================
// The save-time grade toast
// ===========================================================================

/// A grade snapshot taken just before a builder changed something.
///
/// Parked on the session by `note_grade_before` and drained after the command
/// finishes. This is the same shape as the XP feed's `xp_buffer`
/// (`src/progress.rs`) and for the same reason: the interesting fact is a
/// *change*, and a change needs a before as well as an after.
#[derive(Debug, Clone)]
pub struct PendingAudit {
    pub kind: crate::types::ContentKind,
    pub key: String,
    /// `None` when the entity did not exist yet — a creation, which has no
    /// previous grade to move from.
    pub before: Option<(i32, char)>,
}

/// Snapshot an entity's grade, or `None` if it does not exist yet.
///
/// `ctx` is the cached room context from `World::audit_ctx`. See the field's
/// doc for why the toast reads a cached one rather than building its own.
pub fn grade_snapshot(db: &Db, kind: crate::types::ContentKind, key: &str, ctx: &AuditCtx) -> PendingAudit {
    let before = scan_entity_with_ctx(db, kind, key, Some(ctx))
        .ok()
        .flatten()
        .map(|e| (e.grade.score, e.grade.letter));
    PendingAudit {
        kind,
        key: key.to_string(),
        before,
    }
}

/// The one line a builder sees after an edit, or `None` when there is nothing
/// worth saying.
///
/// **Silence when the letter has not moved** is the whole design. `src/tiers.rs`
/// states the rule for named tiers — "a move inside a band says nothing, and a
/// move across one announces itself" — and it applies just as well to letters:
/// a line after every single setter would be a ticker, and a ticker is noise.
///
/// A creation always reports, because "it exists now, and here is what it is"
/// is a change from nothing.
pub fn grade_change_line(db: &Db, pending: &PendingAudit, ctx: &AuditCtx) -> Option<String> {
    let after = scan_entity_with_ctx(db, pending.kind, &pending.key, Some(ctx))
        .ok()
        .flatten()?;

    let (verb, colour) = match pending.before {
        None => ("graded", "\x1b[0;36m"),
        Some((_, before_letter)) => {
            if before_letter == after.grade.letter {
                return None;
            }
            if super::letter_rank(after.grade.letter) < super::letter_rank(before_letter) {
                ("now grades", "\x1b[1;32m")
            } else {
                ("now grades", "\x1b[1;33m")
            }
        }
    };

    let was = match pending.before {
        Some((_, l)) => format!(" (was {l})"),
        None => String::new(),
    };

    let blockers = after.grade.count(super::Severity::Blocker);
    let warns = after.grade.count(super::Severity::Warn);
    let tail = if blockers > 0 {
        format!("  \x1b[1;31m{blockers} blocker(s) left\x1b[0m")
    } else if warns > 0 {
        format!("  \x1b[0;37m{warns} warning(s) left\x1b[0m")
    } else {
        "  \x1b[0;32mnothing left to fix\x1b[0m".to_string()
    };

    Some(format!(
        "{colour}{} {verb} {}{was}.\x1b[0m{tail}",
        after.name.trim(),
        after.grade.letter
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn area(name: &str, prefix: &str) -> AreaData {
        serde_json::from_value(json!({
            "id": Uuid::new_v4(),
            "name": name,
            "prefix": prefix,
        }))
        .expect("area fixture")
    }

    #[test]
    fn an_area_answers_to_prefix_name_and_uuid() {
        let a = area("Dungeon Crawl Level 1", "dungeon1");
        assert!(area_matches(&a, "dungeon1"));
        assert!(area_matches(&a, "DUNGEON1"));
        assert!(area_matches(&a, "Dungeon Crawl Level 1"));
        assert!(area_matches(&a, "dungeon crawl level 1"));
        assert!(area_matches(&a, &a.id.to_string()));
    }

    #[test]
    fn padding_on_the_stored_name_does_not_hide_an_area() {
        // The reported bug. Every call site trimmed the needle and none of them
        // trimmed the stored value, so an area whose name picked up a leading
        // space through the API path was unreachable by name.
        let a = area(" Dungeon Crawl Level 1", "dungeon1");
        assert!(area_matches(&a, "Dungeon Crawl Level 1"));
        assert!(area_matches(&a, " Dungeon Crawl Level 1 "));
        assert!(area_matches(&a, "dungeon1"));
    }

    #[test]
    fn a_different_area_does_not_match() {
        let a = area("Midgaard", "midgaard");
        assert!(!area_matches(&a, "dungeon1"));
        assert!(!area_matches(&a, "midg"));
    }

    #[test]
    fn an_empty_needle_matches_nothing() {
        // An area with an empty prefix must not become the answer to every
        // keyless lookup in the world.
        let a = area("Nameless", "");
        assert!(!area_matches(&a, ""));
        assert!(!area_matches(&a, "   "));
    }
}
