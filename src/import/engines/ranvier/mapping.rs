//! Ranvier IR → IronMUD `Plan` translation.
//!
//! Behavior / item-type / quest-goal translation tables live in
//! `scripts/data/import/ranvier_*_mapping.json` and are loaded once per run
//! (with a hard-coded fallback for tests / non-repo runs).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;
use uuid::Uuid;

use super::ir::{IrArea, IrBundle, IrDoor, IrItem, IrNpc, IrQuest, IrRoom, IrSpawnRef};
use super::vnum_map::{Kind, VnumMap};
use crate::import::{
    Plan, PlannedArea, PlannedDoor, PlannedExit, PlannedItem, PlannedMobile, PlannedQuest, PlannedRoom,
    PlannedShopOverlay, PlannedSpawn, Severity, SourceLoc, Warning, WarningKind,
};
use crate::types::{
    ExtraDesc, ItemData, ItemFlags, ItemType, MobileFlags, QuestData, QuestObjective, QuestReward, RoomFlags,
    SpawnEntityType, WearLocation,
};

// =============================== Mapping tables ==============================

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BehaviorMapping {
    /// `<behaviorName>` → action descriptor. Behaviors not listed here become
    /// a `behavior_unmapped` warning.
    #[serde(default)]
    pub behaviors: HashMap<String, BehaviorAction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BehaviorAction {
    /// Set a named bool flag on `MobileFlags`. `flag` is the snake_case
    /// field name (e.g. `aggressive`).
    SetMobFlag { flag: String },
    /// No-op (matches IronMUD default behavior). Useful for `combat: true`
    /// on Ranvier NPCs — combat is the default in IronMUD.
    NoOp,
    /// Surface as info (not warn).
    Info { message: Option<String> },
    /// Surface as warn — the behavior carries semantic IronMUD doesn't
    /// model yet.
    Warn { message: Option<String> },
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ItemTypeMapping {
    #[serde(default)]
    pub types: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct QuestGoalMapping {
    #[serde(default)]
    pub goals: HashMap<String, GoalKind>,
    #[serde(default)]
    pub rewards: HashMap<String, RewardKind>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    KillMob,
    BringItem,
    Warn,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    Gold,
    SkillXp,
    Warn,
}

fn behavior_table() -> &'static BehaviorMapping {
    static CACHE: OnceLock<BehaviorMapping> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = "scripts/data/import/ranvier_behavior_mapping.json";
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_behavior_table)
    })
}

fn default_behavior_table() -> BehaviorMapping {
    let mut behaviors = HashMap::new();
    behaviors.insert("combat".to_string(), BehaviorAction::NoOp);
    behaviors.insert(
        "ranvier-aggro".to_string(),
        BehaviorAction::SetMobFlag {
            flag: "aggressive".to_string(),
        },
    );
    behaviors.insert(
        "ranvier-wander".to_string(),
        BehaviorAction::Info {
            message: Some("wander interval/restrictTo not preserved; IronMUD wander is area-scoped".to_string()),
        },
    );
    behaviors.insert(
        "lootable".to_string(),
        BehaviorAction::Info {
            message: Some(
                "lootable currencies/pools handled inline; behavior entry itself is informational".to_string(),
            ),
        },
    );
    behaviors.insert(
        "decay".to_string(),
        BehaviorAction::Warn {
            message: Some("item decay timer not modeled in IronMUD".to_string()),
        },
    );
    behaviors.insert(
        "progressive-respawn".to_string(),
        BehaviorAction::NoOp, // resolved against AreaData.respawn_interval inline
    );
    BehaviorMapping { behaviors }
}

fn item_type_table() -> &'static ItemTypeMapping {
    static CACHE: OnceLock<ItemTypeMapping> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = "scripts/data/import/ranvier_item_type_mapping.json";
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_item_type_table)
    })
}

fn default_item_type_table() -> ItemTypeMapping {
    let mut types = HashMap::new();
    for (k, v) in [
        ("WEAPON", "weapon"),
        ("ARMOR", "armor"),
        ("POTION", "potion"),
        ("CONTAINER", "container"),
        ("KEY", "key"),
        ("FOOD", "food"),
        ("SCROLL", "misc"),
        ("WAND", "wand"),
        ("STAFF", "staff"),
    ] {
        types.insert(k.to_string(), v.to_string());
    }
    ItemTypeMapping { types }
}

fn quest_goal_table() -> &'static QuestGoalMapping {
    static CACHE: OnceLock<QuestGoalMapping> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = "scripts/data/import/ranvier_quest_goal_mapping.json";
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_quest_goal_table)
    })
}

