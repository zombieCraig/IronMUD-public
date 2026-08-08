//! Where a piece of content came from, and who is responsible for it.
//!
//! Three fields ride on every authored entity — `RoomData`, `ItemData`,
//! `MobileData`, `AreaData`, `QuestData` — and they exist to answer two
//! questions the codebase previously could not:
//!
//! * **Who built this?** `AreaData.owner` and `trusted_builders` are an ACL,
//!   not a credit. Nothing in the engine displayed who authored anything.
//! * **Does it count?** One CircleMUD import is 679 items and 1,286 spawn
//!   points. Without provenance, `ironmud-import` is a cheat code for any
//!   system that rewards building.
//!
//! # Why the default is `Unknown` rather than `Builder`
//!
//! Every row written before this existed deserialises with the default, and
//! the honest label for those rows is that we do not know. Defaulting to
//! `Builder` would silently declare the entire pre-existing world to be
//! hand-authored content, which is the exact failure the field is here to
//! prevent — a fail-safe default costs a builder a credit they can see and
//! fix, a fail-open one hands out credit nobody earned.
//!
//! Nothing is backfilled for the same reason: an existing database cannot
//! distinguish seed content from builder content after the fact, and guessing
//! is worse than admitting.
//!
//! # The stamping rules
//!
//! * **Creating** content sets `authored_by`, `last_edited_by` and
//!   `origin = Builder`.
//! * **Editing** content sets `last_edited_by` only. Editing someone else's
//!   room never reassigns it, and editing a seed room never converts it into
//!   your work.
//!
//! Those two rules are why the entry points are separate functions rather
//! than one function with a flag.

use serde::{Deserialize, Serialize};

/// The five kinds of authored entity.
///
/// One enum, used by both attribution (who made it) and the auditor (is it any
/// good) — `crate::audit::EntityKind` is a re-export of this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Room,
    Item,
    Mobile,
    Quest,
    Area,
}

impl ContentKind {
    pub fn key(self) -> &'static str {
        match self {
            ContentKind::Room => "room",
            ContentKind::Item => "item",
            ContentKind::Mobile => "mobile",
            ContentKind::Quest => "quest",
            ContentKind::Area => "area",
        }
    }

    pub fn from_key(s: &str) -> Option<ContentKind> {
        match s.trim().to_lowercase().as_str() {
            "room" => Some(ContentKind::Room),
            "item" | "object" | "obj" => Some(ContentKind::Item),
            "mob" | "mobile" | "npc" => Some(ContentKind::Mobile),
            "quest" => Some(ContentKind::Quest),
            "area" | "zone" => Some(ContentKind::Area),
            _ => None,
        }
    }

    pub const ALL: &'static [ContentKind] = &[
        ContentKind::Room,
        ContentKind::Item,
        ContentKind::Mobile,
        ContentKind::Quest,
        ContentKind::Area,
    ];
}

/// Where content came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentOrigin {
    /// Written before provenance existed, or by a path that does not stamp.
    /// Counts toward nothing.
    #[default]
    Unknown,
    /// Shipped by `src/seed/` as part of the demo world.
    Seed,
    /// Produced by `ironmud-import` from a foreign MUD's data files.
    Import,
    /// Authored in-game through OLC, or through the REST/MCP builder API.
    /// **The only variant that counts toward a builder's score.**
    Builder,
}

