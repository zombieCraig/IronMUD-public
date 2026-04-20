// scripts/commands/rgoto.rhai
// Usage: rgoto <room_id|vnum>
// Teleport to a room by its ID or vnum

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Cannot teleport while in combat
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You cannot teleport while in combat! Use 'flee' to escape.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: rgoto <room_id|vnum>\nUse 'rlist' to see available rooms.");
        return;
    }

    // Resolve vnum or UUID to UUID
    let target_room_id = resolve_room_id(args);
    if target_room_id == "" {
        send_client_message(connection_id, "Room not found: " + args);
        return;
    }

    // Get the target room
    let target_room = get_room_data(target_room_id);
    if target_room == () {
        send_client_message(connection_id, "Room not found: " + args);
        return;
    }

    // Check if already there
    if char.current_room_id == target_room_id {
        send_client_message(connection_id, "You are already in that room.");
        return;
    }

    // Announce departure from old room
    let old_room_id = char.current_room_id;
    broadcast_to_room(old_room_id, char.name + " vanishes in a flash of light.", char.name);

    // Move the character
    char.current_room_id = target_room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    // Announce arrival in new room
    broadcast_to_room(target_room_id, char.name + " appears in a flash of light.", char.name);

    // Show teleport message and display the room using shared display function
    send_client_message(connection_id, "You teleport to " + target_room.title + ".\n");
    display_room(target_room_id, connection_id, char.name);
}
