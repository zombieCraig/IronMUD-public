// scripts/commands/roomflags.rhai
// Toggle room flags/vnum display for builders
// Usage: roomflags [on|off]
// @access: builder
// @description: Toggle room flags and vnum display in room titles
// @category: building

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Check if user is builder or admin
    if !char.is_builder && !char.is_admin {
        send_client_message(connection_id, "This command is for builders only.");
        return;
    }

    let current = is_room_flags_enabled(connection_id);

    if args == "" {
        // Show current status
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Room flags display is currently " + status + ".");
        send_client_message(connection_id, "Usage: roomflags on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, room titles show vnum and flags in dark gray.");
        send_client_message(connection_id, "Example: Town Square [vnum:town:square, safe, indoors]");
        return;
    }

    let arg = args.to_lower();

    if arg == "on" || arg == "enable" || arg == "true" || arg == "1" {
        if set_room_flags_enabled(connection_id, true) {
            send_client_message(connection_id, "Room flags display enabled.");
            // Show the current room with flags
            let room_id = char.current_room_id;
            display_room(room_id, connection_id, char.name);
        } else {
            send_client_message(connection_id, "Error enabling room flags display.");
        }
        return;
    }

    if arg == "off" || arg == "disable" || arg == "false" || arg == "0" {
        if set_room_flags_enabled(connection_id, false) {
            send_client_message(connection_id, "Room flags display disabled.");
        } else {
            send_client_message(connection_id, "Error disabling room flags display.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: roomflags on|off");
}
