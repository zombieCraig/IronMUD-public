// pedit.rhai - Property template editor
// Builder command for creating and managing property templates
// Usage: pedit create <name> - Create a new property template (multi-word names supported)
//        pedit <vnum>        - Enter template for editing (shortcut)
//        pedit edit <vnum>   - Enter template for editing
//        pedit <subcommand>  - Modify current template (when in template room)
//        pedit done          - Exit template editing mode

// Import shared OLC helpers
import "olc_helpers" as olc;

fn meta() {
    #{
        "access": "builder",
        "description": "Property template editor - create/edit rental property templates"
    }
}

fn format_template_info(template, room_count, amenity_count) {
    let output = "\n=== Property Template: " + template.vnum + " ===\n\n";
    output += "Name: " + template.name + "\n";
    output += "Description: " + (if template.description == "" { "(not set)" } else { template.description }) + "\n";
    output += "Monthly Rent: " + template.monthly_rent + " gold\n";
    output += "Max Instances: " + (if template.max_instances == 0 { "unlimited" } else { template.max_instances }) + "\n";
    output += "Level Requirement: " + (if template.level_requirement == 0 { "none" } else { template.level_requirement }) + "\n";
    output += "\nRooms: " + room_count + "\n";
    output += "Amenities: " + amenity_count + " (items with no_get flag)\n";
    output += "\nCommands:\n";
    output += "  pedit name <text>    - Set display name\n";
    output += "  pedit vnum <vnum>    - Change template vnum\n";
    output += "  pedit desc           - Edit description (enter multi-line)\n";
    output += "  pedit rent <amount>  - Set monthly rent\n";
    output += "  pedit max <count>    - Set max instances (0=unlimited)\n";
    output += "  pedit level <num>    - Set level requirement\n";
    output += "  pedit entrance       - Mark current room as entrance\n";
    output += "  pedit done           - Exit and save template\n";
    return output;
}

fn count_template_amenities(template_id) {
    let rooms = get_template_rooms(template_id);
    let count = 0;
    for room in rooms {
        let items = get_items_in_room(room.id);
        for item in items {
            if get_item_flag(item.id, "no_get") {
                count += 1;
            }
        }
    }
    return count;
}

fn move_player_to_room(char, connection_id, room_id) {
    char.current_room_id = room_id;
    save_character_data(char);
    set_player_character(connection_id, char);
}

fn show_help(connection_id) {
    let output = "\n=== Property Template Editor ===\n\n";
    output += "You are not currently in a property template room.\n\n";
    output += "Commands:\n";
    output += "  pedit create <name>  - Create a new property template (multi-word names supported)\n";
    output += "  pedit <vnum>         - Enter an existing template for editing\n";
    output += "  pedit edit <vnum>    - Same as above\n";
    output += "\nWhen inside a template room:\n";
    output += "  pedit                - Show template info\n";
    output += "  pedit name <text>    - Set display name\n";
    output += "  pedit vnum <vnum>    - Change template vnum\n";
    output += "  pedit desc           - Edit description\n";
    output += "  pedit rent <amount>  - Set monthly rent\n";
    output += "  pedit max <count>    - Set max instances (0=unlimited)\n";
    output += "  pedit level <num>    - Set level requirement\n";
    output += "  pedit entrance       - Mark current room as entrance\n";
    output += "  pedit done           - Exit and save template\n";
    send_client_message(connection_id, output);
}

fn handle_create(char, room, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit create <name>");
        send_client_message(connection_id, "Example: pedit create Sendai Apartments");
        return;
    }

    // Join all parts after "create" as the name (supports multi-word names)
    let name = olc::build_string_from_parts(parts, 1);

    // Generate a unique vnum from the name
    let vnum = generate_unique_property_vnum(name);

    let template = new_property_template(vnum);
    template.name = name;

    if room.area_id != "" {
        template.area_id = room.area_id;
    }

    if !save_property_template(template) {
        send_client_message(connection_id, "Error: Failed to save property template.");
        return;
    }

    let entrance = create_template_room(template.id, "Template Entrance", "An empty room. Use 'redit' to customize.", true);
    if entrance == () {
        send_client_message(connection_id, "Error: Failed to create entrance room.");
        return;
    }

    // Save origin room before moving
    char.tour_origin_room = char.current_room_id;
    char.on_tour = true;

    move_player_to_room(char, connection_id, entrance.id);

    let output = "\nCreated property template '" + name + "' (vnum: " + vnum + ").\n";
    output += "You are now in the template entrance room.\n";
    output += "Use 'redit' to edit rooms, 'dig' to add rooms.\n";
    output += "Use 'pedit done' when finished.\n";
    send_client_message(connection_id, output);
}

