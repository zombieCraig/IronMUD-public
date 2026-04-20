// src/script/garden.rs
// Gardening system functions: plant types, instances, and gardening actions

use crate::db::Db;
use crate::{GrowthStage, GrowthStageDef, InfestationType, PlantCategory, PlantInstance, PlantPrototype, Season};
use rhai::Engine;
use std::sync::Arc;

/// Register gardening-related types and functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Type Registration ==========

    // Register GrowthStageDef type
    engine
        .register_type_with_name::<GrowthStageDef>("GrowthStageDef")
        .register_get("stage", |s: &mut GrowthStageDef| {
            s.stage.to_display_string().to_string()
        })
        .register_get("duration_game_hours", |s: &mut GrowthStageDef| s.duration_game_hours)
        .register_get("description", |s: &mut GrowthStageDef| s.description.clone())
        .register_get("examine_desc", |s: &mut GrowthStageDef| s.examine_desc.clone());

    // Register PlantPrototype type
    engine
        .register_type_with_name::<PlantPrototype>("PlantPrototype")
        .register_get("id", |p: &mut PlantPrototype| p.id.to_string())
        .register_get("vnum", |p: &mut PlantPrototype| p.vnum.clone().unwrap_or_default())
        .register_get("name", |p: &mut PlantPrototype| p.name.clone())
        .register_set("name", |p: &mut PlantPrototype, v: String| p.name = v)
        .register_get("keywords", |p: &mut PlantPrototype| {
            p.keywords
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("seed_vnum", |p: &mut PlantPrototype| p.seed_vnum.clone())
        .register_set("seed_vnum", |p: &mut PlantPrototype, v: String| p.seed_vnum = v)
        .register_get("harvest_vnum", |p: &mut PlantPrototype| p.harvest_vnum.clone())
        .register_set("harvest_vnum", |p: &mut PlantPrototype, v: String| p.harvest_vnum = v)
        .register_get("harvest_min", |p: &mut PlantPrototype| p.harvest_min as i64)
        .register_set("harvest_min", |p: &mut PlantPrototype, v: i64| p.harvest_min = v as i32)
        .register_get("harvest_max", |p: &mut PlantPrototype| p.harvest_max as i64)
        .register_set("harvest_max", |p: &mut PlantPrototype, v: i64| p.harvest_max = v as i32)
        .register_get("category", |p: &mut PlantPrototype| {
            p.category.to_display_string().to_string()
        })
        .register_get("stages", |p: &mut PlantPrototype| {
            p.stages
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("preferred_seasons", |p: &mut PlantPrototype| {
            p.preferred_seasons
                .iter()
                .map(|s| rhai::Dynamic::from(format!("{}", s)))
                .collect::<Vec<_>>()
        })
        .register_get("forbidden_seasons", |p: &mut PlantPrototype| {
            p.forbidden_seasons
                .iter()
                .map(|s| rhai::Dynamic::from(format!("{}", s)))
                .collect::<Vec<_>>()
        })
        .register_get("water_consumption_per_hour", |p: &mut PlantPrototype| {
            p.water_consumption_per_hour
        })
        .register_get("water_capacity", |p: &mut PlantPrototype| p.water_capacity)
        .register_get("indoor_only", |p: &mut PlantPrototype| p.indoor_only)
        .register_get("min_skill_to_plant", |p: &mut PlantPrototype| {
            p.min_skill_to_plant as i64
        })
        .register_set("min_skill_to_plant", |p: &mut PlantPrototype, v: i64| {
            p.min_skill_to_plant = v as i32
        })
        .register_get("base_xp", |p: &mut PlantPrototype| p.base_xp as i64)
        .register_set("base_xp", |p: &mut PlantPrototype, v: i64| p.base_xp = v as i32)
        .register_get("pest_resistance", |p: &mut PlantPrototype| p.pest_resistance as i64)
        .register_set("pest_resistance", |p: &mut PlantPrototype, v: i64| {
            p.pest_resistance = v as i32
        })
        .register_get("multi_harvest", |p: &mut PlantPrototype| p.multi_harvest)
        .register_set("multi_harvest", |p: &mut PlantPrototype, v: bool| p.multi_harvest = v)
        .register_get("is_prototype", |p: &mut PlantPrototype| p.is_prototype);

    // Register PlantInstance type
    engine
        .register_type_with_name::<PlantInstance>("PlantInstance")
        .register_get("id", |p: &mut PlantInstance| p.id.to_string())
        .register_get("prototype_vnum", |p: &mut PlantInstance| p.prototype_vnum.clone())
        .register_get("room_id", |p: &mut PlantInstance| p.room_id.to_string())
        .register_get("planter_name", |p: &mut PlantInstance| p.planter_name.clone())
        .register_get("group_members", |p: &mut PlantInstance| {
            p.group_members
                .iter()
                .map(|s| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        })
        .register_get("stage", |p: &mut PlantInstance| p.stage.to_display_string().to_string())
        .register_get("stage_progress_hours", |p: &mut PlantInstance| p.stage_progress_hours)
        .register_get("water_level", |p: &mut PlantInstance| p.water_level)
        .register_get("health", |p: &mut PlantInstance| p.health)
        .register_get("fertilized", |p: &mut PlantInstance| p.fertilized)
        .register_get("fertilizer_hours_remaining", |p: &mut PlantInstance| {
            p.fertilizer_hours_remaining
        })
        .register_get("infestation", |p: &mut PlantInstance| {
            p.infestation.to_display_string().to_string()
        })
        .register_get("infestation_severity", |p: &mut PlantInstance| p.infestation_severity)
        .register_get("is_potted", |p: &mut PlantInstance| p.is_potted)
        .register_get("pot_item_id", |p: &mut PlantInstance| {
            p.pot_item_id.map(|u| u.to_string()).unwrap_or_default()
        })
        .register_get("times_harvested", |p: &mut PlantInstance| p.times_harvested as i64)
        .register_get("planted_at", |p: &mut PlantInstance| p.planted_at)
        .register_get("planted_game_month", |p: &mut PlantInstance| {
            p.planted_game_month as i64
        })
        .register_get("planted_game_year", |p: &mut PlantInstance| p.planted_game_year as i64);

    // ========== Prototype CRUD Functions ==========

    // get_plant_prototype_by_vnum(vnum) -> PlantPrototype or ()
    let cloned_db = db.clone();
    engine.register_fn("get_plant_prototype_by_vnum", move |vnum: String| {
        match cloned_db.get_plant_prototype_by_vnum(&vnum) {
            Ok(Some(proto)) => rhai::Dynamic::from(proto),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // list_plant_prototypes() -> Array of PlantPrototype
    let cloned_db = db.clone();
    engine.register_fn("list_plant_prototypes", move || {
        cloned_db
            .list_all_plant_prototypes()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect::<Vec<_>>()
    });

    // new_plant_prototype(name, vnum) -> PlantPrototype
    engine.register_fn("new_plant_prototype", |name: String, vnum: String| {
        PlantPrototype::new(name, vnum)
    });

    // save_plant_prototype(proto) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_plant_prototype", move |proto: PlantPrototype| {
        cloned_db.save_plant_prototype(proto).is_ok()
    });

    // delete_plant_prototype(id_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_plant_prototype", move |id_str: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
            cloned_db.delete_plant_prototype(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // get_plant_prototype(id_str) -> PlantPrototype or ()
    let cloned_db = db.clone();
    engine.register_fn("get_plant_prototype", move |id_str: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
            match cloned_db.get_plant_prototype(&uuid) {
                Ok(Some(proto)) => rhai::Dynamic::from(proto),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // set_plant_prototype_category(proto_id, category_str) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_plant_prototype_category",
        move |id_str: String, cat_str: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    if let Some(cat) = PlantCategory::from_str(&cat_str) {
                        proto.category = cat;
                        return cloned_db.save_plant_prototype(proto).is_ok();
                    }
                }
            }
            false
        },
    );

    // set_plant_prototype_indoor_only(proto_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_plant_prototype_indoor_only", move |id_str: String, value: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
            if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                proto.indoor_only = value;
                return cloned_db.save_plant_prototype(proto).is_ok();
            }
        }
        false
    });

    // set_plant_prototype_water(proto_id, consumption, capacity) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_plant_prototype_water",
        move |id_str: String, consumption: f64, capacity: f64| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    proto.water_consumption_per_hour = consumption;
                    proto.water_capacity = capacity;
                    return cloned_db.save_plant_prototype(proto).is_ok();
                }
            }
            false
        },
    );

    // add_plant_prototype_season(proto_id, season_str, list_type) -> bool
    // list_type: "preferred" or "forbidden"
    let cloned_db = db.clone();
    engine.register_fn(
        "add_plant_prototype_season",
        move |id_str: String, season_str: String, list_type: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    let season = match season_str.to_lowercase().as_str() {
                        "spring" => Season::Spring,
                        "summer" => Season::Summer,
                        "autumn" | "fall" => Season::Autumn,
                        "winter" => Season::Winter,
                        _ => return false,
                    };
                    match list_type.to_lowercase().as_str() {
                        "preferred" => {
                            if !proto.preferred_seasons.contains(&season) {
                                proto.preferred_seasons.push(season);
                            }
                        }
                        "forbidden" => {
                            if !proto.forbidden_seasons.contains(&season) {
                                proto.forbidden_seasons.push(season);
                            }
                        }
                        _ => return false,
                    }
                    return cloned_db.save_plant_prototype(proto).is_ok();
                }
            }
            false
        },
    );

    // remove_plant_prototype_season(proto_id, season_str, list_type) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_plant_prototype_season",
        move |id_str: String, season_str: String, list_type: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    let season = match season_str.to_lowercase().as_str() {
                        "spring" => Season::Spring,
                        "summer" => Season::Summer,
                        "autumn" | "fall" => Season::Autumn,
                        "winter" => Season::Winter,
                        _ => return false,
                    };
                    match list_type.to_lowercase().as_str() {
                        "preferred" => proto.preferred_seasons.retain(|s| *s != season),
                        "forbidden" => proto.forbidden_seasons.retain(|s| *s != season),
                        _ => return false,
                    }
                    return cloned_db.save_plant_prototype(proto).is_ok();
                }
            }
            false
        },
    );

    // add_plant_prototype_stage(proto_id, stage_str, duration, description, examine_desc) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_plant_prototype_stage",
        move |id_str: String, stage_str: String, duration: i64, desc: String, examine: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    if let Some(stage) = GrowthStage::from_str(&stage_str) {
                        // Remove existing stage def if present
                        proto.stages.retain(|s| s.stage != stage);
                        proto.stages.push(GrowthStageDef {
                            stage,
                            duration_game_hours: duration,
                            description: desc,
                            examine_desc: examine,
                        });
                        // Sort stages by growth order
                        let order = GrowthStage::living_stages();
                        proto
                            .stages
                            .sort_by_key(|s| order.iter().position(|o| *o == s.stage).unwrap_or(99));
                        return cloned_db.save_plant_prototype(proto).is_ok();
                    }
                }
            }
            false
        },
    );

    // remove_plant_prototype_stage(proto_id, index) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_plant_prototype_stage", move |id_str: String, index: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
            if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                let idx = index as usize;
                if idx < proto.stages.len() {
                    proto.stages.remove(idx);
                    return cloned_db.save_plant_prototype(proto).is_ok();
                }
            }
        }
        false
    });

    // add_plant_prototype_keyword(proto_id, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_plant_prototype_keyword", move |id_str: String, keyword: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
            if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                let kw_lower = keyword.to_lowercase();
                if !proto.keywords.contains(&kw_lower) {
                    proto.keywords.push(kw_lower);
                }
                return cloned_db.save_plant_prototype(proto).is_ok();
            }
        }
        false
    });

    // remove_plant_prototype_keyword(proto_id, keyword) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_plant_prototype_keyword",
        move |id_str: String, keyword: String| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                if let Ok(Some(mut proto)) = cloned_db.get_plant_prototype(&uuid) {
                    let kw_lower = keyword.to_lowercase();
                    proto.keywords.retain(|k| k != &kw_lower);
                    return cloned_db.save_plant_prototype(proto).is_ok();
                }
            }
            false
        },
    );

    // ========== Plant Instance Functions ==========

    // get_plant_data(plant_id) -> PlantInstance or ()
    let cloned_db = db.clone();
    engine.register_fn("get_plant_data", move |plant_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            match cloned_db.get_plant(&uuid) {
                Ok(Some(plant)) => rhai::Dynamic::from(plant),
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // save_plant_data(plant) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_plant_data", move |plant: PlantInstance| {
        cloned_db.save_plant(plant).is_ok()
    });

    // get_plants_in_room(room_id) -> Array of PlantInstance
    let cloned_db = db.clone();
    engine.register_fn("get_plants_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_plants_in_room(&uuid)
                .unwrap_or_default()
                .into_iter()
                .map(rhai::Dynamic::from)
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    });

    // create_plant_instance(proto_vnum, room_id, planter_name, is_potted, pot_item_id) -> String (new plant UUID)
    let cloned_db = db.clone();
    engine.register_fn(
        "create_plant_instance",
        move |proto_vnum: String,
              room_id_str: String,
              planter_name: String,
              is_potted: bool,
              pot_item_id_str: String| {
            let room_id = match uuid::Uuid::parse_str(&room_id_str) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            let pot_id = if pot_item_id_str.is_empty() {
                None
            } else {
                uuid::Uuid::parse_str(&pot_item_id_str).ok()
            };

            // Get game time for planted_game_month/year
            let (month, year) = match cloned_db.get_game_time() {
                Ok(gt) => (gt.month, gt.year),
                Err(_) => (1, 1),
            };

            let mut plant = PlantInstance::new(proto_vnum, room_id, planter_name, is_potted, pot_id);
            plant.planted_game_month = month;
            plant.planted_game_year = year;

            let id = plant.id.to_string();
            if cloned_db.save_plant(plant).is_ok() {
                id
            } else {
                String::new()
            }
        },
    );

    // delete_plant_instance(plant_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_plant_instance", move |plant_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            cloned_db.delete_plant(&uuid).unwrap_or(false)
        } else {
            false
        }
    });

    // count_plants_in_room(room_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("count_plants_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db.get_plants_in_room(&uuid).map(|p| p.len() as i64).unwrap_or(0)
        } else {
            0_i64
        }
    });

    // count_ground_plants_in_room(room_id) -> i64 (excludes potted)
    let cloned_db = db.clone();
    engine.register_fn("count_ground_plants_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            cloned_db
                .get_plants_in_room(&uuid)
                .map(|p| p.iter().filter(|plant| !plant.is_potted).count() as i64)
                .unwrap_or(0)
        } else {
            0_i64
        }
    });

    // ========== Gardening Action Functions ==========

    // water_plant(plant_id, amount) -> bool
    let cloned_db = db.clone();
    engine.register_fn("water_plant", move |plant_id: String, amount: f64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(mut plant)) = cloned_db.get_plant(&uuid) {
                // Get capacity from prototype
                let capacity = match cloned_db.get_plant_prototype_by_vnum(&plant.prototype_vnum) {
                    Ok(Some(proto)) => proto.water_capacity,
                    _ => 100.0,
                };
                plant.water_level = (plant.water_level + amount).min(capacity);
                return cloned_db.save_plant(plant).is_ok();
            }
        }
        false
    });

    // damage_plant(plant_id, amount) -> bool - Reduce plant health
    let cloned_db = db.clone();
    engine.register_fn("damage_plant", move |plant_id: String, amount: f64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(mut plant)) = cloned_db.get_plant(&uuid) {
                plant.health = (plant.health - amount).clamp(0.0, 100.0);
                return cloned_db.save_plant(plant).is_ok();
            }
        }
        false
    });

    // fertilize_plant(plant_id, duration_hours) -> bool
    let cloned_db = db.clone();
    engine.register_fn("fertilize_plant", move |plant_id: String, duration_hours: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(mut plant)) = cloned_db.get_plant(&uuid) {
                plant.fertilized = true;
                plant.fertilizer_hours_remaining = duration_hours as f64;
                return cloned_db.save_plant(plant).is_ok();
            }
        }
        false
    });

    // treat_plant_infestation(plant_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("treat_plant_infestation", move |plant_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(mut plant)) = cloned_db.get_plant(&uuid) {
                plant.infestation = InfestationType::None;
                plant.infestation_severity = 0.0;
                // Restore some health
                plant.health = (plant.health + 10.0).min(100.0);
                return cloned_db.save_plant(plant).is_ok();
            }
        }
        false
    });

    // harvest_plant(plant_id, skill_level) -> i64 (returns yield count, 0 on failure)
    let cloned_db = db.clone();
    engine.register_fn("harvest_plant", move |plant_id: String, skill_level: i64| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(mut plant)) = cloned_db.get_plant(&uuid) {
                if plant.stage != GrowthStage::Flowering {
                    return 0_i64;
                }

                let proto = match cloned_db.get_plant_prototype_by_vnum(&plant.prototype_vnum) {
                    Ok(Some(p)) => p,
                    _ => return 0_i64,
                };

                // Calculate yield
                let health_factor = plant.health / 100.0;
                let base_yield =
                    proto.harvest_min as f64 + health_factor * (proto.harvest_max - proto.harvest_min) as f64;
                let skill_bonus = skill_level as f64 * 0.1; // +10% per skill level
                let total_yield = ((base_yield * (1.0 + skill_bonus)).round() as i64).max(1);

                // Transition plant
                if proto.multi_harvest {
                    plant.stage = GrowthStage::Growing;
                    plant.stage_progress_hours = 0.0;
                    plant.times_harvested += 1;
                    let _ = cloned_db.save_plant(plant);
                } else {
                    plant.stage = GrowthStage::Dead;
                    plant.health = 0.0;
                    let _ = cloned_db.save_plant(plant);
                }

                return total_yield;
            }
        }
        0_i64
    });

    // get_plant_stage_description(plant_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_plant_stage_description", move |plant_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(plant)) = cloned_db.get_plant(&uuid) {
                if let Ok(Some(proto)) = cloned_db.get_plant_prototype_by_vnum(&plant.prototype_vnum) {
                    if let Some(stage_def) = proto.get_stage_def(&plant.stage) {
                        return stage_def.description.clone();
                    }
                    return format!("A {} ({}).", proto.name, plant.stage.to_display_string());
                }
            }
        }
        String::new()
    });

    // get_plant_examine_description(plant_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_plant_examine_description", move |plant_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&plant_id) {
            if let Ok(Some(plant)) = cloned_db.get_plant(&uuid) {
                if let Ok(Some(proto)) = cloned_db.get_plant_prototype_by_vnum(&plant.prototype_vnum) {
                    let examine = proto
                        .get_stage_def(&plant.stage)
                        .map(|s| s.examine_desc.clone())
                        .unwrap_or_else(|| {
                            format!("A {} in the {} stage.", proto.name, plant.stage.to_display_string())
                        });
                    return examine;
                }
            }
        }
        String::new()
    });

    // get_all_growth_stages() -> Array of stage name strings
    engine.register_fn("get_all_growth_stages", || {
        GrowthStage::all_names()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });

    // get_all_plant_categories() -> Array of category name strings
    engine.register_fn("get_all_plant_categories", || {
        PlantCategory::all_names()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });

    // get_all_infestation_types() -> Array of infestation type name strings
    engine.register_fn("get_all_infestation_types", || {
        InfestationType::all_names()
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect::<Vec<_>>()
    });
}
