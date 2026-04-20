// src/script/bugs.rs
// Bug reporting system functions for Rhai scripting

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::{AdminNote, BugContext, BugPriority, BugReport, BugStatus, SharedConnections};

/// Register bug-reporting related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== Bug Submission ==========

    // submit_bug_report(reporter, description, context_map, is_admin) -> String
    // Returns ticket number as string on success, "ERROR:..." on failure
    let cloned_db = db.clone();
    engine.register_fn("submit_bug_report", move |reporter: String, description: String, context: rhai::Map, is_admin: bool| -> String {
        // Generate ticket number
        let ticket_number = match cloned_db.next_bug_ticket_number() {
            Ok(n) => n,
            Err(e) => return format!("ERROR: Failed to generate ticket number: {}", e),
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Build BugContext from the Rhai map
        let bug_context = BugContext {
            room_id: context.get("room_id").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            room_vnum: context.get("room_vnum").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            room_title: context.get("room_title").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            character_level: context.get("character_level").and_then(|d| d.clone().as_int().ok()).unwrap_or(0) as i32,
            character_class: context.get("character_class").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            character_race: context.get("character_race").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            character_position: context.get("character_position").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            hp: context.get("hp").and_then(|d| d.clone().as_int().ok()).unwrap_or(0) as i32,
            max_hp: context.get("max_hp").and_then(|d| d.clone().as_int().ok()).unwrap_or(0) as i32,
            mana: context.get("mana").and_then(|d| d.clone().as_int().ok()).unwrap_or(0) as i32,
            max_mana: context.get("max_mana").and_then(|d| d.clone().as_int().ok()).unwrap_or(0) as i32,
            in_combat: context.get("in_combat").and_then(|d| d.clone().try_cast::<bool>()).unwrap_or(false),
            game_time: context.get("game_time").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            season: context.get("season").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            weather: context.get("weather").and_then(|d| d.clone().try_cast::<String>()).unwrap_or_default(),
            players_in_room: context.get("players_in_room")
                .and_then(|d| d.clone().try_cast::<rhai::Array>())
                .map(|arr| arr.into_iter().filter_map(|d| d.try_cast::<String>()).collect())
                .unwrap_or_default(),
            mobiles_in_room: context.get("mobiles_in_room")
                .and_then(|d| d.clone().try_cast::<rhai::Array>())
                .map(|arr| arr.into_iter().filter_map(|d| d.try_cast::<String>()).collect())
                .unwrap_or_default(),
        };

        let report = BugReport {
            id: uuid::Uuid::new_v4(),
            ticket_number,
            reporter,
            description,
            status: BugStatus::Open,
            priority: BugPriority::Normal,
            approved: is_admin, // Auto-approve if admin
            created_at: now,
            updated_at: now,
            resolved_at: None,
            resolved_by: None,
            admin_notes: Vec::new(),
            context: bug_context,
        };

        match cloned_db.store_bug_report(report) {
            Ok(()) => ticket_number.to_string(),
            Err(e) => format!("ERROR: Failed to store bug report: {}", e),
        }
    });

    // ========== Bug Query Functions ==========

    // get_bug_by_ticket(ticket) -> Dynamic (map with all fields, or () if not found)
    let cloned_db = db.clone();
    engine.register_fn("get_bug_by_ticket", move |ticket: i64| -> rhai::Dynamic {
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(report)) => bug_report_to_dynamic(&report),
            _ => rhai::Dynamic::UNIT,
        }
    });

    // list_bugs(status_filter) -> Array of summary maps
    // status_filter: "open", "closed", "inprogress", "resolved", "all", "unapproved"
    let cloned_db = db.clone();
    engine.register_fn("list_bugs", move |status_filter: String| -> rhai::Array {
        let filter_lower = status_filter.to_lowercase();

        let (status, approved_only) = if filter_lower == "unapproved" {
            (None, false) // We'll filter unapproved manually
        } else if filter_lower == "all" {
            (None, false)
        } else {
            (BugStatus::from_str(&filter_lower), false)
        };

        match cloned_db.list_bug_reports(status.as_ref(), approved_only) {
            Ok(reports) => {
                let filtered: Vec<&BugReport> = if filter_lower == "unapproved" {
                    reports.iter().filter(|r| !r.approved).collect()
                } else {
                    reports.iter().collect()
                };

                filtered.iter().map(|report| {
                    let mut map = rhai::Map::new();
                    map.insert("ticket_number".into(), rhai::Dynamic::from(report.ticket_number));
                    map.insert("reporter".into(), rhai::Dynamic::from(report.reporter.clone()));
                    let preview = if report.description.len() > 50 {
                        format!("{}...", &report.description[..47])
                    } else {
                        report.description.clone()
                    };
                    map.insert("preview".into(), rhai::Dynamic::from(preview));
                    map.insert("status".into(), rhai::Dynamic::from(report.status.to_display_string().to_string()));
                    map.insert("priority".into(), rhai::Dynamic::from(report.priority.to_display_string().to_string()));
                    map.insert("approved".into(), rhai::Dynamic::from(report.approved));
                    map.insert("created_at".into(), rhai::Dynamic::from(report.created_at));
                    rhai::Dynamic::from(map)
                }).collect()
            }
            Err(_) => rhai::Array::new(),
        }
    });

    // count_open_bugs() -> i64
    let cloned_db = db.clone();
    engine.register_fn("count_open_bugs", move || -> i64 {
        cloned_db.count_open_bug_reports().unwrap_or(0)
    });

    // ========== Bug Admin Functions ==========

    // add_bug_note(ticket, author, message) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_bug_note", move |ticket: i64, author: String, message: String| -> bool {
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(mut report)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                report.admin_notes.push(AdminNote {
                    author,
                    message,
                    created_at: now,
                });
                report.updated_at = now;
                cloned_db.save_bug_report(report).is_ok()
            }
            _ => false,
        }
    });

    // approve_bug(ticket) -> bool
    let cloned_db = db.clone();
    engine.register_fn("approve_bug", move |ticket: i64| -> bool {
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(mut report)) => {
                report.approved = true;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                report.updated_at = now;
                cloned_db.save_bug_report(report).is_ok()
            }
            _ => false,
        }
    });

    // set_bug_status(ticket, status) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_bug_status", move |ticket: i64, status: String| -> bool {
        let new_status = match BugStatus::from_str(&status) {
            Some(s) => s,
            None => return false,
        };
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(mut report)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                report.status = new_status;
                report.updated_at = now;
                cloned_db.save_bug_report(report).is_ok()
            }
            _ => false,
        }
    });

    // set_bug_priority(ticket, priority) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_bug_priority", move |ticket: i64, priority: String| -> bool {
        let new_priority = match BugPriority::from_str(&priority) {
            Some(p) => p,
            None => return false,
        };
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(mut report)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                report.priority = new_priority;
                report.updated_at = now;
                cloned_db.save_bug_report(report).is_ok()
            }
            _ => false,
        }
    });

    // close_bug(ticket, admin, note) -> bool
    let cloned_db = db.clone();
    engine.register_fn("close_bug", move |ticket: i64, admin: String, note: String| -> bool {
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(mut report)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                report.status = BugStatus::Closed;
                report.resolved_at = Some(now);
                report.resolved_by = Some(admin.clone());
                report.updated_at = now;
                if !note.is_empty() {
                    report.admin_notes.push(AdminNote {
                        author: admin,
                        message: note,
                        created_at: now,
                    });
                }
                cloned_db.save_bug_report(report).is_ok()
            }
            _ => false,
        }
    });

    // delete_bug(ticket) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_bug", move |ticket: i64| -> bool {
        match cloned_db.get_bug_report_by_ticket(ticket) {
            Ok(Some(report)) => cloned_db.delete_bug_report(&report.id).unwrap_or(false),
            _ => false,
        }
    });

    // ========== Notification Functions ==========

    // notify_admins_bug(reporter, ticket, preview) - Notifies online admins
    let conns = connections.clone();
    engine.register_fn("notify_admins_bug", move |reporter: String, ticket: i64, preview: String| {
        let conns_guard = conns.lock().unwrap();
        for (_id, session) in conns_guard.iter() {
            if let Some(ref char) = session.character {
                if char.is_admin {
                    let msg = format!(
                        "\n\x1b[1;33m[BUG #{}]\x1b[0m New bug report from {}: {}\n",
                        ticket, reporter, preview
                    );
                    let _ = session.sender.send(msg);
                }
            }
        }
    });

    // notify_player_bug_update(reporter, ticket, status) - Notifies reporter if online
    let conns = connections.clone();
    engine.register_fn("notify_player_bug_update", move |reporter: String, ticket: i64, status: String| {
        let conns_guard = conns.lock().unwrap();
        for (_id, session) in conns_guard.iter() {
            if let Some(ref char) = session.character {
                if char.name.to_lowercase() == reporter.to_lowercase() {
                    let msg = format!(
                        "\n\x1b[1;36m[BUG #{}]\x1b[0m Your bug report status has been updated to: {}\n",
                        ticket, status
                    );
                    let _ = session.sender.send(msg);
                    return;
                }
            }
        }
    });
}

