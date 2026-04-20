//! Migration tick: async wrapper around `ironmud::migration::process_migration_tick`.
//!
//! The core logic lives in the library's `migration` module so it's reachable
//! from integration tests without the tokio runtime.

use std::path::PathBuf;
use tokio::time::{interval, Duration};
use tracing::error;

use ironmud::{
    db,
    migration::{load_migration_data, process_migration_tick},
    SharedConnections,
};

/// Wall-clock interval between migration ticks. The game-day interval per
/// area is independently enforced inside the tick.
pub const MIGRATION_TICK_INTERVAL_SECS: u64 = 300;

/// Background task that runs the migration tick on a fixed wall-clock cadence.
pub async fn run_migration_tick(db: db::Db, connections: SharedConnections, data_dir: PathBuf) {
    let mut ticker = interval(Duration::from_secs(MIGRATION_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        match load_migration_data(&data_dir) {
            Ok(data) => {
                if let Err(e) = process_migration_tick(&db, &connections, &data) {
                    error!("Migration tick error: {}", e);
                }
            }
            Err(e) => error!("Migration tick: failed to load migration data: {}", e),
        }
    }
}
