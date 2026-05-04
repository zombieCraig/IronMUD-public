//! Pure mapping layer: engine-neutral [`ImportIR`] → [`Plan`] of IronMUD writes.
//!
//! No I/O. The mapping table is loaded separately and passed in via
//! [`MappingOptions`] so this module is trivially unit-testable with synthetic
//! IR.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use std::collections::HashSet;

use crate::import::{
    AttachType, DeferredItem, ImportIR, IrItem, IrMob, IrResetKind, IrRoom, IrShop, IrTrigger, IrZone,
    MappingOptions, Plan, PlannedArea, PlannedDoor, PlannedExit, PlannedItem, PlannedMobile, PlannedRoom,
    PlannedShopOverlay, PlannedSpawn, PlannedSpawnDep, PlannedTriggerOverlay, Severity, SourceLoc,
    TriggerMutation, Warning, WarningKind,
};
use crate::types::{
    CombatZoneType, DamageType, ExtraDesc, ItemData, ItemFlags, ItemTrigger, ItemTriggerType, ItemType, LiquidType,
    MobileFlags, MobileTrigger, MobileTriggerType, RoomFlags, RoomTrigger, SpawnDestination, SpawnEntityType,
    TriggerType, WearLocation,
};

/// CircleMUD-specific mapping table. Loaded from JSON, but defaults to a
/// hard-coded copy embedded in the binary so the importer works even when
/// the JSON file is missing (e.g. running outside the repo).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CircleMappingTable {
    #[serde(default)]
    pub sector_to_flags: HashMap<String, SectorMapping>,
    #[serde(default)]
    pub room_flag_actions: HashMap<String, FlagAction>,
    #[serde(default)]
    pub mob_flag_actions: HashMap<String, FlagAction>,
    #[serde(default)]
    pub aff_flag_actions: HashMap<String, FlagAction>,
    /// CircleMUD ITEM_* extra-bit name → action. Item-only (the SetStat /
    /// SetArmorClass actions are rejected on rooms and mobs).
    #[serde(default)]
    pub extra_flag_actions: HashMap<String, FlagAction>,
    /// CircleMUD APPLY_* (object affect) location name → action. Drives the
    /// per-affect interpretation in `map_item`.
    #[serde(default)]
    pub apply_actions: HashMap<String, FlagAction>,
    /// CircleMUD shop `buy_types` token (FOOD, LIQ CONTAINER, ...) →
    /// action. `set_flag.ironmud_flag` carries an IronMUD `ItemType`
    /// display string (`food`, `weapon`, ...). Used by `map_shop`.
    #[serde(default)]
    pub buy_type_actions: HashMap<String, FlagAction>,
    /// CircleMUD specproc name (lowercase: `cityguard`, `puff`, `bank`, ...)
    /// → action. Drives `map_triggers`. Specprocs not listed here surface
    /// as a default `warn` so the importer never silently loses a binding.
    #[serde(default)]
    pub trigger_actions: HashMap<String, TriggerAction>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SectorMapping {
    #[serde(default)]
    pub set_flags: Vec<String>,
    #[serde(default)]
    pub info: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FlagAction {
    /// Set a single IronMUD bool flag in `RoomFlags`.
    SetFlag { ironmud_flag: String },
    /// Set the room's combat zone (e.g. PEACEFUL → safe).
    SetCombatZone { value: String },
    /// Set a numeric stat bonus on an `ItemData` (APPLY_STR..APPLY_CHA).
    /// `ironmud_stat` is the snake-case field name on `ItemData`.
    /// Item-only: rejected on rooms / mobs.
    SetStat { ironmud_stat: String },
    /// Set `ItemData.armor_class` (APPLY_ARMOR). The mapping flips the sign
    /// because Circle's AC is *negative-is-better* and IronMUD's
    /// `armor_class` is positive damage reduction. Item-only.
    SetArmorClass {
        #[serde(default)]
        info: Option<String>,
    },
    /// Emit a warning (severity = Warn).
    Warn { message: String },
    /// Silently ignore this flag (e.g. CircleMUD runtime / editor flags).
    Drop {
        #[serde(default)]
        info: Option<String>,
    },
}

/// Action keyed by lowercase CircleMUD specproc name in
/// `circle_trigger_mapping.json`. Each binding from `spec_assign.c` is
/// translated by looking the specproc up here.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TriggerAction {
    /// Set a single bool field on `MobileFlags` by snake_case name.
    /// Rejected for OBJ/ROOM bindings (warn instead).
    SetMobFlag { ironmud_flag: String },
    /// Append a `MobileTrigger` to the mob's `triggers` Vec. `args` from
    /// `fallback_args` are used unless the parser captured something
    /// specific (e.g. puff()'s do_say quotes).
    AddMobTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        interval_secs: Option<i64>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    AddItemTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    AddRoomTrigger {
        trigger_type: String,
        script_name: String,
        #[serde(default)]
        chance: Option<i32>,
        #[serde(default)]
        interval_secs: Option<i64>,
        #[serde(default)]
        fallback_args: Vec<String>,
    },
    /// Surface as a Warn so a builder can re-author the behavior in Rhai.
    /// Multiple bindings of the same specproc collapse to one dedup line.
    Warn { message: String },
    /// Silently drop the binding.
    Drop {
        #[serde(default)]
        info: Option<String>,
    },
}

const DEFAULT_ROOM_MAPPING_JSON: &str = include_str!("../../scripts/data/import/circle_room_mapping.json");
const DEFAULT_MOB_MAPPING_JSON: &str = include_str!("../../scripts/data/import/circle_mob_mapping.json");
const DEFAULT_OBJ_MAPPING_JSON: &str = include_str!("../../scripts/data/import/circle_obj_mapping.json");
const DEFAULT_SHOP_MAPPING_JSON: &str = include_str!("../../scripts/data/import/circle_shop_mapping.json");
const DEFAULT_TRIGGER_MAPPING_JSON: &str = include_str!("../../scripts/data/import/circle_trigger_mapping.json");

impl CircleMappingTable {
    pub fn load_default() -> Self {
        // The bundled JSON is checked at compile time via `include_str!` and
        // tested below; a runtime parse failure would mean we shipped a
        // broken default. Panic so it's caught in dev.
        let mut table: Self = serde_json::from_str(DEFAULT_ROOM_MAPPING_JSON)
            .expect("bundled circle_room_mapping.json must parse");
        let mob: Self =
            serde_json::from_str(DEFAULT_MOB_MAPPING_JSON).expect("bundled circle_mob_mapping.json must parse");
        let obj: Self =
            serde_json::from_str(DEFAULT_OBJ_MAPPING_JSON).expect("bundled circle_obj_mapping.json must parse");
        let shop: Self =
            serde_json::from_str(DEFAULT_SHOP_MAPPING_JSON).expect("bundled circle_shop_mapping.json must parse");
        let trig: Self = serde_json::from_str(DEFAULT_TRIGGER_MAPPING_JSON)
            .expect("bundled circle_trigger_mapping.json must parse");
        // Merge: room mapping JSON owns sector + room actions; mob mapping
        // JSON owns mob + aff actions; obj mapping JSON owns extra + apply
        // actions; shop mapping JSON owns buy_type actions; trigger mapping
        // JSON owns trigger_actions. Either file is allowed to define any
        // section so a `--mapping` override can fully replace defaults.
        table.mob_flag_actions = mob.mob_flag_actions;
        table.aff_flag_actions = mob.aff_flag_actions;
        table.extra_flag_actions = obj.extra_flag_actions;
        table.apply_actions = obj.apply_actions;
        table.buy_type_actions = shop.buy_type_actions;
        table.trigger_actions = trig.trigger_actions;
        table
    }

