// src/script/simulation.rs
// NPC needs simulation system Rhai API

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::{NeedsState, SimGoal, SimulationConfig};

/// Register simulation-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // Register SimulationConfig type with getters/setters
    engine.register_type_with_name::<SimulationConfig>("SimulationConfig")
        .register_get("home_room_vnum", |c: &mut SimulationConfig| c.home_room_vnum.clone())
        .register_set("home_room_vnum", |c: &mut SimulationConfig, v: String| c.home_room_vnum = v)
        .register_get("work_room_vnum", |c: &mut SimulationConfig| c.work_room_vnum.clone())
        .register_set("work_room_vnum", |c: &mut SimulationConfig, v: String| c.work_room_vnum = v)
        .register_get("shop_room_vnum", |c: &mut SimulationConfig| c.shop_room_vnum.clone())
        .register_set("shop_room_vnum", |c: &mut SimulationConfig, v: String| c.shop_room_vnum = v)
        .register_get("preferred_food_vnum", |c: &mut SimulationConfig| c.preferred_food_vnum.clone())
        .register_set("preferred_food_vnum", |c: &mut SimulationConfig, v: String| c.preferred_food_vnum = v)
        .register_get("work_pay", |c: &mut SimulationConfig| c.work_pay as i64)
        .register_set("work_pay", |c: &mut SimulationConfig, v: i64| c.work_pay = v as i32)
        .register_get("work_start_hour", |c: &mut SimulationConfig| c.work_start_hour as i64)
        .register_set("work_start_hour", |c: &mut SimulationConfig, v: i64| c.work_start_hour = v as u8)
        .register_get("work_end_hour", |c: &mut SimulationConfig| c.work_end_hour as i64)
        .register_set("work_end_hour", |c: &mut SimulationConfig, v: i64| c.work_end_hour = v as u8)
        .register_get("hunger_decay_rate", |c: &mut SimulationConfig| c.hunger_decay_rate as i64)
        .register_set("hunger_decay_rate", |c: &mut SimulationConfig, v: i64| c.hunger_decay_rate = v as i32)
        .register_get("energy_decay_rate", |c: &mut SimulationConfig| c.energy_decay_rate as i64)
        .register_set("energy_decay_rate", |c: &mut SimulationConfig, v: i64| c.energy_decay_rate = v as i32)
        .register_get("comfort_decay_rate", |c: &mut SimulationConfig| c.comfort_decay_rate as i64)
        .register_set("comfort_decay_rate", |c: &mut SimulationConfig, v: i64| c.comfort_decay_rate = v as i32)
        .register_get("low_gold_threshold", |c: &mut SimulationConfig| c.low_gold_threshold as i64)
        .register_set("low_gold_threshold", |c: &mut SimulationConfig, v: i64| c.low_gold_threshold = v as i32);

    // Register NeedsState type with getters
    engine.register_type_with_name::<NeedsState>("NeedsState")
        .register_get("hunger", |n: &mut NeedsState| n.hunger as i64)
        .register_get("energy", |n: &mut NeedsState| n.energy as i64)
        .register_get("comfort", |n: &mut NeedsState| n.comfort as i64)
        .register_get("current_goal", |n: &mut NeedsState| n.current_goal.to_display_string())
        .register_get("paid_this_shift", |n: &mut NeedsState| n.paid_this_shift);

    // Register SimGoal type with getter
    engine.register_type_with_name::<SimGoal>("SimGoal")
        .register_get("name", |g: &mut SimGoal| g.to_display_string());

    // ========== Configuration Functions ==========

    // set_npc_simulation(mobile_id, home_vnum, work_vnum, shop_vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_simulation", move |mobile_id: String, home_vnum: String, work_vnum: String, shop_vnum: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        mobile.simulation = Some(SimulationConfig {
            home_room_vnum: home_vnum,
            work_room_vnum: work_vnum,
            shop_room_vnum: shop_vnum,
            preferred_food_vnum: String::new(),
            work_pay: 50,
            work_start_hour: 8,
            work_end_hour: 17,
            hunger_decay_rate: 0,
            energy_decay_rate: 0,
            comfort_decay_rate: 0,
            low_gold_threshold: 10,
        });
        mobile.needs = Some(NeedsState::default());
        cloned_db.save_mobile_data(mobile).is_ok()
    });

    // remove_npc_simulation(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_npc_simulation", move |mobile_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        mobile.simulation = None;
        mobile.needs = None;
        cloned_db.save_mobile_data(mobile).is_ok()
    });

    // set_npc_work_pay(mobile_id, amount) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_work_pay", move |mobile_id: String, amount: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut config) = mobile.simulation {
            config.work_pay = amount as i32;
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // set_npc_low_gold_threshold(mobile_id, threshold) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_low_gold_threshold", move |mobile_id: String, threshold: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut config) = mobile.simulation {
            config.low_gold_threshold = (threshold as i32).max(0);
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // set_npc_work_hours(mobile_id, start_hour, end_hour) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_work_hours", move |mobile_id: String, start_hour: i64, end_hour: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut config) = mobile.simulation {
            config.work_start_hour = start_hour as u8;
            config.work_end_hour = end_hour as u8;
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // set_npc_preferred_food(mobile_id, food_vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_preferred_food", move |mobile_id: String, food_vnum: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut config) = mobile.simulation {
            config.preferred_food_vnum = food_vnum;
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // set_npc_decay_rate(mobile_id, need_name, rate) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_decay_rate", move |mobile_id: String, need_name: String, rate: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut config) = mobile.simulation {
            match need_name.to_lowercase().as_str() {
                "hunger" => config.hunger_decay_rate = rate as i32,
                "energy" => config.energy_decay_rate = rate as i32,
                "comfort" => config.comfort_decay_rate = rate as i32,
                _ => return false,
            }
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // ========== Query Functions ==========

    // is_npc_simulated(mobile_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_npc_simulated", move |mobile_id: String| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m.simulation.is_some(),
            _ => false,
        }
    });

    // get_npc_needs(mobile_id) -> Map { hunger, energy, comfort, goal } or ()
    let cloned_db = db.clone();
    engine.register_fn("get_npc_needs", move |mobile_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return rhai::Dynamic::UNIT,
        };
        match mobile.needs {
            Some(ref needs) => {
                let mut map = rhai::Map::new();
                map.insert("hunger".into(), rhai::Dynamic::from(needs.hunger as i64));
                map.insert("energy".into(), rhai::Dynamic::from(needs.energy as i64));
                map.insert("comfort".into(), rhai::Dynamic::from(needs.comfort as i64));
                map.insert("goal".into(), rhai::Dynamic::from(needs.current_goal.to_display_string()));
                map.insert("paid".into(), rhai::Dynamic::from(needs.paid_this_shift));
                rhai::Dynamic::from(map)
            }
            None => rhai::Dynamic::UNIT,
        }
    });

    // get_npc_goal(mobile_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_npc_goal", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => {
                match m.needs {
                    Some(ref needs) => needs.current_goal.to_display_string(),
                    None => String::new(),
                }
            }
            _ => String::new(),
        }
    });

    // get_npc_simulation_config(mobile_id) -> Map or ()
    let cloned_db = db.clone();
    engine.register_fn("get_npc_simulation_config", move |mobile_id: String| -> rhai::Dynamic {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return rhai::Dynamic::UNIT,
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return rhai::Dynamic::UNIT,
        };
        match mobile.simulation {
            Some(ref config) => {
                let mut map = rhai::Map::new();
                map.insert("home_room_vnum".into(), rhai::Dynamic::from(config.home_room_vnum.clone()));
                map.insert("work_room_vnum".into(), rhai::Dynamic::from(config.work_room_vnum.clone()));
                map.insert("shop_room_vnum".into(), rhai::Dynamic::from(config.shop_room_vnum.clone()));
                map.insert("preferred_food_vnum".into(), rhai::Dynamic::from(config.preferred_food_vnum.clone()));
                map.insert("work_pay".into(), rhai::Dynamic::from(config.work_pay as i64));
                map.insert("work_start_hour".into(), rhai::Dynamic::from(config.work_start_hour as i64));
                map.insert("work_end_hour".into(), rhai::Dynamic::from(config.work_end_hour as i64));
                map.insert("hunger_decay_rate".into(), rhai::Dynamic::from(config.hunger_decay_rate as i64));
                map.insert("energy_decay_rate".into(), rhai::Dynamic::from(config.energy_decay_rate as i64));
                map.insert("comfort_decay_rate".into(), rhai::Dynamic::from(config.comfort_decay_rate as i64));
                map.insert("low_gold_threshold".into(), rhai::Dynamic::from(config.low_gold_threshold as i64));
                rhai::Dynamic::from(map)
            }
            None => rhai::Dynamic::UNIT,
        }
    });

    // ========== Override Functions ==========

    // set_npc_need(mobile_id, need_name, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_npc_need", move |mobile_id: String, need_name: String, value: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut needs) = mobile.needs {
            let clamped = (value as i32).clamp(0, 100);
            match need_name.to_lowercase().as_str() {
                "hunger" => needs.hunger = clamped,
                "energy" => needs.energy = clamped,
                "comfort" => needs.comfort = clamped,
                _ => return false,
            }
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // boost_npc_need(mobile_id, need_name, amount) -> bool
    let cloned_db = db.clone();
    engine.register_fn("boost_npc_need", move |mobile_id: String, need_name: String, amount: i64| -> bool {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let mut mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        if let Some(ref mut needs) = mobile.needs {
            match need_name.to_lowercase().as_str() {
                "hunger" => needs.hunger = (needs.hunger + amount as i32).clamp(0, 100),
                "energy" => needs.energy = (needs.energy + amount as i32).clamp(0, 100),
                "comfort" => needs.comfort = (needs.comfort + amount as i32).clamp(0, 100),
                _ => return false,
            }
            return cloned_db.save_mobile_data(mobile).is_ok();
        }
        false
    });

    // ========== Dialogue Functions ==========

    // get_npc_needs_dialogue(mobile_id) -> String
    // Returns a dynamic dialogue string based on current needs
    let cloned_db = db.clone();
    engine.register_fn("get_npc_needs_dialogue", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return String::new(),
        };
        let needs = match mobile.needs {
            Some(ref n) => n,
            None => return String::new(),
        };

        build_needs_dialogue(needs, &mobile.gold)
    });

    // get_npc_visual_cues(mobile_id) -> String
    // Returns appearance hints based on needs state
    let cloned_db = db.clone();
    engine.register_fn("get_npc_visual_cues", move |mobile_id: String| -> String {
        let uuid = match uuid::Uuid::parse_str(&mobile_id) {
            Ok(u) => u,
            Err(_) => return String::new(),
        };
        let mobile = match cloned_db.get_mobile_data(&uuid) {
            Ok(Some(m)) => m,
            _ => return String::new(),
        };
        let needs = match mobile.needs {
            Some(ref n) => n,
            None => return String::new(),
        };

        build_visual_cues(needs, mobile.gold)
    });
}

