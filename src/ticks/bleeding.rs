//! Bleeding tick system for IronMUD
//!
//! Applies bleeding damage from wounds at a consistent interval (30s),
//! regardless of whether the character/mobile is in combat.

use anyhow::Result;
use tokio::time::{interval, Duration};
use tracing::error;

use ironmud::{db, BloodTrail, SharedConnections};

use super::broadcast::{
    broadcast_to_room_awake, broadcast_to_room_except, send_message_to_character,
    sync_character_to_session,
};
use super::combat::{process_mobile_death, process_player_death};

/// Bleeding tick interval in seconds
pub const BLEEDING_TICK_INTERVAL_SECS: u64 = 30;

/// Blood trail expiry time in seconds (5 minutes)
const BLOOD_TRAIL_EXPIRY_SECS: i64 = 300;

/// Deposit a blood trail in a room (bled in place, no direction)
fn deposit_blood_trail(db: &db::Db, room_id: &uuid::Uuid, name: &str, bleeding: i32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let severity = bleeding.clamp(1, 5);
    let name_lower = name.to_lowercase();
    let name_owned = name.to_string();

    let _ = db.update_room(room_id, |room| {
        // Expire old trails
        room.blood_trails.retain(|t| now - t.timestamp < BLOOD_TRAIL_EXPIRY_SECS);

        if let Some(existing) = room
            .blood_trails
            .iter_mut()
            .find(|t| t.name.to_lowercase() == name_lower)
        {
            existing.timestamp = now;
            existing.severity = severity;
        } else {
            room.blood_trails.push(BloodTrail {
                name: name_owned.clone(),
                severity,
                timestamp: now,
                direction: None,
            });
        }

        while room.blood_trails.len() > 10 {
            room.blood_trails.remove(0);
        }
    });
}

/// Background task that processes bleeding damage periodically
pub async fn run_bleeding_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(BLEEDING_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_bleeding_tick(&db, &connections) {
            error!("Bleeding tick error: {}", e);
        }
    }
}

/// Process bleeding damage for all characters and mobiles
fn process_bleeding_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    process_character_bleeding(db, connections)?;
    process_mobile_bleeding(db, connections)?;
    Ok(())
}

/// Phase A: Process bleeding for all logged-in player characters
fn process_character_bleeding(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    // Collect player names outside the lock
    let player_names: Vec<String> = {
        let conns = connections.lock().unwrap();
        conns
            .iter()
            .filter_map(|(_, session)| {
                let char = session.character.as_ref()?;
                if !char.creation_complete || char.god_mode {
                    return None;
                }
                Some(char.name.clone())
            })
            .collect()
    };

    for char_name in player_names {
        let char = match db.get_character_data(&char_name)? {
            Some(c) => c,
            None => continue,
        };

        // Skip bleeding for build mode players in their editable areas
        if ironmud::check_build_mode(db, &char_name, &char.current_room_id) {
            continue;
        }

        // Skip if already dead/unconscious and in combat (combat tick handles bleedout there)
        if char.is_unconscious && char.combat.in_combat {
            continue;
        }

        let room_id = char.current_room_id;

        // Process bleedout timer for unconscious characters NOT in combat
        if char.is_unconscious {
            let post = db.update_character(&char_name, |c| {
                c.bleedout_rounds_remaining -= 1;
            })?;
            let Some(mut char) = post else { continue };

            if char.bleedout_rounds_remaining <= 0 {
                process_player_death(db, connections, &mut char, &room_id)?;
                continue;
            }

            send_message_to_character(
                connections,
                &char_name,
                &format!(
                    "You are unconscious and bleeding out! {} rounds remaining...",
                    char.bleedout_rounds_remaining
                ),
            );
            continue;
        }

        // Sum bleeding severity from all wounds
        let bleeding: i32 = char.wounds.iter().map(|w| w.bleeding_severity).sum();
        if bleeding <= 0 {
            continue;
        }

        // Hemophiliac trait: +30% bleed damage
        let has_hemophiliac = char.traits.iter().any(|t| t == "hemophiliac");
        let bleed_mod = if has_hemophiliac { 130 } else { 100 };
        let adjusted_bleeding = (bleeding * bleed_mod / 100).max(1);

        let Some(mut char) = db.update_character(&char_name, |c| {
            c.hp -= adjusted_bleeding;
        })? else { continue };

        // Deposit blood trail in room
        deposit_blood_trail(db, &room_id, &char.name, bleeding);

        send_message_to_character(
            connections,
            &char_name,
            &format!("You lose {} HP from bleeding!", adjusted_bleeding),
        );

        if char.hp <= 0 {
            let post = db.update_character(&char_name, |c| {
                c.is_unconscious = true;
                c.bleedout_rounds_remaining = 5;
            })?;
            if let Some(updated) = post {
                char = updated;
            }
            sync_character_to_session(connections, &char);

            send_message_to_character(
                connections,
                &char_name,
                "You collapse, unconscious from blood loss!",
            );
            broadcast_to_room_except(
                connections,
                &room_id,
                &format!("{} collapses, unconscious!", char.name),
                &char_name,
            );
            continue;
        }

        sync_character_to_session(connections, &char);
    }

    Ok(())
}

/// Phase B: Process bleeding for all spawned mobiles
fn process_mobile_bleeding(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let mobiles = db.list_all_mobiles()?;

    for mut mobile in mobiles {
        // Skip prototypes (no room assignment) and dead mobiles
        if mobile.is_prototype || mobile.current_hp <= 0 {
            continue;
        }

        let room_id = match mobile.current_room_id {
            Some(id) => id,
            None => continue,
        };

        let bleeding: i32 = mobile.wounds.iter().map(|w| w.bleeding_severity).sum();
        if bleeding <= 0 {
            continue;
        }

        let after = db.update_mobile(&mobile.id, |m| {
            m.current_hp -= bleeding;
        })?;
        if let Some(m) = after {
            mobile.current_hp = m.current_hp;
        }

        // Deposit blood trail in room
        deposit_blood_trail(db, &room_id, &mobile.name, bleeding);

        broadcast_to_room_awake(
            connections,
            &room_id,
            &format!("{} loses {} HP from bleeding!", mobile.name, bleeding),
        );

        if mobile.current_hp <= 0 {
            process_mobile_death(db, connections, &mut mobile, &room_id)?;
        }
    }

    Ok(())
}