    pub fn load_from_path(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading mapping {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing mapping {}", path.display()))
    }
}

pub fn ir_to_plan(ir: &ImportIR, opts: &MappingOptions) -> (Plan, Vec<Warning>) {
    let mut plan = Plan::default();
    let mut warnings = Vec::new();
    // Track prefixes assigned within *this* import for in-import
    // disambiguation. Existing DB prefixes are *not* in this list — those
    // become Block warnings instead of being silently suffixed, so a
    // double-apply is loud rather than a stealth no-op.
    let mut prefix_in_use: Vec<String> = Vec::new();
    // Names of E-block attributes already warned about. Stock CircleMUD
    // mostly uses `BareHandAttack`; without dedup the report would carry
    // hundreds of identical per-mob lines.
    let mut seen_extra_attrs: HashSet<String> = HashSet::new();

    for zone in &ir.zones {
        if zone.rooms.is_empty()
            && zone.mobiles.is_empty()
            && zone.items.is_empty()
            && zone.resets.is_empty()
            && zone.deferred.is_empty()
        {
            continue;
        }
        let prefix = unique_prefix(&zone.name, zone.vnum, &prefix_in_use);
        prefix_in_use.push(prefix.clone());

        if opts.existing_area_prefixes.iter().any(|p| p == &prefix) {
            warnings.push(
                Warning::new(
                    WarningKind::PrefixCollision,
                    Severity::Block,
                    zone.source.clone(),
                    format!("area prefix '{prefix}' already exists in target DB"),
                )
                .with_suggestion("rename the source zone or remove the existing area before re-running --apply"),
            );
        }

        plan.areas.push(PlannedArea {
            source_vnum: zone.vnum,
            name: zone.name.trim().to_string(),
            prefix: prefix.clone(),
            description: zone
                .description
                .clone()
                .unwrap_or_else(|| format!("Imported from CircleMUD zone #{}", zone.vnum)),
        });

        // Surface deferred features (zone resets, etc.) as warnings.
        for d in &zone.deferred {
            warnings.push(deferred_to_warning(d));
        }

        for room in &zone.rooms {
            let (room_plan, room_warnings) = map_room(zone, &prefix, room, opts);
            warnings.extend(room_warnings);
            // Build exits from the room's parsed IrExit list.
            for ex in &room.exits {
                plan.exits.push(PlannedExit {
                    from_vnum: room_plan.vnum.clone(),
                    direction: ex.direction.clone(),
                    to_source_vnum: ex.to_room_vnum,
                });
            }
            // Vnum-collision check against existing IronMUD rooms.
            if opts.existing_room_vnums.iter().any(|v| v == &room_plan.vnum) {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    room_plan.source.clone(),
                    format!("room vnum '{}' already exists in target DB", room_plan.vnum),
                ));
            }
            plan.rooms.push(room_plan);
        }

        for mob in &zone.mobiles {
            let (mob_plan, mob_warnings) = map_mob(zone, &prefix, mob, opts, &mut seen_extra_attrs);
            warnings.extend(mob_warnings);
            // Mobile vnum collision: Block, mirrors room behavior.
            if opts
                .existing_mobile_vnums
                .iter()
                .any(|v| v.eq_ignore_ascii_case(&mob_plan.vnum))
            {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    mob_plan.source.clone(),
                    format!("mobile vnum '{}' already exists in target DB", mob_plan.vnum),
                ));
            }
            plan.mobiles.push(mob_plan);
        }

        for item in &zone.items {
            let (item_plan, item_warnings) = map_item(zone, &prefix, item, opts);
            warnings.extend(item_warnings);
            // Item vnum collision against an existing prototype: Block.
            if opts
                .existing_item_vnums
                .iter()
                .any(|v| v.eq_ignore_ascii_case(&item_plan.vnum))
            {
                warnings.push(Warning::new(
                    WarningKind::DuplicateVnum,
                    Severity::Block,
                    item_plan.source.clone(),
                    format!("item vnum '{}' already exists in target DB", item_plan.vnum),
                ));
            }
            plan.items.push(item_plan);
        }
    }

    // Shops cross-cut zones: a `.shp` file's keeper mob may live in a
    // different zone than the shop record itself. Build global vnum
    // indices from the per-zone passes above, then resolve shop overlays
    // in a second sweep across all zones.
    let mob_index: HashMap<i32, String> = plan
        .mobiles
        .iter()
        .map(|m| (m.source_vnum, m.vnum.clone()))
        .collect();
    let item_index: HashMap<i32, String> = plan
        .items
        .iter()
        .map(|i| (i.source_vnum, i.vnum.clone()))
        .collect();

    for zone in &ir.zones {
        for shop in &zone.shops {
            let (overlay, shop_warnings) = map_shop(shop, &mob_index, &item_index, opts);
            warnings.extend(shop_warnings);
            if let Some(o) = overlay {
                plan.shop_overlays.push(o);
            }
        }
    }

    // Zone reset commands → spawn points + door overrides. Walk each zone's
    // resets in source order; needs the prefix from PlannedArea + global mob
    // and item indices. Door overrides mutate already-planned rooms in place.
    let prefix_by_source_vnum: HashMap<i32, String> = plan
        .areas
        .iter()
        .map(|a| (a.source_vnum, a.prefix.clone()))
        .collect();
    // Item-vnum → ItemType for container detection (P chains onto Containers).
    let item_type_by_source_vnum: HashMap<i32, ItemType> = plan
        .items
        .iter()
        .map(|p| (p.source_vnum, p.data.item_type))
        .collect();

    for zone in &ir.zones {
        if zone.resets.is_empty() {
            continue;
        }
        let Some(prefix) = prefix_by_source_vnum.get(&zone.vnum).cloned() else {
            continue;
        };
        let respawn_secs = zone.default_respawn_secs.unwrap_or(300);
        let (spawns, door_overrides, reset_warnings) = map_resets(
            zone,
            &prefix,
            respawn_secs,
            &mob_index,
            &item_index,
            &item_type_by_source_vnum,
        );
        warnings.extend(reset_warnings);
        plan.spawns.extend(spawns);
        // Apply D-reset door overrides to the already-planned rooms.
        for ov in door_overrides {
            apply_door_override(&mut plan.rooms, &prefix, &ov, &mut warnings);
        }
    }

    // Specproc / trigger overlays. spec_assign.c is one tree-wide file, so
    // these live on `ir.triggers` (not per-zone). Resolve each binding via
    // the global mob/item/room indexes and emit a `PlannedTriggerOverlay`
    // (or a Warn).
    if !ir.triggers.is_empty() {
        let room_index: HashMap<i32, String> = plan
            .rooms
            .iter()
            .map(|r| (r.source_vnum, r.vnum.clone()))
            .collect();
        let (overlays, trig_warnings) = map_triggers(&ir.triggers, &mob_index, &item_index, &room_index, opts);
        plan.trigger_overlays.extend(overlays);
        warnings.extend(trig_warnings);
    }

    (plan, warnings)
}

/// Door state change requested by a CircleMUD `D` reset. Applied after
/// all reset translation by mutating an existing [`PlannedDoor`] on the
/// matching planned room. If the room exists but has no door on that
/// direction, surfaces a warning rather than fabricating one.
#[derive(Debug, Clone)]
struct DoorOverride {
    room_source_vnum: i32,
    direction: String,
    is_closed: bool,
    is_locked: bool,
    source: SourceLoc,
}

fn apply_door_override(
    rooms: &mut [PlannedRoom],
    area_prefix: &str,
    ov: &DoorOverride,
    warnings: &mut Vec<Warning>,
) {
    let target_vnum = format!("{}_{}", area_prefix, ov.room_source_vnum);
    let Some(room) = rooms.iter_mut().find(|r| r.vnum == target_vnum) else {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            ov.source.clone(),
            format!(
                "D reset targets room #{} but it wasn't imported in this run",
                ov.room_source_vnum
            ),
        ));
        return;
    };
    let Some(door) = room.doors.iter_mut().find(|d| d.direction == ov.direction) else {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            ov.source.clone(),
            format!(
                "D reset on room #{} {} has no matching door (no EX_ISDOOR exit) — drop",
                ov.room_source_vnum, ov.direction
            ),
        ));
        return;
    };
    door.is_closed = ov.is_closed;
    door.is_locked = ov.is_locked;
}

const RESET_DIRECTIONS: &[&str] = &["north", "east", "south", "west", "up", "down"];

