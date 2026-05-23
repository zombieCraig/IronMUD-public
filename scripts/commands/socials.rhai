// scripts/commands/socials.rhai
// socials - List all available social commands

fn run_command(args, connection_id) {
    let names = get_social_command_names();
    if names.len() == 0 {
        send_client_message(connection_id, "No socials are loaded on this server.");
        return;
    }

    let term_width = get_terminal_width(connection_id);
    let output = "Available socials (" + names.len() + "):\n";
    output += format_columns(names, term_width, 2);
    output += "\nUsage: <social> [target]   e.g. wave, bow bob, smile";
    send_client_message(connection_id, output);
}
