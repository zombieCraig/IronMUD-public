// scripts/commands/junk.rhai
// junk <item> - Permanently destroy an item from inventory or equipment
// junk all - Permanently destroy all items from inventory
// junk all.<keyword> - Permanently destroy all matching items from inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Junk what?");
        return;
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(args);
    if all_parsed.is_all {
        junk_all_items(char, all_parsed.keyword, connection_id);
        return;
    }

    // Single item junk - check inventory first, then equipped items
    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    let is_equipped = false;

    // If not in inventory, check equipped items (including wielded/held)
    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(args, eq_items);
        if item != () {
            is_equipped = true;
        }
    }

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    if item.flags.no_drop && !char.god_mode {
        send_client_message(connection_id, "You can't get rid of that!");
        return;
    }

    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot junk quest items.");
        return;
    }

    // Check no_remove flag for equipped items
    if is_equipped && item.flags.no_remove && !char.god_mode {
        send_client_message(connection_id, "You can't remove " + item.name + "!");
        return;
    }

    // Fire on_junk trigger - can cancel the action
    let trigger_result = fire_item_trigger(item.id, "on_junk", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id
    });
    if trigger_result == "cancel" {
        return;
    }

    let item_name = item.name;
    delete_item(item.id);
    send_client_message(connection_id, "You junk " + item_name + ".");
    broadcast_to_room(char.current_room_id, char.name + " junks " + item_name + ".", char.name);
}

fn junk_all_items(char, filter_keyword, connection_id) {
    let inv_items = get_items_in_inventory(char.name);

    // Filter by keyword if provided
    let items_to_junk = [];
    if filter_keyword == "" {
        items_to_junk = inv_items;
    } else {
        items_to_junk = find_items_by_keyword(filter_keyword, inv_items);
    }

    if items_to_junk.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "You don't have anything to junk.");
        } else {
            send_client_message(connection_id, "You don't have any " + filter_keyword + ".");
        }
        return;
    }

    let junked = 0;
    let skipped = 0;

    for item in items_to_junk {
        // Skip no_drop items
        if item.flags.no_drop && !char.god_mode {
            skipped += 1;
            continue;
        }

        // Skip quest items
        if item.flags.quest_item && !char.god_mode {
            skipped += 1;
            continue;
        }

        // Fire on_junk trigger - can cancel the action
        let trigger_result = fire_item_trigger(item.id, "on_junk", connection_id, #{
            "char_name": char.name,
            "room_id": char.current_room_id
        });
        if trigger_result == "cancel" {
            skipped += 1;
            continue;
        }

        delete_item(item.id);
        junked += 1;
    }

    // Build result message
    if junked > 0 {
        if junked == 1 {
            send_client_message(connection_id, "You junk 1 item.");
        } else {
            send_client_message(connection_id, "You junk " + junked + " items.");
        }
        broadcast_to_room(char.current_room_id, char.name + " junks several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't junk any of those items.");
    }
}
