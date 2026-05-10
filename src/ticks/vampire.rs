//! Tokio loop wrapper for the vampire ticks. The actual processing logic
//! lives in `ironmud::vampire` (lib-side) so integration tests can drive
//! it directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::vampire::{
    BLOOD_TICK_INTERVAL_SECS, SUN_TICK_INTERVAL_SECS, process_blood_tick, process_sun_tick,
};
use ironmud::{SharedConnections, db};

pub async fn run_sun_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(SUN_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = process_sun_tick(&db, &connections) {
            error!("Sun tick error: {}", e);
        }
    }
}

pub async fn run_blood_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(BLOOD_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = process_blood_tick(&db, &connections) {
            error!("Blood tick error: {}", e);
        }
    }
}
