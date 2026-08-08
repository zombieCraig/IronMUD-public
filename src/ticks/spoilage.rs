//! Spoilage tick systems for IronMUD
//!
//! Handles corpse decay and food spoilage.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{ItemLocation, ItemType, SharedConnections, TemperatureCategory, db};

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
        crate::ticks::heartbeat::beat("corpse_decay");

        if let Err(e) = process_corpse_decay(&db, &connections) {
            error!("Corpse decay tick error: {}", e);
        }
    }
}

/// Process corpse decay - remove old corpses
fn process_corpse_decay(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let items = db.list_all_items().unwrap_or_default();
    decay_corpses(db, connections, items)
}

/// The decay pass, over a snapshot the caller took.
///
/// The snapshot is a parameter rather than a local because *staleness is the
/// interesting property here*: everything in `items` was read before this
/// function ran, and a player can loot a corpse in between. Passing it in is
/// what lets a test hand over a snapshot that no longer matches the database
/// and check that the writes below cope — which is the bug this shape was
/// introduced to pin, where the warning wrote the whole stale item back and
/// restored loot someone had already taken.
fn decay_corpses(db: &db::Db, connections: &SharedConnections, items: Vec<ironmud::ItemData>) -> Result<()> {
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

    let warn_fractions = ironmud::corpse::parse_warn_fractions(
        &db.get_setting_or_default("corpse_decay_warn_fractions", ironmud::corpse::DEFAULT_WARN_FRACTIONS)
            .unwrap_or_else(|_| ironmud::corpse::DEFAULT_WARN_FRACTIONS.to_string()),
    );

    {
        for item in items {
            // A corpse prototype is a builder's template, not a body on a
            // timer. `process_spoilage` below skips prototypes for the same
            // reason; this loop did not.
            if !item.flags.is_corpse || item.is_prototype {
                continue;
            }

            let age = now - item.flags.corpse_created_at;

            let decay_time = if item.flags.corpse_is_player {
                player_corpse_decay
            } else {
                mobile_corpse_decay
            };

            if age >= decay_time {
                // Get room for message
                if let ItemLocation::Room(room_id) = item.location {
                    broadcast_to_room(connections, &room_id, &format!("The {} crumbles to dust.", item.name));
                }

                // The owner is told directly, wherever they are. A player who
                // could not make it back deserves to know the run is over
                // rather than arriving at an empty room and guessing.
                if item.flags.corpse_is_player {
                    ironmud::script::achievements::send_to_player(
                        connections,
                        &item.flags.corpse_owner,
                        ironmud::corpse::decay_final_line(),
                    );
                    // Drop it from the owner's `corpse_ids` so `locate` does
                    // not have to prune it later. Through `apply_to_character`
                    // rather than `update_character`: the owner may well be
                    // online, and a DB-only write to an online player is
                    // reverted by the next session flush.
                    let gone = item.id;
                    ironmud::script::achievements::apply_to_character(db, connections, &item.flags.corpse_owner, |c| {
                        let before = c.corpse_ids.len();
                        c.corpse_ids.retain(|id| *id != gone);
                        c.corpse_ids.len() != before
                    });
                }

                // Delete all items in the corpse
                for item_id in &item.container_contents {
                    let _ = db.delete_item(item_id);
                }

                // Delete the corpse itself
                let _ = db.delete_item(&item.id);

                debug!("Corpse {} decayed", item.name);
                continue;
            }

            // Decay warnings, player corpses only. A mob corpse rotting is not
            // a loss anyone needs chasing; a player's is the whole stake of
            // dying, and it used to run out on a blind timer with no signal at
            // all.
            if !item.flags.corpse_is_player || warn_fractions.is_empty() {
                continue;
            }
            if let Some(threshold) =
                ironmud::corpse::next_warning(age, decay_time, item.flags.corpse_warned_pct, &warn_fractions)
            {
                ironmud::script::achievements::send_to_player(
                    connections,
                    &item.flags.corpse_owner,
                    &ironmud::corpse::decay_warning_line(decay_time - age),
                );
                // Recorded on the item, not in the tick, so a restart does not
                // replay every warning the owner already had.
                //
                // A CAS update, not `save_item_data(item)`. `item` came out of
                // a `list_all_items()` snapshot taken at the top of this
                // function, and writing it back whole would revert anything
                // that changed since — on a corpse the field that changes is
                // `container_contents`, so a player looting in the same moment
                // a threshold fired would have the loot put back and end up
                // with two of it.
                let _ = db.update_item(&item.id, |i| {
                    i.flags.corpse_warned_pct = threshold;
                });
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
        crate::ticks::heartbeat::beat("spoilage");

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
                        2 => 0.0,  // Freezer - no spoilage
                        1 => 0.25, // Fridge - very slow
                        _ => 0.5,  // Sealed preserving container
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
        ItemLocation::Room(room_id) => db.get_room_data(room_id).ok().flatten(),
        ItemLocation::Container(container_id) => {
            // Resolve the container's location (one level deep)
            if let Ok(Some(container)) = db.get_item_data(container_id) {
                match &container.location {
                    ItemLocation::Room(room_id) => {
                        return db.get_room_data(room_id).ok().flatten();
                    }
                    ItemLocation::Inventory(char_name) | ItemLocation::Equipped(char_name) => {
                        if let Ok(Some(ch)) = db.get_character_data(char_name) {
                            return db.get_room_data(&ch.current_room_id).ok().flatten();
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        ItemLocation::Inventory(char_name) | ItemLocation::Equipped(char_name) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ironmud::corpse::CorpseBuilder;
    use ironmud::types::ItemData;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn conns() -> SharedConnections {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// The decay warning must not write the corpse back whole.
    ///
    /// `item` comes out of a `list_all_items()` snapshot taken at the top of
    /// the tick. Saving it wholesale reverts anything that changed since, and
    /// on a corpse the field that changes is `container_contents` — so a player
    /// looting in the same moment a threshold fired had the loot put back and
    /// ended up holding two of it. Simulated here by emptying the corpse
    /// between the snapshot and the write, which is exactly what a loot does.
    #[test]
    fn a_decay_warning_does_not_restore_looted_contents() {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = db::Db::open(temp.path()).expect("open db");

        db.set_setting("player_corpse_decay_secs", "3600").expect("setting");

        let loot = ItemData::new("Dagger".into(), "a rusty dagger".into(), String::new());
        let loot_id = loot.id;
        db.save_item_data(loot).expect("save loot");

        // Old enough to have crossed the 50% threshold but not to have rotted.
        let mut corpse = CorpseBuilder::for_player("Kaleth", uuid::Uuid::new_v4(), 0).build();
        corpse.flags.corpse_created_at = now() - 2000;
        corpse.container_contents.push(loot_id);
        let corpse_id = corpse.id;
        db.save_item_data(corpse).expect("save corpse");

        // The snapshot the tick works from — taken *before* the loot, which is
        // the whole point. A real tick reads every item at the top and then
        // spends time working through them; a player looting in that window is
        // ordinary, not exotic.
        let snapshot = db.list_all_items().expect("snapshot");

        // The loot leaves the corpse, as `get` would move it.
        db.update_item(&corpse_id, |i| i.container_contents.clear())
            .expect("loot it");

        decay_corpses(&db, &conns(), snapshot).expect("tick runs");

        let after = db.get_item_data(&corpse_id).expect("read").expect("still there");
        assert!(
            after.container_contents.is_empty(),
            "the warning must not put the loot back: {:?}",
            after.container_contents
        );
        assert_eq!(after.flags.corpse_warned_pct, 50, "and it still records the warning");
    }

    /// A corpse prototype is a builder's template, not a body on a timer.
    #[test]
    fn a_corpse_prototype_is_not_on_the_clock() {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = db::Db::open(temp.path()).expect("open db");

        let mut proto = CorpseBuilder::for_player("Template", uuid::Uuid::new_v4(), 0).build();
        proto.is_prototype = true;
        proto.flags.corpse_created_at = 0; // ancient
        let proto_id = proto.id;
        db.save_item_data(proto).expect("save prototype");

        process_corpse_decay(&db, &conns()).expect("tick runs");

        assert!(
            db.get_item_data(&proto_id).expect("read").is_some(),
            "the prototype survives"
        );
    }
}
