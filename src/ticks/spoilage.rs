//! Spoilage tick systems for IronMUD
//!
//! Handles corpse decay and food spoilage.

use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::{debug, error};

use ironmud::{db, ItemLocation, ItemType, SharedConnections, TemperatureCategory};

use super::broadcast::broadcast_to_room;

/// Corpse decay tick interval - check every 60 seconds
pub const CORPSE_DECAY_INTERVAL_SECS: u64 = 60;

/// Spoilage tick interval - accumulate food spoilage every 60 seconds
pub const SPOILAGE_TICK_INTERVAL_SECS: u64 = 60;

/// Background task that processes corpse decay
pub async fn run_corpse_decay_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(CORPSE_DECAY_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_corpse_decay(&db, &connections) {
            error!("Corpse decay tick error: {}", e);
        }
    }
}

/// Process corpse decay - remove old corpses
fn process_corpse_decay(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let player_corpse_decay: i64 = db
        .get_setting_or_default("player_corpse_decay_secs", "3600")
        .unwrap_or_else(|_| "3600".to_string())
        .parse::<i64>()
        .unwrap_or(3600)
        .max(60);
    let mobile_corpse_decay: i64 = db
        .get_setting_or_default("mobile_corpse_decay_secs", "600")
        .unwrap_or_else(|_| "600".to_string())
        .parse::<i64>()
        .unwrap_or(600)
        .max(60);

    // Get all items and check for decayed corpses
    if let Ok(items) = db.list_all_items() {
        for item in items {
            if !item.flags.is_corpse {
                continue;
            }

            let age = now - item.flags.corpse_created_at;

            let decay_time = if item.flags.corpse_is_player { player_corpse_decay } else { mobile_corpse_decay };

            if age >= decay_time {
                // Get room for message
                if let ItemLocation::Room(room_id) = item.location {
                    broadcast_to_room(connections, &room_id, &format!("The {} crumbles to dust.", item.name));
                }

                // Delete all items in the corpse
                for item_id in &item.container_contents {
                    let _ = db.delete_item(item_id);
                }

                // Delete the corpse itself
                let _ = db.delete_item(&item.id);

                debug!("Corpse {} decayed", item.name);
            }
        }
    }

    Ok(())
}

/// Background task that accumulates food spoilage based on temperature and container modifiers
pub async fn run_spoilage_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SPOILAGE_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_spoilage(&db, &connections) {
            error!("Spoilage tick error: {}", e);
        }
    }
}

/// Determine the temperature modifier for spoilage based on the room's effective temperature
fn get_spoilage_temp_modifier(room: &ironmud::RoomData, db: &db::Db) -> f64 {
    // Room-level overrides
    if room.flags.always_cold {
        return 0.0; // Freezing room = no spoilage
    }
    if room.flags.always_hot {
        return 2.0; // Sweltering
    }
    if room.flags.climate_controlled {
        return 1.0; // Mild
    }

    // Use global game time temperature
    if let Ok(game_time) = db.get_game_time() {
        match game_time.get_temperature_category() {
            TemperatureCategory::Freezing => 0.0,
            TemperatureCategory::Cold => 0.5,
            TemperatureCategory::Cool => 0.75,
            TemperatureCategory::Mild => 1.0,
            TemperatureCategory::Warm => 1.25,
            TemperatureCategory::Hot => 1.5,
            TemperatureCategory::Sweltering => 2.0,
        }
    } else {
        1.0 // Default to mild if game time unavailable
    }
}

/// Determine the container modifier for spoilage
fn get_spoilage_container_modifier(item: &ironmud::ItemData, db: &db::Db) -> f64 {
    if let ItemLocation::Container(container_id) = &item.location {
        if let Ok(Some(container)) = db.get_item_data(container_id) {
            if container.container_closed {
                if container.flags.preserves_contents {
                    return match container.preservation_level {
                        2 => 0.0,   // Freezer - no spoilage
                        1 => 0.25,  // Fridge - very slow
                        _ => 0.5,   // Sealed preserving container
                    };
                }
                return 0.75; // Closed but no preservation
            }
            // Open container = no benefit
        }
    }
    1.0 // Not in a container
}

