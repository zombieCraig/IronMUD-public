//! Tick task supervision.
//!
//! Tokio drops a panicked task silently and never restarts it, so one panic
//! in a tick (a bad record, an unexpected `None`) used to stop that system
//! until the next server restart — the heartbeat watchdog would only report
//! it. [`supervise`] runs a tick's loop in a child task and, when the child
//! panics, logs the panic, tells online builders, and starts a fresh loop
//! after a short backoff.
//!
//! A restarted loop begins from scratch (new interval timer, any loop-local
//! state reset), which every tick already tolerates: they re-read their
//! state from the database and sessions on each iteration.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use tracing::{error, warn};

use ironmud::SharedConnections;
use ironmud::session::broadcast_to_builders;

/// A loop that stays up this long is considered healthy again, so a later
/// panic starts the backoff from the bottom.
const HEALTHY_RUN: Duration = Duration::from_secs(300);
/// Upper bound on the wait between restarts of a loop that keeps panicking.
const MAX_BACKOFF_SECS: u64 = 60;

/// Delay before restart number `restarts` (1-based): 1s, 2s, 4s ... capped.
pub fn backoff_for(restarts: u32) -> Duration {
    let exp = restarts.saturating_sub(1).min(16);
    Duration::from_secs((1u64 << exp).min(MAX_BACKOFF_SECS))
}

/// Best-effort text of a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Spawn `make()` as a supervised task named `name`. `make` builds a fresh
/// loop future each time, so it must clone what it captures.
pub fn supervise<F, Fut>(name: &'static str, connections: SharedConnections, make: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut restarts: u32 = 0;
        loop {
            let started = Instant::now();
            match tokio::spawn(make()).await {
                Ok(()) => {
                    warn!("Tick task '{}' returned; not restarting", name);
                    return;
                }
                Err(e) if e.is_panic() => {
                    if started.elapsed() >= HEALTHY_RUN {
                        restarts = 0;
                    }
                    restarts = restarts.saturating_add(1);
                    let detail = panic_message(&*e.into_panic());
                    let wait = backoff_for(restarts);
                    error!(
                        "Tick task '{}' panicked: {} — restarting in {}s (restart #{})",
                        name,
                        detail,
                        wait.as_secs(),
                        restarts
                    );
                    // The panic may have poisoned the connections lock; a
                    // second panic here must not take the supervisor down.
                    let msg = format!("[tick] {} panicked and is restarting: {}", name, detail);
                    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| broadcast_to_builders(&connections, &msg)));
                    tokio::time::sleep(wait).await;
                }
                Err(_) => {
                    warn!("Tick task '{}' was cancelled", name);
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_for(1), Duration::from_secs(1));
        assert_eq!(backoff_for(2), Duration::from_secs(2));
        assert_eq!(backoff_for(4), Duration::from_secs(8));
        assert_eq!(backoff_for(10), Duration::from_secs(MAX_BACKOFF_SECS));
        assert_eq!(backoff_for(u32::MAX), Duration::from_secs(MAX_BACKOFF_SECS));
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_loop_is_restarted() {
        let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let runs = Arc::new(AtomicU32::new(0));
        let counter = runs.clone();
        supervise("test_tick", connections, move || {
            let counter = counter.clone();
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    panic!("boom {n}");
                }
                // Third run stays up.
                std::future::pending::<()>().await;
            }
        });
        // Paused clock: advancing past the 1s + 2s backoffs lets both
        // restarts happen.
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }
}
