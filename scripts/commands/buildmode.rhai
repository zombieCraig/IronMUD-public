fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () { return; }

    let new_state = !char.build_mode;
    if !set_build_mode(connection_id, new_state) {
        send_client_message(connection_id, "You must be a builder to use build mode.");
        return;
    }

    if new_state {
        send_client_message(connection_id, "\x1b[1;33m[BUILD MODE ENABLED]\x1b[0m Restrictions bypassed in areas you can edit.");
    } else {
        send_client_message(connection_id, "\x1b[1;33m[BUILD MODE DISABLED]\x1b[0m Normal restrictions apply.");
    }
}
