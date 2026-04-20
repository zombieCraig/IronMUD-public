// scripts/commands/pray.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to pray.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Pray what?");
        return;
    }

    let char_name = char.name;
    let room_id = char.current_room_id;

    // Show to actor
    send_client_message(connection_id, "You pray to the gods: " + args);

    // Broadcast to others in room
    broadcast_to_room(room_id, char_name + " prays to the gods.", char_name);

    // Send to Matrix
    matrix_broadcast(char_name + " prays to the gods: " + args);
}
