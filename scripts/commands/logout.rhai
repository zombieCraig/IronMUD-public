// scripts/commands/logout.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);

    if char != () {
        // Announce departure to room
        let char_name = char.name;
        let room_id = char.current_room_id;
        broadcast_to_room(room_id, char_name + " fades away into the ether.", char_name);

        // Announce to Matrix room
        matrix_broadcast(char_name + " has left the realm.");

        // Save character data before logout
        send_client_message(connection_id, "Saving character '" + char_name + "'...");
        save_character_data(char);

        // Clear character from session (but don't disconnect)
        clear_player_character(connection_id);

        send_client_message(connection_id, "You have logged out.\n");
    }

    // Redisplay the welcome banner
    send_banner(connection_id);
}
