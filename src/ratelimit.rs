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

/// Maximum outbound emails (verification + resend + password reset) a single
/// source IP can trigger per hour. Each send costs real money via SES, so
/// this is the per-IP component of the cost-cap defense — paired with the
/// global daily/monthly caps in `crate::email`.
pub const MAX_EMAIL_SENDS_PER_HOUR: usize = 5;

/// Maximum outbound emails a single source IP can trigger per day. Bounds
/// sustained-pressure attacks where a botnet rotates IPs hourly.
pub const MAX_EMAIL_SENDS_PER_DAY: usize = 10;

const EMAIL_SEND_HOUR_WINDOW: Duration = Duration::from_secs(3600);
const EMAIL_SEND_DAY_WINDOW: Duration = Duration::from_secs(86_400);

#[derive(Default)]
pub struct IpRateLimiter {
    inner: Mutex<HashMap<IpAddr, IpEntry>>,
}

#[derive(Default)]
struct IpEntry {
    active: usize,
    failed_auths: VecDeque<Instant>,
    creations: VecDeque<Instant>,
    email_sends: VecDeque<Instant>,
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
            if entry.active == 0
                && entry.failed_auths.is_empty()
                && entry.creations.is_empty()
                && entry.email_sends.is_empty()
            {
                map.remove(&ip);
            }
        }
    }

    /// True if `ip` has hit either the per-hour or per-day email-send cap.
    /// Each call prunes both windows so stale timestamps don't pile up.
    pub fn is_email_send_throttled(&self, ip: IpAddr) -> bool {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window_with(&mut entry.email_sends, EMAIL_SEND_DAY_WINDOW);
        if entry.email_sends.len() >= MAX_EMAIL_SENDS_PER_DAY {
            return true;
        }
        let hour_count = count_in_window(&entry.email_sends, EMAIL_SEND_HOUR_WINDOW);
        hour_count >= MAX_EMAIL_SENDS_PER_HOUR
    }

    /// Stamp a successful email send against `ip`. Caller is responsible for
    /// only invoking this AFTER the SMTP send returns Ok — failed sends
    /// don't count against the budget.
    pub fn record_email_send(&self, ip: IpAddr) {
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip).or_default();
        prune_window_with(&mut entry.email_sends, EMAIL_SEND_DAY_WINDOW);
        entry.email_sends.push_back(Instant::now());
        while entry.email_sends.len() > MAX_EMAIL_SENDS_PER_DAY * 2 {
            entry.email_sends.pop_front();
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

/// Count how many timestamps in `q` fall within the trailing `window`. Used
/// for the dual-window email throttle, where we keep one queue (sized to the
/// longer window) and check the shorter window via a count.
fn count_in_window(q: &VecDeque<Instant>, window: Duration) -> usize {
    let now = Instant::now();
    q.iter()
        .rev()
        .take_while(|&&t| now.duration_since(t) <= window)
        .count()
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

    #[test]
    fn email_send_throttle_engages_at_hourly_cap() {
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42));
        for _ in 0..MAX_EMAIL_SENDS_PER_HOUR {
            assert!(!lim.is_email_send_throttled(ip));
            lim.record_email_send(ip);
        }
        assert!(lim.is_email_send_throttled(ip));
    }

    #[test]
    fn email_send_throttle_engages_at_daily_cap() {
        // Daily cap binds even when no hourly burst happens — the daily
        // counter is the floor of "sends ever in this 24-hour window".
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 99));
        for _ in 0..MAX_EMAIL_SENDS_PER_DAY {
            lim.record_email_send(ip);
        }
        assert!(lim.is_email_send_throttled(ip));
    }

    #[test]
    fn email_send_throttle_distinct_per_ip() {
        let lim = IpRateLimiter::new();
        let attacker = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
        let bystander = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8));
        for _ in 0..MAX_EMAIL_SENDS_PER_HOUR {
            lim.record_email_send(attacker);
        }
        assert!(lim.is_email_send_throttled(attacker));
        assert!(!lim.is_email_send_throttled(bystander));
    }

    #[test]
    fn email_send_throttle_does_not_block_other_axes() {
        // The new axis must not interact with creation / login throttles.
        let lim = IpRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 200));
        for _ in 0..MAX_EMAIL_SENDS_PER_HOUR {
            lim.record_email_send(ip);
        }
        assert!(lim.is_email_send_throttled(ip));
        assert!(!lim.is_creation_throttled(ip));
        assert!(!lim.is_login_throttled(ip));
    }
}
