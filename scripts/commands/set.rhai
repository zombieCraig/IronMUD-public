// scripts/commands/set.rhai
// Unified settings management command
// Usage: set [setting] [on|off]
// @description Manage personal settings
// @category settings

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    let is_builder = false;
    let is_admin = false;

    if char != () {
        is_builder = char.is_builder;
        is_admin = char.is_admin;
    }

    if args == "" {
        show_settings_table(connection_id, is_builder, is_admin);
        return;
    }

    // Parse arguments
    let parts = args.split(' ');
    let setting_name = parts[0].to_lower();
    let value_arg = "";
    if parts.len() > 1 {
        value_arg = parts[1].to_lower();
    }

    // Handle each setting
    if setting_name == "mxp" {
        handle_mxp(connection_id, value_arg);
        return;
    }

    if setting_name == "color" || setting_name == "colors" {
        handle_color(connection_id, value_arg);
        return;
    }

    if setting_name == "roomflags" {
        // Permission check
        if !is_builder && !is_admin {
            send_client_message(connection_id, "The 'roomflags' setting is for builders only.");
            return;
        }
        handle_roomflags(connection_id, value_arg, char);
        return;
    }

    if setting_name == "afk" {
        handle_afk(connection_id, value_arg);
        return;
    }

    if setting_name == "builderdebug" {
        // Permission check
        if !is_builder && !is_admin {
            send_client_message(connection_id, "The 'builderdebug' setting is for builders only.");
            return;
        }
        handle_builderdebug(connection_id, value_arg);
        return;
    }

    if setting_name == "helpline" {
        handle_helpline(connection_id, value_arg);
        return;
    }

    if setting_name == "abbrev" || setting_name == "abbreviations" {
        handle_abbrev(connection_id, value_arg);
        return;
    }

    if setting_name == "summonable" {
        handle_summonable(connection_id, value_arg);
        return;
    }

    // Unknown setting
    send_client_message(connection_id, "Unknown setting: " + setting_name);
    send_client_message(connection_id, "Type 'set' to see available settings.");
}

fn show_settings_table(connection_id, is_builder, is_admin) {
    let output = "\nPersonal Settings:\n";
    output += "  " + pad_right("Setting", 14) + pad_right("Value", 10) + "Description\n";
    output += "  " + pad_right("-------", 14) + pad_right("-----", 10) + "-----------\n";

    // MXP setting (available to all)
    let mxp_val = if is_mxp_enabled(connection_id) { "on" } else { "off" };
    let mxp_toggle = make_toggle_link("set mxp", mxp_val, connection_id);
    output += "  " + pad_right("mxp", 14) + pad_right(mxp_toggle, 10);
    output += "Enable MXP clickable links\n";

    // Color setting (available to all)
    let color_val = if is_colors_enabled(connection_id) { "on" } else { "off" };
    let color_toggle = make_toggle_link("set color", color_val, connection_id);
    output += "  " + pad_right("color", 14) + pad_right(color_toggle, 10);
    output += "Enable ANSI color display\n";

    // AFK setting (available to all)
    let afk_val = if is_afk(connection_id) { "on" } else { "off" };
    let afk_toggle = make_toggle_link("set afk", afk_val, connection_id);
    output += "  " + pad_right("afk", 14) + pad_right(afk_toggle, 10);
    output += "Mark yourself as away from keyboard\n";

    // Helpline setting (available to all)
    let hl_val = if is_helpline_enabled(connection_id) { "on" } else { "off" };
    let hl_toggle = make_toggle_link("set helpline", hl_val, connection_id);
    output += "  " + pad_right("helpline", 14) + pad_right(hl_toggle, 10);
    output += "Receive medical emergency alerts\n";

    // Summonable setting (available to all)
    let sm_val = if is_summonable(connection_id) { "on" } else { "off" };
    let sm_toggle = make_toggle_link("set summonable", sm_val, connection_id);
    output += "  " + pad_right("summonable", 14) + pad_right(sm_toggle, 10);
    output += "Allow other players to summon you with the summon spell\n";

    // Abbrev setting (available to all)
    let ab_val = if is_abbrev_enabled(connection_id) { "on" } else { "off" };
    let ab_toggle = make_toggle_link("set abbrev", ab_val, connection_id);
    output += "  " + pad_right("abbrev", 14) + pad_right(ab_toggle, 10);
    output += "Enable command abbreviations (e.g., 'sc' -> 'scan')\n";

    // Roomflags setting (builder only)
    if is_builder || is_admin {
        let rf_val = if is_room_flags_enabled(connection_id) { "on" } else { "off" };
        let rf_toggle = make_toggle_link("set roomflags", rf_val, connection_id);
        output += "  " + pad_right("roomflags", 14) + pad_right(rf_toggle, 10);
        output += "Show room flags and vnums (builder)\n";

        let bd_val = if is_builder_debug_enabled(connection_id) { "on" } else { "off" };
        let bd_toggle = make_toggle_link("set builderdebug", bd_val, connection_id);
        output += "  " + pad_right("builderdebug", 14) + pad_right(bd_toggle, 10);
        output += "Receive spawn system debug messages (builder)\n";
    }

    output += "\nType 'set <setting> on|off' to change a setting.\n";

    send_mxp_message(connection_id, output);
}

