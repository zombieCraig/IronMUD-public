//! Spawn tick system for IronMUD
//!
//! Handles respawning of mobiles and items at spawn points.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    ItemType, SharedConnections, SpawnDestination, SpawnEntityType, SpawnPointData, broadcast_to_builders, db,
    spawn::apply_spawn_dependencies,
};

/// Spawn tick interval in seconds
pub const SPAWN_TICK_INTERVAL_SECS: u64 = 30;

/// Background task that processes spawn points periodically
pub async fn run_spawn_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SPAWN_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("spawn");

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

        // Ranvier `replaceOnRespawn` semantic: when set, force-delete every
        // tracked alive instance so the spawn path below re-creates them
        // fresh (refreshed inventory/equipment/container contents).
        if sp.replace_on_respawn && !sp.spawned_entities.is_empty() {
            for entity_id in std::mem::take(&mut sp.spawned_entities) {
                match sp.entity_type {
                    SpawnEntityType::Mobile => {
                        let _ = db.delete_mobile(&entity_id);
                    }
                    SpawnEntityType::Item => {
                        // Recursive: a container being replaced needs its
                        // contents cleaned up too, or unique items inside
                        // get orphaned and keep their world cap full
                        // (e.g. pirates_chest re-burying with pirate_cutlass).
                        let _ = db.delete_item_recursive(&entity_id);
                    }
                }
            }
            db.save_spawn_point(sp.clone())?;
        }

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
                    let _ = ironmud::script::fire_mobile_triggers_from_rust(
                        db,
                        connections,
                        &mobile.id.to_string(),
                        "on_load",
                        "",
                        &std::collections::HashMap::new(),
                    );
                    Some(mobile.id)
                }),
                SpawnEntityType::Item => db.spawn_item_from_prototype(&sp.vnum)?.and_then(|item| {
                    let item_id = item.id;
                    db.move_item_to_room(&item_id, &sp.room_id).ok();
                    if sp.bury_on_spawn {
                        if let Ok(Some(mut spawned)) = db.get_item_data(&item_id) {
                            spawned.flags.buried = true;
                            let _ = db.save_item_data(spawned);
                        }
                    }
                    if let Ok(Some(loaded)) = db.get_item_data(&item_id) {
                        let db_arc = std::sync::Arc::new(db.clone());
                        ironmud::script::dg::fire_item_dg_triggers(
                            &db_arc,
                            connections,
                            &loaded,
                            ironmud::ItemTriggerType::OnLoad,
                            "",
                        );
                    }
                    Some(item_id)
                }),
            }
        } else {
            None
        };

        if let Some(entity_id) = spawned_id {
            let dep_success_count = apply_spawn_dependencies(db, connections, &sp, &entity_id);
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
