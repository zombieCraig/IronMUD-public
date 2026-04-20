// src/script/mail.rs
// Mail system functions for sending and receiving in-game mail

use crate::db::Db;
use crate::{MailMessage, SharedConnections};
use rhai::Engine;
use std::sync::Arc;

/// Register mail-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Configuration Getters ==========

    // get_stamp_price() -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_stamp_price", move || -> i64 {
        cloned_db
            .get_setting_or_default("mail_stamp_price", "10")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<i64>()
            .unwrap_or(10)
            .max(0)
    });

    // get_max_mailbox_size() -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_max_mailbox_size", move || -> i64 {
        cloned_db
            .get_setting_or_default("mail_max_messages", "50")
            .unwrap_or_else(|_| "50".to_string())
            .parse::<i64>()
            .unwrap_or(50)
            .max(1)
    });

    // get_mail_level_requirement() -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_mail_level_requirement", move || -> i64 {
        cloned_db
            .get_setting_or_default("mail_level_requirement", "5")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<i64>()
            .unwrap_or(5)
            .max(0)
    });

    // ========== Mail Query Functions ==========

    // has_mail(recipient) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_mail", move |recipient: String| -> bool {
        cloned_db
            .get_unread_mail_count(&recipient)
            .map(|count| count > 0)
            .unwrap_or(false)
    });

    // get_unread_mail_count(recipient) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_unread_mail_count", move |recipient: String| -> i64 {
        cloned_db.get_unread_mail_count(&recipient).unwrap_or(0)
    });

    // get_mailbox_size(recipient) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_mailbox_size", move |recipient: String| -> i64 {
        cloned_db.get_mailbox_size(&recipient).unwrap_or(0)
    });

    // get_mail_list(recipient) -> Array of mail summaries
    // Returns array of maps with: id, sender, preview, sent_at, read
    let cloned_db = db.clone();
    engine.register_fn("get_mail_list", move |recipient: String| -> rhai::Array {
        match cloned_db.get_mail_for_recipient(&recipient) {
            Ok(messages) => {
                messages
                    .into_iter()
                    .map(|msg| {
                        let mut map = rhai::Map::new();
                        map.insert("id".into(), rhai::Dynamic::from(msg.id.to_string()));
                        map.insert("sender".into(), rhai::Dynamic::from(msg.sender.clone()));
                        // Use first line or first 50 chars as preview
                        let preview = msg
                            .body
                            .lines()
                            .next()
                            .map(|line| {
                                if line.len() > 50 {
                                    format!("{}...", &line[..47])
                                } else {
                                    line.to_string()
                                }
                            })
                            .unwrap_or_else(|| "(empty)".to_string());
                        map.insert("preview".into(), rhai::Dynamic::from(preview));
                        map.insert("sent_at".into(), rhai::Dynamic::from(msg.sent_at));
                        map.insert("read".into(), rhai::Dynamic::from(msg.read));
                        rhai::Dynamic::from(map)
                    })
                    .collect()
            }
            Err(_) => rhai::Array::new(),
        }
    });

    // get_mail_by_index(recipient, index) -> Full message map or ()
    // Index is 1-based for user friendliness
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mail_by_index",
        move |recipient: String, index: i64| -> rhai::Dynamic {
            match cloned_db.get_mail_for_recipient(&recipient) {
                Ok(messages) => {
                    let idx = (index - 1) as usize;
                    if idx < messages.len() {
                        let msg = &messages[idx];
                        let mut map = rhai::Map::new();
                        map.insert("id".into(), rhai::Dynamic::from(msg.id.to_string()));
                        map.insert("sender".into(), rhai::Dynamic::from(msg.sender.clone()));
                        map.insert("body".into(), rhai::Dynamic::from(msg.body.clone()));
                        map.insert("sent_at".into(), rhai::Dynamic::from(msg.sent_at));
                        map.insert("read".into(), rhai::Dynamic::from(msg.read));
                        rhai::Dynamic::from(map)
                    } else {
                        rhai::Dynamic::UNIT
                    }
                }
                Err(_) => rhai::Dynamic::UNIT,
            }
        },
    );

    // ========== Mail Action Functions ==========

    // send_mail(sender, recipient, body) -> String result
    // Returns "ok" on success, or error message on failure
    // Handles auto-delete of oldest read message when mailbox is full
    let cloned_db = db.clone();
    engine.register_fn(
        "send_mail",
        move |sender: String, recipient: String, body: String| -> String {
            // Check recipient exists
            match cloned_db.get_character_data(&recipient) {
                Ok(Some(_)) => {}
                Ok(None) => return "Character not found.".to_string(),
                Err(_) => return "Database error checking recipient.".to_string(),
            }

            // Check mailbox size
            let max_mailbox: i64 = cloned_db
                .get_setting_or_default("mail_max_messages", "50")
                .unwrap_or_else(|_| "50".to_string())
                .parse::<i64>()
                .unwrap_or(50)
                .max(1);
            let mailbox_size = cloned_db.get_mailbox_size(&recipient).unwrap_or(0);
            if mailbox_size >= max_mailbox {
                // Try to delete oldest read message
                match cloned_db.all_mail_unread(&recipient) {
                    Ok(true) => return "Recipient's mailbox is full.".to_string(),
                    Ok(false) => {
                        // Delete oldest read message to make room
                        if cloned_db.delete_oldest_read_mail(&recipient).is_err() {
                            return "Failed to make room in mailbox.".to_string();
                        }
                    }
                    Err(_) => return "Database error checking mailbox.".to_string(),
                }
            }

            // Create and store the message
            let message = MailMessage::new(sender, recipient, body);
            match cloned_db.store_mail(message) {
                Ok(()) => "ok".to_string(),
                Err(_) => "Failed to send mail.".to_string(),
            }
        },
    );

    // mark_mail_read(mail_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("mark_mail_read", move |mail_id: String| -> bool {
        match uuid::Uuid::parse_str(&mail_id) {
            Ok(id) => cloned_db.mark_mail_read(&id).unwrap_or(false),
            Err(_) => false,
        }
    });

    // delete_mail_by_index(recipient, index) -> bool
    // Index is 1-based for user friendliness
    let cloned_db = db.clone();
    engine.register_fn("delete_mail_by_index", move |recipient: String, index: i64| -> bool {
        match cloned_db.get_mail_for_recipient(&recipient) {
            Ok(messages) => {
                let idx = (index - 1) as usize;
                if idx < messages.len() {
                    cloned_db.delete_mail(&messages[idx].id).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    });

    // character_exists(name) -> bool
    let cloned_db = db.clone();
    engine.register_fn("character_exists", move |name: String| -> bool {
        cloned_db
            .get_character_data(&name)
            .map(|opt| opt.is_some())
            .unwrap_or(false)
    });

    // ========== Live Notification Function ==========

    // notify_mail_received(recipient, sender) -> bool
    // Sends immediate notification to online recipient
    let conns = connections.clone();
    engine.register_fn(
        "notify_mail_received",
        move |recipient: String, sender: String| -> bool {
            let conns_guard = conns.lock().unwrap();
            for (_id, session) in conns_guard.iter() {
                if let Some(ref char) = session.character {
                    if char.name.to_lowercase() == recipient.to_lowercase() {
                        let msg = format!("\n[MAIL] You have received a letter from {}.\n", sender);
                        let _ = session.sender.send(msg);
                        return true;
                    }
                }
            }
            false
        },
    );

    // ========== Room Flag Check ==========

    // is_post_office(room_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("is_post_office", move |room_id: String| -> bool {
        match uuid::Uuid::parse_str(&room_id) {
            Ok(uuid) => cloned_db
                .get_room_data(&uuid)
                .map(|opt| opt.map(|room| room.flags.post_office).unwrap_or(false))
                .unwrap_or(false),
            Err(_) => false,
        }
    });
}