/// Build a natural dialogue response based on NPC needs
fn build_needs_dialogue(needs: &NeedsState, gold: &i32) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Most urgent need first
    match needs.hunger {
        0..=15 => parts.push("I... I can't remember my last meal.".to_string()),
        16..=30 => parts.push("I'm starving, I need to find some food.".to_string()),
        31..=50 => parts.push("I'm getting pretty hungry...".to_string()),
        51..=80 => parts.push("I could eat soon.".to_string()),
        _ => parts.push("I just ate, feeling great!".to_string()),
    }

    match needs.energy {
        0..=15 => parts.push("I can barely keep my eyes open...".to_string()),
        16..=30 => parts.push("I'm exhausted, I need to rest.".to_string()),
        31..=50 => parts.push("I'm a bit tired.".to_string()),
        _ => {}
    }

    match needs.comfort {
        0..=20 => parts.push("I'm miserable, I just want to go home.".to_string()),
        21..=40 => parts.push("I could use some time at home.".to_string()),
        _ => {}
    }

    // Gold awareness
    if *gold <= 0 {
        parts.push("I can't even afford a meal right now.".to_string());
    }

    // Goal awareness
    match needs.current_goal {
        SimGoal::GoingToWork => parts.push("I need to get to work.".to_string()),
        SimGoal::Working => parts.push("Just trying to get through the work day.".to_string()),
        SimGoal::SeekFood => parts.push("I'm heading to get something to eat.".to_string()),
        SimGoal::GoingHome => parts.push("I'm heading home.".to_string()),
        _ => {}
    }

    if parts.is_empty() {
        return "I'm doing well, thanks for asking!".to_string();
    }

    parts.join(" ")
}

/// Build visual cue text for examine command
fn build_visual_cues(needs: &NeedsState, gold: i32) -> String {
    let mut cues: Vec<&str> = Vec::new();

    if needs.hunger <= 30 {
        cues.push("They look gaunt and underfed.");
    }
    if needs.energy <= 30 {
        cues.push("Dark circles under their eyes suggest exhaustion.");
    }
    if needs.comfort <= 30 {
        cues.push("They seem restless and on edge.");
    }
    if needs.hunger > 80 && needs.energy > 80 {
        cues.push("They look healthy and well-rested.");
    }
    if gold <= 0 {
        cues.push("Their pockets look empty.");
    }

    cues.join(" ")
}
