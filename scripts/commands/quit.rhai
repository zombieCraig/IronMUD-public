// scripts/commands/quit.rhai

// Function to handle the 'quit' command
// connection_id: string - the ID of the current connection
fn run_command(args, connection_id) {
    // Use try-catch since get_player_character throws if not logged in
    let char = ();
    try {
        char = get_player_character(connection_id);
    }
    catch (err) {
        // char stays as ()
    }

    if char != () { // If a character is logged in
        // Announce departure to room
        let char_name = char.name;
        let room_id = char.current_room_id;
        broadcast_to_room(room_id, char_name + " fades away into the ether.", char_name);

        // Announce to Matrix room
        matrix_broadcast(char_name + " has left the realm.");

        // Save character data
        send_client_message(connection_id, "Saving character '" + char_name + "'...");
        save_character_data(char);

        // Clear character from session
        clear_player_character(connection_id);

        send_client_message(connection_id, "Goodbye, " + char_name + ".");
    } else {
        send_client_message(connection_id, "Goodbye.");
    }

    // Disconnect the client
    disconnect_client(connection_id);
}