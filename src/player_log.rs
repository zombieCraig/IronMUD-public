//! Per-player activity log: what each logged-in player typed, where, and how
//! the server answered (ok / unknown command / denied / ...), plus a summary
//! row per finished session. Lets an admin review how real players actually
//! play — `playlog` in game, `/api/v1/logs/players` over HTTP, and the
//! `list_player_sessions` / `get_player_log` MCP tools.
//!
//! Capture lives in `handle_connection` via [`SessionTracker`]; storage is two
//! sled trees on [`crate::db::Db`]. Only typed input is stored, never server
//! output. Input from wizard / editor / mail-compose modes is not recorded,
//! and the arguments of [`SENSITIVE_COMMANDS`] are redacted.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::Db;
use crate::{ConnectionId, SharedConnections};

/// Entries kept per character before the oldest are dropped.
pub const PLAYER_LOG_MAX_PER_CHAR: usize = 5000;
/// Entries older than this are dropped when the character next logs in.
pub const PLAYER_LOG_MAX_AGE_SECS: i64 = 90 * 24 * 60 * 60;
/// Finished-session summaries retained across all players.
pub const PLAYER_SESSION_RING_SIZE: usize = 2000;

/// Commands whose arguments are never stored (credentials, email addresses).
pub const SENSITIVE_COMMANDS: &[&str] = &["password", "email", "login", "create", "forgot"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerLogKind {
    Login,
    Logout,
    Command,
}

impl PlayerLogKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlayerLogKind::Login => "login",
            PlayerLogKind::Logout => "logout",
            PlayerLogKind::Command => "command",
        }
    }
}

