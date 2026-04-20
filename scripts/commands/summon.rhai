// Summon command - summon a mobile or player to a location
// Admin-only command

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if !char.is_admin {
        send_client_message(connection_id, "This command requires admin access.");
        return;
    }

    let parts = args.split(" ");
    let sub_type = if parts.len() > 0 && parts[0] != "" {
        parts[0].to_lower()
    } else {
        ""
    };

    if sub_type == "" {
        show_usage(connection_id);
        return;
    }

    if sub_type == "mob" {
        handle_summon_mob(connection_id, char, parts);
        return;
    }

    if sub_type == "player" {
        handle_summon_player(connection_id, char, parts);
        return;
    }

    show_usage(connection_id);
}

fn show_usage(connection_id) {
    send_client_message(connection_id, "Usage: summon mob <vnum> [room_vnum]");
    send_client_message(connection_id, "       summon player <name> [room_vnum]");
}

fn handle_summon_mob(connection_id, admin_char, parts) {
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: summon mob <vnum> [room_vnum]");
        return;
    }

    let vnum = parts[1];

    // Determine destination room
    let dest_room_id = get_dest_room(connection_id, admin_char, parts, 2);
    if dest_room_id == "" {
        return;
    }

    // Find first spawned instance of this vnum
    let instances = get_mobile_instances_by_vnum(vnum);
    if instances.len() == 0 {
        send_client_message(connection_id, "No spawned instances of mobile vnum '" + vnum + "' found.");
        return;
    }

    let mobile = instances[0];

    // Check if already in destination room
    if mobile.current_room_id == dest_room_id {
        send_client_message(connection_id, mobile.short_desc + " is already in that room.");
        return;
    }

    let old_room_id = mobile.current_room_id;

    // Clear combat state if in combat
    if is_mobile_in_combat(mobile.id) {
        exit_mobile_combat(mobile.id);
    }

    // Announce departure from old room
    broadcast_to_room(old_room_id, mobile.short_desc + " vanishes in a flash of light!", "");

    // Move the mobile
    mobile.current_room_id = dest_room_id;
    save_mobile_data(mobile);

    // Announce arrival
    broadcast_to_room(dest_room_id, mobile.short_desc + " appears in a flash of light!", "");

    // Confirm to admin
    send_client_message(connection_id, "You have summoned " + mobile.short_desc + " (vnum: " + vnum + ") to room " + dest_room_id + ".");
}

fn handle_summon_player(connection_id, admin_char, parts) {
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: summon player <name> [room_vnum]");
        return;
    }

    let target_name = parts[1];

    // Can't summon yourself
    if target_name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot summon yourself.");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Could not find character data for '" + target_name + "'.");
        return;
    }

    // Determine destination room
    let dest_room_id = get_dest_room(connection_id, admin_char, parts, 2);
    if dest_room_id == "" {
        return;
    }

    // Check if already in destination room
    if target_char.current_room_id == dest_room_id {
        send_client_message(connection_id, target_char.name + " is already in that room.");
        return;
    }

    let old_room_id = target_char.current_room_id;

    // Clear combat state if in combat
    if is_in_combat(target_char.name) {
        exit_combat(target_char.name);

        // Clear any mobiles targeting this player in the old room
        let mobiles = get_mobiles_in_room(old_room_id);
        for mobile in mobiles {
            exit_mobile_combat(mobile.id);
        }

        send_client_message(target_conn, "Your combat has ended due to being summoned.");
    }

    // Announce departure from old room
    broadcast_to_room(old_room_id, target_char.name + " vanishes in a flash of light!", target_char.name);

    // Move the target character
    target_char.current_room_id = dest_room_id;
    save_character_data(target_char);
    set_player_character(target_conn, target_char);

    // Announce arrival
    broadcast_to_room_except(dest_room_id, target_conn, target_char.name + " appears in a flash of light!");

    // Notify target player
    send_client_message(target_conn, "You have been summoned by " + admin_char.name + "!");

    // Show target the new room
    display_room(dest_room_id, target_conn, target_char.name);

    // Confirm to admin
    send_client_message(connection_id, "You have summoned " + target_char.name + " to room " + dest_room_id + ".");
}

fn get_dest_room(connection_id, admin_char, parts, room_arg_index) {
    if parts.len() > room_arg_index && parts[room_arg_index] != "" {
        let room_vnum = parts[room_arg_index];
        let resolved = resolve_room_id(room_vnum);
        if resolved == "" {
            send_client_message(connection_id, "Room vnum '" + room_vnum + "' not found.");
            return "";
        }
        return resolved;
    }
    return admin_char.current_room_id;
}
