//! Mobile tick systems for IronMUD
//!
//! Handles mobile wandering, aggressive behavior, and periodic effects like poison emotes.

use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use tokio::time::{Duration, interval};
use tracing::{debug, error, warn};

use ironmud::{
    CharacterPosition, CombatDistance, CombatTarget, CombatTargetType, CombatZoneType, InputEvent, ItemData,
    MobileData, RoomData, SharedConnections, WoundType, broadcast_to_builders, db, get_opposite_direction,
};

use super::broadcast::{
    broadcast_to_room_awake, broadcast_to_room_except_awake, broadcast_to_room_mobiles, send_message_to_character,
    sync_character_to_session,
};

/// Mobile wandering tick interval in seconds
pub const WANDER_TICK_INTERVAL_SECS: u64 = 60;

/// Mobile effects tick interval in seconds
pub const MOBILE_EFFECTS_TICK_INTERVAL_SECS: u64 = 30;

/// Background task that processes mobile wandering periodically
pub async fn run_wander_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(WANDER_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_wander_tick(&db, &connections) {
            error!("Wander tick error: {}", e);
        }
    }
}

/// Maximum BFS depth for routine pathfinding
const MAX_BFS_DEPTH: usize = 20;

/// Action needed to pass through a door during routine movement
enum DoorAction {
    /// Door is closed but unlocked - just open it
    Open,
    /// Door is closed and locked - mobile has the key
    UnlockAndOpen,
}

/// Get exits that a mobile with can_open_doors can traverse (includes closed doors).
/// Returns (direction, target_room_id, optional door action needed).
fn get_routine_exits(
    db: &db::Db,
    room: &RoomData,
    mobile_key_ids: &HashSet<uuid::Uuid>,
    can_open_doors: bool,
    cant_swim: bool,
) -> Result<Vec<(String, uuid::Uuid, Option<DoorAction>)>> {
    let mut exits = Vec::new();

    let directions = [
        ("north", room.exits.north),
        ("south", room.exits.south),
        ("east", room.exits.east),
        ("west", room.exits.west),
        ("up", room.exits.up),
        ("down", room.exits.down),
    ];

    for (dir_name, exit_opt) in directions {
        if let Some(target_id) = exit_opt {
            let mut door_action = None;

            // Check for door
            if let Some(door) = room.doors.get(dir_name) {
                if door.is_closed {
                    if !can_open_doors {
                        continue; // Can't pass through closed door
                    }
                    if door.is_locked {
                        // Check if mobile has the key
                        if let Some(key_id) = door.key_id {
                            if mobile_key_ids.contains(&key_id) {
                                door_action = Some(DoorAction::UnlockAndOpen);
                            } else {
                                continue; // Locked and no key
                            }
                        } else {
                            continue; // Locked with no key_id defined
                        }
                    } else {
                        door_action = Some(DoorAction::Open);
                    }
                }
            }

            // Check target room's no_mob flag and water flags
            if let Ok(Some(target_room)) = db.get_room_data(&target_id) {
                if target_room.flags.no_mob {
                    continue;
                }
                // cant_swim mobiles cannot enter any water rooms
                if cant_swim
                    && (target_room.flags.shallow_water || target_room.flags.deep_water || target_room.flags.underwater)
                {
                    continue;
                }
                exits.push((dir_name.to_string(), target_id, door_action));
            }
        }
    }

    Ok(exits)
}

/// Render a room as `<vnum> "<title>"` (or `<uuid-prefix> "<title>"` if the
/// room has no vnum, or `<uuid>` if the room is missing). Used only for log
/// and builder-debug messages — not gameplay.
fn describe_room(db: &db::Db, id: &uuid::Uuid) -> String {
    match db.get_room_data(id) {
        Ok(Some(room)) => {
            let label = room.vnum.clone().unwrap_or_else(|| {
                let s = id.to_string();
                s[..8.min(s.len())].to_string()
            });
            format!("{} \"{}\"", label, room.title)
        }
        _ => id.to_string(),
    }
}

/// Outcome of a BFS pathfinding attempt for routine movement.
enum BfsOutcome {
    /// Found a next step toward the destination.
    Step { direction: String },
    /// Source and destination are the same room.
    AlreadyThere,
    /// Explored the reachable graph but never found the destination. `explored`
    /// is the number of distinct rooms that were reachable from `from`.
    NoPath { explored: usize },
    /// BFS hit the depth cap before finding the destination. `explored` counts
    /// the rooms visited up to that depth.
    TooFar { explored: usize },
}

