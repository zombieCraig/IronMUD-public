//! Engine-agnostic importer framework for legacy MUD content.
//!
//! Each supported source engine (CircleMUD, Diku, ROM, ...) implements
//! [`MudEngine`], producing an engine-neutral [`ImportIR`] tree. The mapping
//! layer in [`mapping`] converts that into a [`Plan`] of IronMUD writes plus
//! a list of [`Warning`]s for anything the source engine expressed but
//! IronMUD does not (yet) model. The writer in [`writer`] either pretty-prints
//! the plan (dry-run) or commits it to a Sled database.
//!
//! See `docs/import-guide.md` for end-user docs and the contract for adding
//! new engines.

use std::path::{Path, PathBuf};

pub mod engines;
pub mod mapping;
pub mod writer;

/// Identifies a position inside a source MUD's data files. Surfaced through
/// warnings so a builder reading the dry-run report can jump to the exact
/// `.wld`/`.zon`/whatever line that triggered an issue.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SourceLoc {
    pub file: PathBuf,
    pub line: Option<u32>,
    pub zone_vnum: Option<i32>,
    pub room_vnum: Option<i32>,
}

impl SourceLoc {
    pub fn file(file: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            ..Default::default()
        }
    }

    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_zone(mut self, zone: i32) -> Self {
        self.zone_vnum = Some(zone);
        self
    }

    pub fn with_room(mut self, room: i32) -> Self {
        self.room_vnum = Some(room);
        self
    }
}

/// Engine-neutral world data produced by an engine parser.
#[derive(Debug, Default, Clone)]
pub struct ImportIR {
    pub zones: Vec<IrZone>,
    /// Trigger / specproc bindings that cross zone boundaries. CircleMUD's
    /// `src/spec_assign.c` is one tree-wide file binding mob/obj/room vnums
    /// to C function names; bucketing per zone would be artificial. The
    /// mapping layer resolves each source vnum against the global
    /// mob/item/room indexes built during the per-zone passes.
    pub triggers: Vec<IrTrigger>,
}

#[derive(Debug, Clone)]
pub struct IrZone {
    /// Source-engine vnum (e.g. CircleMUD zone number). Stays as a signed
    /// int to mirror the legacy formats; we re-stringify for IronMUD vnums.
    pub vnum: i32,
    pub name: String,
    pub description: Option<String>,
    pub vnum_range: Option<(i32, i32)>,
    /// Per-zone default respawn cadence for any [`PlannedSpawn`]s derived
    /// from this zone's reset commands. CircleMUD encodes this as the
    /// `lifespan` field in `.zon` headers (in minutes); the engine converts
    /// to seconds. `None` falls back to a 5-minute default during mapping.
    pub default_respawn_secs: Option<i64>,
    pub source: SourceLoc,
    pub rooms: Vec<IrRoom>,
    pub mobiles: Vec<IrMob>,
    pub items: Vec<IrItem>,
    pub shops: Vec<IrShop>,
    /// Structured zone reset commands (CircleMUD M/O/G/E/P/D/R). Translated
    /// into [`PlannedSpawn`]s + door overrides during mapping. Anything the
    /// mapper can't translate becomes a warning at that stage.
    pub resets: Vec<IrReset>,
    /// Engine features the parser surfaced but couldn't fit into a structured
    /// IR slot (e.g. spec_proc names). Always become warnings during mapping.
    /// Reset commands historically lived here; they now live on `resets` and
    /// only fall back to `deferred` when even the parser can't categorise them.
    pub deferred: Vec<DeferredItem>,
}

#[derive(Debug, Clone)]
pub struct IrRoom {
    pub vnum: i32,
    pub name: String,
    pub description: String,
    /// Engine-specific sector identifier (CircleMUD: 0..=9). Mapped via the
    /// mapping JSON to one or more IronMUD `RoomFlags`.
    pub sector: i32,
    /// Engine-specific room flag bits. Each bit's meaning is engine-defined;
    /// the mapping JSON knows the bit-name table for each engine.
    pub flag_bits: u64,
    /// Names of room flags the engine recognised but the *mapping table* has
    /// no entry for. Kept separate from `flag_bits` so we can surface them as
    /// "unknown flag X" warnings instead of silently dropping them.
    pub unknown_flag_names: Vec<String>,
    pub exits: Vec<IrExit>,
    pub extras: Vec<IrExtraDesc>,
    pub source: SourceLoc,
}

