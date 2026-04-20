// scripts/commands/motd.rhai
// Admin command to manage the Message of the Day

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
    let subcommand = if parts.len() > 0 && parts[0] != "" {
        parts[0].to_lower()
    } else {
        ""
    };

    if subcommand == "" || subcommand == "help" {
        show_help(connection_id);
        return;
    }

    if subcommand == "show" {
        show_motd(connection_id);
        return;
    }

    if subcommand == "edit" {
        enter_motd_editor(connection_id);
        return;
    }

    if subcommand == "clear" {
        clear_motd(connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand. Type 'motd help' for commands.");
}

fn show_help(connection_id) {
    let help = "=== MOTD (Message of the Day) ===\n";
    help += "The MOTD is displayed to players after they log in.\n\n";
    help += "Commands:\n";
    help += "  motd show    - Display current MOTD\n";
    help += "  motd edit    - Edit MOTD with multi-line editor\n";
    help += "  motd clear   - Remove the MOTD\n";
    help += "  motd help    - Show this help\n";
    send_client_message(connection_id, help);
}

fn show_motd(connection_id) {
    let motd = get_setting("motd");
    if motd == "" {
        send_client_message(connection_id, "No MOTD is currently set.");
        return;
    }
    let output = "=== Current MOTD ===\n" + motd + "\n====================";
    send_client_message(connection_id, output);
}

fn enter_motd_editor(connection_id) {
    set_olc_mode(connection_id, "collecting_motd");
    let existing = get_setting("motd");
    let lines = if existing != "" { existing.split("\n") } else { [] };
    set_olc_buffer(connection_id, lines);

    let output = "=== MOTD Editor ===\n";
    output += if lines.len() == 0 { "(empty)\n" } else { format_buffer_lines(lines) };
    output += "\nCommands: .h help | .l list | .s /old/new/ | . save | @ cancel\n";
    output += "Or type text to append new lines.\n";

    send_client_message(connection_id, output);
}

fn clear_motd(connection_id) {
    let current = get_setting("motd");
    if current == "" {
        send_client_message(connection_id, "No MOTD is currently set.");
        return;
    }
    set_setting("motd", "");
    send_client_message(connection_id, "MOTD has been cleared.");
}

fn format_line_num(n) {
    if n < 10 { return "  " + n + ": "; }
    if n < 100 { return " " + n + ": "; }
    return n + ": ";
}

fn format_buffer_lines(lines) {
    let output = "";
    for i in 0..lines.len() {
        output += format_line_num(i + 1) + lines[i] + "\n";
    }
    return output;
}
