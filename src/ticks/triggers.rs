//! Periodic trigger tick system for IronMUD
//!
//! Handles periodic room triggers, mobile idle triggers, and fishing bite notifications.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    CharacterPosition, DoorState, MobileTrigger, MobileTriggerType, SharedConnections, TriggerType, db,
    script::execute_room_template, session::broadcast_to_builders,
};

/// Periodic trigger tick interval in seconds (more frequent than spawn tick)
pub const PERIODIC_TRIGGER_INTERVAL_SECS: u64 = 10;

/// Background task that processes periodic room triggers
pub async fn run_periodic_trigger_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(PERIODIC_TRIGGER_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_periodic_triggers(&db, &connections) {
            error!("Periodic trigger tick error: {}", e);
        }
    }
}

/// Process all periodic room triggers
fn process_periodic_triggers(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use rand::Rng;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let rooms = db.list_all_rooms()?;

    for mut room in rooms {
        let mut fired_triggers: Vec<(usize, i64)> = Vec::new();
        let room_id = room.id;

        for (idx, trigger) in room.triggers.iter_mut().enumerate() {
            // Only process periodic triggers
            if trigger.trigger_type != TriggerType::Periodic {
                continue;
            }
            if !trigger.enabled {
                continue;
            }

            // Check if enough time has passed
            if now < trigger.last_fired + trigger.interval_secs {
                continue;
            }

            // Check chance
            if trigger.chance < 100 {
                let roll: i32 = rand::thread_rng().gen_range(1..=100);
                if roll > trigger.chance {
                    // Update timestamp even on failed roll to avoid rapid re-rolls
                    trigger.last_fired = now;
                    fired_triggers.push((idx, now));
                    continue;
                }
            }

            // Find all awake players in this room (sleeping players don't see periodic triggers)
            let players_in_room: Vec<_> = {
                let conns = connections.lock().unwrap();
                conns
                    .iter()
                    .filter_map(|(conn_id, session)| {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room.id && char.position != CharacterPosition::Sleeping {
                                return Some((*conn_id, session.sender.clone()));
                            }
                        }
                        None
                    })
                    .collect()
            };

            if players_in_room.is_empty() {
                // No awake players in room, skip trigger execution but don't update timestamp
                continue;
            }

            // Handle built-in templates (script_name starts with @)
            if trigger.script_name.starts_with('@') {
                let template_name = &trigger.script_name[1..];
                let ctx_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                execute_room_template(template_name, &trigger.args, &room.id, connections, &ctx_map);
                trigger.last_fired = now;
                fired_triggers.push((idx, now));
                continue;
            }

            // Execute trigger for each player in the room
            let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
            let script_content = match std::fs::read_to_string(&script_path) {
                Ok(content) => content,
                Err(e) => {
                    debug!("Periodic trigger script not found: {} - {}", script_path, e);
                    continue;
                }
            };

            // Create a minimal Rhai engine for trigger execution
            let mut trigger_engine = rhai::Engine::new();

            // Register basic send function for periodic triggers
            for (conn_id, sender) in &players_in_room {
                let conn_id_str = conn_id.to_string();
                let sender_clone = sender.clone();

                // Register send_client_message for this player
                trigger_engine.register_fn("send_client_message", move |cid: String, message: String| {
                    if cid == conn_id_str {
                        let _ = sender_clone.send(message);
                    }
                });
            }

            // Register broadcast_to_room
            let conns_clone = connections.clone();
            let room_id = room.id;
            trigger_engine.register_fn(
                "broadcast_to_room",
                move |_rid: String, message: String, _exclude: String| {
                    // Broadcast to all in room
                    let conns = conns_clone.lock().unwrap();
                    for (_, session) in conns.iter() {
                        if let Some(ref char) = session.character {
                            if char.current_room_id == room_id {
                                let _ = session.sender.send(message.clone());
                            }
                        }
                    }
                },
            );

            // Register random_int
            trigger_engine.register_fn("random_int", |min: i64, max: i64| {
                if min >= max {
                    return min;
                }
                rand::thread_rng().gen_range(min..=max)
            });

            // Compile and run trigger
            match trigger_engine.compile(&script_content) {
                Ok(ast) => {
                    for (conn_id, _) in &players_in_room {
                        let mut scope = rhai::Scope::new();
                        let room_id_str = room.id.to_string();
                        let conn_id_str = conn_id.to_string();

                        // Build context map
                        let mut context = rhai::Map::new();
                        context.insert("trigger_type".into(), "periodic".into());

                        match trigger_engine.call_fn::<rhai::Dynamic>(
                            &mut scope,
                            &ast,
                            "run_trigger",
                            (room_id_str.clone(), conn_id_str.clone(), context.clone()),
                        ) {
                            Ok(_) => {
                                debug!(
                                    "Periodic trigger {} executed for player in room {}",
                                    trigger.script_name, room.id
                                );
                            }
                            Err(e) => {
                                let msg = format!("Periodic trigger script error in {}: {}", script_path, e);
                                error!("{}", msg);
                                broadcast_to_builders(connections, &msg);
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to compile periodic trigger script {}: {}", script_path, e);
                    error!("{}", msg);
                    broadcast_to_builders(connections, &msg);
                }
            }

            // Update last_fired timestamp
            trigger.last_fired = now;
            fired_triggers.push((idx, now));
        }

        // Commit trigger timestamp updates via CAS so a parallel room edit
        // (e.g. a builder adding a trigger via redit) doesn't get reverted.
        // The vector of triggers may shift under us; re-apply by index and
        // silently skip if the index no longer points at the same script.
        if !fired_triggers.is_empty() {
            let triggers_snapshot: Vec<(usize, String, i64)> = fired_triggers
                .iter()
                .filter_map(|(idx, ts)| room.triggers.get(*idx).map(|t| (*idx, t.script_name.clone(), *ts)))
                .collect();
            let _ = db.update_room(&room_id, |r| {
                for (idx, script_name, ts) in &triggers_snapshot {
                    if let Some(t) = r.triggers.get_mut(*idx) {
                        if &t.script_name == script_name {
                            t.last_fired = *ts;
                        }
                    }
                }
            });
        }
    }

    // Process mobile idle triggers (only when players present)
    process_mobile_idle_triggers(db, connections, now)?;

    // Process mobile always triggers (regardless of player presence)
    process_mobile_always_triggers(db, connections, now)?;

    // Process fishing bite notifications
    process_fishing_bites(connections, now);

    Ok(())
}

/// Process idle triggers for mobiles in rooms with players
fn process_mobile_idle_triggers(db: &db::Db, connections: &SharedConnections, now: i64) -> Result<()> {
    use rand::Rng;

    // Build a map of room_id -> list of awake players for rooms with logged-in characters
    // Sleeping players don't see mobile idle actions (emotes, says, etc.)
    let rooms_with_players: std::collections::HashMap<
        uuid::Uuid,
        Vec<(uuid::Uuid, tokio::sync::mpsc::UnboundedSender<String>)>,
    > = {
        let conns = connections.lock().unwrap();
        let mut map: std::collections::HashMap<uuid::Uuid, Vec<_>> = std::collections::HashMap::new();
        for (conn_id, session) in conns.iter() {
            if let Some(ref char) = session.character {
                // Skip sleeping players - they don't notice mobile idle actions
                if char.position == CharacterPosition::Sleeping {
                    continue;
                }
                map.entry(char.current_room_id)
                    .or_default()
                    .push((*conn_id, session.sender.clone()));
            }
        }
        map
    };

    if rooms_with_players.is_empty() {
        return Ok(());
    }

    // For each room with players, check mobiles for idle triggers
    for (room_id, players) in &rooms_with_players {
        let mobiles = db.get_mobiles_in_room(room_id)?;

        for mut mobile in mobiles {
            let mut mobile_modified = false;

            // Extract mobile info needed for trigger execution before iterating
            let mobile_name = mobile.name.clone();
            let mobile_id = mobile.id;
            let mobile_room_id = mobile.current_room_id;
            let mut fired: Vec<(usize, String, i64)> = Vec::new();

            for (idx, trigger) in mobile.triggers.iter_mut().enumerate() {
                // Only process OnIdle triggers
                if trigger.trigger_type != MobileTriggerType::OnIdle {
                    continue;
                }
                if !trigger.enabled {
                    continue;
                }

                // Check if enough time has passed
                if now < trigger.last_fired + trigger.interval_secs {
                    continue;
                }

                // Check chance
                if trigger.chance < 100 {
                    let roll: i32 = rand::thread_rng().gen_range(1..=100);
                    if roll > trigger.chance {
                        // Update timestamp even on failed roll to avoid rapid re-rolls
                        trigger.last_fired = now;
                        mobile_modified = true;
                        fired.push((idx, trigger.script_name.clone(), now));
                        continue;
                    }
                }

                // Execute the trigger with extracted mobile info
                execute_mobile_idle_trigger(
                    &mobile_name,
                    mobile_id,
                    mobile_room_id,
                    trigger,
                    players,
                    db,
                    connections,
                );

                trigger.last_fired = now;
                mobile_modified = true;
                fired.push((idx, trigger.script_name.clone(), now));
            }

            if mobile_modified {
                let _ = db.update_mobile(&mobile_id, |m| {
                    for (idx, name, ts) in &fired {
                        if let Some(t) = m.triggers.get_mut(*idx) {
                            if &t.script_name == name {
                                t.last_fired = *ts;
                            }
                        }
                    }
                });
            }
        }
    }

    Ok(())
}

/// Process always triggers for all mobiles regardless of player presence
fn process_mobile_always_triggers(db: &db::Db, connections: &SharedConnections, now: i64) -> Result<()> {
    use rand::Rng;

    let all_mobiles = db.list_all_mobiles()?;

    // Build a map of rooms with awake players (for template broadcasts)
    let rooms_with_players: std::collections::HashMap<
        uuid::Uuid,
        Vec<(uuid::Uuid, tokio::sync::mpsc::UnboundedSender<String>)>,
    > = {
        let conns = connections.lock().unwrap();
        let mut map: std::collections::HashMap<uuid::Uuid, Vec<_>> = std::collections::HashMap::new();
        for (conn_id, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.position == CharacterPosition::Sleeping {
                    continue;
                }
                map.entry(char.current_room_id)
                    .or_default()
                    .push((*conn_id, session.sender.clone()));
            }
        }
        map
    };

    for mut mobile in all_mobiles {
        // Skip prototypes (no room assigned)
        let mobile_room_id = match mobile.current_room_id {
            Some(rid) => rid,
            None => continue,
        };

        // Quick check: does this mobile have any always triggers?
        let has_always = mobile
            .triggers
            .iter()
            .any(|t| t.trigger_type == MobileTriggerType::OnAlways && t.enabled);
        if !has_always {
            continue;
        }

        let mobile_name = mobile.name.clone();
        let mobile_id = mobile.id;
        let mut mobile_modified = false;
        let mut fired: Vec<(usize, String, i64)> = Vec::new();
        let empty_players = Vec::new();
        let players = rooms_with_players.get(&mobile_room_id).unwrap_or(&empty_players);

        for (idx, trigger) in mobile.triggers.iter_mut().enumerate() {
            if trigger.trigger_type != MobileTriggerType::OnAlways {
                continue;
            }
            if !trigger.enabled {
                continue;
            }

            // Check if enough time has passed
            if now < trigger.last_fired + trigger.interval_secs {
                continue;
            }

            // Check chance
            if trigger.chance < 100 {
                let roll: i32 = rand::thread_rng().gen_range(1..=100);
                if roll > trigger.chance {
                    trigger.last_fired = now;
                    mobile_modified = true;
                    fired.push((idx, trigger.script_name.clone(), now));
                    continue;
                }
            }

            // Execute the trigger (reuses idle trigger execution)
            execute_mobile_idle_trigger(
                &mobile_name,
                mobile_id,
                Some(mobile_room_id),
                trigger,
                players,
                db,
                connections,
            );

            trigger.last_fired = now;
            mobile_modified = true;
            fired.push((idx, trigger.script_name.clone(), now));
        }

        if mobile_modified {
            let _ = db.update_mobile(&mobile_id, |m| {
                for (idx, name, ts) in &fired {
                    if let Some(t) = m.triggers.get_mut(*idx) {
                        if &t.script_name == name {
                            t.last_fired = *ts;
                        }
                    }
                }
            });
        }
    }

    Ok(())
}

/// Execute a mobile idle trigger (template or custom script)
fn execute_mobile_idle_trigger(
    mobile_name: &str,
    mobile_id: uuid::Uuid,
    mobile_room_id: Option<uuid::Uuid>,
    trigger: &MobileTrigger,
    players: &[(uuid::Uuid, tokio::sync::mpsc::UnboundedSender<String>)],
    db: &db::Db,
    connections: &SharedConnections,
) {
    use rand::Rng;

    // Handle built-in templates (script_name starts with @)
    if trigger.script_name.starts_with('@') {
        let template_name = &trigger.script_name[1..];
        execute_mobile_idle_template(template_name, &trigger.args, mobile_name, players);
        return;
    }

    // Load and execute custom trigger script
    let script_path = format!("scripts/triggers/{}.rhai", trigger.script_name);
    let script_content = match std::fs::read_to_string(&script_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to load mobile idle trigger script {}: {}", script_path, e);
            return;
        }
    };

    // Create engine and execute
    let mut trigger_engine = rhai::Engine::new();

    // Register broadcast function for idle triggers (uses connections for room lookup)
    let conns_clone = connections.clone();
    trigger_engine.register_fn(
        "broadcast_to_room",
        move |rid: String, message: String, _exclude: String| {
            if let Ok(room_uuid) = uuid::Uuid::parse_str(&rid) {
                let conns = conns_clone.lock().unwrap();
                for (_, session) in conns.iter() {
                    if let Some(ref char) = session.character {
                        if char.current_room_id == room_uuid {
                            let _ = session.sender.send(format!("{}\n", message));
                        }
                    }
                }
            }
        },
    );

    // Register random_int
    trigger_engine.register_fn("random_int", |min: i64, max: i64| {
        if min >= max {
            return min;
        }
        rand::thread_rng().gen_range(min..=max)
    });

    // Register get_mobile_data to return the mobile info
    let mobile_name_clone = mobile_name.to_string();
    trigger_engine.register_fn("get_mobile_data", move |_id: String| {
        let mut map = rhai::Map::new();
        map.insert("name".into(), mobile_name_clone.clone().into());
        map
    });

    // Register DoorState type with getters
    trigger_engine
        .register_type_with_name::<DoorState>("DoorState")
        .register_get("name", |d: &mut DoorState| d.name.clone())
        .register_get("is_closed", |d: &mut DoorState| d.is_closed)
        .register_get("is_locked", |d: &mut DoorState| d.is_locked)
        .register_get("key_vnum", |d: &mut DoorState| d.key_vnum.clone().unwrap_or_default())
        .register_get("description", |d: &mut DoorState| {
            d.description.clone().unwrap_or_default()
        })
        .register_get("keywords", |d: &mut DoorState| {
            d.keywords
                .iter()
                .map(|s: &String| rhai::Dynamic::from(s.clone()))
                .collect::<Vec<_>>()
        });

    // Register door functions
    let db_clone = db.clone();
    trigger_engine.register_fn("get_door", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = db_clone.get_room_data(&uuid) {
                let dir = direction.to_lowercase();
                if let Some(door) = room.doors.get(&dir) {
                    return rhai::Dynamic::from(door.clone());
                }
            }
        }
        rhai::Dynamic::UNIT
    });

    let db_clone = db.clone();
    trigger_engine.register_fn("has_door", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = db_clone.get_room_data(&uuid) {
                return room.doors.contains_key(&direction.to_lowercase());
            }
        }
        false
    });

    let db_clone = db.clone();
    trigger_engine.register_fn(
        "set_door_closed",
        move |room_id: String, direction: String, closed: bool| {
            let uuid = match uuid::Uuid::parse_str(&room_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let dir = direction.to_lowercase();
            let mut found = false;
            let result = db_clone.update_room(&uuid, |r| {
                if let Some(door) = r.doors.get_mut(&dir) {
                    door.is_closed = closed;
                    found = true;
                }
            });
            result.is_ok() && found
        },
    );

    let db_clone = db.clone();
    trigger_engine.register_fn("get_exit_target", move |room_id: String, direction: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            if let Ok(Some(room)) = db_clone.get_room_data(&uuid) {
                let target = match direction.to_lowercase().as_str() {
                    "north" | "n" => room.exits.north,
                    "south" | "s" => room.exits.south,
                    "east" | "e" => room.exits.east,
                    "west" | "w" => room.exits.west,
                    "up" | "u" => room.exits.up,
                    "down" | "d" => room.exits.down,
                    _ => None,
                };
                return target.map(|u| u.to_string()).unwrap_or_default();
            }
        }
        String::new()
    });

    // Compile and run trigger
    match trigger_engine.compile(&script_content) {
        Ok(ast) => {
            let mut scope = rhai::Scope::new();
            let mobile_id_str = mobile_id.to_string();
            let room_id_str = mobile_room_id.map(|r| r.to_string()).unwrap_or_default();

            // Build context map
            let mut context = rhai::Map::new();
            context.insert("trigger_type".into(), "idle".into());
            context.insert("mobile_name".into(), mobile_name.to_string().into());

            match trigger_engine.call_fn::<rhai::Dynamic>(
                &mut scope,
                &ast,
                "run_trigger",
                (mobile_id_str, room_id_str, context),
            ) {
                Ok(_) => {
                    debug!(
                        "Mobile idle trigger {} executed for {}",
                        trigger.script_name, mobile_name
                    );
                }
                Err(e) => {
                    let msg = format!("Mobile idle trigger script error in {}: {}", script_path, e);
                    error!("{}", msg);
                    broadcast_to_builders(connections, &msg);
                }
            }
        }
        Err(e) => {
            let msg = format!("Failed to compile mobile idle trigger script {}: {}", script_path, e);
            error!("{}", msg);
            broadcast_to_builders(connections, &msg);
        }
    }
}