#[derive(Debug, Clone)]
pub struct IrExit {
    /// Canonical direction name: north, east, south, west, up, down.
    pub direction: String,
    pub general_description: Option<String>,
    pub keyword: Option<String>,
    pub door_flags: u32,
    /// Engine-specific door-flag bits we did not recognise (named) — surfaces
    /// as a warning so we don't silently lose patched flag bits.
    pub unknown_door_flags: Vec<String>,
    pub key_vnum: Option<i32>,
    pub to_room_vnum: i32,
}

#[derive(Debug, Clone)]
pub struct IrExtraDesc {
    pub keywords: Vec<String>,
    pub description: String,
}

/// CircleMUD mobile prototype (`.mob` file entry). Engine-neutral in that we
/// keep the source-side numeric stats (THAC0, alignment, position, sex) even
/// when IronMUD has no equivalent — the mapping layer decides what to drop
/// vs. warn vs. translate.
#[derive(Debug, Clone)]
pub struct IrMob {
    pub vnum: i32,
    /// Whitespace-split keyword aliases (CircleMUD `namelist`).
    pub keywords: Vec<String>,
    /// Short noun phrase used in attack/action messages, e.g. "the wizard".
    pub short_descr: String,
    /// Sentence shown when the mob is in a room, e.g. "A wizard walks
    /// around behind the counter."
    pub long_descr: String,
    /// Multi-line look/examine text. May be empty.
    pub description: String,
    /// CircleMUD MOB_* action bitvector.
    pub mob_flag_bits: u64,
    /// CircleMUD AFF_* affected-by bitvector.
    pub aff_flag_bits: u64,
    pub alignment: i32,
    pub level: i32,
    pub thac0: i32,
    pub ac: i32,
    /// Raw HP dice expression, e.g. "5d10+550".
    pub hp_dice: String,
    /// Raw damage dice expression, e.g. "2d8+18".
    pub damage_dice: String,
    pub gold: i32,
    pub exp: i32,
    pub position: i32,
    pub default_position: i32,
    pub sex: i32,
    /// 'S' (simple) or 'E' (enhanced — has a named-attribute block).
    pub format: char,
    /// E-block named attributes (e.g. `BareHandAttack: 12`). Captured
    /// verbatim; surfaced as warn-once-per-distinct-name during mapping.
    pub extra_attrs: Vec<(String, String)>,
    pub source: SourceLoc,
}

/// CircleMUD object prototype (`.obj` file entry). Engine-neutral in that we
/// keep the source-side numeric values verbatim — the mapping layer decides
/// what each value means based on `item_type`.
#[derive(Debug, Clone)]
pub struct IrItem {
    pub vnum: i32,
    /// Whitespace-split keyword aliases (CircleMUD `namelist`).
    pub keywords: Vec<String>,
    /// Ground appearance, e.g. "a long sword".
    pub short_descr: String,
    /// In-room sentence, e.g. "A long sword has been left here.".
    pub long_descr: String,
    /// Stock CircleMUD's "action description" — printed when the item is
    /// used (drink/eat/wear). Often empty. No IronMUD analogue.
    pub action_descr: String,
    /// CircleMUD ITEM_TYPE constant (1..=23). 0/unrecognised values become
    /// Misc with a warning.
    pub item_type: i32,
    /// CircleMUD ITEM_* (extra) bitvector — wear-independent flags like GLOW,
    /// HUM, NODROP, BLESS, ANTI_GOOD …
    pub extra_flag_bits: u64,
    /// CircleMUD ITEM_WEAR_* bitvector — TAKE + which body slot(s) accept
    /// the item.
    pub wear_flag_bits: u64,
    /// Names of extra-bits the engine recognised but the *mapping table* has
    /// no entry for. Surfaced as "no mapping for ITEM_X" warnings.
    pub unknown_extra_flags: Vec<String>,
    /// Names of wear-bits beyond the canonical 15 (likely patched). Surfaced
    /// as warnings — the standard set has no JSON mapping (it's coded).
    pub unknown_wear_flags: Vec<String>,
    /// The four type-specific values (`v0..v3`). Their meaning depends on
    /// `item_type` per `building.tex` — the mapping layer interprets each.
    pub values: [i32; 4],
    pub weight: i32,
    pub cost: i32,
    pub rent: i32,
    /// `E`-blocks attached to the object (keyword/lore descriptions). No
    /// `ItemData` target today; the mapping layer warns once per item.
    pub extra_descs: Vec<IrExtraDesc>,
    /// `A`-blocks: pairs of (`apply_location`, modifier).
    pub affects: Vec<(i32, i32)>,
    pub source: SourceLoc,
}

