// scripts/commands/tattoo.rhai
// Admin tattoo management.
//   tattoo list <player>            - list a character's permanent tattoos
//   tattoo remove <player> <index>  - strip the tattoo at <index> (1-based)
//
// Repairs marks left behind by mis-keyworded tattoo items. A room entry gate
// (e.g. projfu:the_fu_gateway_entrance) matches a character's tattoo keywords,
// so a tattoo applied from an item with empty keywords never satisfies it;
// removing the broken mark lets the player re-apply a fixed copy cleanly.
// Backed by get_character_tattoos / admin_remove_tattoo in src/script/tattoos.rs.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }
    if !char.is_admin {
        send_client_message(connection_id, "This command requires admin access.");
        return;
    }

    let parts = args.split(" ");
    if parts.len() < 1 || parts[0] == "" {
        send_tattoo_usage(connection_id);
        return;
    }

    let sub = parts[0].to_lower();
    if sub == "list" {
        if parts.len() < 2 || parts[1] == "" {
            send_client_message(connection_id, "Usage: tattoo list <player>");
            return;
        }
        show_tattoos(connection_id, parts[1]);
        return;
    }
    if sub == "remove" {
        if parts.len() < 3 || parts[1] == "" || parts[2] == "" {
            send_client_message(connection_id, "Usage: tattoo remove <player> <index>");
            return;
        }
        remove_player_tattoo(connection_id, parts[1], parts[2]);
        return;
    }

    send_tattoo_usage(connection_id);
}

fn send_tattoo_usage(connection_id) {
    send_client_message(connection_id,
        "Admin tattoo management:\n" +
        "  tattoo list <player>            - list a character's tattoos\n" +
        "  tattoo remove <player> <index>  - remove the tattoo at <index> (1-based)");
}

fn show_tattoos(connection_id, target_name) {
    let target = get_character_data(target_name);
    if target == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }
    let tattoos = get_character_tattoos(target.name);
    if tattoos.len() == 0 {
        send_client_message(connection_id, target.name + " has no tattoos.");
        return;
    }
    let output = "Tattoos on " + target.name + ":\n";
    let i = 0;
    for t in tattoos {
        i += 1;
        output += "  " + i + ". [" + t.location + "] " + t.short_desc;
        let kws = t.keywords;
        if kws.len() > 0 {
            output += " (keywords: " + kws.join(", ") + ")";
        } else {
            output += " (keywords: none)";
        }
        output += "\n";
    }
    send_client_message(connection_id, output);
}

fn remove_player_tattoo(connection_id, target_name, index_str) {
    let target = get_character_data(target_name);
    if target == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    let display_index = try_parse_int(index_str);
    if display_index == () || display_index < 1 {
        send_client_message(connection_id,
            "Index must be a positive number. Use 'tattoo list <player>' to see indexes.");
        return;
    }

    let tattoos = get_character_tattoos(target.name);
    if display_index > tattoos.len() {
        send_client_message(connection_id,
            target.name + " has only " + tattoos.len() + " tattoo(s).");
        return;
    }

    // Resolve the short description by walking the list so we never index a
    // dynamic element directly; the listing is 1-based, the binding 0-based.
    let removed_desc = "the tattoo";
    let i = 0;
    for t in tattoos {
        i += 1;
        if i == display_index {
            removed_desc = t.short_desc;
        }
    }

    if admin_remove_tattoo(target.name, display_index - 1) {
        send_client_message(connection_id,
            "Removed " + removed_desc + " from " + target.name + ".");
    } else {
        send_client_message(connection_id,
            "Failed to remove tattoo " + display_index + " from " + target.name + ".");
    }
}