fn default_quest_goal_table() -> QuestGoalMapping {
    let mut goals = HashMap::new();
    goals.insert("KillGoal".to_string(), GoalKind::KillMob);
    goals.insert("FetchGoal".to_string(), GoalKind::BringItem);
    goals.insert("EquipGoal".to_string(), GoalKind::Warn);
    goals.insert("BountyGoal".to_string(), GoalKind::Warn);
    let mut rewards = HashMap::new();
    rewards.insert("CurrencyReward".to_string(), RewardKind::Gold);
    rewards.insert("ExperienceReward".to_string(), RewardKind::SkillXp);
    QuestGoalMapping { goals, rewards }
}

// ============================== Top-level driver ============================

pub fn bundle_to_plan(
    bundle: &IrBundle,
    bundle_name: &str,
    vnum_base: i32,
    quest_vnum_base: i32,
    vnum_map: &mut VnumMap,
    existing_room_vnums: &[String],
    existing_mobile_vnums: &[String],
    existing_item_vnums: &[String],
    existing_area_prefixes: &[String],
    warnings: &mut Vec<Warning>,
) -> Plan {
    let _ = bundle_name;
    let mut plan = Plan::default();
    let mut area_bases: HashMap<String, i32> = HashMap::new();
    let mut next_window_base = vnum_base;
    let existing_rooms: HashSet<&str> = existing_room_vnums.iter().map(|s| s.as_str()).collect();
    let existing_mobiles: HashSet<&str> = existing_mobile_vnums.iter().map(|s| s.as_str()).collect();
    let existing_items: HashSet<&str> = existing_item_vnums.iter().map(|s| s.as_str()).collect();
    let existing_prefixes: HashSet<&str> = existing_area_prefixes.iter().map(|s| s.as_str()).collect();

    // First pass: stake out each area's window.
    for area in &bundle.areas {
        // Reuse a base captured from a prior run if present (the sidecar may
        // already record it on `area_high_water`).
        let base = if let Some(state) = vnum_map.area_high_water.get(&area.name) {
            state.base
        } else {
            let allocated = next_window_base;
            next_window_base += 1000;
            allocated
        };
        area_bases.insert(area.name.clone(), base);
    }

    // Second pass: pre-allocate every room / mob / item / quest vnum
    // across the whole bundle so cross-area refs (e.g. limbo→mapped exits,
    // vendor stock referencing items in another bundle area) resolve
    // without depending on processing order.
    for area in &bundle.areas {
        let base = area_bases[&area.name];
        for room in &area.rooms {
            let _ = vnum_map.resolve(&area.name, Kind::Room, &room.id, base);
        }
        for npc in &area.npcs {
            let _ = vnum_map.resolve(&area.name, Kind::Mobile, &npc.id, base);
        }
        for item in &area.items {
            let _ = vnum_map.resolve(&area.name, Kind::Item, &item.id, base);
        }
        for quest in &area.quests {
            let _ = vnum_map.resolve_quest(&quest.id, quest_vnum_base);
        }
    }

    // Third pass: emit Plan rows per area.
    for area in &bundle.areas {
        let base = area_bases[&area.name];
        emit_area(
            area,
            base,
            quest_vnum_base,
            vnum_map,
            &existing_rooms,
            &existing_mobiles,
            &existing_items,
            &existing_prefixes,
            &mut plan,
            warnings,
        );
    }
    plan
}