/// CircleMUD shop record (`.shp` file entry). Engine-neutral in that we
/// keep the source-side numeric values verbatim — the mapping layer
/// resolves the keeper vnum and producing list to PlannedMobile/Item
/// vnums and decides what to translate vs. warn vs. drop.
#[derive(Debug, Clone)]
pub struct IrShop {
    pub vnum: i32,
    /// CircleMUD vnum of the mob who runs this shop. Resolved against the
    /// global mob vnum index during mapping.
    pub keeper_vnum: i32,
    /// Item vnums the shop creates from thin air ("producing" list).
    pub producing: Vec<i32>,
    /// Markup multiplier the shop charges when selling to a player
    /// (Circle's `profit_buy`, e.g. `2.1` = 210%).
    pub profit_buy: f32,
    /// Multiplier the shop pays when buying from a player (Circle's
    /// `profit_sell`, e.g. `0.5` = 50%).
    pub profit_sell: f32,
    /// CircleMUD item-type tokens the shop will buy back, e.g.
    /// `["FOOD", "LIQ CONTAINER"]`. Already canonicalised (uppercase, with
    /// embedded spaces preserved). Empty when the source list was just `-1`.
    pub buy_types: Vec<String>,
    /// Tokens that didn't parse as a known CircleMUD ITEM_TYPE — surfaced
    /// as Warn entries during mapping.
    pub unknown_buy_types: Vec<String>,
    /// 7 shop message strings (no_such_item1/2, do_not_buy,
    /// missing_cash1/2, message_buy, message_sell). Captured for the
    /// dry-run report; not imported (no IronMUD analogue).
    pub messages: [String; 7],
    /// Keeper temper when broke (0..=2). No IronMUD analogue.
    pub temper: i32,
    /// Shop bitvector (WILL_START_FIGHT=1, WILL_BANK_MONEY=2). No analogue.
    pub bitvector: u32,
    /// `with_who` bitvector — TRADE_NO* alignment/class restrictions.
    pub with_who: u32,
    /// Room vnums the shop operates in. IronMUD shops travel with the
    /// keeper, so multi-room lists surface as advisory warnings.
    pub rooms: Vec<i32>,
    pub open1: i32,
    pub close1: i32,
    pub open2: i32,
    pub close2: i32,
    pub source: SourceLoc,
}

/// A single CircleMUD `.zon` reset command. The mapping layer walks these
/// in source order and emits [`PlannedSpawn`]s (for M/O), spawn dependencies
/// (for chained G/E/P), or [`PlannedDoor`] mutations (for D). Commands that
/// can't be translated (R, cross-block P, etc.) become warnings.
#[derive(Debug, Clone)]
pub struct IrReset {
    /// CircleMUD `if_flag`: true (1) means "only run if the previous M/O
    /// succeeded"; false (0) means unconditional. Translation collapses
    /// runtime success-checking into "was the parent translated at all" —
    /// the spawn tick re-evaluates per-fire.
    pub if_flag: bool,
    pub kind: IrResetKind,
    pub source: SourceLoc,
}

