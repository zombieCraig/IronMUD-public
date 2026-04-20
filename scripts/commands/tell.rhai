// scripts/commands/tell.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to send tells.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: tell <player> <message>");
        return;
    }

    // Split args into target name and message
    let space_pos = args.index_of(" ");
    if space_pos < 0 {
        send_client_message(connection_id, "Usage: tell <player> <message>");
        return;
    }

    let target_name = args.sub_string(0, space_pos);
    let message = args.sub_string(space_pos + 1);

    if message == "" {
        send_client_message(connection_id, "Tell them what?");
        return;
    }

    // Can't tell yourself
    if target_name.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "Talking to yourself?");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's actual name (proper capitalization)
    let target_char = get_player_character(target_conn);
    let actual_name = target_char.name;

    // Apply jaw garbling to outgoing speech
    let spoken = message;
    let jaw_penalty = get_jaw_wound_penalty(char.name);
    if jaw_penalty > 0 {
        spoken = garble_jaw_speech(spoken, jaw_penalty);
    }

    // Apply ear garbling for recipient
    let heard = spoken;
    let ear_penalty = get_ear_wound_penalty(actual_name);
    if ear_penalty > 0 {
        heard = garble_hearing(heard, ear_penalty);
    }

    // Send message to target
    send_client_message(target_conn, char.name + " tells you: " + heard);

    // Confirm to sender (show original)
    send_client_message(connection_id, "You tell " + actual_name + ": " + message);
}
