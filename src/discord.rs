//! Discord integration module for IronMUD
//!
//! Provides bidirectional communication with a Discord channel:
//! - Game events (login/logout) are announced to the Discord channel
//! - Discord users can send commands (!who, !tell) to interact with the game

use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;

use serenity::builder::{CreateAllowedMentions, CreateMessage};

use crate::SharedConnections;
use crate::chat::{ChatMessage, handle_bridge_command, truncate_chars};

/// Discord's per-message character limit.
const DISCORD_MAX_CHARS: usize = 2000;

/// Post `text` to `channel` with every mention disabled (no @everyone,
/// role or user pings from player-controlled text) and truncated to fit.
async fn post(http: &serenity::http::Http, channel: ChannelId, text: &str) -> serenity::Result<Message> {
    let msg = CreateMessage::new()
        .content(truncate_chars(text, DISCORD_MAX_CHARS))
        .allowed_mentions(CreateAllowedMentions::new());
    channel.send_message(http, msg).await
}

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
        let avatar_path = env::var("DISCORD_AVATAR").ok().or_else(|| {
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

        if let Some(response) = handle_bridge_command("Discord", body, sender, &self.connections) {
            if let Err(e) = post(&ctx.http, self.target_channel, &response).await {
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

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::DIRECT_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = match Client::builder(&config.token, intents).event_handler(handler).await {
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
    if let Err(e) = post(&http, target_channel, "IronMUD server is now online.").await {
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

        if let Err(e) = post(&http, target_channel, &text).await {
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
    current_user
        .edit(http, serenity::builder::EditProfile::new().avatar(&attachment))
        .await?;
    info!("Discord avatar uploaded successfully from {}", avatar_path);

    Ok(())
}
