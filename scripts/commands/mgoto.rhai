// scripts/commands/mgoto.rhai
// Usage: mgoto <mobile_id|vnum>
// Teleport to the room where a mobile by its ID or vnum is located

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
        send_client_message(connection_id, "Usage: mgoto <mobile_id|vnum>\nUse 'mlist' to see available mobiles.");
        return;
    }

    // Resolve vnum or UUID to UUID
    let target_mobile_id = resolve_mobile_id(args);
    if target_mobile_id == "" {
        send_client_message(connection_id, "Mobile not found: " + args);
        return;
    }

    // Get the target mobile
    let target_mobile = get_mobile_data(target_mobile_id);
    if target_mobile == () {
        send_client_message(connection_id, "Mobile not found: " + args);
        return;
    }

    // Check if already there
    if char.current_room_id == target_mobile.room_id {
        send_client_message(connection_id, "You are already in the same room as that mobile.");
        return;
    }

    // Announce departure from old room
    let old_room_id = char.current_room_id;
    broadcast_to_room(old_room_id, char.name + " vanishes in a flash of light.", char.name);

    // Move the character
    char.current_room_id = target_mobile.room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    // Announce arrival in new room
    broadcast_to_room(target_mobile.room_id, char.name + " appears in a flash of light.", char.name);

    // Show teleport message and display the room using shared display function
    send_client_message(connection_id, "You teleport to the room where " + target_mobile.name + " is located.\n");
    display_room(target_mobile.room_id, connection_id, char.name);
}

// Function to resolve mobile ID from vnum or name
fn resolve_mobile_id(args) {
    // Try resolving by UUID first
    let mobile = get_mobile_data(args);
    if mobile != () {
        return args;
    }

    // If not found by UUID, try resolving by vnum or name.
    // Skip prototypes and roomless mobiles: a prototype shares the vnum but
    // has no room, so teleporting to it strands the player in a void.
    let mobiles = list_all_mobiles();
    let fallback = "";
    for mob in mobiles {
        if mob.is_prototype { continue; }
        if mob.vnum == args || mob.name.to_lower() == args.to_lower() {
            if mob.has_room_id {
                return mob.id;
            }
            if fallback == "" {
                fallback = mob.id;
            }
        }
    }

    // Return the best (live, roomed) match, or a roomless fallback if that's
    // all we found, or empty string if no match at all.
    fallback
}
