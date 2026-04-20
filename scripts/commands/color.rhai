// scripts/commands/color.rhai
// Toggle ANSI color support
// Usage: color [on|off]
// @description Toggle ANSI color display
// @category settings

fn run_command(args, connection_id) {
    let current = is_colors_enabled(connection_id);

    if args == "" {
        // Show current status
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "Colors are currently " + status + ".");
        send_client_message(connection_id, "Usage: color on|off");

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

    let arg = args.to_lower();

    if arg == "on" || arg == "enable" || arg == "true" || arg == "1" {
        if set_colors_enabled(connection_id, true) {
            let msg = color_or(ansi_green("Colors enabled!"), "Colors enabled!", connection_id);
            send_client_message(connection_id, msg);
        } else {
            send_client_message(connection_id, "Error enabling colors.");
        }
        return;
    }

    if arg == "off" || arg == "disable" || arg == "false" || arg == "0" {
        if set_colors_enabled(connection_id, false) {
            send_client_message(connection_id, "Colors disabled. Plain text mode.");
        } else {
            send_client_message(connection_id, "Error disabling colors.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: color on|off");
}
