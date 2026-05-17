// scripts/commands/last.rhai
//
// `last <character>` — show when a player was last online and how much total
// time they've played. Resolves the character name to its owning account row
// for `last_login_at`; the per-character `total_seconds_played` field comes
// from the character row directly.
//
// Player-facing: never surfaces IP, email, or account name.

fn format_play_time(secs) {
    if secs <= 0 {
        return "less than a minute";
    }
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        return hours + "h " + mins + "m";
    }
    if mins > 0 {
        return mins + "m";
    }
    return "less than a minute";
}

fn run_command(args, connection_id) {
    let name = args.trim();
    if name == "" {
        send_client_message(connection_id, "Usage: last <character>");
        return;
    }

    // Reverse-lookup account by character name. () means no such character
    // has ever logged in here (or never finished creation).
    let account = get_account_for_character(name);
    if account == () {
        send_client_message(connection_id,
            "No character named '" + name + "' has played here.");
        return;
    }

    // Resolve the canonical character name (case-insensitive lookup).
    let char = get_character_data(name);
    let display_name = if char != () { char.name } else { name };

    // Online check uses the same source as `who`. Invisibility is not
    // honored here — `last` reports presence, not identity-level visibility.
    let online = get_online_players();
    let is_online = false;
    for p in online {
        if p.name.to_lower() == display_name.to_lower() {
            is_online = true;
            break;
        }
    }

    let output = "";
    if is_online {
        output += display_name + " is currently online.\n";
    } else if account.last_login_at == 0 {
        output += display_name + " has not logged in yet.\n";
    } else {
        output += display_name + " was last seen "
            + format_unix_time(account.last_login_at) + ".\n";
    }

    if char != () && char.total_seconds_played > 0 {
        output += "Total play time: " + format_play_time(char.total_seconds_played) + ".";
    }

    send_client_message(connection_id, output);
}