/// Translate a single zone's CircleMUD reset commands into [`PlannedSpawn`]s
/// + [`DoorOverride`]s + warnings for anything we can't model.
///
/// Anchor tracking: G/E with `if=1` chain onto the most-recent translated M;
/// P with `if=1` chains onto the most-recent translated O *if* that O loaded
/// a Container item. The runtime "did the M actually spawn this tick?" check
/// is intentionally not modelled — that's the spawn tick's job
/// (`src/ticks/spawn.rs`); we treat any translated parent as a live anchor.
fn map_resets(
    zone: &IrZone,
    area_prefix: &str,
    respawn_secs: i64,
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
    item_type_by_source: &HashMap<i32, ItemType>,
) -> (Vec<PlannedSpawn>, Vec<DoorOverride>, Vec<Warning>) {
    let mut spawns: Vec<PlannedSpawn> = Vec::new();
    let mut doors: Vec<DoorOverride> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut last_mob_idx: Option<usize> = None;
    let mut last_obj_idx: Option<usize> = None;
    // Track NECK_1+NECK_2 collision per-mob (warn-once).
    let mut neck_warned_for: HashSet<usize> = HashSet::new();
    // Track which mob spawn points already had a slot used to detect
    // duplicate-equip-on-same-slot (Circle's two-neck case).
    let mut used_neck_slot: HashMap<usize, bool> = HashMap::new();

    for reset in &zone.resets {
        match &reset.kind {
            IrResetKind::LoadMob {
                vnum,
                max,
                room_vnum,
            } => {
                let Some(mob_pref) = mob_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("M reset references mob #{vnum} not in import set — drop"),
                    ));
                    last_mob_idx = None;
                    continue;
                };
                let room_pref = format!("{}_{}", area_prefix, room_vnum);
                spawns.push(PlannedSpawn {
                    area_prefix: area_prefix.to_string(),
                    vnum: mob_pref.clone(),
                    entity_type: SpawnEntityType::Mobile,
                    room_vnum: room_pref,
                    max_count: (*max).max(1),
                    respawn_interval_secs: respawn_secs,
                    dependencies: Vec::new(),
                    source: reset.source.clone(),
                });
                last_mob_idx = Some(spawns.len() - 1);
                // O→P chains shouldn't survive an intervening M.
                last_obj_idx = None;
            }
            IrResetKind::LoadObj {
                vnum,
                max,
                room_vnum,
            } => {
                let Some(item_pref) = item_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("O reset references obj #{vnum} not in import set — drop"),
                    ));
                    last_obj_idx = None;
                    continue;
                };
                let room_pref = format!("{}_{}", area_prefix, room_vnum);
                spawns.push(PlannedSpawn {
                    area_prefix: area_prefix.to_string(),
                    vnum: item_pref.clone(),
                    entity_type: SpawnEntityType::Item,
                    room_vnum: room_pref,
                    max_count: (*max).max(1),
                    respawn_interval_secs: respawn_secs,
                    dependencies: Vec::new(),
                    source: reset.source.clone(),
                });
                // Only track O as a P-chain anchor if the item is actually
                // a container — chaining P onto a non-container is meaningless.
                last_obj_idx = match item_type_by_source.get(vnum) {
                    Some(ItemType::Container) => Some(spawns.len() - 1),
                    _ => None,
                };
            }
            IrResetKind::GiveObj { vnum, max: _ } => {
                if !reset.if_flag {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("G reset with if=0 for obj #{vnum} has no anchor — drop"),
                    ));
                    continue;
                }
                let Some(parent_idx) = last_mob_idx else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("G reset for obj #{vnum} has no preceding M — drop"),
                    ));
                    continue;
                };
                let Some(item_pref) = item_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("G reset references obj #{vnum} not in import set — drop"),
                    ));
                    continue;
                };
                spawns[parent_idx].dependencies.push(PlannedSpawnDep {
                    item_vnum: item_pref.clone(),
                    destination: SpawnDestination::Inventory,
                    count: 1,
                });
            }
            IrResetKind::EquipObj { vnum, max: _, wear_loc } => {
                if !reset.if_flag {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("E reset with if=0 for obj #{vnum} has no anchor — drop"),
                    ));
                    continue;
                }
                let Some(parent_idx) = last_mob_idx else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("E reset for obj #{vnum} has no preceding M — drop"),
                    ));
                    continue;
                };
                let Some(item_pref) = item_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("E reset references obj #{vnum} not in import set — drop"),
                    ));
                    continue;
                };
                let Some(loc) = super::engines::circle::wear::map_wear_loc(*wear_loc) else {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedValueSemantic,
                        Severity::Warn,
                        reset.source.clone(),
                        format!(
                            "E reset wear-slot {wear_loc} for obj #{vnum} has no IronMUD analogue — drop"
                        ),
                    ));
                    continue;
                };
                // NECK_1 + NECK_2 collision: both map to Neck, second is dropped per mob.
                if super::engines::circle::wear::is_neck_slot(*wear_loc) {
                    let already = used_neck_slot.entry(parent_idx).or_insert(false);
                    if *already {
                        if !neck_warned_for.contains(&parent_idx) {
                            warnings.push(Warning::new(
                                WarningKind::UnsupportedValueSemantic,
                                Severity::Warn,
                                reset.source.clone(),
                                format!(
                                    "E reset uses both NECK_1 and NECK_2 on the same mob (obj #{vnum}); IronMUD has one Neck slot — drop second"
                                ),
                            ));
                            neck_warned_for.insert(parent_idx);
                        }
                        continue;
                    }
                    *already = true;
                }
                if super::engines::circle::wear::is_paired_slot_collapse(*wear_loc) {
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        reset.source.clone(),
                        format!(
                            "E reset slot {wear_loc} (LEGS/FEET/HANDS/ARMS) collapsed to left-side IronMUD slot for obj #{vnum}"
                        ),
                    ));
                }
                spawns[parent_idx].dependencies.push(PlannedSpawnDep {
                    item_vnum: item_pref.clone(),
                    destination: SpawnDestination::Equipped(loc),
                    count: 1,
                });
            }
            IrResetKind::PutObj {
                vnum,
                max: _,
                container_vnum,
            } => {
                if !reset.if_flag {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("P reset with if=0 for obj #{vnum} — drop"),
                    ));
                    continue;
                }
                let Some(parent_idx) = last_obj_idx else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!(
                            "P reset for obj #{vnum} into container #{container_vnum} — no preceding O of a Container in this zone (cross-block chain) — drop"
                        ),
                    ));
                    continue;
                };
                // Verify the immediately-prior O matches the P's container vnum.
                let parent_container_pref = item_index.get(container_vnum);
                let parent_vnum = &spawns[parent_idx].vnum;
                if parent_container_pref.map(|p| p != parent_vnum).unwrap_or(true) {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!(
                            "P reset target container #{container_vnum} doesn't match prior O ({}); cross-container chains not modelled — drop",
                            parent_vnum
                        ),
                    ));
                    continue;
                }
                let Some(item_pref) = item_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("P reset references obj #{vnum} not in import set — drop"),
                    ));
                    continue;
                };
                spawns[parent_idx].dependencies.push(PlannedSpawnDep {
                    item_vnum: item_pref.clone(),
                    destination: SpawnDestination::Container,
                    count: 1,
                });
            }
            IrResetKind::SetDoor {
                room_vnum,
                dir,
                state,
            } => {
                let direction = match RESET_DIRECTIONS.get(*dir as usize) {
                    Some(d) => (*d).to_string(),
                    None => {
                        warnings.push(Warning::new(
                            WarningKind::DeferredFeature,
                            Severity::Warn,
                            reset.source.clone(),
                            format!("D reset on room #{room_vnum} has invalid direction {dir} — drop"),
                        ));
                        continue;
                    }
                };
                let (is_closed, is_locked) = match state {
                    0 => (false, false),
                    1 => (true, false),
                    2 => (true, true),
                    _ => {
                        warnings.push(Warning::new(
                            WarningKind::DeferredFeature,
                            Severity::Warn,
                            reset.source.clone(),
                            format!(
                                "D reset on room #{room_vnum} has unsupported state {state} (only 0/1/2) — drop"
                            ),
                        ));
                        continue;
                    }
                };
                doors.push(DoorOverride {
                    room_source_vnum: *room_vnum,
                    direction,
                    is_closed,
                    is_locked,
                    source: reset.source.clone(),
                });
            }
            IrResetKind::RemoveObj { room_vnum, vnum } => {
                warnings.push(
                    Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!(
                            "R reset (remove obj #{vnum} from room #{room_vnum}) has no IronMUD analogue — drop"
                        ),
                    )
                    .with_suggestion("revisit if a per-room cleanup hook lands; see import-guide backlog"),
                );
            }
        }
    }
    (spawns, doors, warnings)
}

fn deferred_to_warning(d: &DeferredItem) -> Warning {
    Warning::new(
        WarningKind::DeferredFeature,
        Severity::Warn,
        d.source.clone(),
        format!("[{}] {}", d.category, d.summary),
    )
    .with_suggestion("translate manually to a spawn point / trigger after import (zone resets are not applied)")
}