fn make_toggle_link(cmd, current_value, connection_id) {
    // Create MXP toggle link if enabled
    let new_value = if current_value == "on" { "off" } else { "on" };
    let mxp_link = mxp_send(cmd + " " + new_value, "[" + current_value + "]");
    return mxp_or(mxp_link, current_value, connection_id);
}

fn pad_right(text, width) {
    let result = text;
    let padding = width - text.len();
    if padding > 0 {
        for i in 0..padding {
            result += " ";
        }
    }
    return result;
}

fn handle_mxp(connection_id, value_arg) {
    let current = is_mxp_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "MXP is currently " + status + ".");
        send_client_message(connection_id, "Usage: set mxp on|off");
        if !current {
            send_client_message(connection_id, "Note: Your client must support MXP for clickable links to work.");
        }
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_mxp_enabled(connection_id, true) {
            send_client_message(connection_id, "MXP enabled. Clickable links are now active.");
            send_mxp_message(connection_id, "Test link: " + mxp_send("look", "[Click here to look]"));
        } else {
            send_client_message(connection_id, "Error enabling MXP.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_mxp_enabled(connection_id, false) {
            send_client_message(connection_id, "MXP disabled. Plain text mode.");
        } else {
            send_client_message(connection_id, "Error disabling MXP.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set mxp on|off");
}

fn handle_color(connection_id, value_arg) {
    let current = is_colors_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Colors are currently " + status + ".");
        send_client_message(connection_id, "Usage: set color on|off");

        if current {
            // Show color preview
            let preview = "\nColor preview:";
            preview += "\n  " + color_or(ansi_cyan("Cyan"), "Cyan", connection_id) + " - Room titles";
            preview += "\n  " + color_or(ansi_white("White"), "White", connection_id) + " - Descriptions";
            preview += "\n  " + color_or(ansi_yellow("Yellow"), "Yellow", connection_id) + " - Items";
            preview += "\n  " + color_or(ansi_green("Green"), "Green", connection_id) + " - Characters/NPCs";
            preview += "\n  " + color_or(ansi_magenta("Magenta"), "Magenta", connection_id) + " - Exits";
            preview += "\n\nOLC colors:";
            preview += "\n  " + color_or(ansi_green("Green"), "Green", connection_id) + " - Headers";
            preview += "\n  " + color_or(ansi_cyan("Cyan"), "Cyan", connection_id) + " - Labels";
            preview += "\n  " + color_or(ansi_yellow("Yellow"), "Yellow", connection_id) + " - Warnings/Status";
            send_client_message(connection_id, preview);
        }
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_colors_enabled(connection_id, true) {
            let msg = color_or(ansi_green("Colors enabled!"), "Colors enabled!", connection_id);
            send_client_message(connection_id, msg);
        } else {
            send_client_message(connection_id, "Error enabling colors.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_colors_enabled(connection_id, false) {
            send_client_message(connection_id, "Colors disabled. Plain text mode.");
        } else {
            send_client_message(connection_id, "Error disabling colors.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set color on|off");
}

fn handle_roomflags(connection_id, value_arg, char) {
    let current = is_room_flags_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Room flags display is currently " + status + ".");
        send_client_message(connection_id, "Usage: set roomflags on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, room titles show vnum and flags in dark gray.");
        send_client_message(connection_id, "Example: Town Square [vnum:town:square, safe, indoors]");
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_room_flags_enabled(connection_id, true) {
            send_client_message(connection_id, "Room flags display enabled.");
            if char != () {
                let room_id = char.current_room_id;
                display_room(room_id, connection_id, char.name);
            }
        } else {
            send_client_message(connection_id, "Error enabling room flags display.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_room_flags_enabled(connection_id, false) {
            send_client_message(connection_id, "Room flags display disabled.");
        } else {
            send_client_message(connection_id, "Error disabling room flags display.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set roomflags on|off");
}

fn handle_afk(connection_id, value_arg) {
    let current = is_afk(connection_id);

    if value_arg == "" {
        // Toggle mode (for the 'afk' alias)
        let new_value = !current;
        set_afk(connection_id, new_value);
        if new_value {
            send_client_message(connection_id, "You are now AFK.");
        } else {
            send_client_message(connection_id, "You are no longer AFK.");
        }
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        set_afk(connection_id, true);
        send_client_message(connection_id, "You are now AFK.");
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        set_afk(connection_id, false);
        send_client_message(connection_id, "You are no longer AFK.");
        return;
    }

    send_client_message(connection_id, "Usage: set afk on|off (or just 'afk' to toggle)");
}

fn handle_builderdebug(connection_id, value_arg) {
    let current = is_builder_debug_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Builder debug channel is currently " + status + ".");
        send_client_message(connection_id, "Usage: set builderdebug on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, you receive debug messages from the spawn system,");
        send_client_message(connection_id, "such as warnings about items that can't be equipped.");
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_builder_debug_enabled(connection_id, true) {
            send_client_message(connection_id, "Builder debug channel enabled. You will receive spawn system messages.");
        } else {
            send_client_message(connection_id, "Error enabling builder debug channel.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_builder_debug_enabled(connection_id, false) {
            send_client_message(connection_id, "Builder debug channel disabled.");
        } else {
            send_client_message(connection_id, "Error disabling builder debug channel.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set builderdebug on|off");
}

fn handle_helpline(connection_id, value_arg) {
    let current = is_helpline_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Helpline channel is currently " + status + ".");
        send_client_message(connection_id, "Usage: set helpline on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, you receive alerts when players need medical help,");
        send_client_message(connection_id, "such as when someone is unconscious or bleeding heavily.");
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_helpline_enabled(connection_id, true) {
            send_client_message(connection_id, "Helpline channel enabled. You will receive medical emergency alerts.");
        } else {
            send_client_message(connection_id, "Error enabling helpline channel.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_helpline_enabled(connection_id, false) {
            send_client_message(connection_id, "Helpline channel disabled.");
        } else {
            send_client_message(connection_id, "Error disabling helpline channel.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set helpline on|off");
}

fn handle_abbrev(connection_id, value_arg) {
    let current = is_abbrev_enabled(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Command abbreviations are currently " + status + ".");
        send_client_message(connection_id, "Usage: set abbrev on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, partial command names are matched to full commands.");
        send_client_message(connection_id, "Example: 'sc' becomes 'scan', 'inv' becomes 'inventory'.");
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_abbrev_enabled(connection_id, true) {
            send_client_message(connection_id, "Command abbreviations enabled.");
        } else {
            send_client_message(connection_id, "Error enabling command abbreviations.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_abbrev_enabled(connection_id, false) {
            send_client_message(connection_id, "Command abbreviations disabled.");
        } else {
            send_client_message(connection_id, "Error disabling command abbreviations.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set abbrev on|off");
}

fn handle_summonable(connection_id, value_arg) {
    let current = is_summonable(connection_id);

    if value_arg == "" {
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Summonable is currently " + status + ".");
        send_client_message(connection_id, "Usage: set summonable on|off");
        send_client_message(connection_id, "");
        send_client_message(connection_id, "When enabled, other players can target you with the summon spell.");
        send_client_message(connection_id, "When disabled (default), the summon spell cannot affect you.");
        return;
    }

    if value_arg == "on" || value_arg == "enable" || value_arg == "true" || value_arg == "1" {
        if set_summonable(connection_id, true) {
            send_client_message(connection_id, "You are now summonable by other players.");
        } else {
            send_client_message(connection_id, "Error enabling summonable.");
        }
        return;
    }

    if value_arg == "off" || value_arg == "disable" || value_arg == "false" || value_arg == "0" {
        if set_summonable(connection_id, false) {
            send_client_message(connection_id, "You are no longer summonable.");
        } else {
            send_client_message(connection_id, "Error disabling summonable.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: set summonable on|off");
}
