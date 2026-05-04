//! CircleMUD 3.x importer engine.
//!
//! Discovers `.zon` and `.wld` files under a CircleMUD source tree and
//! produces an engine-neutral [`ImportIR`]. Pairs each zone with the rooms
//! whose vnum falls in the zone's `bot..=top` range.

pub mod flags;
pub mod mob;
pub mod obj;
pub mod parser;
pub mod shp;
pub mod spec;
pub mod wear;
pub mod wld;
pub mod zon;

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::{AttachType, ImportIR, IrTrigger, IrZone, MudEngine, Severity, SourceLoc, Warning, WarningKind};

pub struct CircleEngine;

impl MudEngine for CircleEngine {
    fn name(&self) -> &'static str {
        "circle"
    }

    fn parse(&self, source: &Path) -> Result<(ImportIR, Vec<Warning>)> {
        let world_root = locate_world_root(source)
            .with_context(|| format!("could not find CircleMUD world data under {}", source.display()))?;
        let wld_dir = world_root.join("wld");
        let zon_dir = world_root.join("zon");
        let mob_dir = world_root.join("mob");
        let obj_dir = world_root.join("obj");
        let shp_dir = world_root.join("shp");

        let mut warnings = Vec::new();
        let mut all_rooms = Vec::new();
        for entry in read_files(&wld_dir, "wld")? {
            match wld::parse_file(&entry) {
                Ok(rooms) => all_rooms.extend(rooms),
                Err(e) => warnings.push(Warning::new(
                    WarningKind::Parse,
                    Severity::Warn,
                    SourceLoc::file(entry.clone()),
                    format!("skipped {}: {}", entry.display(), e),
                )),
            }
        }

        // The mob/ dir is optional — old or partial Circle trees may ship
        // rooms only. Treat a missing dir as "no mobiles to import" rather
        // than a hard failure.
        let mut all_mobs = Vec::new();
        if mob_dir.is_dir() {
            for entry in read_files(&mob_dir, "mob")? {
                match mob::parse_file(&entry) {
                    Ok(mobs) => all_mobs.extend(mobs),
                    Err(e) => warnings.push(Warning::new(
                        WarningKind::Parse,
                        Severity::Warn,
                        SourceLoc::file(entry.clone()),
                        format!("skipped {}: {}", entry.display(), e),
                    )),
                }
            }
        }

        // Same treatment for obj/. Stock Circle ships ~27 .obj files; partial
        // trees (rooms-only fixtures) may omit the directory entirely.
        let mut all_items = Vec::new();
        if obj_dir.is_dir() {
            for entry in read_files(&obj_dir, "obj")? {
                match obj::parse_file(&entry) {
                    Ok(items) => all_items.extend(items),
                    Err(e) => warnings.push(Warning::new(
                        WarningKind::Parse,
                        Severity::Warn,
                        SourceLoc::file(entry.clone()),
                        format!("skipped {}: {}", entry.display(), e),
                    )),
                }
            }
        }

        // shp/ is similarly optional. Stock Circle 3.1 ships eight `.shp`
        // files (shops are sparse — most zones don't have one); partial
        // trees may omit the directory entirely.
        let mut all_shops = Vec::new();
        if shp_dir.is_dir() {
            for entry in read_files(&shp_dir, "shp")? {
                match shp::parse_file(&entry) {
                    Ok(shops) => all_shops.extend(shops),
                    Err(e) => warnings.push(Warning::new(
                        WarningKind::Parse,
                        Severity::Warn,
                        SourceLoc::file(entry.clone()),
                        format!("skipped {}: {}", entry.display(), e),
                    )),
                }
            }
        }

        let mut parsed_zons = Vec::new();
        for entry in read_files(&zon_dir, "zon")? {
            match zon::parse_file(&entry) {
                Ok(z) => parsed_zons.push((entry, z)),
                Err(e) => warnings.push(Warning::new(
                    WarningKind::Parse,
                    Severity::Warn,
                    SourceLoc::file(entry.clone()),
                    format!("skipped {}: {}", entry.display(), e),
                )),
            }
        }

        // Map zone vnum -> index in the output Vec for assignment below.
        let mut zone_index: HashMap<i32, usize> = HashMap::new();
        let mut zones: Vec<IrZone> = parsed_zons
            .iter()
            .map(|(path, z)| {
                zone_index.insert(z.header.vnum, zones_len_placeholder());
                IrZone {
                    vnum: z.header.vnum,
                    name: z.header.name.clone(),
                    description: None,
                    vnum_range: Some((z.header.bot, z.header.top)),
                    default_respawn_secs: if z.header.lifespan > 0 {
                        Some((z.header.lifespan as i64) * 60)
                    } else {
                        None
                    },
                    source: SourceLoc::file(path.clone()).with_zone(z.header.vnum),
                    rooms: Vec::new(),
                    mobiles: Vec::new(),
                    items: Vec::new(),
                    shops: Vec::new(),
                    resets: z.resets.clone(),
                    deferred: z.deferred.clone(),
                }
            })
            .collect();
        // Re-fill the index with real positions now that the vec is built.
        zone_index.clear();
        for (idx, z) in zones.iter().enumerate() {
            zone_index.insert(z.vnum, idx);
        }

        // Assign rooms to zones by vnum range.
        let mut orphan_zone: Option<usize> = None;
        for room in all_rooms {
            let target = zones.iter().position(|z| {
                z.vnum_range
                    .map(|(b, t)| room.vnum >= b && room.vnum <= t)
                    .unwrap_or(false)
            });
            match target {
                Some(idx) => zones[idx].rooms.push(room),
                None => {
                    let idx = ensure_orphan_zone(&mut zones, &mut orphan_zone, &world_root);
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        room.source.clone(),
                        format!(
                            "room #{} did not fall in any zone vnum range; bucketed under 'Imported'",
                            room.vnum
                        ),
                    ));
                    zones[idx].rooms.push(room);
                }
            }
        }

        // Assign mobiles to zones by the same vnum-range rule. Mob and room
        // vnums share a single zone range in stock CircleMUD (`bot..=top`).
        for mob in all_mobs {
            let target = zones.iter().position(|z| {
                z.vnum_range
                    .map(|(b, t)| mob.vnum >= b && mob.vnum <= t)
                    .unwrap_or(false)
            });
            match target {
                Some(idx) => zones[idx].mobiles.push(mob),
                None => {
                    let idx = ensure_orphan_zone(&mut zones, &mut orphan_zone, &world_root);
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        mob.source.clone(),
                        format!(
                            "mob #{} did not fall in any zone vnum range; bucketed under 'Imported'",
                            mob.vnum
                        ),
                    ));
                    zones[idx].mobiles.push(mob);
                }
            }
        }

        // Shops bucket by their own `vnum` field (the shop record number,
        // not the keeper's vnum). Stock CircleMUD numbers shops in the same
        // range as the owning zone's mobs/rooms, so a single shop record
        // typically lands in the zone hosting its keeper. Cross-zone keepers
        // are still resolved at mapping time via the global mob index.
        for shop in all_shops {
            let target = zones.iter().position(|z| {
                z.vnum_range
                    .map(|(b, t)| shop.vnum >= b && shop.vnum <= t)
                    .unwrap_or(false)
            });
            match target {
                Some(idx) => zones[idx].shops.push(shop),
                None => {
                    let idx = ensure_orphan_zone(&mut zones, &mut orphan_zone, &world_root);
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        shop.source.clone(),
                        format!(
                            "shop #{} did not fall in any zone vnum range; bucketed under 'Imported'",
                            shop.vnum
                        ),
                    ));
                    zones[idx].shops.push(shop);
                }
            }
        }

        // Items share the same per-zone vnum range as rooms and mobs.
        for item in all_items {
            let target = zones.iter().position(|z| {
                z.vnum_range
                    .map(|(b, t)| item.vnum >= b && item.vnum <= t)
                    .unwrap_or(false)
            });
            match target {
                Some(idx) => zones[idx].items.push(item),
                None => {
                    let idx = ensure_orphan_zone(&mut zones, &mut orphan_zone, &world_root);
                    warnings.push(Warning::new(
                        WarningKind::Info,
                        Severity::Info,
                        item.source.clone(),
                        format!(
                            "obj #{} did not fall in any zone vnum range; bucketed under 'Imported'",
                            item.vnum
                        ),
                    ));
                    zones[idx].items.push(item);
                }
            }
        }

        // Specprocs: optional. Try to locate `src/spec_assign.c` and
        // `src/castle.c` relative to the source. Missing is an Info note,
        // not an error — partial trees (lib/world only) won't have them.
        let mut triggers: Vec<IrTrigger> = Vec::new();
        match locate_src_dir(source, &world_root) {
            Some(src_dir) => {
                let spec_path = src_dir.join("spec_assign.c");
                let castle_path = src_dir.join("castle.c");
                let spec_procs_path = src_dir.join("spec_procs.c");
                if spec_path.is_file() {
                    match spec::parse_spec_assign(&spec_path) {
                        Ok(mut ts) => {
                            // Attach puff()'s quote literals as args on the
                            // first puff binding so the importer can fold them
                            // into a `@say_random` template trigger.
                            if spec_procs_path.is_file() {
                                if let Ok(quotes) = spec::parse_puff_quotes(&spec_procs_path) {
                                    if !quotes.is_empty() {
                                        for t in ts.iter_mut() {
                                            if t.specproc_name == "puff" && t.attach_type == AttachType::Mob {
                                                t.args = quotes.clone();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            triggers.extend(ts);
                        }
                        Err(e) => warnings.push(Warning::new(
                            WarningKind::Parse,
                            Severity::Warn,
                            SourceLoc::file(spec_path.clone()),
                            format!("skipped {}: {}", spec_path.display(), e),
                        )),
                    }
                }
                if castle_path.is_file() {
                    match spec::parse_castle(&castle_path) {
                        Ok(ts) => triggers.extend(ts),
                        Err(e) => warnings.push(Warning::new(
                            WarningKind::Parse,
                            Severity::Warn,
                            SourceLoc::file(castle_path.clone()),
                            format!("skipped {}: {}", castle_path.display(), e),
                        )),
                    }
                }
            }
            None => warnings.push(Warning::new(
                WarningKind::Info,
                Severity::Info,
                SourceLoc::file(source.to_path_buf()),
                "no src/spec_assign.c located near source — specproc bindings skipped".to_string(),
            )),
        }

        Ok((ImportIR { zones, triggers }, warnings))
    }
}

/// Locate the CircleMUD `src/` directory containing `spec_assign.c`.
/// Tries common relative positions: source/src, source/../src,
/// world_root/../../src (when world_root = source/lib/world). Returns
/// the directory if it contains `spec_assign.c`.
fn locate_src_dir(source: &Path, world_root: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.push(source.join("src"));
    if let Some(p) = source.parent() {
        candidates.push(p.join("src"));
    }
    // world_root is typically <root>/lib/world; src/ lives at <root>/src.
    if let Some(lib) = world_root.parent() {
        if let Some(root) = lib.parent() {
            candidates.push(root.join("src"));
        }
    }
    for c in candidates {
        if c.join("spec_assign.c").is_file() {
            return Some(c);
        }
    }
    None
}

/// Placeholder that gets overwritten in the second pass — kept around so we
/// don't need to thread vec lengths through the iterator chain.
fn zones_len_placeholder() -> usize {
    0
}

/// Lazily appends an "orphan" bucket zone to hold rooms/mobs whose vnum
/// fell outside every parsed zone range, returning its index. Both the
/// room and mob assignment passes share this bucket.
fn ensure_orphan_zone(zones: &mut Vec<IrZone>, slot: &mut Option<usize>, world_root: &PathBuf) -> usize {
    if let Some(idx) = *slot {
        return idx;
    }
    zones.push(IrZone {
        vnum: -1,
        name: "Imported (orphans)".into(),
        description: Some("Rooms, mobiles, or objects that did not fall inside any zone vnum range.".into()),
        vnum_range: None,
        default_respawn_secs: None,
        source: SourceLoc::file(world_root.clone()),
        rooms: Vec::new(),
        mobiles: Vec::new(),
        items: Vec::new(),
        shops: Vec::new(),
        resets: Vec::new(),
        deferred: Vec::new(),
    });
    let idx = zones.len() - 1;
    *slot = Some(idx);
    idx
}

/// Find the directory that contains `wld/` and `zon/`. Accepts the world
/// dir itself (`<root>/lib/world`), or a parent (`<root>` or `<root>/lib`).
fn locate_world_root(source: &Path) -> Result<PathBuf> {
    let candidates = [
        source.to_path_buf(),
        source.join("lib").join("world"),
        source.join("world"),
    ];
    for c in candidates {
        if c.join("wld").is_dir() && c.join("zon").is_dir() {
            return Ok(c);
        }
    }
    Err(anyhow!("no wld/ + zon/ directories found under {}", source.display()))
}

fn read_files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Err(anyhow!("expected directory: {}", dir.display()));
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some(ext) {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}
