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
    /// Item instance ids attached to this message. Each lives in the
    /// `items` tree with `location = Nowhere` while in transit and is
    /// returned to circulation on `mail claim` (or destroyed if the
    /// message is deleted/auto-purged).
    #[serde(default)]
    pub attached_items: Vec<Uuid>,
}

impl MailMessage {
    pub fn new(sender: String, recipient: String, body: String) -> Self {
        Self::with_attachments(sender, recipient, body, Vec::new())
    }

    pub fn with_attachments(
        sender: String,
        recipient: String,
        body: String,
        attached_items: Vec<Uuid>,
    ) -> Self {
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
            attached_items,
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
