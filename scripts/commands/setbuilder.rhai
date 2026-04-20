// scripts/commands/setbuilder.rhai
// Usage varies based on builder_mode setting:
// - "all": setbuilder [on|off] - toggles OWN builder status
// - "granted": setbuilder <character_name> [on|off] - admin grants/revokes (current behavior)
// - "none": setbuilder shows error

fn is_off_flag(flag) {
    if flag == "off" { return true; }
    if flag == "false" { return true; }
    if flag == "0" { return true; }
    if flag == "no" { return true; }
    return false;
}

fn is_on_flag(flag) {
    if flag == "on" { return true; }
    if flag == "true" { return true; }
    if flag == "1" { return true; }
    if flag == "yes" { return true; }
    return false;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Get builder_mode setting (default: "all" for backwards compatibility)
    let builder_mode = get_setting_or_default("builder_mode", "all");

    if builder_mode == "none" {
        send_client_message(connection_id, "Builder access management is disabled. Contact a server administrator.");
        return;
    }

    if builder_mode == "all" {
        // Self-toggle mode: setbuilder [on|off]
        let enable = true;

        if args != "" {
            let flag = args.to_lower();
            if is_off_flag(flag) {
                enable = false;
            } else if is_on_flag(flag) {
                enable = true;
            }
        } else {
            // Toggle current status
            enable = !char.is_builder;
        }

        if toggle_own_builder_flag(connection_id, enable) {
            if enable {
                send_client_message(connection_id, "You are now a builder. Use builder commands like 'dig', 'redit', etc.");
            } else {
                send_client_message(connection_id, "You are no longer a builder.");
            }
        } else {
            send_client_message(connection_id, "Error: Failed to update builder status.");
        }
        return;
    }

    // "granted" mode - admin only
    if !char.is_admin {
        send_client_message(connection_id, "Only administrators can grant or revoke builder access.");
        return;
    }

    let parts = args.split(" ");
    if parts.len() < 1 || parts[0] == "" {
        send_client_message(connection_id, "Usage: setbuilder <character_name> [on|off]\nExample: setbuilder MyCharacter on");
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

    // Try to set the builder flag
    if set_builder_flag(connection_id, target_name, enable) {
        if enable {
            send_client_message(connection_id, target_name + " is now a builder.");
        } else {
            send_client_message(connection_id, target_name + " is no longer a builder.");
        }
    } else {
        send_client_message(connection_id, "Error: Character '" + target_name + "' not found.");
    }
}
