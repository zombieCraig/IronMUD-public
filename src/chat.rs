//! Chat bridge module for IronMUD
//!
//! Provides a unified message type and fan-out bridge for chat integrations
//! (Matrix, Discord, etc.). Game code sends messages to a single ChatSender,
//! and the bridge distributes them to all configured backends.
//!
//! Inbound commands from every platform (`!who`, `!tell`, `!help`) are
//! handled here by [`handle_bridge_command`], so the safety rules (sanitized
//! text, length caps, ignore lists, per-sender cooldown) are written once.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::warn;

use crate::{SharedConnections, find_player_connection_by_name, get_online_players, send_client_message};

/// Message type for chat integrations. All messages are broadcasts.
#[derive(Clone, Debug)]
pub enum ChatMessage {
    Broadcast(String),
}

/// Sender handle for game code to send messages to chat backends
pub type ChatSender = mpsc::UnboundedSender<ChatMessage>;

/// Outbound messages the bridge forwards per [`OUTBOUND_WINDOW`]. Anything
/// past that is dropped: players can drive broadcasts (`pray`, logins), and
/// an unthrottled flood gets the bot rate-limited or banned by the platform.
pub const OUTBOUND_PER_WINDOW: usize = 20;
pub const OUTBOUND_WINDOW: Duration = Duration::from_secs(10);

/// Run the chat bridge, fanning out messages to all registered backends.
/// Each backend has its own unbounded channel receiver.
pub async fn run_chat_bridge(
    mut rx: mpsc::UnboundedReceiver<ChatMessage>,
    backends: Vec<mpsc::UnboundedSender<ChatMessage>>,
) {
    let mut window_start = Instant::now();
    let mut sent_in_window = 0usize;
    let mut dropped_in_window = 0usize;
    while let Some(msg) = rx.recv().await {
        if window_start.elapsed() >= OUTBOUND_WINDOW {
            if dropped_in_window > 0 {
                warn!(
                    "Chat bridge dropped {} message(s) over the rate limit",
                    dropped_in_window
                );
            }
            window_start = Instant::now();
            sent_in_window = 0;
            dropped_in_window = 0;
        }
        if sent_in_window >= OUTBOUND_PER_WINDOW {
            dropped_in_window += 1;
            continue;
        }
        sent_in_window += 1;
        for backend in &backends {
            if let Err(e) = backend.send(msg.clone()) {
                warn!("Chat backend channel closed: {}", e);
            }
        }
    }
}

/// Longest `!tell` message relayed into the game.
pub const MAX_TELL_CHARS: usize = 400;
/// Longest sender name shown in game.
pub const MAX_SENDER_CHARS: usize = 64;
/// Minimum gap between two `!tell`s from the same external sender.
pub const TELL_COOLDOWN: Duration = Duration::from_secs(3);

