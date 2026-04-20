//! Pursuit tick system for IronMUD
//!
//! Handles mob pursuit after being sniped from an adjacent room.
//! Mobs with active pursuit state move toward the sniper's room,
//! then engage in combat if the target is found.

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::{debug, error};

use ironmud::{
    CharacterPosition, CombatDistance, CombatTarget, CombatTargetType, CombatZoneType, SharedConnections, db,
};

use super::broadcast::{
    broadcast_to_room_awake, broadcast_to_room_except_awake, broadcast_to_room_mobiles, send_message_to_character,
    sync_character_to_session,
};
use super::mobile::{get_opposite_direction_rust, get_valid_wander_exits, propagate_mobile_followers};

/// Pursuit tick interval in seconds
pub const PURSUIT_TICK_INTERVAL_SECS: u64 = 10;

/// Background task that processes mob pursuit periodically
pub async fn run_pursuit_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(PURSUIT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_pursuit_tick(&db, &connections) {
            error!("Pursuit tick error: {}", e);
        }
    }
}

/// Process pursuit for all mobs with active pursuit state
fn process_pursuit_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use rand::Rng;
    use rand::seq::SliceRandom;

    let mobiles = db.list_all_mobiles()?;
    let mut rng = rand::thread_rng();

    for mobile in mobiles {
        // Skip prototypes, dead mobs, and mobs not pursuing
        if mobile.is_prototype || mobile.current_hp <= 0 || mobile.pursuit_target_room.is_none() {
            continue;
        }

        // Re-fetch for freshness
        let current_mobile = match db.get_mobile_data(&mobile.id)? {
            Some(m) => m,
            None => continue,
        };

        // Double-check pursuit is still active after re-fetch
        if current_mobile.pursuit_target_room.is_none() {
            continue;
        }

        // Skip mobs already in combat
        if current_mobile.combat.in_combat {
            clear_pursuit(db, &current_mobile.id);
            continue;
        }

        let mobile_room_id = match current_mobile.current_room_id {
            Some(id) => id,
            None => {
                clear_pursuit(db, &current_mobile.id);
                continue;
            }
        };

        // Get current room
        let current_room = match db.get_room_data(&mobile_room_id)? {
            Some(r) => r,
            None => {
                clear_pursuit(db, &current_mobile.id);
                continue;
            }
        };

        // Build valid exits
        let valid_exits = get_valid_wander_exits(db, &current_room)?;
        if valid_exits.is_empty() {
            clear_pursuit(db, &current_mobile.id);
            continue;
        }

        // Determine movement direction
        let chosen_exit = if current_mobile.pursuit_certain {
            // Certain: use the pursuit direction
            valid_exits
                .iter()
                .find(|(dir, _)| *dir == current_mobile.pursuit_direction)
                .cloned()
        } else if !current_mobile.pursuit_direction.is_empty() && rng.gen_bool(0.5) {
            // Uncertain: 50% chance correct direction
            valid_exits
                .iter()
                .find(|(dir, _)| *dir == current_mobile.pursuit_direction)
                .cloned()
        } else {
            // Random valid exit
            valid_exits.choose(&mut rng).cloned()
        };

        // Fall back to random if the preferred direction isn't available
        let (direction, target_room_id) = match chosen_exit {
            Some(exit) => exit,
            None => match valid_exits.choose(&mut rng) {
                Some(exit) => exit.clone(),
                None => {
                    clear_pursuit(db, &current_mobile.id);
                    continue;
                }
            },
        };

        debug!(
            "Pursuit: {} moving {} toward sniper {}",
            current_mobile.name, direction, current_mobile.pursuit_target_name
        );

        // Move the mob
        if db.move_mobile_to_room(&current_mobile.id, &target_room_id).is_ok() {
            // Broadcast departure
            let departure_msg = format!("{} charges off to the {}!\n", current_mobile.name, direction);
            broadcast_to_room_mobiles(connections, &mobile_room_id, &departure_msg);

            // Broadcast arrival
            let arrival_dir = get_opposite_direction_rust(&direction);
            let arrival_msg = format!(
                "{} arrives from the {}, looking furious!\n",
                current_mobile.name, arrival_dir
            );
            broadcast_to_room_mobiles(connections, &target_room_id, &arrival_msg);

            propagate_mobile_followers(
                connections,
                &current_mobile.id,
                &current_mobile.name,
                &mobile_room_id,
                &direction,
            );

            // Check if the target player is in the arrival room
            let target_name = current_mobile.pursuit_target_name.clone();
            if let Some(player_name) = find_player_in_room_by_name(connections, &target_room_id, &target_name) {
                // Check room is not a safe zone
                let is_safe = db
                    .get_room_data(&target_room_id)
                    .ok()
                    .flatten()
                    .map(|r| r.flags.combat_zone == Some(CombatZoneType::Safe))
                    .unwrap_or(false);

                if !is_safe {
                    // Enter combat at ranged distance (mirrors aggressive mob pattern)
                    if let Ok(Some(mut char)) = db.get_character_data(&player_name) {
                        if !char.god_mode && !ironmud::check_build_mode(db, &player_name, &target_room_id) {
                            // Wake sleeping players
                            if char.position == CharacterPosition::Sleeping {
                                char.position = CharacterPosition::Standing;
                                send_message_to_character(
                                    connections,
                                    &player_name,
                                    "You are jolted awake by a furious attacker!",
                                );
                                broadcast_to_room_except_awake(
                                    connections,
                                    &target_room_id,
                                    &format!("{} is jolted awake!", char.name),
                                    &player_name,
                                );
                            }

                            // Put mobile in combat + clear pursuit atomically via CAS
                            let player_target_id = uuid::Uuid::nil();
                            let updated = db.update_mobile(&current_mobile.id, |m| {
                                m.combat.in_combat = true;
                                if !m
                                    .combat
                                    .targets
                                    .iter()
                                    .any(|t| t.target_type == CombatTargetType::Player)
                                {
                                    m.combat.targets.push(CombatTarget {
                                        target_type: CombatTargetType::Player,
                                        target_id: player_target_id,
                                    });
                                }
                                m.combat.distances.insert(player_target_id, CombatDistance::Ranged);
                                m.pursuit_target_name = String::new();
                                m.pursuit_target_room = None;
                                m.pursuit_direction = String::new();
                                m.pursuit_certain = false;
                            })?;

                            if updated.is_some() {
                                // Put the player in combat with this mobile
                                let char_name = char.name.clone();
                                let mob_id = current_mobile.id;
                                let after = db.update_character(&char_name, |c| {
                                    c.combat.in_combat = true;
                                    if !c.combat.targets.iter().any(|t| t.target_id == mob_id) {
                                        c.combat.targets.push(CombatTarget {
                                            target_type: CombatTargetType::Mobile,
                                            target_id: mob_id,
                                        });
                                    }
                                    c.combat.distances.insert(mob_id, CombatDistance::Ranged);
                                })?;
                                if let Some(fresh) = after {
                                    sync_character_to_session(connections, &fresh);
                                }

                                broadcast_to_room_awake(
                                    connections,
                                    &target_room_id,
                                    &format!("{} snarls and attacks {}!", current_mobile.name, player_name),
                                );

                                debug!("Pursuit: {} found and attacked {}", current_mobile.name, player_name);
                                continue; // Skip the clear_pursuit below, already handled
                            }
                        }
                    }
                }
            }
        }

        // Always clear pursuit state after the move attempt (single-hop)
        clear_pursuit(db, &current_mobile.id);
    }

    Ok(())
}

/// Clear pursuit state on a mobile
fn clear_pursuit(db: &db::Db, mobile_id: &uuid::Uuid) {
    let _ = db.update_mobile(mobile_id, |m| {
        m.pursuit_target_name = String::new();
        m.pursuit_target_room = None;
        m.pursuit_direction = String::new();
        m.pursuit_certain = false;
    });
}

/// Find a specific player by name in a room
fn find_player_in_room_by_name(
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
    target_name: &str,
) -> Option<String> {
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id && char.name.to_lowercase() == target_name.to_lowercase() {
                    return Some(char.name.clone());
                }
            }
        }
    }
    None
}
