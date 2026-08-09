//! Stamping authorship onto content, and reading it back.
//!
//! `crate::types::provenance` owns the *rules* — who a create claims for, what
//! an edit is allowed to touch. This module owns the *I/O*: load the entity,
//! apply the rule, save it. Every path that creates or edits builder content
//! goes through here, which is the only way the rules stay the same across
//! four surfaces that have nothing else in common.
//!
//! # Why not hook the database
//!
//! `Db::save_room_data` and its three siblings are the true chokepoints —
//! every write in the engine passes through them. They are also the wrong
//! place, for two reasons that are worth writing down so nobody tries it
//! again:
//!
//! * **They carry no actor.** A save knows what changed, never who changed it.
//! * **They are not builder-only.** The combat tick, the weather tick, the
//!   migration system and the spawn system all save rooms and mobiles. Hooking
//!   the save layer would credit a builder for every wandering NPC that
//!   stepped through their area.
//!
//! So the stamps live at the two surfaces where a builder is unambiguously the
//! actor: the REST/MCP handlers (which already resolve an
//! `AuthenticatedUser`) and the OLC editors (which already gate on
//! `can_edit_area`). Both were already doing a permission check at exactly the
//! right moment; the stamp goes next to it.

use anyhow::Result;
use uuid::Uuid;

use crate::db::Db;
use crate::types::{Authored, ContentKind, ContentOrigin, Provenance};

/// A specific entity. Rooms, items, mobiles and areas are keyed by uuid;
/// quests are keyed by vnum, which is why this is an enum rather than a
/// `(kind, uuid)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentRef {
    Room(Uuid),
    Item(Uuid),
    Mobile(Uuid),
    Area(Uuid),
    Quest(String),
}

impl ContentRef {
    pub fn kind(&self) -> ContentKind {
        match self {
            ContentRef::Room(_) => ContentKind::Room,
            ContentRef::Item(_) => ContentKind::Item,
            ContentRef::Mobile(_) => ContentKind::Mobile,
            ContentRef::Area(_) => ContentKind::Area,
            ContentRef::Quest(_) => ContentKind::Quest,
        }
    }

    /// Parse `kind` + a key string. Returns `None` for an unknown kind, or a
    /// uuid-keyed kind whose key is not a uuid.
    pub fn parse(kind: &str, key: &str) -> Option<ContentRef> {
        let kind = ContentKind::from_key(kind)?;
        let key = key.trim();
        match kind {
            ContentKind::Quest => Some(ContentRef::Quest(key.to_string())),
            ContentKind::Room => Uuid::parse_str(key).ok().map(ContentRef::Room),
            ContentKind::Item => Uuid::parse_str(key).ok().map(ContentRef::Item),
            ContentKind::Mobile => Uuid::parse_str(key).ok().map(ContentRef::Mobile),
            ContentKind::Area => Uuid::parse_str(key).ok().map(ContentRef::Area),
        }
    }
}

/// Load, apply `f` to the provenance-carrying entity, save. `Ok(false)` means
/// the entity does not exist.
///
/// One generic body rather than five, so an entity kind cannot quietly get
/// different stamping behaviour from its siblings.
fn mutate(db: &Db, target: &ContentRef, f: impl FnOnce(&mut dyn Authored)) -> Result<bool> {
    match target {
        ContentRef::Room(id) => match db.get_room_data(id)? {
            Some(mut e) => {
                f(&mut e);
                db.save_room_data(e)?;
                Ok(true)
            }
            None => Ok(false),
        },
        ContentRef::Item(id) => match db.get_item_data(id)? {
            Some(mut e) => {
                f(&mut e);
                db.save_item_data(e)?;
                Ok(true)
            }
            None => Ok(false),
        },
        ContentRef::Mobile(id) => match db.get_mobile_data(id)? {
            Some(mut e) => {
                f(&mut e);
                db.save_mobile_data(e)?;
                Ok(true)
            }
            None => Ok(false),
        },
        ContentRef::Area(id) => match db.get_area_data(id)? {
            Some(mut e) => {
                f(&mut e);
                db.save_area_data(e)?;
                Ok(true)
            }
            None => Ok(false),
        },
        ContentRef::Quest(vnum) => match db.get_quest_data(vnum)? {
            Some(mut e) => {
                f(&mut e);
                db.save_quest_data(&e)?;
                Ok(true)
            }
            None => Ok(false),
        },
    }
}

