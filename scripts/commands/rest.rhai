// scripts/commands/rest.rhai
// Alias for sit - sit down to rest

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sitting" {
        send_client_message(connection_id, "You are already resting.");
        return;
    }
    if char.position == "sleeping" {
        send_client_message(connection_id, "You need to wake up first.");
        return;
    }

    // Cannot rest in water without a boat
    if !char.god_mode {
        let room = get_room_data(char.current_room_id);
        if room != () && (room.flags.shallow_water || room.flags.deep_water || room.flags.underwater) {
            let has_boat = player_has_item_flag(char.name, "boat");
            if !has_boat {
                send_client_message(connection_id, "You can't rest in the water!");
                return;
            }
        }
    }

    char.position = "sitting";
    save_character_data(char);
    set_player_character(connection_id, char);

    send_client_message(connection_id, "You sit down and rest.");
    broadcast_to_room(char.current_room_id, char.name + " sits down to rest.", char.name);
}