impl ContentOrigin {
    pub fn key(self) -> &'static str {
        match self {
            ContentOrigin::Unknown => "unknown",
            ContentOrigin::Seed => "seed",
            ContentOrigin::Import => "import",
            ContentOrigin::Builder => "builder",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ContentOrigin::Unknown => "unattributed",
            ContentOrigin::Seed => "shipped with the engine",
            ContentOrigin::Import => "imported",
            ContentOrigin::Builder => "built here",
        }
    }

    pub fn from_key(s: &str) -> Option<ContentOrigin> {
        match s.trim().to_lowercase().as_str() {
            "unknown" | "" => Some(ContentOrigin::Unknown),
            "seed" => Some(ContentOrigin::Seed),
            "import" | "imported" => Some(ContentOrigin::Import),
            "builder" | "built" => Some(ContentOrigin::Builder),
            _ => None,
        }
    }

    /// Whether this content can be credited to a builder.
    ///
    /// The single question every scoring and rollup path asks. Keeping it a
    /// method rather than an `== Builder` comparison scattered across modules
    /// means a future variant cannot be missed at half the call sites.
    pub fn counts_for_score(self) -> bool {
        matches!(self, ContentOrigin::Builder)
    }
}

/// The three fields, as one unit, for the code that moves them around.
///
/// Deliberately *not* `#[serde(flatten)]`-ed onto the entity structs: flatten
/// forces the parent through serde's buffering path on every deserialise, and
/// `ItemData` is loaded thousands of times per tick. The entities carry three
/// plain fields; this struct is the in-memory carrier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    pub authored_by: Option<String>,
    pub last_edited_by: Option<String>,
    pub origin: ContentOrigin,
}

impl Provenance {
    /// A builder just created this.
    pub fn created_by(name: &str) -> Provenance {
        Provenance {
            authored_by: Some(name.to_string()),
            last_edited_by: Some(name.to_string()),
            origin: ContentOrigin::Builder,
        }
    }

    pub fn from_origin(origin: ContentOrigin) -> Provenance {
        Provenance {
            authored_by: None,
            last_edited_by: None,
            origin,
        }
    }
}

/// Read and write the three fields on anything that carries them.
///
/// Five entity types need identical stamping logic. Without this trait the
/// stamp helpers would be five near-identical functions, which is how the
/// rules start to differ between them.
pub trait Authored {
    fn authored_by(&self) -> Option<&str>;
    fn last_edited_by(&self) -> Option<&str>;
    fn origin(&self) -> ContentOrigin;
    fn set_provenance(&mut self, p: Provenance);

    fn provenance(&self) -> Provenance {
        Provenance {
            authored_by: self.authored_by().map(str::to_string),
            last_edited_by: self.last_edited_by().map(str::to_string),
            origin: self.origin(),
        }
    }

    /// Stamp a fresh creation, if nobody has claimed it yet.
    ///
    /// A newly-constructed entity has nothing worth keeping, and every call
    /// site today really is a creation — the uuid-keyed creates mint a fresh id
    /// and both quest-create paths reject a duplicate vnum first. But this is
    /// the one function that *can* take another builder's credit, and that
    /// safety currently lives entirely in the callers. A future create-or-
    /// replace endpoint would become credit theft with no diff to this file, so
    /// the check belongs here rather than in a comment asking people to be
    /// careful. Returns whether the stamp was applied.
    fn stamp_created(&mut self, builder: &str) -> bool {
        if self.authored_by().is_some() {
            return false;
        }
        self.set_provenance(Provenance::created_by(builder));
        true
    }

    /// Record an edit.
    ///
    /// Touches `last_edited_by` and nothing else. Authorship and origin are
    /// deliberately immutable here: rewriting a seed room does not make it
    /// yours, and editing a colleague's room does not take it from them.
    fn stamp_edited(&mut self, builder: &str) {
        let mut p = self.provenance();
        p.last_edited_by = Some(builder.to_string());
        self.set_provenance(p);
    }

    /// Label an entity as machine-produced, but only if nothing has claimed
    /// it. Seed and import passes call this in bulk, and a bulk pass must
    /// never trample a builder's credit if it is ever run twice or run over a
    /// world that already has hand-built content in it.
    fn stamp_origin_if_unattributed(&mut self, origin: ContentOrigin) -> bool {
        if self.origin() != ContentOrigin::Unknown || self.authored_by().is_some() {
            return false;
        }
        self.set_provenance(Provenance {
            authored_by: None,
            last_edited_by: None,
            origin,
        });
        true
    }
}

