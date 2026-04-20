// plist.rhai - List property templates
// Builder command to list all property templates
// Usage: plist

fn meta() {
    #{
        "access": "builder",
        "description": "List all property templates"
    }
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

    let templates = list_property_templates();

    if templates.len() == 0 {
        send_client_message(connection_id, "\n=== Property Templates ===\n\nNo property templates found.\nUse 'pedit create <vnum>' to create one.\n");
        return;
    }

    let output = "\n=== Property Templates ===\n\n";

    // Header
    output += "Vnum             Name                     Rent/mo  Rooms  Active\n";
    output += "---------------  -----------------------  -------  -----  ------\n";

    for template in templates {
        // Get room count
        let rooms = get_template_rooms(template.id);
        let room_count = rooms.len();

        // Get active lease count
        let active_count = count_template_instances(template.vnum);

        // Format each column
        let vnum_str = template.vnum;
        // Pad vnum to 15 chars
        while vnum_str.len() < 15 {
            vnum_str += " ";
        }
        if vnum_str.len() > 15 {
            vnum_str = vnum_str.sub_string(0, 15);
        }

        let name_str = template.name;
        // Pad name to 23 chars
        while name_str.len() < 23 {
            name_str += " ";
        }
        if name_str.len() > 23 {
            name_str = name_str.sub_string(0, 23);
        }

        let rent_str = template.monthly_rent + "g";
        // Pad rent to 7 chars
        while rent_str.len() < 7 {
            rent_str = " " + rent_str;
        }

        let rooms_str = "" + room_count;
        // Pad rooms to 5 chars
        while rooms_str.len() < 5 {
            rooms_str = " " + rooms_str;
        }

        let active_str = "" + active_count;
        // Pad active to 6 chars
        while active_str.len() < 6 {
            active_str = " " + active_str;
        }

        output += vnum_str + "  " + name_str + "  " + rent_str + "  " + rooms_str + "  " + active_str + "\n";
    }

    output += "\nTotal: " + templates.len() + " template(s)\n";
    send_client_message(connection_id, output);
}
