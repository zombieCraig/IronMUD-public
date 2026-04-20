// scripts/commands/link.rhai
// Usage: link <direction> <room_id> [both]
// Links an exit to an existing room

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
        send_client_message(connection_id, "You must be logged in to link rooms.");
        return;
    }

    // Parse arguments: direction room_id [both]
    let parts = args.split(" ");
    if parts.len() < 2 || parts[0] == "" || parts[1] == "" {
        send_client_message(connection_id, "Usage: link <direction> <vnum or room_id> [both]\nExample: link east tavern_backroom\nAdd 'both' to create bidirectional link.");
        return;
    }

    let direction = normalize_direction(parts[0]);
    let target_room_id = parts[1];
    let bidirectional = false;

    if parts.len() >= 3 && parts[2].to_lower() == "both" {
        bidirectional = true;
    }

    let valid_dirs = ["north", "south", "east", "west", "up", "down"];

    let is_valid = false;
    for v in valid_dirs {
        if direction == v {
            is_valid = true;
        }
    }

    if !is_valid {
        send_client_message(connection_id, "Invalid direction. Use: north, south, east, west, up, down");
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
            send_client_message(connection_id, "You don't have permission to link exits from rooms in this area.");
            return;
        }
    }

    // Check target room exists - try vnum first, then UUID
    let target_room = get_room_by_vnum(target_room_id);
    if target_room == () {
        target_room = get_room_data(target_room_id);
    }
    if target_room == () {
        send_client_message(connection_id, "Error: Target room '" + target_room_id + "' not found.\nUse 'rlist' to see available rooms.");
        return;
    }
    // Use the actual room ID for linking
    let actual_target_id = target_room.id;

    // Check area permissions for target room if bidirectional
    if bidirectional {
        let target_area_id = target_room.area_id;
        if target_area_id != "" {
            if !can_edit_area(target_area_id, char.name) {
                send_client_message(connection_id, "You don't have permission to link exits to rooms in that area. Use without 'both' for one-way link.");
                return;
            }
        }
    }

    // Link current room to target room
    if !set_room_exit(char.current_room_id, direction, actual_target_id) {
        send_client_message(connection_id, "Error: Failed to create exit.");
        return;
    }

    let output = "Exit linked: " + direction + " -> " + target_room.title + "\n";

    // If bidirectional, create reverse link
    if bidirectional {
        let opposite = get_opposite_direction(direction);
        if opposite != "" {
            if set_room_exit(actual_target_id, opposite, char.current_room_id) {
                output += "Reverse exit linked: " + opposite + " -> " + current_room.title;
            } else {
                output += "Warning: Failed to create reverse exit.";
            }
        }
    }

    send_client_message(connection_id, output);
}
