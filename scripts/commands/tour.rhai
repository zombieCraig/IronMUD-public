// scripts/commands/tour.rhai
// tour <property> - Preview a property template before renting
// tour end - End the tour and return to the leasing office

fn move_player_to_room(char, connection_id, room_id) {
    char.current_room_id = room_id;
    save_character_data(char);
    set_player_character(connection_id, char);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Handle "tour end"
    if args.to_lower() == "end" {
        if !is_on_tour(connection_id) {
            send_client_message(connection_id, "You are not currently on a property tour.");
            return;
        }

        // Restore to origin room
        let return_room_id = char.tour_origin_room;
        char.on_tour = false;
        char.tour_origin_room = "";
        move_player_to_room(char, connection_id, return_room_id);

        send_client_message(connection_id, "Tour ended. You return to the leasing office.\n");
        run_look_command("", connection_id);
        return;
    }

    // Check if already on tour
    if is_on_tour(connection_id) {
        send_client_message(connection_id, "You are already on a property tour. Use 'tour end' to finish.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: tour <property name>\n       tour end");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here to arrange a tour.");
        return;
    }

    // Find the template by keyword
    let template_vnums = get_agent_templates(agent.id);
    let target_template = ();
    let search_term = args.to_lower();

    for vnum in template_vnums {
        let template = get_property_template(vnum);
        if template == () {
            continue;
        }

        // Match by vnum or name
        if vnum.to_lower() == search_term || template.name.to_lower().contains(search_term) {
            target_template = template;
            break;
        }
    }

    if target_template == () {
        send_client_message(connection_id, "No property matching '" + args + "' is available here.\nUse 'properties' to see what's available.");
        return;
    }

    // Check template has an entrance
    if target_template.entrance_room_id == "" {
        send_client_message(connection_id, "This property template is not ready for tours yet.");
        return;
    }

    // Start the tour - save origin room before moving
    char.tour_origin_room = char.current_room_id;
    char.on_tour = true;
    move_player_to_room(char, connection_id, target_template.entrance_room_id);

    send_client_message(connection_id, "You begin a tour of '" + target_template.name + "'...\n");
    send_client_message(connection_id, "[Tour Mode: You cannot pick up or drop items. Use 'tour end' to return.]\n");
    run_look_command("", connection_id);
}

fn run_look_command(args, connection_id) {
    // Call the look command to show the new room
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