fn map_room(zone: &IrZone, area_prefix: &str, room: &IrRoom, opts: &MappingOptions) -> (PlannedRoom, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut flags = RoomFlags::default();
    let _ = zone; // currently unused but kept in the signature for future per-zone overrides

    // Sector → flags via the mapping table.
    let sector_name = super::engines::circle::flags::sector_name(room.sector);
    match opts.circle.sector_to_flags.get(&sector_name) {
        Some(m) => {
            apply_set_flags(&mut flags, &m.set_flags, room, &mut warnings);
            if let Some(info) = &m.info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    room.source.clone(),
                    format!("sector {sector_name}: {info}"),
                ));
            }
        }
        None => warnings.push(Warning::new(
            WarningKind::UnsupportedSector,
            Severity::Warn,
            room.source.clone(),
            format!(
                "unknown CircleMUD sector {sector_name} ({}); no flags applied",
                room.sector
            ),
        )),
    }

    // Decode room-flag bits and apply per-flag actions.
    let (known, unknown) = super::engines::circle::flags::decode_room_flags(room.flag_bits);
    for flag in known {
        match opts.circle.room_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_room_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        room.source.clone(),
                        format!("mapping points {flag} → {ironmud_flag}, but no such IronMUD RoomFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { value }) => match CombatZoneType::from_str(value) {
                Some(z) => flags.combat_zone = Some(z),
                None => warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("unknown combat_zone value {value:?} for flag {flag}"),
                )),
            },
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("ROOM_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. }) | Some(FlagAction::SetArmorClass { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!("mapping uses an item-only action for ROOM_{flag}; ignored on rooms"),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                room.source.clone(),
                format!("no mapping for ROOM_{flag}"),
            )),
        }
    }
    for u in unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            room.source.clone(),
            format!("unrecognised room flag bit {u} (likely a patched flag)"),
        ));
    }

    // Doors.
    let mut doors = Vec::new();
    for ex in &room.exits {
        let (door_known, door_unknown) = super::engines::circle::flags::decode_exit_flags(ex.door_flags);
        let mut is_door = false;
        let mut is_closed = false;
        let mut is_locked = false;
        for f in &door_known {
            match *f {
                "ISDOOR" => is_door = true,
                "CLOSED" => is_closed = true,
                "LOCKED" => is_locked = true,
                "PICKPROOF" => warnings.push(Warning::new(
                    WarningKind::UnsupportedDoorFlag,
                    Severity::Warn,
                    room.source.clone(),
                    format!(
                        "exit {} pickproof flag not modeled; treating as locked door",
                        ex.direction
                    ),
                )),
                _ => {}
            }
        }
        for u in door_unknown {
            warnings.push(Warning::new(
                WarningKind::UnsupportedDoorFlag,
                Severity::Warn,
                room.source.clone(),
                format!("exit {} unknown door flag bit {u}", ex.direction),
            ));
        }
        if !is_door && !is_closed && !is_locked {
            continue;
        }
        let (name, keywords) = match &ex.keyword {
            Some(kw) => {
                let mut parts: Vec<String> = kw.split_whitespace().map(str::to_string).collect();
                let primary = parts.first().cloned().unwrap_or_else(|| "door".to_string());
                let extras = if parts.len() > 1 {
                    parts.split_off(1)
                } else {
                    Vec::new()
                };
                (primary, extras)
            }
            None => ("door".to_string(), Vec::new()),
        };
        doors.push(PlannedDoor {
            direction: ex.direction.clone(),
            name,
            keywords,
            description: ex.general_description.clone(),
            is_closed,
            is_locked,
            key_source_vnum: ex.key_vnum,
        });
    }

    let extra_descs: Vec<ExtraDesc> = room
        .extras
        .iter()
        .map(|e| ExtraDesc {
            keywords: e.keywords.clone(),
            description: e.description.clone(),
        })
        .collect();

    let vnum = format!("{}_{}", area_prefix, room.vnum);
    let title = room.name.trim().to_string();
    let description = room.description.clone();

    (
        PlannedRoom {
            area_prefix: area_prefix.to_string(),
            source_vnum: room.vnum,
            vnum,
            title,
            description,
            flags,
            extra_descs,
            doors,
            source: room.source.clone(),
        },
        warnings,
    )
}

fn apply_set_flags(flags: &mut RoomFlags, names: &[String], room: &IrRoom, warnings: &mut Vec<Warning>) {
    for name in names {
        if !apply_named_room_flag(flags, name) {
            warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                room.source.clone(),
                format!("mapping references unknown IronMUD RoomFlag '{name}'"),
            ));
        }
    }
}

/// Set a `RoomFlags` bool by snake_case name. Returns false if the name
/// isn't a known flag — surfaces typos in the mapping JSON.
/// Set a `MobileFlags` bool by snake_case name. Returns false if the name
/// isn't a known flag — surfaces typos in the mapping JSON.
fn apply_named_mob_flag(flags: &mut MobileFlags, name: &str) -> bool {
    match name {
        "aggressive" => flags.aggressive = true,
        "sentinel" => flags.sentinel = true,
        "scavenger" => flags.scavenger = true,
        "shopkeeper" => flags.shopkeeper = true,
        "no_attack" => flags.no_attack = true,
        "healer" => flags.healer = true,
        "leasing_agent" => flags.leasing_agent = true,
        "cowardly" => flags.cowardly = true,
        "can_open_doors" => flags.can_open_doors = true,
        "guard" => flags.guard = true,
        "thief" => flags.thief = true,
        "cant_swim" => flags.cant_swim = true,
        "poisonous" => flags.poisonous = true,
        "fiery" => flags.fiery = true,
        "chilling" => flags.chilling = true,
        "corrosive" => flags.corrosive = true,
        "shocking" => flags.shocking = true,
        _ => return false,
    }
    true
}

fn apply_named_room_flag(flags: &mut RoomFlags, name: &str) -> bool {
    match name {
        "dark" => flags.dark = true,
        "no_mob" => flags.no_mob = true,
        "indoors" => flags.indoors = true,
        "underwater" => flags.underwater = true,
        "climate_controlled" => flags.climate_controlled = true,
        "always_hot" => flags.always_hot = true,
        "always_cold" => flags.always_cold = true,
        "city" => flags.city = true,
        "no_windows" => flags.no_windows = true,
        "difficult_terrain" => flags.difficult_terrain = true,
        "dirt_floor" => flags.dirt_floor = true,
        "property_storage" => flags.property_storage = true,
        "post_office" => flags.post_office = true,
        "bank" => flags.bank = true,
        "garden" => flags.garden = true,
        "spawn_point" => flags.spawn_point = true,
        "shallow_water" => flags.shallow_water = true,
        "deep_water" => flags.deep_water = true,
        "liveable" => flags.liveable = true,
        "private" | "private_room" => flags.private_room = true,
        "tunnel" => flags.tunnel = true,
        "death" => flags.death = true,
        "no_magic" => flags.no_magic = true,
        _ => return false,
    }
    true
}

fn map_mob(
    zone: &IrZone,
    area_prefix: &str,
    mob: &IrMob,
    opts: &MappingOptions,
    seen_extra_attrs: &mut HashSet<String>,
) -> (PlannedMobile, Vec<Warning>) {
    let _ = zone;
    let mut warnings = Vec::new();
    let mut flags = MobileFlags::default();

    // MOB_* bits
    let (known_mob, unknown_mob) = super::engines::circle::flags::decode_mob_flags(mob.mob_flag_bits);
    for flag in known_mob {
        match opts.circle.mob_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_mob_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        mob.source.clone(),
                        format!("mapping points MOB_{flag} → {ironmud_flag}, but no such IronMUD MobileFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses set_combat_zone for MOB_{flag} — that action only applies to rooms"),
                ));
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("MOB_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. }) | Some(FlagAction::SetArmorClass { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses an item-only action for MOB_{flag}; ignored on mobs"),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("no mapping for MOB_{flag}"),
            )),
        }
    }
    for u in unknown_mob {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            mob.source.clone(),
            format!("unrecognised mob flag bit {u} (likely a patched flag)"),
        ));
    }

    // AFF_* bits — most have no IronMUD equivalent. Anything not listed in
    // the mapping JSON gets a default "permanent affect not modeled" warn,
    // generated here so the JSON stays compact.
    let (known_aff, unknown_aff) = super::engines::circle::flags::decode_aff_flags(mob.aff_flag_bits);
    for flag in known_aff {
        match opts.circle.aff_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_mob_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        mob.source.clone(),
                        format!("mapping points AFF_{flag} → {ironmud_flag}, but no such IronMUD MobileFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses set_combat_zone for AFF_{flag} — that action only applies to rooms"),
                ));
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("AFF_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. }) | Some(FlagAction::SetArmorClass { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses an item-only action for AFF_{flag}; ignored on mobs"),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("permanent AFF_{flag} not modeled at prototype level"),
            )),
        }
    }
    for u in unknown_aff {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            mob.source.clone(),
            format!("unrecognised affected-by flag bit {u} (likely a patched flag)"),
        ));
    }

    // Numeric stats with no IronMUD equivalent. Most are silently dropped;
    // a few warn so builders know to revisit balance.
    if mob.alignment != 0 {
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            mob.source.clone(),
            format!(
                "alignment {} dropped (IronMUD has no alignment system)",
                mob.alignment
            ),
        ));
    }
    if mob.sex == 1 || mob.sex == 2 {
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            mob.source.clone(),
            "sex/gender not modeled at prototype level (Characteristics live on simulated migrants only)".to_string(),
        ));
    }

    // E-block named attrs: warn once per distinct attribute name across the
    // whole import. `BareHandAttack` shows up on dozens of stock mobs and
    // would otherwise dominate the report.
    for (name, value) in &mob.extra_attrs {
        if seen_extra_attrs.insert(name.clone()) {
            warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("E-block attr {name:?} (e.g. {value:?}) not imported"),
            ));
        }
    }

    let max_hp = dice_max(&mob.hp_dice).unwrap_or(0).max(1);
    let level = mob.level.max(0);
    let gold = mob.gold.max(0);

    let vnum = format!("{}_{}", area_prefix, mob.vnum);

    (
        PlannedMobile {
            area_prefix: area_prefix.to_string(),
            source_vnum: mob.vnum,
            vnum,
            name: mob.short_descr.clone(),
            short_desc: mob.long_descr.clone(),
            long_desc: mob.description.clone(),
            keywords: mob.keywords.clone(),
            level,
            max_hp,
            damage_dice: mob.damage_dice.clone(),
            armor_class: mob.ac,
            gold,
            flags,
            source: mob.source.clone(),
        },
        warnings,
    )
}