/// Implement [`Authored`] for a type carrying the three standard fields.
#[macro_export]
macro_rules! impl_authored {
    ($t:ty) => {
        impl $crate::types::Authored for $t {
            fn authored_by(&self) -> Option<&str> {
                self.authored_by.as_deref()
            }
            fn last_edited_by(&self) -> Option<&str> {
                self.last_edited_by.as_deref()
            }
            fn origin(&self) -> $crate::types::ContentOrigin {
                self.origin
            }
            fn set_provenance(&mut self, p: $crate::types::Provenance) {
                self.authored_by = p.authored_by;
                self.last_edited_by = p.last_edited_by;
                self.origin = p.origin;
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Thing {
        authored_by: Option<String>,
        last_edited_by: Option<String>,
        origin: ContentOrigin,
    }

    impl Authored for Thing {
        fn authored_by(&self) -> Option<&str> {
            self.authored_by.as_deref()
        }
        fn last_edited_by(&self) -> Option<&str> {
            self.last_edited_by.as_deref()
        }
        fn origin(&self) -> ContentOrigin {
            self.origin
        }
        fn set_provenance(&mut self, p: Provenance) {
            self.authored_by = p.authored_by;
            self.last_edited_by = p.last_edited_by;
            self.origin = p.origin;
        }
    }

    #[test]
    fn the_default_is_unknown_and_counts_for_nothing() {
        assert_eq!(ContentOrigin::default(), ContentOrigin::Unknown);
        assert!(!ContentOrigin::Unknown.counts_for_score());
        assert!(!ContentOrigin::Seed.counts_for_score());
        assert!(!ContentOrigin::Import.counts_for_score());
        assert!(ContentOrigin::Builder.counts_for_score());
    }

    #[test]
    fn creating_claims_it_and_editing_does_not() {
        let mut t = Thing::default();
        t.stamp_created("Ana");
        assert_eq!(t.authored_by(), Some("Ana"));
        assert_eq!(t.origin(), ContentOrigin::Builder);

        t.stamp_edited("Bo");
        assert_eq!(t.authored_by(), Some("Ana"), "an edit must not reassign authorship");
        assert_eq!(t.last_edited_by(), Some("Bo"));
        assert_eq!(t.origin(), ContentOrigin::Builder);
    }

    #[test]
    fn editing_seed_content_does_not_convert_it() {
        let mut t = Thing {
            origin: ContentOrigin::Seed,
            ..Default::default()
        };
        t.stamp_edited("Ana");
        assert_eq!(t.origin(), ContentOrigin::Seed);
        assert_eq!(t.authored_by(), None);
        assert!(!t.origin().counts_for_score());
    }

    #[test]
    fn a_bulk_origin_pass_never_trample_a_builders_credit() {
        let mut mine = Thing::default();
        mine.stamp_created("Ana");
        assert!(!mine.stamp_origin_if_unattributed(ContentOrigin::Import));
        assert_eq!(mine.origin(), ContentOrigin::Builder);
        assert_eq!(mine.authored_by(), Some("Ana"));

        let mut legacy = Thing::default();
        assert!(legacy.stamp_origin_if_unattributed(ContentOrigin::Seed));
        assert_eq!(legacy.origin(), ContentOrigin::Seed);
        // And it is idempotent — running the pass twice changes nothing.
        assert!(!legacy.stamp_origin_if_unattributed(ContentOrigin::Import));
        assert_eq!(legacy.origin(), ContentOrigin::Seed);
    }

    #[test]
    fn origin_keys_round_trip() {
        for o in [
            ContentOrigin::Unknown,
            ContentOrigin::Seed,
            ContentOrigin::Import,
            ContentOrigin::Builder,
        ] {
            assert_eq!(ContentOrigin::from_key(o.key()), Some(o));
        }
        assert_eq!(ContentOrigin::from_key("nonsense"), None);
    }
}
