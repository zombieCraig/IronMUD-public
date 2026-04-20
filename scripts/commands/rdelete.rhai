// scripts/commands/rdelete.rhai
// Usage: rdelete <room_id>
// Delete a room from the database

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: rdelete <room_id>\nUse 'rlist' to see available rooms.");
        return;
    }

    let target_room_id = args;

    // Check if room exists
    let target_room = get_room_data(target_room_id);
    if target_room == () {
        send_client_message(connection_id, "Room not found: " + target_room_id);
        return;
    }

    // Check area permissions if room belongs to an area
    let room_area_id = target_room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to delete rooms in this area.");
            return;
        }
    }

    // Don't allow deleting the room you're in
    if char.current_room_id == target_room_id {
        send_client_message(connection_id, "You cannot delete the room you are standing in. Use 'rgoto' to move first.");
        return;
    }

    // Don't allow deleting the starting room
    let starting_room = "00000000-0000-0000-0000-000000000001";
    if target_room_id == starting_room {
        send_client_message(connection_id, "You cannot delete the starting room.");
        return;
    }

    let room_title = target_room.title;

    // Clean up exits pointing to this room from other rooms
    let all_rooms = list_all_rooms();
    let cleaned_exits = 0;

    for room in all_rooms {
        let exits = room.exits;
        let needs_update = false;

        if exits.north == target_room_id {
            clear_room_exit(room.id, "north");
            cleaned_exits += 1;
        }
        if exits.south == target_room_id {
            clear_room_exit(room.id, "south");
            cleaned_exits += 1;
        }
        if exits.east == target_room_id {
            clear_room_exit(room.id, "east");
            cleaned_exits += 1;
        }
        if exits.west == target_room_id {
            clear_room_exit(room.id, "west");
            cleaned_exits += 1;
        }
        if exits.up == target_room_id {
            clear_room_exit(room.id, "up");
            cleaned_exits += 1;
        }
        if exits.down == target_room_id {
            clear_room_exit(room.id, "down");
            cleaned_exits += 1;
        }
    }

    // Delete the room
    if delete_room(target_room_id) {
        let output = "Deleted room: " + room_title + " (" + target_room_id + ")";
        if cleaned_exits > 0 {
            output += "\nRemoved " + cleaned_exits + " exit(s) pointing to this room.";
        }
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, "Error: Failed to delete room.");
    }
}
