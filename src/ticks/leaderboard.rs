//! Tokio loop wrapper for the leaderboard scan. The work lives in
//! `ironmud::leaderboard` (lib-side) so integration tests can drive it
//! directly without spinning up the runtime.
//!
//! `interval` fires once immediately, so the boards are populated within a
//! moment of boot rather than five minutes into it — `top` is only ever empty
//! on a world with no qualifying characters.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::leaderboard::{LEADERBOARD_TICK_INTERVAL_SECS, process_leaderboard_tick};
use ironmud::{SharedState, db};

pub async fn run_leaderboard_tick(db: db::Db, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(LEADERBOARD_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("leaderboard");
        // On a blocking thread: the scan deserializes the entire character
        // tree, which is the heaviest read the server performs, and doing it
        // inline would park a runtime worker for its whole duration. Nothing
        // else here needs to be async, so the wrapper is the natural place to
        // pay for it.
        let db = db.clone();
        let state = state.clone();
        let result = tokio::task::spawn_blocking(move || process_leaderboard_tick(&db, &state)).await;
        match result {
            Ok(Err(e)) => error!("Leaderboard tick error: {}", e),
            Err(e) => error!("Leaderboard tick panicked: {}", e),
            Ok(Ok(())) => {}
        }
    }
}
