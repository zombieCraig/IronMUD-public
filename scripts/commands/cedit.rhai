// scripts/commands/cedit.rhai
// Class kit editor — admins author per-class starting gold + item kits.
// Backed by the `class_loadouts` sled tree (see src/script/classes.rs);
// edits overlay onto the JSON-loaded ClassDefinition at startup.
//
// Usage:
//   cedit list                        — list all classes with kit summary
//   cedit <class>                     — show one class's kit
//   cedit <class> show                — same
//   cedit <class> gold <amount>       — set starting gold (>= 0)
//   cedit <class> items add <vnum>    — add an item vnum to the kit
//   cedit <class> items remove <vnum> — remove an item vnum from the kit
//   cedit <class> items clear         — empty the kit

import "olc_helpers" as olc;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }
    if !char.is_builder && !char.is_admin {
        send_client_message(connection_id, "You don't have permission to edit classes.");
        return;
    }

    let parts = args.split(" ");
    if args == "" || parts.len() == 0 {
        show_usage(connection_id);
        return;
    }

    let first = parts[0].to_lower();

    if first == "list" {
        handle_list(connection_id);
        return;
    }

    if first == "help" || first == "?" {
        show_usage(connection_id);
        return;
    }

    let class_id = first;
    if !class_exists(class_id) {
        send_client_message(connection_id, "No such class: " + class_id);
        send_client_message(connection_id, "Use 'cedit list' to see available classes.");
        return;
    }

    let subcommand = "";
    if parts.len() >= 2 {
        subcommand = parts[1].to_lower();
    }

    if subcommand == "" || subcommand == "show" {
        show_class_kit(class_id, connection_id);
        return;
    }

    if subcommand == "gold" {
        handle_gold(class_id, parts, connection_id);
        return;
    }

    if subcommand == "items" {
        handle_items(class_id, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
    send_client_message(connection_id, "Use 'cedit " + class_id + "' to see the kit.");
}

fn show_usage(connection_id) {
    send_client_message(connection_id, "Usage:");
    send_client_message(connection_id, "  cedit list                          List all classes");
    send_client_message(connection_id, "  cedit <class>                       Show kit for a class");
    send_client_message(connection_id, "  cedit <class> gold <amount>         Set starting gold");
    send_client_message(connection_id, "  cedit <class> items add <vnum>      Add a starting item");
    send_client_message(connection_id, "  cedit <class> items remove <vnum>   Remove a starting item");
    send_client_message(connection_id, "  cedit <class> items clear           Clear the kit");
}

fn handle_list(connection_id) {
    let ids = list_class_ids();
    if ids.len() == 0 {
        send_client_message(connection_id, "No classes are loaded.");
        return;
    }
    send_client_message(connection_id, olc::c_header("Class Starting Kits", connection_id));
    for id in ids {
        let gold = get_class_starting_gold(id);
        let items = get_class_starting_items(id);
        let item_str = "";
        if items.len() == 0 {
            item_str = "(none)";
        } else {
            item_str = olc::join_array(items);
        }
        let line = "  " + id + " — gold=" + gold + ", items=" + item_str;
        send_client_message(connection_id, line);
    }
}

fn show_class_kit(class_id, connection_id) {
    let gold = get_class_starting_gold(class_id);
    let items = get_class_starting_items(class_id);
    send_client_message(connection_id, olc::c_header("Starting kit for " + class_id, connection_id));
    send_client_message(connection_id, "  gold:  " + gold);
    if items.len() == 0 {
        send_client_message(connection_id, "  items: (none)");
    } else {
        send_client_message(connection_id, "  items:");
        for v in items {
            send_client_message(connection_id, "    - " + v);
        }
    }
}

fn handle_gold(class_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: cedit " + class_id + " gold <amount>");
        return;
    }
    let amount_str = parts[2];
    let amount = parse_int(amount_str);
    if amount == () {
        send_client_message(connection_id, "Amount must be a non-negative integer.");
        return;
    }
    if amount < 0 {
        send_client_message(connection_id, "Amount must be non-negative.");
        return;
    }
    if set_class_starting_gold(class_id, amount) {
        send_client_message(connection_id, class_id + " starting gold set to " + amount + ".");
    } else {
        send_client_message(connection_id, "Failed to set starting gold.");
    }
}

fn handle_items(class_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: cedit " + class_id + " items <add|remove|clear> [vnum]");
        return;
    }
    let action = parts[2].to_lower();
    if action == "clear" {
        if clear_class_starting_items(class_id) {
            send_client_message(connection_id, class_id + " starting items cleared.");
        } else {
            send_client_message(connection_id, "Failed to clear starting items.");
        }
        return;
    }
    if action != "add" && action != "remove" {
        send_client_message(connection_id, "Unknown items action: " + action + " (expected add, remove, or clear)");
        return;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: cedit " + class_id + " items " + action + " <vnum>");
        return;
    }
    let vnum = parts[3];
    if action == "add" {
        if !item_vnum_exists(vnum) {
            send_client_message(connection_id, "No item prototype with vnum: " + vnum);
            return;
        }
        if add_class_starting_item(class_id, vnum) {
            send_client_message(connection_id, "Added " + vnum + " to " + class_id + " starting kit.");
        } else {
            send_client_message(connection_id, "Failed to add item (already in kit?).");
        }
        return;
    }
    // remove
    if remove_class_starting_item(class_id, vnum) {
        send_client_message(connection_id, "Removed " + vnum + " from " + class_id + " starting kit.");
    } else {
        send_client_message(connection_id, vnum + " is not in " + class_id + " starting kit.");
    }
}
