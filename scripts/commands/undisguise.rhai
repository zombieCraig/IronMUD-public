// scripts/commands/undisguise.rhai
// undisguise - Remove your disguise and reveal your true identity

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !has_buff(char.name, "disguise") {
        send_client_message(connection_id, "You are not wearing a disguise.");
        return;
    }

    remove_buff(char.name, "disguise");
    send_client_message(connection_id, "You remove your disguise and reveal your true identity.");
    broadcast_to_room_awake(char.current_room_id, char.name + " removes a disguise!", char.name);
}