// Helper to enter a template by vnum - shared by handle_edit and implicit vnum shortcut
fn enter_template(char, vnum, connection_id) {
    let template = get_property_template(vnum);
    if template == () {
        send_client_message(connection_id, "Property template '" + vnum + "' not found.");
        return false;
    }

    if template.entrance_room_id == "" || template.entrance_room_id == "00000000-0000-0000-0000-000000000000" {
        send_client_message(connection_id, "Error: Template has no entrance room set.");
        return false;
    }

    // Save origin room before moving
    char.tour_origin_room = char.current_room_id;
    char.on_tour = true;

    move_player_to_room(char, connection_id, template.entrance_room_id);
    send_client_message(connection_id, "\nEntering template '" + vnum + "' for editing...\n");
    return true;
}

fn handle_edit(char, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit edit <vnum>");
        return;
    }
    let vnum = parts[1].to_lower();
    enter_template(char, vnum, connection_id);
}

fn handle_done(char, room, connection_id) {
    if !room.is_property_template {
        send_client_message(connection_id, "You are not currently in a property template room.");
        return;
    }

    let template = get_template_for_room(room.id);
    if template == () {
        send_client_message(connection_id, "Error: Could not find template for this room.");
        return;
    }

    let rooms = get_template_rooms(template.id);
    let amenities = count_template_amenities(template.id);

    // Determine return destination
    let return_room_name = "town square";
    let return_room_id = "";

    // Check if we have a saved origin room from tour state
    if char.on_tour && char.tour_origin_room != "" {
        let origin = get_room_data(char.tour_origin_room);
        if origin != () {
            return_room_id = origin.id;
            return_room_name = origin.title;
        }
    }

    // Fallback to town:square if no valid origin room
    if return_room_id == "" {
        let starting_room = get_room_by_vnum("town:square");
        if starting_room != () {
            return_room_id = starting_room.id;
            return_room_name = "town square";
        }
    }

    // Clear tour state
    char.on_tour = false;
    char.tour_origin_room = "";

    // Move player to return destination
    if return_room_id != "" {
        move_player_to_room(char, connection_id, return_room_id);
    }

    let output = "\nTemplate '" + template.vnum + "' saved.\n";
    output += "  Rooms: " + rooms.len() + "\n";
    output += "  Amenities: " + amenities + "\n";
    output += "\nReturning to " + return_room_name + "...\n";
    send_client_message(connection_id, output);
}

