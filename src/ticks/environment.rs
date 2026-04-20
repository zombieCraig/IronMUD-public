//! Environment tick systems for IronMUD
//!
//! Handles game time advancement, weather updates, and weather exposure effects.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::session::broadcast_to_builders;
use ironmud::{
    BodyPart, CharacterData, GameTime, Season, SharedConnections, TemperatureCategory, TimeOfDay, TriggerType,
    WeatherCondition, Wound, WoundLevel, WoundType, broadcast_to_all_players, broadcast_to_outdoor_players, db,
};

use super::broadcast::broadcast_to_room_except;
use super::character::calculate_character_insulation;

/// Time tick interval in seconds (2 real minutes = 1 game hour)
pub const TIME_TICK_INTERVAL_SECS: u64 = 120;

/// Interval for weather exposure tick (wet, cold, heat effects)
pub const EXPOSURE_TICK_INTERVAL_SECS: u64 = 30;

/// Background task that advances game time periodically
pub async fn run_time_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(TIME_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_time_tick(&db, &connections) {
            error!("Time tick error: {}", e);
        }
    }
}

/// Process game time advancement
fn process_time_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut game_time = db.get_game_time()?;
    let old_time_of_day = game_time.get_time_of_day();
    let old_weather = game_time.weather;
    let old_season = game_time.get_season();
    let old_month = game_time.month;

    // Advance time by 1 hour
    game_time.advance_hour();

    let new_time_of_day = game_time.get_time_of_day();
    let new_season = game_time.get_season();

    // Broadcast time-of-day transitions to players who can see outside
    if old_time_of_day != new_time_of_day {
        let message = get_time_transition_message(&new_time_of_day);
        broadcast_to_outdoor_players(db, connections, &format!("\n{}\n", message));

        // Fire OnTimeChange triggers
        let mut context = std::collections::HashMap::new();
        context.insert("old_time".to_string(), format!("{}", old_time_of_day));
        context.insert("new_time".to_string(), format!("{}", new_time_of_day));
        context.insert("is_dawn".to_string(), (new_time_of_day == TimeOfDay::Dawn).to_string());
        context.insert("is_dusk".to_string(), (new_time_of_day == TimeOfDay::Dusk).to_string());
        context.insert(
            "is_night".to_string(),
            (new_time_of_day == TimeOfDay::Night).to_string(),
        );
        context.insert("is_day".to_string(), game_time.is_daytime().to_string());

        if let Err(e) = fire_environmental_triggers(db, connections, TriggerType::OnTimeChange, &context) {
            error!("Error firing time change triggers: {}", e);
        }
    }

    // Update weather periodically (every ~15 game hours = 30 real minutes)
    if now - game_time.last_weather_change >= 1800 {
        update_weather(&mut game_time);
        game_time.last_weather_change = now;

        // Fire OnWeatherChange triggers if weather actually changed
        if game_time.weather != old_weather {
            let mut context = std::collections::HashMap::new();
            context.insert("old_weather".to_string(), format!("{:?}", old_weather).to_lowercase());
            context.insert(
                "new_weather".to_string(),
                format!("{:?}", game_time.weather).to_lowercase(),
            );

            // Helper flags for common weather categories
            let is_raining = matches!(
                game_time.weather,
                WeatherCondition::LightRain
                    | WeatherCondition::Rain
                    | WeatherCondition::HeavyRain
                    | WeatherCondition::Thunderstorm
            );
            let is_snowing = matches!(
                game_time.weather,
                WeatherCondition::LightSnow | WeatherCondition::Snow | WeatherCondition::Blizzard
            );
            let is_clear = matches!(
                game_time.weather,
                WeatherCondition::Clear | WeatherCondition::PartlyCloudy
            );

            context.insert("is_raining".to_string(), is_raining.to_string());
            context.insert("is_snowing".to_string(), is_snowing.to_string());
            context.insert("is_clear".to_string(), is_clear.to_string());

            if let Err(e) = fire_environmental_triggers(db, connections, TriggerType::OnWeatherChange, &context) {
                error!("Error firing weather change triggers: {}", e);
            }
        }
    }

    // Fire OnSeasonChange triggers if season changed
    if old_season != new_season {
        let message = get_season_transition_message(&new_season);
        broadcast_to_all_players(connections, &format!("\n{}\n", message));

        let mut context = std::collections::HashMap::new();
        context.insert("old_season".to_string(), format!("{}", old_season).to_lowercase());
        context.insert("new_season".to_string(), format!("{}", new_season).to_lowercase());
        context.insert("is_spring".to_string(), (new_season == Season::Spring).to_string());
        context.insert("is_summer".to_string(), (new_season == Season::Summer).to_string());
        context.insert("is_autumn".to_string(), (new_season == Season::Autumn).to_string());
        context.insert("is_winter".to_string(), (new_season == Season::Winter).to_string());

        if let Err(e) = fire_environmental_triggers(db, connections, TriggerType::OnSeasonChange, &context) {
            error!("Error firing season change triggers: {}", e);
        }
    }

    // Fire OnMonthChange triggers if month changed
    if old_month != game_time.month {
        let mut context = std::collections::HashMap::new();
        context.insert("old_month".to_string(), old_month.to_string());
        context.insert("new_month".to_string(), game_time.month.to_string());
        context.insert("new_day".to_string(), game_time.day.to_string());
        context.insert("new_year".to_string(), game_time.year.to_string());

        // Festival flags for common events (every 4th month)
        let is_festival = game_time.month == 4 || game_time.month == 8 || game_time.month == 12;
        context.insert("is_festival".to_string(), is_festival.to_string());

        if let Err(e) = fire_environmental_triggers(db, connections, TriggerType::OnMonthChange, &context) {
            error!("Error firing month change triggers: {}", e);
        }
    }

    game_time.last_time_tick = now;
    db.save_game_time(&game_time)?;

    Ok(())
}

