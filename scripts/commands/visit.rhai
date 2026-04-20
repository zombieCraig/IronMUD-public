// scripts/commands/visit.rhai
// visit <player> - Visit a grouped player's property

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: visit <player name>\nVisit a grouped player's property.");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.\nYou can only visit properties from a leasing office.");
        return;
    }

    // Get area_id from agent (uses fallback to prototype for spawned instances)
    let area_id = get_agent_leasing_area(agent.id);
    if area_id == "" {
        send_client_message(connection_id, "This leasing agent is not properly configured.");
        return;
    }

    // Find the target player
    let target_name = args.trim();

    // Can't visit yourself
    if target_name.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "You can use 'enter' to access your own property.");
        return;
    }

    // Check if target player has a property in this area
    let target_lease = get_player_lease_by_name(target_name, area_id);
    if target_lease == () {
        send_client_message(connection_id, target_name + " does not have a property in this area.");
        return;
    }

    // Check if in same group
    if !are_in_same_group(char.name, target_lease.owner_name) {
        send_client_message(connection_id, "You must be grouped with " + target_lease.owner_name + " to visit their property.");
        return;
    }

    // Check if visitor has permission
    if !can_enter_property(target_lease.id, char.name) {
        send_client_message(connection_id, target_lease.owner_name + " has not granted you access to their property.\nThey can use 'property access visit' or 'property trust " + char.name + "' to allow you in.");
        return;
    }

    // Get the entrance room
    let entrance_id = get_property_entrance(target_lease.id);
    if entrance_id == "" {
        send_client_message(connection_id, "That property doesn't have a valid entrance.");
        return;
    }

    // Move the player
    char.current_room_id = entrance_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    send_client_message(connection_id, "You enter " + target_lease.owner_name + "'s property...\n");

    // Show the room
    run_look_command(connection_id);
}

fn run_look_command(connection_id) {
    let char = get_player_character(connection_id);
    if char == () { return; }

    let room = get_room_data(char.current_room_id);
    if room == () { return; }

    // Build basic room display
    let output = room.title + "\n";
    output += room.description + "\n";

    // Build exits
    let exits = [];
    if room.exits.north != () { exits.push("north"); }
    if room.exits.south != () { exits.push("south"); }
    if room.exits.east != () { exits.push("east"); }
    if room.exits.west != () { exits.push("west"); }
    if room.exits.up != () { exits.push("up"); }
    if room.exits.down != () { exits.push("down"); }
    if room.exits.out != () { exits.push("out"); }

    if exits.len() > 0 {
        let exit_str = "";
        for e in exits {
            if exit_str != "" { exit_str += " "; }
            exit_str += e;
        }
        output += "[Exits: " + exit_str + "]";
    } else {
        output += "[Exits: none]";
    }

    send_client_message(connection_id, output);
}
