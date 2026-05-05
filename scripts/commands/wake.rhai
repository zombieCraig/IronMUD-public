// scripts/commands/wake.rhai
// Wake up from sleeping (goes to sitting position), or wake another player/mobile.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let target_name = args;

    // No argument: wake self.
    if target_name == "" {
        if char.position != "sleeping" {
            send_client_message(connection_id, "You are not asleep.");
            return;
        }

        // Check if can wake (10% stamina required)
        let wake_threshold = char.max_stamina / 10;
        if char.stamina < wake_threshold {
            send_client_message(connection_id, "You are too exhausted to wake up. You need at least 10% stamina (" + wake_threshold + "/" + char.max_stamina + ").");
            return;
        }

        char.position = "sitting";
        save_character_data(char);
        set_player_character(connection_id, char);

        send_client_message(connection_id, "You wake up and sit up.");
        broadcast_to_room(char.current_room_id, char.name + " wakes up.", char.name);
        return;
    }

    // Must be awake to wake someone else.
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep yourself.");
        return;
    }

    // Try a player in the same room first.
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn != "" {
        let target_char = get_player_character(target_conn);
        if target_char != () && target_char.current_room_id == char.current_room_id {
            if target_char.position != "sleeping" {
                send_client_message(connection_id, target_char.name + " is not asleep.");
                return;
            }

            target_char.position = "sitting";
            save_character_data(target_char);
            set_player_character(target_conn, target_char);

            send_client_message(connection_id, "You gently shake " + target_char.name + " awake.");
            send_client_message(target_conn, char.name + " shakes you awake.");
            broadcast_to_room(char.current_room_id, char.name + " shakes " + target_char.name + " awake.", char.name);
            return;
        }
    }

    // Try a mobile in the current room (filter invisible ones the waker can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
    let mobile = find_mobile_by_keyword(target_name, mobiles);
    if mobile != () {
        let activity = get_mobile_activity(mobile.id);
        if activity != "sleeping" {
            send_client_message(connection_id, mobile.name + " is not asleep.");
            return;
        }

        set_mobile_activity(mobile.id, "working");

        send_client_message(connection_id, "You gently shake " + mobile.name + " awake.");
        broadcast_to_room(char.current_room_id, char.name + " shakes " + mobile.name + " awake.", char.name);
        return;
    }

    send_client_message(connection_id, "There is no '" + target_name + "' here to wake.");
}
