//! Writer layer: turns a [`Plan`] into either a dry-run report or a series
//! of Sled writes via [`crate::db::Db`].

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::db::Db;
use crate::import::{AttachType, Plan, PlannedDoor, PlannedMobile, Severity, TriggerMutation, Warning};
use crate::types::{
    AreaData, AreaFlags, AreaPermission, CombatZoneType, DoorState, GoldRange, ImmigrationFamilyChance,
    ImmigrationVariationChances, MobileData, RoomData, RoomExits, RoomFlags, SpawnDependency, SpawnPointData,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportSummary {
    pub planned_areas: usize,
    pub planned_rooms: usize,
    pub planned_exits: usize,
    pub planned_mobiles: usize,
    pub planned_items: usize,
    pub planned_shop_overlays: usize,
    pub planned_spawns: usize,
    pub planned_trigger_overlays: usize,
    pub block_warnings: usize,
    pub warn_warnings: usize,
    pub info_warnings: usize,
    pub written_areas: usize,
    pub written_rooms: usize,
    pub linked_exits: usize,
    pub dropped_exits: usize,
    pub written_mobiles: usize,
    pub written_items: usize,
    pub overlaid_shops: usize,
    pub written_spawns: usize,
    pub applied_triggers: usize,
}

pub fn print_dry_run(plan: &Plan, warnings: &[Warning]) -> ReportSummary {
    let summary = summarize(plan, warnings, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    println!(
        "== Import dry-run ==\n  areas: {}\n  rooms: {}\n  exits: {}\n  mobiles: {}\n  items: {}\n  shop overlays: {}\n  spawns: {}\n  trigger overlays: {}\n",
        summary.planned_areas,
        summary.planned_rooms,
        summary.planned_exits,
        summary.planned_mobiles,
        summary.planned_items,
        summary.planned_shop_overlays,
        summary.planned_spawns,
        summary.planned_trigger_overlays,
    );
    if !plan.areas.is_empty() {
        println!("Areas:");
        for a in &plan.areas {
            println!("  - {} (prefix: {}, source #{})", a.name, a.prefix, a.source_vnum);
        }
        println!();
    }
    print_warnings(warnings);
    summary
}

pub fn print_warnings(warnings: &[Warning]) {
    if warnings.is_empty() {
        println!("No warnings.\n");
        return;
    }
    let mut by_sev: HashMap<&'static str, Vec<&Warning>> = HashMap::new();
    for w in warnings {
        let key = match w.severity {
            Severity::Block => "BLOCK",
            Severity::Warn => "WARN",
            Severity::Info => "INFO",
        };
        by_sev.entry(key).or_default().push(w);
    }
    for sev in ["BLOCK", "WARN", "INFO"] {
        if let Some(items) = by_sev.get(sev) {
            println!("[{sev}] {} entries", items.len());
            for w in items {
                let where_ = format_loc(&w.source);
                println!("  {where_} {:?}: {}", w.kind, w.message);
                if let Some(s) = &w.suggestion {
                    println!("      suggestion: {s}");
                }
            }
            println!();
        }
    }
}

fn format_loc(loc: &crate::import::SourceLoc) -> String {
    let mut s = loc.file.display().to_string();
    if let Some(line) = loc.line {
        s.push_str(&format!(":{line}"));
    }
    if let Some(z) = loc.zone_vnum {
        s.push_str(&format!(" [zone#{z}]"));
    }
    if let Some(r) = loc.room_vnum {
        s.push_str(&format!(" [room#{r}]"));
    }
    s
}

pub fn write_report_file(path: &Path, plan: &Plan, warnings: &[Warning], summary: &ReportSummary) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Report<'a> {
        summary: &'a ReportSummary,
        warnings: &'a [Warning],
        areas: Vec<AreaSummary<'a>>,
    }
    #[derive(serde::Serialize)]
    struct AreaSummary<'a> {
        name: &'a str,
        prefix: &'a str,
        source_vnum: i32,
        room_count: usize,
    }
    let areas: Vec<AreaSummary> = plan
        .areas
        .iter()
        .map(|a| AreaSummary {
            name: &a.name,
            prefix: &a.prefix,
            source_vnum: a.source_vnum,
            room_count: plan.rooms.iter().filter(|r| r.area_prefix == a.prefix).count(),
        })
        .collect();
    let report = Report {
        summary,
        warnings,
        areas,
    };
    let text = serde_json::to_string_pretty(&report).context("serializing report")?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn apply(db: &Db, plan: &Plan, warnings: &[Warning]) -> Result<ReportSummary> {
    if has_blocking(warnings) {
        anyhow::bail!(
            "refusing to apply: {} blocking warning(s) (run without --apply to review)",
            warnings.iter().filter(|w| w.severity == Severity::Block).count()
        );
    }

    // Pass 1: areas → DB (record vnum-prefix → area_id).
    let mut prefix_to_area: HashMap<String, Uuid> = HashMap::new();
    for a in &plan.areas {
        let id = Uuid::new_v4();
        let area = AreaData {
            id,
            name: a.name.clone(),
            prefix: a.prefix.clone(),
            description: a.description.clone(),
            level_min: 0,
            level_max: 0,
            theme: String::new(),
            owner: None,
            permission_level: AreaPermission::AllBuilders,
            trusted_builders: Vec::new(),
            city_forage_table: Vec::new(),
            wilderness_forage_table: Vec::new(),
            shallow_water_forage_table: Vec::new(),
            deep_water_forage_table: Vec::new(),
            underwater_forage_table: Vec::new(),
            combat_zone: CombatZoneType::default(),
            flags: AreaFlags::default(),
            default_room_flags: RoomFlags::default(),
            immigration_enabled: false,
            immigration_room_vnum: String::new(),
            immigration_name_pool: String::new(),
            immigration_visual_profile: String::new(),
            migration_interval_days: 0,
            migration_max_per_check: 0,
            migrant_sim_defaults: None,
            last_migration_check_day: None,
            immigration_variation_chances: ImmigrationVariationChances::default(),
            immigration_family_chance: ImmigrationFamilyChance::default(),
            migrant_starting_gold: GoldRange::default(),
            guard_wage_per_hour: 0,
            healer_wage_per_hour: 0,
            scavenger_wage_per_hour: 0,
        };
        db.save_area_data(area)
            .with_context(|| format!("saving area {}", a.prefix))?;
        prefix_to_area.insert(a.prefix.clone(), id);
    }

    // Pass 1b: rooms → DB (without exits). Build vnum→UUID map.
    let mut vnum_to_id: HashMap<String, Uuid> = HashMap::new();
    let mut source_vnum_to_string: HashMap<(String, i32), String> = HashMap::new();
    for r in &plan.rooms {
        let id = Uuid::new_v4();
        let area_id = prefix_to_area.get(&r.area_prefix).copied();
        let mut doors_map = std::collections::HashMap::new();
        let key_lookup = |source_vnum: i32| -> Option<String> {
            // Resolve a key vnum from the source-side numeric to the
            // prefixed IronMUD vnum string. Keys may live in any of the
            // imported areas; first match wins.
            for ar in &plan.areas {
                let v = format!("{}_{}", ar.prefix, source_vnum);
                if plan.rooms.iter().any(|rr| rr.vnum == v) {
                    return Some(v);
                }
            }
            // Items may reference a vnum we haven't imported yet — keep the
            // raw form prefixed by this area so it round-trips.
            None
        };
        for d in &r.doors {
            doors_map.insert(d.direction.clone(), build_door_state(d, key_lookup));
        }
        let room = RoomData {
            id,
            title: r.title.clone(),
            description: r.description.clone(),
            exits: RoomExits::default(),
            flags: r.flags.clone(),
            extra_descs: r.extra_descs.clone(),
            vnum: Some(r.vnum.clone()),
            area_id,
            triggers: Vec::new(),
            doors: doors_map,
            spring_desc: None,
            summer_desc: None,
            autumn_desc: None,
            winter_desc: None,
            dynamic_desc: None,
            water_type: Default::default(),
            catch_table: Vec::new(),
            is_property_template: false,
            property_template_id: None,
            is_template_entrance: false,
            property_lease_id: None,
            property_entrance: false,
            recent_departures: Vec::new(),
            blood_trails: Vec::new(),
            traps: Vec::new(),
            living_capacity: 0,
            residents: Vec::new(),
        };
        db.save_room_data(room)
            .with_context(|| format!("saving room {}", r.vnum))?;
        vnum_to_id.insert(r.vnum.clone(), id);
        source_vnum_to_string.insert((r.area_prefix.clone(), r.source_vnum), r.vnum.clone());
    }

    // Pass 2: link exits. For each PlannedExit, resolve the destination
    // source vnum to a target room. We try the originating area first
    // (intra-zone), then fall back to any area sharing that source vnum
    // (cross-zone).
    let mut linked = 0usize;
    let mut dropped = 0usize;
    for ex in &plan.exits {
        let from_id = match vnum_to_id.get(&ex.from_vnum) {
            Some(id) => *id,
            None => {
                dropped += 1;
                continue;
            }
        };
        // Find originating area prefix from from_vnum (everything before the
        // last `_NNN`).
        let origin_prefix: Option<&str> = ex.from_vnum.rsplit_once('_').map(|(left, _)| left);
        let mut to_id: Option<Uuid> = None;
        if let Some(prefix) = origin_prefix {
            if let Some(v) = source_vnum_to_string.get(&(prefix.to_string(), ex.to_source_vnum)) {
                to_id = vnum_to_id.get(v).copied();
            }
        }
        if to_id.is_none() {
            // Cross-zone fallback: any area with that source vnum.
            for ((_, src_vnum), v) in &source_vnum_to_string {
                if *src_vnum == ex.to_source_vnum {
                    if let Some(id) = vnum_to_id.get(v) {
                        to_id = Some(*id);
                        break;
                    }
                }
            }
        }
        match to_id {
            Some(target) => {
                db.set_room_exit(&from_id, &ex.direction, &target)
                    .with_context(|| format!("linking {} {} -> #{}", ex.from_vnum, ex.direction, ex.to_source_vnum))?;
                linked += 1;
            }
            None => dropped += 1,
        }
    }

    // The vnum index is a separate Sled tree that save_room_data does not
    // update. Rebuild it once at the end so get_room_by_vnum works on the
    // freshly imported rooms.
    db.rebuild_vnum_index().context("rebuilding vnum index after import")?;

    // Pass 3: mobiles. Each PlannedMobile becomes a fresh prototype
    // MobileData. Mobile vnum lookup is a linear scan in db.rs so no
    // separate index needs rebuilding.
    let mut written_mobiles = 0usize;
    for m in &plan.mobiles {
        let mobile = build_mobile(m);
        db.save_mobile_data(mobile)
            .with_context(|| format!("saving mobile {}", m.vnum))?;
        written_mobiles += 1;
    }

    // Pass 4: items. The ItemData was fully built during mapping; we just
    // mint a fresh UUID per save (vnum stays stable across re-imports). No
    // vnum-index rebuild needed — get_item_by_vnum in db.rs is a linear scan.
    let mut written_items = 0usize;
    for it in &plan.items {
        let mut item = it.data.clone();
        item.id = Uuid::new_v4();
        db.save_item_data(item)
            .with_context(|| format!("saving item {}", it.vnum))?;
        written_items += 1;
    }

    // Pass 5: shop overlays. The keeper mob already exists (Pass 3); load
    // it, set the shop fields, and save it back. Shopkeeper flag is set
    // defensively — `MOB_SHOPKEEPER` may not appear in the source `.mob`
    // bitvector since stock CircleMUD doesn't carry it there.
    let mut overlaid_shops = 0usize;
    for overlay in &plan.shop_overlays {
        let Some(mut mobile) = db
            .get_mobile_by_vnum(&overlay.keeper_vnum)
            .with_context(|| format!("looking up keeper {}", overlay.keeper_vnum))?
        else {
            continue;
        };
        mobile.flags.shopkeeper = true;
        mobile.shop_stock = overlay.stock_vnums.clone();
        mobile.shop_buy_rate = overlay.buy_rate;
        mobile.shop_sell_rate = overlay.sell_rate;
        // Preserve the default ["all"] only if the source file omitted a
        // buy list; an explicit empty list means "this shop sells but
        // doesn't buy back from players".
        mobile.shop_buys_types = overlay.buys_types.clone();
        db.save_mobile_data(mobile)
            .with_context(|| format!("saving keeper {}", overlay.keeper_vnum))?;
        overlaid_shops += 1;
    }

    // Pass 6: spawn points. Each PlannedSpawn produces a fresh
    // SpawnPointData. room_vnum / area_prefix are resolved via the maps
    // built during the room pass.
    let mut written_spawns = 0usize;
    for sp in &plan.spawns {
        let Some(&room_id) = vnum_to_id.get(&sp.room_vnum) else {
            // The room must have been imported in this run (we only emit
            // spawns whose room lives in the same area). A miss here means
            // the planning layer let an out-of-set room through; skip rather
            // than abort so the rest of the pass still applies.
            continue;
        };
        let Some(&area_id) = prefix_to_area.get(&sp.area_prefix) else {
            continue;
        };
        let dependencies: Vec<SpawnDependency> = sp
            .dependencies
            .iter()
            .map(|d| SpawnDependency {
                item_vnum: d.item_vnum.clone(),
                destination: d.destination.clone(),
                count: d.count,
                chance: 100,
            })
            .collect();
        let data = SpawnPointData {
            id: Uuid::new_v4(),
            area_id,
            room_id,
            entity_type: sp.entity_type,
            vnum: sp.vnum.clone(),
            max_count: sp.max_count,
            respawn_interval_secs: sp.respawn_interval_secs,
            enabled: true,
            last_spawn_time: 0,
            spawned_entities: Vec::new(),
            dependencies,
            bury_on_spawn: false,
        };
        db.save_spawn_point(data)
            .with_context(|| format!("saving spawn point for {} in {}", sp.vnum, sp.room_vnum))?;
        written_spawns += 1;
    }

    // Pass 7: trigger overlays (specproc bindings from spec_assign.c).
    // Runs after shop overlays so a single mob may compose flags from both
    // (e.g. cityguard + receptionist on the same vnum). Each overlay
    // either flips a bool field on `MobileFlags` or pushes a `*Trigger`
    // onto the entity's `triggers` Vec.
    let mut applied_triggers = 0usize;
    for ov in &plan.trigger_overlays {
        match ov.attach_type {
            AttachType::Mob => {
                let Some(mut mobile) = db
                    .get_mobile_by_vnum(&ov.target_vnum)
                    .with_context(|| format!("looking up mob {}", ov.target_vnum))?
                else {
                    continue;
                };
                let changed = match &ov.mutation {
                    TriggerMutation::SetMobFlag { ironmud_flag } => {
                        apply_named_mob_flag(&mut mobile.flags, ironmud_flag)
                    }
                    TriggerMutation::AddMobTrigger(t) => {
                        mobile.triggers.push(t.clone());
                        true
                    }
                    _ => false,
                };
                if changed {
                    db.save_mobile_data(mobile)
                        .with_context(|| format!("saving mob {} after trigger overlay", ov.target_vnum))?;
                    applied_triggers += 1;
                }
            }
            AttachType::Obj => {
                let Some(mut item) = db
                    .get_item_by_vnum(&ov.target_vnum)
                    .with_context(|| format!("looking up item {}", ov.target_vnum))?
                else {
                    continue;
                };
                let changed = match &ov.mutation {
                    TriggerMutation::AddItemTrigger(t) => {
                        item.triggers.push(t.clone());
                        true
                    }
                    _ => false,
                };
                if changed {
                    db.save_item_data(item)
                        .with_context(|| format!("saving item {} after trigger overlay", ov.target_vnum))?;
                    applied_triggers += 1;
                }
            }
            AttachType::Room => {
                let Some(mut room) = db
                    .get_room_by_vnum(&ov.target_vnum)
                    .with_context(|| format!("looking up room {}", ov.target_vnum))?
                else {
                    continue;
                };
                let changed = match &ov.mutation {
                    TriggerMutation::AddRoomTrigger(t) => {
                        room.triggers.push(t.clone());
                        true
                    }
                    _ => false,
                };
                if changed {
                    db.save_room_data(room)
                        .with_context(|| format!("saving room {} after trigger overlay", ov.target_vnum))?;
                    applied_triggers += 1;
                }
            }
        }
    }

    let summary = summarize(
        plan,
        warnings,
        plan.areas.len(),
        plan.rooms.len(),
        linked,
        dropped,
        written_mobiles,
        written_items,
        overlaid_shops,
        written_spawns,
        applied_triggers,
    );
    println!(
        "== Import applied ==\n  areas: {}\n  rooms: {}\n  exits linked: {} (dropped: {})\n  mobiles: {}\n  items: {}\n  shop overlays: {}\n  spawn points: {}\n  trigger overlays: {}\n",
        summary.written_areas,
        summary.written_rooms,
        summary.linked_exits,
        summary.dropped_exits,
        summary.written_mobiles,
        summary.written_items,
        summary.overlaid_shops,
        summary.written_spawns,
        summary.applied_triggers,
    );
    if dropped > 0 {
        println!("  note: {dropped} exit(s) pointed to vnums outside the imported set; they were not linked.");
    }
    Ok(summary)
}

