// scripts/commands/unlink.rhai
// Usage: unlink <direction>
// Removes an exit from the current room

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
        send_client_message(connection_id, "You must be logged in to unlink exits.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: unlink <direction>\nExample: unlink north");
        return;
    }

    let direction = normalize_direction(args);
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
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check area permissions if current room belongs to an area
    let room_area_id = room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to unlink exits from rooms in this area.");
            return;
        }
    }

    // Check if exit exists
    let exits = room.exits;
    let existing_exit = "";
    if direction == "north" { existing_exit = exits.north; }
    if direction == "south" { existing_exit = exits.south; }
    if direction == "east" { existing_exit = exits.east; }
    if direction == "west" { existing_exit = exits.west; }
    if direction == "up" { existing_exit = exits.up; }
    if direction == "down" { existing_exit = exits.down; }

    if existing_exit == "" {
        send_client_message(connection_id, "There is no exit to the " + direction + ".");
        return;
    }

    // Get target room name for message
    let target_room = get_room_data(existing_exit);
    let target_name = if target_room != () { target_room.title } else { "unknown" };

    // Remove the exit
    if clear_room_exit(char.current_room_id, direction) {
        send_client_message(connection_id, "Removed exit " + direction + " (was: " + target_name + ").");
    } else {
        send_client_message(connection_id, "Error: Failed to remove exit.");
    }
}
