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
    // Rhai 1.17 does not register `trim` on strings; rely on the dispatcher
    // having stripped leading whitespace, and treat empty input as no-arg.
    let name = args;
    if name == "" {
        let char = get_player_character(connection_id);
        if char != () && char.is_admin {
            show_recent_logins(connection_id);
            return;
        }
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

// Admin-only: `last` with no argument. We only ever have one timestamp per
// account (last_login_at gets overwritten on every login, no session
// history), so this is a most-recent-per-account activity list, not a true
// multi-session Unix `last` log.
fn show_recent_logins(connection_id) {
    let accounts = list_all_accounts();

    // Never-logged-in accounts have nothing to report; drop them before sorting.
    let seen = [];
    for account in accounts {
        if account.last_login_at > 0 {
            seen.push(account);
        }
    }

    // Selection sort by last_login_at, descending. Account counts are small
    // enough that O(n^2) is fine and keeps this readable in Rhai.
    let n = seen.len();
    let i = 0;
    while i < n {
        let max_idx = i;
        let j = i + 1;
        while j < n {
            if seen[j].last_login_at > seen[max_idx].last_login_at {
                max_idx = j;
            }
            j += 1;
        }
        if max_idx != i {
            let tmp = seen[i];
            seen[i] = seen[max_idx];
            seen[max_idx] = tmp;
        }
        i += 1;
    }

    let online = get_online_players();

    let output = "=== Recent Logins (" + n + " account(s) with activity) ===\n\n";
    let limit = 25;
    let idx = 0;
    for account in seen {
        if idx >= limit {
            output += "\n... " + (n - limit) + " more not shown.\n";
            break;
        }

        let who = account.name;
        if account.character_names.len() > 0 {
            who += " (" + account.character_names[0] + ")";
        }

        let is_online = false;
        for cn in account.character_names {
            for p in online {
                if p.name.to_lower() == cn.to_lower() {
                    is_online = true;
                }
            }
        }

        if is_online {
            output += "  " + who + " — online now\n";
        } else {
            output += "  " + who + " — " + format_unix_time(account.last_login_at) + "\n";
        }
        idx += 1;
    }

    send_client_message(connection_id, output);
}
