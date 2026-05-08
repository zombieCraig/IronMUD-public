//! Per-character per-command cooldown bookkeeping.
//!
//! Wide-blast chat commands (`shout`, `tell`, future global channels)
//! would otherwise let a single player or scripted bot spam every
//! online player. A short cooldown stops the spam without inhibiting
//! normal conversation cadence.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct CommandThrottle {
    inner: Mutex<HashMap<(String, String), Instant>>,
}

impl CommandThrottle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically check-and-stamp the cooldown for `(player, command)`.
    /// Returns `0` if the call is allowed (and the timestamp is updated),
    /// otherwise the integer number of whole seconds the caller still
    /// has to wait. The returned remaining time is always >= 1 when the
    /// throttle is engaged so the caller can render an accurate message.
    pub fn try_consume(&self, player: &str, command: &str, cooldown: Duration) -> u64 {
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        let key = (player.to_lowercase(), command.to_string());
        if let Some(last) = map.get(&key) {
            let elapsed = now.duration_since(*last);
            if elapsed < cooldown {
                let remaining = cooldown - elapsed;
                let secs = remaining.as_secs();
                return if remaining.subsec_nanos() > 0 { secs + 1 } else { secs.max(1) };
            }
        }
        map.insert(key, now);
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn first_call_allowed() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("alice", "shout", Duration::from_secs(5)), 0);
    }

    #[test]
    fn second_call_within_cooldown_returns_remaining() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("alice", "shout", Duration::from_secs(5)), 0);
        let remaining = t.try_consume("alice", "shout", Duration::from_secs(5));
        assert!(remaining >= 1 && remaining <= 5, "remaining was {}", remaining);
    }

    #[test]
    fn cooldown_releases_after_window() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("alice", "shout", Duration::from_millis(50)), 0);
        sleep(Duration::from_millis(80));
        assert_eq!(t.try_consume("alice", "shout", Duration::from_millis(50)), 0);
    }

    #[test]
    fn separate_commands_have_independent_cooldowns() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("alice", "shout", Duration::from_secs(5)), 0);
        assert_eq!(t.try_consume("alice", "tell", Duration::from_secs(5)), 0);
    }

    #[test]
    fn separate_players_have_independent_cooldowns() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("alice", "shout", Duration::from_secs(5)), 0);
        assert_eq!(t.try_consume("bob", "shout", Duration::from_secs(5)), 0);
    }

    #[test]
    fn player_lookup_is_case_insensitive() {
        let t = CommandThrottle::new();
        assert_eq!(t.try_consume("Alice", "shout", Duration::from_secs(5)), 0);
        let remaining = t.try_consume("alice", "shout", Duration::from_secs(5));
        assert!(remaining >= 1, "case-insensitive match should engage throttle (got {})", remaining);
    }
}
