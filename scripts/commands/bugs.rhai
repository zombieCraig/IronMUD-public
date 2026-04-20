// bugs - Admin bug report management
// Usage: bugs [list|read|approve|note|status|priority|close|delete|help]

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }
    if !char.is_admin {
        send_client_message(connection_id, "This command is for administrators only.");
        return;
    }

    if args == "" {
        show_bug_summary(connection_id);
        return;
    }

    // Parse subcommand
    let parts = split_first_word(args);
    let subcmd = parts[0];
    let rest = parts[1];

    if subcmd == "list" { handle_list(rest, connection_id); }
    else if subcmd == "read" { handle_read(rest, connection_id); }
    else if subcmd == "approve" { handle_approve(rest, connection_id, char.name); }
    else if subcmd == "note" { handle_note(rest, connection_id, char.name); }
    else if subcmd == "status" { handle_status(rest, connection_id, char.name); }
    else if subcmd == "priority" { handle_priority(rest, connection_id); }
    else if subcmd == "close" { handle_close(rest, connection_id, char.name); }
    else if subcmd == "delete" { handle_delete(rest, connection_id); }
    else if subcmd == "help" { show_help(connection_id); }
    else { send_client_message(connection_id, "Unknown subcommand. Type 'bugs help' for usage."); }
}

fn split_first_word(s) {
    let idx = 0;
    for ch in s.chars() {
        if ch == ' ' {
            return [s.sub_string(0, idx), s.sub_string(idx + 1)];
        }
        idx += 1;
    }
    return [s, ""];
}

fn show_bug_summary(connection_id) {
    let open_count = count_open_bugs();
    let all_bugs = list_bugs("all");
    let unapproved = 0;
    for b in all_bugs {
        if !b.approved {
            unapproved += 1;
        }
    }
    let msg = "\x1b[1;33m=== Bug Reports ===\x1b[0m\n";
    msg += "Open/In-Progress: " + open_count + "\n";
    msg += "Unapproved:       " + unapproved + "\n";
    msg += "Total:            " + all_bugs.len() + "\n";
    if unapproved > 0 {
        msg += "\n\x1b[1;31m" + unapproved + " report(s) awaiting approval.\x1b[0m Use 'bugs list unapproved' to review.";
    }
    msg += "\n\nType 'bugs help' for available commands.";
    send_client_message(connection_id, msg);
}

fn handle_list(filter_arg, connection_id) {
    let filter = "open";
    if filter_arg != "" {
        filter = filter_arg;
    }

    let valid_filters = ["open", "closed", "inprogress", "resolved", "all", "unapproved"];
    if !valid_filters.contains(filter.to_lower()) {
        send_client_message(connection_id, "Valid filters: open, closed, inprogress, resolved, all, unapproved");
        return;
    }

    let bugs = list_bugs(filter);
    if bugs.len() == 0 {
        send_client_message(connection_id, "No bug reports found for filter: " + filter);
        return;
    }

    let msg = "\x1b[1;33m=== Bug Reports (" + filter + ") ===\x1b[0m\n";
    for b in bugs {
        msg += format_bug_line(b);
    }
    send_client_message(connection_id, msg);
}

fn format_bug_line(b) {
    let approval_tag = "\x1b[1;31m[!]\x1b[0m";
    if b.approved {
        approval_tag = "\x1b[1;32m[+]\x1b[0m";
    }
    let priority_tag = "";
    if b.priority == "High" { priority_tag = " \x1b[1;33m[HIGH]\x1b[0m"; }
    if b.priority == "Critical" { priority_tag = " \x1b[1;31m[CRIT]\x1b[0m"; }

    return "  " + approval_tag + " #" + b.ticket_number + " [" + b.status + "]" + priority_tag + " " + b.reporter + ": " + b.preview + "\n";
}

