// pdelete.rhai - Delete property templates
// Builder command to delete a property template
// Usage: pdelete <vnum> [confirm]

fn meta() {
    #{
        "access": "builder",
        "description": "Delete a property template and its rooms"
    }
}

fn move_player_to_room(char, connection_id, room_id) {
    char.current_room_id = room_id;
    save_character_data(char);
    set_player_character(connection_id, char);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if !char.is_builder {
        send_client_message(connection_id, "You must be a builder to use this command.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: pdelete <vnum> [confirm]");
        return;
    }

    let parts = args.split(" ");
    let vnum = parts[0].to_lower();
    let confirm = parts.len() > 1 && parts[1].to_lower() == "confirm";

    let template = get_property_template(vnum);
    if template == () {
        send_client_message(connection_id, "Property template '" + vnum + "' not found.");
        return;
    }

    let rooms = get_template_rooms(template.id);
    let active = count_template_instances(template.vnum);

    if !confirm {
        let output = "\n=== Delete Property Template ===\n\n";
        output += "Template: " + template.vnum + " (" + template.name + ")\n";
        output += "Rooms: " + rooms.len() + "\n";
        output += "\nWARNING: This will delete the template and all its rooms.\n";
        if active > 0 {
            output += "Active leases: " + active + " (will NOT be affected - they have copies)\n";
        }
        output += "\nType 'pdelete " + vnum + " confirm' to proceed.\n";
        send_client_message(connection_id, output);
        return;
    }

    // Check if builder is currently in one of the template rooms
    let current_room = get_room_data(char.current_room_id);
    if current_room != () && current_room.property_template_id == template.id {
        // Move builder to town square first
        let starting_room = get_room_by_vnum("town:square");
        if starting_room != () {
            move_player_to_room(char, connection_id, starting_room.id);
        }
    }

    // Delete the template (this also deletes all template rooms via Rust function)
    if delete_property_template(template.id) {
        send_client_message(connection_id, "\nProperty template '" + vnum + "' and its " + rooms.len() + " room(s) have been deleted.\n");
    } else {
        send_client_message(connection_id, "Error: Failed to delete property template.");
    }
}
