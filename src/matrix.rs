//! Matrix integration module for IronMUD
//!
//! Provides bidirectional communication with a Matrix room:
//! - Game events (login/logout) are announced to the Matrix room
//! - Matrix users can send commands (!who, !tell) to interact with the game

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use matrix_sdk::{
    Client,
    config::SyncSettings,
    room::Room,
    ruma::{
        OwnedRoomId,
        events::Mentions,
        events::room::message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
    },
};

use crate::SharedConnections;
use crate::chat::{ChatMessage, handle_bridge_command, truncate_chars};

/// Cap on outbound message length; Matrix events max out at 64 KiB.
const MATRIX_MAX_CHARS: usize = 4000;

/// Configuration for Matrix bot, loaded from environment variables
#[derive(Clone)]
pub struct MatrixConfig {
    pub homeserver: String,
    pub user_id: String,
    pub password: String,
    pub room_id: OwnedRoomId,
    pub avatar_path: Option<String>,
    /// Where the login session (device + access token) is kept between
    /// restarts. `MATRIX_SESSION_FILE`, else set by the caller.
    pub session_path: Option<std::path::PathBuf>,
}

impl MatrixConfig {
    /// Load configuration from environment variables.
    /// Returns None if any required variable is missing.
    pub fn from_env() -> Option<Self> {
        let homeserver = env::var("MATRIX_HOMESERVER").ok()?;
        let user_id = env::var("MATRIX_USER").ok()?;
        let password = env::var("MATRIX_PASSWORD").ok()?;
        let room_id_str = env::var("MATRIX_ROOM").ok()?;

        let room_id = match room_id_str.parse::<OwnedRoomId>() {
            Ok(id) => id,
            Err(e) => {
                error!("Invalid MATRIX_ROOM format: {} - {}", room_id_str, e);
                return None;
            }
        };

        // Avatar path: use env var if set, otherwise check for default file
        let avatar_path = env::var("MATRIX_AVATAR").ok().or_else(|| {
            let default = "assets/matrix_avatar.png";
            if std::path::Path::new(default).exists() {
                Some(default.to_string())
            } else {
                None
            }
        });

        Some(Self {
            homeserver,
            user_id,
            password,
            room_id,
            avatar_path,
            session_path: env::var("MATRIX_SESSION_FILE").ok().map(Into::into),
        })
    }
}

/// Run the Matrix bot. This function runs indefinitely until the server shuts down.
///
/// # Arguments
/// * `config` - Matrix configuration
/// * `connections` - Shared player connections for handling !tell and !who
/// * `rx` - Receiver for messages from the game
pub async fn run_matrix_bot(
    config: MatrixConfig,
    connections: SharedConnections,
    mut rx: mpsc::UnboundedReceiver<ChatMessage>,
) {
    info!("Starting Matrix bot, connecting to {}", config.homeserver);

    // Restore the saved session, or log in (which creates a new device)
    let client = match connect(&config).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to log in to Matrix: {}", e);
            return;
        }
    };

    info!("Matrix bot logged in as {}", config.user_id);

    // Set avatar if configured
    if let Some(ref avatar_path) = config.avatar_path {
        info!("Setting Matrix avatar from: {}", avatar_path);
        if let Err(e) = setup_avatar(&client, avatar_path).await {
            warn!("Failed to set Matrix avatar: {}", e);
            // Continue anyway - avatar is optional
        }
    } else {
        info!("No Matrix avatar configured (set MATRIX_AVATAR env var or place file at assets/matrix_avatar.png)");
    }

    // Get the room
    let room = match client.get_room(&config.room_id) {
        Some(r) => r,
        None => {
            // Try to join the room
            match client.join_room_by_id(&config.room_id).await {
                Ok(r) => Room::from(r),
                Err(e) => {
                    error!("Failed to join Matrix room {}: {}", config.room_id, e);
                    return;
                }
            }
        }
    };

    info!("Matrix bot connected to room {}", config.room_id);

    // Send startup message
    if let Err(e) = send_to_room(&room, "IronMUD server is now online.").await {
        warn!("Failed to send startup message: {}", e);
    }

    // Record startup time to filter out old messages on reconnect
    let startup_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    // Clone for the event handler
    let connections_clone = connections.clone();
    let room_clone = room.clone();

    // Set up message handler for incoming Matrix messages
    client.add_event_handler(move |ev: OriginalSyncRoomMessageEvent, room: Room| {
        let conns = connections_clone.clone();
        let target_room = room_clone.clone();
        async move {
            // Only handle messages from the configured room
            if room.room_id() != target_room.room_id() {
                return;
            }

            // Don't respond to our own messages
            // Compare full Matrix IDs: a same-named user on another
            // homeserver shares our localpart but is not us.
            if target_room.client().user_id() == Some(ev.sender.as_ref()) {
                return;
            }

            // Skip messages sent before we started (avoids replaying old commands on reconnect)
            let msg_time_ms: u64 = ev.origin_server_ts.0.into();
            if msg_time_ms < startup_time_ms {
                debug!("Ignoring old message from {} (sent before bot startup)", ev.sender);
                return;
            }

            if let MessageType::Text(text) = ev.content.msgtype {
                let body = text.body.trim();
                // The full MXID (@user:server), never the bare localpart:
                // anyone can register `@alice` on their own homeserver.
                let sender = ev.sender.to_string();

                if let Some(response) = handle_bridge_command("Matrix", body, &sender, &conns) {
                    if let Err(e) = send_to_room(&target_room, &response).await {
                        error!("Failed to send response to Matrix: {}", e);
                    }
                }
            }
        }
    });

    // Start syncing in background
    let sync_client = client.clone();
    tokio::spawn(async move {
        if let Err(e) = sync_client.sync(SyncSettings::default()).await {
            error!("Matrix sync error: {}", e);
        }
    });

    // Main loop: process outgoing messages from the game
    while let Some(msg) = rx.recv().await {
        let text = match msg {
            ChatMessage::Broadcast(text) => text,
        };

        if let Err(e) = send_to_room(&room, &text).await {
            error!("Failed to send to Matrix room: {}", e);
        }
    }

    info!("Matrix bot shutting down");
}

