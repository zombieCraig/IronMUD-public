//! Post-apply patches: fields that don't live on the CircleMUD-shaped
//! [`crate::import::Plan`] get applied here as a second writer pass.
//!
//! Currently:
//! - `RoomData.coordinates` from Ranvier `coordinates: [x, y, z]`
//! - `SpawnPointData.replace_on_respawn` from Ranvier
//!   `room.npcs[].replaceOnRespawn` / `room.items[].replaceOnRespawn`

use anyhow::{Context, Result};

use super::ir::IrBundle;
use super::vnum_map::{Kind, VnumMap};
use crate::db::Db;

#[derive(Debug, Clone, Default)]
pub struct PostPatches {
    /// Room vnum strings → coordinates to stamp.
    pub room_coordinates: Vec<(String, (i32, i32, i32))>,
    /// (room_vnum, entity_vnum) pairs → replace_on_respawn flag to set.
    pub replace_on_respawn: Vec<(String, String)>,
}

pub fn collect_post_patches(bundle: &IrBundle, vnum_map: &VnumMap) -> PostPatches {
    let mut patches = PostPatches::default();
    for area in &bundle.areas {
        let prefix = area.name.to_lowercase();
        for room in &area.rooms {
            let Some(rv) = vnum_map.get(&area.name, Kind::Room, &room.id) else {
                continue;
            };
            let room_vnum = format!("{prefix}_{rv}");
            if let Some([x, y, z]) = room.coordinates {
                patches.room_coordinates.push((room_vnum.clone(), (x, y, z)));
            }
            for spawn in room.npcs.iter().chain(room.items.iter()) {
                if !spawn.replace_on_respawn {
                    continue;
                }
                let scoped = match spawn.id.split_once(':') {
                    Some((a, b)) => (a.to_string(), b.to_string()),
                    None => (area.name.clone(), spawn.id.clone()),
                };
                let kind = if room.npcs.iter().any(|s| std::ptr::eq(s, spawn)) {
                    Kind::Mobile
                } else {
                    Kind::Item
                };
                let Some(ev) = vnum_map.get(&scoped.0, kind, &scoped.1) else {
                    continue;
                };
                let entity_vnum = format!("{}_{ev}", scoped.0.to_lowercase());
                patches
                    .replace_on_respawn
                    .push((room_vnum.clone(), entity_vnum));
            }
        }
    }
    patches
}

pub fn apply_post_patches(db: &Db, patches: &PostPatches) -> Result<()> {
    // Coordinates: load each room by vnum, set, save.
    for (vnum, coords) in &patches.room_coordinates {
        let Some(mut room) = db
            .get_room_by_vnum(vnum)
            .with_context(|| format!("looking up room {vnum} for coordinates patch"))?
        else {
            continue;
        };
        room.coordinates = Some(*coords);
        db.save_room_data(room)
            .with_context(|| format!("saving room {vnum} after coordinates patch"))?;
    }

    // replace_on_respawn: scan all spawn points, match by (room_id, vnum).
    if patches.replace_on_respawn.is_empty() {
        return Ok(());
    }
    let all_spawns = db.list_all_spawn_points().context("listing spawn points")?;
    for sp in all_spawns {
        for (room_vnum, entity_vnum) in &patches.replace_on_respawn {
            // Resolve the room_vnum once per outer iteration: only patch
            // the spawn points whose room matches.
            let Some(target_room) = db
                .get_room_by_vnum(room_vnum)
                .with_context(|| format!("looking up room {room_vnum}"))?
            else {
                continue;
            };
            if sp.room_id != target_room.id {
                continue;
            }
            if &sp.vnum != entity_vnum {
                continue;
            }
            let mut updated = sp.clone();
            updated.replace_on_respawn = true;
            db.save_spawn_point(updated)
                .with_context(|| format!("patching replace_on_respawn for {entity_vnum} in {room_vnum}"))?;
        }
    }
    Ok(())
}
