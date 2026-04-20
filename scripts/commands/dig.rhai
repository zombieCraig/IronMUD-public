// scripts/commands/dig.rhai
// Usage: dig <direction> [title]
// Creates a new room and connects it bidirectionally

fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "down"; }
    if dir == "down" { return "up"; }
    return "";
}

fn normalize_direction(dir) {
    let d = dir.to_lower();
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to dig.");
        return;
    }

    // Parse arguments: direction [title...]
    let parts = args.split(" ");
    if parts.len() < 1 || parts[0] == "" {
        send_client_message(connection_id, "Usage: dig <direction> [title]\nExample: dig north The Dark Forest");
        return;
    }

    let direction = normalize_direction(parts[0]);
    let valid_dirs = ["north", "south", "east", "west", "up", "down"];

    let is_valid = false;
    for v in valid_dirs {
        if direction == v {
            is_valid = true;
        }
    }

    if !is_valid {
        send_client_message(connection_id, "Invalid direction. Use: north, south, east, west, up, down (or n, s, e, w, u, d)");
        return;
    }

    // Get current room
    let current_room = get_room_data(char.current_room_id);
    if current_room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check area permissions if current room belongs to an area
    let room_area_id = current_room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to dig from rooms in this area.");
            return;
        }
    }

    // Check if exit already exists
    let exits = current_room.exits;
    let existing_exit = "";
    if direction == "north" { existing_exit = exits.north; }
    if direction == "south" { existing_exit = exits.south; }
    if direction == "east" { existing_exit = exits.east; }
    if direction == "west" { existing_exit = exits.west; }
    if direction == "up" { existing_exit = exits.up; }
    if direction == "down" { existing_exit = exits.down; }

    if existing_exit != "" {
        send_client_message(connection_id, "An exit already exists to the " + direction + ". Use 'unlink " + direction + "' first.");
        return;
    }

    // Parse title (remaining args joined)
    let title = "A New Room";
    if parts.len() >= 2 {
        let title_parts = [];
        for i in 1..parts.len() {
            title_parts.push(parts[i]);
        }
        title = title_parts.reduce(|a, b| a + " " + b);
    }

    // Create the new room
    let new_room = create_room(title, "This room has no description yet.");
    let new_room_id = new_room.id;

    // Check if current room is a property template room
    let is_template = current_room.is_property_template;
    let template_id = current_room.property_template_id;

    // If digging from a template room, link new room to the same template
    if is_template && template_id != "" && template_id != () {
        mark_room_as_template(new_room_id, template_id);
    }

    // Inherit area from current room and auto-generate vnum
    let inherited_vnum = "";
    if room_area_id != "" {
        set_room_area(new_room_id, room_area_id);

        // Only auto-generate vnum if a custom title was provided (not default)
        // and this is not a template room (template rooms don't need vnums)
        if title != "A New Room" && !is_template {
            let area = get_area_data(room_area_id);
            if area != () {
                inherited_vnum = generate_unique_vnum(area.prefix, title);
                set_room_vnum(new_room_id, inherited_vnum);
            }
        }
    }

    // Link current room to new room
    set_room_exit(char.current_room_id, direction, new_room_id);

    // Link new room back to current room
    let opposite = get_opposite_direction(direction);
    if opposite != "" {
        set_room_exit(new_room_id, opposite, char.current_room_id);
    }

    // Build response
    let output = "Created room: " + title + "\n";
    output += "Room ID: " + new_room_id + "\n";
    if inherited_vnum != "" {
        output += "Vnum: " + inherited_vnum + "\n";
    }
    if is_template {
        output += "(Room linked to property template)\n";
    }
    if room_area_id != "" {
        let area = get_area_data(room_area_id);
        if area != () {
            output += "Area: " + area.name + " [" + area.prefix + "]\n";
        }
    }
    output += "Exit " + direction + " -> " + title + "\n";
    if opposite != "" {
        output += "Reverse exit " + opposite + " -> " + current_room.title;
    }

    send_client_message(connection_id, output);
}
