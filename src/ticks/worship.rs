//! Tokio loop wrapper for the worship anger tick. The actual processing
//! logic lives in `ironmud::worship` (lib-side) so integration tests can
//! drive it directly without spinning up the runtime.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::worship::{WORSHIP_TICK_INTERVAL_SECS, process_worship_tick};
use ironmud::{SharedConnections, db};

pub async fn run_worship_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(WORSHIP_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("worship");
        if let Err(e) = process_worship_tick(&db, &connections) {
            error!("Worship tick error: {}", e);
        }
    }
}
