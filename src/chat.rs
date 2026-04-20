//! Chat bridge module for IronMUD
//!
//! Provides a unified message type and fan-out bridge for chat integrations
//! (Matrix, Discord, etc.). Game code sends messages to a single ChatSender,
//! and the bridge distributes them to all configured backends.

use tokio::sync::mpsc;
use tracing::warn;

/// Message type for chat integrations. All messages are broadcasts.
#[derive(Clone, Debug)]
pub enum ChatMessage {
    Broadcast(String),
}

/// Sender handle for game code to send messages to chat backends
pub type ChatSender = mpsc::UnboundedSender<ChatMessage>;

/// Run the chat bridge, fanning out messages to all registered backends.
/// Each backend has its own unbounded channel receiver.
pub async fn run_chat_bridge(
    mut rx: mpsc::UnboundedReceiver<ChatMessage>,
    backends: Vec<mpsc::UnboundedSender<ChatMessage>>,
) {
    while let Some(msg) = rx.recv().await {
        for backend in &backends {
            if let Err(e) = backend.send(msg.clone()) {
                warn!("Chat backend channel closed: {}", e);
            }
        }
    }
}