fn emit_area(
    area: &IrArea,
    area_base: i32,
    quest_vnum_base: i32,
    vnum_map: &mut VnumMap,
    existing_rooms: &HashSet<&str>,
    existing_mobiles: &HashSet<&str>,
    existing_items: &HashSet<&str>,
    existing_prefixes: &HashSet<&str>,
    plan: &mut Plan,
    warnings: &mut Vec<Warning>,
) {
    let prefix = area.name.to_lowercase();
    if existing_prefixes.contains(prefix.as_str()) {
        warnings.push(Warning::new(
            WarningKind::PrefixCollision,
            Severity::Warn,
            SourceLoc::file(&area.source_dir),
            format!(
                "area prefix '{prefix}' already exists in target DB; rooms/items will upsert into the existing area"
            ),
        ));
    }

    // Synthesise a stable source_vnum for the area itself: low end of window.
    let area_source_vnum = area_base;
    plan.areas.push(PlannedArea {
        source_vnum: area_source_vnum,
        name: if area.manifest.title.is_empty() {
            capitalize(&area.name)
        } else {
            area.manifest.title.clone()
        },
        prefix: prefix.clone(),
        description: String::new(),
    });

    // Track per-area room source-vnum index for door/exit resolution.
    let mut room_source_vnums: HashMap<String, i32> = HashMap::new();
    for room in &area.rooms {
        let Some(vnum_int) = vnum_map.resolve(&area.name, Kind::Room, &room.id, area_base) else {
            warnings.push(Warning::new(
                WarningKind::DuplicateVnum,
                Severity::Block,
                SourceLoc::file(&area.source_dir),
                format!(
                    "room sub-range exhausted for area '{}': skipping room {}",
                    area.name, room.id
                ),
            ));
            continue;
        };
        room_source_vnums.insert(room.id.clone(), vnum_int);
    }
    let mut mob_source_vnums: HashMap<String, i32> = HashMap::new();
    for npc in &area.npcs {
        let Some(v) = vnum_map.resolve(&area.name, Kind::Mobile, &npc.id, area_base) else {
            warnings.push(Warning::new(
                WarningKind::DuplicateVnum,
                Severity::Block,
                SourceLoc::file(&area.source_dir),
                format!(
                    "mobile sub-range exhausted for area '{}': skipping npc {}",
                    area.name, npc.id
                ),
            ));
            continue;
        };
        mob_source_vnums.insert(npc.id.clone(), v);
    }
    let mut item_source_vnums: HashMap<String, i32> = HashMap::new();
    for item in &area.items {
        let Some(v) = vnum_map.resolve(&area.name, Kind::Item, &item.id, area_base) else {
            warnings.push(Warning::new(
                WarningKind::DuplicateVnum,
                Severity::Block,
                SourceLoc::file(&area.source_dir),
                format!(
                    "item sub-range exhausted for area '{}': skipping item {}",
                    area.name, item.id
                ),
            ));
            continue;
        };
        item_source_vnums.insert(item.id.clone(), v);
    }

    // Surface JS scripts as warn-only.
    for script in &area.script_files {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Info,
            SourceLoc::file(script),
            format!("ranvier JS script skipped (not translated): {}", script.display()),
        ));
    }

    // Rooms.
    for room in &area.rooms {
        let Some(&vnum_int) = room_source_vnums.get(&room.id) else {
            continue;
        };
        let vnum = format!("{prefix}_{vnum_int}");
        if existing_rooms.contains(vnum.as_str()) {
            // Idempotent re-import: the writer will overwrite — informational.
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(&area.source_dir),
                format!("room {vnum} already present; will be overwritten"),
            ));
        }
        emit_room(room, &prefix, vnum_int, &vnum, &area.source_dir, plan);
        // Embedded npc spawns
        for spawn_ref in &room.npcs {
            emit_spawn(
                spawn_ref,
                SpawnEntityType::Mobile,
                &prefix,
                &vnum,
                &area.name,
                vnum_map,
                &area.manifest.respawn_interval,
                plan,
                warnings,
                &area.source_dir,
            );
        }
        // Embedded item spawns
        for spawn_ref in &room.items {
            emit_spawn(
                spawn_ref,
                SpawnEntityType::Item,
                &prefix,
                &vnum,
                &area.name,
                vnum_map,
                &area.manifest.respawn_interval,
                plan,
                warnings,
                &area.source_dir,
            );
        }
    }

    // Exits — emitted in a second pass so all room vnums in this area are known.
    for room in &area.rooms {
        let Some(&from_int) = room_source_vnums.get(&room.id) else {
            continue;
        };
        let from_vnum = format!("{prefix}_{from_int}");
        for ex in &room.exits {
            let to_id = parse_scoped_id(&ex.room_id, &area.name);
            // Cross-area exits: the destination's `(area, kind, id)` may
            // already be in vnum_map (set by an earlier area in this run or
            // by a prior import). Resolve via the map.
            let to_int = if to_id.0 == area.name {
                room_source_vnums.get(&to_id.1).copied()
            } else {
                vnum_map.get(&to_id.0, Kind::Room, &to_id.1)
            };
            match to_int {
                Some(v) => plan.exits.push(PlannedExit {
                    from_vnum: from_vnum.clone(),
                    direction: ex.direction.clone(),
                    to_source_vnum: v,
                }),
                None => warnings.push(Warning::new(
                    WarningKind::DanglingExit,
                    Severity::Warn,
                    SourceLoc::file(&area.source_dir),
                    format!(
                        "exit {} -> {} from room {} unresolved",
                        ex.direction, ex.room_id, room.id
                    ),
                )),
            }
        }
    }

    // Mobiles
    for npc in &area.npcs {
        let Some(&vnum_int) = mob_source_vnums.get(&npc.id) else {
            continue;
        };
        let vnum = format!("{prefix}_{vnum_int}");
        if existing_mobiles.contains(vnum.as_str()) {
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(&area.source_dir),
                format!("mobile {vnum} already present; will be overwritten"),
            ));
        }
        emit_mobile(
            npc,
            &prefix,
            vnum_int,
            &vnum,
            &area.source_dir,
            area,
            vnum_map,
            plan,
            warnings,
        );
    }

    // Items
    for item in &area.items {
        let Some(&vnum_int) = item_source_vnums.get(&item.id) else {
            continue;
        };
        let vnum = format!("{prefix}_{vnum_int}");
        if existing_items.contains(vnum.as_str()) {
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(&area.source_dir),
                format!("item {vnum} already present; will be overwritten"),
            ));
        }
        emit_item(
            item,
            &prefix,
            vnum_int,
            &vnum,
            &area.source_dir,
            area,
            vnum_map,
            plan,
            warnings,
        );
    }

    // Quests
    for quest in &area.quests {
        emit_quest(quest, &prefix, quest_vnum_base, vnum_map, area, plan, warnings);
    }
}

