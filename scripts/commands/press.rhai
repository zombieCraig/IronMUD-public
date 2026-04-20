// scripts/commands/press.rhai
// press button - Call on-demand transport (elevator) to current location
// press <number> - Select destination floor/stop inside a transport vehicle
// press <name> - Select destination by name inside a transport vehicle

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let room_id = char.current_room_id;

    if args == "" {
        send_client_message(connection_id, "Press what? Try 'press button' or 'press <number>'.");
        return;
    }

    let arg_lower = args.to_lower();

    // Check if player is inside a transport vehicle
    let vehicle_transport = get_transport_by_interior_room(room_id);
    if vehicle_transport != () {
        handle_vehicle_press(char, vehicle_transport, args, arg_lower, connection_id);
        return;
    }

    // Check if player is at a transport stop
    let stop_transport = get_transport_for_stop_room(room_id);
    if stop_transport != () {
        handle_stop_press(char, stop_transport, room_id, arg_lower, connection_id);
        return;
    }

    // No transport found
    send_client_message(connection_id, "There's nothing here to press.");
}

// Handle press command when inside a transport vehicle
fn handle_vehicle_press(char, transport, args, arg_lower, connection_id) {
    // Check if transport is moving
    if transport.state == "moving" {
        send_client_message(connection_id, "The " + transport.name + " is currently in motion. Please wait until it stops.");
        return;
    }

    // For scheduled transports, floor selection doesn't work
    if !is_transport_ondemand(transport.id) {
        send_client_message(connection_id, "This transport operates on a schedule. You'll arrive at the next stop automatically.");
        return;
    }

    // Try to parse as a floor number (1-indexed for users)
    let floor_num = parse_int(args);
    if floor_num > 0 {
        select_floor_by_number(char, transport, floor_num, connection_id);
        return;
    }

    // Try to match by stop name
    select_floor_by_name(char, transport, arg_lower, connection_id);
}

// Select floor by number (1-indexed)
fn select_floor_by_number(char, transport, floor_num, connection_id) {
    let stops = transport.stops;
    let num_stops = stops.len();

    if floor_num < 1 || floor_num > num_stops {
        send_client_message(connection_id, "Invalid floor number. Valid floors: 1 to " + num_stops + ".");
        return;
    }

    let dest_index = floor_num - 1; // Convert to 0-indexed
    initiate_travel(char, transport, dest_index, connection_id);
}

// Select floor by name
fn select_floor_by_name(char, transport, name_lower, connection_id) {
    let stops = transport.stops;

    for i in 0..stops.len() {
        let stop = stops[i];
        if stop.name.to_lower() == name_lower {
            initiate_travel(char, transport, i, connection_id);
            return;
        }
    }

    // Name not found, show available floors with MXP links
    let floor_list = "";
    for i in 0..stops.len() {
        if floor_list != "" { floor_list += ", "; }
        let floor_num = i + 1;
        let floor_link = mxp_send("press " + floor_num, "[" + floor_num + "]");
        let floor_display = mxp_or(floor_link, "[" + floor_num + "]", connection_id);
        floor_list += floor_display + " " + stops[i].name;
    }
    send_client_message(connection_id, "Unknown floor. Available floors: " + floor_list);
}

// Initiate travel to destination
fn initiate_travel(char, transport, dest_index, connection_id) {
    let current_index = transport.current_stop_index;

    if dest_index == current_index {
        let stop_name = transport.stops[dest_index].name;
        send_client_message(connection_id, "You're already at " + stop_name + ".");
        return;
    }

    let dest_stop = transport.stops[dest_index];
    let current_stop = transport.stops[current_index];

    // Calculate travel time based on floor distance
    let travel_time = calculate_elevator_travel_time(current_index, dest_index);

    // Set travel time on transport before starting travel
    set_transport_travel_time(transport.id, travel_time);

    // Start travel (disconnects from current stop, sets destination)
    if !start_transport_travel(transport.id, dest_index) {
        send_client_message(connection_id, "Something went wrong with the " + transport.name + ".");
        return;
    }

    // Determine direction for message
    let direction_msg = "";
    if dest_index > current_index {
        direction_msg = "You feel the " + transport.name + " begin to rise...";
    } else {
        direction_msg = "You feel the " + transport.name + " begin to descend...";
    }

    // Send messages
    send_client_message(connection_id, "The doors slide closed.");
    broadcast_to_room_except(transport.interior_room_id, connection_id, char.name + " presses a button. The doors slide closed.");
    send_client_message(connection_id, direction_msg);
    broadcast_to_room_except(transport.interior_room_id, connection_id, direction_msg);

    // The transport tick will handle arrival after travel_time elapses
}

// Handle press command when at a transport stop
fn handle_stop_press(char, transport, room_id, arg_lower, connection_id) {
    // Only "button" works at stops
    if arg_lower != "button" {
        send_client_message(connection_id, "There's no '" + arg_lower + "' here. Try 'press button' to call the " + transport.name + ".");
        return;
    }

    // For scheduled transports, buttons don't work at stops
    if !is_transport_ondemand(transport.id) {
        send_client_message(connection_id, "There's no button here. The " + transport.name + " operates on a schedule.");
        return;
    }

    // Find which stop index this room is
    let stop_index = get_stop_index_for_room(transport.id, room_id);
    if stop_index < 0 {
        send_client_message(connection_id, "Something is wrong with this stop.");
        return;
    }

    // Check if transport is already here
    if transport.state == "stopped" && transport.current_stop_index == stop_index {
        let stop = transport.stops[stop_index];
        let exit_link = mxp_send(stop.exit_direction, stop.exit_direction);
        let exit_display = mxp_or(exit_link, stop.exit_direction, connection_id);
        send_client_message(connection_id, "The " + transport.name + " is already here. Enter via '" + exit_display + "'.");
        return;
    }

    // Check if transport is moving
    if transport.state == "moving" {
        send_client_message(connection_id, "You press the call button. The " + transport.name + " is on its way...");
        // For now, we don't interrupt ongoing travel - would need more complex logic
        return;
    }

    // Transport is stopped at another floor - call it to this floor
    // Calculate travel time from current location
    let current_index = transport.current_stop_index;
    let travel_time = calculate_elevator_travel_time(current_index, stop_index);

    // Set travel time and start travel to this stop
    set_transport_travel_time(transport.id, travel_time);

    if !start_transport_travel(transport.id, stop_index) {
        send_client_message(connection_id, "The " + transport.name + " doesn't respond.");
        return;
    }

    send_client_message(connection_id, "You press the call button.");
    broadcast_to_room_except(room_id, connection_id, char.name + " presses the call button.");

    // Transport tick will handle arrival and send the "ding" message
}
