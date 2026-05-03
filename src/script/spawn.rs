// src/script/spawn.rs
// Spawn point system functions

use crate::db::Db;
use crate::{SpawnDependency, SpawnDestination, SpawnEntityType, SpawnPointData, WearLocation};
use rhai::Engine;
use std::sync::Arc;

/// Register spawn point functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // Register SpawnPointData type with getters
    engine
        .register_type_with_name::<SpawnPointData>("SpawnPointData")
        .register_get("id", |sp: &mut SpawnPointData| sp.id.to_string())
        .register_get("area_id", |sp: &mut SpawnPointData| sp.area_id.to_string())
        .register_get("room_id", |sp: &mut SpawnPointData| sp.room_id.to_string())
        .register_get("entity_type", |sp: &mut SpawnPointData| match sp.entity_type {
            SpawnEntityType::Mobile => "mobile".to_string(),
            SpawnEntityType::Item => "item".to_string(),
        })
        .register_get("vnum", |sp: &mut SpawnPointData| sp.vnum.clone())
        .register_get("max_count", |sp: &mut SpawnPointData| sp.max_count as i64)
        .register_get("respawn_interval_secs", |sp: &mut SpawnPointData| {
            sp.respawn_interval_secs
        })
        .register_get("enabled", |sp: &mut SpawnPointData| sp.enabled)
        .register_get("last_spawn_time", |sp: &mut SpawnPointData| sp.last_spawn_time)
        .register_get("spawned_count", |sp: &mut SpawnPointData| {
            sp.spawned_entities.len() as i64
        })
        .register_get("bury_on_spawn", |sp: &mut SpawnPointData| sp.bury_on_spawn)
        .register_get("dependency_count", |sp: &mut SpawnPointData| {
            sp.dependencies.len() as i64
        });

    // Register SpawnDependency type with getters
    engine
        .register_type_with_name::<SpawnDependency>("SpawnDependency")
        .register_get("item_vnum", |dep: &mut SpawnDependency| dep.item_vnum.clone())
        .register_get("count", |dep: &mut SpawnDependency| dep.count as i64)
        .register_get("chance", |dep: &mut SpawnDependency| dep.chance as i64)
        .register_get("destination", |dep: &mut SpawnDependency| match &dep.destination {
            SpawnDestination::Inventory => "inventory".to_string(),
            SpawnDestination::Equipped(loc) => format!("equipped:{}", loc.to_display_string()),
            SpawnDestination::Container => "container".to_string(),
        });

    // create_spawn_point(area_id, room_id, entity_type, vnum, max_count, interval_secs) -> SpawnPointData or ()
    let cloned_db = db.clone();
    engine.register_fn(
        "create_spawn_point",
        move |area_id: String,
              room_id: String,
              entity_type: String,
              vnum: String,
              max_count: i64,
              interval_secs: i64| {
            let area_uuid = uuid::Uuid::parse_str(&area_id).ok();
            let room_uuid = uuid::Uuid::parse_str(&room_id).ok();

            let etype = match entity_type.to_lowercase().as_str() {
                "mobile" | "mob" => SpawnEntityType::Mobile,
                "item" | "object" => SpawnEntityType::Item,
                _ => return rhai::Dynamic::UNIT,
            };

            match (area_uuid, room_uuid) {
                (Some(aid), Some(rid)) => {
                    let sp = SpawnPointData {
                        id: uuid::Uuid::new_v4(),
                        area_id: aid,
                        room_id: rid,
                        entity_type: etype,
                        vnum,
                        max_count: max_count as i32,
                        respawn_interval_secs: interval_secs,
                        enabled: true,
                        last_spawn_time: 0,
                        spawned_entities: Vec::new(),
                        dependencies: Vec::new(),
                        bury_on_spawn: false,
                    };
                    if cloned_db.save_spawn_point(sp.clone()).is_ok() {
                        rhai::Dynamic::from(sp)
                    } else {
                        rhai::Dynamic::UNIT
                    }
                }
                _ => rhai::Dynamic::UNIT,
            }
        },
    );

    // get_spawn_point(spawn_point_id) -> SpawnPointData or ()
    let cloned_db = db.clone();
    engine.register_fn("get_spawn_point", move |spawn_point_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
            match cloned_db.get_spawn_point(&uuid) {
                Ok(Some(sp)) => rhai::Dynamic::from(sp),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // delete_spawn_point(spawn_point_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_spawn_point", move |spawn_point_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
            cloned_db.delete_spawn_point(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // list_spawn_points_in_area(area_id) -> Array of SpawnPointData
    let cloned_db = db.clone();
    engine.register_fn("list_spawn_points_in_area", move |area_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&area_id) {
            cloned_db
                .get_spawn_points_for_area(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // set_spawn_point_enabled(spawn_point_id, enabled) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_spawn_point_enabled",
        move |spawn_point_id: String, enabled: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    sp.enabled = enabled;
                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // set_spawn_point_max_count(spawn_point_id, max_count) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_spawn_point_max_count",
        move |spawn_point_id: String, max_count: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    sp.max_count = max_count as i32;
                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // set_spawn_point_interval(spawn_point_id, interval_secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_spawn_point_interval",
        move |spawn_point_id: String, interval_secs: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    sp.respawn_interval_secs = interval_secs;
                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // set_spawn_point_bury_on_spawn(spawn_point_id, buried) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_spawn_point_bury_on_spawn",
        move |spawn_point_id: String, buried: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_point_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    sp.bury_on_spawn = buried;
                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // trigger_area_reset(area_id) -> i64 (returns number of entities spawned)
    let cloned_db = db.clone();
    engine.register_fn("trigger_area_reset", move |area_id: String| {
        if let Ok(area_uuid) = uuid::Uuid::parse_str(&area_id) {
            let spawn_points = cloned_db.get_spawn_points_for_area(&area_uuid).unwrap_or_default();
            let mut spawned_count = 0i64;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            for sp in spawn_points {
                if !sp.enabled {
                    continue;
                }

                // Cleanup dead refs
                let _ = cloned_db.cleanup_spawn_point_refs(&sp.id);

                // Reload
                let mut sp = match cloned_db.get_spawn_point(&sp.id) {
                    Ok(Some(s)) => s,
                    _ => continue,
                };

                // Count existing entities of the same vnum already in the room
                // to prevent duplicates from manual spawns or untracked entities
                let existing_in_room = match sp.entity_type {
                    SpawnEntityType::Mobile => cloned_db
                        .get_mobiles_in_room(&sp.room_id)
                        .unwrap_or_default()
                        .iter()
                        .filter(|m| m.vnum == sp.vnum)
                        .count() as i32,
                    SpawnEntityType::Item => cloned_db
                        .get_items_in_room(&sp.room_id)
                        .unwrap_or_default()
                        .iter()
                        .filter(|i| i.vnum.as_deref() == Some(&sp.vnum))
                        .count() as i32,
                };

                // Spawn up to max, considering both tracked and untracked entities
                let mut local_spawned = 0i32;
                while (sp.spawned_entities.len() as i32) < sp.max_count
                    && (existing_in_room + local_spawned) < sp.max_count
                {
                    let spawned_id =
                        match sp.entity_type {
                            SpawnEntityType::Mobile => cloned_db
                                .spawn_mobile_from_prototype(&sp.vnum)
                                .ok()
                                .flatten()
                                .and_then(|m| {
                                    let _ = cloned_db.move_mobile_to_room(&m.id, &sp.room_id);
                                    Some(m.id)
                                }),
                            SpawnEntityType::Item => cloned_db
                                .spawn_item_from_prototype(&sp.vnum)
                                .ok()
                                .flatten()
                                .and_then(|i| {
                                    let _ = cloned_db.move_item_to_room(&i.id, &sp.room_id);
                                    Some(i.id)
                                }),
                        };

                    if let Some(id) = spawned_id {
                        sp.spawned_entities.push(id);
                        spawned_count += 1;
                        local_spawned += 1;
                    } else {
                        break; // Failed to spawn, stop trying
                    }
                }

                sp.last_spawn_time = now;
                let _ = cloned_db.save_spawn_point(sp);
            }

            spawned_count
        } else {
            0
        }
    });

    // get_spawn_dependencies(spawn_id) -> Array of SpawnDependency
    let cloned_db = db.clone();
    engine.register_fn("get_spawn_dependencies", move |spawn_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_id) {
            if let Ok(Some(sp)) = cloned_db.get_spawn_point(&uuid) {
                return sp.dependencies.into_iter().map(rhai::Dynamic::from).collect::<Vec<_>>();
            }
        }
        vec![]
    });

    // add_spawn_dependency(spawn_id, item_vnum, destination_type, wear_location, count) -> bool
    // destination_type: "inventory", "equipped", "container"
    // wear_location: only used if destination_type is "equipped", e.g. "head", "mainhand"
    let cloned_db = db.clone();
    engine.register_fn(
        "add_spawn_dependency",
        move |spawn_id: String, item_vnum: String, dest_type: String, wear_loc: String, count: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    let destination = match dest_type.to_lowercase().as_str() {
                        "inventory" | "inv" => SpawnDestination::Inventory,
                        "equipped" | "equip" | "wear" => {
                            match WearLocation::from_str(&wear_loc) {
                                Some(loc) => SpawnDestination::Equipped(loc),
                                None => return false, // Invalid wear location
                            }
                        }
                        "container" | "contain" => SpawnDestination::Container,
                        _ => return false, // Invalid destination type
                    };

                    sp.dependencies.push(SpawnDependency {
                        item_vnum,
                        destination,
                        count: count.max(1) as i32,
                        chance: 100,
                    });

                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // add_spawn_dependency(spawn_id, item_vnum, destination_type, wear_location, count, chance) -> bool
    // 6-param overload with chance (1-100 percentage)
    let cloned_db = db.clone();
    engine.register_fn(
        "add_spawn_dependency",
        move |spawn_id: String, item_vnum: String, dest_type: String, wear_loc: String, count: i64, chance: i64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_id) {
                if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                    let destination = match dest_type.to_lowercase().as_str() {
                        "inventory" | "inv" => SpawnDestination::Inventory,
                        "equipped" | "equip" | "wear" => match WearLocation::from_str(&wear_loc) {
                            Some(loc) => SpawnDestination::Equipped(loc),
                            None => return false,
                        },
                        "container" | "contain" => SpawnDestination::Container,
                        _ => return false,
                    };

                    sp.dependencies.push(SpawnDependency {
                        item_vnum,
                        destination,
                        count: count.max(1) as i32,
                        chance: (chance as i32).clamp(1, 100),
                    });

                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
            false
        },
    );

    // remove_spawn_dependency(spawn_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_spawn_dependency", move |spawn_id: String, index: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_id) {
            if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                let idx = index as usize;
                if idx < sp.dependencies.len() {
                    sp.dependencies.remove(idx);
                    return cloned_db.save_spawn_point(sp).is_ok();
                }
            }
        }
        false
    });

    // clear_spawn_dependencies(spawn_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("clear_spawn_dependencies", move |spawn_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&spawn_id) {
            if let Ok(Some(mut sp)) = cloned_db.get_spawn_point(&uuid) {
                sp.dependencies.clear();
                return cloned_db.save_spawn_point(sp).is_ok();
            }
        }
        false
    });
}