/// How the server handled a logged command.
pub mod outcome {
    pub const OK: &str = "ok";
    pub const UNKNOWN: &str = "unknown";
    pub const DENIED: &str = "denied";
    pub const UNCONSCIOUS: &str = "unconscious";
    pub const SCRIPT_ERROR: &str = "script_error";
    pub const SOCIAL: &str = "social";
    pub const DG_SUPPRESSED: &str = "dg_suppressed";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLogEntry {
    /// Unix seconds.
    pub ts: i64,
    /// Connection id of the session this entry belongs to.
    pub session_id: String,
    pub kind: PlayerLogKind,
    /// Vnum (or uuid when the room has no vnum) of the room the player was in.
    #[serde(default)]
    pub room: Option<String>,
    /// Command line as typed (redacted for sensitive commands), or a short
    /// description for login/logout entries.
    pub text: String,
    /// One of the [`outcome`] tags; commands only.
    #[serde(default)]
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSessionSummary {
    pub name: String,
    pub session_id: String,
    pub ip: String,
    #[serde(default)]
    pub client: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
    /// "quit", "logout", "linkdead", "switch".
    pub end_reason: String,
    pub commands: u32,
    pub unknown_commands: u32,
}

/// True when `line` is (an abbreviation of) a command that carries a secret,
/// so it must not be kept in command history. Errs on the side of skipping.
pub fn is_sensitive_line(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let w = first.to_lowercase();
    if w.chars().count() >= 3 && SENSITIVE_COMMANDS.iter().any(|s| s.starts_with(&w)) {
        return true;
    }
    if "admin".starts_with(&w) && w.len() >= 2 {
        let sub = words.next().unwrap_or("");
        let key = words.next().unwrap_or("");
        return sub.eq_ignore_ascii_case("config") && crate::email::is_secret_setting(key);
    }
    false
}

/// Replace a sensitive command's arguments; leave everything else untouched.
pub fn redact(resolved_command: &str, line: &str) -> String {
    // `admin config <secret-key> <value>`: keep the key, drop the value.
    if resolved_command == "admin" {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() > 3 && words[1].eq_ignore_ascii_case("config") && crate::email::is_secret_setting(words[2]) {
            return format!("{} {} {} [redacted]", words[0], words[1], words[2]);
        }
    }
    if SENSITIVE_COMMANDS.contains(&resolved_command) {
        let verb = line.split_whitespace().next().unwrap_or(resolved_command);
        if line.trim() == verb {
            verb.to_string()
        } else {
            format!("{verb} [redacted]")
        }
    } else {
        line.to_string()
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Snapshot of who issued a command and where, taken before dispatch so the
/// entry still records correctly when the command itself ends the session
/// (`quit`) or moves the player.
pub struct CommandContext {
    room_id: uuid::Uuid,
}

/// Per-connection state for the activity log. Owned by `handle_connection`.
pub struct SessionTracker {
    db: Db,
    connection_id: ConnectionId,
    ip: String,
    name: Option<String>,
    client: Option<String>,
    started_at: i64,
    commands: u32,
    unknown_commands: u32,
    /// Entries written since the last trim. Trimming only at login let one
    /// long session grow a character's log without bound.
    writes_since_trim: std::cell::Cell<u32>,
}

/// Trim the character's log after this many writes within a session.
pub const PLAYER_LOG_TRIM_EVERY: u32 = 200;

impl SessionTracker {
    pub fn new(db: Db, connection_id: ConnectionId, addr: std::net::SocketAddr) -> Self {
        Self {
            db,
            connection_id,
            ip: addr.ip().to_string(),
            name: None,
            client: None,
            started_at: 0,
            commands: 0,
            unknown_commands: 0,
            writes_since_trim: std::cell::Cell::new(0),
        }
    }

    /// Notice login / logout / character switches. Call once per input loop
    /// iteration, before dispatching the next command.
    pub fn sync(&mut self, connections: &SharedConnections) {
        let (current, room_id, client) = {
            let conns = connections.lock();
            match conns.get(&self.connection_id) {
                Some(s) => (
                    s.character.as_ref().map(|c| c.name.clone()),
                    s.character.as_ref().map(|c| c.current_room_id),
                    s.telnet_state.client_name.clone(),
                ),
                None => (None, None, None),
            }
        };
        if current == self.name {
            return;
        }
        if self.name.is_some() {
            let reason = if current.is_some() { "switch" } else { "logout" };
            self.finish(reason);
        }
        if let Some(name) = current {
            self.started_at = now_secs();
            self.commands = 0;
            self.unknown_commands = 0;
            self.client = client;
            info!("Login: {} from {}", name, self.ip);
            let text = match &self.client {
                Some(c) => format!("from {} ({})", self.ip, c),
                None => format!("from {}", self.ip),
            };
            self.write(&name, PlayerLogKind::Login, room_id, text, None, true);
            self.name = Some(name);
        }
    }

    /// Close the current session, if any. `reason` lands in the summary.
    pub fn finish(&mut self, reason: &str) {
        let Some(name) = self.name.take() else {
            return;
        };
        let ended_at = now_secs();
        let seconds = (ended_at - self.started_at).max(0);
        info!(
            "Logout: {} ({}, {}s, {} commands)",
            name, reason, seconds, self.commands
        );
        self.write(
            &name,
            PlayerLogKind::Logout,
            None,
            format!("{reason} after {seconds}s, {} commands", self.commands),
            None,
            false,
        );
        let summary = PlayerSessionSummary {
            name,
            session_id: self.connection_id.to_string(),
            ip: self.ip.clone(),
            client: self.client.clone(),
            started_at: self.started_at,
            ended_at,
            end_reason: reason.to_string(),
            commands: self.commands,
            unknown_commands: self.unknown_commands,
        };
        if let Err(e) = self.db.record_player_session(&summary) {
            warn!("player_log: failed to record session summary: {}", e);
        }
    }

    /// Capture who/where before a command runs. `None` when not logged in,
    /// which keeps pre-login input (`login <name> <password>`) out of the log.
    pub fn context(&self, connections: &SharedConnections) -> Option<CommandContext> {
        self.name.as_ref()?;
        let conns = connections.lock();
        conns
            .get(&self.connection_id)
            .and_then(|s| s.character.as_ref())
            .map(|c| CommandContext {
                room_id: c.current_room_id,
            })
    }

    pub fn command(&mut self, ctx: &Option<CommandContext>, resolved_command: &str, line: &str, outcome: &str) {
        let (Some(ctx), Some(name)) = (ctx, self.name.clone()) else {
            return;
        };
        self.commands += 1;
        if outcome == outcome::UNKNOWN {
            self.unknown_commands += 1;
        }
        let text = redact(resolved_command, line);
        self.write(
            &name,
            PlayerLogKind::Command,
            Some(ctx.room_id),
            text,
            Some(outcome.to_string()),
            false,
        );
    }

    fn write(
        &self,
        name: &str,
        kind: PlayerLogKind,
        room_id: Option<uuid::Uuid>,
        text: String,
        outcome: Option<String>,
        trim: bool,
    ) {
        let room = room_id.map(|id| match self.db.get_room_data(&id) {
            Ok(Some(r)) => r.vnum.unwrap_or_else(|| id.to_string()),
            _ => id.to_string(),
        });
        let entry = PlayerLogEntry {
            ts: now_secs(),
            session_id: self.connection_id.to_string(),
            kind,
            room,
            text,
            outcome,
        };
        let n = self.writes_since_trim.get() + 1;
        let trim = trim || n >= PLAYER_LOG_TRIM_EVERY;
        self.writes_since_trim.set(if trim { 0 } else { n });
        if let Err(e) = self.db.record_player_log(name, &entry, trim) {
            warn!("player_log: failed to record entry for {}: {}", name, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_sensitive_args_only() {
        assert_eq!(redact("password", "password old new"), "password [redacted]");
        assert_eq!(redact("password", "pass"), "pass");
        assert_eq!(
            redact("admin", "admin config smtp_pass hunter2"),
            "admin config smtp_pass [redacted]"
        );
        assert_eq!(redact("admin", "admin config motd Hello"), "admin config motd Hello");
        assert!(is_sensitive_line("password hunter2 hunter3"));
        assert!(is_sensitive_line("passw x"));
        assert!(is_sensitive_line("admin config smtp_pass x"));
        assert!(!is_sensitive_line("look"));
        assert!(!is_sensitive_line("say my password"));
        assert_eq!(redact("email", "email me@example.com"), "email [redacted]");
        assert_eq!(
            redact("say", "say my password is hunter2"),
            "say my password is hunter2"
        );
    }

    #[test]
    fn tracker_records_login_commands_and_logout() {
        use parking_lot::Mutex;
        use std::collections::HashMap;
        use std::sync::Arc;

        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let conn_id = uuid::Uuid::new_v4();
        let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        conns
            .lock()
            .insert(conn_id, crate::PlayerSession::new_for_test(tx_client, tx_input));
        std::mem::forget((rx, rx_in));

        let mut tracker = SessionTracker::new(db.clone(), conn_id, "127.0.0.1:4000".parse().unwrap());

        // Not logged in yet: nothing is captured, even a typed password.
        tracker.sync(&conns);
        let ctx = tracker.context(&conns);
        tracker.command(&ctx, "login", "login maya hunter2", outcome::OK);

        let ch: crate::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "Maya",
            "password_hash": "",
            "current_room_id": uuid::Uuid::new_v4(),
        }))
        .expect("build character");
        conns.lock().get_mut(&conn_id).unwrap().character = Some(ch);
        tracker.sync(&conns);
        let ctx = tracker.context(&conns);
        tracker.command(&ctx, "look", "look", outcome::OK);
        tracker.command(&ctx, "xyzzy", "xyzzy", outcome::UNKNOWN);
        tracker.command(&ctx, "password", "password old new", outcome::OK);

        conns.lock().remove(&conn_id);
        tracker.finish("quit");

        let log = db.list_player_log("maya", 0, 100).unwrap();
        let texts: Vec<_> = log.iter().map(|e| (e.kind, e.text.as_str())).collect();
        assert_eq!(texts[0].0, PlayerLogKind::Login);
        assert_eq!(texts[1], (PlayerLogKind::Command, "look"));
        assert_eq!(log[2].outcome.as_deref(), Some("unknown"));
        assert_eq!(texts[3], (PlayerLogKind::Command, "password [redacted]"));
        assert_eq!(texts[4].0, PlayerLogKind::Logout);
        assert!(log.iter().all(|e| !e.text.contains("hunter2")));

        let sessions = db.list_player_sessions(Some("maya"), 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].end_reason, "quit");
        assert_eq!(sessions[0].commands, 3);
        assert_eq!(sessions[0].unknown_commands, 1);
    }

    #[test]
    fn long_session_log_stays_bounded() {
        use parking_lot::Mutex;
        use std::collections::HashMap;
        use std::sync::Arc;

        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        let conns: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let conn_id = uuid::Uuid::new_v4();
        let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(
            serde_json::from_value(serde_json::json!({
                "name": "Rin", "password_hash": "", "current_room_id": uuid::Uuid::new_v4(),
            }))
            .unwrap(),
        );
        conns.lock().insert(conn_id, session);
        std::mem::forget((rx, rx_in));

        let mut tracker = SessionTracker::new(db.clone(), conn_id, "127.0.0.1:4000".parse().unwrap());
        tracker.sync(&conns);
        let ctx = tracker.context(&conns);
        let total = PLAYER_LOG_MAX_PER_CHAR + 3 * PLAYER_LOG_TRIM_EVERY as usize;
        for _ in 0..total {
            tracker.command(&ctx, "look", "look", outcome::OK);
        }
        let stored = db.list_player_log("rin", 0, usize::MAX).unwrap().len();
        assert!(
            stored <= PLAYER_LOG_MAX_PER_CHAR + PLAYER_LOG_TRIM_EVERY as usize,
            "log grew to {stored} in one session"
        );
    }
}