/// A builder just created this: claim it, and mark it `Builder` origin.
///
/// Content that already names an author is left alone — see
/// [`Authored::stamp_created`]. The `Ok(true)` here means "the entity existed
/// and was written", not "the author changed".
pub fn stamp_created(db: &Db, target: &ContentRef, builder: &str) -> Result<bool> {
    if builder.trim().is_empty() {
        return Ok(false);
    }
    let builder = builder.to_string();
    mutate(db, target, move |e| {
        e.stamp_created(&builder);
    })
}

/// A builder just changed this: record the edit, and nothing else.
///
/// Deliberately does not claim unattributed content on first edit. A builder
/// who rewrites a seed area from scratch gets no credit for it, which is the
/// conservative side of a call that cannot be made correctly from a database
/// that predates attribution — an explicit claim surface is the honest fix,
/// not a heuristic here.
pub fn stamp_edited(db: &Db, target: &ContentRef, builder: &str) -> Result<bool> {
    if builder.trim().is_empty() {
        return Ok(false);
    }
    let builder = builder.to_string();
    mutate(db, target, move |e| e.stamp_edited(&builder))
}

pub fn read(db: &Db, target: &ContentRef) -> Result<Option<Provenance>> {
    Ok(match target {
        ContentRef::Room(id) => db.get_room_data(id)?.map(|e| e.provenance()),
        ContentRef::Item(id) => db.get_item_data(id)?.map(|e| e.provenance()),
        ContentRef::Mobile(id) => db.get_mobile_data(id)?.map(|e| e.provenance()),
        ContentRef::Area(id) => db.get_area_data(id)?.map(|e| e.provenance()),
        ContentRef::Quest(vnum) => db.get_quest_data(vnum)?.map(|e| e.provenance()),
    })
}

/// How many rows a bulk origin pass touched, per kind.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StampCounts {
    pub rooms: usize,
    pub items: usize,
    pub mobiles: usize,
    pub areas: usize,
    pub quests: usize,
}

impl StampCounts {
    pub fn total(&self) -> usize {
        self.rooms + self.items + self.mobiles + self.areas + self.quests
    }
}

/// Label every currently-unattributed entity in the world with `origin`.
///
/// Used by the seed pass (which runs once, on an empty database) and available
/// to operators who need to declare an existing world's provenance after the
/// fact. It **never** overwrites content that already has an origin or an
/// author, so running it twice, or running it over a world that already
/// contains hand-built rooms, cannot take credit away from anyone.
///
/// This is why the seed pass is a sweep rather than a stamp inside each
/// `seed_*` function: one call site that cannot be forgotten when a sixth
/// seed module is added, and one that is safe to re-run.
/// What a claim took, and what it deliberately left alone.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub claimed: StampCounts,
    /// Already names an author. Never reassigned, not even to the area owner.
    pub already_credited: usize,
    pub skipped_seed: usize,
    pub skipped_import: usize,
}

/// What a claim should do with one entity.
enum Verdict {
    Claim,
    AlreadyCredited,
    Seed,
    Import,
}

/// The whole anti-cheat rule, in one place so five loops cannot drift.
///
/// `stamp_created` sets `origin = Builder`, and `Builder` is the only origin
/// that [`ContentOrigin::counts_for_score`] accepts — so claiming *is* the act
/// that converts content into score. That makes the origin filter the load-
/// bearing part of this function rather than a nicety: without it, claiming an
/// imported area turns 679 machine-translated items into 679 credited builds,
/// which is the exact failure `crate::types::provenance` was written to
/// prevent. Seed and import content is reported back to the claimant and never
/// stamped.
///
/// `Builder` origin with no author should not exist, but if it does it is
/// unattributed builder work and claimable — the bar is machine provenance,
/// not the absence of a stamp.
fn verdict(e: &dyn Authored) -> Verdict {
    if e.authored_by().is_some() {
        return Verdict::AlreadyCredited;
    }
    match e.origin() {
        ContentOrigin::Seed => Verdict::Seed,
        ContentOrigin::Import => Verdict::Import,
        ContentOrigin::Unknown | ContentOrigin::Builder => Verdict::Claim,
    }
}

