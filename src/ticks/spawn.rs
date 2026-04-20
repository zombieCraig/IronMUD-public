//! Spawn tick system for IronMUD
//!
//! Handles respawning of mobiles and items at spawn points.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    ItemType, SharedConnections, SpawnDestination, SpawnEntityType, SpawnPointData, broadcast_to_builders, db,
};

/// Spawn tick interval in seconds
pub const SPAWN_TICK_INTERVAL_SECS: u64 = 30;

/// Background task that processes spawn points periodically
pub async fn run_spawn_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SPAWN_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_spawn_points(&db, &connections) {
            error!("Spawn tick error: {}", e);
        }
    }
}

/// Refill container dependencies for existing spawned containers
fn refill_container_dependencies(db: &db::Db, connections: &SharedConnections, sp: &SpawnPointData) -> Result<()> {
    // Only process dependencies with Container destination
    let container_deps: Vec<_> = sp
        .dependencies
        .iter()
        .filter(|d| matches!(d.destination, SpawnDestination::Container))
        .collect();

    if container_deps.is_empty() {
        return Ok(());
    }

    // Check each spawned container
    for container_id in &sp.spawned_entities {
        // Verify container still exists and is actually a container
        let _container = match db.get_item_data(container_id)? {
            Some(c) if c.item_type == ItemType::Container => c,
            _ => continue, // Container gone or not a container
        };

        // Get current contents
        let contents = db.get_items_in_container(container_id)?;

        // For each container dependency, check if refill needed
        for dep in &container_deps {
            // Count items with matching vnum already in container
            let current_count = contents
                .iter()
                .filter(|item| item.vnum.as_ref() == Some(&dep.item_vnum))
                .count() as i32;

            // Spawn items to reach the target count
            let needed = dep.count - current_count;
            for _ in 0..needed {
                match db.spawn_item_from_prototype(&dep.item_vnum) {
                    Ok(Some(item)) => {
                        if let Err(e) = db.move_item_to_container(&item.id, container_id) {
                            broadcast_to_builders(connections, &format!("Container refill error: {}", e));
                            let _ = db.delete_item(&item.id);
                        }
                    }
                    Ok(None) => {
                        // Prototype not found - already warned during initial spawn
                    }
                    Err(e) => {
                        broadcast_to_builders(connections, &format!("Container refill spawn error: {}", e));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Process all spawn points, respawning entities as needed
fn process_spawn_points(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let spawn_points = db.list_all_spawn_points()?;

    for sp in spawn_points {
        if !sp.enabled {
            continue;
        }

        // Check if enough time has passed
        if now < sp.last_spawn_time + sp.respawn_interval_secs {
            continue;
        }

        // Clean up dead entity references first
        db.cleanup_spawn_point_refs(&sp.id)?;

        // Reload spawn point after cleanup
        let mut sp = match db.get_spawn_point(&sp.id)? {
            Some(s) => s,
            None => continue,
        };

        // Check current count - only spawn new entity if below max
        let current_count = sp.spawned_entities.len() as i32;

        // Also count existing entities of the same vnum in the room
        // to prevent duplicates from manual spawns or untracked entities
        let existing_in_room = match sp.entity_type {
            SpawnEntityType::Mobile => db
                .get_mobiles_in_room(&sp.room_id)
                .unwrap_or_default()
                .iter()
                .filter(|m| m.vnum == sp.vnum)
                .count() as i32,
            SpawnEntityType::Item => db
                .get_items_in_room(&sp.room_id)
                .unwrap_or_default()
                .iter()
                .filter(|i| i.vnum.as_deref() == Some(&sp.vnum))
                .count() as i32,
        };

        let needs_new_spawn = current_count < sp.max_count && existing_in_room < sp.max_count;

        // Spawn new entity if needed
        let spawned_id = if needs_new_spawn {
            match sp.entity_type {
                SpawnEntityType::Mobile => db.spawn_mobile_from_prototype(&sp.vnum)?.and_then(|mobile| {
                    db.move_mobile_to_room(&mobile.id, &sp.room_id).ok();
                    Some(mobile.id)
                }),
                SpawnEntityType::Item => db.spawn_item_from_prototype(&sp.vnum)?.and_then(|item| {
                    db.move_item_to_room(&item.id, &sp.room_id).ok();
                    Some(item.id)
                }),
            }
        } else {
            None
        };

        if let Some(entity_id) = spawned_id {
            // Process spawn dependencies
            let mut dep_success_count = 0;
            for dep in &sp.dependencies {
                // Roll chance - skip this dependency if the roll fails
                if dep.chance < 100 {
                    use rand::Rng;
                    let roll: i32 = rand::thread_rng().gen_range(1..=100);
                    if roll > dep.chance {
                        continue;
                    }
                }
                for _ in 0..dep.count {
                    // Spawn the dependency item from prototype
                    match db.spawn_item_from_prototype(&dep.item_vnum) {
                        Ok(Some(item)) => {
                            let item_id = item.id;
                            let result = match &dep.destination {
                                SpawnDestination::Inventory => db.move_item_to_mobile_inventory(&item_id, &entity_id),
                                SpawnDestination::Equipped(wear_loc) => {
                                    // Validate the item can be worn at this location
                                    if !item.wear_locations.contains(wear_loc) {
                                        broadcast_to_builders(
                                            connections,
                                            &format!(
                                                "Spawn warning: Item '{}' cannot be equipped at {:?} (not in wear_locations)",
                                                dep.item_vnum, wear_loc
                                            ),
                                        );
                                        // Delete the spawned item since we can't use it
                                        let _ = db.delete_item(&item_id);
                                        continue;
                                    }
                                    db.move_item_to_mobile_equipped(&item_id, &entity_id)
                                }
                                SpawnDestination::Container => {
                                    // For container destination, entity_id should be an item (container)
                                    match db.move_item_to_container(&item_id, &entity_id) {
                                        Ok(_) => Ok(true),
                                        Err(e) => {
                                            broadcast_to_builders(
                                                connections,
                                                &format!(
                                                    "Spawn warning: Cannot put item '{}' in container: {}",
                                                    dep.item_vnum, e
                                                ),
                                            );
                                            // Delete the spawned item since we can't use it
                                            let _ = db.delete_item(&item_id);
                                            continue;
                                        }
                                    }
                                }
                            };

                            match result {
                                Ok(true) => dep_success_count += 1,
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
                                &format!(
                                    "Spawn warning: Item prototype '{}' not found for spawn point {}",
                                    dep.item_vnum, sp.id
                                ),
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

            sp.spawned_entities.push(entity_id);

            if dep_success_count > 0 {
                debug!(
                    "Spawned {} with {} dependency items at spawn point {}",
                    sp.vnum, dep_success_count, sp.id
                );
            } else {
                debug!("Spawned {} at spawn point {}", sp.vnum, sp.id);
            }
        }

        // Refill container dependencies for existing Item spawn points
        if sp.entity_type == SpawnEntityType::Item {
            refill_container_dependencies(db, connections, &sp)?;
        }

        // Update last_spawn_time for this spawn point (whether we spawned or just refilled)
        sp.last_spawn_time = now;
        db.save_spawn_point(sp)?;
    }

    Ok(())
}
