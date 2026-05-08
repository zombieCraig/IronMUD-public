// scripts/commands/quest.rhai
// quest / quest list / quest <name> / quest abandon <name>

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let trimmed = args;
    let parts = [];
    for tok in trimmed.split(" ") {
        if tok != "" { parts.push(tok); }
    }

    if parts.len() == 0 || parts[0] == "list" {
        list_quests(char.name, connection_id);
        return;
    }

    if parts[0] == "abandon" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Abandon which quest?");
            return;
        }
        let needle = "";
        let i = 1;
        while i < parts.len() {
            if needle == "" { needle = parts[i]; }
            else { needle = needle + " " + parts[i]; }
            i += 1;
        }
        let vnum = resolve_quest_keyword(needle);
        if vnum == "" {
            send_client_message(connection_id, "No such quest: " + needle);
            return;
        }
        let err = quest_abandon(char.name, vnum);
        if err == "" {
            send_client_message(connection_id, "[ Abandoned quest. Progress lost. ]");
        } else {
            send_client_message(connection_id, err);
        }
        return;
    }

    // quest <keyword-or-vnum>: detail view
    let needle = parts[0];
    let i = 1;
    while i < parts.len() {
        needle = needle + " " + parts[i];
        i += 1;
    }
    show_quest_detail(char.name, needle, connection_id);
}

fn list_quests(char_name, connection_id) {
    let entries = quest_format_progress(char_name);
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

fn show_quest_detail(char_name, needle, connection_id) {
    let detail = quest_show_detail(char_name, needle);
    if detail.len() == 0 {
        send_client_message(connection_id, "No such quest: " + needle);
        return;
    }
    let out = "\x1b[1;36m" + detail.name + "\x1b[0m";
    if detail.summary != "" {
        out += " — " + detail.summary;
    }
    out += "\n";
    if detail.description != "" {
        out += detail.description + "\n";
    }
    let status_label = "";
    if detail.status == "active" { status_label = "\x1b[1;33m[ active ]\x1b[0m"; }
    else if detail.status == "completed" { status_label = "\x1b[1;32m[ completed ]\x1b[0m"; }
    else { status_label = "[ not started ]"; }
    out += "Status: " + status_label;
    if detail.repeatable { out += " (repeatable)"; }
    out += "\n";

    if detail.status == "active" && detail.progress.len() > 0 {
        out += "Objectives:\n";
        for line in detail.progress {
            out += "  " + line + "\n";
        }
    }

    if detail.rewards.len() > 0 {
        out += "Rewards:\n";
        for line in detail.rewards {
            out += "  " + line + "\n";
        }
    }

    send_client_message(connection_id, out);
}
