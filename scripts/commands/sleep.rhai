// scripts/commands/sleep.rhai
// Go to sleep - fastest stamina and HP regeneration

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are already asleep.");
        return;
    }

    // Cannot sleep in water without a boat
    if !char.god_mode {
        let room = get_room_data(char.current_room_id);
        if room != () && (room.flags.shallow_water || room.flags.deep_water || room.flags.underwater) {
            let has_boat = player_has_item_flag(char.name, "boat");
            if !has_boat {
                send_client_message(connection_id, "You can't sleep in the water!");
                return;
            }
        }
    }

    char.position = "sleeping";
    save_character_data(char);
    set_player_character(connection_id, char);

    send_client_message(connection_id, "You lie down and go to sleep.");
    broadcast_to_room(char.current_room_id, char.name + " lies down and goes to sleep.", char.name);
}