fn build_mobile(p: &PlannedMobile) -> MobileData {
    let mut m = MobileData::new(p.name.clone());
    m.is_prototype = true;
    m.vnum = p.vnum.clone();
    m.short_desc = p.short_desc.clone();
    m.long_desc = p.long_desc.clone();
    m.keywords = p.keywords.clone();
    m.level = p.level;
    m.max_hp = p.max_hp;
    m.current_hp = p.max_hp;
    m.damage_dice = p.damage_dice.clone();
    m.armor_class = p.armor_class;
    m.gold = p.gold;
    m.flags = p.flags.clone();
    m.world_max_count = p.world_max_count;
    m
}

fn build_door_state(d: &PlannedDoor, key_lookup: impl Fn(i32) -> Option<String>) -> DoorState {
    DoorState {
        name: d.name.clone(),
        is_closed: d.is_closed,
        is_locked: d.is_locked,
        key_vnum: d.key_source_vnum.and_then(key_lookup),
        description: d.description.clone(),
        keywords: d.keywords.clone(),
        pickproof: d.pickproof,
    }
}

fn has_blocking(warnings: &[Warning]) -> bool {
    warnings.iter().any(|w| w.severity == Severity::Block)
}

fn summarize(
    plan: &Plan,
    warnings: &[Warning],
    written_areas: usize,
    written_rooms: usize,
    linked_exits: usize,
    dropped_exits: usize,
    written_mobiles: usize,
    written_items: usize,
    overlaid_shops: usize,
    written_spawns: usize,
    applied_triggers: usize,
) -> ReportSummary {
    let mut blocks = 0;
    let mut warns = 0;
    let mut infos = 0;
    for w in warnings {
        match w.severity {
            Severity::Block => blocks += 1,
            Severity::Warn => warns += 1,
            Severity::Info => infos += 1,
        }
    }
    ReportSummary {
        planned_areas: plan.areas.len(),
        planned_rooms: plan.rooms.len(),
        planned_exits: plan.exits.len(),
        planned_mobiles: plan.mobiles.len(),
        planned_items: plan.items.len(),
        planned_shop_overlays: plan.shop_overlays.len(),
        planned_spawns: plan.spawns.len(),
        planned_trigger_overlays: plan.trigger_overlays.len(),
        block_warnings: blocks,
        warn_warnings: warns,
        info_warnings: infos,
        written_areas,
        written_rooms,
        linked_exits,
        dropped_exits,
        written_mobiles,
        written_items,
        overlaid_shops,
        written_spawns,
        applied_triggers,
    }
}

/// Set a single bool field on `MobileFlags` by snake_case name. Mirrors
/// `apply_named_room_flag` over in mapping.rs but kept here because the
/// writer handles the apply pass for trigger overlays.
fn apply_named_mob_flag(flags: &mut crate::types::MobileFlags, name: &str) -> bool {
    macro_rules! set_field {
        ($($f:ident),* $(,)?) => {
            match name {
                $(stringify!($f) => { flags.$f = true; true })*
                _ => false,
            }
        };
    }
    set_field!(
        sentinel,
        scavenger,
        aggressive,
        cowardly,
        guard,
        helper,
        healer,
        leasing_agent,
        shopkeeper,
        no_attack,
        can_open_doors,
        poisonous,
        fiery,
        chilling,
        corrosive,
        shocking,
    )
}
