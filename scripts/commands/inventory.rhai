// scripts/commands/inventory.rhai
// inventory - List items you're carrying

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let items = get_items_in_inventory(char.name);

    if items.len() == 0 {
        // Still show encumbrance even with empty inventory
        let enc_line = build_encumbrance_line(char.name, connection_id);
        send_client_message(connection_id, "You aren't carrying anything.\n" + enc_line);
        return;
    }

    let output = "You are carrying:\n";
    for item in items {
        output += "  " + build_item_line(item) + "\n";
    }

    // Add encumbrance summary
    output += "\n" + build_encumbrance_line(char.name, connection_id);

    send_client_message(connection_id, output);
}

fn build_item_line(item) {
    let line = item.name;

    // Add weight
    if item.weight > 0 {
        line += " (" + item.weight + " lbs)";
    }

    // Show ammo count for ammunition
    if item.is_ammunition && item.ammo_count > 0 {
        line += " (" + item.ammo_count + " rounds)";
    }

    // Check if it's a container with contents
    if item.is_container {
        let contents = get_items_in_container(item.id);
        if contents.len() > 0 {
            line += " [" + contents.len() + " items]";
        }
    }

    return line;
}

fn build_encumbrance_line(char_name, connection_id) {
    let current = get_character_carry_weight(char_name);
    let max = get_player_max_carry_weight(connection_id);
    let percent = get_encumbrance_percent(char_name);
    let level = get_encumbrance_level(char_name);

    // Capitalize level for display
    let level_display = capitalize_first(level);

    let line = "Carrying: " + current + "/" + max + " lbs (" + percent + "%) - " + level_display;

    return line;
}

fn capitalize_first(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1);
    return first.to_upper() + rest;
}
