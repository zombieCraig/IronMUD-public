//! Garden tick system for IronMUD
//!
//! Processes plant growth, water depletion, health effects, infestation,
//! and stage advancement. Uses elapsed-time calculation so plants grow
//! while offline.

use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::{debug, error};

use ironmud::{db, GrowthStage, InfestationType, SharedConnections, TemperatureCategory, WeatherCondition};

use super::broadcast::broadcast_to_room;

/// Garden tick interval - process every 120 seconds (aligned with time tick = 1 game hour)
pub const GARDEN_TICK_INTERVAL_SECS: u64 = 120;

/// Background task that processes garden growth
pub async fn run_garden_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(GARDEN_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_garden(&db, &connections) {
            error!("Garden tick error: {}", e);
        }
    }
}

/// Get rain water bonus per game hour based on weather condition
fn rain_bonus_per_hour(weather: &WeatherCondition) -> f64 {
    match weather {
        WeatherCondition::LightRain => 3.0,
        WeatherCondition::Rain => 6.0,
        WeatherCondition::HeavyRain | WeatherCondition::Thunderstorm => 10.0,
        WeatherCondition::LightSnow => 1.0,
        WeatherCondition::Snow => 2.0,
        WeatherCondition::Blizzard => 3.0,
        _ => 0.0,
    }
}

/// Check if the current weather can cause frost damage
fn is_frost_weather(temp_cat: &TemperatureCategory) -> bool {
    matches!(temp_cat, TemperatureCategory::Freezing | TemperatureCategory::Cold)
}

