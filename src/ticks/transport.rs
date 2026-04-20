//! Transport tick system for IronMUD
//!
//! Handles movement of transports (elevators, buses, trains, ferries, airships).

use anyhow::Result;
use tokio::time::{Duration, interval};
use tracing::error;

use ironmud::{
    NPCTravelSchedule, SharedConnections, TransportData, TransportSchedule, TransportState, TransportType, db,
};

use super::broadcast::{broadcast_to_room, broadcast_to_room_awake};

/// Transport tick interval in seconds (check frequently for responsive elevators)
pub const TRANSPORT_TICK_INTERVAL_SECS: u64 = 1;

/// Background task that processes transport movement (elevators, buses, trains)
pub async fn run_transport_tick(db: db::Db, connections: SharedConnections) {
    let mut ticker = interval(Duration::from_secs(TRANSPORT_TICK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        if let Err(e) = process_transport_tick(&db, &connections) {
            error!("Transport tick error: {}", e);
        }
    }
}

/// Get type-specific arrival messages for a transport
fn get_transport_arrival_messages(transport_type: &TransportType, name: &str, stop_name: &str) -> (String, String) {
    match transport_type {
        TransportType::Elevator => (
            format!("*Ding!* The {} stops. The doors slide open to {}.\n", name, stop_name),
            format!("The {} arrives and the doors slide open.\n", name),
        ),
        TransportType::Bus => (
            format!("The {} slows to a stop at {}. The doors hiss open.\n", name, stop_name),
            format!("The {} pulls up to the stop with a squeal of brakes.\n", name),
        ),
        TransportType::Train => (
            format!(
                "The {} slows with a screech of brakes. \"{}!\" calls the conductor.\n",
                name, stop_name
            ),
            format!("The {} rumbles into the station, coming to a stop.\n", name),
        ),
        TransportType::Ferry => (
            format!("The {} docks at {}. The gangway is lowered.\n", name, stop_name),
            format!("The {} approaches the dock, its horn sounding.\n", name),
        ),
        TransportType::Airship => (
            format!(
                "The {} descends gently to {}. The boarding ramp extends.\n",
                name, stop_name
            ),
            format!("The {} descends from the clouds, mooring at the tower.\n", name),
        ),
    }
}

/// Get type-specific departure messages for a transport
fn get_transport_departure_messages(transport_type: &TransportType, name: &str, next_stop: &str) -> (String, String) {
    match transport_type {
        TransportType::Elevator => (
            format!("The doors slide closed. Next stop: {}.\n", next_stop),
            format!("The {} doors slide closed.\n", name),
        ),
        TransportType::Bus => (
            format!("The doors close with a hiss. Next stop: {}.\n", next_stop),
            format!("The {} pulls away from the curb.\n", name),
        ),
        TransportType::Train => (
            format!(
                "\"All aboard!\" The doors close and the {} lurches forward. Next stop: {}.\n",
                name, next_stop
            ),
            format!("With a blast of its whistle, the {} departs.\n", name),
        ),
        TransportType::Ferry => (
            format!(
                "The gangway is raised. The {} casts off, heading for {}.\n",
                name, next_stop
            ),
            format!("The {} sounds its horn and pulls away from the dock.\n", name),
        ),
        TransportType::Airship => (
            format!(
                "The boarding ramp retracts. The {} rises smoothly toward {}.\n",
                name, next_stop
            ),
            format!("The {} releases its moorings and ascends into the sky.\n", name),
        ),
    }
}

/// Process NPC boarding and disembarking when transport arrives at a stop
fn process_npc_transport_at_stop(
    db: &db::Db,
    connections: &SharedConnections,
    transport: &TransportData,
    stop_index: usize,
    game_hour: u8,
) {
    let stop = &transport.stops[stop_index];
    let stop_room_id = stop.room_id;
    let interior_room_id = transport.interior_room_id;

    // Get all mobiles with transport routes for this transport
    let mobiles = match db.list_all_mobiles() {
        Ok(m) => m,
        Err(_) => return,
    };

    for mut mobile in mobiles {
        if let Some(ref mut route) = mobile.transport_route {
            if route.transport_id != transport.id {
                continue;
            }

            // Handle Permanent schedule (conductors) - always on transport
            if matches!(route.schedule, NPCTravelSchedule::Permanent) {
                if !route.is_on_transport {
                    // Put them on the transport
                    mobile.current_room_id = Some(interior_room_id);
                    route.is_on_transport = true;
                    let _ = db.save_mobile_data(mobile);
                }
                continue;
            }

            // Check if NPC should disembark at this stop
            if route.is_on_transport {
                let should_disembark = if route.is_at_destination {
                    // Going home - disembark at home stop
                    stop_index == route.home_stop_index
                } else {
                    // Going to destination - disembark at destination stop
                    stop_index == route.destination_stop_index
                };

                if should_disembark {
                    // Move NPC to stop room
                    mobile.current_room_id = Some(stop_room_id);
                    route.is_on_transport = false;
                    route.is_at_destination = !route.is_at_destination; // Toggle location
                    let _ = db.save_mobile_data(mobile.clone());

                    // Announce arrival
                    let msg = format!("{} steps off the {}.\n", mobile.name, transport.name);
                    broadcast_to_room_awake(connections, &stop_room_id, &msg);
                    continue;
                }
            }

            // Check if NPC should board at this stop
            if !route.is_on_transport {
                let at_boarding_stop = if route.is_at_destination {
                    // At destination, check if should return home
                    stop_index == route.destination_stop_index && mobile.current_room_id == Some(stop_room_id)
                } else {
                    // At home, check if should go to destination
                    stop_index == route.home_stop_index && mobile.current_room_id == Some(stop_room_id)
                };

                if at_boarding_stop {
                    let should_board = match &route.schedule {
                        NPCTravelSchedule::FixedHours {
                            depart_hour,
                            return_hour,
                        } => {
                            if route.is_at_destination {
                                // At destination, board to return home
                                game_hour >= *return_hour || game_hour < *depart_hour
                            } else {
                                // At home, board to go to destination
                                game_hour >= *depart_hour && game_hour < *return_hour
                            }
                        }
                        NPCTravelSchedule::Random { chance_per_hour } => {
                            // Roll for chance
                            use rand::Rng;
                            rand::thread_rng().gen_range(1..=100) <= *chance_per_hour
                        }
                        NPCTravelSchedule::Permanent => false, // Handled above
                    };

                    if should_board {
                        // Move NPC to transport interior
                        mobile.current_room_id = Some(interior_room_id);
                        route.is_on_transport = true;
                        let _ = db.save_mobile_data(mobile.clone());

                        // Announce boarding
                        let msg = format!("{} boards the {}.\n", mobile.name, transport.name);
                        broadcast_to_room_awake(connections, &stop_room_id, &msg);
                    }
                }
            }
        }
    }
}

/// Process all transports, completing travel for those in motion
fn process_transport_tick(db: &db::Db, connections: &SharedConnections) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let transports = db.list_all_transports()?;

    for mut transport in transports {
        match transport.state {
            TransportState::Moving => {
                // Check if travel time has elapsed
                let travel_complete_time = transport.last_state_change + transport.travel_time_secs;
                if now >= travel_complete_time {
                    // Travel complete - arrive at destination
                    let stop = &transport.stops[transport.current_stop_index];
                    let stop_name = stop.name.clone();
                    let stop_room_id = stop.room_id;
                    let exit_direction = stop.exit_direction.clone();

                    // Create exits (connect transport to stop)
                    // Exit from stop room to vehicle interior
                    db.set_room_exit(&stop_room_id, &exit_direction, &transport.interior_room_id)?;
                    // Exit from vehicle interior to stop room (use opposite direction for cardinal, "out" for custom)
                    let interior_exit = ironmud::types::get_opposite_direction(&exit_direction).unwrap_or("out");
                    db.set_room_exit(&transport.interior_room_id, interior_exit, &stop_room_id)?;

                    // Update transport state
                    transport.state = TransportState::Stopped;
                    transport.last_state_change = now;
                    db.save_transport(&transport)?;

                    // Set dynamic description on stop room
                    let dynamic_desc = format!("The {} is here, doors open.", transport.name);
                    if let Ok(Some(mut room)) = db.get_room_data(&stop_room_id) {
                        room.dynamic_desc = Some(dynamic_desc);
                        let _ = db.save_room_data(room);
                    }

                    // Type-specific arrival messages
                    let (inside_msg, outside_msg) =
                        get_transport_arrival_messages(&transport.transport_type, &transport.name, &stop_name);
                    broadcast_to_room(connections, &transport.interior_room_id, &inside_msg);
                    broadcast_to_room_awake(connections, &stop_room_id, &outside_msg);

                    // Process NPC boarding/disembarking
                    let game_hour = db.get_game_time().map(|t| t.hour).unwrap_or(12);
                    process_npc_transport_at_stop(db, connections, &transport, transport.current_stop_index, game_hour);
                }
            }
            TransportState::Stopped => {
                // For scheduled transports, check if it's time to depart
                if let TransportSchedule::GameTime { dwell_time_secs, .. } = transport.schedule {
                    let depart_time = transport.last_state_change + dwell_time_secs;
                    if now >= depart_time {
                        // Check operating hours
                        let game_time = db.get_game_time()?;
                        let hour = game_time.hour;
                        if transport.is_within_operating_hours(hour) {
                            // Time to depart
                            let current_stop = &transport.stops[transport.current_stop_index];
                            let current_stop_room_id = current_stop.room_id;
                            let current_exit_dir = current_stop.exit_direction.clone();

                            // Remove exits (disconnect transport from stop)
                            db.clear_room_exit(&current_stop_room_id, &current_exit_dir)?;
                            let interior_exit =
                                ironmud::types::get_opposite_direction(&current_exit_dir).unwrap_or("out");
                            db.clear_room_exit(&transport.interior_room_id, interior_exit)?;

                            // Clear dynamic description from stop room
                            if let Ok(Some(mut room)) = db.get_room_data(&current_stop_room_id) {
                                room.dynamic_desc = None;
                                let _ = db.save_room_data(room);
                            }

                            // Advance to next stop
                            transport.advance_to_next_stop();
                            let next_stop_name = transport.stops[transport.current_stop_index].name.clone();

                            // Update state
                            transport.state = TransportState::Moving;
                            transport.last_state_change = now;
                            db.save_transport(&transport)?;

                            // Type-specific departure messages
                            let (inside_msg, outside_msg) = get_transport_departure_messages(
                                &transport.transport_type,
                                &transport.name,
                                &next_stop_name,
                            );
                            broadcast_to_room(connections, &transport.interior_room_id, &inside_msg);
                            broadcast_to_room_awake(connections, &current_stop_room_id, &outside_msg);
                        }
                    }
                }
                // On-demand transports stay stopped until called
            }
        }
    }

    Ok(())
}
