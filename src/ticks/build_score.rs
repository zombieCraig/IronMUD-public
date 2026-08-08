//! The builder-score tick.
//!
//! A thin tokio wrapper, exactly like `src/ticks/leaderboard.rs`. The work
//! lives lib-side in `crate::build_score::process_build_score_tick` so
//! integration tests can drive it without a runtime, and it runs inside
//! `spawn_blocking` because it deserialises the whole room, item, mobile,
//! quest and character trees.
//!
//! The interval fires once immediately, so scores exist at boot rather than
//! five minutes into it — a builder who logs in and sees a blank sheet assumes
//! the feature is broken.

use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::build_score::{BUILD_SCORE_TICK_INTERVAL_SECS, process_build_score_tick};
use ironmud::{SharedConnections, SharedState, db};

pub async fn run_build_score_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(BUILD_SCORE_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("build_score");

        let db = db.clone();
        let connections = connections.clone();
        let state = state.clone();
        let result = tokio::task::spawn_blocking(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            process_build_score_tick(&db, &connections, &state, now)
        })
        .await;

        match result {
            Ok(Err(e)) => error!("Build score tick failed: {}", e),
            Err(e) => error!("Build score tick panicked: {}", e),
            Ok(Ok(())) => {}
        }
    }
}
