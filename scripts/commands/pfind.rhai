// pfind.rhai - Search property templates
// Builder command to search property templates by vnum or name
// Usage: pfind <keyword>

fn meta() {
    #{
        "access": "builder",
        "description": "Search property templates by vnum or name"
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

    if args == "" {
        send_client_message(connection_id, "Usage: pfind <keyword>");
        return;
    }

    let keyword = args.to_lower();
    let templates = list_property_templates();
    let matches = [];

    for template in templates {
        let vnum_match = template.vnum.to_lower().contains(keyword);
        let name_match = template.name.to_lower().contains(keyword);

        if vnum_match || name_match {
            matches.push(template);
        }
    }

    if matches.len() == 0 {
        send_client_message(connection_id, "\nNo property templates found matching '" + args + "'.\n");
        return;
    }

    let output = "\nFound " + matches.len() + " template(s) matching '" + args + "':\n\n";

    for template in matches {
        let rooms = get_template_rooms(template.id);
        let active = count_template_instances(template.vnum);

        output += "  " + template.vnum + " - " + template.name + "\n";
        output += "    Rent: " + template.monthly_rent + " gold/month\n";
        output += "    Rooms: " + rooms.len() + "\n";
        output += "    Active leases: " + active + "\n";
        if template.max_instances > 0 {
            output += "    Max instances: " + template.max_instances + "\n";
        }
        if template.level_requirement > 0 {
            output += "    Level requirement: " + template.level_requirement + "\n";
        }
        output += "\n";
    }

    send_client_message(connection_id, output);
}