/// Compute the maximum value of a dice expression like `5d10+550` or
/// `2d6+3`. Returns `None` if the input doesn't parse.
fn dice_max(expr: &str) -> Option<i32> {
    let s = expr.trim();
    let (dice_part, bonus): (&str, i32) = match s.find(['+', '-']) {
        Some(i) => {
            let (left, right) = s.split_at(i);
            let bonus: i32 = right.parse().ok()?;
            (left, bonus)
        }
        None => (s, 0),
    };
    let (n, sides) = dice_part.split_once('d')?;
    let n: i32 = n.parse().ok()?;
    let sides: i32 = sides.parse().ok()?;
    Some(n * sides + bonus)
}

/// Slug a zone name into an IronMUD area prefix (alphanumeric + underscore,
/// lowercase). Falls back to `zone_<vnum>` for empty / collision cases.
fn unique_prefix(name: &str, vnum: i32, taken: &[String]) -> String {
    let base = slug(name);
    let base = if base.is_empty() { format!("zone_{vnum}") } else { base };
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let with_vnum = format!("{base}_{vnum}");
    if !taken.iter().any(|t| t == &with_vnum) {
        return with_vnum;
    }
    // Should be vanishingly rare; final fallback walks an integer suffix.
    let mut i = 2;
    loop {
        let candidate = format!("{with_vnum}_{i}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        i += 1;
    }
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_was_underscore = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_underscore = false;
        } else if !last_was_underscore && !out.is_empty() {
            out.push('_');
            last_was_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Map an `IrItem` to an IronMUD `ItemData` prototype plus warnings for
/// anything we couldn't translate cleanly. This is the item analogue of
/// `map_room` / `map_mob`. The pipeline:
///   1. base `ItemData` from name/short/long
///   2. ITEM_TYPE → `ItemType` and type-specific value handling
///   3. ITEM_* extra-bit decode via the JSON action table
///   4. ITEM_WEAR_* decode (hard-coded; the right-hand side is a Vec)
///   5. APPLY_* affect decode via the JSON action table
///   6. extra descriptions surface as a single `DeferredFeature` warning
fn map_item(zone: &IrZone, area_prefix: &str, item: &IrItem, opts: &MappingOptions) -> (PlannedItem, Vec<Warning>) {
    let _ = zone;
    let mut warnings = Vec::new();

    let mut data = ItemData::new(
        item.short_descr.clone(),
        item.short_descr.clone(),
        if item.long_descr.is_empty() {
            item.short_descr.clone()
        } else {
            item.long_descr.clone()
        },
    );
    data.is_prototype = true;
    let vnum = format!("{}_{}", area_prefix, item.vnum);
    data.vnum = Some(vnum.clone());
    data.keywords = item.keywords.clone();
    data.weight = item.weight.max(0);
    data.value = item.cost.max(0);

    if !item.action_descr.is_empty() {
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            item.source.clone(),
            "action description present (CircleMUD use-message); discarded — IronMUD has no analogue".to_string(),
        ));
    }

    // Type-specific decode: sets ItemType and any value-derived fields.
    apply_item_type(item, &mut data, &mut warnings, area_prefix);

    // ITEM_* extra-bit decode via the JSON table.
    let (extra_known, extra_unknown) = super::engines::circle::flags::decode_extra_flags(item.extra_flag_bits);
    for flag in extra_known {
        match opts.circle.extra_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_item_flag(&mut data.flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        item.source.clone(),
                        format!("mapping points ITEM_{flag} → {ironmud_flag}, but no such IronMUD ItemFlag"),
                    ));
                }
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!("ITEM_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetCombatZone { .. })
            | Some(FlagAction::SetStat { .. })
            | Some(FlagAction::SetArmorClass { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!(
                        "mapping uses an action that doesn't apply to extra-bits (ITEM_{flag}); ignored"
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                item.source.clone(),
                format!("no mapping for ITEM_{flag}"),
            )),
        }
    }
    for u in extra_unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            item.source.clone(),
            format!("unrecognised extra-flag bit {u} (likely a patched flag)"),
        ));
    }

    // ITEM_WEAR_* decode. The right-hand side is a Vec<WearLocation> per
    // CircleMUD bit, so we hard-code rather than going through the JSON.
    let (wear_known, wear_unknown) = super::engines::circle::flags::decode_wear_flags(item.wear_flag_bits);
    let mut wear_locations: Vec<WearLocation> = Vec::new();
    let mut takeable = false;
    for flag in wear_known {
        match flag {
            "TAKE" => takeable = true,
            "FINGER" => {
                wear_locations.push(WearLocation::FingerLeft);
                wear_locations.push(WearLocation::FingerRight);
            }
            "NECK" => wear_locations.push(WearLocation::Neck),
            "BODY" => wear_locations.push(WearLocation::Torso),
            "HEAD" => wear_locations.push(WearLocation::Head),
            "LEGS" => {
                wear_locations.push(WearLocation::LeftLeg);
                wear_locations.push(WearLocation::RightLeg);
            }
            "FEET" => {
                wear_locations.push(WearLocation::LeftFoot);
                wear_locations.push(WearLocation::RightFoot);
            }
            "HANDS" => {
                wear_locations.push(WearLocation::LeftHand);
                wear_locations.push(WearLocation::RightHand);
            }
            "ARMS" => {
                wear_locations.push(WearLocation::LeftArm);
                wear_locations.push(WearLocation::RightArm);
            }
            "SHIELD" => wear_locations.push(WearLocation::OffHand),
            "ABOUT" => wear_locations.push(WearLocation::Back),
            "WAIST" => wear_locations.push(WearLocation::Waist),
            "WRIST" => {
                wear_locations.push(WearLocation::WristLeft);
                wear_locations.push(WearLocation::WristRight);
            }
            "WIELD" => wear_locations.push(WearLocation::Wielded),
            "HOLD" => wear_locations.push(WearLocation::Ready),
            _ => {}
        }
    }
    for u in wear_unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            item.source.clone(),
            format!("unrecognised wear-flag bit {u} (likely a patched flag)"),
        ));
    }
    if !takeable {
        // Stock CircleMUD uses !TAKE for fixtures and signs. IronMUD has no
        // "fixed in place" notion, so the import surfaces an Info note.
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            item.source.clone(),
            "ITEM_WEAR_TAKE absent — IronMUD has no immovable-item flag; imported as takeable"
                .to_string(),
        ));
    }
    data.wear_locations = wear_locations;

    // APPLY_* affect decode.
    for (loc, modifier) in &item.affects {
        if *loc == 0 {
            continue; // APPLY_NONE — slot exists but is unused.
        }
        let name = super::engines::circle::flags::apply_type_name(*loc);
        match opts.circle.apply_actions.get(&name) {
            Some(FlagAction::SetStat { ironmud_stat }) => {
                if !apply_named_item_stat(&mut data, ironmud_stat, *modifier) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        item.source.clone(),
                        format!("mapping points APPLY_{name} → {ironmud_stat}, but no such ItemData stat field"),
                    ));
                }
            }
            Some(FlagAction::SetArmorClass { .. }) => {
                // Circle: negative-is-better. IronMUD: positive damage
                // reduction. Sign-flip preserves the relative ordering.
                let prior = data.armor_class.unwrap_or(0);
                data.armor_class = Some(prior + (-modifier));
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!("APPLY_{name} ({modifier:+}): {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetFlag { .. })
            | Some(FlagAction::SetCombatZone { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!(
                        "mapping uses an action that doesn't apply to APPLY_* (APPLY_{name}); ignored"
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                item.source.clone(),
                format!("no mapping for APPLY_{name} ({modifier:+}) — affect dropped"),
            )),
        }
    }

    // Extra descriptions: ItemData has no `extra_descs` field today. Surface
    // a single warning per item rather than per-block to keep the report
    // readable on real Circle imports (most named items have at least one).
    if !item.extra_descs.is_empty() {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            item.source.clone(),
            format!(
                "{} extra description(s) on item not imported (no ItemData.extra_descs target yet)",
                item.extra_descs.len()
            ),
        ));
    }

    (
        PlannedItem {
            area_prefix: area_prefix.to_string(),
            source_vnum: item.vnum,
            vnum,
            data,
            source: item.source.clone(),
        },
        warnings,
    )
}