/// Get the message to broadcast when time of day changes
pub fn get_time_transition_message(tod: &TimeOfDay) -> &'static str {
    match tod {
        TimeOfDay::Dawn => "The sun begins to rise on the horizon, painting the sky in shades of orange and pink.",
        TimeOfDay::Morning => "The morning sun casts long shadows across the land.",
        TimeOfDay::Noon => "The sun reaches its peak in the sky, bathing everything in bright light.",
        TimeOfDay::Afternoon => "The afternoon sun warms the land as the day continues.",
        TimeOfDay::Dusk => "The sun begins to set, painting the sky in orange and crimson hues.",
        TimeOfDay::Evening => "Twilight settles over the land as stars begin to appear.",
        TimeOfDay::Night => "Darkness falls as night takes hold. The stars shine brightly overhead.",
    }
}

/// Get the message to broadcast when season changes
pub fn get_season_transition_message(season: &Season) -> &'static str {
    match season {
        Season::Spring => "The air grows warmer as spring arrives. Flowers begin to bloom across the land.",
        Season::Summer => "Summer has arrived! The sun beats down warmly and the days grow long.",
        Season::Autumn => {
            "The leaves begin to change color as autumn settles in. A cool breeze carries the scent of fallen leaves."
        }
        Season::Winter => "Winter descends upon the land. A chill fills the air as frost blankets the ground.",
    }
}

/// Update weather conditions based on season and randomness
fn update_weather(game_time: &mut GameTime) {
    use rand::Rng;

    let mut rng = rand::thread_rng();

    // Weather transition probabilities based on season
    let (rain_chance, snow_chance, clear_chance) = match game_time.get_season() {
        Season::Spring => (30, 5, 40),
        Season::Summer => (20, 0, 60),
        Season::Autumn => (35, 10, 35),
        Season::Winter => (15, 40, 30),
    };

    let roll: i32 = rng.gen_range(1..=100);

    game_time.weather = if roll <= clear_chance {
        if rng.gen_bool(0.3) {
            WeatherCondition::PartlyCloudy
        } else {
            WeatherCondition::Clear
        }
    } else if roll <= clear_chance + rain_chance {
        // Check if it should be snow instead in cold conditions
        if game_time.get_season() == Season::Winter && game_time.calculate_effective_temperature() < 2 {
            match rng.gen_range(1..=3) {
                1 => WeatherCondition::LightSnow,
                2 => WeatherCondition::Snow,
                _ => WeatherCondition::Blizzard,
            }
        } else {
            match rng.gen_range(1..=4) {
                1 => WeatherCondition::LightRain,
                2 => WeatherCondition::Rain,
                3 => WeatherCondition::HeavyRain,
                _ => WeatherCondition::Thunderstorm,
            }
        }
    } else if roll <= clear_chance + rain_chance + snow_chance {
        match rng.gen_range(1..=3) {
            1 => WeatherCondition::LightSnow,
            2 => WeatherCondition::Snow,
            _ => WeatherCondition::Blizzard,
        }
    } else {
        match rng.gen_range(1..=3) {
            1 => WeatherCondition::Cloudy,
            2 => WeatherCondition::Overcast,
            _ => WeatherCondition::Fog,
        }
    };

    // Adjust base temperature with some randomness
    game_time.base_temperature = 18 + rng.gen_range(-5..=5);
}