/// Execute a built-in mobile idle template
fn execute_mobile_idle_template(
    template_name: &str,
    args: &[String],
    mobile_name: &str,
    players: &[(uuid::Uuid, tokio::sync::mpsc::UnboundedSender<String>)],
) {
    use rand::Rng;

    let broadcast = |msg: &str| {
        for (_, sender) in players {
            let _ = sender.send(format!("{}\n", msg));
        }
    };

    match template_name {
        "say_greeting" | "say_idle" => {
            if let Some(message) = args.first() {
                broadcast(&format!("{} says: \"{}\"", mobile_name, message));
            }
        }
        "say_random" | "idle_random" => {
            if !args.is_empty() {
                let idx = rand::thread_rng().gen_range(0..args.len());
                broadcast(&format!("{} says: \"{}\"", mobile_name, args[idx]));
            }
        }
        "emote" | "emote_idle" => {
            if let Some(action) = args.first() {
                broadcast(&format!("{} {}", mobile_name, action));
            }
        }
        _ => {
            tracing::warn!("Unknown mobile idle template: @{}", template_name);
        }
    }
}

/// Process fishing bite notifications for players who are currently fishing
fn process_fishing_bites(connections: &SharedConnections, now: i64) {
    let mut conns = connections.lock().unwrap();

    for (_conn_id, session) in conns.iter_mut() {
        // Skip if not fishing
        let fishing_state = match &mut session.fishing_state {
            Some(state) => state,
            None => continue,
        };

        // Skip if bite hasn't happened yet
        if now < fishing_state.bite_time {
            continue;
        }

        // Check if player is still in the same room
        if let Some(ref char) = session.character {
            if char.current_room_id != fishing_state.room_id {
                // Player moved - cancel fishing
                session.fishing_state = None;
                let _ = session
                    .sender
                    .send("Your fishing line snaps as you moved away from the water!\n".to_string());
                continue;
            }
        }

        // Calculate how long the bite has been waiting (they might miss it if they wait too long)
        let wait_time = now - fishing_state.bite_time;

        // First notification: something tugged the line (send once)
        if !fishing_state.bite_notified {
            fishing_state.bite_notified = true;
            let _ = session
                .sender
                .send("\n*** Something tugs at your line! Type 'reel' to pull it in! ***\n".to_string());
        } else if wait_time >= 5 && !fishing_state.warning_notified {
            // Warning after 5 seconds (send once)
            fishing_state.warning_notified = true;
            let _ = session
                .sender
                .send("\n*** Your line is still taut... 'reel' before it gets away! ***\n".to_string());
        } else if wait_time >= 15 {
            // Fish got away after 15 seconds of not reeling
            session.fishing_state = None;
            let _ = session
                .sender
                .send("\nYou waited too long and the fish got away!\n".to_string());
        }
    }
}
