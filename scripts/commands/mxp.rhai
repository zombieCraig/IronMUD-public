// scripts/commands/mxp.rhai
// Toggle MXP (MUD eXtension Protocol) support for clickable links
// Usage: mxp [on|off]

fn run_command(args, connection_id) {
    let current = is_mxp_enabled(connection_id);

    if args == "" {
        // Show current status
        let status = if current { "enabled" } else { "disabled" };
        send_client_message(connection_id, "MXP is currently " + status + ".");
        send_client_message(connection_id, "Usage: mxp on|off");
        if !current {
            send_client_message(connection_id, "Note: Your client must support MXP for clickable links to work.");
        }
        return;
    }

    let arg = args.to_lower();

    if arg == "on" || arg == "enable" || arg == "true" || arg == "1" {
        if set_mxp_enabled(connection_id, true) {
            send_client_message(connection_id, "MXP enabled. Clickable links are now active.");
            // Send a test link so users can verify it works
            send_mxp_message(connection_id, "Test link: " + mxp_send("look", "[Click here to look]"));
        } else {
            send_client_message(connection_id, "Error enabling MXP.");
        }
        return;
    }

    if arg == "off" || arg == "disable" || arg == "false" || arg == "0" {
        if set_mxp_enabled(connection_id, false) {
            send_client_message(connection_id, "MXP disabled. Plain text mode.");
        } else {
            send_client_message(connection_id, "Error disabling MXP.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: mxp on|off");
}
