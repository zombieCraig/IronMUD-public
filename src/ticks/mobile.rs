//! Mobile tick systems for IronMUD
//!
//! Handles mobile wandering, aggressive behavior, and periodic effects like poison emotes.

use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use tokio::time::{Duration, interval};
use tracing::{debug, error, warn};

use ironmud::{
    CharacterPosition, CombatDistance, CombatTarget, CombatTargetType, CombatZoneType, EffectType, InputEvent,
    ItemData, MobileData, RoomData, SharedConnections, SharedState, WoundType, broadcast_to_builders, db,
    get_opposite_direction,
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
pub async fn run_wander_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(WANDER_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("wander");

        if let Err(e) = process_wander_tick(&db, &connections, &state) {
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
    mobile_key_vnums: &HashSet<String>,
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
                        // Check if mobile has the key (matched by prototype vnum)
                        if let Some(key_vnum) = door.key_vnum.as_ref() {
                            if mobile_key_vnums.contains(key_vnum) {
                                door_action = Some(DoorAction::UnlockAndOpen);
                            } else {
                                continue; // Locked and no key
                            }
                        } else {
                            continue; // Locked with no key vnum defined
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

    // Collect mobile's key vnums once for door checks
    let mobile_key_vnums: HashSet<String> = if mobile.flags.can_open_doors {
        db.get_items_in_mobile_inventory(&mobile.id)
            .unwrap_or_default()
            .iter()
            .filter_map(|item| item.vnum.clone())
            .collect()
    } else {
        HashSet::new()
    };

    // BFS queue: (current_room, first_step_direction, first_step_room)
    let mut queue: VecDeque<(uuid::Uuid, String, uuid::Uuid)> = VecDeque::new();
    let mut visited: HashSet<uuid::Uuid> = HashSet::new();
    visited.insert(from);

    // MOB_STAY_ZONE clamps BFS to the mobile's home area. Closure stays
    // local to BFS so the existing get_routine_exits signature is unchanged.
    let stay_zone_home: Option<uuid::Uuid> = if mobile.flags.stay_zone {
        mobile.home_area_id
    } else {
        None
    };
    let in_zone = |target_id: &uuid::Uuid| -> bool {
        match stay_zone_home {
            None => true,
            Some(home) => db
                .get_room_data(target_id)
                .ok()
                .flatten()
                .and_then(|r| r.area_id)
                .map_or(false, |aid| aid == home),
        }
    };

    // Seed with exits from starting room
    if let Ok(Some(start_room)) = db.get_room_data(&from) {
        if let Ok(exits) = get_routine_exits(
            db,
            &start_room,
            &mobile_key_vnums,
            mobile.flags.can_open_doors,
            mobile.flags.cant_swim,
        ) {
            for (dir, target_id, _) in exits {
                if !visited.contains(&target_id) && in_zone(&target_id) {
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
                &mobile_key_vnums,
                mobile.flags.can_open_doors,
                mobile.flags.cant_swim,
            ) {
                for (_, target_id, _) in exits {
                    if !visited.contains(&target_id) && in_zone(&target_id) {
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

    // Update the other side of the door.
    // Re-fetch the room (a previous save just touched it); if it was deleted
    // out from under us between the save and now, skip the opposite-side
    // sync rather than panicking and taking the whole tick task down.
    let refreshed = match db.get_room_data(room_id)? {
        Some(r) => r,
        None => return Ok(true),
    };
    if let Some(exit_target) = get_exit_target_for_direction(&refreshed, direction) {
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

    // Update the other side. Re-fetch the departure room; if it vanished
    // between our save and this read, skip the opposite-side close rather
    // than panicking and killing the tick.
    let refreshed = match db.get_room_data(departure_room_id)? {
        Some(r) => r,
        None => return Ok(()),
    };
    if let Some(exit_target) = get_exit_target_for_direction(&refreshed, direction) {
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
    // Charmed mobs stay put; movement happens via master-follow propagation.
    if mobile.is_charmed_by_anyone() {
        return true;
    }

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
fn process_wander_tick(db: &db::Db, connections: &SharedConnections, state: &SharedState) -> Result<()> {
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
        // BUT: sentinel mobiles with a routine destination should still process step movement,
        // and a terrified sentinel abandons its post (panic-wander below).
        if mobile.flags.sentinel
            && mobile.routine_destination_room.is_none()
            && !ironmud::script::fear::is_feared(&mobile.active_buffs)
        {
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
                    // Check for and handle door in this direction. If the mob's
                    // current room got deleted out from under us, clear the
                    // routine destination and move on rather than panicking
                    // (which would kill the entire tick task).
                    let room = match db.get_room_data(&current_room_id)? {
                        Some(r) => r,
                        None => {
                            db.update_mobile(&current_mobile.id, |m| {
                                m.routine_destination_room = None;
                            })?;
                            continue;
                        }
                    };
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

                            // Reload room data after door changes. Same
                            // missing-room hardening as above.
                            let updated_room = match db.get_room_data(&current_room_id)? {
                                Some(r) => r,
                                None => {
                                    db.update_mobile(&current_mobile.id, |m| {
                                        m.routine_destination_room = None;
                                    })?;
                                    continue;
                                }
                            };
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

        // Check for aggressive / memory-driven attack BEFORE wandering.
        // The gate here and the early return inside
        // `find_aggression_target_for_mob` must agree on who is even a
        // candidate, so both ask the same helper. They did not before: this
        // site listed only aggressive / rage / memory, which meant a mob whose
        // sole reason to attack was the alignment flags never reached the scan
        // that reads them.
        if may_aggress(&current_mobile) {
            if let Some(room_id) = current_mobile.current_room_id {
                // Check if room allows combat (not a safe zone)
                if let Ok(Some(room)) = db.get_room_data(&room_id) {
                    let is_safe = room.flags.combat_zone == Some(CombatZoneType::Safe);

                    if !is_safe {
                        if let Some((player_name, was_remembered)) =
                            find_aggression_target_for_mob(connections, state, &current_mobile, &room_id)
                        {
                            debug!(
                                "Aggression: {} targeting player {} in room {} (remembered={})",
                                current_mobile.name, player_name, room_id, was_remembered
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
                                            target_name: None,
                                        });
                                    }
                                    m.combat.distances.insert(player_target_id, CombatDistance::Ranged);
                                });

                                // Put the player in combat with this mobile
                                char.combat.in_combat = true;
                                if !char.combat.targets.iter().any(|t| t.target_id == current_mobile.id) {
                                    char.combat.targets.push(CombatTarget::mobile(current_mobile.id));
                                }
                                // Player also at ranged distance from mob
                                char.combat.distances.insert(current_mobile.id, CombatDistance::Ranged);
                                let _ = db.save_character_data(char.clone());
                                sync_character_to_session(connections, &char, state);

                                // Notify the room (sleeping players don't see this).
                                // Memory-driven attacks get a recognition emote.
                                let attack_msg = if was_remembered {
                                    format!(
                                        "{} snarls, '{}! I remember you!' and attacks!",
                                        current_mobile.name, player_name
                                    )
                                } else {
                                    format!("{} snarls and attacks {}!", current_mobile.name, player_name)
                                };
                                broadcast_to_room_awake(connections, &room_id, &attack_msg);

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
                                        sync_character_to_session(connections, &char, state);

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
                                                        target_name: None,
                                                    });
                                                }
                                                m.combat.distances.insert(player_target_id, CombatDistance::Melee);
                                            });

                                            char.combat.in_combat = true;
                                            if !char.combat.targets.iter().any(|t| t.target_id == current_mobile.id) {
                                                char.combat.targets.push(CombatTarget::mobile(current_mobile.id));
                                            }
                                            char.combat.distances.insert(current_mobile.id, CombatDistance::Melee);
                                            let _ = db.save_character_data(char.clone());
                                            sync_character_to_session(connections, &char, state);

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

        // A terrified mobile panic-wanders: it overrides sentinel posts,
        // routine suppression, and the stay-put roll, moving every wander
        // tick until the Feared buff expires.
        let feared = ironmud::script::fear::is_feared(&current_mobile.active_buffs);

        // Check if routine suppresses wandering
        if should_suppress_wander(&current_mobile) && !feared {
            continue;
        }

        // Sentinel mobiles should not randomly wander (only routine movement above)
        if current_mobile.flags.sentinel && !feared {
            continue;
        }

        // Random chance to stay in place
        if !feared && rng.gen_range(0..100) >= wander_chance_percent {
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
        let mut valid_exits = filter_exits_by_stay_zone(db, &current_mobile, valid_exits);

        // Vampires shelter indoors during the day. Filter outdoor exits while
        // the sun is up; if no indoor exits remain the mob simply stays put
        // (and the existing sun tick handles damage at room scope).
        if current_mobile.flags.vampire {
            if let Ok(time) = db.get_game_time() {
                if time.is_daytime() {
                    valid_exits.retain(|(_, target_room_id)| {
                        db.get_room_data(target_room_id)
                            .ok()
                            .flatten()
                            .map(|r| r.flags.indoors)
                            .unwrap_or(false)
                    });
                }
            }
        }

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
            let departure_msg = if feared {
                format!("{} bolts {} in a blind panic!\n", current_mobile.name, direction)
            } else {
                format!("{} leaves heading {}.\n", current_mobile.name, direction)
            };
            broadcast_to_room_mobiles(connections, &mobile_room_id, &departure_msg);

            // Broadcast arrival message
            let arrival_dir = get_opposite_direction_rust(&direction);
            let arrival_msg = if feared {
                format!("{} arrives from the {}, panicked!\n", current_mobile.name, arrival_dir)
            } else {
                format!("{} arrives from the {}.\n", current_mobile.name, arrival_dir)
            };
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

/// Filter exit candidates to those staying inside the mobile's home area
/// when `flags.stay_zone` is set. Mobiles without `stay_zone` or a home
/// area pass through unchanged. Used by wander, pursuit, and routine BFS
/// to keep MOB_STAY_ZONE bound to its zone.
pub fn filter_exits_by_stay_zone(
    db: &db::Db,
    mobile: &MobileData,
    exits: Vec<(String, uuid::Uuid)>,
) -> Vec<(String, uuid::Uuid)> {
    if !mobile.flags.stay_zone {
        return exits;
    }
    let home = match mobile.home_area_id {
        Some(id) => id,
        None => return exits,
    };
    exits
        .into_iter()
        .filter(|(_, target_id)| {
            db.get_room_data(target_id)
                .ok()
                .flatten()
                .and_then(|r| r.area_id)
                .map_or(false, |aid| aid == home)
        })
        .collect()
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
        let _ = session.input_sender.try_send(InputEvent::Line(direction.to_string()));
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

/// [`find_players_in_room`], but carrying each player's character rather than
/// just their name.
///
/// The session copy is authoritative for anyone online — every write path in
/// the game syncs it — so a caller that needs to *read* several fields off
/// each player in a room should take them from here rather than paying a
/// `get_character_data` per player. That per-player read is the difference
/// between a free scan and a per-mob, per-player database round trip on a
/// tick that walks every mobile in the world.
pub fn find_players_in_room_with_data(
    connections: &SharedConnections,
    room_id: &uuid::Uuid,
) -> Vec<ironmud::CharacterData> {
    let mut players = Vec::new();
    if let Ok(conns) = connections.lock() {
        for (_, session) in conns.iter() {
            if let Some(ref char) = session.character {
                if char.current_room_id == *room_id {
                    players.push(char.clone());
                }
            }
        }
    }
    players
}

/// Has this mob any reason at all to attack somebody on sight?
///
/// The cheap pre-filter both the wander tick and
/// [`find_aggression_target_for_mob`] gate on, so the two cannot disagree
/// about who is a candidate. Everything expensive — loading each player in the
/// room, visibility, standing — happens only after this returns true.
pub fn may_aggress(mob: &MobileData) -> bool {
    mob.flags.aggressive
        || mob.active_buffs.iter().any(|b| b.effect_type == EffectType::Rage)
        || (mob.flags.memory && !mob.remembered_enemies.is_empty())
        || mob.flags.aggro_good
        || mob.flags.aggro_evil
        || mob.flags.aggro_neutral
        // A faction-tagged mob has a standing to check even with no
        // aggression flag set. Untagged mobs — most of the world's wildlife —
        // stop here, which is what keeps this off the hot path.
        || ironmud::reputation::normalize(mob.faction.as_deref()).is_some()
}

/// Returns a candidate player for a mob's aggression / memory / alignment /
/// faction logic, if any. Honours MOB_AWARE (visibility) and MOB_MEMORY
/// (remembered enemies). The boolean in the tuple is `true` when the candidate
/// matched the memory list — used to gate the "I remember you!" emote.
pub fn find_aggression_target_for_mob(
    connections: &SharedConnections,
    state: &SharedState,
    mob: &MobileData,
    room_id: &uuid::Uuid,
) -> Option<(String, bool)> {
    if !may_aggress(mob) {
        return None;
    }
    // A Rage buff overrides temperament: the mob attacks anyone it can see,
    // including its own charm master.
    let raging = mob.active_buffs.iter().any(|b| b.effect_type == EffectType::Rage);
    let aggressive = mob.flags.aggressive || raging;
    // Resolved once, not per player: this is the only World-lock read on the
    // path, and it must not be taken while the connections lock is held.
    let faction_def =
        ironmud::reputation::normalize(mob.faction.as_deref()).map(|f| ironmud::reputation::definition(state, &f));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let remembered: HashSet<String> = mob
        .remembered_enemies
        .iter()
        .filter(|e| e.expires_at_secs > now)
        .map(|e| e.name.to_lowercase())
        .collect();

    let charm_master = mob.charm_master().map(|s| s.to_lowercase());
    // Off the session, not the database. This used to be one
    // `get_character_data` per player per candidate mob, and `may_aggress`
    // now admits every faction-tagged mob — so a town of tagged guards and a
    // handful of players was hundreds of character deserializations a tick.
    // The session copy is authoritative for anyone online, and combat,
    // morality and reputation all sync it on write, so it is also the
    // *correct* source here rather than merely the cheap one.
    let players = find_players_in_room_with_data(connections, room_id);
    for ch in players {
        let name = ch.name.clone();
        let key = name.to_lowercase();
        // Charmed mobs never aggro their master — unless rage takes them.
        if !raging && charm_master.as_deref() == Some(&key) {
            continue;
        }
        if !ironmud::script::is_player_visible_to_mob(&ch, mob) {
            continue;
        }
        let is_remembered = remembered.contains(&key);
        let alignment_match = (mob.flags.aggro_evil && ch.morality < -24)
            || (mob.flags.aggro_good && ch.morality > 24)
            || (mob.flags.aggro_neutral && ch.morality >= -24 && ch.morality <= 24);
        // Faction hostility: a group whose members you have been killing stops
        // waiting to be provoked. Threshold is per-faction, so a jumpy militia
        // and a patient guild can share one mechanism.
        let faction_match = faction_def
            .as_ref()
            .map(|d| ironmud::reputation::is_hostile_at(d, ironmud::reputation::standing(&ch.reputation, &d.key)))
            .unwrap_or(false);
        if aggressive || is_remembered || alignment_match || faction_match {
            return Some((name, is_remembered));
        }
    }
    None
}

/// Background task that processes mobile periodic effects (poison emotes, etc.)
pub async fn run_mobile_effects_tick(db: db::Db, connections: SharedConnections, state: SharedState) {
    let mut ticker = interval(Duration::from_secs(MOBILE_EFFECTS_TICK_INTERVAL_SECS));
    loop {
        ticker.tick().await;
        crate::ticks::heartbeat::beat("mobile_effects");
        if let Err(e) = process_mobile_effects(&db, &connections, &state) {
            error!("Mobile effects tick error: {}", e);
        }
    }
}

fn process_mobile_effects(db: &db::Db, connections: &SharedConnections, _state: &SharedState) -> Result<()> {
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

        // Passive stance HP regen (mirrors PC formula in
        // src/ticks/character.rs::process_regen_tick). Standing=0, Sitting=1,
        // Sleeping=2 HP per MOBILE_EFFECTS_TICK_INTERVAL_SECS. Composes
        // additively with the Regeneration buff below.
        if mobile.position != ironmud::types::MobilePosition::Standing && mobile.current_hp < mobile.max_hp {
            let _ = db.update_mobile(&mobile.id, |m| {
                ironmud::script::apply_mobile_passive_stance_regen(m);
            })?;
        }

        // Decay mood/social buffs on simulated mobiles. Also tick
        // Regeneration buffs (magnitude HP per effects-tick interval, capped
        // at max_hp).
        if !mobile.active_buffs.is_empty() {
            let tick_secs = MOBILE_EFFECTS_TICK_INTERVAL_SECS as i32;
            let _ = db.update_mobile(&mobile.id, |m| {
                if let Some(regen) = m
                    .active_buffs
                    .iter()
                    .find(|b| b.effect_type == EffectType::Regeneration)
                {
                    let amt = regen.magnitude;
                    if m.current_hp < m.max_hp && amt > 0 {
                        m.current_hp = (m.current_hp + amt).min(m.max_hp);
                    }
                }
                ironmud::social::decay_mobile_buffs(m, tick_secs);
            })?;
        }

        // Prune expired MOB_MEMORY entries so dead-name lists don't pile up
        // on long-lived mobs that are no longer being attacked.
        if !mobile.remembered_enemies.is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if mobile.remembered_enemies.iter().any(|e| e.expires_at_secs <= now) {
                let _ = db.update_mobile(&mobile.id, |m| {
                    m.remembered_enemies.retain(|e| e.expires_at_secs > now);
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironmud::reputation;

    fn temp_db() -> (db::Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = db::Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn mob(faction: Option<&str>) -> MobileData {
        let mut m = MobileData::new("a guardsman".into());
        m.faction = faction.map(|s| s.to_string());
        m
    }

    /// A player standing in `room`, online, with the given faction standing.
    fn player_in(db: &db::Db, room: uuid::Uuid, name: &str, standing: Option<(&str, i32)>) -> SharedConnections {
        let mut ch: ironmud::CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        if let Some((f, v)) = standing {
            ch.reputation.insert(f.to_string(), v);
        }
        db.save_character_data(ch.clone()).expect("save char");

        let (tx_client, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<InputEvent>(1);
        let mut session = ironmud::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(ch);
        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);
        conns
    }

    fn state_with(db: &db::Db, conns: &SharedConnections, defs: Vec<reputation::FactionDefinition>) -> SharedState {
        let state = ironmud::World::minimal_shared(db.clone(), conns.clone());
        {
            let mut w = state.lock().unwrap();
            for d in defs {
                w.faction_definitions.insert(d.key.clone(), d);
            }
        }
        state
    }

    /// The wander tick used to gate the whole scan on aggressive/rage/memory,
    /// so a mob whose only reason to attack was an alignment flag never
    /// reached the code that reads it. Both sites now ask `may_aggress`.
    #[test]
    fn alignment_flags_alone_make_a_mob_a_candidate() {
        let mut m = mob(None);
        assert!(!may_aggress(&m), "a placid untagged mob is not a candidate");

        m.flags.aggro_evil = true;
        assert!(may_aggress(&m));

        let mut m = mob(None);
        m.flags.aggro_good = true;
        assert!(may_aggress(&m));

        let mut m = mob(None);
        m.flags.aggro_neutral = true;
        assert!(may_aggress(&m));
    }

    #[test]
    fn a_faction_tag_alone_makes_a_mob_a_candidate_but_a_blank_one_does_not() {
        assert!(may_aggress(&mob(Some("iron_guard"))));
        // Blank and whitespace tags are how a mob opts out entirely; they must
        // not drag every wildlife mob onto the expensive path.
        assert!(!may_aggress(&mob(Some(""))));
        assert!(!may_aggress(&mob(Some("   "))));
        assert!(!may_aggress(&mob(None)));
    }

    #[test]
    fn a_faction_attacks_someone_it_has_come_to_hate() {
        let (db, _t) = temp_db();
        let room = uuid::Uuid::new_v4();
        let conns = player_in(&db, room, "rook", Some(("iron_guard", -500)));
        let state = state_with(
            &db,
            &conns,
            vec![reputation::FactionDefinition::unregistered("iron_guard")],
        );

        let found = find_aggression_target_for_mob(&conns, &state, &mob(Some("iron_guard")), &room);
        assert_eq!(found.map(|(n, _)| n), Some("rook".to_string()));
    }

    #[test]
    fn a_faction_ignores_someone_who_has_merely_annoyed_it() {
        let (db, _t) = temp_db();
        let room = uuid::Uuid::new_v4();
        // Disliked, but above the default hostile_at of -200.
        let conns = player_in(&db, room, "rook", Some(("iron_guard", -199)));
        let state = state_with(
            &db,
            &conns,
            vec![reputation::FactionDefinition::unregistered("iron_guard")],
        );

        assert!(find_aggression_target_for_mob(&conns, &state, &mob(Some("iron_guard")), &room).is_none());
    }

    #[test]
    fn each_faction_sets_its_own_patience() {
        let (db, _t) = temp_db();
        let room = uuid::Uuid::new_v4();
        let conns = player_in(&db, room, "rook", Some(("reavers", -60)));
        let jumpy = reputation::FactionDefinition {
            hostile_at: -50,
            ..reputation::FactionDefinition::unregistered("reavers")
        };
        let state = state_with(&db, &conns, vec![jumpy]);

        assert!(
            find_aggression_target_for_mob(&conns, &state, &mob(Some("reavers")), &room).is_some(),
            "a faction can be quicker to take offence than the default"
        );
    }

    /// The scan reads the session, not the database.
    ///
    /// Two reasons, and the test pins both at once by making the two copies
    /// disagree. Correctness: the session copy is authoritative for anyone
    /// online, so a standing that moved this tick is visible there first.
    /// Cost: `may_aggress` admits every faction-tagged mob, so a DB read per
    /// player here was a read per *player per tagged mob* — a town of guards
    /// with a few players in it ran hundreds of character deserializations a
    /// tick, on a loop that already walks every mobile in the world.
    #[test]
    fn the_scan_reads_the_session_copy_rather_than_the_database() {
        let (db, _t) = temp_db();
        let room = uuid::Uuid::new_v4();

        // The stored character is a stranger to the faction...
        let mut stored: ironmud::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "rook",
            "password_hash": "",
            "current_room_id": room,
        }))
        .expect("build character");
        db.save_character_data(stored.clone()).expect("save char");

        // ...while the live session has fallen well past hostile.
        stored.reputation.insert("iron_guard".to_string(), -800);
        let (tx_client, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<InputEvent>(1);
        let mut session = ironmud::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(stored);
        let conns: SharedConnections = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);

        let state = state_with(
            &db,
            &conns,
            vec![reputation::FactionDefinition::unregistered("iron_guard")],
        );

        assert!(
            find_aggression_target_for_mob(&conns, &state, &mob(Some("iron_guard")), &room).is_some(),
            "the session standing is the one that counts"
        );
    }

    #[test]
    fn a_tagged_mob_leaves_a_stranger_alone() {
        let (db, _t) = temp_db();
        let room = uuid::Uuid::new_v4();
        // No standing row at all: a faction you have never dealt with is
        // Neutral, and tagging a mob must not make it aggressive by itself.
        let conns = player_in(&db, room, "rook", None);
        let state = state_with(
            &db,
            &conns,
            vec![reputation::FactionDefinition::unregistered("iron_guard")],
        );

        assert!(find_aggression_target_for_mob(&conns, &state, &mob(Some("iron_guard")), &room).is_none());
    }
}
