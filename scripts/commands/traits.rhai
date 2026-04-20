// scripts/commands/traits.rhai
// Spend remaining trait points after character creation

fn run_command(args, connection_id) {
    let mode = get_olc_mode(connection_id);

    // If in trait selection mode, handle input
    if mode == "traits_select" {
        handle_trait_input(args, connection_id);
        return;
    }

    // Must be logged in
    let char_data = get_player_character(connection_id);
    if char_data == () {
        send_client_message(connection_id, "You must be logged in to use this command.");
        return;
    }

    // Show trait selection screen
    show_trait_selection(connection_id, char_data);
    set_olc_mode(connection_id, "traits_select");
}

fn show_trait_selection(connection_id, char_data) {
    let output = "\n=== Trait Selection (Points: " + char_data.trait_points + ") ===\n\n";

    // Show current traits
    output += "Your traits: " + format_traits(char_data.traits) + "\n\n";

    if char_data.trait_points <= 0 {
        output += "You have no trait points remaining.\n";
        output += "\nType 'back' to exit.\n> ";
        send_client_message(connection_id, output);
        return;
    }

    // Show available traits
    let traits = get_trait_list();
    if traits.len() == 0 {
        output += "(No traits available yet)\n";
    } else {
        output += "Available Traits:\n";
        let idx = 1;
        for trait_id in traits {
            let info = get_trait_info(trait_id);
            let cost_str = "";
            let cost_val = info["cost"];
            if cost_val > 0 {
                cost_str = "-" + cost_val + " pts";
            } else if cost_val < 0 {
                let gain = -cost_val;
                cost_str = "+" + gain + " pts";
            } else {
                cost_str = "0 pts";
            }
            let line = "  [" + idx + "] ";
            line += info["name"];
            line += " (" + cost_str + ") - ";
            line += info["description"] + "\n";
            output += line;
            idx += 1;
        }
    }

    output += "\nWARNING: Traits are PERMANENT once accepted!\n";
    output += "\nCommands:\n";
    output += "  <number> - Preview trait details\n";
    output += "  accept <number> - PERMANENTLY add trait\n";
    output += "  back - Exit trait selection\n";
    output += "\n> ";

    send_client_message(connection_id, output);
}

fn handle_trait_input(input, connection_id) {
    let lower = "";
    for ch in input.chars() {
        if ch >= 'A' && ch <= 'Z' {
            lower += ch + 32;
        } else {
            lower += ch;
        }
    }

    if lower == "back" || lower == "exit" || lower == "quit" {
        set_olc_mode(connection_id, "");
        send_client_message(connection_id, "Exited trait selection.\n");
        return;
    }

    // Check for "accept N" command
    if lower.index_of("accept ") == 0 {
        let num_str = lower.sub_string(7, lower.len() - 7);
        let idx = parse_int(num_str);
        if idx != () {
            accept_trait(connection_id, idx);
        } else {
            send_client_message(connection_id, "Usage: accept <number>\n> ");
        }
        return;
    }

    // Try to preview a trait by number
    let idx = parse_int(lower);
    if idx != () {
        preview_trait(connection_id, idx);
        return;
    }

    send_client_message(connection_id, "Commands: <number> to preview, 'accept <number>' to add trait, 'back' to exit\n> ");
}

fn preview_trait(connection_id, idx) {
    let traits = get_trait_list();
    if idx < 1 || idx > traits.len() {
        send_client_message(connection_id, "Invalid trait number.\n> ");
        return;
    }

    let trait_id = traits[idx - 1];
    let info = get_trait_info(trait_id);

    let output = "\n--- " + info["name"] + " ---\n";
    output += info["description"] + "\n";
    output += "Category: " + info["category"] + "\n";
    if info["cost"] > 0 {
        output += "Cost: " + info["cost"] + " points\n";
    } else if info["cost"] < 0 {
        output += "Grants: " + (-info["cost"]) + " points\n";
    }
    output += "\nTo accept this trait permanently, type: accept " + idx + "\n> ";

    send_client_message(connection_id, output);
}

fn accept_trait(connection_id, idx) {
    let traits = get_trait_list();
    if idx < 1 || idx > traits.len() {
        send_client_message(connection_id, "Invalid trait number.\n> ");
        return;
    }

    let trait_id = traits[idx - 1];
    let info = get_trait_info(trait_id);

    let char_data = get_player_character(connection_id);
    if char_data == () {
        send_client_message(connection_id, "Error: Not logged in.\n");
        set_olc_mode(connection_id, "");
        return;
    }

    // Check if already has this trait
    for t in char_data.traits {
        if t == trait_id {
            send_client_message(connection_id, "You already have the " + info["name"] + " trait.\n> ");
            return;
        }
    }

    // Check if can afford
    let new_points = char_data.trait_points - info["cost"];
    if new_points < 0 {
        send_client_message(connection_id, "Not enough trait points! You need " + info["cost"] + " but only have " + char_data.trait_points + ".\n> ");
        return;
    }

    // Check conflicts
    let conflicts = info["conflicts_with"];
    let has_conflict = false;
    let conflict_msg = "";
    for c in conflicts {
        for t in char_data.traits {
            if t == c {
                let conflict_info = get_trait_info(c);
                conflict_msg = info["name"] + " conflicts with your ";
                conflict_msg += conflict_info["name"] + " trait.\n> ";
                has_conflict = true;
            }
        }
    }
    if has_conflict {
        send_client_message(connection_id, conflict_msg);
        return;
    }

    // Add the trait
    let new_traits = char_data.traits;
    new_traits.push(trait_id);
    char_data.traits = new_traits;
    char_data.trait_points = new_points;
    save_character_data(char_data);

    // Also update session character
    set_player_character(connection_id, char_data);

    send_client_message(connection_id, "\nTrait '" + info["name"] + "' PERMANENTLY added! Points remaining: " + new_points);

    // Refresh the trait selection screen
    let updated_char = get_player_character(connection_id);
    show_trait_selection(connection_id, updated_char);
}

fn format_traits(traits) {
    if traits.len() == 0 { return "(none)"; }
    let result = "";
    for t in traits {
        if result != "" { result += ", "; }
        let info = get_trait_info(t);
        if info["name"] != () && info["name"] != "" {
            result += info["name"];
        } else {
            result += t;
        }
    }
    return result;
}
