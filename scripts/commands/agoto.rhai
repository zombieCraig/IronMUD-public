fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You cannot teleport while in combat! Use 'flee' to escape.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: agoto <area_prefix>\nUse 'alist' to see available areas.");
        return;
    }

    let area = get_area_by_prefix(args);
    if area == () {
        area = get_area_data(args);
    }
    if area == () {
        send_client_message(connection_id, "Area not found: " + args + "\nUse 'alist' to see available areas.");
        return;
    }

    let starting_vnum = get_area_starting_room_vnum(area.id);
    if starting_vnum == "" {
        send_client_message(connection_id, "Area '" + area.name + "' has no starting room set.\nUse 'aedit " + area.prefix + " starting_room <vnum>' to set one.");
        return;
    }

    let target_room_id = resolve_room_id(starting_vnum);
    if target_room_id == "" {
        send_client_message(connection_id, "Starting room vnum '" + starting_vnum + "' no longer resolves to a valid room.\nUse 'aedit " + area.prefix + " starting_room <vnum>' to update it.");
        return;
    }

    let target_room = get_room_data(target_room_id);
    if target_room == () {
        send_client_message(connection_id, "Could not load target room.");
        return;
    }

    if char.current_room_id == target_room_id {
        send_client_message(connection_id, "You are already in that room.");
        return;
    }

    let old_room_id = char.current_room_id;
    broadcast_to_room(old_room_id, char.name + " vanishes in a flash of light.", char.name);

    char.current_room_id = target_room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    broadcast_to_room(target_room_id, char.name + " appears in a flash of light.", char.name);

    send_client_message(connection_id, "You teleport to " + area.name + " (" + target_room.title + ").\n");
    display_room(target_room_id, connection_id, char.name);
}