/// Process all plants: growth, water, health, infestations
fn process_garden(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Get current game time for season/weather
    let game_time = db.get_game_time()?;
    let current_season = game_time.get_season();
    let temp_cat = game_time.get_temperature_category();

    let plants = db.list_all_plants()?;
    if plants.is_empty() {
        return Ok(());
    }

    for mut plant in plants {
        // Skip dead plants (they remain for display until uprooted)
        if plant.stage == GrowthStage::Dead {
            continue;
        }

        // Calculate elapsed game hours since last update
        let elapsed_secs = (now - plant.last_update_timestamp).max(0) as f64;
        let elapsed_game_hours = elapsed_secs / GARDEN_TICK_INTERVAL_SECS as f64;

        if elapsed_game_hours <= 0.0 {
            continue;
        }

        // Load the prototype for this plant
        let proto = match db.get_plant_prototype_by_vnum(&plant.prototype_vnum)? {
            Some(p) => p,
            None => {
                debug!("Plant {} has no prototype (vnum: {}), skipping", plant.id, plant.prototype_vnum);
                continue;
            }
        };

        // Check if plant is outdoors (for weather effects)
        let is_outdoor = !plant.is_potted && {
            if let Ok(Some(room)) = db.get_room_data(&plant.room_id) {
                !room.flags.indoors && !room.flags.climate_controlled
            } else {
                false
            }
        };

        // === Water Depletion ===
        let water_drain = proto.water_consumption_per_hour * elapsed_game_hours;
        plant.water_level = (plant.water_level - water_drain).max(0.0);

        // Rain bonus for outdoor plants
        if is_outdoor {
            let rain = rain_bonus_per_hour(&game_time.weather) * elapsed_game_hours;
            if rain > 0.0 {
                plant.water_level = (plant.water_level + rain).min(proto.water_capacity);
            }
        }

        // === Health Effects ===
        let prev_health = plant.health;

        // Dehydration damage
        if plant.water_level <= 0.0 {
            plant.health -= 2.0 * elapsed_game_hours;
        } else if plant.water_level < 20.0 {
            plant.health -= 0.5 * elapsed_game_hours;
        }

        // Frost damage for outdoor plants in cold/freezing temps
        if is_outdoor && is_frost_weather(&temp_cat) && !plant.is_potted {
            let frost_dmg = match temp_cat {
                TemperatureCategory::Freezing => 3.0,
                TemperatureCategory::Cold => 1.0,
                _ => 0.0,
            };
            plant.health -= frost_dmg * elapsed_game_hours;
        }

        // Infestation damage (scaled by severity)
        if plant.infestation != InfestationType::None {
            let infestation_dmg = 1.5 * plant.infestation_severity * elapsed_game_hours;
            plant.health -= infestation_dmg;

            // Infestation severity grows over time if untreated
            plant.infestation_severity = (plant.infestation_severity + 0.02 * elapsed_game_hours).min(1.0);
        }

        // Clamp health
        plant.health = plant.health.clamp(0.0, 100.0);

        // === Death Check ===
        if plant.health <= 0.0 {
            plant.stage = GrowthStage::Dead;
            plant.health = 0.0;
            plant.last_update_timestamp = now;
            let _ = db.save_plant(plant.clone());

            // Broadcast death message
            let name = proto.name.clone();
            broadcast_to_room(connections, &plant.room_id,
                &format!("A {} withers and dies.", name));
            debug!("Plant {} ({}) died", plant.id, proto.name);
            continue;
        }

        // Broadcast health warning if crossing thresholds
        if prev_health >= 30.0 && plant.health < 30.0 {
            broadcast_to_room(connections, &plant.room_id,
                &format!("A {} looks sickly and near death.", proto.name));
        }

        // === Growth Calculation ===
        if plant.stage != GrowthStage::Wilting && plant.stage != GrowthStage::Dead {
            let mut growth_hours = elapsed_game_hours;

            // Season modifier
            if proto.forbidden_seasons.contains(&current_season) {
                growth_hours = 0.0;
            } else if proto.preferred_seasons.contains(&current_season) {
                growth_hours *= 1.25;
            }

            // Fertilizer modifier
            if plant.fertilized && plant.fertilizer_hours_remaining > 0.0 {
                growth_hours *= 1.5;
                plant.fertilizer_hours_remaining = (plant.fertilizer_hours_remaining - elapsed_game_hours).max(0.0);
                if plant.fertilizer_hours_remaining <= 0.0 {
                    plant.fertilized = false;
                }
            }

            // Water modifier
            let water_pct = plant.water_level / proto.water_capacity * 100.0;
            if water_pct < 10.0 {
                growth_hours *= 0.1;
            } else if water_pct < 30.0 {
                growth_hours *= 0.5;
            }

            // Health modifier (only applies when below 50%)
            if plant.health < 50.0 {
                growth_hours *= plant.health / 100.0;
            }

            // Accumulate progress
            plant.stage_progress_hours += growth_hours;

            // === Stage Advancement ===
            let stage_def = proto.get_stage_def(&plant.stage);
            let stage_duration = stage_def
                .map(|s| s.duration_game_hours as f64)
                .unwrap_or(24.0); // Default 24 game hours if no def

            if plant.stage_progress_hours >= stage_duration {
                if let Some(next_stage) = plant.stage.next() {
                    let old_stage = plant.stage;
                    plant.stage = next_stage;
                    plant.stage_progress_hours = 0.0;

                    // Broadcast stage advancement
                    let stage_desc = proto.get_stage_def(&next_stage)
                        .map(|s| s.description.clone())
                        .unwrap_or_else(|| format!("A {} is now {}.", proto.name, next_stage.to_display_string()));
                    broadcast_to_room(connections, &plant.room_id, &stage_desc);
                    debug!("Plant {} ({}) advanced from {} to {}", plant.id, proto.name,
                        old_stage.to_display_string(), next_stage.to_display_string());
                }
            }
        }

        // === Flowering Decay ===
        // Unharvested Flowering plants eventually wilt (after 48 game hours at Flowering)
        if plant.stage == GrowthStage::Flowering {
            let flowering_def = proto.get_stage_def(&GrowthStage::Flowering);
            let wilt_threshold = flowering_def
                .map(|s| s.duration_game_hours as f64)
                .unwrap_or(48.0);
            if plant.stage_progress_hours >= wilt_threshold {
                plant.stage = GrowthStage::Wilting;
                plant.stage_progress_hours = 0.0;
                broadcast_to_room(connections, &plant.room_id,
                    &format!("A {} begins to wilt, its blooms fading.", proto.name));
            }
        }

        // === Wilting -> Dead ===
        if plant.stage == GrowthStage::Wilting {
            plant.stage_progress_hours += elapsed_game_hours;
            // Wilt for 24 game hours before dying
            if plant.stage_progress_hours >= 24.0 {
                plant.stage = GrowthStage::Dead;
                plant.health = 0.0;
                broadcast_to_room(connections, &plant.room_id,
                    &format!("A {} has withered and died.", proto.name));
            }
        }

        // === Random Infestation ===
        // ~1% chance per game day (24 hours), so per tick: 1% * (elapsed/24)
        if plant.infestation == InfestationType::None
            && plant.stage != GrowthStage::Seed
            && plant.stage != GrowthStage::Dead
            && plant.stage != GrowthStage::Wilting
        {
            let chance_per_day = 0.01;
            let resistance_factor = 1.0 - (proto.pest_resistance as f64 / 100.0);
            let health_factor = if plant.health < 50.0 { 1.5 } else { 1.0 };
            let infestation_chance = chance_per_day * (elapsed_game_hours / 24.0) * resistance_factor * health_factor;

            // Use a simple deterministic pseudo-random based on plant id + timestamp
            let pseudo_rand = ((plant.id.as_u128() ^ now as u128) % 10000) as f64 / 10000.0;
            if pseudo_rand < infestation_chance {
                // Pick a random infestation type
                let types = [InfestationType::Aphids, InfestationType::Blight, InfestationType::RootRot, InfestationType::Frost];
                let idx = ((plant.id.as_u128() ^ (now as u128).wrapping_mul(7)) % types.len() as u128) as usize;
                plant.infestation = types[idx];
                plant.infestation_severity = 0.1;
                broadcast_to_room(connections, &plant.room_id,
                    &format!("A {} shows signs of {}!", proto.name, plant.infestation.to_display_string()));
            }
        }

        // Save updated plant
        plant.last_update_timestamp = now;
        let _ = db.save_plant(plant);
    }

    Ok(())
}
