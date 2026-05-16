//! Tick liveness registry and watchdog.
//!
//! Background tick tasks call [`beat`] at the top of every loop iteration. A
//! single [`run_watchdog`] task wakes periodically, scans the registry, and
//! flags any tick that hasn't beat in more than `2 × expected_interval` —
//! the standard symptom of a panicked task (tokio drops them silently,
//! doesn't restart) or a hung iteration. Stale ticks are surfaced via
//! `warn!()` and `broadcast_to_builders` so admins online see them in chat.
//!
//! The healthy path emits **no log lines**. Only stale ticks produce output.
//!
//! Intervals are registered once at startup (in `main.rs`) next to each
//! `tokio::spawn` call. `beat` is idempotent and cheap: a single mutex
//! acquisition + hashmap insert per tick iteration.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::time::interval;
use tracing::warn;

use ironmud::SharedConnections;
use ironmud::session::broadcast_to_builders;

/// How often the watchdog inspects heartbeats for staleness.
pub const WATCHDOG_INTERVAL_SECS: u64 = 300;

/// A staleness threshold floor — a tick with a very short expected interval
/// (e.g. 1s) shouldn't be flagged just because the watchdog itself runs every
/// 5 minutes. We treat any tick that beat within the last 30 seconds as
/// healthy regardless of its declared interval.
const MIN_FRESH_WINDOW: Duration = Duration::from_secs(30);

/// Multiplier applied to a tick's expected interval to compute its staleness
/// threshold. A factor of 2 means a tick must miss two consecutive cycles
/// before it's flagged.
const STALE_MULTIPLIER: u32 = 2;

#[derive(Debug, Clone)]
pub struct TickStatus {
    pub name: &'static str,
    pub expected_interval: Duration,
    pub last_beat: Instant,
}

impl TickStatus {
    pub fn age(&self) -> Duration {
        self.last_beat.elapsed()
    }

    pub fn is_stale(&self) -> bool {
        let threshold = self
            .expected_interval
            .saturating_mul(STALE_MULTIPLIER)
            .max(MIN_FRESH_WINDOW);
        self.age() > threshold
    }
}

struct Registry {
    // (name) -> (expected interval, last beat)
    map: Mutex<HashMap<&'static str, (Duration, Instant)>>,
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| Registry {
        map: Mutex::new(HashMap::new()),
    })
}

/// Declare a tick's expected cadence. Call once at startup before the tick
/// task is spawned. Safe to call multiple times — the latest registration
/// wins. Initializes the entry with `last_beat = now` so the watchdog never
/// flags a tick before it has had a chance to beat.
pub fn register(name: &'static str, expected_interval: Duration) {
    let reg = registry();
    if let Ok(mut map) = reg.map.lock() {
        let now = Instant::now();
        map.entry(name)
            .and_modify(|(int, _)| *int = expected_interval)
            .or_insert((expected_interval, now));
    }
}

/// Record a heartbeat for the named tick. Called once per tick iteration.
/// If the tick wasn't registered, this is a no-op — register at startup
/// so the watchdog has an expected interval to compare against.
pub fn beat(name: &'static str) {
    let reg = registry();
    if let Ok(mut map) = reg.map.lock() {
        if let Some(entry) = map.get_mut(name) {
            entry.1 = Instant::now();
        }
    }
}

/// Snapshot the registry. Sorted by name for deterministic admin display.
pub fn snapshot() -> Vec<TickStatus> {
    let reg = registry();
    let Ok(map) = reg.map.lock() else { return Vec::new() };
    let mut out: Vec<TickStatus> = map
        .iter()
        .map(|(name, (interval, last))| TickStatus {
            name,
            expected_interval: *interval,
            last_beat: *last,
        })
        .collect();
    out.sort_by_key(|s| s.name);
    out
}

/// Return only stale entries. Used by the watchdog and by tests.
pub fn stale() -> Vec<TickStatus> {
    snapshot().into_iter().filter(|s| s.is_stale()).collect()
}

/// Background watchdog: wakes every [`WATCHDOG_INTERVAL_SECS`] and surfaces
/// stale ticks via `warn!()` and `broadcast_to_builders`. Silent on the
/// healthy path.
pub async fn run_watchdog(connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(WATCHDOG_INTERVAL_SECS));
    // tokio's interval fires immediately on the first tick — skip that to
    // avoid alerting before any tick has even spun up.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let stale_ticks = stale();
        for s in stale_ticks {
            let msg = format!(
                "[heartbeat] Tick `{}` last beat {}s ago (expected every {}s — task may be dead)",
                s.name,
                s.age().as_secs(),
                s.expected_interval.as_secs()
            );
            warn!("{}", msg);
            broadcast_to_builders(&connections, &msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All tests share a global registry, so they prefix names to stay
    /// isolated from one another.
    #[test]
    fn beat_after_register_keeps_status_fresh() {
        register("hb_test_fresh", Duration::from_secs(60));
        beat("hb_test_fresh");
        let snap = snapshot();
        let s = snap
            .iter()
            .find(|s| s.name == "hb_test_fresh")
            .expect("registered tick should appear in snapshot");
        assert!(s.age() < Duration::from_secs(1));
        assert!(!s.is_stale());
    }

    #[test]
    fn beat_without_register_is_noop() {
        // beat() never panics or creates orphan entries; without a prior
        // register() it simply does nothing.
        beat("hb_test_unregistered");
        let snap = snapshot();
        assert!(!snap.iter().any(|s| s.name == "hb_test_unregistered"));
    }

    #[test]
    fn stale_respects_min_fresh_window() {
        // A 1-second interval × 2 = 2s threshold, but we never flag faster
        // than MIN_FRESH_WINDOW (30s). A fresh beat must therefore not be
        // stale even with an aggressive expected_interval.
        register("hb_test_min_window", Duration::from_secs(1));
        beat("hb_test_min_window");
        let stale_now = stale();
        assert!(
            !stale_now.iter().any(|s| s.name == "hb_test_min_window"),
            "tick that beat <30s ago must never be stale"
        );
    }

    #[test]
    fn register_idempotent_updates_interval() {
        register("hb_test_idem", Duration::from_secs(60));
        register("hb_test_idem", Duration::from_secs(120));
        let snap = snapshot();
        let s = snap.iter().find(|s| s.name == "hb_test_idem").unwrap();
        assert_eq!(s.expected_interval, Duration::from_secs(120));
    }
}