/// Fire environmental triggers (time change or weather change) for all rooms
fn fire_environmental_triggers(
    db: &db::Db,
    connections: &SharedConnections,
    trigger_type: TriggerType,
    context: &std::collections::HashMap<String, String>,
) -> Result<()> {
    use rand::Rng;

    let rooms = db.list_all_rooms()?;

    for room in rooms {
        // For time, weather, and season triggers, skip indoor/climate_controlled rooms
        if trigger_type == TriggerType::OnTimeChange
            || trigger_type == TriggerType::OnWeatherChange
            || trigger_type == TriggerType::OnSeasonChange
        {
            // Check for climate_controlled (room or area inherited)
            let is_climate_controlled = room.flags.climate_controlled
                || room
                    .area_id
                    .and_then(|aid| db.get_area_data(&aid).ok().flatten())
                    .map(|area| area.flags.climate_controlled)
                    .unwrap_or(false);
            if room.flags.indoors || is_climate_controlled {
                continue;
            }
        }

        // Find matching triggers
        for trigger in &room.triggers {
            if trigger.trigger_type != trigger_type || !trigger.enabled {
                continue;
            }

            // Check chance
            if trigger.chance < 100 {
                let roll: i32 = rand::thread_rng().gen_range(1..=100);
                if roll > trigger.chance {
                    continue;
                }
            }

            // Find all awake players in this room (sleeping players don't see environmental triggers)
            let players_in_room: Vec<(uuid::Uuid, tokio::sync::mpsc::UnboundedSender<String>)> = {
                let conns = connections.lock().unwrap();
                conns
                    .iter()
                    .filter_map(|(conn_id, session)| {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room.id && char.position != ironmud::CharacterPosition::Sleeping
                            {
                                return Some((*conn_id, session.sender.clone()));
                            }
                        }
                        None
                    })
                    .collect()
            };

            if players_in_room.is_empty() {
                continue;
            }

            // Handle built-in templates (script_name starts with @)
            if trigger.script_name.starts_with('@') {
                let template_name = &trigger.script_name[1..];
                execute_room_template_main(template_name, &trigger.args, &room.id, connections, context);
                continue;
            }

            // Execute trigger script
            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(content) => content,
                Err(e) => {
                    debug!("Environmental trigger script not found: {} - {}", script_path, e);
                    continue;
                }
            };

            // Create a minimal Rhai engine for trigger execution
            let mut trigger_engine = rhai::Engine::new();

            // Register send_client_message
            for (conn_id, sender) in &players_in_room {
                let conn_id_str = conn_id.to_string();
                let sender_clone = sender.clone();

                trigger_engine.register_fn("send_client_message", move |cid: String, message: String| {
                    if cid == conn_id_str {
                        let _ = sender_clone.send(message);
                    }
                });
            }

            // Register broadcast_to_room
            let conns_clone = connections.clone();
            let room_id = room.id;
            trigger_engine.register_fn(
                "broadcast_to_room",
                move |_rid: String, message: String, _exclude: String| {
                    let conns = conns_clone.lock().unwrap();
                    for (_, session) in conns.iter() {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room_id {
                                let _ = session.sender.send(message.clone());
                            }
                        }
                    }
                },
            );

            // Register random_int
            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            // Compile and run trigger
            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    for (conn_id, _) in &players_in_room {
                        let mut scope = rhai::Scope::new();
                        let room_id_str = room.id.to_string();
                        let conn_id_str = conn_id.to_string();

                        // Build context map
                        let mut rhai_context = rhai::Map::new();
                        for (k, v) in context {
                            rhai_context.insert(k.clone().into(), v.clone().into());
                        }

                        match trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (room_id_str.clone(), conn_id_str.clone(), rhai_context.clone()),
                        ) {
                            Ok(_) => {
                                debug!(
                                    "Environmental trigger {} executed for room {}",
                                    trigger.script_name, room.id
                                );
                            }
                            Err(e) => {
                                let msg = format!("Environmental trigger script error in {}: {}", script_path, e);
                                error!("{}", msg);
                                broadcast_to_builders(connections, &msg);
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to compile environmental trigger script {}: {}", script_path, e);
                    error!("{}", msg);
                    broadcast_to_builders(connections, &msg);
                }
            }
        }
    }

    Ok(())
}

