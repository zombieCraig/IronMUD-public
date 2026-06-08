//! Donation decay tick.
//!
//! Items donated via the `donate` command are stamped with `donated_at`
//! and teleported into an area's configured donation room. This tick
//! sweeps stamped items still sitting in a room and deletes them once
//! their age exceeds `donation_decay_secs` (default 1800).

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{ItemLocation, SharedConnections, db};

use super::broadcast::broadcast_to_room;

/// Donation decay tick interval — check every 60 seconds.
pub const DONATION_DECAY_INTERVAL_SECS: u64 = 60;

/// Background task that decays donated items left in donation rooms.
pub async fn run_donation_decay_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(DONATION_DECAY_INTERVAL_SECS));

    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("donation_decay");

        if let Err(e) = process_donation_decay(&db, &connections) {
            error!("Donation decay tick error: {}", e);
        }
    }
}

/// Sweep donated items past their decay limit, deleting them in place
/// with a "crumbles to dust" broadcast to the host room.
pub fn process_donation_decay(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let decay_secs: i64 = db
        .get_setting_or_default("donation_decay_secs", "1800")
        .unwrap_or_else(|_| "1800".to_string())
        .parse::<i64>()
        .unwrap_or(1800)
        .max(60);

    let items = db.list_all_items()?;
    for item in items {
        let stamped = match item.donated_at {
            Some(t) => t,
            None => continue,
        };

        if now - stamped < decay_secs {
            continue;
        }

        // Defensive: only sweep items still sitting in a room. Pickup paths
        // clear `donated_at`, but if a path missed it we don't want to
        // delete the item out from under a player.
        let room_id = match item.location {
            ItemLocation::Room(id) => id,
            _ => continue,
        };

        broadcast_to_room(connections, &room_id, &format!("{} crumbles to dust.", item.short_desc));
        let _ = db.delete_item(&item.id);
        debug!("Donated item {} decayed", item.name);
    }

    Ok(())
}