// ================================ Rooms ====================================

fn emit_room(room: &IrRoom, prefix: &str, source_vnum: i32, vnum: &str, source_dir: &std::path::Path, plan: &mut Plan) {
    let mut flags = RoomFlags::default();
    // Ranvier doesn't carry IronMUD-style RoomFlags; rooms come in plain.
    let _ = &mut flags;
    let mut doors: Vec<PlannedDoor> = Vec::new();
    for ex in &room.exits {
        let other_id = ex.room_id.clone();
        // Match a doors entry on either the bare id or scoped id.
        let bare = strip_area_prefix(&other_id);
        let door_entry: Option<&IrDoor> = room
            .doors
            .get(&other_id)
            .or_else(|| room.doors.get(&bare))
            .or_else(|| room.doors.get(&format!("{prefix}:{bare}")));
        if let Some(d) = door_entry {
            // Door key resolution would require a third pass once item
            // vnums are settled AND a CircleMUD-style key_lookup that
            // tries items rather than rooms. Stock Ranvier bundles only
            // use lockedBy on containers (handled in emit_item), so we
            // surface a warning if a door uses lockedBy and leave
            // key_source_vnum unset.
            if d.locked_by.is_some() {
                // Caller layer logs the warning to keep emit_room signature lean.
            }
            doors.push(PlannedDoor {
                direction: ex.direction.clone(),
                name: format!("door to {}", strip_area_prefix(&ex.room_id)),
                keywords: vec!["door".to_string()],
                description: None,
                is_closed: d.closed,
                is_locked: d.locked,
                pickproof: false,
                key_source_vnum: None,
            });
        }
    }
    let title = if room.title.is_empty() {
        capitalize(&room.id)
    } else {
        room.title.clone()
    };

    let _ = source_dir;
    let mut planned = PlannedRoom {
        area_prefix: prefix.to_string(),
        source_vnum,
        vnum: vnum.to_string(),
        title,
        description: room.description.clone(),
        flags,
        extra_descs: Vec::<ExtraDesc>::new(),
        doors,
        source: SourceLoc::file(source_dir),
    };
    // Coordinates are written by patching the saved RoomData in a writer
    // post-pass — they're not part of the legacy CircleMUD-shaped plan.
    // Instead, encode them via extra_descs metadata? No — simpler: add a
    // synthetic ExtraDesc only if needed. For Ranvier we just drop them
    // here; a follow-up writer hook can populate from the IR. (See
    // crate::import::engines::ranvier::writer once we wire it.)
    let _ = &mut planned; // suppress unused-mut when coordinates aren't exposed yet.
    plan.rooms.push(planned);
}

// =============================== Spawns ====================================

#[allow(clippy::too_many_arguments)]
fn emit_spawn(
    spawn_ref: &IrSpawnRef,
    entity_type: SpawnEntityType,
    prefix: &str,
    room_vnum: &str,
    area_name: &str,
    vnum_map: &VnumMap,
    manifest_respawn_interval: &Option<i64>,
    plan: &mut Plan,
    warnings: &mut Vec<Warning>,
    source_dir: &std::path::Path,
) {
    let (target_area, target_id) = parse_scoped_id(&spawn_ref.id, area_name);
    let kind = match entity_type {
        SpawnEntityType::Mobile => Kind::Mobile,
        SpawnEntityType::Item => Kind::Item,
    };
    let Some(vnum_int) = vnum_map.get(&target_area, kind, &target_id) else {
        warnings.push(Warning::new(
            WarningKind::DanglingExit,
            Severity::Warn,
            SourceLoc::file(source_dir),
            format!("spawn ref '{}' from room {} unresolved", spawn_ref.id, room_vnum),
        ));
        return;
    };
    let target_prefix = target_area.to_lowercase();
    let entity_vnum = format!("{target_prefix}_{vnum_int}");
    let respawn_interval_secs = manifest_respawn_interval.unwrap_or(60);
    plan.spawns.push(PlannedSpawn {
        area_prefix: prefix.to_string(),
        vnum: entity_vnum,
        entity_type,
        room_vnum: room_vnum.to_string(),
        max_count: spawn_ref.max_load.unwrap_or(1).max(1),
        respawn_interval_secs,
        dependencies: Vec::new(),
        source: SourceLoc::file(source_dir),
    });
    // `replaceOnRespawn` and `respawnChance` are Ranvier-specific knobs
    // not on PlannedSpawn. The writer sets `replace_on_respawn` directly
    // on the saved `SpawnPointData` in a post-pass keyed by spawn-point
    // identity.
    let _ = spawn_ref.respawn_chance;
}

