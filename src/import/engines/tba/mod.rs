//! tbaMUD importer engine.
//!
//! tbaMUD is a modern CircleMUD descendant. It shares CircleMUD's core file
//! layout (`lib/world/{wld,mob,obj,zon,shp}/`) but extends three things:
//!
//! - **128-bit ascii flag fields** on mob action lines (10 tokens) and obj
//!   type/flags lines (13 tokens). Stock CircleMUD uses 4 / 3 tokens.
//! - **DG Scripts trigger attachments** (`T <vnum>` lines on rooms / mobs /
//!   objects, plus `lib/world/trg/*.trg` source files). Warn-only stubs
//!   today — bodies are not translated.
//! - **Quests** (`lib/world/qst/*.qst`). No IronMUD analog; warn-only.
//!
//! Sharing strategy: rooms / shops parse via `circle::wld::parse_file` and
//! `circle::shp::parse_file` unchanged (the wld parser already tolerates
//! tbaMUD's slightly longer flag line). T-trailer consumption on rooms is
//! handled by [`wld::extract_trigger_attachments`] running over the source
//! file as a second pass. Mob, obj, and zone parsing fork into the local
//! submodules for the format deltas.

pub mod mob;
pub mod obj;
pub mod qst;
pub mod trg;
pub mod trg_map;
pub mod wld;
pub mod zon;

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::circle::{shp, wld as circle_wld};
use super::super::{
    ImportIR, IrDgTrigger, IrQuest, IrZone, MudEngine, Severity, SourceLoc, Warning, WarningKind,
};

pub struct TbaEngine;

impl MudEngine for TbaEngine {
    fn name(&self) -> &'static str {
        "tba"
    }

    fn parse(&self, source: &Path) -> Result<(ImportIR, Vec<Warning>)> {
        let world_root = locate_world_root(source).with_context(|| {
            format!(
                "could not find tbaMUD world data under {} (expected lib/world or world/ with wld/+zon/)",
                source.display()
            )
        })?;
        let wld_dir = world_root.join("wld");
        let zon_dir = world_root.join("zon");
        let mob_dir = world_root.join("mob");
        let obj_dir = world_root.join("obj");
        let shp_dir = world_root.join("shp");
        let trg_dir = world_root.join("trg");
        let qst_dir = world_root.join("qst");

        let mut warnings: Vec<Warning> = Vec::new();

        // Rooms: shared circle parser. T-trailer attachments are extracted in
        // a separate pass below so the main parser stays format-agnostic.
        let mut all_rooms = Vec::new();
        let mut room_trigger_attach: HashMap<i32, Vec<i32>> = HashMap::new();
        for entry in read_files(&wld_dir, "wld")? {
            match circle_wld::parse_file(&entry) {
                Ok(rooms) => all_rooms.extend(rooms),
                Err(e) => warnings.push(Warning::new(
                    WarningKind::Parse,
                    Severity::Warn,
                    SourceLoc::file(entry.clone()),
                    format!("skipped {}: {}", entry.display(), e),
                )),
            }
            // T attachments live AFTER each room's `S` terminator in tbaMUD,
            // outside the room block — extract them via a per-file pass over
            // the raw source. (Stock circle wld parser would error if T were
            // inside the block; tbaMUD always puts them between records.)
            if let Ok(text) = std::fs::read_to_string(&entry) {
                wld::extract_trigger_attachments(&text, &mut room_trigger_attach);
            }
        }
        for r in &mut all_rooms {
            if let Some(vs) = room_trigger_attach.remove(&r.vnum) {
                r.trigger_vnums = vs;
            }
        }

        // Mobs: tba-specific parser handles 10-token action line + T trailers.
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

        // Objects: tba-specific parser handles 13-token type/flags line +
        // 5-token weight/cost/rent line + T attachments.
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

        // Shops: identical format to stock circle (tbaMUD didn't extend it).
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

        // Zones: tba-specific parser tolerates the longer header (zone_flags,
        // levels, builders) and handles new T/V reset commands.
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

        // DG Scripts: warn-only stubs. Each .trg record gets recorded so the
        // mapper can emit per-attachment Warns naming the source vnum.
        let mut dg_triggers: Vec<IrDgTrigger> = Vec::new();
        if trg_dir.is_dir() {
            for entry in read_files(&trg_dir, "trg")? {
                match trg::parse_file(&entry) {
                    Ok(ts) => dg_triggers.extend(ts),
                    Err(e) => warnings.push(Warning::new(
                        WarningKind::Parse,
                        Severity::Warn,
                        SourceLoc::file(entry.clone()),
                        format!("skipped {}: {}", entry.display(), e),
                    )),
                }
            }
        }

        // Quests: warn-only stubs. IronMUD has no quest system.
        let mut quests: Vec<IrQuest> = Vec::new();
        if qst_dir.is_dir() {
            for entry in read_files(&qst_dir, "qst")? {
                match qst::parse_file(&entry) {
                    Ok(qs) => quests.extend(qs),
                    Err(e) => warnings.push(Warning::new(
                        WarningKind::Parse,
                        Severity::Warn,
                        SourceLoc::file(entry.clone()),
                        format!("skipped {}: {}", entry.display(), e),
                    )),
                }
            }
        }

        // Build IrZones, then bucket entities by vnum range. Mirrors the
        // logic in `circle::CircleEngine::parse`.
        let mut zones: Vec<IrZone> = parsed_zons
            .iter()
            .map(|(path, z)| IrZone {
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
            })
            .collect();

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

        // tbaMUD ships DG Scripts in place of CircleMUD's specprocs; emit one
        // Info note so the dry-run report makes the omission obvious.
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            SourceLoc::file(world_root.clone()),
            "tbaMUD uses DG Scripts; CircleMUD spec_assign.c parsing skipped".to_string(),
        ));

        Ok((
            ImportIR {
                zones,
                triggers: Vec::new(),
                dg_triggers,
                quests,
            },
            warnings,
        ))
    }
}

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
    Err(anyhow!(
        "no wld/ + zon/ directories found under {}",
        source.display()
    ))
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