fn handle_name(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit name <text>");
        return;
    }
    let name_parts = [];
    for i in 1..parts.len() {
        name_parts.push(parts[i]);
    }
    let new_name = name_parts.join(" ");

    template.name = new_name;
    if save_property_template(template) {
        send_client_message(connection_id, "Name set to '" + new_name + "'.");
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn handle_desc(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit desc <text>");
        send_client_message(connection_id, "Current: " + template.description);
        return;
    }
    let desc_parts = [];
    for i in 1..parts.len() {
        desc_parts.push(parts[i]);
    }
    let new_desc = desc_parts.join(" ");

    template.description = new_desc;
    if save_property_template(template) {
        send_client_message(connection_id, "Description updated.");
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn handle_rent(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit rent <amount>");
        return;
    }
    let amount = parse_int(parts[1]);
    if amount == () || amount < 0 {
        send_client_message(connection_id, "Invalid rent amount. Must be a positive number.");
        return;
    }

    template.monthly_rent = amount;
    if save_property_template(template) {
        send_client_message(connection_id, "Monthly rent set to " + amount + " gold.");
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn handle_max(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit max <count> (0=unlimited)");
        return;
    }
    let count = parse_int(parts[1]);
    if count == () || count < 0 {
        send_client_message(connection_id, "Invalid count. Must be a non-negative number.");
        return;
    }

    template.max_instances = count;
    if save_property_template(template) {
        let msg = if count == 0 { "Maximum instances set to unlimited." } else { "Maximum instances set to " + count + "." };
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn handle_level(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit level <num> (0=no requirement)");
        return;
    }
    let level = parse_int(parts[1]);
    if level == () || level < 0 {
        send_client_message(connection_id, "Invalid level. Must be a non-negative number.");
        return;
    }

    template.level_requirement = level;
    if save_property_template(template) {
        let msg = if level == 0 { "Level requirement removed." } else { "Level requirement set to " + level + "." };
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn handle_entrance(room, connection_id) {
    if set_room_as_template_entrance(room.id) {
        send_client_message(connection_id, "Current room marked as template entrance.");
    } else {
        send_client_message(connection_id, "Error: Failed to set room as entrance.");
    }
}

fn handle_vnum(template, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: pedit vnum <new_vnum>");
        send_client_message(connection_id, "Current: " + template.vnum);
        return;
    }

    // Join all parts after "vnum" and convert to slug format
    let vnum = olc::build_string_from_parts(parts, 1);
    vnum = vnum.to_lower();
    // Replace spaces with underscores
    let slug = "";
    for c in vnum.chars() {
        if c == ' ' {
            slug += "_";
        } else {
            slug += c;
        }
    }
    vnum = slug;

    // Check for collision with different template
    let existing = get_property_template(vnum);
    if existing != () && existing.id != template.id {
        send_client_message(connection_id, "VNUM '" + vnum + "' already in use by: " + existing.name);
        return;
    }

    template.vnum = vnum;
    if save_property_template(template) {
        send_client_message(connection_id, "VNUM set to: " + vnum);
    } else {
        send_client_message(connection_id, "Error: Failed to save template.");
    }
}

fn dispatch_template_subcommand(subcommand, template, room, parts, connection_id) {
    if subcommand == "name" { handle_name(template, parts, connection_id); return true; }
    if subcommand == "vnum" { handle_vnum(template, parts, connection_id); return true; }
    if subcommand == "desc" { handle_desc(template, parts, connection_id); return true; }
    if subcommand == "rent" { handle_rent(template, parts, connection_id); return true; }
    if subcommand == "max" { handle_max(template, parts, connection_id); return true; }
    if subcommand == "level" { handle_level(template, parts, connection_id); return true; }
    if subcommand == "entrance" { handle_entrance(room, connection_id); return true; }
    return false;
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

    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "Error: Could not find current room.");
        return;
    }

    let parts = split_quoted_args(args);
    let subcommand = if parts.len() > 0 { parts[0].to_lower() } else { "" };

    // Handle top-level subcommands with early returns
    if subcommand == "create" { handle_create(char, room, parts, connection_id); return; }
    if subcommand == "edit" { handle_edit(char, parts, connection_id); return; }
    if subcommand == "done" { handle_done(char, room, connection_id); return; }

    // If not in a template room, check if subcommand is a valid vnum shortcut
    if !room.is_property_template {
        // Try to interpret subcommand as a template vnum (shortcut for "pedit edit <vnum>")
        if subcommand != "" {
            let template = get_property_template(subcommand);
            if template != () {
                // Found a template - teleport into it
                enter_template(char, subcommand, connection_id);
                return;
            }
        }
        show_help(connection_id);
        return;
    }

    // We're in a template room - get the template
    let template = get_template_for_room(room.id);
    if template == () {
        send_client_message(connection_id, "Error: Could not find template for this room.");
        return;
    }

    // Handle no subcommand (show info)
    if subcommand == "" {
        let rooms = get_template_rooms(template.id);
        let amenities = count_template_amenities(template.id);
        let output = format_template_info(template, rooms.len(), amenities);
        send_client_message(connection_id, output);
        return;
    }

    // Dispatch to template subcommand handlers
    if dispatch_template_subcommand(subcommand, template, room, parts, connection_id) {
        return;
    }

    // Unknown subcommand
    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
    send_client_message(connection_id, "Use 'pedit' for help.");
}