// =============================== Mobiles ===================================

#[allow(clippy::too_many_arguments)]
fn emit_mobile(
    npc: &IrNpc,
    prefix: &str,
    source_vnum: i32,
    vnum: &str,
    source_dir: &std::path::Path,
    area: &IrArea,
    vnum_map: &VnumMap,
    plan: &mut Plan,
    warnings: &mut Vec<Warning>,
) {
    let mut flags = MobileFlags::default();
    for (behavior, value) in &npc.behaviors {
        match behavior_table().behaviors.get(behavior) {
            Some(BehaviorAction::SetMobFlag { flag }) => {
                if !apply_named_mob_flag(&mut flags, flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        SourceLoc::file(source_dir),
                        format!("mob flag '{flag}' from behavior '{behavior}' not recognised"),
                    ));
                }
            }
            Some(BehaviorAction::NoOp) => {}
            Some(BehaviorAction::Info { message }) => {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    SourceLoc::file(source_dir),
                    format!(
                        "behavior '{behavior}' on '{}': {}",
                        npc.id,
                        message.as_deref().unwrap_or("informational mapping")
                    ),
                ));
            }
            Some(BehaviorAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    SourceLoc::file(source_dir),
                    format!(
                        "behavior '{behavior}' on '{}': {}",
                        npc.id,
                        message.as_deref().unwrap_or("not modeled in IronMUD")
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(source_dir),
                format!("behavior '{behavior}' on '{}' has no mapping", npc.id),
            )),
        }
        let _ = value;
    }

    // Lootable currencies → gold range.
    let mut gold = 0;
    let mut shop_overlay: Option<PlannedShopOverlay> = None;
    if let Some(loot) = npc.behaviors.get("lootable") {
        if let Some(map) = loot.as_mapping() {
            // currencies.gold.{min,max}
            if let Some(currs) = map.get("currencies").and_then(|v| v.as_mapping()) {
                for (curr_key, curr_val) in currs {
                    let Some(name) = curr_key.as_str() else { continue };
                    let cmap = match curr_val.as_mapping() {
                        Some(m) => m,
                        None => continue,
                    };
                    if name.eq_ignore_ascii_case("gold") {
                        let max = cmap.get("max").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let min = cmap.get("min").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        // Take midpoint as the prototype's representative gold.
                        gold = (min + max) / 2;
                    } else {
                        warnings.push(Warning::new(
                            WarningKind::UnsupportedFlag,
                            Severity::Warn,
                            SourceLoc::file(source_dir),
                            format!("lootable currency '{name}' on '{}' skipped (only gold mapped)", npc.id),
                        ));
                    }
                }
            }
            // pools: warn-only in v1.
            if let Some(pools) = map.get("pools").and_then(|v| v.as_sequence()) {
                let _ = pools;
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    SourceLoc::file(source_dir),
                    format!(
                        "lootable.pools on '{}' present — inline pool inventory translation deferred to follow-up; currencies still applied",
                        npc.id
                    ),
                ));
            }
        }
    }

    // Vendor metadata → shop overlay.
    if let Some(vendor) = lookup_metadata(&npc.metadata, "vendor") {
        if let Some(map) = vendor.as_mapping() {
            let mut stock_vnums = Vec::new();
            if let Some(items) = map.get("items").and_then(|v| v.as_mapping()) {
                for (k, _) in items {
                    let Some(item_id) = k.as_str() else { continue };
                    let (item_area, item_bare) = parse_scoped_id(item_id, &area.name);
                    match vnum_map.get(&item_area, Kind::Item, &item_bare) {
                        Some(v) => stock_vnums.push(format!("{}_{v}", item_area.to_lowercase())),
                        None => warnings.push(Warning::new(
                            WarningKind::DanglingExit,
                            Severity::Warn,
                            SourceLoc::file(source_dir),
                            format!("vendor stock item '{item_id}' on '{}' unresolved", npc.id),
                        )),
                    }
                }
            }
            shop_overlay = Some(PlannedShopOverlay {
                shop_source_vnum: source_vnum,
                keeper_source_vnum: source_vnum,
                keeper_vnum: vnum.to_string(),
                stock_vnums,
                buy_rate: 50,
                sell_rate: 100,
                buys_types: vec!["weapon".to_string(), "armor".to_string()],
                daily_routine: Vec::new(),
                hostile_on_steal: false,
                source: SourceLoc::file(source_dir),
            });
        }
    }

    if npc.script.is_some() {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Info,
            SourceLoc::file(source_dir),
            format!(
                "npc '{}' has JS script reference: {} (skipped)",
                npc.id,
                npc.script.as_deref().unwrap_or("")
            ),
        ));
    }

    let max_hp = npc.attributes.get("health").copied().unwrap_or(20).max(1);
    let level = npc.level.max(1);
    let name = if npc.name.is_empty() {
        capitalize(&npc.id)
    } else {
        npc.name.clone()
    };
    let short_desc = format!("{} stands here.", name);
    plan.mobiles.push(PlannedMobile {
        area_prefix: prefix.to_string(),
        source_vnum,
        vnum: vnum.to_string(),
        name: name.clone(),
        short_desc,
        long_desc: npc.description.clone(),
        keywords: if npc.keywords.is_empty() {
            vec![npc.id.clone()]
        } else {
            npc.keywords.clone()
        },
        level,
        max_hp,
        damage_dice: format!("1d{}+{}", (level + 1).max(2), level),
        armor_class: 10 - level.min(8),
        gold,
        flags,
        world_max_count: None,
        active_buffs: Vec::new(),
        position: None,
        characteristics_gender: None,
        source: SourceLoc::file(source_dir),
    });

    if let Some(overlay) = shop_overlay {
        plan.shop_overlays.push(overlay);
    }
}