/// BFS pathfinding: find the next step direction to move from `from` toward `to`.
/// Returns a [`BfsOutcome`] so the caller can distinguish success, an unreachable
/// destination, and a destination beyond [`MAX_BFS_DEPTH`].
fn bfs_next_step(db: &db::Db, from: uuid::Uuid, to: uuid::Uuid, mobile: &MobileData) -> BfsOutcome {
    if from == to {
        return BfsOutcome::AlreadyThere;
    }

    // Collect mobile's key IDs once for door checks
    let mobile_key_ids: HashSet<uuid::Uuid> = if mobile.flags.can_open_doors {
        db.get_items_in_mobile_inventory(&mobile.id)
            .unwrap_or_default()
            .iter()
            .map(|item| item.id)
            .collect()
    } else {
        HashSet::new()
    };

    // BFS queue: (current_room, first_step_direction, first_step_room)
    let mut queue: VecDeque<(uuid::Uuid, String, uuid::Uuid)> = VecDeque::new();
    let mut visited: HashSet<uuid::Uuid> = HashSet::new();
    visited.insert(from);

    // Seed with exits from starting room
    if let Ok(Some(start_room)) = db.get_room_data(&from) {
        if let Ok(exits) = get_routine_exits(
            db,
            &start_room,
            &mobile_key_ids,
            mobile.flags.can_open_doors,
            mobile.flags.cant_swim,
        ) {
            for (dir, target_id, _) in exits {
                if !visited.contains(&target_id) {
                    visited.insert(target_id);
                    if target_id == to {
                        return BfsOutcome::Step { direction: dir };
                    }
                    queue.push_back((target_id, dir, target_id));
                }
            }
        }
    }

    let mut depth = 1;
    let mut nodes_at_depth = queue.len();
    let mut nodes_processed = 0;

    while let Some((current, first_dir, first_room)) = queue.pop_front() {
        nodes_processed += 1;
        if nodes_processed >= nodes_at_depth {
            depth += 1;
            if depth > MAX_BFS_DEPTH {
                return BfsOutcome::TooFar {
                    explored: visited.len(),
                };
            }
            nodes_at_depth = queue.len();
            nodes_processed = 0;
        }

        if let Ok(Some(room)) = db.get_room_data(&current) {
            if let Ok(exits) = get_routine_exits(
                db,
                &room,
                &mobile_key_ids,
                mobile.flags.can_open_doors,
                mobile.flags.cant_swim,
            ) {
                for (_, target_id, _) in exits {
                    if !visited.contains(&target_id) {
                        visited.insert(target_id);
                        if target_id == to {
                            return BfsOutcome::Step { direction: first_dir };
                        }
                        queue.push_back((target_id, first_dir.clone(), first_room));
                    }
                }
            }
        }
    }

    BfsOutcome::NoPath {
        explored: visited.len(),
    }
}

/// Handle opening/unlocking a door before a mobile moves through it.
/// Returns true if passage is now clear.
fn handle_routine_door(
    db: &db::Db,
    connections: &SharedConnections,
    mobile: &MobileData,
    room_id: &uuid::Uuid,
    direction: &str,
) -> Result<bool> {
    let mut room = match db.get_room_data(room_id)? {
        Some(r) => r,
        None => return Ok(false),
    };

    let door = match room.doors.get(direction) {
        Some(d) if d.is_closed => d.clone(),
        _ => return Ok(true), // No closed door, passage is clear
    };

    let door_name = door.name.clone();
    let was_locked = door.is_locked;

    // Unlock if locked
    if was_locked {
        // Unlock departure side
        if let Some(d) = room.doors.get_mut(direction) {
            d.is_locked = false;
        }
        broadcast_to_room_awake(
            connections,
            room_id,
            &format!("{} unlocks the {}.", mobile.name, door_name),
        );
    }

    // Open departure side
    if let Some(d) = room.doors.get_mut(direction) {
        d.is_closed = false;
    }
    db.save_room_data(room)?;

    broadcast_to_room_awake(
        connections,
        room_id,
        &format!("{} opens the {}.", mobile.name, door_name),
    );

    // Update the other side of the door
    if let Some(exit_target) = get_exit_target_for_direction(&db.get_room_data(room_id)?.unwrap(), direction) {
        if let Some(opposite_dir) = get_opposite_direction(direction) {
            if let Ok(Some(mut target_room)) = db.get_room_data(&exit_target) {
                if let Some(other_door) = target_room.doors.get_mut(opposite_dir) {
                    if was_locked {
                        other_door.is_locked = false;
                    }
                    other_door.is_closed = false;
                    db.save_room_data(target_room)?;
                }
            }
        }
    }

    Ok(true)
}

