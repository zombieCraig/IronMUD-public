// scripts/commands/quests.rhai
// `quests` is an alias for `quest list`.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }
    let entries = quest_format_progress(char.name);
    if entries.len() == 0 {
        send_client_message(connection_id, "You have no active quests.");
        return;
    }
    let out = "Active quests:\n";
    for entry in entries {
        out += "\x1b[1;36m" + entry.name + "\x1b[0m";
        if entry.summary != "" {
            out += " — " + entry.summary;
        }
        out += "\n";
        for line in entry.lines {
            out += "  " + line + "\n";
        }
    }
    out += "\nUse `quest <name>` for details, `quest abandon <name>` to drop one.";
    send_client_message(connection_id, out);
}