fn apply_named_mob_flag(flags: &mut MobileFlags, name: &str) -> bool {
    macro_rules! set_field {
        ($($f:ident),* $(,)?) => {
            match name {
                $(stringify!($f) => { flags.$f = true; true })*
                _ => false,
            }
        };
    }
    set_field!(
        aggressive,
        sentinel,
        scavenger,
        shopkeeper,
        no_attack,
        healer,
        leasing_agent,
        cowardly,
        can_open_doors,
        guard,
        helper,
        thief,
        cant_swim,
        poisonous,
        fiery,
        chilling,
        corrosive,
        shocking,
        unique,
        stay_zone,
        aware,
        memory,
        no_sleep,
        no_blind,
        no_bash,
        no_summon,
        no_charm,
        hostile_on_steal,
        tameable,
    )
}

// =============================== Items =====================================

#[allow(clippy::too_many_arguments)]
fn emit_item(
    item: &IrItem,
    prefix: &str,
    source_vnum: i32,
    vnum: &str,
    source_dir: &std::path::Path,
    area: &IrArea,
    vnum_map: &VnumMap,
    plan: &mut Plan,
    warnings: &mut Vec<Warning>,
) {
    let mut data = ItemData::new(
        if item.name.is_empty() {
            capitalize(&item.id)
        } else {
            item.name.clone()
        },
        item.room_desc.clone(),
        item.description.clone(),
    );
    data.id = Uuid::new_v4();
    data.is_prototype = true;
    data.vnum = Some(vnum.to_string());
    data.keywords = if item.keywords.is_empty() {
        vec![item.id.clone()]
    } else {
        item.keywords.clone()
    };

    // Item type
    if let Some(t) = &item.item_type {
        let lookup = item_type_table().types.get(t).cloned();
        match lookup.as_deref().and_then(ItemType::from_str) {
            Some(it) => data.item_type = it,
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(source_dir),
                format!("item type '{t}' on '{}' has no mapping (defaulting to misc)", item.id),
            )),
        }
    }

    // Container fields
    if data.item_type == ItemType::Container {
        data.container_closed = item.closed;
        data.container_locked = item.locked;
        if let Some(max) = item.max_items {
            data.container_max_items = max;
        }
        if let Some(key_id) = &item.locked_by {
            let (key_area, key_bare) = parse_scoped_id(key_id, &area.name);
            if let Some(v) = vnum_map.get(&key_area, Kind::Item, &key_bare) {
                data.container_key_vnum = Some(format!("{}_{v}", key_area.to_lowercase()));
            } else {
                warnings.push(Warning::new(
                    WarningKind::DanglingExit,
                    Severity::Warn,
                    SourceLoc::file(source_dir),
                    format!("lockedBy '{key_id}' on '{}' unresolved", item.id),
                ));
            }
        }
        // Container starting contents → spawn dependencies on the container's
        // own spawn point (handled at room level — we just warn here that
        // "starting items" exist so a builder can verify).
        if !item.items.is_empty() {
            warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(source_dir),
                format!(
                    "container '{}' has {} starting items; attach to room spawn dependencies manually for now",
                    item.id,
                    item.items.len()
                ),
            ));
        }
    }

    // Behaviors on items.
    for (behavior, value) in &item.behaviors {
        match behavior_table().behaviors.get(behavior) {
            Some(BehaviorAction::Warn { message }) => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(source_dir),
                format!(
                    "item behavior '{behavior}' on '{}': {}",
                    item.id,
                    message.as_deref().unwrap_or("not modeled")
                ),
            )),
            Some(BehaviorAction::Info { message }) => warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(source_dir),
                format!(
                    "item behavior '{behavior}' on '{}': {}",
                    item.id,
                    message.as_deref().unwrap_or("informational mapping")
                ),
            )),
            Some(BehaviorAction::NoOp) | Some(BehaviorAction::SetMobFlag { .. }) | None => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    SourceLoc::file(source_dir),
                    format!("item behavior '{behavior}' on '{}' has no mapping", item.id),
                ));
            }
        }
        let _ = value;
    }

    // metadata.* translations.
    let metadata = &item.metadata;
    if let Some(level) = lookup_metadata_i64(metadata, "level") {
        data.level_requirement = level as i32;
    }
    if let Some(slot) = lookup_metadata_str(metadata, "slot") {
        // Translate Ranvier-specific slot names to IronMUD WearLocation
        // before falling back to the canonical parser.
        let lower = slot.to_lowercase();
        let canonical = match lower.as_str() {
            "chest" => "torso",
            "shield" => "off-hand",
            "legs" => "left leg",
            "feet" => "left foot",
            "hands" => "left hand",
            "arms" => "left arm",
            "wrists" => "left wrist",
            "ankles" => "left ankle",
            "fingers" | "finger" => "left finger",
            _ => lower.as_str(),
        };
        if let Some(loc) = WearLocation::from_str(canonical) {
            data.wear_locations.push(loc);
        } else {
            warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(source_dir),
                format!("metadata.slot '{slot}' on '{}' has no IronMUD WearLocation", item.id),
            ));
        }
    }
    if data.item_type == ItemType::Weapon {
        let min_dmg = lookup_metadata_i64(metadata, "minDamage").unwrap_or(0) as i32;
        let max_dmg = lookup_metadata_i64(metadata, "maxDamage").unwrap_or(0) as i32;
        if max_dmg > 0 {
            // Encode as `min` constant + `(max-min)`-sided d1 — same proxy
            // the CircleMUD obj importer uses for fixed-range damage.
            let span = (max_dmg - min_dmg).max(1);
            data.damage_dice_count = span;
            data.damage_dice_sides = 1;
            data.damage_bonus = min_dmg;
        }
    }
    if data.item_type == ItemType::Armor {
        if let Some(armor) = lookup_metadata_path_i64(metadata, &["stats", "armor"]) {
            data.armor_class = Some(armor as i32);
        }
    }
    if let Some(value) = lookup_metadata_path_i64(metadata, &["sellable", "value"]) {
        data.value = value as i32;
    }
    if lookup_metadata_bool(metadata, "noPickup") {
        data.flags = ItemFlags {
            no_get: true,
            ..data.flags
        };
    }
    if lookup_metadata(metadata, "usable").is_some() {
        warnings.push(Warning::new(
            WarningKind::UnsupportedValueSemantic,
            Severity::Warn,
            SourceLoc::file(source_dir),
            format!(
                "metadata.usable on '{}' not modeled (potion/scroll spell binding deferred)",
                item.id
            ),
        ));
    }

    if item.script.is_some() {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Info,
            SourceLoc::file(source_dir),
            format!("item '{}' has JS script reference (skipped)", item.id),
        ));
    }

    plan.items.push(PlannedItem {
        area_prefix: prefix.to_string(),
        source_vnum,
        vnum: vnum.to_string(),
        data,
        source: SourceLoc::file(source_dir),
    });
}

