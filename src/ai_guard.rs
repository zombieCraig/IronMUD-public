//! Cost and abuse limits for the AI description helpers (Claude / Gemini).
//!
//! Every request costs real money and an external API call, and the model's
//! reply is shown to the builder and can be saved into the world. This module
//! holds the shared limits:
//!
//! - one request in flight per connection;
//! - a per-character cooldown between requests;
//! - a daily cap across the whole server (`IRONMUD_AI_DAILY_CAP`);
//! - at most [`MAX_CONCURRENT_CALLS`] API calls at once;
//! - input size caps, and sanitizing of the model's output.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

/// Longest builder prompt sent to the model.
pub const MAX_PROMPT_CHARS: usize = 1000;
/// Longest existing text sent for a rephrase.
pub const MAX_EXISTING_CHARS: usize = 4000;
/// Longest value of each context field (title, area name, theme, ...).
pub const MAX_CONTEXT_CHARS: usize = 200;
/// Longest model output kept.
pub const MAX_OUTPUT_CHARS: usize = 4000;
/// Minimum gap between two requests from one character.
pub const PER_CHAR_COOLDOWN: Duration = Duration::from_secs(10);
/// API calls in progress at once, server-wide.
pub const MAX_CONCURRENT_CALLS: usize = 4;
/// Requests per UTC day, server-wide, unless `IRONMUD_AI_DAILY_CAP` says
/// otherwise.
pub const DEFAULT_DAILY_CAP: u32 = 500;
pub const DAILY_CAP_ENV: &str = "IRONMUD_AI_DAILY_CAP";

fn in_flight() -> &'static Mutex<HashSet<Uuid>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<Uuid>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

fn daily() -> &'static Mutex<(u64, u32)> {
    static DAILY: OnceLock<Mutex<(u64, u32)>> = OnceLock::new();
    DAILY.get_or_init(|| Mutex::new((0, 0)))
}

fn daily_cap() -> u32 {
    std::env::var(DAILY_CAP_ENV)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_DAILY_CAP)
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// Semaphore bounding concurrent API calls.
pub fn api_permits() -> &'static tokio::sync::Semaphore {
    static SEM: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_CALLS))
}

/// Admit a new request from `conn` on behalf of `who` (a character name, or
/// anything stable for the requester). On success the connection is marked
/// in flight until [`finish`]. On refusal returns the message to show.
pub fn begin(conn: Uuid, who: &str) -> Result<(), String> {
    let mut flight = in_flight().lock();
    if flight.contains(&conn) {
        return Err("An AI request is already in progress. Wait for it to finish.".to_string());
    }
    {
        let d = daily().lock();
        if d.0 == today() && d.1 >= daily_cap() {
            return Err("The server's daily AI request limit has been reached. Try again tomorrow.".to_string());
        }
    }
    if !crate::chat::cooldown_ok(&format!("ai\0{}", who.to_lowercase()), PER_CHAR_COOLDOWN) {
        return Err(format!(
            "Please wait {} seconds between AI requests.",
            PER_CHAR_COOLDOWN.as_secs()
        ));
    }
    {
        let mut d = daily().lock();
        let now = today();
        if d.0 != now {
            *d = (now, 0);
        }
        d.1 += 1;
    }
    flight.insert(conn);
    Ok(())
}

/// The request from `conn` has finished (delivered, failed, or abandoned).
pub fn finish(conn: Uuid) {
    in_flight().lock().remove(&conn);
}

/// Truncate builder input to `max` characters.
pub fn cap(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Make model output safe to show and store: no control characters other
/// than newline and tab (so no ANSI/telnet escapes), no `<`/`>` (so no MXP
/// tags in descriptions), and at most [`MAX_OUTPUT_CHARS`] characters.
pub fn clean_output(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .map(|c| match c {
            '<' => '(',
            '>' => ')',
            other => other,
        })
        .take(MAX_OUTPUT_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_request_in_flight_per_connection_and_per_char_cooldown() {
        let conn = Uuid::new_v4();
        let who = format!("tester-{conn}");
        assert!(begin(conn, &who).is_ok());
        assert!(begin(conn, &who).unwrap_err().contains("already in progress"));
        finish(conn);
        // Cooldown applies even from another connection.
        let err = begin(Uuid::new_v4(), &who).unwrap_err();
        assert!(err.contains("wait"), "{err}");
    }

    #[test]
    fn output_is_cleaned() {
        let out = clean_output("A <send>room</send>\x1b[2J here.\nLine two\x07");
        assert_eq!(out, "A (send)room(/send)[2J here.\nLine two");
        assert_eq!(clean_output(&"x".repeat(MAX_OUTPUT_CHARS + 10)).len(), MAX_OUTPUT_CHARS);
    }
}
