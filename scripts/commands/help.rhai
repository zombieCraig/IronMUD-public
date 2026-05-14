// scripts/commands/help.rhai
// help - List available commands
// help <command> - Show details for a specific command

fn color_cmd_name(name, access, conn_id) {
    if access == "builder" {
        return color_or(ansi_cyan(name), name, conn_id);
    }
    if access == "admin" {
        return color_or(ansi_yellow(name), name, conn_id);
    }
    return name;
}

fn run_command(args, connection_id) {
    let term_width = get_terminal_width(connection_id);
    let commands = get_available_commands(connection_id);

    // If specific command requested, show details
    if args != "" {
        show_command_help(args.to_lower(), commands, connection_id);
        return;
    }

    // Build colored command names
    let cmd_names = [];
    let has_builder = false;
    let has_admin = false;
    for cmd in commands {
        cmd_names.push(color_cmd_name(cmd.name, cmd.access, connection_id));
        if cmd.access == "builder" { has_builder = true; }
        if cmd.access == "admin" { has_admin = true; }
    }

    let output = "Available commands:\n";
    output += format_columns(cmd_names, term_width, 2);

    // Legend for colored commands
    if has_builder || has_admin {
        let legend = "";
        if has_builder {
            legend += color_or(ansi_cyan("cyan"), "cyan", connection_id) + " = builder";
        }
        if has_admin {
            if legend != "" { legend += "  "; }
            legend += color_or(ansi_yellow("yellow"), "yellow", connection_id) + " = admin";
        }
        output += legend + "\n";
    }

    output += "\nUse 'help <command>' for details.";

    send_client_message(connection_id, output);
}

fn show_command_help(name, commands, connection_id) {
    // First try exact match
    for cmd in commands {
        if cmd.name == name {
            let colored = color_cmd_name(cmd.name, cmd.access, connection_id);
            let output = colored + " - " + cmd.description;
            send_client_message(connection_id, output);
            return;
        }
    }

    // If no exact match, search names and descriptions
    let matches = [];
    for cmd in commands {
        if cmd.name.contains(name) || cmd.description.to_lower().contains(name) {
            let colored = color_cmd_name(cmd.name, cmd.access, connection_id);
            matches.push(colored + " - " + cmd.description);
        }
    }

    if matches.len() > 0 {
        let output = "No exact match for '" + name + "', but found these possibilities:\n";
        for m in matches {
            output += m + "\n";
        }
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, "Unknown command: " + name);
    }
}