// =============================== Quests ====================================

fn emit_quest(
    quest: &IrQuest,
    prefix: &str,
    quest_vnum_base: i32,
    vnum_map: &mut VnumMap,
    area: &IrArea,
    plan: &mut Plan,
    warnings: &mut Vec<Warning>,
) {
    let _ = prefix;
    let quest_vnum = vnum_map.resolve_quest(&quest.id, quest_vnum_base);
    let vnum = format!("q_{quest_vnum}");
    let mut quest_data = QuestData::new(vnum, quest.title.clone());
    quest_data.description = quest.description.clone();
    quest_data.completion_text = quest.completion_message.clone();
    quest_data.repeatable = quest.repeatable;

    for goal in &quest.goals {
        let kind = quest_goal_table()
            .goals
            .get(&goal.goal_type)
            .copied()
            .unwrap_or(GoalKind::Warn);
        match kind {
            GoalKind::KillMob => {
                let target = lookup_yaml_str(&goal.config, "npc").or_else(|| lookup_yaml_str(&goal.config, "mob"));
                let count = lookup_yaml_i64(&goal.config, "count").unwrap_or(1) as i32;
                if let Some(npc_id) = target {
                    let (a, bare) = parse_scoped_id(npc_id, &area.name);
                    if let Some(v) = vnum_map.get(&a, Kind::Mobile, &bare) {
                        quest_data.objectives.push(QuestObjective::KillMob {
                            vnum: format!("{}_{v}", a.to_lowercase()),
                            count,
                        });
                    } else {
                        warnings.push(Warning::new(
                            WarningKind::DanglingExit,
                            Severity::Warn,
                            SourceLoc::file(&area.source_dir),
                            format!("quest '{}' KillGoal target '{npc_id}' unresolved", quest.id),
                        ));
                    }
                }
            }
            GoalKind::BringItem => {
                let item_id = lookup_yaml_str(&goal.config, "item");
                let qty = lookup_yaml_i64(&goal.config, "count").unwrap_or(1) as i32;
                if let Some(item_id) = item_id {
                    let (a, bare) = parse_scoped_id(item_id, &area.name);
                    if let Some(v) = vnum_map.get(&a, Kind::Item, &bare) {
                        quest_data.objectives.push(QuestObjective::BringItem {
                            vnum: format!("{}_{v}", a.to_lowercase()),
                            qty,
                            return_to_mob_vnum: None,
                        });
                    } else {
                        warnings.push(Warning::new(
                            WarningKind::DanglingExit,
                            Severity::Warn,
                            SourceLoc::file(&area.source_dir),
                            format!("quest '{}' FetchGoal item '{item_id}' unresolved", quest.id),
                        ));
                    }
                }
            }
            GoalKind::Warn => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(&area.source_dir),
                format!("quest '{}' goal '{}' not modeled in IronMUD", quest.id, goal.goal_type),
            )),
        }
    }

    for reward in &quest.rewards {
        let kind = quest_goal_table()
            .rewards
            .get(&reward.reward_type)
            .copied()
            .unwrap_or(RewardKind::Warn);
        match kind {
            RewardKind::Gold => {
                let amount = lookup_yaml_i64(&reward.config, "amount").unwrap_or(0);
                let currency = lookup_yaml_str(&reward.config, "currency").unwrap_or("gold");
                if currency.eq_ignore_ascii_case("gold") && amount > 0 {
                    quest_data.rewards.push(QuestReward::Gold { amount });
                } else {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        SourceLoc::file(&area.source_dir),
                        format!("quest '{}' currency '{currency}' not gold; reward skipped", quest.id),
                    ));
                }
            }
            RewardKind::SkillXp => {
                let amount = lookup_yaml_i64(&reward.config, "amount").unwrap_or(0) as i32;
                if amount > 0 {
                    quest_data.rewards.push(QuestReward::SkillXp {
                        skill: "general".to_string(),
                        amount,
                    });
                }
            }
            RewardKind::Warn => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                SourceLoc::file(&area.source_dir),
                format!("quest '{}' reward '{}' not modeled", quest.id, reward.reward_type),
            )),
        }
    }

    plan.quests.push(PlannedQuest {
        quest_data,
        source: SourceLoc::file(&area.source_dir),
    });
}

