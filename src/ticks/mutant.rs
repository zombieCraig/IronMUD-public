//! Tokio loop wrappers for the mutant ticks. The actual processing logic
//! lives in `ironmud::mutant` (lib-side) so integration tests can drive it
//! directly without spinning up the runtime.
//!
//! Two loops:
//! - mutation tick: re-asserts passive mutation buffs/traits (NO MP regen —
//!   the push economy is strictly self-harm-funded).
//! - rot tick: world contamination for every race; needs only db (room
//!   lookup) + connections.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::mutant::{MUTATION_TICK_INTERVAL_SECS, ROT_TICK_INTERVAL_SECS, process_mutation_tick, process_rot_tick};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_mutation_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(MUTATION_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("mutation");
        // Clone the definitions out of a short World lock; never hold the
        // World and Connections locks together.
        let defs = match state.lock() {
            Ok(w) => w.mutation_definitions.clone(),
            Err(_) => continue,
        };
        if let Err(e) = process_mutation_tick(&db, &connections, &defs) {
            error!("Mutation tick error: {}", e);
        }
    }
}

pub async fn run_rot_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(ROT_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("rot");
        if let Err(e) = process_rot_tick(&db, &connections) {
            error!("Rot tick error: {}", e);
        }
    }
}