fn handle_read(ticket_str, connection_id) {
    if ticket_str == "" {
        send_client_message(connection_id, "Usage: bugs read <ticket#>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    let bug = get_bug_by_ticket(ticket);
    if bug == () {
        send_client_message(connection_id, "Bug report #" + ticket_str + " not found.");
        return;
    }

    let msg = format_bug_detail(bug);
    msg += format_bug_context(bug);
    msg += format_bug_notes(bug);
    send_client_message(connection_id, msg);
}

fn format_bug_detail(bug) {
    let approval_badge = "\x1b[1;31m[UNAPPROVED]\x1b[0m";
    if bug.approved {
        approval_badge = "\x1b[1;32m[APPROVED]\x1b[0m";
    }

    let msg = "\x1b[1;33m=== Bug Report #" + bug.ticket_number + " ===\x1b[0m\n";
    msg += "Status:   " + bug.status + " | Priority: " + bug.priority + " | " + approval_badge + "\n";
    msg += "Reporter: " + bug.reporter + "\n";
    msg += "Filed:    " + format_timestamp(bug.created_at) + "\n";
    if bug.resolved_by != () {
        msg += "Resolved: " + bug.resolved_by + " at " + format_timestamp(bug.resolved_at) + "\n";
    }
    msg += "\n\x1b[1mDescription:\x1b[0m\n" + bug.description + "\n";
    return msg;
}

fn format_bug_context(bug) {
    let ctx = bug.context;
    let msg = "\n\x1b[1mCapture Context:\x1b[0m\n";
    if ctx.room_title != "" {
        msg += "  Room:     " + ctx.room_title;
        if ctx.room_vnum != "" { msg += " [" + ctx.room_vnum + "]"; }
        msg += "\n";
    }
    if ctx.character_class != "" { msg += "  Class:    " + ctx.character_class + " (Lv " + ctx.character_level + ")\n"; }
    if ctx.character_race != "" { msg += "  Race:     " + ctx.character_race + "\n"; }
    msg += "  HP:       " + ctx.hp + "/" + ctx.max_hp;
    if ctx.max_mana > 0 { msg += " | Mana: " + ctx.mana + "/" + ctx.max_mana; }
    msg += "\n";
    if ctx.in_combat { msg += "  \x1b[1;31mIn Combat\x1b[0m\n"; }
    msg += "  Position: " + ctx.character_position + "\n";
    if ctx.game_time != "" { msg += "  Time:     " + ctx.game_time + "\n"; }
    if ctx.season != "" { msg += "  Season:   " + ctx.season + " | Weather: " + ctx.weather + "\n"; }
    if ctx.players_in_room.len() > 0 { msg += "  Players:  " + join_strings(ctx.players_in_room, ", ") + "\n"; }
    if ctx.mobiles_in_room.len() > 0 { msg += "  Mobiles:  " + join_strings(ctx.mobiles_in_room, ", ") + "\n"; }
    return msg;
}

fn format_bug_notes(bug) {
    if bug.admin_notes.len() == 0 {
        return "";
    }
    let msg = "\n\x1b[1mAdmin Notes:\x1b[0m\n";
    for note in bug.admin_notes {
        msg += "  [" + format_timestamp(note.created_at) + "] " + note.author + ": " + note.message + "\n";
    }
    return msg;
}

fn join_strings(arr, sep) {
    let result = "";
    let first = true;
    for s in arr {
        if !first { result += sep; }
        result += s;
        first = false;
    }
    return result;
}

fn format_timestamp(ts) {
    if ts == () { return "N/A"; }
    if ts == 0 { return "N/A"; }
    // Simple relative formatting - just show the unix timestamp
    // In a real implementation this could be fancier
    return "" + ts;
}

fn handle_approve(ticket_str, connection_id, admin_name) {
    if ticket_str == "" {
        send_client_message(connection_id, "Usage: bugs approve <ticket#>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    let bug = get_bug_by_ticket(ticket);
    if bug == () {
        send_client_message(connection_id, "Bug report #" + ticket_str + " not found.");
        return;
    }
    if bug.approved {
        send_client_message(connection_id, "Bug report #" + ticket_str + " is already approved.");
        return;
    }

    if approve_bug(ticket) {
        send_client_message(connection_id, "\x1b[1;32mBug report #" + ticket_str + " approved.\x1b[0m Now visible to MCP/API services.");
    } else {
        send_client_message(connection_id, "Failed to approve bug report #" + ticket_str + ".");
    }
}

fn handle_note(rest, connection_id, admin_name) {
    let parts = split_first_word(rest);
    let ticket_str = parts[0];
    let message = parts[1];

    if ticket_str == "" || message == "" {
        send_client_message(connection_id, "Usage: bugs note <ticket#> <message>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    if add_bug_note(ticket, admin_name, message) {
        send_client_message(connection_id, "Note added to bug report #" + ticket_str + ".");
    } else {
        send_client_message(connection_id, "Failed to add note. Bug report #" + ticket_str + " not found.");
    }
}

fn handle_status(rest, connection_id, admin_name) {
    let parts = split_first_word(rest);
    let ticket_str = parts[0];
    let new_status = parts[1];

    if ticket_str == "" || new_status == "" {
        send_client_message(connection_id, "Usage: bugs status <ticket#> <open|inprogress|resolved|closed>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    let valid = ["open", "inprogress", "resolved", "closed"];
    if !valid.contains(new_status.to_lower()) {
        send_client_message(connection_id, "Valid statuses: open, inprogress, resolved, closed");
        return;
    }

    let bug = get_bug_by_ticket(ticket);
    if bug == () {
        send_client_message(connection_id, "Bug report #" + ticket_str + " not found.");
        return;
    }

    if set_bug_status(ticket, new_status) {
        send_client_message(connection_id, "Bug report #" + ticket_str + " status changed to " + new_status + ".");
        // Notify reporter
        notify_player_bug_update(bug.reporter, ticket, new_status);
    } else {
        send_client_message(connection_id, "Failed to update status.");
    }
}

fn handle_priority(rest, connection_id) {
    let parts = split_first_word(rest);
    let ticket_str = parts[0];
    let new_priority = parts[1];

    if ticket_str == "" || new_priority == "" {
        send_client_message(connection_id, "Usage: bugs priority <ticket#> <low|normal|high|critical>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    let valid = ["low", "normal", "high", "critical"];
    if !valid.contains(new_priority.to_lower()) {
        send_client_message(connection_id, "Valid priorities: low, normal, high, critical");
        return;
    }

    if set_bug_priority(ticket, new_priority) {
        send_client_message(connection_id, "Bug report #" + ticket_str + " priority changed to " + new_priority + ".");
    } else {
        send_client_message(connection_id, "Failed to update priority. Bug report not found.");
    }
}

fn handle_close(rest, connection_id, admin_name) {
    let parts = split_first_word(rest);
    let ticket_str = parts[0];
    let note = parts[1];

    if ticket_str == "" {
        send_client_message(connection_id, "Usage: bugs close <ticket#> [resolution note]");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    let bug = get_bug_by_ticket(ticket);
    if bug == () {
        send_client_message(connection_id, "Bug report #" + ticket_str + " not found.");
        return;
    }

    if close_bug(ticket, admin_name, note) {
        send_client_message(connection_id, "\x1b[1;32mBug report #" + ticket_str + " closed.\x1b[0m");
        notify_player_bug_update(bug.reporter, ticket, "Closed");
    } else {
        send_client_message(connection_id, "Failed to close bug report.");
    }
}

fn handle_delete(ticket_str, connection_id) {
    if ticket_str == "" {
        send_client_message(connection_id, "Usage: bugs delete <ticket#>");
        return;
    }
    let ticket = parse_int(ticket_str);
    if ticket <= 0 {
        send_client_message(connection_id, "Invalid ticket number.");
        return;
    }

    if delete_bug(ticket) {
        send_client_message(connection_id, "Bug report #" + ticket_str + " deleted.");
    } else {
        send_client_message(connection_id, "Failed to delete. Bug report #" + ticket_str + " not found.");
    }
}

fn show_help(connection_id) {
    let msg = "\x1b[1;33m=== Bug Report Commands ===\x1b[0m\n";
    msg += "  bugs                       - Show open/unapproved counts\n";
    msg += "  bugs list [filter]         - List bugs (open/closed/inprogress/resolved/all/unapproved)\n";
    msg += "  bugs read <#>              - View full report with context and notes\n";
    msg += "  bugs approve <#>           - Approve report (makes visible to MCP/API)\n";
    msg += "  bugs note <#> <message>    - Add admin note\n";
    msg += "  bugs status <#> <status>   - Change status (open/inprogress/resolved/closed)\n";
    msg += "  bugs priority <#> <pri>    - Change priority (low/normal/high/critical)\n";
    msg += "  bugs close <#> [note]      - Close with optional resolution note\n";
    msg += "  bugs delete <#>            - Delete report\n";
    msg += "\n\x1b[1mApproval:\x1b[0m Reports must be approved before MCP/API services can see them.\n";
    msg += "  [!] = unapproved, [+] = approved";
    send_client_message(connection_id, msg);
}
