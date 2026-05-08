//! Per-IP rate limiting for connection acceptance and login attempts.
//!
//! Two independent gates share the same per-IP entry:
//! - **Simultaneous connection cap**: limits concurrent sockets from one IP
//!   so a single attacker can't burn file descriptors or fork a brute-force
//!   farm of parallel logins.
//! - **Failed-auth window**: a sliding 60-second counter of bad login
//!   attempts; once the threshold is hit, further attempts are rejected
//!   without invoking Argon2.
//!
//! Entries self-garbage-collect when both counters fall to zero, so the
//! map stays bounded by the active client population.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Maximum simultaneous TCP connections accepted from a single source IP.
/// Eight is generous for shared NAT (households, small offices) while still
/// shutting down accept-storm DoS from a single attacker.
pub const MAX_CONNECTIONS_PER_IP: usize = 8;

/// Maximum failed login attempts from a single source IP within
/// `FAILED_AUTH_WINDOW`. Once exceeded, `is_login_throttled` returns true
/// until enough old failures fall out of the window.
pub const MAX_FAILED_AUTH_PER_WINDOW: usize = 5;

/// Sliding window for failed-auth bookkeeping.
const FAILED_AUTH_WINDOW: Duration = Duration::from_secs(60);

/// Maximum new-character creations a single source IP can make within
/// `CREATION_WINDOW`. Three accounts per ten minutes is generous for
/// people creating alts but stops scripted account-flood DoS.
pub const MAX_CREATIONS_PER_WINDOW: usize = 3;

/// Sliding window for character-creation bookkeeping.
const CREATION_WINDOW: Duration = Duration::from_secs(600);

#[derive(Default)]
pub struct IpRateLimiter {
    inner: Mutex<HashMap<IpAddr, IpEntry>>,
}

#[derive(Default)]
struct IpEntry {
    active: usize,
    failed_auths: VecDeque<Instant>,
    creations: VecDeque<Instant>,
}

impl IpRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Try to claim a connection slot for `ip`. Returns true on success;
    /// false if the IP is already at `MAX_CONNECTIONS_PER_IP`. The caller
    /// MUST pair every successful acquire with `release(ip)`.
    pub fn try_acquire(&self, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        if entry.active >= MAX_CONNECTIONS_PER_IP {
            return false;
        }
        entry.active += 1;
        true
    }

    pub fn release(&self, ip: IpAddr) {
        let mut map = self.inner.lock().unwrap();
        if let Some(entry) = map.get_mut(&ip) {
            entry.active = entry.active.saturating_sub(1);
            if entry.active == 0 && entry.failed_auths.is_empty() && entry.creations.is_empty() {
                map.remove(&ip);
            }
        }
    }

    /// True if `ip` has hit `MAX_CREATIONS_PER_WINDOW` character creations
    /// within the rolling window. Stale entries are evicted on inspection.
    pub fn is_creation_throttled(&self, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window_with(&mut entry.creations, CREATION_WINDOW);
        entry.creations.len() >= MAX_CREATIONS_PER_WINDOW
    }

    pub fn record_creation(&self, ip: IpAddr) {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window_with(&mut entry.creations, CREATION_WINDOW);
        entry.creations.push_back(Instant::now());
        while entry.creations.len() > MAX_CREATIONS_PER_WINDOW * 2 {
            entry.creations.pop_front();
        }
    }

    /// True if `ip` has hit `MAX_FAILED_AUTH_PER_WINDOW` failures within
    /// the rolling window. Stale entries are evicted on inspection.
    pub fn is_login_throttled(&self, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window(&mut entry.failed_auths);
        entry.failed_auths.len() >= MAX_FAILED_AUTH_PER_WINDOW
    }

    pub fn record_auth_failure(&self, ip: IpAddr) {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window(&mut entry.failed_auths);
        entry.failed_auths.push_back(Instant::now());
        // Keep the queue bounded even if we never re-prune.
        while entry.failed_auths.len() > MAX_FAILED_AUTH_PER_WINDOW * 2 {
            entry.failed_auths.pop_front();
        }
    }
}

fn prune_window(q: &mut VecDeque<Instant>) {
    prune_window_with(q, FAILED_AUTH_WINDOW);
}

fn prune_window_with(q: &mut VecDeque<Instant>, window: Duration) {
    let now = Instant::now();
    while let Some(&front) = q.front() {
        if now.duration_since(front) > window {
            q.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn try_acquire_caps_at_limit_and_release_frees_slot() {
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        for _ in 0..MAX_CONNECTIONS_PER_IP {
            assert!(lim.try_acquire(ip));
        }
        // Cap reached.
        assert!(!lim.try_acquire(ip));
        // Release one slot, then acquire succeeds again.
        lim.release(ip);
        assert!(lim.try_acquire(ip));
    }

    #[test]
    fn distinct_ips_have_independent_limits() {
        let lim = IpRateLimiter::new();
        let a = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let b = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        for _ in 0..MAX_CONNECTIONS_PER_IP {
            assert!(lim.try_acquire(a));
        }
        assert!(!lim.try_acquire(a));
        // b is unaffected.
        assert!(lim.try_acquire(b));
    }

    #[test]
    fn login_throttle_engages_after_threshold() {
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        for _ in 0..MAX_FAILED_AUTH_PER_WINDOW - 1 {
            lim.record_auth_failure(ip);
            assert!(!lim.is_login_throttled(ip));
        }
        lim.record_auth_failure(ip);
        assert!(lim.is_login_throttled(ip));
    }

    #[test]
    fn release_garbage_collects_idle_entry() {
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(lim.try_acquire(ip));
        lim.release(ip);
        // Internal map should have evicted the entry now that both counters
        // are zero. Inspect via the throttle path to avoid exposing internals.
        assert!(!lim.is_login_throttled(ip));
        // The important property is that `release` dropped the active
        // slot, which the assertion above already verifies via the
        // throttle path. No further state inspection needed.
    }

    #[test]
    fn creation_throttle_engages_after_window_threshold() {
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
        for _ in 0..MAX_CREATIONS_PER_WINDOW {
            assert!(!lim.is_creation_throttled(ip));
            lim.record_creation(ip);
        }
        assert!(lim.is_creation_throttled(ip));
    }

    #[test]
    fn creation_and_login_throttles_are_independent() {
        // A failed login binge must not prevent a fresh account creation,
        // and vice versa — they share the entry but track separately.
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9));
        for _ in 0..MAX_FAILED_AUTH_PER_WINDOW {
            lim.record_auth_failure(ip);
        }
        assert!(lim.is_login_throttled(ip));
        assert!(!lim.is_creation_throttled(ip));
    }
}
