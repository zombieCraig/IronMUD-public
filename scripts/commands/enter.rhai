// scripts/commands/enter.rhai
// enter - Enter your rented property from the leasing office

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.\nYou can only enter your property from a leasing office.");
        return;
    }

    // Get area_id from agent (uses fallback to prototype for spawned instances)
    let area_id = get_agent_leasing_area(agent.id);
    if area_id == "" {
        send_client_message(connection_id, "This leasing agent is not properly configured.");
        return;
    }

    // Get player's lease in this area
    let lease = get_player_lease_in_area(char.name, area_id);
    if lease == () {
        send_client_message(connection_id, "You don't have a property in this area.\nUse 'properties' to see available rentals, or 'rent <property>' to lease one.");
        return;
    }

    // Check if evicted
    if lease.is_evicted {
        send_client_message(connection_id, "Your property was evicted due to non-payment.\nYou'll need to rent a new property.");
        return;
    }

    // Get the entrance room
    let entrance_id = get_property_entrance(lease.id);
    if entrance_id == "" {
        send_client_message(connection_id, "Your property doesn't have a valid entrance. Contact an administrator.");
        return;
    }

    // Move the player
    char.current_room_id = entrance_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    send_client_message(connection_id, "You enter your property...\n");

    // Show the room using the shared display function
    display_room(entrance_id, connection_id, char.name);
}
