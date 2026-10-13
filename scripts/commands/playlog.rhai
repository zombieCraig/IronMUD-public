// scripts/commands/playlog.rhai
//
// Admin-only review of the player activity log (see src/player_log.rs).
//
//   playlog                      recent finished sessions, all players
//   playlog <name>               that player's recent sessions
//   playlog <name> session [k]   full log of the k-th most recent session
//                                (0 = latest, including one still live)
//   playlog <name> tail [n]      last n entries across sessions (default 50)

fn format_duration(secs) {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        return hours + "h" + mins + "m";
    }
    if mins > 0 {
        return mins + "m";
    }
    return secs + "s";
}

fn parse_count(parts, idx, fallback) {
    if parts.len() > idx && parts[idx] != "" {
        let n = try_parse_int(parts[idx]);
        if n != () && n >= 0 {
            return n;
        }
    }
    return fallback;
}

fn format_entry(e) {
    let line = format_unix_time(e.ts) + " ";
    if e.kind == "command" {
        line += "[" + e.room + "] > " + e.text;
        if e.outcome != "ok" {
            line += "   (" + e.outcome + ")";
        }
        return line;
    }
    return line + "*** " + e.kind + " " + e.text;
}

fn show_sessions(connection_id, name) {
    let sessions = list_player_sessions(name, 20);
    if sessions.len() == 0 {
        let who = if name == "" { "anyone" } else { name };
        send_client_message(connection_id, "No finished sessions recorded for " + who + ".");
        return;
    }
    let out = "Recent sessions (newest first):\n";
    for s in sessions {
        out += "  " + format_unix_time(s.started_at) + "  " + s.name + "  " + format_duration(s.seconds)
            + "  " + s.commands + " cmds";
        if s.unknown_commands > 0 {
            out += " (" + s.unknown_commands + " unknown)";
        }
        out += "  " + s.end_reason + "  " + s.ip;
        if s.client != "" {
            out += " " + s.client;
        }
        out += "\n";
    }
    if name != "" {
        out += "Use 'playlog " + name + " session [k]' to read one (0 = latest).\n";
    }
    send_client_message(connection_id, out);
}

fn show_entries(connection_id, title, entries) {
    if entries.len() == 0 {
        send_client_message(connection_id, "Nothing logged.");
        return;
    }
    let out = title + " (" + entries.len() + " entries):\n";
    for e in entries {
        out += "  " + format_entry(e) + "\n";
    }
    send_client_message(connection_id, out);
}

fn run_command(args, connection_id) {
    let parts = args.split(" ");
    let name = if parts.len() > 0 { parts[0] } else { "" };
    if name == "" {
        show_sessions(connection_id, "");
        return;
    }
    let mode = if parts.len() > 1 { parts[1] } else { "" };
    if mode == "" {
        show_sessions(connection_id, name);
        return;
    }
    if mode == "session" {
        let k = parse_count(parts, 2, 0);
        show_entries(connection_id, "Session " + k + " for " + name, get_player_session_log(name, k));
        return;
    }
    if mode == "tail" {
        let n = parse_count(parts, 2, 50);
        show_entries(connection_id, "Last entries for " + name, get_player_log(name, n));
        return;
    }
    send_client_message(connection_id, "Usage: playlog [<name> [session [k] | tail [n]]]");
}