#[derive(Debug, Clone)]
pub enum IrResetKind {
    /// `M if mob_vnum max room_vnum` — load mobile into room.
    LoadMob { vnum: i32, max: i32, room_vnum: i32 },
    /// `O if obj_vnum max room_vnum` — load object into room.
    LoadObj { vnum: i32, max: i32, room_vnum: i32 },
    /// `G if obj_vnum max` — give object to last-loaded mob's inventory.
    GiveObj { vnum: i32, max: i32 },
    /// `E if obj_vnum max wear_loc` — equip last-loaded mob (wear_loc 0..17).
    EquipObj { vnum: i32, max: i32, wear_loc: i32 },
    /// `P if obj_vnum max container_vnum` — put object in a container.
    PutObj { vnum: i32, max: i32, container_vnum: i32 },
    /// `D if room_vnum dir state` — set door state (0=open, 1=closed,
    /// 2=closed+locked).
    SetDoor { room_vnum: i32, dir: i32, state: i32 },
    /// `R if room_vnum obj_vnum` — remove object from room (cleanup).
    RemoveObj { room_vnum: i32, vnum: i32 },
}

/// CircleMUD specproc binding: a single `ASSIGN(MOB|OBJ|ROOM)(vnum, fname)`
/// line in `spec_assign.c` (or a `castle_mob_spec(vnum, fname)` line in
/// `castle.c`). The mapping layer translates each entry via
/// `circle_trigger_mapping.json` into either a `MobileFlags` bit or a
/// `*Trigger` push, or surfaces it as an "unsupported" warning.
#[derive(Debug, Clone)]
pub struct IrTrigger {
    pub source_vnum: i32,
    pub attach_type: AttachType,
    /// Lowercase specproc identifier (`cityguard`, `puff`, `bank`, ...).
    pub specproc_name: String,
    /// Optional template arguments captured by the parser (currently only
    /// puff's `do_say` quotes; left empty for all other specprocs).
    pub args: Vec<String>,
    pub source: SourceLoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttachType {
    Mob,
    Obj,
    Room,
}

/// A planned overlay onto a [`PlannedMobile`]. Shops cross-cut zones
/// (a `.shp` file lives next to the zone declaring the shop, but its
/// keeper mob may live in any imported zone), so the writer applies these
/// in a separate pass after mobile prototypes are saved.
#[derive(Debug, Clone)]
pub struct PlannedShopOverlay {
    /// Source-side shop vnum (informational; not used as a key).
    pub shop_source_vnum: i32,
    /// Source-side keeper-mob vnum. Resolves to a `PlannedMobile` via
    /// the global `(prefix, source_vnum) -> vnum` index built during
    /// mapping.
    pub keeper_source_vnum: i32,
    /// Already-prefixed mobile vnum of the keeper. The writer uses this
    /// to find the saved `MobileData`.
    pub keeper_vnum: String,
    /// Producing items, already prefix-rewritten. Items missing from the
    /// global item index were dropped (with a Warn); this list contains
    /// only resolvable references.
    pub stock_vnums: Vec<String>,
    /// IronMUD `shop_buy_rate` (% paid to player when buying back).
    pub buy_rate: i32,
    /// IronMUD `shop_sell_rate` (% charged to player when selling).
    pub sell_rate: i32,
    /// IronMUD `shop_buys_types` — already mapped to IronMUD `ItemType`
    /// display strings, deduped.
    pub buys_types: Vec<String>,
    pub source: SourceLoc,
}

/// A planned IronMUD spawn point derived from one CircleMUD `M`/`O` reset
/// command. Chained `G`/`E`/`P` resets attach as `dependencies`. The writer
/// resolves `room_vnum` to a UUID via the room vnum map and saves a
/// `SpawnPointData`.
#[derive(Debug, Clone)]
pub struct PlannedSpawn {
    pub area_prefix: String,
    /// Already-prefixed mobile or item vnum (the entity that gets spawned).
    pub vnum: String,
    /// Whether `vnum` refers to a mobile or item prototype.
    pub entity_type: crate::types::SpawnEntityType,
    /// Already-prefixed room vnum (the spawn anchor).
    pub room_vnum: String,
    pub max_count: i32,
    pub respawn_interval_secs: i64,
    pub dependencies: Vec<PlannedSpawnDep>,
    pub source: SourceLoc,
}

/// A child entity spawned alongside a [`PlannedSpawn`]'s parent: an item
/// equipped/inventoried on a spawned mob (G/E) or placed in a spawned
/// container (P).
#[derive(Debug, Clone)]
pub struct PlannedSpawnDep {
    /// Already-prefixed item vnum.
    pub item_vnum: String,
    pub destination: crate::types::SpawnDestination,
    pub count: i32,
}

/// A planned mutation derived from a CircleMUD specproc binding. Either
/// sets a flag bit on a mob, or pushes a trigger struct onto a mob/item/room.
/// The writer applies these in a separate pass after mobiles, items, rooms
/// and shop overlays have landed.
#[derive(Debug, Clone)]
pub struct PlannedTriggerOverlay {
    pub attach_type: AttachType,
    /// Already-prefixed vnum of the target mob/item/room.
    pub target_vnum: String,
    /// Source-side specproc name, retained for diagnostics.
    pub specproc_name: String,
    pub mutation: TriggerMutation,
    pub source: SourceLoc,
}

#[derive(Debug, Clone)]
pub enum TriggerMutation {
    /// Set a single bool field on `MobileFlags` by snake_case name.
    SetMobFlag { ironmud_flag: String },
    /// Append a `MobileTrigger` to the mob's `triggers` Vec.
    AddMobTrigger(crate::types::MobileTrigger),
    /// Append an `ItemTrigger` to the item's `triggers` Vec.
    AddItemTrigger(crate::types::ItemTrigger),
    /// Append a `RoomTrigger` to the room's `triggers` Vec.
    AddRoomTrigger(crate::types::RoomTrigger),
}

/// Catch-all for engine features we parsed but won't try to translate in
/// this phase. Each becomes a [`Warning`] during mapping so builders can
/// audit what was dropped.
#[derive(Debug, Clone)]
pub struct DeferredItem {
    pub category: String, // "zone_reset", "spec_proc", ...
    pub summary: String,
    pub source: SourceLoc,
}

/// Trait every source engine implements. Adding a new engine is a matter of
/// implementing this trait and registering an instance in
/// `src/bin/ironmud-import.rs`.
pub trait MudEngine {
    /// Short identifier used in CLI subcommands and warning text.
    fn name(&self) -> &'static str;

