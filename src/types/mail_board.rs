//! Persistent message types: player mail and bulletin-board posts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A mail message between players
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: Uuid,
    pub sender: String,
    pub recipient: String, // lowercase for lookup
    pub body: String,
    pub sent_at: i64, // Unix timestamp
    pub read: bool,
}

impl MailMessage {
    pub fn new(sender: String, recipient: String, body: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        MailMessage {
            id: Uuid::new_v4(),
            sender,
            recipient: recipient.to_lowercase(),
            body,
            sent_at: now,
            read: false,
        }
    }
}

/// A single bulletin board post. Posts live in the `boards` sled tree
/// keyed by `id`; `board_vnum` identifies which board prototype owns them
/// (matches `ItemData.vnum: Option<String>` shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardPost {
    pub id: Uuid,
    pub board_vnum: String,
    pub author: String,
    pub subject: String,
    pub body: String,
    pub posted_at: i64,
}

impl BoardPost {
    pub fn new(board_vnum: String, author: String, subject: String, body: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        BoardPost {
            id: Uuid::new_v4(),
            board_vnum,
            author,
            subject,
            body,
            posted_at: now,
        }
    }
}
