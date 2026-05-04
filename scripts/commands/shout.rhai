// scripts/commands/shout.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to shout.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Shout what?");
        return;
    }

    // Cannot shout underwater
    let room = get_room_data(char.current_room_id);
    if room != () && room.flags.underwater && !char.god_mode {
        send_client_message(connection_id, "You can't shout underwater!");
        return;
    }

    let char_name = char.name;
    let room_id = char.current_room_id;

    // Shouting breaks stealth
    if is_stealthy(char.name) {
        break_stealth(char.name);
        send_client_message(connection_id, "You reveal yourself by shouting.");
    }

    // Get current room data for exits
    let room = get_room_data(room_id);
    if room == () {
        send_client_message(connection_id, "You are in an invalid room!");
        return;
    }

    // Apply jaw garbling to outgoing speech
    let spoken = args;
    let jaw_penalty = get_jaw_wound_penalty(char.name);
    if jaw_penalty > 0 {
        spoken = garble_jaw_speech(spoken, jaw_penalty);
    }

    // Show to shouter (show original)
    send_client_message(connection_id, "You shout: " + args + "!");

    let source_soundproof = room.flags.soundproof && !char.god_mode;

    // Broadcast per-listener in current room with ear garbling
    let chars_in_room = get_characters_in_room(room_id);
    for listener_name in chars_in_room {
        if listener_name.to_lower() == char_name.to_lower() { continue; }
        let listener_conn = find_player_connection_by_name(listener_name);
        if listener_conn == "" { continue; }
        let heard = spoken;
        let ear_penalty = get_ear_wound_penalty(listener_name);
        if ear_penalty > 0 { heard = garble_hearing(heard, ear_penalty); }
        send_client_message(listener_conn, char_name + " shouts: " + heard + "!");
    }

    if source_soundproof {
        send_client_message(connection_id, "Your shout dies in the silence of this place.");
        return;
    }

    // Broadcast to connected rooms (anonymous, per-listener ear garbling)
    let exits = room.exits;
    let adj_rooms = [];
    if exits.north != "" { adj_rooms.push(exits.north); }
    if exits.east != "" { adj_rooms.push(exits.east); }
    if exits.south != "" { adj_rooms.push(exits.south); }
    if exits.west != "" { adj_rooms.push(exits.west); }
    if exits.up != "" { adj_rooms.push(exits.up); }
    if exits.down != "" { adj_rooms.push(exits.down); }

    for adj_room in adj_rooms {
        let adj_room_data = get_room_data(adj_room);
        if adj_room_data != () && adj_room_data.flags.soundproof { continue; }
        let adj_chars = get_characters_in_room(adj_room);
        for listener_name in adj_chars {
            let listener_conn = find_player_connection_by_name(listener_name);
            if listener_conn == "" { continue; }
            let heard = spoken;
            let ear_penalty = get_ear_wound_penalty(listener_name);
            if ear_penalty > 0 { heard = garble_hearing(heard, ear_penalty); }
            send_client_message(listener_conn, "Someone shouts: " + heard + "!");
        }
    }
}