/// Claim every unattributed row in one area for `builder`.
///
/// The explicit claim surface. `AreaData.owner` is an ACL and `authored_by` is
/// a credit, and nothing bridged them — so a builder who owned an area built
/// before attribution existed could never be credited for it, because
/// `stamp_created` refuses to overwrite and `stamp_edited` never reassigns.
/// Guessing the bridge at read time was the alternative and it is worse: it
/// makes credit a derivative of an ACL, so handing someone the keys would hand
/// them the authorship too.
///
/// **Authorisation is the caller's job** and must not be skipped — see the
/// `claim_area_content` binding in `crate::script::build`. This function will
/// claim an area for anyone.
///
/// Idempotent: a second run finds every row authored and claims nothing.
pub fn claim_area(db: &Db, area_id: Uuid, builder: &str) -> Result<ClaimOutcome> {
    let mut out = ClaimOutcome::default();
    if builder.trim().is_empty() {
        return Ok(out);
    }

    macro_rules! sweep {
        ($rows:expr, $save:expr, $counter:ident) => {
            for mut e in $rows {
                match verdict(&e) {
                    Verdict::Claim => {
                        e.stamp_created(builder);
                        #[allow(clippy::redundant_closure_call)]
                        ($save)(&mut e)?;
                        out.claimed.$counter += 1;
                    }
                    Verdict::AlreadyCredited => out.already_credited += 1,
                    Verdict::Seed => out.skipped_seed += 1,
                    Verdict::Import => out.skipped_import += 1,
                }
            }
        };
    }

    let in_area = |owner: Option<Uuid>| owner == Some(area_id);

    sweep!(
        db.list_all_areas()?.into_iter().filter(|a| a.id == area_id),
        |e: &mut crate::types::AreaData| db.save_area_data(e.clone()),
        areas
    );
    sweep!(
        db.list_all_rooms()?.into_iter().filter(|r| in_area(r.area_id)),
        |e: &mut crate::types::RoomData| db.save_room_data(e.clone()),
        rooms
    );
    // Prototypes only. An instance is spawned content, not authored content —
    // the same filter `WorldSnapshot::load` applies, and claiming instances
    // would credit a builder once per spawn.
    sweep!(
        db.list_all_items()?
            .into_iter()
            .filter(|i| i.is_prototype && in_area(i.area_id)),
        |e: &mut crate::types::ItemData| db.save_item_data(e.clone()),
        items
    );
    let mobiles: Vec<crate::types::MobileData> = db
        .list_all_mobiles()?
        .into_iter()
        .filter(|m| m.is_prototype && in_area(m.area_id))
        .collect();
    // A quest has no `area_id`; it belongs to whichever area its giver lives
    // in. Same association `get_area_credits` makes, taken before the sweep
    // consumes the list.
    let givers: std::collections::HashSet<String> = mobiles.iter().map(|m| m.vnum.clone()).collect();
    sweep!(
        mobiles.into_iter(),
        |e: &mut crate::types::MobileData| db.save_mobile_data(e.clone()),
        mobiles
    );
    sweep!(
        db.list_all_quests()?
            .into_iter()
            .filter(|q| q.giver_mob_vnum.as_deref().is_some_and(|v| givers.contains(v))),
        |e: &mut crate::types::QuestData| db.save_quest_data(e),
        quests
    );

    Ok(out)
}

pub fn stamp_unattributed(db: &Db, origin: ContentOrigin) -> Result<StampCounts> {
    let mut counts = StampCounts::default();

    for mut e in db.list_all_rooms()? {
        if e.stamp_origin_if_unattributed(origin) {
            db.save_room_data(e)?;
            counts.rooms += 1;
        }
    }
    for mut e in db.list_all_items()? {
        if e.stamp_origin_if_unattributed(origin) {
            db.save_item_data(e)?;
            counts.items += 1;
        }
    }
    for mut e in db.list_all_mobiles()? {
        if e.stamp_origin_if_unattributed(origin) {
            db.save_mobile_data(e)?;
            counts.mobiles += 1;
        }
    }
    for mut e in db.list_all_areas()? {
        if e.stamp_origin_if_unattributed(origin) {
            db.save_area_data(e)?;
            counts.areas += 1;
        }
    }
    for mut e in db.list_all_quests()? {
        if e.stamp_origin_if_unattributed(origin) {
            db.save_quest_data(&e)?;
            counts.quests += 1;
        }
    }

    Ok(counts)
}
