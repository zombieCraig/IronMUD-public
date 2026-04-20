// scripts/commands/stand.rhai
// Stand up from sitting or sleeping

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "standing" {
        send_client_message(connection_id, "You are already standing.");
        return;
    }

    if char.position == "swimming" {
        send_client_message(connection_id, "You are swimming! You need to leave the water first.");
        return;
    }

    // Check if can wake from sleeping (10% stamina required)
    if char.position == "sleeping" {
        let wake_threshold = char.max_stamina / 10;
        if char.stamina < wake_threshold {
            send_client_message(connection_id, "You are too exhausted to wake up. You need at least 10% stamina (" + wake_threshold + "/" + char.max_stamina + ").");
            return;
        }
    }

    let old_position = char.position;
    char.position = "standing";
    save_character_data(char);
    set_player_character(connection_id, char);

    if old_position == "sleeping" {
        send_client_message(connection_id, "You wake up and stand up.");
        broadcast_to_room(char.current_room_id, char.name + " wakes up and stands.", char.name);
    } else {
        send_client_message(connection_id, "You stand up.");
        broadcast_to_room(char.current_room_id, char.name + " stands up.", char.name);
    }
}