/// Execute a built-in room template for environmental triggers
fn execute_room_template_main(
    template_name: &str,
    args: &[String],
    room_id: &uuid::Uuid,
    connections: &SharedConnections,
    context: &std::collections::HashMap<String, String>,
) {
    // Helper to broadcast message to all players in the room
    let broadcast = |msg: &str| {
        if let Ok(conns) = connections.lock() {
            for (_, session) in conns.iter() {
                if let Some(ref char_data) = session.character {
                    if char_data.current_room_id == *room_id {
                        let _ = session.sender.send(format!("{}\n", msg));
                    }
                }
            }
        }
    };

    match template_name {
        "room_message" => {
            if let Some(message) = args.first() {
                broadcast(message);
            }
        }
        "time_message" => {
            if args.len() >= 2 {
                let target_time = &args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_time) = context.get("new_time") {
                    if new_time.to_lowercase() == *target_time {
                        broadcast(message);
                    }
                }
            }
        }
        "weather_message" => {
            if args.len() >= 2 {
                let target_weather = args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_weather) = context.get("new_weather") {
                    let weather_lower = new_weather.to_lowercase();
                    let matches = weather_lower == target_weather
                        || (target_weather == "raining"
                            && (weather_lower.contains("rain") || weather_lower == "thunderstorm"))
                        || (target_weather == "snowing" && weather_lower.contains("snow"))
                        || (target_weather == "stormy" && weather_lower == "thunderstorm")
                        || (target_weather == "precipitation"
                            && (weather_lower.contains("rain") || weather_lower.contains("snow")));

                    if matches {
                        broadcast(message);
                    }
                }
            }
        }
        "season_message" => {
            if args.len() >= 2 {
                let target_season = args[0].to_lowercase();
                let message = &args[1];

                if let Some(new_season) = context.get("new_season") {
                    if new_season.to_lowercase() == target_season {
                        broadcast(message);
                    }
                }
            }
        }
        _ => {
            debug!("Unknown room template: @{}", template_name);
        }
    }
}

// === Weather Exposure Tick System ===

/// Background task that processes weather exposure effects
pub async fn run_exposure_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(EXPOSURE_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_exposure_tick(&db, &connections) {
            error!("Exposure tick error: {}", e);
        }
    }
}

