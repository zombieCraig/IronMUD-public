// scripts/commands/sell.rhai
// sell <item> - Sell an item to a shopkeeper in the room
// sell all - Sell all sellable items to a shopkeeper
// sell all.<keyword> - Sell all matching items to a shopkeeper

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Sell what? Use 'inventory' to see what you're carrying.");
        return;
    }

    // Check for vending machine first and reject
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        send_client_message(connection_id, "You can't sell to a vending machine.");
        return;
    }

    let shopkeeper = find_shopkeeper_in_room(char.current_room_id);
    if shopkeeper == () {
        send_client_message(connection_id, "There is no shopkeeper here.");
        return;
    }

    let activity = get_mobile_activity(shopkeeper.id);
    if activity != "working" {
        if activity == "sleeping" {
            send_client_message(connection_id, shopkeeper.name + " is sleeping and can't serve you right now.");
        } else {
            send_client_message(connection_id, shopkeeper.name + " is not available for business right now.");
        }
        return;
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(args);
    if all_parsed.is_all {
        sell_all_items(char, shopkeeper, all_parsed.keyword, connection_id);
        return;
    }

    // Single item sell
    let keyword = args.to_lower();
    let item = find_item_in_inventory(char.name, keyword);

    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    // Check if item can be sold
    if item.flags.no_sell && !char.god_mode {
        send_client_message(connection_id, "You can't sell " + item.name + ".");
        return;
    }

    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot sell quest items.");
        return;
    }

    if item.value <= 0 {
        send_client_message(connection_id, shopkeeper.name + " isn't interested in " + item.name + ".");
        return;
    }

    // Check if shopkeeper will buy this item (type, category, value range, preset)
    if !shop_will_buy_item(shopkeeper.id, item.id) {
        send_client_message(connection_id, shopkeeper.name + " doesn't buy that type of item.");
        return;
    }

    // Calculate price
    let buy_rate = shopkeeper.shop_buy_rate;
    let price = calculate_buy_price(item.value, buy_rate);

    if price <= 0 {
        send_client_message(connection_id, shopkeeper.name + " isn't interested in " + item.name + ".");
        return;
    }

    // Remove item from player inventory and add to shop inventory
    move_item_to_nowhere(item.id);
    add_to_shop_inventory(shopkeeper.id, item.id);
    add_character_gold(char.name, price);

    send_client_message(connection_id, "You sell " + item.name + " for " + price + " gold.");
    broadcast_to_room(char.current_room_id, char.name + " sells " + item.name + ".", char.name);
}

fn sell_all_items(char, shopkeeper, filter_keyword, connection_id) {
    let inv_items = get_items_in_inventory(char.name);

    // Filter by keyword if provided
    let items_to_sell = [];
    if filter_keyword == "" {
        items_to_sell = inv_items;
    } else {
        items_to_sell = find_items_by_keyword(filter_keyword, inv_items);
    }

    if items_to_sell.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "You don't have anything to sell.");
        } else {
            send_client_message(connection_id, "You don't have any " + filter_keyword + ".");
        }
        return;
    }

    let sold = 0;
    let total_gold = 0;
    let skipped = 0;

    for item in items_to_sell {
        // Skip no_sell items
        if item.flags.no_sell && !char.god_mode {
            skipped += 1;
            continue;
        }

        // Skip quest items
        if item.flags.quest_item && !char.god_mode {
            skipped += 1;
            continue;
        }

        // Skip worthless items
        if item.value <= 0 {
            skipped += 1;
            continue;
        }

        // Check if shopkeeper will buy this item
        if !shop_will_buy_item(shopkeeper.id, item.id) {
            skipped += 1;
            continue;
        }

        // Calculate price
        let buy_rate = shopkeeper.shop_buy_rate;
        let price = calculate_buy_price(item.value, buy_rate);

        if price <= 0 {
            skipped += 1;
            continue;
        }

        // Sell the item
        move_item_to_nowhere(item.id);
        add_to_shop_inventory(shopkeeper.id, item.id);
        total_gold += price;
        sold += 1;
    }

    // Add all gold at once
    if total_gold > 0 {
        add_character_gold(char.name, total_gold);
    }

    // Build result message
    if sold > 0 {
        if sold == 1 {
            send_client_message(connection_id, "You sell 1 item for " + total_gold + " gold.");
        } else {
            send_client_message(connection_id, "You sell " + sold + " items for " + total_gold + " gold.");
        }
        broadcast_to_room(char.current_room_id, char.name + " sells several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, shopkeeper.name + " isn't interested in any of those items.");
    }
}