    /// Parse a source tree rooted at `source`. Returns the IR plus any
    /// parser-level warnings (e.g. "skipped malformed file"). Hard parse
    /// failures should be returned as `Err`; recoverable issues become
    /// warnings.
    fn parse(&self, source: &Path) -> anyhow::Result<(ImportIR, Vec<Warning>)>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Block,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub severity: Severity,
    pub source: SourceLoc,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    Parse,
    UnsupportedFlag,
    UnknownFlag,
    UnsupportedSector,
    UnsupportedDoorFlag,
    PrefixCollision,
    DanglingExit,
    DuplicateVnum,
    DeferredFeature,
    /// Item type bits / value semantics with no IronMUD analogue (e.g.
    /// ITEM_LIGHT capacity hours, SCROLL spell list). Distinct from
    /// `UnsupportedFlag` so reports can group them.
    UnsupportedValueSemantic,
    Info,
}

impl Warning {
    pub fn new(kind: WarningKind, severity: Severity, source: SourceLoc, message: impl Into<String>) -> Self {
        Self {
            kind,
            severity,
            source,
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, s: impl Into<String>) -> Self {
        self.suggestion = Some(s.into());
        self
    }
}

/// A planned set of IronMUD writes derived from an [`ImportIR`].
///
/// The writer applies these in two passes: first all areas + rooms (without
/// exit links), then exits and doors using the vnum→UUID map built during
/// pass one. A pure `Plan` keeps the mapping layer trivially testable.
#[derive(Debug, Default, Clone)]
pub struct Plan {
    pub areas: Vec<PlannedArea>,
    pub rooms: Vec<PlannedRoom>,
    pub exits: Vec<PlannedExit>,
    pub mobiles: Vec<PlannedMobile>,
    pub items: Vec<PlannedItem>,
    /// Shop fields to overlay onto already-saved keeper mobile prototypes.
    /// Applied in a separate writer pass after mobiles land.
    pub shop_overlays: Vec<PlannedShopOverlay>,
    /// Spawn points derived from CircleMUD `.zon` reset commands. Applied
    /// in a writer pass after mobiles, items, and shops land.
    pub spawns: Vec<PlannedSpawn>,
    /// Specproc → flag/trigger overlays derived from `spec_assign.c` and
    /// `castle.c`. Applied in a writer pass after spawn points land so the
    /// overlay can compose with shopkeeper / receptionist mob mutations.
    pub trigger_overlays: Vec<PlannedTriggerOverlay>,
}

#[derive(Debug, Clone)]
pub struct PlannedArea {
    pub source_vnum: i32,
    pub name: String,
    pub prefix: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct PlannedRoom {
    pub area_prefix: String,
    pub source_vnum: i32,
    pub vnum: String,
    pub title: String,
    pub description: String,
    pub flags: crate::types::RoomFlags,
    pub extra_descs: Vec<crate::types::ExtraDesc>,
    pub doors: Vec<PlannedDoor>,
    pub source: SourceLoc,
}

#[derive(Debug, Clone)]
pub struct PlannedDoor {
    pub direction: String,
    pub name: String,
    pub keywords: Vec<String>,
    pub description: Option<String>,
    pub is_closed: bool,
    pub is_locked: bool,
    pub pickproof: bool,
    pub key_source_vnum: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PlannedExit {
    /// Vnum of the room owning this exit (already prefixed).
    pub from_vnum: String,
    pub direction: String,
    /// CircleMUD-side vnum of the destination room (unprefixed). Resolved
    /// to a destination vnum string via the global vnum_index at apply time.
    pub to_source_vnum: i32,
}

/// Output of mapping an [`IrMob`] to an IronMUD prototype. Only fields the
/// importer actually populates are listed; everything else falls back to
/// `MobileData::new` defaults at write time.
#[derive(Debug, Clone)]
pub struct PlannedMobile {
    pub area_prefix: String,
    pub source_vnum: i32,
    /// `<area_prefix>_<source_vnum>` — used as the IronMUD prototype vnum.
    pub vnum: String,
    /// Short noun phrase, populates `MobileData.name`.
    pub name: String,
    /// In-room sentence, populates `MobileData.short_desc`.
    pub short_desc: String,
    /// Look/examine body text, populates `MobileData.long_desc`.
    pub long_desc: String,
    pub keywords: Vec<String>,
    pub level: i32,
    pub max_hp: i32,
    pub damage_dice: String,
    pub armor_class: i32,
    pub gold: i32,
    pub flags: crate::types::MobileFlags,
    pub source: SourceLoc,
}

/// Output of mapping an [`IrItem`] to an IronMUD prototype. The fully-built
/// `ItemData` is stored on the plan so the writer is a thin save loop.
#[derive(Debug, Clone)]
pub struct PlannedItem {
    pub area_prefix: String,
    pub source_vnum: i32,
    /// `<area_prefix>_<source_vnum>` — used as the IronMUD prototype vnum.
    pub vnum: String,
    pub data: crate::types::ItemData,
    pub source: SourceLoc,
}

/// Tunable knobs for the mapping layer. Loaded from a JSON file so non-coders
/// can tweak how legacy flags translate into IronMUD flags without rebuilding.
#[derive(Debug, Clone)]
pub struct MappingOptions {
    pub circle: mapping::CircleMappingTable,
    /// Existing area prefixes (lowercase) that the importer must not collide
    /// with. Populated from the live DB before mapping runs.
    pub existing_area_prefixes: Vec<String>,
    /// Existing room vnums; same idea — collisions become Block warnings.
    pub existing_room_vnums: Vec<String>,
    /// Existing mobile prototype vnums in the target DB (lowercase).
    /// Collisions become Block warnings, mirroring the room behavior so a
    /// double-`--apply` is loud rather than a stealth duplication.
    pub existing_mobile_vnums: Vec<String>,
    /// Existing item prototype vnums in the target DB (lowercase). Same
    /// collision-as-Block treatment as rooms and mobiles.
    pub existing_item_vnums: Vec<String>,
}