/// Process weather exposure for all logged-in players
fn process_exposure_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let game_time = db.get_game_time()?;
    let outdoor_temp = game_time.calculate_effective_temperature();
    let weather = game_time.weather;

    // Collect helpline alerts to send after the main loop
    let mut helpline_alerts: Vec<String> = Vec::new();

    // Collect room broadcasts to send after releasing the lock (prevents deadlock)
    let mut room_broadcasts: Vec<(uuid::Uuid, String, String)> = Vec::new();

    let mut conns = connections.lock().unwrap();

    for (_conn_id, session) in conns.iter_mut() {
        if let Some(ref mut char) = session.character {
            // Skip if character creation is not complete
            if !char.creation_complete {
                continue;
            }

            // Skip exposure for god mode and build mode players
            if char.god_mode || ironmud::check_build_mode(db, &char.name, &char.current_room_id) {
                continue;
            }

            // Get current room data
            let room = match db.get_room_data(&char.current_room_id) {
                Ok(Some(r)) => r,
                _ => continue,
            };

            let mut modified = false;

            // === Calculate effective temperature for this room ===
            // Check for climate_controlled (room or area inherited)
            let is_climate_controlled = room.flags.climate_controlled
                || room
                    .area_id
                    .and_then(|aid| db.get_area_data(&aid).ok().flatten())
                    .map(|area| area.flags.climate_controlled)
                    .unwrap_or(false);
            let is_outdoors = !room.flags.indoors && !is_climate_controlled;
            let expose_to_elements = is_outdoors || room.flags.always_cold || room.flags.always_hot;

            // Calculate effective temperature for this room
            let effective_temp = if room.flags.always_cold {
                -5 // Always freezing (ice caves, freezers)
            } else if room.flags.always_hot {
                36 // Always sweltering (forges, volcanoes)
            } else if !is_outdoors {
                let target = 15;
                outdoor_temp + ((target - outdoor_temp) * 60 / 100)
            } else {
                outdoor_temp
            };

            let temp_category = match effective_temp {
                t if t < 0 => TemperatureCategory::Freezing,
                t if t < 10 => TemperatureCategory::Cold,
                t if t < 15 => TemperatureCategory::Cool,
                t if t < 20 => TemperatureCategory::Mild,
                t if t < 25 => TemperatureCategory::Warm,
                t if t < 35 => TemperatureCategory::Hot,
                _ => TemperatureCategory::Sweltering,
            };

            // === Process wet status ===
            let has_waterproof = check_waterproof_coverage(char, db);
            let has_warmth = check_room_warmth(&char.current_room_id, db);

            // Rain/snow causes wetness outdoors
            if is_outdoors && !has_waterproof {
                let wet_increase = match weather {
                    WeatherCondition::Thunderstorm | WeatherCondition::Blizzard => 30,
                    WeatherCondition::HeavyRain => 25,
                    WeatherCondition::Rain | WeatherCondition::Snow => 20,
                    WeatherCondition::LightRain | WeatherCondition::LightSnow => 10,
                    WeatherCondition::Fog => 5,
                    WeatherCondition::PartlyCloudy
                    | WeatherCondition::Overcast
                    | WeatherCondition::Cloudy
                    | WeatherCondition::Clear => 0,
                };
                if wet_increase > 0 {
                    char.wet_level = (char.wet_level + wet_increase).min(100);
                    char.is_wet = true;
                    modified = true;
                }
            }

            // Drying: faster near warmth sources (campfire, fireplace)
            let base_dry_rate = if has_warmth {
                30 // Near fire - fastest drying
            } else if room.flags.indoors {
                10 // Indoors - moderate drying
            } else {
                5 // Outdoors - slow drying
            };

            // Wet recovery traits
            let has_weatherproof = char.traits.iter().any(|t| t == "weatherproof");
            let has_delicate = char.traits.iter().any(|t| t == "delicate");
            let mut wet_mod: i32 = 0;
            if has_weatherproof {
                wet_mod += 50;
            }
            if has_delicate {
                wet_mod -= 25;
            }
            let dry_rate = base_dry_rate * (100 + wet_mod) / 100;

            // Can dry if: near warmth, indoors, or outdoors with clear weather
            if char.wet_level > 0
                && (has_warmth || room.flags.indoors || (is_outdoors && weather == WeatherCondition::Clear))
            {
                char.wet_level = (char.wet_level - dry_rate).max(0);
                char.is_wet = char.wet_level > 0;
                modified = true;
            }

            // === Process temperature exposure ===
            let insulation = calculate_character_insulation(char, db);
            let wet_penalty = char.wet_level / 2; // Wet reduces insulation effectiveness

            // Cold exposure - only dangerous temperatures cause exposure
            let cold_threshold = match temp_category {
                TemperatureCategory::Freezing => 80, // < 0°C - Need heavy insulation
                TemperatureCategory::Cold => 40,     // 0-9°C - Need moderate insulation
                TemperatureCategory::Cool => 0,      // 10-14°C - Cool but safe
                _ => 0,
            };

            // Exposure traits (shared for cold and heat)
            let has_hardy = char.traits.iter().any(|t| t == "hardy");
            let has_exposure_prone = char.traits.iter().any(|t| t == "exposure_prone");

            if cold_threshold > 0 && expose_to_elements {
                let effective_insulation = (insulation - wet_penalty).max(0);
                let mut exposure_rate = if effective_insulation >= cold_threshold {
                    // Recovering - faster near warmth
                    if has_warmth {
                        -20
                    } else if !expose_to_elements {
                        -10
                    } else {
                        -5
                    }
                } else {
                    ((cold_threshold - effective_insulation) / 10).max(1) as i32
                };
                // Apply exposure traits (only to positive exposure gain)
                if exposure_rate > 0 {
                    let mut exposure_mod: i32 = 0;
                    if has_hardy {
                        exposure_mod -= 25;
                    }
                    if has_exposure_prone {
                        exposure_mod += 25;
                    }
                    exposure_rate = (exposure_rate * (100 + exposure_mod) / 100).max(1);
                }
                char.cold_exposure = (char.cold_exposure + exposure_rate).clamp(0, 100);
                modified = true;
            } else {
                // Warm enough or sheltered indoors, recover from cold
                if char.cold_exposure > 0 {
                    let recovery = if has_warmth {
                        25
                    } else if !expose_to_elements {
                        15
                    } else {
                        10
                    };
                    char.cold_exposure = (char.cold_exposure - recovery).max(0);
                    modified = true;
                }
            }

            // Heat exposure (insulation works against you in heat!)
            let heat_threshold = match temp_category {
                TemperatureCategory::Sweltering => 30, // High heat exposure
                TemperatureCategory::Hot => 15,        // Moderate heat exposure
                _ => 0,
            };

            if heat_threshold > 0 && expose_to_elements {
                // Insulation makes heat worse!
                let mut exposure_rate = (heat_threshold + (insulation / 5)).max(1) as i32;
                // Apply exposure traits
                let mut heat_exposure_mod: i32 = 0;
                if has_hardy {
                    heat_exposure_mod -= 25;
                }
                if has_exposure_prone {
                    heat_exposure_mod += 25;
                }
                exposure_rate = (exposure_rate * (100 + heat_exposure_mod) / 100).max(1);
                char.heat_exposure = (char.heat_exposure + exposure_rate).clamp(0, 100);
                modified = true;
            } else {
                // Cool/indoors, recover from heat
                if char.heat_exposure > 0 {
                    char.heat_exposure = (char.heat_exposure - 15).max(0);
                    modified = true;
                }
            }

            // === Apply conditions based on exposure ===

            // Hypothermia at 50% cold exposure
            let had_hypothermia = char.has_hypothermia;
            char.has_hypothermia = char.cold_exposure >= 50;
            if char.has_hypothermia && !had_hypothermia {
                let _ = session
                    .sender
                    .send("You begin to shiver uncontrollably. You're getting hypothermia!\n".to_string());
            } else if !char.has_hypothermia && had_hypothermia {
                let _ = session
                    .sender
                    .send("Your body temperature returns to normal.\n".to_string());
            }

            // Frostbite wound at 80% cold exposure
            if char.cold_exposure >= 80 && char.has_frostbite.is_empty() {
                // Add frostbite wound to extremities
                let frostbite_part = if rand::random::<bool>() {
                    BodyPart::LeftFoot
                } else {
                    BodyPart::RightHand
                };
                char.has_frostbite.push(frostbite_part);
                char.wounds.push(Wound {
                    body_part: frostbite_part,
                    level: WoundLevel::Moderate,
                    wound_type: WoundType::Frostbite,
                    bleeding_severity: 0,
                });
                let _ = session.sender.send(format!(
                    "Your {} is getting frostbitten!\n",
                    frostbite_part.to_display_string()
                ));
                modified = true;
            }

            // Heat exhaustion at 50% heat exposure
            let had_heat_exhaustion = char.has_heat_exhaustion;
            char.has_heat_exhaustion = char.heat_exposure >= 50;
            if char.has_heat_exhaustion && !had_heat_exhaustion {
                let _ = session
                    .sender
                    .send("You're feeling lightheaded and weak from the heat!\n".to_string());
            } else if !char.has_heat_exhaustion && had_heat_exhaustion {
                let _ = session.sender.send("You feel better as you cool down.\n".to_string());
            }

            // Heat stroke at 80% heat exposure
            let had_heat_stroke = char.has_heat_stroke;
            char.has_heat_stroke = char.heat_exposure >= 80;
            if char.has_heat_stroke && !had_heat_stroke {
                let _ = session.sender.send(
                    "WARNING: You're suffering from heat stroke! Get to shade or you will collapse!\n".to_string(),
                );
            }

            // Illness progression from being wet and cold
            let has_sickly = char.traits.iter().any(|t| t == "sickly");
            let has_vigorous_env = char.traits.iter().any(|t| t == "vigorous");
            if char.is_wet && char.cold_exposure >= 25 && !char.food_sick {
                let mut illness_gain = 5;
                if has_weatherproof {
                    illness_gain = illness_gain * 50 / 100;
                } // 50% reduction
                if has_sickly {
                    illness_gain = illness_gain * 150 / 100;
                } // 50% increase
                if has_delicate {
                    illness_gain = illness_gain * 150 / 100;
                } // 50% increase
                if has_vigorous_env {
                    illness_gain = illness_gain * 75 / 100;
                } // 25% reduction
                illness_gain = illness_gain.max(1);
                char.illness_progress = (char.illness_progress + illness_gain).min(100);
                if char.illness_progress >= 50 && !char.has_illness {
                    char.has_illness = true;
                    let _ = session
                        .sender
                        .send("You're coming down with a cold. You should get warm and dry!\n".to_string());
                }
                modified = true;
            } else if char.has_illness && char.food_sick {
                // Food sickness recovery: decrement by 1 per tick regardless of conditions
                char.illness_progress = (char.illness_progress - 1).max(0);
                if char.illness_progress == 0 {
                    char.has_illness = false;
                    char.food_sick = false;
                    let _ = session
                        .sender
                        .send("Your stomach finally settles. You feel better.\n".to_string());
                }
                modified = true;
            } else if char.has_illness && !char.food_sick {
                // Natural recovery when warm and dry (weather illness)
                if !char.is_wet && char.cold_exposure == 0 {
                    char.illness_progress = (char.illness_progress - 2).max(0);
                    if char.illness_progress == 0 {
                        char.has_illness = false;
                        let _ = session
                            .sender
                            .send("You feel better - your illness has passed.\n".to_string());
                    }
                    modified = true;
                }
            }

            // === Condition damage ===

            // Heat stroke damage
            if char.has_heat_stroke
                && !char.god_mode
                && !ironmud::check_build_mode(db, &char.name, &char.current_room_id)
            {
                char.hp = (char.hp - 2).max(1);
                modified = true;
            }

            // Illness damage (minor HP drain)
            if char.has_illness && char.illness_progress >= 75 {
                char.hp = (char.hp - 1).max(1);
                modified = true;
            }

            // Involuntary sneeze when ill (random chance each tick)
            if char.has_illness {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // ~10% chance per tick (every 30 seconds) = sneeze roughly every 5 minutes
                if rng.gen_range(0..100) < 10 {
                    // Tell the player they sneezed
                    let _ = session.sender.send("You sneeze uncontrollably!\n".to_string());
                    // Collect broadcast for others in the room (sent after lock is released)
                    room_broadcasts.push((
                        char.current_room_id,
                        format!("{} sneezes.", char.name),
                        char.name.clone(),
                    ));
                }
            }

            // Involuntary coughing when severely ill (illness_progress >= 75)
            if char.has_illness && char.illness_progress >= 75 {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // ~15% chance per tick = cough roughly every 3-4 minutes when severe
                if rng.gen_range(0..100) < 15 {
                    let _ = session.sender.send("You have a coughing fit!\n".to_string());
                    room_broadcasts.push((
                        char.current_room_id,
                        format!("{} coughs violently.", char.name),
                        char.name.clone(),
                    ));
                }
            }

            // Vomiting emotes when food sick
            if char.food_sick {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // ~8% chance per tick
                if rng.gen_range(0..100) < 8 {
                    let (player_msg, room_msg) = match rng.gen_range(0..3) {
                        0 => ("You retch and vomit.\n", format!("{} retches and vomits.", char.name)),
                        1 => (
                            "Your stomach churns violently.\n",
                            format!("{} looks very ill.", char.name),
                        ),
                        _ => ("You feel nauseous.\n", format!("{} looks queasy.", char.name)),
                    };
                    let _ = session.sender.send(player_msg.to_string());
                    room_broadcasts.push((char.current_room_id, room_msg, char.name.clone()));
                }
            }

            // Poison emotes when character has poisoned wounds
            if char.wounds.iter().any(|w| w.wound_type == WoundType::Poisoned) {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // ~8% chance per tick
                if rng.gen_range(0..100) < 8 {
                    let (player_msg, room_msg) = match rng.gen_range(0..3) {
                        0 => (
                            "You shudder as poison courses through your veins.\n",
                            format!("{} shudders, looking poisoned.", char.name),
                        ),
                        1 => (
                            "A wave of nausea from the poison washes over you.\n",
                            format!("{} looks sickly and pale.", char.name),
                        ),
                        _ => (
                            "Your vision blurs as the poison takes its toll.\n",
                            format!("{} sways unsteadily, looking ill.", char.name),
                        ),
                    };
                    let _ = session.sender.send(player_msg.to_string());
                    room_broadcasts.push((char.current_room_id, room_msg, char.name.clone()));
                }
            }

            // Shivering from cold exposure (25+, more frequent as exposure increases)
            if char.cold_exposure >= 25 {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // Chance scales with exposure: 5% at 25, 10% at 50, 15% at 75, 20% at 100
                let shiver_chance = 5 + (char.cold_exposure - 25) / 5;
                if rng.gen_range(0..100) < shiver_chance as i32 {
                    let (player_msg, room_msg) = if char.cold_exposure >= 75 {
                        (
                            "You shiver violently from the bitter cold!\n",
                            format!("{} shivers violently.", char.name),
                        )
                    } else if char.cold_exposure >= 50 {
                        (
                            "You shiver uncontrollably!\n",
                            format!("{} shivers uncontrollably.", char.name),
                        )
                    } else {
                        ("You shiver from the cold.\n", format!("{} shivers.", char.name))
                    };
                    let _ = session.sender.send(player_msg.to_string());
                    room_broadcasts.push((char.current_room_id, room_msg, char.name.clone()));
                }
            }

            // Sweating from heat exposure (25+, more frequent as exposure increases)
            if char.heat_exposure >= 25 {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                // Chance scales with exposure: 5% at 25, 10% at 50, 15% at 75, 20% at 100
                let sweat_chance = 5 + (char.heat_exposure - 25) / 5;
                if rng.gen_range(0..100) < sweat_chance as i32 {
                    let (player_msg, room_msg) = if char.heat_exposure >= 75 {
                        (
                            "Sweat pours down your face as you struggle with the oppressive heat!\n",
                            format!("{} is drenched in sweat.", char.name),
                        )
                    } else if char.heat_exposure >= 50 {
                        (
                            "You wipe the sweat from your brow.\n",
                            format!("{} wipes sweat from their brow.", char.name),
                        )
                    } else {
                        (
                            "You feel yourself starting to sweat.\n",
                            format!("{} is starting to sweat.", char.name),
                        )
                    };
                    let _ = session.sender.send(player_msg.to_string());
                    room_broadcasts.push((char.current_room_id, room_msg, char.name.clone()));
                }
            }

            // === Auto-alert helpline for critical states ===
            if char.has_heat_stroke || char.cold_exposure >= 90 {
                // Collect alert for helpline channel
                let room_name = room.title.clone();
                let alert_msg = if char.has_heat_stroke {
                    format!("{} is suffering from heat stroke!", char.name)
                } else {
                    format!("{} is severely hypothermic!", char.name)
                };
                helpline_alerts.push(format!("[HELPLINE] {} Location: {}\n", alert_msg, room_name));
            }

            if modified {
                let _ = db.save_character_data(char.clone());
            }
        }
    }

    // Drop the connections lock before sending broadcasts (prevents deadlock)
    drop(conns);

    // Send collected room broadcasts (each call acquires its own lock briefly)
    for (room_id, message, exclude) in room_broadcasts {
        broadcast_to_room_except(connections, &room_id, &message, &exclude);
    }

    // Send helpline alerts to all players with helpline enabled
    if !helpline_alerts.is_empty() {
        if let Ok(conns) = connections.lock() {
            for session in conns.values() {
                if let Some(ref char) = session.character {
                    if char.helpline_enabled {
                        for alert in &helpline_alerts {
                            let _ = session.sender.send(alert.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if character has waterproof coverage from equipment
fn check_waterproof_coverage(char: &CharacterData, db: &db::Db) -> bool {
    // Query database for equipped items (source of truth is ItemLocation::Equipped)
    if let Ok(equipped_items) = db.get_equipped_items(&char.name) {
        for item in equipped_items {
            if item.flags.waterproof {
                return true;
            }
        }
    }
    false
}

/// Check if room has warmth-providing items (campfire, fireplace, etc.)
fn check_room_warmth(room_id: &uuid::Uuid, db: &db::Db) -> bool {
    if let Ok(items) = db.get_items_in_room(room_id) {
        for item in items {
            if item.flags.provides_warmth {
                return true;
            }
        }
    }
    false
}