/// Resolve an item's effective room for temperature purposes
fn resolve_item_room(item: &ironmud::ItemData, db: &db::Db) -> Option<ironmud::RoomData> {
    match &item.location {
        ItemLocation::Room(room_id) => {
            db.get_room_data(room_id).ok().flatten()
        }
        ItemLocation::Container(container_id) => {
            // Resolve the container's location (one level deep)
            if let Ok(Some(container)) = db.get_item_data(container_id) {
                match &container.location {
                    ItemLocation::Room(room_id) => {
                        return db.get_room_data(room_id).ok().flatten();
                    }
                    ItemLocation::Inventory(char_name) |
                    ItemLocation::Equipped(char_name) => {
                        if let Ok(Some(ch)) = db.get_character_data(char_name) {
                            return db.get_room_data(&ch.current_room_id).ok().flatten();
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        ItemLocation::Inventory(char_name) |
        ItemLocation::Equipped(char_name) => {
            if let Ok(Some(ch)) = db.get_character_data(char_name) {
                db.get_room_data(&ch.current_room_id).ok().flatten()
            } else {
                None
            }
        }
        ItemLocation::Nowhere => None,
    }
}

/// Process food spoilage accumulation for all food items
fn process_spoilage(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if let Ok(items) = db.list_all_items() {
        for item in items {
            // Only process food items with spoil duration set, not yet spoiled, not prototypes
            if item.item_type != ItemType::Food
                || item.food_spoil_duration == 0
                || item.food_spoilage_points >= 1.0
                || item.is_prototype
            {
                continue;
            }

            let mut points = item.food_spoilage_points;

            // Legacy migration: if spoilage_points is 0.0 but food_created_at exists,
            // compute initial points from elapsed time at 1x rate
            if points == 0.0 {
                if let Some(created) = item.food_created_at {
                    let elapsed = (now - created).max(0) as f64;
                    let duration = item.food_spoil_duration as f64;
                    if duration > 0.0 {
                        points = (elapsed / duration).min(1.0);
                    }
                }
            }

            // If already spoiled after legacy migration, save and broadcast
            if points >= 1.0 {
                let _ = db.update_item(&item.id, |i| {
                    i.food_spoilage_points = 1.0;
                });
                continue;
            }

            // Base increment per tick
            let base_increment = SPOILAGE_TICK_INTERVAL_SECS as f64 / item.food_spoil_duration as f64;

            // Temperature modifier from the item's effective room
            let temp_mod = if let Some(room) = resolve_item_room(&item, db) {
                get_spoilage_temp_modifier(&room, db)
            } else {
                // No room found (Nowhere) - use global temperature
                if let Ok(game_time) = db.get_game_time() {
                    match game_time.get_temperature_category() {
                        TemperatureCategory::Freezing => 0.0,
                        TemperatureCategory::Cold => 0.5,
                        TemperatureCategory::Cool => 0.75,
                        TemperatureCategory::Mild => 1.0,
                        TemperatureCategory::Warm => 1.25,
                        TemperatureCategory::Hot => 1.5,
                        TemperatureCategory::Sweltering => 2.0,
                    }
                } else {
                    1.0
                }
            };

            // Container modifier
            let container_mod = get_spoilage_container_modifier(&item, db);

            // Accumulate spoilage
            let new_points = (points + base_increment * temp_mod * container_mod).min(1.0);

            // Only save if changed meaningfully
            if (new_points - item.food_spoilage_points).abs() < f64::EPSILON {
                continue;
            }

            let just_spoiled = new_points >= 1.0 && item.food_spoilage_points < 1.0;

            let _ = db.update_item(&item.id, |i| {
                i.food_spoilage_points = new_points;
            });

            // Broadcast spoilage message if food just went bad and is in a room
            if just_spoiled {
                if let ItemLocation::Room(room_id) = &item.location {
                    broadcast_to_room(connections, room_id, &format!("{} has gone bad.", item.name));
                }
                debug!("Food item {} spoiled", item.name);
            }
        }
    }

    Ok(())
}