/// Map an `IrShop` to a [`PlannedShopOverlay`]. Returns `None` (with a
/// Warn) if the keeper mob isn't in this import — those shops can't be
/// applied. Most other gaps surface as advisory warnings: messages,
/// temper, bitvector, with_who, multi-room, non-default hours.
fn map_shop(
    shop: &IrShop,
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
    opts: &MappingOptions,
) -> (Option<PlannedShopOverlay>, Vec<Warning>) {
    let mut warnings = Vec::new();

    // Resolve the keeper mob. Without it we can't apply the shop.
    let Some(keeper_vnum) = mob_index.get(&shop.keeper_vnum).cloned() else {
        warnings.push(Warning::new(
            WarningKind::DanglingExit,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} keeper mob #{} is not in the import set; shop dropped",
                shop.vnum, shop.keeper_vnum
            ),
        ));
        return (None, warnings);
    };

    // Producing list. Drop entries we can't resolve, with a per-entry warn.
    let mut stock_vnums: Vec<String> = Vec::new();
    for v in &shop.producing {
        match item_index.get(v) {
            Some(rewritten) => stock_vnums.push(rewritten.clone()),
            None => warnings.push(Warning::new(
                WarningKind::DanglingExit,
                Severity::Warn,
                shop.source.clone(),
                format!(
                    "shop #{}: producing item #{} is not in the import set; entry dropped",
                    shop.vnum, v
                ),
            )),
        }
    }

    // Profit multipliers. Circle profit_buy = markup the shop charges =
    // IronMUD shop_sell_rate (% of base value the player pays). Circle
    // profit_sell = fraction the shop pays = IronMUD shop_buy_rate.
    let sell_rate = (shop.profit_buy * 100.0).round() as i32;
    let buy_rate = (shop.profit_sell * 100.0).round() as i32;
    let sell_rate = sell_rate.clamp(0, 10_000);
    let buy_rate = buy_rate.clamp(0, 10_000);

    // Buy types via the JSON action table. Dedupe.
    let mut buys_types: Vec<String> = Vec::new();
    for token in &shop.buy_types {
        match opts.circle.buy_type_actions.get(token) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                let v = ironmud_flag.to_lowercase();
                if !buys_types.contains(&v) {
                    buys_types.push(v);
                }
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    shop.source.clone(),
                    format!("shop #{} buy_type {token}: {message}", shop.vnum),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(_) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    shop.source.clone(),
                    format!(
                        "shop #{} buy_type {token}: mapping uses an action that doesn't apply to buy_types",
                        shop.vnum
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                shop.source.clone(),
                format!("shop #{} buy_type {token}: no mapping (entry dropped)", shop.vnum),
            )),
        }
    }
    for raw in &shop.unknown_buy_types {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} unrecognised buy_type token {raw:?} (entry dropped)",
                shop.vnum
            ),
        ));
    }

    // Advisory warnings for unsupported features. Builders can revisit
    // these manually; the importer doesn't translate them.
    if shop.messages.iter().any(|m| !m.is_empty()) {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} carries {} custom message string(s); IronMUD has no per-shop messages — discarded",
                shop.vnum,
                shop.messages.iter().filter(|m| !m.is_empty()).count()
            ),
        ));
    }
    if shop.temper != 0 {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Info,
            shop.source.clone(),
            format!("shop #{} temper={} dropped (no IronMUD analogue)", shop.vnum, shop.temper),
        ));
    }
    if shop.bitvector != 0 {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} bitvector={} (WILL_START_FIGHT/WILL_BANK_MONEY) not modeled",
                shop.vnum, shop.bitvector
            ),
        ));
    }
    if shop.with_who != 0 {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} with_who={} (TRADE_NO* alignment/class trade gates) not modeled — shop will trade with anyone",
                shop.vnum, shop.with_who
            ),
        ));
    }
    if shop.rooms.len() > 1 {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} operates in {} rooms; IronMUD shopkeepers travel with their shop — only the keeper's current room is honored",
                shop.vnum,
                shop.rooms.len()
            ),
        ));
    }
    if !is_default_hours(shop) {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} hours (open1={} close1={} open2={} close2={}) not translated — author a daily_routine on the keeper to gate trading",
                shop.vnum, shop.open1, shop.close1, shop.open2, shop.close2
            ),
        ));
    }

    (
        Some(PlannedShopOverlay {
            shop_source_vnum: shop.vnum,
            keeper_source_vnum: shop.keeper_vnum,
            keeper_vnum,
            stock_vnums,
            buy_rate,
            sell_rate,
            buys_types,
            source: shop.source.clone(),
        }),
        warnings,
    )
}

