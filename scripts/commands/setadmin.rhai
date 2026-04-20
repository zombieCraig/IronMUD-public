// scripts/commands/setadmin.rhai
// Usage: setadmin <character_name> [on|off]
// Admin-only command to grant/revoke admin status

fn is_off_flag(flag) {
    if flag == "off" { return true; }
    if flag == "false" { return true; }
    if flag == "0" { return true; }
    if flag == "no" { return true; }
    return false;
}

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
    if parts.len() < 1 || parts[0] == "" {
        send_client_message(connection_id, "Usage: setadmin <character_name> [on|off]\nExample: setadmin MyCharacter on");
        return;
    }

    let target_name = parts[0];
    let enable = true;

    if parts.len() >= 2 {
        let flag = parts[1].to_lower();
        if is_off_flag(flag) {
            enable = false;
        }
    }

    // Prevent removing your own admin
    let target_lower = target_name.to_lower();
    let char_lower = char.name.to_lower();
    if target_lower == char_lower && !enable {
        send_client_message(connection_id, "You cannot remove your own admin status. Use the admin utility if necessary.");
        return;
    }

    if set_admin_flag(connection_id, target_name, enable) {
        if enable {
            send_client_message(connection_id, target_name + " is now an administrator.");
        } else {
            send_client_message(connection_id, target_name + " is no longer an administrator.");
        }
    } else {
        send_client_message(connection_id, "Error: Character '" + target_name + "' not found.");
    }
}
