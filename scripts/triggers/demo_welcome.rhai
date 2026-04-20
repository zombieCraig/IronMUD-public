// scripts/triggers/demo_welcome.rhai
// Welcome message when entering the town square.
// Fires on room enter, gives players a brief orientation.

fn run_trigger(room_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () { return "continue"; }

    send_client_message(connection_id, "\n\x1b[1;33m--- Welcome to Oakvale Village! ---\x1b[0m");
    send_client_message(connection_id, "This is the town square, the heart of the village.");
    send_client_message(connection_id, "Type \x1b[1mhelp\x1b[0m to see available commands, or \x1b[1mlook\x1b[0m to see your surroundings.");

    return "continue";
}
