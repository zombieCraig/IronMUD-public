//! Spawn-time helpers shared across reset paths.
//!
//! `apply_spawn_dependencies` materializes a spawn point's `dependencies`
//! (CircleMUD G/E/P resets) onto a freshly-spawned entity. It's called by:
//!
//! - the periodic spawn tick (`src/ticks/spawn.rs::process_spawn_points`),
//! - the Rhai `trigger_area_reset` binding (`src/script/spawn.rs`),
//! - the `POST /api/areas/<id>/reset` handler (`src/api/areas.rs`).
//!
//! Keeping the routing logic here means all three reset entry points
//! produce the same equipment/inventory/container state.

use crate::db::Db;
use crate::session::broadcast::broadcast_to_builders;
use crate::types::{SpawnDestination, SpawnPointData};
use crate::SharedConnections;
use uuid::Uuid;

/// Apply a spawn point's dependencies to a just-spawned entity.
///
/// `entity_id` is a mobile id for `Inventory` / `Equipped` deps and an
/// item id (the container) for `Container` deps. Returns the number of
/// deps successfully placed.
pub fn apply_spawn_dependencies(
    db: &Db,
    connections: &SharedConnections,
    sp: &SpawnPointData,
    entity_id: &Uuid,
) -> usize {
    let mut success_count = 0usize;
    for dep in &sp.dependencies {
        if dep.chance < 100 {
            use rand::Rng;
            let roll: i32 = rand::thread_rng().gen_range(1..=100);
            if roll > dep.chance {
                continue;
            }
        }
        for _ in 0..dep.count {
            match db.spawn_item_from_prototype(&dep.item_vnum) {
                Ok(Some(item)) => {
                    let item_id = item.id;
                    let result = match &dep.destination {
                        SpawnDestination::Inventory => db.move_item_to_mobile_inventory(&item_id, entity_id),
                        SpawnDestination::Equipped(wear_loc) => {
                            if !item.wear_locations.contains(wear_loc) {
                                broadcast_to_builders(
                                    connections,
                                    &format!(
                                        "Spawn warning: Item '{}' cannot be equipped at {:?} (not in wear_locations)",
                                        dep.item_vnum, wear_loc
                                    ),
                                );
                                let _ = db.delete_item(&item_id);
                                continue;
                            }
                            db.move_item_to_mobile_equipped_at(&item_id, entity_id, Some(*wear_loc))
                        }
                        SpawnDestination::Container => match db.move_item_to_container(&item_id, entity_id) {
                            Ok(_) => Ok(true),
                            Err(e) => {
                                broadcast_to_builders(
                                    connections,
                                    &format!("Spawn warning: Cannot put item '{}' in container: {}", dep.item_vnum, e),
                                );
                                let _ = db.delete_item(&item_id);
                                continue;
                            }
                        },
                    };

                    match result {
                        Ok(true) => success_count += 1,
                        Ok(false) => {
                            broadcast_to_builders(
                                connections,
                                &format!(
                                    "Spawn warning: Failed to place item '{}' for spawn point {}",
                                    dep.item_vnum, sp.id
                                ),
                            );
                            let _ = db.delete_item(&item_id);
                        }
                        Err(e) => {
                            broadcast_to_builders(
                                connections,
                                &format!("Spawn error: Failed to place item '{}': {}", dep.item_vnum, e),
                            );
                            let _ = db.delete_item(&item_id);
                        }
                    }
                }
                Ok(None) => {
                    broadcast_to_builders(
                        connections,
                        &format!("Spawn warning: Item prototype '{}' not found for spawn point {}", dep.item_vnum, sp.id),
                    );
                }
                Err(e) => {
                    broadcast_to_builders(
                        connections,
                        &format!("Spawn error: Failed to spawn item '{}': {}", dep.item_vnum, e),
                    );
                }
            }
        }
    }
    success_count
}