/// Close (and optionally re-lock) a door behind a mobile after passing through.
/// `departure_room_id` is the room the mobile just left (where the door is).
fn close_door_behind(
    db: &db::Db,
    connections: &SharedConnections,
    direction: &str,
    departure_room_id: &uuid::Uuid,
    was_locked: bool,
) -> Result<()> {
    let mut room = match db.get_room_data(departure_room_id)? {
        Some(r) => r,
        None => return Ok(()),
    };

    let door_name = match room.doors.get(direction) {
        Some(d) => d.name.clone(),
        None => return Ok(()),
    };

    // Close departure side
    if let Some(d) = room.doors.get_mut(direction) {
        d.is_closed = true;
    }

    broadcast_to_room_awake(connections, departure_room_id, &format!("The {} closes.", door_name));

    // Re-lock if it was originally locked
    if was_locked {
        if let Some(d) = room.doors.get_mut(direction) {
            d.is_locked = true;
        }
        broadcast_to_room_awake(connections, departure_room_id, &format!("The {} locks.", door_name));
    }

    db.save_room_data(room)?;

    // Update the other side
    if let Some(exit_target) = get_exit_target_for_direction(&db.get_room_data(departure_room_id)?.unwrap(), direction)
    {
        if let Some(opposite_dir) = get_opposite_direction(direction) {
            if let Ok(Some(mut target_room)) = db.get_room_data(&exit_target) {
                if let Some(other_door) = target_room.doors.get_mut(opposite_dir) {
                    other_door.is_closed = true;
                    if was_locked {
                        other_door.is_locked = true;
                    }
                    db.save_room_data(target_room)?;
                }
            }
        }
    }

    Ok(())
}

/// Get the target room UUID for a given direction from a room
fn get_exit_target_for_direction(room: &RoomData, direction: &str) -> Option<uuid::Uuid> {
    match direction {
        "north" => room.exits.north,
        "south" => room.exits.south,
        "east" => room.exits.east,
        "west" => room.exits.west,
        "up" => room.exits.up,
        "down" => room.exits.down,
        _ => None,
    }
}

/// Check if a mobile's active routine entry suppresses wandering
fn should_suppress_wander(mobile: &MobileData) -> bool {
    // Suppress if mobile has a routine destination it's walking toward
    if mobile.routine_destination_room.is_some() {
        return true;
    }

    // Suppress if active routine entry has suppress_wander set
    if !mobile.daily_routine.is_empty() {
        // We need the game hour but don't have db access here, so we check
        // the current_activity indirectly through the routine entries.
        // The routine tick sets current_activity, so we check all entries for
        // suppress_wander matching the current activity.
        for entry in &mobile.daily_routine {
            if entry.activity == mobile.current_activity && entry.suppress_wander {
                return true;
            }
        }
    }

    false
}

