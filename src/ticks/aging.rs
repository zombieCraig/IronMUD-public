//! Aging tick: async wrapper around `ironmud::aging::process_aging_tick`.
//!
//! The tick polls on a short wall-clock cadence; the body is a no-op unless
//! a new game day has started (gated inside the sync core).

use tokio::time::{interval, Duration};
use tracing::error;

use ironmud::{aging::process_aging_tick, db, SharedConnections};

/// Wall-clock interval between aging-tick polls. Actual work is gated inside
/// `process_aging_tick` by `AGING_LAST_CHECK_KEY`, so short polling is cheap.
pub const AGING_TICK_INTERVAL_SECS: u64 = 300;

pub async fn run_aging_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(AGING_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = process_aging_tick(&db, &connections) {
            error!("Aging tick error: {}", e);
        }
    }
}
