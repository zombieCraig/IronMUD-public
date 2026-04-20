// scripts/commands/rfind.rhai
// Usage: rfind <keyword>
// Search for rooms by keyword in title or description

fn truncate_desc(desc, max_len) {
    if desc.len() <= max_len {
        return desc;
    }
    // Find last space before max_len
    let truncated = "";
    let chars = desc.chars();
    let count = 0;
    for ch in chars {
        if count >= max_len {
            break;
        }
        truncated += ch;
        count += 1;
    }
    return truncated + "...";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: rfind <keyword>\nSearches room titles and descriptions.");
        return;
    }

    let results = search_rooms(args);

    if results.len() == 0 {
        send_client_message(connection_id, "No rooms found matching: " + args);
        return;
    }

    let output = "=== Room Search: \"" + args + "\" ===\n";
    output += "Found " + results.len() + " room(s):\n\n";

    for room in results {
        output += room.title + "\n";
        let vnum_str = room.vnum;
        if vnum_str != "" {
            output += "  Vnum: " + vnum_str + "\n";
        }
        output += "  ID: " + room.id + "\n";
        let desc_preview = truncate_desc(room.description, 60);
        output += "  " + desc_preview + "\n\n";
    }

    send_client_message(connection_id, output);
}
