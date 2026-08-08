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
