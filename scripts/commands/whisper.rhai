// scripts/commands/whisper.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to whisper.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: whisper <player> <message>");
        return;
    }

    // Split args into target name and message
    let space_pos = args.index_of(" ");
    if space_pos < 0 {
        send_client_message(connection_id, "Usage: whisper <player> <message>");
        return;
    }

    let target_name = args.sub_string(0, space_pos);
    let message = args.sub_string(space_pos + 1);

    if message == "" {
        send_client_message(connection_id, "Whisper what?");
        return;
    }

    // Can't whisper to yourself
    if target_name.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "Whispering to yourself?");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    let actual_name = target_char.name;

    // Check if target is in the same room
    if target_char.current_room_id != char.current_room_id {
        send_client_message(connection_id, actual_name + " is not in this room.");
        return;
    }

    // Send message to target
    send_client_message(target_conn, char.name + " whispers: " + message);

    // Confirm to sender
    send_client_message(connection_id, "You whisper to " + actual_name + ": " + message);
}
