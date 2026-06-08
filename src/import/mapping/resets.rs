use std::collections::{HashMap, HashSet};

use crate::import::{
    DeferredItem, IrResetKind, IrZone, PlannedRoom, PlannedSpawn, PlannedSpawnDep, Severity, SourceLoc, Warning,
    WarningKind,
};
use crate::types::{ItemType, SpawnDestination, SpawnEntityType};

const RESET_DIRECTIONS: &[&str] = &["north", "east", "south", "west", "up", "down"];

/// Door state change requested by a CircleMUD `D` reset. Applied after
/// all reset translation by mutating an existing [`PlannedDoor`] on the
/// matching planned room. If the room exists but has no door on that
/// direction, surfaces a warning rather than fabricating one.
#[derive(Debug, Clone)]
pub(super) struct DoorOverride {
    room_source_vnum: i32,
    direction: String,
    is_closed: bool,
    is_locked: bool,
    source: SourceLoc,
}

pub(super) fn apply_door_override(
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

/// Translate a single zone's CircleMUD reset commands into [`PlannedSpawn`]s
/// + [`DoorOverride`]s + warnings for anything we can't model.
///
/// Anchor tracking: G/E with `if=1` chain onto the most-recent translated M;
/// P with `if=1` chains onto the most-recent translated O *if* that O loaded
/// a Container item. The runtime "did the M actually spawn this tick?" check
/// is intentionally not modelled — that's the spawn tick's job
/// (`src/ticks/spawn.rs`); we treat any translated parent as a live anchor.
pub(super) fn map_resets(
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
    // Cross-block P chaining: vnum → spawn index of the most-recently-loaded
    // O reset for a Container of that vnum in this zone. Falls back here when
    // an intervening M/non-container O has cleared `last_obj_idx`. Most
    // recent wins, matching Circle's loader stack semantics.
    let mut container_idx_by_vnum: HashMap<i32, usize> = HashMap::new();
    // Track NECK_1+NECK_2 collision per-mob (warn-once).
    let mut neck_warned_for: HashSet<usize> = HashSet::new();
    // Track which mob spawn points already had a slot used to detect
    // duplicate-equip-on-same-slot (Circle's two-neck case).
    let mut used_neck_slot: HashMap<usize, bool> = HashMap::new();

    for reset in &zone.resets {
        match &reset.kind {
            IrResetKind::LoadMob { vnum, max, room_vnum } => {
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
            IrResetKind::LoadObj { vnum, max, room_vnum } => {
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
                    Some(ItemType::Container) => {
                        // Stamp into the cross-block lookup too so a later
                        // P after an intervening M/non-container O can still
                        // reach this container by vnum.
                        container_idx_by_vnum.insert(*vnum, spawns.len() - 1);
                        Some(spawns.len() - 1)
                    }
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
                let Some(loc) = crate::import::engines::circle::wear::map_wear_loc(*wear_loc) else {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedValueSemantic,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("E reset wear-slot {wear_loc} for obj #{vnum} has no IronMUD analogue — drop"),
                    ));
                    continue;
                };
                // NECK_1 + NECK_2 collision: both map to Neck, second is dropped per mob.
                if crate::import::engines::circle::wear::is_neck_slot(*wear_loc) {
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
                if crate::import::engines::circle::wear::is_paired_slot_collapse(*wear_loc) {
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
                // Resolve the parent container spawn point. Try the
                // immediately-prior O first (cheap, common path); if that
                // doesn't match the named container, fall back to the
                // per-zone vnum→spawn-index map populated by earlier
                // Container O resets. The fallback is what makes
                // cross-block chains (P after an intervening M / non-
                // container O) attach instead of dropping.
                let parent_container_pref = item_index.get(container_vnum);
                let mut parent_idx_opt: Option<usize> = None;
                let mut cross_block = false;
                if let Some(idx) = last_obj_idx {
                    let parent_vnum = &spawns[idx].vnum;
                    if parent_container_pref.map(|p| p == parent_vnum).unwrap_or(false) {
                        parent_idx_opt = Some(idx);
                    }
                }
                if parent_idx_opt.is_none() {
                    if let Some(&idx) = container_idx_by_vnum.get(container_vnum) {
                        parent_idx_opt = Some(idx);
                        cross_block = true;
                    }
                }
                let Some(parent_idx) = parent_idx_opt else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!(
                            "P reset for obj #{vnum} into container #{container_vnum} — no Container O for that vnum in this zone — drop"
                        ),
                    ));
                    continue;
                };
                let Some(item_pref) = item_index.get(vnum) else {
                    warnings.push(Warning::new(
                        WarningKind::DeferredFeature,
                        Severity::Warn,
                        reset.source.clone(),
                        format!("P reset references obj #{vnum} not in import set — drop"),
                    ));
                    continue;
                };
                if cross_block {
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        reset.source.clone(),
                        format!(
                            "P reset for obj #{vnum} into container #{container_vnum} resolved cross-block (intervening M/non-container O cleared the immediate anchor)"
                        ),
                    ));
                }
                spawns[parent_idx].dependencies.push(PlannedSpawnDep {
                    item_vnum: item_pref.clone(),
                    destination: SpawnDestination::Container,
                    count: 1,
                });
            }
            IrResetKind::SetDoor { room_vnum, dir, state } => {
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
                            format!("D reset on room #{room_vnum} has unsupported state {state} (only 0/1/2) — drop"),
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
                // CircleMUD `R` exists to dedupe room contents across resets;
                // IronMUD's spawn tick + area reset already cap by (room, vnum)
                // via `max_count`, so `R` is redundant. Keep an Info note in
                // the report so importers can still see it was authored.
                warnings.push(Warning::new(
                    WarningKind::DeferredFeature,
                    Severity::Info,
                    reset.source.clone(),
                    format!(
                        "R reset (remove obj #{vnum} from room #{room_vnum}) skipped — superseded by per-(room,vnum) dedupe in spawn tick"
                    ),
                ));
            }
        }
    }
    (spawns, doors, warnings)
}

pub(super) fn deferred_to_warning(d: &DeferredItem) -> Warning {
    Warning::new(
        WarningKind::DeferredFeature,
        Severity::Warn,
        d.source.clone(),
        format!("[{}] {}", d.category, d.summary),
    )
    .with_suggestion("translate manually to a spawn point / trigger after import (zone resets are not applied)")
}
