// scripts/commands/emote.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to emote.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Emote what?");
        return;
    }

    let char_name = char.name;
    let room_id = char.current_room_id;

    // Show to actor
    send_client_message(connection_id, "You " + args);

    // Broadcast to others in room (sleeping players don't see emotes)
    broadcast_to_room_awake(room_id, char_name + " " + args, char_name);
}