/// Translate `IrTrigger`s (CircleMUD specproc bindings) into
/// [`PlannedTriggerOverlay`]s + warnings. Each binding is looked up in
/// `opts.circle.trigger_actions`; bindings with no entry default to a
/// Warn so the importer never silently loses information.
///
/// To keep the report readable on real Circle imports (`magic_user` is
/// bound to 93 separate mobs in stock 3.1), `Warn`-action specprocs that
/// target ≥3 distinct vnums collapse to a single dedup line listing the
/// count + first 8 vnums. Resolution failures (vnum not in the import
/// set) collapse the same way.
fn map_triggers(
    triggers: &[IrTrigger],
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
    room_index: &HashMap<i32, String>,
    opts: &MappingOptions,
) -> (Vec<PlannedTriggerOverlay>, Vec<Warning>) {
    let mut overlays: Vec<PlannedTriggerOverlay> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    // For Warn-action collapse: specproc → (sample_message, source, vnums)
    let mut warn_buckets: HashMap<String, (String, SourceLoc, Vec<i32>)> = HashMap::new();
    // For "vnum not in import set" collapse: specproc → vnums
    let mut orphan_buckets: HashMap<String, (SourceLoc, Vec<i32>)> = HashMap::new();
    // Track (attach_type, target_vnum) → most recent overlay index so a
    // duplicate ASSIGN line for the same vnum overrides the prior one
    // (matches CircleMUD runtime behavior — last assignment wins).
    let mut last_overlay_for: HashMap<(AttachType, String), usize> = HashMap::new();

    for trig in triggers {
        let action = opts.circle.trigger_actions.get(&trig.specproc_name);
        // Resolve the target vnum first — if it's missing from the import
        // set we drop with a per-specproc collapsed warning, regardless of
        // what the action would have been.
        let target_vnum: Option<String> = match trig.attach_type {
            AttachType::Mob => mob_index.get(&trig.source_vnum).cloned(),
            AttachType::Obj => item_index.get(&trig.source_vnum).cloned(),
            AttachType::Room => room_index.get(&trig.source_vnum).cloned(),
        };
        let Some(target_vnum) = target_vnum else {
            let bucket = orphan_buckets
                .entry(trig.specproc_name.clone())
                .or_insert_with(|| (trig.source.clone(), Vec::new()));
            bucket.1.push(trig.source_vnum);
            continue;
        };

        let mutation: Option<TriggerMutation> = match action {
            Some(TriggerAction::SetMobFlag { ironmud_flag }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses set_mob_flag but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else {
                    Some(TriggerMutation::SetMobFlag {
                        ironmud_flag: ironmud_flag.clone(),
                    })
                }
            }
            Some(TriggerAction::AddMobTrigger {
                trigger_type,
                script_name,
                chance,
                interval_secs,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Mob {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_mob_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_mob_trigger_type(trigger_type) {
                    let mut t = MobileTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    if let Some(s) = interval_secs {
                        t.interval_secs = (*s).max(1);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddMobTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "mobile",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::AddItemTrigger {
                trigger_type,
                script_name,
                chance,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Obj {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_item_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_item_trigger_type(trigger_type) {
                    let mut t = ItemTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddItemTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "item",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::AddRoomTrigger {
                trigger_type,
                script_name,
                chance,
                interval_secs,
                fallback_args,
            }) => {
                if trig.attach_type != AttachType::Room {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        trig.source.clone(),
                        format!(
                            "specproc `{}` mapping uses add_room_trigger but binding attaches to {:?}; ignored",
                            trig.specproc_name, trig.attach_type
                        ),
                    ));
                    None
                } else if let Some(tt) = parse_room_trigger_type(trigger_type) {
                    let mut t = RoomTrigger::default();
                    t.trigger_type = tt;
                    t.script_name = script_name.clone();
                    t.enabled = true;
                    if let Some(c) = chance {
                        t.chance = (*c).clamp(1, 100);
                    }
                    if let Some(s) = interval_secs {
                        t.interval_secs = (*s).max(1);
                    }
                    t.args = if !trig.args.is_empty() {
                        trig.args.clone()
                    } else {
                        fallback_args.clone()
                    };
                    Some(TriggerMutation::AddRoomTrigger(t))
                } else {
                    warnings.push(unknown_trigger_type_warning(
                        "room",
                        trigger_type,
                        &trig.specproc_name,
                        &trig.source,
                    ));
                    None
                }
            }
            Some(TriggerAction::Warn { message }) => {
                let bucket = warn_buckets
                    .entry(trig.specproc_name.clone())
                    .or_insert_with(|| (message.clone(), trig.source.clone(), Vec::new()));
                bucket.2.push(trig.source_vnum);
                None
            }
            Some(TriggerAction::Drop { .. }) => None,
            None => {
                let msg = format!(
                    "no mapping for specproc `{}` — binding dropped (add an entry to circle_trigger_mapping.json)",
                    trig.specproc_name
                );
                let bucket = warn_buckets
                    .entry(trig.specproc_name.clone())
                    .or_insert_with(|| (msg, trig.source.clone(), Vec::new()));
                bucket.2.push(trig.source_vnum);
                None
            }
        };

        if let Some(mutation) = mutation {
            // Last-assignment-wins: if a prior overlay targets the same
            // entity, replace it with this one and emit an Info note.
            let key = (trig.attach_type, target_vnum.clone());
            if let Some(&prior_idx) = last_overlay_for.get(&key) {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    trig.source.clone(),
                    format!(
                        "duplicate specproc binding for vnum {} — `{}` overrides earlier `{}`",
                        target_vnum, trig.specproc_name, overlays[prior_idx].specproc_name
                    ),
                ));
                overlays[prior_idx] = PlannedTriggerOverlay {
                    attach_type: trig.attach_type,
                    target_vnum,
                    specproc_name: trig.specproc_name.clone(),
                    mutation,
                    source: trig.source.clone(),
                };
            } else {
                overlays.push(PlannedTriggerOverlay {
                    attach_type: trig.attach_type,
                    target_vnum: target_vnum.clone(),
                    specproc_name: trig.specproc_name.clone(),
                    mutation,
                    source: trig.source.clone(),
                });
                last_overlay_for.insert(key, overlays.len() - 1);
            }
        }
    }

    // Collapse warn buckets: per-specproc count + first-8 vnum sample.
    for (name, (message, source, vnums)) in warn_buckets {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            source,
            format_specproc_warn(&name, &message, &vnums),
        ));
    }
    // Collapse orphan-vnum buckets the same way.
    for (name, (source, vnums)) in orphan_buckets {
        warnings.push(Warning::new(
            WarningKind::DanglingExit,
            Severity::Warn,
            source,
            format!(
                "specproc `{}` bound to {} vnum(s) not in the import set — binding(s) dropped: {}",
                name,
                vnums.len(),
                format_vnum_sample(&vnums)
            ),
        ));
    }

    (overlays, warnings)
}

fn format_specproc_warn(name: &str, message: &str, vnums: &[i32]) -> String {
    if vnums.len() <= 1 {
        format!("specproc `{}` (vnum {}): {}", name, vnums.first().copied().unwrap_or(0), message)
    } else {
        format!(
            "specproc `{}` ({} bindings): {} — vnums: {}",
            name,
            vnums.len(),
            message,
            format_vnum_sample(vnums)
        )
    }
}

fn format_vnum_sample(vnums: &[i32]) -> String {
    let take = vnums.iter().take(8).map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
    if vnums.len() > 8 {
        format!("{take}, … ({} more)", vnums.len() - 8)
    } else {
        take
    }
}

fn unknown_trigger_type_warning(scope: &str, trigger_type: &str, specproc: &str, source: &SourceLoc) -> Warning {
    Warning::new(
        WarningKind::UnsupportedFlag,
        Severity::Warn,
        source.clone(),
        format!(
            "specproc `{}` mapping has unknown {} trigger_type {:?}; binding dropped",
            specproc, scope, trigger_type
        ),
    )
}

fn parse_mob_trigger_type(s: &str) -> Option<MobileTriggerType> {
    serde_json::from_str::<MobileTriggerType>(&format!("\"{}\"", s.trim())).ok()
}

fn parse_item_trigger_type(s: &str) -> Option<ItemTriggerType> {
    serde_json::from_str::<ItemTriggerType>(&format!("\"{}\"", s.trim())).ok()
}

fn parse_room_trigger_type(s: &str) -> Option<TriggerType> {
    serde_json::from_str::<TriggerType>(&format!("\"{}\"", s.trim())).ok()
}

/// Treat "open all the time" as the default. Stock CircleMUD encodes that
/// as `0 28 0 0` (open at midnight, close at hour 28 which the runtime
/// reads as "always open"; second shift unused).
fn is_default_hours(shop: &IrShop) -> bool {
    let always_first_shift = shop.open1 == 0 && shop.close1 >= 24;
    let no_second_shift = shop.open2 == 0 && shop.close2 == 0;
    always_first_shift && no_second_shift
}

/// Apply CircleMUD ITEM_TYPE-specific value semantics to `data`. Each branch
/// sets ItemType and type-relevant fields; lossy bits surface as warnings.
fn apply_item_type(item: &IrItem, data: &mut ItemData, warnings: &mut Vec<Warning>, area_prefix: &str) {
    let v = item.values;
    let type_name = super::engines::circle::flags::item_type_name(item.item_type);
    match item.item_type {
        // ITEM_LIGHT — capacity hours discarded.
        1 => {
            data.item_type = ItemType::Misc;
            data.flags.provides_light = true;
            if v[2] != 0 {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedValueSemantic,
                    Severity::Warn,
                    item.source.clone(),
                    format!("ITEM_LIGHT capacity hours = {} discarded (no light-burn-time in IronMUD)", v[2]),
                ));
            }
        }
        // ITEM_SCROLL / WAND / STAFF / POTION — spell-list semantics not modeled.
        2 | 3 | 4 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!(
                    "ITEM_{type_name} spell list (level={}, spells {}/{}/{} or charges {}) not imported",
                    v[0], v[1], v[2], v[3], v[2]
                ),
            ));
        }
        // ITEM_WEAPON — values map cleanly to damage dice + damage type.
        5 => {
            data.item_type = ItemType::Weapon;
            data.damage_dice_count = v[1].max(0);
            data.damage_dice_sides = v[2].max(0);
            data.damage_type = circle_weapon_damage_type(v[3]);
        }
        // ITEM_FIRE_WEAPON / MISSILE — unimplemented in stock Circle.
        6 | 7 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!("ITEM_{type_name} unimplemented in stock CircleMUD; imported as Misc"),
            ));
        }
        // ITEM_TREASURE — Misc + categorised so shopkeeper / craft logic can
        // find it later.
        8 => {
            data.item_type = ItemType::Misc;
            data.categories.push("treasure".to_string());
        }
        // ITEM_ARMOR — v0 is the AC bonus. Sign-flip (negative-is-better in
        // Circle, positive-is-better in IronMUD).
        9 => {
            data.item_type = ItemType::Armor;
            data.armor_class = Some(-v[0]);
        }
        // ITEM_POTION — fold into LiquidContainer with capacity 1 sip.
        10 => {
            data.item_type = ItemType::LiquidContainer;
            data.liquid_max = 1;
            data.liquid_current = 1;
            data.liquid_type = LiquidType::HealingPotion;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!(
                    "ITEM_POTION spell list (level={}, spells {}/{}/{}) not imported; modeled as a 1-sip liquid",
                    v[0], v[1], v[2], v[3]
                ),
            ));
        }
        // ITEM_WORN — unimplemented in stock; treat as Misc and let
        // wear_locations carry the slot info.
        11 => {
            data.item_type = ItemType::Misc;
        }
        // ITEM_OTHER — clean Misc.
        12 => {
            data.item_type = ItemType::Misc;
        }
        // ITEM_TRASH — Misc + categories tag.
        13 => {
            data.item_type = ItemType::Misc;
            data.categories.push("trash".to_string());
        }
        // ITEM_TRAP — unimplemented in stock CircleMUD.
        14 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                "ITEM_TRAP unimplemented in stock CircleMUD; imported as Misc".to_string(),
            ));
        }
        // ITEM_CONTAINER — full-fidelity mapping.
        15 => {
            data.item_type = ItemType::Container;
            data.container_max_weight = v[0].max(0);
            // v1 is a small numeric bitvector (closeable/pickproof/closed/locked).
            let bits = v[1] as u32;
            // bit 0 (1) = CLOSEABLE — IronMUD has no "non-closeable" notion; drop.
            if bits & 0b0010 != 0 {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedDoorFlag,
                    Severity::Warn,
                    item.source.clone(),
                    "container PICKPROOF flag not modeled; treated as locked".to_string(),
                ));
            }
            if bits & 0b0100 != 0 {
                data.container_closed = true;
            }
            if bits & 0b1000 != 0 {
                data.container_locked = true;
            }
            // v2 is the key vnum (or -1 for "no key"). Rewrite to the
            // prefixed IronMUD form.
            if v[2] > 0 {
                data.container_key_vnum = Some(format!("{}_{}", area_prefix, v[2]));
            }
        }
        // ITEM_NOTE — blank-paper writing system not modeled.
        16 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                "ITEM_NOTE writing/language semantics not modeled; imported as Misc".to_string(),
            ));
        }
        // ITEM_DRINKCON — full mapping; v2 indexes the drink table.
        17 => {
            data.item_type = ItemType::LiquidContainer;
            data.liquid_max = v[0].max(0);
            data.liquid_current = v[1].max(0);
            let (lt, info) = circle_liquid_index_to_type(v[2]);
            data.liquid_type = lt;
            data.liquid_poisoned = v[3] != 0;
            if let Some(msg) = info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    item.source.clone(),
                    msg,
                ));
            }
        }
        // ITEM_KEY — clean.
        18 => {
            data.item_type = ItemType::Key;
        }
        // ITEM_FOOD — v0 = hours of hunger satisfied → nutrition; v3 = poisoned.
        19 => {
            data.item_type = ItemType::Food;
            data.food_nutrition = v[0].max(0);
            data.food_poisoned = v[3] != 0;
        }
        // ITEM_MONEY — Gold; v0 = number of coins.
        20 => {
            data.item_type = ItemType::Gold;
            data.value = v[0].max(0);
        }
        // ITEM_PEN — writing-tool with no IronMUD analogue.
        21 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                "ITEM_PEN (writing tool) not modeled; imported as Misc".to_string(),
            ));
        }
        // ITEM_BOAT — Misc + IronMUD's flags.boat which already exists.
        22 => {
            data.item_type = ItemType::Misc;
            data.flags.boat = true;
        }
        // ITEM_FOUNTAIN — same shape as DRINKCON; the infinite-fill behaviour
        // is what differs in stock Circle.
        23 => {
            data.item_type = ItemType::LiquidContainer;
            data.liquid_max = v[0].max(0);
            data.liquid_current = v[1].max(0);
            let (lt, info) = circle_liquid_index_to_type(v[2]);
            data.liquid_type = lt;
            data.liquid_poisoned = v[3] != 0;
            if let Some(msg) = info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    item.source.clone(),
                    msg,
                ));
            }
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                "ITEM_FOUNTAIN infinite-fill behaviour not modeled (treated as a finite drink container)"
                    .to_string(),
            ));
        }
        _ => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!("unknown CircleMUD item_type {} ({type_name}); imported as Misc", item.item_type),
            ));
        }
    }
}

