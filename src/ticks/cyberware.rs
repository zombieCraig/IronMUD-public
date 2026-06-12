//! Tokio loop wrapper for the cyberware psyche tick (cyberpsychosis episode
//! rolls + CHA-erosion recalc). The actual processing logic lives in
//! `ironmud::cyberware` (lib-side) so integration tests can drive it
//! directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::cyberware::{PSYCHE_TICK_INTERVAL_SECS, process_psyche_tick};
use ironmud::{SharedConnections, db};

pub async fn run_psyche_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(PSYCHE_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("psyche");
        if let Err(e) = process_psyche_tick(&db, &connections) {
            error!("Psyche tick error: {}", e);
        }
    }
}
