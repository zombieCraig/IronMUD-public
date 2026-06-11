//! Tokio loop wrapper for the replicant resolve tick. The actual processing
//! logic lives in `ironmud::replicant` (lib-side) so integration tests can
//! drive it directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::replicant::{RESOLVE_TICK_INTERVAL_SECS, process_resolve_tick};
use ironmud::{SharedConnections, db};

pub async fn run_resolve_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(RESOLVE_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("resolve");
        if let Err(e) = process_resolve_tick(&db, &connections) {
            error!("Resolve tick error: {}", e);
        }
    }
}