/// Map CircleMUD's WEAPON v3 (damage-message verb index) to an IronMUD
/// `DamageType`. The choice of bucket is the one that best matches the
/// English verb; a few are lossy (notably `blast` → Lightning).
fn circle_weapon_damage_type(v3: i32) -> DamageType {
    match v3 {
        0 | 5 | 6 | 7 | 9 | 10 | 13 => DamageType::Bludgeoning, // hit, bludgeon, crush, pound, maul, thrash, punch
        2 | 3 | 8 => DamageType::Slashing,                       // whip, slash, claw
        1 | 11 | 14 => DamageType::Piercing,                     // sting, pierce, stab
        4 => DamageType::Bite,                                    // bite
        12 => DamageType::Lightning,                              // blast (lossy)
        _ => DamageType::Bludgeoning,
    }
}

/// CircleMUD `LIQ_*` index → IronMUD `LiquidType`. Returns an Info message
/// when the source liquid has no exact equivalent and we picked the closest
/// IronMUD bucket (e.g. "dark ale" → Ale).
fn circle_liquid_index_to_type(idx: i32) -> (LiquidType, Option<String>) {
    match idx {
        0 => (LiquidType::Water, None),
        1 => (LiquidType::Beer, None),
        2 => (LiquidType::Wine, None),
        3 => (LiquidType::Ale, None),
        4 => (LiquidType::Ale, Some("Circle 'dark ale' folded into Ale (no distinct IronMUD type)".into())),
        5 => (LiquidType::Spirits, None),
        6 => (LiquidType::Juice, Some("Circle 'lemonade' folded into Juice".into())),
        7 => (LiquidType::Spirits, Some("Circle 'firebreather' folded into Spirits".into())),
        8 => (LiquidType::Ale, Some("Circle 'local speciality' folded into Ale".into())),
        9 => (LiquidType::Juice, Some("Circle 'slime mold juice' folded into Juice".into())),
        10 => (LiquidType::Milk, None),
        11 => (LiquidType::Tea, None),
        12 => (LiquidType::Coffee, None),
        13 => (LiquidType::Blood, None),
        14 => (LiquidType::Water, Some("Circle 'salt water' folded into Water".into())),
        15 => (LiquidType::Water, None),
        _ => (
            LiquidType::Water,
            Some(format!("unknown Circle liquid index {idx}; defaulted to Water")),
        ),
    }
}

/// Set an ItemFlags bool by snake_case name. Returns false if the name isn't
/// a known flag — surfaces typos in the mapping JSON.
fn apply_named_item_flag(flags: &mut ItemFlags, name: &str) -> bool {
    match name {
        "no_drop" => flags.no_drop = true,
        "no_get" => flags.no_get = true,
        "no_remove" => flags.no_remove = true,
        "invisible" => flags.invisible = true,
        "glow" => flags.glow = true,
        "hum" => flags.hum = true,
        "no_sell" => flags.no_sell = true,
        "unique" => flags.unique = true,
        "quest_item" => flags.quest_item = true,
        "provides_light" => flags.provides_light = true,
        "boat" => flags.boat = true,
        "waterproof" => flags.waterproof = true,
        _ => return false,
    }
    true
}

/// Set an ItemData stat-bonus field by snake_case name. Returns false if the
/// name isn't recognised.
fn apply_named_item_stat(data: &mut ItemData, name: &str, modifier: i32) -> bool {
    match name {
        "stat_str" => data.stat_str += modifier,
        "stat_dex" => data.stat_dex += modifier,
        "stat_con" => data.stat_con += modifier,
        "stat_int" => data.stat_int += modifier,
        "stat_wis" => data.stat_wis += modifier,
        "stat_cha" => data.stat_cha += modifier,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mapping_parses() {
        let _ = CircleMappingTable::load_default();
    }

    #[test]
    fn slug_basic() {
        assert_eq!(slug("Haon-Dor, Dark Forest"), "haon_dor_dark_forest");
        assert_eq!(slug("  Mid-Gaard  "), "mid_gaard");
        assert_eq!(slug(""), "");
    }

    #[test]
    fn unique_prefix_disambiguates_with_vnum() {
        let taken = vec!["forest".to_string()];
        assert_eq!(unique_prefix("Forest", 12, &taken), "forest_12");
    }

    #[test]
    fn dice_max_parses_circle_dice() {
        assert_eq!(dice_max("5d10+550"), Some(600));
        assert_eq!(dice_max("1d1+30000"), Some(30001));
        assert_eq!(dice_max("2d6"), Some(12));
        assert_eq!(dice_max("4d6-3"), Some(21));
        assert_eq!(dice_max("garbage"), None);
    }
}
