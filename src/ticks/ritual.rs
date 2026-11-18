//! Tokio loop wrapper for the divine spellcraft ritual tick. The logic lives
//! in `ironmud::spellcraft::ritual` so tests can drive it with fixed dice.

use tokio::time::{Duration, interval};

use ironmud::spellcraft::ritual::{RITUAL_TICK_SECS, run_tick_once};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_ritual_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(RITUAL_TICK_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("ritual");
        run_tick_once(&db, &connections, &state);
    }
}
