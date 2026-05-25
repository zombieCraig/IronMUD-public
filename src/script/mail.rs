// src/script/mail.rs
// Mail system functions for sending and receiving in-game mail

use crate::db::Db;
use crate::types::ItemLocation;
use crate::{MailMessage, SharedConnections};
use rhai::Engine;
use std::sync::Arc;
use uuid::Uuid;

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
                        map.insert(
                            "attachment_count".into(),
                            rhai::Dynamic::from(msg.attached_items.len() as i64),
                        );
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
                        map.insert(
                            "attachment_count".into(),
                            rhai::Dynamic::from(msg.attached_items.len() as i64),
                        );
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

    // ========== Package / Attachment Functions ==========

    // get_max_attachments() -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_max_attachments", move || -> i64 {
        cloned_db
            .get_setting_or_default("mail_max_attachments", "5")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<i64>()
            .unwrap_or(5)
            .max(0)
    });

    // get_package_postage() -> i64 (per-item surcharge added on top of stamp price)
    let cloned_db = db.clone();
    engine.register_fn("get_package_postage", move || -> i64 {
        cloned_db
            .get_setting_or_default("mail_package_postage", "25")
            .unwrap_or_else(|_| "25".to_string())
            .parse::<i64>()
            .unwrap_or(25)
            .max(0)
    });

    // send_mail_with_attachments(sender, recipient, body, ids) -> String
    // Parks each item via move_item_to_nowhere then stores the mail row.
    // On failure best-effort restores any parked items to the sender's inventory.
    // ids is a rhai::Array of UUID strings.
    let cloned_db = db.clone();
    engine.register_fn(
        "send_mail_with_attachments",
        move |sender: String, recipient: String, body: String, ids: rhai::Array| -> String {
            // Resolve recipient.
            match cloned_db.get_character_data(&recipient) {
                Ok(Some(_)) => {}
                Ok(None) => return "Character not found.".to_string(),
                Err(_) => return "Database error checking recipient.".to_string(),
            }

            // Parse and validate every id up front. Each item must currently
            // be in the sender's inventory — otherwise we'd be teleporting
            // someone else's gear.
            let sender_lower = sender.to_lowercase();
            let mut parsed_ids: Vec<Uuid> = Vec::with_capacity(ids.len());
            for dyn_id in ids.into_iter() {
                let id_str = dyn_id.into_string().unwrap_or_default();
                let uuid = match Uuid::parse_str(&id_str) {
                    Ok(u) => u,
                    Err(_) => return format!("Invalid item id: {}", id_str),
                };
                match cloned_db.get_item_data(&uuid) {
                    Ok(Some(item)) => match &item.location {
                        ItemLocation::Inventory(owner) if *owner == sender_lower => {}
                        _ => return format!("You no longer have {}.", item.short_desc),
                    },
                    Ok(None) => return "One of your items has vanished.".to_string(),
                    Err(_) => return "Database error reading items.".to_string(),
                }
                parsed_ids.push(uuid);
            }

            // Mailbox cap: same logic as send_mail, including auto-evict of
            // oldest read message. delete_mail will recursively delete that
            // evicted message's attached items.
            let max_mailbox: i64 = cloned_db
                .get_setting_or_default("mail_max_messages", "50")
                .unwrap_or_else(|_| "50".to_string())
                .parse::<i64>()
                .unwrap_or(50)
                .max(1);
            let mailbox_size = cloned_db.get_mailbox_size(&recipient).unwrap_or(0);
            if mailbox_size >= max_mailbox {
                match cloned_db.all_mail_unread(&recipient) {
                    Ok(true) => return "Recipient's mailbox is full.".to_string(),
                    Ok(false) => {
                        if cloned_db.delete_oldest_read_mail(&recipient).is_err() {
                            return "Failed to make room in mailbox.".to_string();
                        }
                    }
                    Err(_) => return "Database error checking mailbox.".to_string(),
                }
            }

            // Park each item. If anything fails partway, roll back.
            let mut parked: Vec<Uuid> = Vec::with_capacity(parsed_ids.len());
            for id in &parsed_ids {
                match cloned_db.move_item_to_nowhere(id) {
                    Ok(true) => parked.push(*id),
                    _ => {
                        for back in &parked {
                            let _ = cloned_db.move_item_to_inventory(back, &sender);
                        }
                        return "Failed to prepare package.".to_string();
                    }
                }
            }

            // Store the message.
            let message = MailMessage::with_attachments(sender.clone(), recipient, body, parsed_ids);
            match cloned_db.store_mail(message) {
                Ok(()) => "ok".to_string(),
                Err(_) => {
                    for back in &parked {
                        let _ = cloned_db.move_item_to_inventory(back, &sender);
                    }
                    "Failed to send mail.".to_string()
                }
            }
        },
    );

    // get_mail_attachments_by_index(recipient, index) -> rhai::Array
    // Each element: #{id, name, short_desc}. Items missing from the items
    // tree are skipped (defensive against admin/world-reset deletion).
    let cloned_db = db.clone();
    engine.register_fn(
        "get_mail_attachments_by_index",
        move |recipient: String, index: i64| -> rhai::Array {
            let messages = match cloned_db.get_mail_for_recipient(&recipient) {
                Ok(m) => m,
                Err(_) => return rhai::Array::new(),
            };
            let idx = (index - 1) as usize;
            if idx >= messages.len() {
                return rhai::Array::new();
            }
            let msg = &messages[idx];
            let mut out = rhai::Array::new();
            for item_id in &msg.attached_items {
                if let Ok(Some(item)) = cloned_db.get_item_data(item_id) {
                    let mut m = rhai::Map::new();
                    m.insert("id".into(), rhai::Dynamic::from(item.id.to_string()));
                    m.insert("name".into(), rhai::Dynamic::from(item.name.clone()));
                    m.insert("short_desc".into(), rhai::Dynamic::from(item.short_desc.clone()));
                    out.push(rhai::Dynamic::from(m));
                }
            }
            out
        },
    );

    // claim_mail_attachments_by_index(recipient, index) -> #{ok, claimed, missing}
    // Moves each surviving attachment into recipient's inventory, then
    // clears attached_items on the message (the body stays readable).
    let cloned_db = db.clone();
    engine.register_fn(
        "claim_mail_attachments_by_index",
        move |recipient: String, index: i64| -> rhai::Map {
            let mut result = rhai::Map::new();
            let messages = match cloned_db.get_mail_for_recipient(&recipient) {
                Ok(m) => m,
                Err(_) => {
                    result.insert("ok".into(), rhai::Dynamic::from(false));
                    result.insert("claimed".into(), rhai::Dynamic::from(0i64));
                    result.insert("missing".into(), rhai::Dynamic::from(0i64));
                    return result;
                }
            };
            let idx = (index - 1) as usize;
            if idx >= messages.len() {
                result.insert("ok".into(), rhai::Dynamic::from(false));
                result.insert("claimed".into(), rhai::Dynamic::from(0i64));
                result.insert("missing".into(), rhai::Dynamic::from(0i64));
                return result;
            }
            let msg = &messages[idx];
            let mut claimed = 0i64;
            let mut missing = 0i64;
            for item_id in &msg.attached_items {
                match cloned_db.get_item_data(item_id) {
                    Ok(Some(_)) => match cloned_db.move_item_to_inventory(item_id, &recipient) {
                        Ok(true) => claimed += 1,
                        _ => missing += 1,
                    },
                    _ => missing += 1,
                }
            }

            // Persist message with empty attached_items by re-storing it.
            let mut updated = msg.clone();
            updated.attached_items.clear();
            let _ = cloned_db.store_mail(updated);

            result.insert("ok".into(), rhai::Dynamic::from(true));
            result.insert("claimed".into(), rhai::Dynamic::from(claimed));
            result.insert("missing".into(), rhai::Dynamic::from(missing));
            result
        },
    );
}