/// Convert a BugReport to a Rhai Dynamic map (used by get_bug_by_ticket)
fn bug_report_to_dynamic(report: &BugReport) -> rhai::Dynamic {
    let mut map = rhai::Map::new();
    map.insert("id".into(), rhai::Dynamic::from(report.id.to_string()));
    map.insert("ticket_number".into(), rhai::Dynamic::from(report.ticket_number));
    map.insert("reporter".into(), rhai::Dynamic::from(report.reporter.clone()));
    map.insert("description".into(), rhai::Dynamic::from(report.description.clone()));
    map.insert("status".into(), rhai::Dynamic::from(report.status.to_display_string().to_string()));
    map.insert("priority".into(), rhai::Dynamic::from(report.priority.to_display_string().to_string()));
    map.insert("approved".into(), rhai::Dynamic::from(report.approved));
    map.insert("created_at".into(), rhai::Dynamic::from(report.created_at));
    map.insert("updated_at".into(), rhai::Dynamic::from(report.updated_at));
    map.insert("resolved_at".into(), report.resolved_at.map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT));
    map.insert("resolved_by".into(), report.resolved_by.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT));

    // Admin notes as array of maps
    let notes: rhai::Array = report.admin_notes.iter().map(|note| {
        let mut note_map = rhai::Map::new();
        note_map.insert("author".into(), rhai::Dynamic::from(note.author.clone()));
        note_map.insert("message".into(), rhai::Dynamic::from(note.message.clone()));
        note_map.insert("created_at".into(), rhai::Dynamic::from(note.created_at));
        rhai::Dynamic::from(note_map)
    }).collect();
    map.insert("admin_notes".into(), rhai::Dynamic::from(notes));

    // Context as a nested map
    let mut ctx = rhai::Map::new();
    ctx.insert("room_id".into(), rhai::Dynamic::from(report.context.room_id.clone()));
    ctx.insert("room_vnum".into(), rhai::Dynamic::from(report.context.room_vnum.clone()));
    ctx.insert("room_title".into(), rhai::Dynamic::from(report.context.room_title.clone()));
    ctx.insert("character_level".into(), rhai::Dynamic::from(report.context.character_level as i64));
    ctx.insert("character_class".into(), rhai::Dynamic::from(report.context.character_class.clone()));
    ctx.insert("character_race".into(), rhai::Dynamic::from(report.context.character_race.clone()));
    ctx.insert("character_position".into(), rhai::Dynamic::from(report.context.character_position.clone()));
    ctx.insert("hp".into(), rhai::Dynamic::from(report.context.hp as i64));
    ctx.insert("max_hp".into(), rhai::Dynamic::from(report.context.max_hp as i64));
    ctx.insert("mana".into(), rhai::Dynamic::from(report.context.mana as i64));
    ctx.insert("max_mana".into(), rhai::Dynamic::from(report.context.max_mana as i64));
    ctx.insert("in_combat".into(), rhai::Dynamic::from(report.context.in_combat));
    ctx.insert("game_time".into(), rhai::Dynamic::from(report.context.game_time.clone()));
    ctx.insert("season".into(), rhai::Dynamic::from(report.context.season.clone()));
    ctx.insert("weather".into(), rhai::Dynamic::from(report.context.weather.clone()));
    let players: rhai::Array = report.context.players_in_room.iter()
        .map(|s| rhai::Dynamic::from(s.clone())).collect();
    ctx.insert("players_in_room".into(), rhai::Dynamic::from(players));
    let mobiles: rhai::Array = report.context.mobiles_in_room.iter()
        .map(|s| rhai::Dynamic::from(s.clone())).collect();
    ctx.insert("mobiles_in_room".into(), rhai::Dynamic::from(mobiles));
    map.insert("context".into(), rhai::Dynamic::from(ctx));

    rhai::Dynamic::from(map)
}
