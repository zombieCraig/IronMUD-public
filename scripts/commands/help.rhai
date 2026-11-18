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
            let output = colored + " - " + cmd.description + contextual_help_extra(cmd.name, connection_id);
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

fn knows_codex_spell(name) {
    for id in get_eligible_spells(name) {
        if id.starts_with("cx_") { return true; }
    }
    return false;
}

// Extra help lines that only make sense for some players. Kept out of
// commands.json so the average player's help stays lean.
fn contextual_help_extra(name, connection_id) {
    if name == "pray" {
        let char = get_player_character(connection_id);
        let out = "";
        if char != () && spellcraft_enabled() && get_worship_god(char.name) != "" {
            out += "\n  pray petition - in a temple, ask your god to shape a spell (shows what you still need)";
            out += "\n  pray ritual   - once your trial is done, begin the rite that seals the spell";
            out += "\n  pray restore <spell> - in a temple, ask your god to re-light a faded spell";
        }
        if char != () && spellcraft_enabled() && ritual_in_view(connection_id) {
            out += "\n  pray push     - at a pause in your rite: deepen the gift, at a risk";
            out += "\n  pray seal     - at a pause in your rite: inscribe the spell";
            out += "\n  pray aid <name> - lend mana to someone's rite and steady their focus";
        }
        return out;
    }
    if name == "craft" {
        let char = get_player_character(connection_id);
        if char != () && spellcraft_enabled() && knows_codex_spell(char.name) {
            return "\n  craft scroll <spell> - ink a spell of the gods you know onto a blank scroll";
        }
        return "";
    }
    if name == "worship" {
        let char = get_player_character(connection_id);
        if char != () && spellcraft_enabled() && get_worship_god(char.name) != "" {
            return "\n  worship renounce - in a temple: renounce your god (it strikes you, and never forgets)";
        }
        return "";
    }
    return "";
}
