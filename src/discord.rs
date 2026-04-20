//! Discord integration module for IronMUD
//!
//! Provides bidirectional communication with a Discord channel:
//! - Game events (login/logout) are announced to the Discord channel
//! - Discord users can send commands (!who, !tell) to interact with the game

use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;

use crate::chat::ChatMessage;
use crate::{SharedConnections, get_online_players, find_player_connection_by_name, send_client_message};

/// Configuration for Discord bot, loaded from environment variables
#[derive(Clone)]
pub struct DiscordConfig {
    pub token: String,
    pub channel_id: u64,
    pub avatar_path: Option<String>,
}

impl DiscordConfig {
    /// Load configuration from environment variables.
    /// Returns None if any required variable is missing.
    pub fn from_env() -> Option<Self> {
        let token = env::var("DISCORD_TOKEN").ok()?;
        let channel_id_str = env::var("DISCORD_CHANNEL_ID").ok()?;

        let channel_id = match channel_id_str.parse::<u64>() {
            Ok(id) => id,
            Err(e) => {
                error!("Invalid DISCORD_CHANNEL_ID format: {} - {}", channel_id_str, e);
                return None;
            }
        };

        // Avatar path: use env var if set, otherwise check for default file
        let avatar_path = env::var("DISCORD_AVATAR")
            .ok()
            .or_else(|| {
                let default = "assets/discord_avatar.png";
                if std::path::Path::new(default).exists() {
                    Some(default.to_string())
                } else {
                    None
                }
            });

        Some(Self {
            token,
            channel_id,
            avatar_path,
        })
    }
}

/// Event handler for incoming Discord messages
struct Handler {
    connections: SharedConnections,
    target_channel: ChannelId,
    http_sender: tokio::sync::mpsc::UnboundedSender<Arc<serenity::http::Http>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Only handle messages from the configured channel
        if msg.channel_id != self.target_channel {
            return;
        }

        // Don't respond to our own messages
        if msg.author.bot {
            return;
        }

        let body = msg.content.trim();
        let sender = msg.author.name.as_str();

        if let Some(response) = handle_discord_command(body, sender, &self.connections) {
            if let Err(e) = self.target_channel.say(&ctx.http, &response).await {
                error!("Failed to send response to Discord: {}", e);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Discord bot connected as {}", ready.user.name);
        // Send the Http handle to the main loop for sending outgoing messages
        let _ = self.http_sender.send(ctx.http.clone());
    }
}

/// Run the Discord bot. This function runs indefinitely until the server shuts down.
pub async fn run_discord_bot(
    config: DiscordConfig,
    connections: SharedConnections,
    mut rx: mpsc::UnboundedReceiver<ChatMessage>,
) {
    info!("Starting Discord bot");

    let target_channel = ChannelId::new(config.channel_id);

    // Channel to receive the Http handle from the ready event
    let (http_tx, mut http_rx) = tokio::sync::mpsc::unbounded_channel::<Arc<serenity::http::Http>>();

    let handler = Handler {
        connections: connections.clone(),
        target_channel,
        http_sender: http_tx,
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = match Client::builder(&config.token, intents)
        .event_handler(handler)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Discord client: {}", e);
            return;
        }
    };

    // Start the Discord client in a background task
    tokio::spawn(async move {
        if let Err(e) = client.start().await {
            error!("Discord client error: {}", e);
        }
    });

    // Wait for the Http handle from the ready event
    let http = match http_rx.recv().await {
        Some(h) => h,
        None => {
            error!("Failed to receive Discord Http handle - bot may not have connected");
            return;
        }
    };

    // Send startup message
    if let Err(e) = target_channel.say(&http, "IronMUD server is now online.").await {
        warn!("Failed to send Discord startup message: {}", e);
    }

    // Set avatar if configured
    if let Some(ref avatar_path) = config.avatar_path {
        info!("Setting Discord avatar from: {}", avatar_path);
        if let Err(e) = setup_avatar(&http, avatar_path).await {
            warn!("Failed to set Discord avatar: {}", e);
        }
    }

    // Main loop: process outgoing messages from the game
    while let Some(msg) = rx.recv().await {
        let text = match msg {
            ChatMessage::Broadcast(text) => text,
        };

        if let Err(e) = target_channel.say(&http, &text).await {
            error!("Failed to send to Discord channel: {}", e);
        }
    }

    info!("Discord bot shutting down");
}

/// Set the bot's avatar from a local image file
async fn setup_avatar(http: &serenity::http::Http, avatar_path: &str) -> anyhow::Result<()> {
    use serenity::all::CreateAttachment;

    let image_data = std::fs::read(avatar_path)?;

    let filename = std::path::Path::new(avatar_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let attachment = CreateAttachment::bytes(image_data, filename);

    let mut current_user = http.get_current_user().await?;
    current_user.edit(http, serenity::builder::EditProfile::new().avatar(&attachment)).await?;
    info!("Discord avatar uploaded successfully from {}", avatar_path);

    Ok(())
}

/// Handle incoming Discord commands (!who, !tell)
fn handle_discord_command(
    body: &str,
    sender: &str,
    connections: &SharedConnections,
) -> Option<String> {
    if body.eq_ignore_ascii_case("!who") {
        return Some(handle_who_command(connections));
    }

    if body.to_lowercase().starts_with("!tell ") {
        let args = &body[6..].trim();
        return Some(handle_tell_command(args, sender, connections));
    }

    if body.eq_ignore_ascii_case("!help") {
        return Some(
            "IronMUD Discord Commands:\n\
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
        players.iter().map(|p| {
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
        }).collect()
    };
    format!(
        "Players online ({}): {}",
        names.len(),
        names.join(", ")
    )
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
            let formatted_msg = format!(
                "\n[Discord] {} says: {}\n",
                sender, message
            );
            send_client_message(connections, conn_id.to_string(), formatted_msg);
            format!("Message sent to {}.", target_name)
        }
        None => {
            format!("Player '{}' is not online.", target_name)
        }
    }
}