/// Process wandering for all non-sentinel mobiles
fn process_wander_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use rand::Rng;
    use rand::seq::SliceRandom;

    let wander_chance_percent: u32 = db
        .get_setting_or_default("wander_chance_percent", "33")
        .unwrap_or_else(|_| "33".to_string())
        .parse::<u32>()
        .unwrap_or(33)
        .min(100);

    let mobiles = db.list_all_mobiles()?;
    let mut rng = rand::thread_rng();

    for mobile in mobiles {
        // Skip prototypes - only process spawned instances
        if mobile.is_prototype {
            continue;
        }

        // Skip sentinel mobiles (they never wander)
        // BUT: sentinel mobiles with a routine destination should still process step movement
        if mobile.flags.sentinel && mobile.routine_destination_room.is_none() {
            continue;
        }

        // Re-fetch the mobile from DB to get current combat state
        // (combat state may have changed since we loaded the list)
        let mut current_mobile = match db.get_mobile_data(&mobile.id)? {
            Some(m) => m,
            None => continue, // Mobile was deleted
        };

        // Skip mobiles in combat (using fresh data)
        if current_mobile.combat.in_combat {
            debug!(
                "Wander: skipping {} - in combat (targets={})",
                current_mobile.name,
                current_mobile.combat.targets.len()
            );
            continue;
        }

        // Skip dead mobiles (safety check, using fresh data)
        if current_mobile.current_hp <= 0 {
            debug!(
                "Wander: skipping {} - dead (hp={})",
                current_mobile.name, current_mobile.current_hp
            );
            continue;
        }

        // === Routine destination step movement ===
        // Process BEFORE aggressive behavior and random wandering
        if let Some(dest_room_id) = current_mobile.routine_destination_room {
            if let Some(current_room_id) = current_mobile.current_room_id {
                if current_room_id == dest_room_id {
                    // Already at destination, clear it via CAS so we don't
                    // clobber needs/activity updates the sim tick may have
                    // written since we loaded current_mobile.
                    db.update_mobile(&current_mobile.id, |m| {
                        m.routine_destination_room = None;
                    })?;
                    continue;
                }

                // BFS to find next step
                let bfs_result = bfs_next_step(db, current_room_id, dest_room_id, &current_mobile);
                if let BfsOutcome::Step { direction, .. } = bfs_result {
                    // Check for and handle door in this direction
                    let room = db.get_room_data(&current_room_id)?.unwrap();
                    let door_info = room.doors.get(&direction).map(|d| (d.is_closed, d.is_locked));

                    if let Some((is_closed, was_locked)) = door_info {
                        if is_closed {
                            if !handle_routine_door(db, connections, &current_mobile, &current_room_id, &direction)? {
                                // Can't open this door, clear destination
                                db.update_mobile(&current_mobile.id, |m| {
                                    m.routine_destination_room = None;
                                })?;
                                continue;
                            }

                            // Reload room data after door changes
                            let updated_room = db.get_room_data(&current_room_id)?.unwrap();
                            if let Some(target_id) = get_exit_target_for_direction(&updated_room, &direction) {
                                // Move through
                                if db.move_mobile_to_room(&current_mobile.id, &target_id).is_ok() {
                                    let departure_msg =
                                        format!("{} leaves heading {}.\n", current_mobile.name, direction);
                                    broadcast_to_room_mobiles(connections, &current_room_id, &departure_msg);

                                    let arrival_dir = get_opposite_direction_rust(&direction);
                                    let arrival_msg =
                                        format!("{} arrives from the {}.\n", current_mobile.name, arrival_dir);
                                    broadcast_to_room_mobiles(connections, &target_id, &arrival_msg);

                                    propagate_mobile_followers(
                                        connections,
                                        &current_mobile.id,
                                        &current_mobile.name,
                                        &current_room_id,
                                        &direction,
                                    );

                                    // Close and re-lock door behind
                                    close_door_behind(db, connections, &direction, &current_room_id, was_locked)?;
                                }
                            }
                        } else {
                            // Door exists but is open, just move normally
                            if let Some(target_id) = get_exit_target_for_direction(&room, &direction) {
                                if db.move_mobile_to_room(&current_mobile.id, &target_id).is_ok() {
                                    let departure_msg =
                                        format!("{} leaves heading {}.\n", current_mobile.name, direction);
                                    broadcast_to_room_mobiles(connections, &current_room_id, &departure_msg);

                                    let arrival_dir = get_opposite_direction_rust(&direction);
                                    let arrival_msg =
                                        format!("{} arrives from the {}.\n", current_mobile.name, arrival_dir);
                                    broadcast_to_room_mobiles(connections, &target_id, &arrival_msg);

                                    propagate_mobile_followers(
                                        connections,
                                        &current_mobile.id,
                                        &current_mobile.name,
                                        &current_room_id,
                                        &direction,
                                    );
                                }
                            }
                        }
                    } else {
                        // No door, just move
                        if let Some(target_id) = get_exit_target_for_direction(&room, &direction) {
                            if db.move_mobile_to_room(&current_mobile.id, &target_id).is_ok() {
                                let departure_msg = format!("{} leaves heading {}.\n", current_mobile.name, direction);
                                broadcast_to_room_mobiles(connections, &current_room_id, &departure_msg);

                                let arrival_dir = get_opposite_direction_rust(&direction);
                                let arrival_msg =
                                    format!("{} arrives from the {}.\n", current_mobile.name, arrival_dir);
                                broadcast_to_room_mobiles(connections, &target_id, &arrival_msg);

                                propagate_mobile_followers(
                                    connections,
                                    &current_mobile.id,
                                    &current_mobile.name,
                                    &current_room_id,
                                    &direction,
                                );
                            }
                        }
                    }

                    debug!("Routine: {} stepped toward destination", current_mobile.name);
                } else {
                    // BFS found no path - destination unreachable, clear it.
                    // Report in enough detail that a builder can troubleshoot
                    // the offending routine/room without restarting the server.
                    let reason = match &bfs_result {
                        BfsOutcome::NoPath { explored } => {
                            format!(
                                "no path found (explored {} reachable room{})",
                                explored,
                                if *explored == 1 { "" } else { "s" }
                            )
                        }
                        BfsOutcome::TooFar { explored } => format!(
                            "destination more than {} rooms away (explored {} before giving up)",
                            MAX_BFS_DEPTH, explored
                        ),
                        // AlreadyThere is handled above; Step is the success branch.
                        BfsOutcome::AlreadyThere | BfsOutcome::Step { .. } => "unknown".to_string(),
                    };
                    let from_label = describe_room(db, &current_room_id);
                    let dest_label = describe_room(db, &dest_room_id);
                    let msg = format!(
                        "Routine: {} cannot reach destination (activity '{}') — from {} to {}: {}. Clearing destination.",
                        current_mobile.name,
                        current_mobile.current_activity.to_display_string(),
                        from_label,
                        dest_label,
                        reason
                    );
                    warn!("{}", msg);
                    broadcast_to_builders(connections, &msg);
                    db.update_mobile(&current_mobile.id, |m| {
                        m.routine_destination_room = None;
                    })?;
                }
                continue; // Skip random wandering after routine movement
            }
        }

        // Check for aggressive behavior BEFORE wandering
        // Aggressive mobiles attack players on sight
        if current_mobile.flags.aggressive {
            if let Some(room_id) = current_mobile.current_room_id {
                // Check if room allows combat (not a safe zone)
                if let Ok(Some(room)) = db.get_room_data(&room_id) {
                    let is_safe = room.flags.combat_zone == Some(CombatZoneType::Safe);

                    if !is_safe {
                        // Find a player in the room to attack
                        if let Some(player_name) = find_player_name_in_room(connections, &room_id) {
                            debug!(
                                "Aggressive: {} attacking player {} in room {}",
                                current_mobile.name, player_name, room_id
                            );

                            // Get the player's character data
                            if let Ok(Some(mut char)) = db.get_character_data(&player_name) {
                                // Skip god mode and build mode players
                                if char.god_mode || ironmud::check_build_mode(&db, &player_name, &room_id) {
                                    continue;
                                }

                                // Check if player is sleeping - wake them up
                                let was_sleeping = char.position == CharacterPosition::Sleeping;
                                if was_sleeping {
                                    char.position = CharacterPosition::Standing;
                                    send_message_to_character(
                                        connections,
                                        &player_name,
                                        "You are jolted awake by an attack!",
                                    );
                                    broadcast_to_room_except_awake(
                                        connections,
                                        &room_id,
                                        &format!("{} is jolted awake!", char.name),
                                        &player_name,
                                    );
                                }

                                // Put the mobile in combat with the player
                                let player_target_id = uuid::Uuid::nil();
                                let _ = db.update_mobile(&current_mobile.id, |m| {
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
                                });

                                // Put the player in combat with this mobile
                                char.combat.in_combat = true;
                                if !char.combat.targets.iter().any(|t| t.target_id == current_mobile.id) {
                                    char.combat.targets.push(CombatTarget {
                                        target_type: CombatTargetType::Mobile,
                                        target_id: current_mobile.id,
                                    });
                                }
                                // Player also at ranged distance from mob
                                char.combat.distances.insert(current_mobile.id, CombatDistance::Ranged);
                                let _ = db.save_character_data(char.clone());
                                sync_character_to_session(connections, &char);

                                // Notify the room (sleeping players don't see this)
                                broadcast_to_room_awake(
                                    connections,
                                    &room_id,
                                    &format!("{} snarls and attacks {}!", current_mobile.name, player_name),
                                );

                                // Skip wandering - mobile is now in combat
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // Scavenger behavior: pick up a random item from the room
        if current_mobile.flags.scavenger {
            if let Some(room_id) = current_mobile.current_room_id {
                if let Ok(items) = db.get_items_in_room(&room_id) {
                    let pickable: Vec<&ItemData> =
                        items.iter().filter(|i| !i.is_prototype && !i.flags.no_get).collect();
                    if !pickable.is_empty() {
                        let item = pickable[rng.gen_range(0..pickable.len())];
                        let item_name = item.name.clone();
                        let item_id = item.id;
                        if db
                            .move_item_to_mobile_inventory(&item_id, &current_mobile.id)
                            .unwrap_or(false)
                        {
                            broadcast_to_room_awake(
                                connections,
                                &room_id,
                                &format!("{} picks up {}.", current_mobile.name, item_name),
                            );
                        }
                    }
                }
            }
        }

        // Thief behavior: attempt to steal gold from a player
        if current_mobile.flags.thief {
            if let Some(room_id) = current_mobile.current_room_id {
                // ~25% chance per tick
                if rng.gen_range(0..100) < 25 {
                    // Check room is not safe
                    if let Ok(Some(room)) = db.get_room_data(&room_id) {
                        let is_safe = room.flags.combat_zone == Some(CombatZoneType::Safe);
                        if !is_safe {
                            let players = find_players_in_room(connections, &room_id);
                            // Pick a random eligible player
                            let eligible: Vec<String> = players
                                .into_iter()
                                .filter(|name| {
                                    if let Ok(Some(c)) = db.get_character_data(name) {
                                        !c.god_mode
                                            && !ironmud::check_build_mode(&db, name, &room_id)
                                            && c.position != CharacterPosition::Sleeping
                                            && c.gold > 0
                                    } else {
                                        false
                                    }
                                })
                                .collect();

                            if let Some(target_name) = eligible.choose(&mut rng) {
                                if let Ok(Some(mut char)) = db.get_character_data(target_name) {
                                    let mob_level = current_mobile.level;
                                    let player_level = char.level;
                                    let thievery_skill = char.skills.get("thievery").map(|s| s.level).unwrap_or(0);

                                    // Success formula: 25 + (mob_level * 5) - (player_level * 3) - (thievery * 4)
                                    let success_chance = (25 + mob_level * 5 - player_level * 3 - thievery_skill * 4)
                                        .max(5)
                                        .min(75);

                                    if rng.gen_range(0..100) < success_chance {
                                        // Steal succeeded
                                        let max_steal = (char.gold / 4).max(1);
                                        let stolen = rng.gen_range(1..=max_steal);
                                        char.gold -= stolen;
                                        let _ = db.save_character_data(char.clone());
                                        sync_character_to_session(connections, &char);

                                        current_mobile.gold += stolen;
                                        let _ = db.update_mobile(&current_mobile.id, |m| {
                                            m.gold += stolen;
                                        });

                                        send_message_to_character(
                                            connections,
                                            target_name,
                                            &format!("You feel lighter... was that {}?", current_mobile.name),
                                        );
                                    } else {
                                        // Steal failed - caught!
                                        send_message_to_character(
                                            connections,
                                            target_name,
                                            &format!("{} tried to steal from you!", current_mobile.name),
                                        );
                                        broadcast_to_room_except_awake(
                                            connections,
                                            &room_id,
                                            &format!(
                                                "{} is caught trying to pick {}'s pocket!",
                                                current_mobile.name, target_name
                                            ),
                                            target_name,
                                        );

                                        // Combat chance based on mob flags
                                        let combat_chance = if current_mobile.flags.aggressive {
                                            100
                                        } else if current_mobile.flags.cowardly {
                                            0
                                        } else {
                                            50
                                        };

                                        if current_mobile.flags.cowardly {
                                            // Cowardly thief tries to flee - skip to wander
                                        } else if rng.gen_range(0..100) < combat_chance {
                                            // Enter combat with the player
                                            let player_target_id = uuid::Uuid::nil();
                                            let _ = db.update_mobile(&current_mobile.id, |m| {
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
                                                m.combat.distances.insert(player_target_id, CombatDistance::Melee);
                                            });

                                            char.combat.in_combat = true;
                                            if !char.combat.targets.iter().any(|t| t.target_id == current_mobile.id) {
                                                char.combat.targets.push(CombatTarget {
                                                    target_type: CombatTargetType::Mobile,
                                                    target_id: current_mobile.id,
                                                });
                                            }
                                            char.combat.distances.insert(current_mobile.id, CombatDistance::Melee);
                                            let _ = db.save_character_data(char.clone());
                                            sync_character_to_session(connections, &char);

                                            broadcast_to_room_awake(
                                                connections,
                                                &room_id,
                                                &format!(
                                                    "{} draws a weapon and attacks {}!",
                                                    current_mobile.name, target_name
                                                ),
                                            );
                                        }

                                        // Guard response to caught thief
                                        handle_guard_response(
                                            db,
                                            connections,
                                            &current_mobile,
                                            &room_id,
                                            target_name,
                                            &mut rng,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check if routine suppresses wandering
        if should_suppress_wander(&current_mobile) {
            continue;
        }

        // Sentinel mobiles should not randomly wander (only routine movement above)
        if current_mobile.flags.sentinel {
            continue;
        }

        // Random chance to stay in place
        if rng.gen_range(0..100) >= wander_chance_percent {
            continue;
        }

        // Skip mobiles not in a room (use current_mobile for fresh room data)
        let mobile_room_id = match current_mobile.current_room_id {
            Some(id) => id,
            None => continue,
        };

        // Get current room data
        let current_room = match db.get_room_data(&mobile_room_id)? {
            Some(r) => r,
            None => continue,
        };

        // Build list of valid exits (cant_swim mobiles also avoid shallow water)
        let valid_exits = get_valid_wander_exits_with_flags(db, &current_room, current_mobile.flags.cant_swim)?;

        if valid_exits.is_empty() {
            continue;
        }

        // Randomly select an exit
        let (direction, target_room_id) = match valid_exits.choose(&mut rng) {
            Some(exit) => exit.clone(),
            None => continue,
        };

        // Move the mobile
        debug!(
            "Wander: moving {} ({}) from room {} to room {} ({})",
            current_mobile.name, current_mobile.id, mobile_room_id, target_room_id, direction
        );
        if db.move_mobile_to_room(&current_mobile.id, &target_room_id).is_ok() {
            // Broadcast departure message
            let departure_msg = format!("{} leaves heading {}.\n", current_mobile.name, direction);
            broadcast_to_room_mobiles(connections, &mobile_room_id, &departure_msg);

            // Broadcast arrival message
            let arrival_dir = get_opposite_direction_rust(&direction);
            let arrival_msg = format!("{} arrives from the {}.\n", current_mobile.name, arrival_dir);
            broadcast_to_room_mobiles(connections, &target_room_id, &arrival_msg);

            propagate_mobile_followers(
                connections,
                &current_mobile.id,
                &current_mobile.name,
                &mobile_room_id,
                &direction,
            );

            debug!("Wander: {} move complete", current_mobile.name);
        }
    }

    Ok(())
}

/// Get all valid exits for a mobile to wander through
pub fn get_valid_wander_exits(db: &db::Db, room: &RoomData) -> Result<Vec<(String, uuid::Uuid)>> {
    get_valid_wander_exits_with_flags(db, room, false)
}

/// Get valid wander exits, with cant_swim flag blocking shallow water too
pub fn get_valid_wander_exits_with_flags(
    db: &db::Db,
    room: &RoomData,
    cant_swim: bool,
) -> Result<Vec<(String, uuid::Uuid)>> {
    let mut valid_exits = Vec::new();

    // Direction names and their corresponding exit Option<Uuid>
    let directions = [
        ("north", room.exits.north),
        ("south", room.exits.south),
        ("east", room.exits.east),
        ("west", room.exits.west),
        ("up", room.exits.up),
        ("down", room.exits.down),
    ];

    for (dir_name, exit_opt) in directions {
        if let Some(target_id) = exit_opt {
            // Check for closed door
            if let Some(door) = room.doors.get(dir_name) {
                if door.is_closed {
                    continue; // Cannot pass through closed door
                }
            }

            // Check target room's no_mob flag and water flags
            if let Ok(Some(target_room)) = db.get_room_data(&target_id) {
                if target_room.flags.no_mob {
                    continue; // Cannot enter no_mob rooms
                }
                if target_room.flags.deep_water || target_room.flags.underwater {
                    continue; // Non-aquatic mobiles cannot wander into water
                }
                // cant_swim mobiles also can't enter shallow water
                if cant_swim && target_room.flags.shallow_water {
                    continue;
                }

                valid_exits.push((dir_name.to_string(), target_id));
            }
        }
    }

    Ok(valid_exits)
}

/// Get the opposite direction for arrival messages
pub fn get_opposite_direction_rust(direction: &str) -> &'static str {
    match direction {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        "west" => "east",
        "up" => "below",
        "down" => "above",
        _ => "somewhere",
    }
}

/// Find the first player in a room
pub fn find_player_name_in_room(connections: &SharedConnections, room_id: &uuid::Uuid) -> Option<String> {
    debug!("find_player_name_in_room: acquiring connections lock");
    if let Ok(conns) = connections.lock() {
        debug!("find_player_name_in_room: lock acquired");
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id {
                    debug!("find_player_name_in_room: found player {}", char.name);
                    return Some(char.name.clone());
                }
            }
        }
        debug!("find_player_name_in_room: no player found in room");
    } else {
        debug!("find_player_name_in_room: failed to acquire lock");
    }
    None
}

/// Handle guard response when a thief mob is caught stealing.
/// Same-room guards shout and alert. Adjacent-room guards rush in.
fn handle_guard_response(
    db: &db::Db,
    connections: &SharedConnections,
    thief_mobile: &MobileData,
    room_id: &uuid::Uuid,
    _victim_name: &str,
    _rng: &mut impl rand::Rng,
) {
    // Check same-room guards
    if let Ok(mobiles) = db.get_mobiles_in_room(room_id) {
        for guard in &mobiles {
            if guard.flags.guard && guard.id != thief_mobile.id && !guard.combat.in_combat && guard.current_hp > 0 {
                broadcast_to_room_awake(connections, room_id, &format!("{} shouts: Stop, thief!", guard.name));
            }
        }
    }

    // Check adjacent room guards
    if let Ok(Some(room)) = db.get_room_data(room_id) {
        let directions = [
            ("north", room.exits.north),
            ("south", room.exits.south),
            ("east", room.exits.east),
            ("west", room.exits.west),
            ("up", room.exits.up),
            ("down", room.exits.down),
        ];

        for (dir_name, exit_opt) in directions {
            if let Some(adj_room_id) = exit_opt {
                if let Ok(adj_mobiles) = db.get_mobiles_in_room(&adj_room_id) {
                    for guard in adj_mobiles {
                        if guard.flags.guard && !guard.combat.in_combat && guard.current_hp > 0 {
                            // Move guard to the incident room
                            if db.move_mobile_to_room(&guard.id, room_id).is_ok() {
                                let opposite = get_opposite_direction_rust(dir_name);
                                broadcast_to_room_awake(
                                    connections,
                                    &adj_room_id,
                                    &format!("{} rushes {}!", guard.name, dir_name),
                                );
                                broadcast_to_room_awake(
                                    connections,
                                    room_id,
                                    &format!("{} arrives from the {}, looking for trouble!", guard.name, opposite),
                                );
                                broadcast_to_room_awake(
                                    connections,
                                    room_id,
                                    &format!("{} shouts: Stop, thief!", guard.name),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Notify and move along any players who were following a mobile that just moved.
///
/// Sends "You follow <name> <direction>." to each player follower in the source
/// room, then injects the direction as a command on their input channel so the
/// normal go.rhai pipeline handles the move (triggers, doors, stamina, leader
/// chains, etc.).
pub fn propagate_mobile_followers(
    connections: &SharedConnections,
    mobile_id: &uuid::Uuid,
    mobile_name: &str,
    source_room: &uuid::Uuid,
    direction: &str,
) {
    let Ok(conns) = connections.lock() else {
        return;
    };
    for session in conns.values() {
        let Some(ref char) = session.character else {
            continue;
        };
        if char.following_mobile_id != Some(*mobile_id) {
            continue;
        }
        if char.current_room_id != *source_room {
            continue;
        }
        let _ = session
            .sender
            .send(format!("You follow {} {}.\r\n", mobile_name, direction));
        let _ = session.input_sender.send(InputEvent::Line(direction.to_string()));
    }
}

/// Find all player names in a room
pub fn find_players_in_room(connections: &SharedConnections, room_id: &uuid::Uuid) -> Vec<String> {
    let mut players = Vec::new();
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id {
                    players.push(char.name.clone());
                }
            }
        }
    }
    players
}

/// Background task that processes mobile periodic effects (poison emotes, etc.)
pub async fn run_mobile_effects_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(MOBILE_EFFECTS_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = process_mobile_effects(&db, &connections) {
            error!("Mobile effects tick error: {}", e);
        }
    }
}

fn process_mobile_effects(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    use rand::Rng;
    let mobiles = db.list_all_mobiles()?;
    let mut rng = rand::thread_rng();

    for mobile in mobiles {
        if mobile.is_prototype || mobile.current_hp <= 0 {
            continue;
        }

        // cant_swim mobiles take drowning damage in water rooms
        if mobile.flags.cant_swim {
            if let Some(room_id) = mobile.current_room_id {
                if let Ok(Some(room)) = db.get_room_data(&room_id) {
                    if room.flags.shallow_water || room.flags.deep_water || room.flags.underwater {
                        let drowning_damage = ((mobile.max_hp * 15) / 100).max(1);
                        broadcast_to_room_awake(
                            connections,
                            &room_id,
                            &format!("\x1b[1;31m{} thrashes helplessly in the water!\x1b[0m", mobile.name),
                        );
                        // Apply damage via CAS so a concurrent heal or
                        // sim-tick update doesn't get reverted.
                        let after = db.update_mobile(&mobile.id, |m| {
                            m.current_hp = (m.current_hp - drowning_damage).max(0);
                        })?;
                        if let Some(mut m) = after {
                            if m.current_hp <= 0 {
                                super::combat::process_mobile_death(db, connections, &mut m, &room_id)?;
                            }
                        }
                        continue;
                    }
                }
            }
        }

        // Poison emotes
        let is_poisoned = mobile.wounds.iter().any(|w| w.wound_type == WoundType::Poisoned);
        if is_poisoned {
            if let Some(room_id) = mobile.current_room_id {
                // ~8% chance per tick
                if rng.gen_range(0..100) < 8 {
                    let msg = match rng.gen_range(0..3) {
                        0 => format!("{} shudders, looking poisoned.", mobile.name),
                        1 => format!("{} looks sickly and pale.", mobile.name),
                        _ => format!("{} sways unsteadily, looking ill.", mobile.name),
                    };
                    broadcast_to_room_awake(connections, &room_id, &msg);
                }
            }
        }

        // Decay mood/social buffs on simulated mobiles.
        if !mobile.active_buffs.is_empty() {
            let tick_secs = MOBILE_EFFECTS_TICK_INTERVAL_SECS as i32;
            let _ = db.update_mobile(&mobile.id, |m| {
                ironmud::social::decay_mobile_buffs(m, tick_secs);
            })?;
        }
    }
    Ok(())
}