/// Handle an inbound bridge command. `platform` is the display tag
/// ("Discord", "Matrix"); `sender` must be an identity the platform
/// guarantees is unique (a full Matrix ID, not a localpart). Returns the reply
/// to post back, or `None` when `body` isn't a command.
pub fn handle_bridge_command(
    platform: &str,
    body: &str,
    sender: &str,
    connections: &SharedConnections,
) -> Option<String> {
    let body = body.trim();
    if body.eq_ignore_ascii_case("!who") {
        return Some(who_reply(connections));
    }
    if let Some(args) = strip_prefix_ci(body, "!tell ") {
        return Some(tell(platform, args.trim(), sender, connections));
    }
    if body.eq_ignore_ascii_case("!help") {
        return Some(format!(
            "IronMUD {platform} Commands:\n\
            !who - List online players\n\
            !tell <player> <message> - Send a message to an online player\n\
            !help - Show this help"
        ));
    }
    None
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

/// Sanitize external text for display in game: no control characters (so no
/// ANSI/MXP escapes or line breaks), and at most `max` characters.
fn clean(s: &str, max: usize) -> String {
    crate::session::sanitize_player_text(s).chars().take(max).collect()
}

fn who_reply(connections: &SharedConnections) -> String {
    let players = get_online_players(connections);
    if players.is_empty() {
        return "No players currently online.".to_string();
    }
    let names: Vec<String> = {
        let conns = connections.lock();
        players
            .iter()
            .map(|p| {
                let is_afk = conns
                    .values()
                    .any(|s| s.afk && s.character.as_ref().is_some_and(|c| c.name == p.name));
                if is_afk {
                    format!("{} [AFK]", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect()
    };
    format!("Players online ({}): {}", names.len(), names.join(", "))
}

fn tell(platform: &str, args: &str, sender: &str, connections: &SharedConnections) -> String {
    const USAGE: &str = "Usage: !tell <player> <message>";
    let Some((target_name, message)) = args.split_once(' ') else {
        return USAGE.to_string();
    };
    let message = clean(message, MAX_TELL_CHARS);
    if message.trim().is_empty() {
        return USAGE.to_string();
    }
    let sender = clean(sender, MAX_SENDER_CHARS);
    let target_name = clean(target_name, MAX_SENDER_CHARS);

    if !cooldown_ok(&format!("tell\0{platform}\0{sender}"), TELL_COOLDOWN) {
        return "You're sending messages too fast. Wait a moment.".to_string();
    }

    let not_online = format!("Player '{}' is not online.", target_name);
    let Some(conn_id) = find_player_connection_by_name(connections, &target_name) else {
        return not_online;
    };
    // Honor the target's ignore list: `ignore discord` blocks the platform,
    // `ignore discord:<name>` one sender. Like in-game `tell`, an ignoring
    // player looks offline.
    let per_sender = format!("{platform}:{sender}");
    let ignored = {
        let conns = connections.lock();
        conns.get(&conn_id).and_then(|s| s.character.as_ref()).is_some_and(|c| {
            c.ignored
                .iter()
                .any(|i| i.eq_ignore_ascii_case(platform) || i.eq_ignore_ascii_case(&per_sender))
        })
    };
    if ignored {
        return not_online;
    }
    send_client_message(
        connections,
        conn_id.to_string(),
        format!("\n[{platform}] {sender} says: {message}\n"),
    );
    format!("Message sent to {}.", target_name)
}

/// Shared cooldown table: returns false when `key` passed within `gap`,
/// otherwise records now and returns true. Keys are namespaced by caller.
pub fn cooldown_ok(key: &str, gap: Duration) -> bool {
    static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let mut last = LAST.get_or_init(|| Mutex::new(HashMap::new())).lock();
    let now = Instant::now();
    if last.len() > 4096 {
        // Longest gap any caller uses is well under an hour.
        last.retain(|_, t| now.duration_since(*t) < Duration::from_secs(3600));
    }
    if let Some(t) = last.get(key) {
        if now.duration_since(*t) < gap {
            return false;
        }
    }
    last.insert(key.to_string(), now);
    true
}

/// Truncate `s` to at most `max` characters (not bytes).
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn conns_with(name: &str, ignored: &[&str]) -> (SharedConnections, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_in, _rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = crate::PlayerSession::new_for_test(tx, tx_in);
        let mut ch: crate::CharacterData = serde_json::from_value(serde_json::json!({
            "name": name, "password_hash": "", "current_room_id": uuid::Uuid::nil(),
        }))
        .unwrap();
        ch.ignored = ignored.iter().map(|s| s.to_string()).collect();
        session.character = Some(ch);
        connections.lock().insert(uuid::Uuid::new_v4(), session);
        (connections, rx)
    }

    #[test]
    fn tell_strips_escapes_and_line_breaks() {
        let (conns, mut rx) = conns_with("pat", &[]);
        let reply = handle_bridge_command("Discord", "!tell pat hi\x1b[2J\nthere", "evil\x1b[31m", &conns).unwrap();
        assert!(reply.starts_with("Message sent"), "{reply}");
        let got = rx.try_recv().unwrap();
        assert!(!got.contains('\x1b'), "{got:?}");
        assert_eq!(got.trim_matches('\n').matches('\n').count(), 0, "{got:?}");
    }

    #[test]
    fn tell_honors_ignore_and_cooldown() {
        let (conns, mut rx) = conns_with("pat", &["discord"]);
        let reply = handle_bridge_command("Discord", "!tell pat hello", "someone-a", &conns).unwrap();
        assert!(reply.contains("not online"), "{reply}");
        assert!(rx.try_recv().is_err());

        let (conns, mut rx) = conns_with("lee", &["matrix:@troll:example.org"]);
        let reply = handle_bridge_command("Matrix", "!tell lee hi", "@troll:example.org", &conns).unwrap();
        assert!(reply.contains("not online"), "{reply}");
        assert!(rx.try_recv().is_err());

        let (conns, _rx) = conns_with("kim", &[]);
        let first = handle_bridge_command("Matrix", "!TELL kim one", "@spam:example.org", &conns).unwrap();
        assert!(first.starts_with("Message sent"), "{first}");
        let second = handle_bridge_command("Matrix", "!tell kim two", "@spam:example.org", &conns).unwrap();
        assert!(second.contains("too fast"), "{second}");
    }

    #[test]
    fn truncation_is_char_safe() {
        assert_eq!(truncate_chars("héllo", 10), "héllo");
        assert_eq!(truncate_chars("ééééé", 3), "éé…");
    }
}
