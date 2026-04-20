// scripts/commands/drop.rhai
// drop <item> - Drop an item from inventory or equipment
// drop all - Drop all items from inventory (skips equipped items)
// drop all.<keyword> - Drop all matching items from inventory
// drop <amount> gold - Drop gold from your balance

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if is_on_tour(connection_id) {
        send_client_message(connection_id, "You cannot drop items while on a property tour.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Drop what?");
        return;
    }

    // Check for "drop <amount> gold" syntax
    let parts = args.split(" ");
    if parts.len() >= 2 {
        let last_word = parts[parts.len() - 1].to_lower();
        if last_word == "gold" || last_word == "coins" {
            let amount_str = parts[0];
            let amount = parse_int(amount_str);
            if amount != () && amount > 0 {
                drop_gold_from_balance(char, amount, connection_id);
                return;
            }
        }
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(args);
    if all_parsed.is_all {
        drop_all_items(char, all_parsed.keyword, connection_id);
        return;
    }

    // Single item drop - check inventory first, then equipped items
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
        send_client_message(connection_id, "You can't drop that!");
        return;
    }

    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot drop quest items.");
        return;
    }

    // Check no_remove flag for equipped items
    if is_equipped && item.flags.no_remove && !char.god_mode {
        send_client_message(connection_id, "You can't remove " + item.name + "!");
        return;
    }

    // Fire on_drop trigger - can cancel the drop
    let trigger_result = fire_item_trigger(item.id, "on_drop", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id
    });
    if trigger_result == "cancel" {
        return;
    }

    move_item_to_room(item.id, char.current_room_id);
    send_client_message(connection_id, "You drop " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " drops " + item.name + ".", char.name);
}

fn drop_all_items(char, filter_keyword, connection_id) {
    let inv_items = get_items_in_inventory(char.name);

    // Filter by keyword if provided
    let items_to_drop = [];
    if filter_keyword == "" {
        items_to_drop = inv_items;
    } else {
        items_to_drop = find_items_by_keyword(filter_keyword, inv_items);
    }

    if items_to_drop.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "You don't have anything to drop.");
        } else {
            send_client_message(connection_id, "You don't have any " + filter_keyword + ".");
        }
        return;
    }

    let dropped = 0;
    let skipped = 0;

    for item in items_to_drop {
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

        // Fire on_drop trigger - can cancel the drop
        let trigger_result = fire_item_trigger(item.id, "on_drop", connection_id, #{
            "char_name": char.name,
            "room_id": char.current_room_id
        });
        if trigger_result == "cancel" {
            skipped += 1;
            continue;
        }

        move_item_to_room(item.id, char.current_room_id);
        dropped += 1;
    }

    // Build result message
    if dropped > 0 {
        if dropped == 1 {
            send_client_message(connection_id, "You drop 1 item.");
        } else {
            send_client_message(connection_id, "You drop " + dropped + " items.");
        }
        broadcast_to_room(char.current_room_id, char.name + " drops several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't drop any of those items.");
    }
}

fn drop_gold_from_balance(char, amount, connection_id) {
    let current_gold = get_character_gold(char.name);

    if current_gold < amount {
        if current_gold <= 0 {
            send_client_message(connection_id, "You don't have any gold.");
        } else {
            send_client_message(connection_id, "You only have " + current_gold + " gold.");
        }
        return;
    }

    // Subtract from balance
    add_character_gold(char.name, -amount);

    // Spawn gold pile in room (auto-merges with existing)
    let gold = spawn_gold_in_room(amount, char.current_room_id);
    if gold == () {
        // Refund on failure
        add_character_gold(char.name, amount);
        send_client_message(connection_id, "Failed to drop gold.");
        return;
    }

    let desc = get_gold_description(amount);
    send_client_message(connection_id, "You drop " + amount + " gold coins.");
    broadcast_to_room(char.current_room_id, char.name + " drops some gold.", char.name);
}