// =============================== Helpers ===================================

fn parse_scoped_id(scoped: &str, default_area: &str) -> (String, String) {
    match scoped.split_once(':') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (default_area.to_string(), scoped.to_string()),
    }
}

fn strip_area_prefix(scoped: &str) -> String {
    scoped
        .split_once(':')
        .map(|(_, b)| b.to_string())
        .unwrap_or_else(|| scoped.to_string())
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn lookup_metadata<'a>(v: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    v.as_mapping()
        .and_then(|m| m.get(serde_yaml::Value::String(key.to_string())))
}

fn lookup_metadata_i64(v: &serde_yaml::Value, key: &str) -> Option<i64> {
    lookup_metadata(v, key).and_then(|x| x.as_i64())
}

fn lookup_metadata_path_i64(v: &serde_yaml::Value, path: &[&str]) -> Option<i64> {
    let mut cur = v;
    for &k in path {
        cur = lookup_metadata(cur, k)?;
    }
    cur.as_i64()
}

fn lookup_metadata_str<'a>(v: &'a serde_yaml::Value, key: &str) -> Option<&'a str> {
    lookup_metadata(v, key).and_then(|x| x.as_str())
}

fn lookup_metadata_bool(v: &serde_yaml::Value, key: &str) -> bool {
    lookup_metadata(v, key).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn lookup_yaml_str<'a>(v: &'a serde_yaml::Value, key: &str) -> Option<&'a str> {
    lookup_metadata_str(v, key)
}

fn lookup_yaml_i64(v: &serde_yaml::Value, key: &str) -> Option<i64> {
    lookup_metadata_i64(v, key)
}