/// Create the Matrix client
async fn create_client(config: &MatrixConfig) -> anyhow::Result<Client> {
    let client = Client::builder().homeserver_url(&config.homeserver).build().await?;
    Ok(client)
}

/// A logged-in client. Reuses the saved session when it's still valid;
/// otherwise logs in with the password and saves the new session. Logging
/// in on every restart registered a new device each time, which piles up
/// on the bot's account.
async fn connect(config: &MatrixConfig) -> anyhow::Result<Client> {
    if let Some(session) = config.session_path.as_deref().and_then(load_session) {
        if session_matches_user(&session, &config.user_id) {
            let client = create_client(config).await?;
            let restored = client
                .matrix_auth()
                .restore_session(session, matrix_sdk::store::RoomLoadSettings::default())
                .await;
            match restored {
                Ok(()) => match client.whoami().await {
                    Ok(_) => {
                        info!("Matrix session restored");
                        return Ok(client);
                    }
                    Err(e) => warn!("Saved Matrix session no longer valid ({}); logging in again", e),
                },
                Err(e) => warn!("Could not restore Matrix session ({}); logging in again", e),
            }
        }
    }

    let client = create_client(config).await?;
    client
        .matrix_auth()
        .login_username(&config.user_id, &config.password)
        .initial_device_display_name("IronMUD Bot")
        .await?;
    if let (Some(path), Some(session)) = (&config.session_path, client.matrix_auth().session()) {
        if let Err(e) = save_session(path, &session) {
            warn!("Could not save Matrix session to {}: {}", path.display(), e);
        }
    }
    Ok(client)
}

/// Whether a saved session belongs to the configured `MATRIX_USER`, which
/// may be a full MXID (`@bot:server`) or just the localpart (`bot`).
fn session_matches_user(session: &matrix_sdk::authentication::matrix::MatrixSession, configured: &str) -> bool {
    let saved = &session.meta.user_id;
    if configured.starts_with('@') {
        saved.as_str() == configured
    } else {
        saved.localpart() == configured.split(':').next().unwrap_or(configured)
    }
}

fn load_session(path: &std::path::Path) -> Option<matrix_sdk::authentication::matrix::MatrixSession> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("Ignoring unreadable Matrix session file {}: {}", path.display(), e);
            None
        }
    }
}

/// Write the session (it holds an access token) readable by owner only.
fn save_session(
    path: &std::path::Path,
    session: &matrix_sdk::authentication::matrix::MatrixSession,
) -> anyhow::Result<()> {
    use std::io::Write;
    let json = serde_json::to_vec(session)?;
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(&json)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Set the bot's avatar if configured and not already set
async fn setup_avatar(client: &Client, avatar_path: &str) -> anyhow::Result<()> {
    use std::fs;

    // Check if avatar already set
    if client.account().get_avatar_url().await?.is_some() {
        info!("Matrix avatar already set, skipping upload");
        return Ok(());
    }

    // Read image file
    let image_data = fs::read(avatar_path)?;

    // Determine mime type from extension
    let mime_type = if avatar_path.ends_with(".png") {
        mime::IMAGE_PNG
    } else if avatar_path.ends_with(".jpg") || avatar_path.ends_with(".jpeg") {
        mime::IMAGE_JPEG
    } else {
        anyhow::bail!("Unsupported image format for avatar. Use .png or .jpg");
    };

    // Upload and set avatar
    client.account().upload_avatar(&mime_type, image_data).await?;
    info!("Matrix avatar uploaded successfully from {}", avatar_path);

    Ok(())
}

/// Send a text message to a Matrix room
async fn send_to_room(room: &Room, message: &str) -> anyhow::Result<()> {
    // An explicit empty `m.mentions` tells clients this message mentions no
    // one, which turns off the legacy body-matching push rules: player text
    // containing "@room" or a user's name can't ping the room.
    let content =
        RoomMessageEventContent::text_plain(truncate_chars(message, MATRIX_MAX_CHARS)).add_mentions(Mentions::new());
    room.send(content).await?;
    Ok(())
}

#[cfg(test)]
mod session_file_tests {
    use super::*;

    #[test]
    fn session_round_trips_through_an_owner_only_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matrix_session.json");
        let session: matrix_sdk::authentication::matrix::MatrixSession = serde_json::from_value(serde_json::json!({
            "user_id": "@bot:example.org",
            "device_id": "DEVICEID",
            "access_token": "token",
        }))
        .unwrap();
        assert!(session_matches_user(&session, "bot"));
        assert!(session_matches_user(&session, "@bot:example.org"));
        assert!(!session_matches_user(&session, "other"));
        assert!(!session_matches_user(&session, "@bot:elsewhere.org"));
        save_session(&path, &session).unwrap();
        assert_eq!(load_session(&path).unwrap(), session);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        std::fs::write(&path, "not json").unwrap();
        assert!(load_session(&path).is_none());
    }
}
