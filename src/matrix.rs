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
        events::room::message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
    },
};

use crate::chat::ChatMessage;
use crate::{SharedConnections, find_player_connection_by_name, get_online_players, send_client_message};

/// Configuration for Matrix bot, loaded from environment variables
#[derive(Clone)]
pub struct MatrixConfig {
    pub homeserver: String,
    pub user_id: String,
    pub password: String,
    pub room_id: OwnedRoomId,
    pub avatar_path: Option<String>,
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

    // Create the Matrix client
    let client = match create_client(&config).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Matrix client: {}", e);
            return;
        }
    };

    // Log in
    if let Err(e) = login(&client, &config).await {
        error!("Failed to log in to Matrix: {}", e);
        return;
    }

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
            if ev.sender.localpart()
                == target_room
                    .client()
                    .user_id()
                    .map(|u| u.localpart())
                    .unwrap_or_default()
            {
                return;
            }

            // Skip messages sent before we started (avoids replaying old commands on reconnect)
            let msg_time_ms: u64 = ev.origin_server_ts.0.into();
            if msg_time_ms < startup_time_ms {
                debug!(
                    "Ignoring old message from {} (sent before bot startup)",
                    ev.sender.localpart()
                );
                return;
            }

            if let MessageType::Text(text) = ev.content.msgtype {
                let body = text.body.trim();
                let sender = ev.sender.localpart().to_string();

                if let Some(response) = handle_matrix_command(body, &sender, &conns).await {
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

/// Log in to Matrix
async fn login(client: &Client, config: &MatrixConfig) -> anyhow::Result<()> {
    client
        .matrix_auth()
        .login_username(&config.user_id, &config.password)
        .initial_device_display_name("IronMUD Bot")
        .await?;
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
    let content = RoomMessageEventContent::text_plain(message);
    room.send(content).await?;
    Ok(())
}

/// Handle incoming Matrix commands (!who, !tell)
async fn handle_matrix_command(body: &str, sender: &str, connections: &SharedConnections) -> Option<String> {
    if body.eq_ignore_ascii_case("!who") {
        return Some(handle_who_command(connections));
    }

    if body.to_lowercase().starts_with("!tell ") {
        let args = &body[6..].trim();
        return Some(handle_tell_command(args, sender, connections));
    }

    if body.eq_ignore_ascii_case("!help") {
        return Some(
            "IronMUD Matrix Commands:\n\
            !who - List online players\n\
            !tell <player> <message> - Send a message to an online player\n\
            !help - Show this help"
                .to_string(),
        );
    }

    None
}

/// Handle the !who command - list online players
fn handle_who_command(connections: &SharedConnections) -> String {
    let players = get_online_players(connections);

    if players.is_empty() {
        return "No players currently online.".to_string();
    }

    // Build player names with AFK status
    let names: Vec<String> = {
        let conns = connections.lock().unwrap();
        players
            .iter()
            .map(|p| {
                let is_afk = conns.values().any(|session| {
                    if let Some(ref char) = session.character {
                        char.name == p.name && session.afk
                    } else {
                        false
                    }
                });
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

/// Handle the !tell command - send a message to an online player
fn handle_tell_command(args: &str, sender: &str, connections: &SharedConnections) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();

    if parts.len() < 2 {
        return "Usage: !tell <player> <message>".to_string();
    }

    let target_name = parts[0];
    let message = parts[1];

    if message.trim().is_empty() {
        return "Usage: !tell <player> <message>".to_string();
    }

    // Find the player
    match find_player_connection_by_name(connections, target_name) {
        Some(conn_id) => {
            let formatted_msg = format!("\n[Matrix] {} says: {}\n", sender, message);
            send_client_message(connections, conn_id.to_string(), formatted_msg);
            format!("Message sent to {}.", target_name)
        }
        None => {
            format!("Player '{}' is not online.", target_name)
        }
    }
}
