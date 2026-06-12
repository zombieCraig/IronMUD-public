// scripts/commands/buy.rhai
// buy <item> - Buy an item from a shopkeeper or vending machine in the room

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Buy what? Use 'list' to see what's for sale.");
        return;
    }

    // First, check for a shopkeeper
    let shopkeeper = find_shopkeeper_in_room(char.current_room_id);
    if shopkeeper != () {
        let activity = get_mobile_activity(shopkeeper.id);
        if activity != "working" {
            if activity == "sleeping" {
                send_client_message(connection_id, shopkeeper.name + " is sleeping and can't serve you right now.");
            } else {
                send_client_message(connection_id, shopkeeper.name + " is not available for business right now.");
            }
            return;
        }
        buy_from_shopkeeper(shopkeeper, char, args, connection_id);
        return;
    }

    // Next, check for a vending machine
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        buy_from_vending(vending, char, args, connection_id);
        return;
    }

    send_client_message(connection_id, "There is no shopkeeper or vending machine here.");
}

fn buy_from_shopkeeper(shopkeeper, char, args, connection_id) {
    let sell_rate = shop_charisma_rate(shopkeeper.shop_sell_rate, get_effective_charisma(char.name), true, shopkeeper.flags.no_charm);

    // Check if args is a numeric index
    let target_index = try_parse_int(args);
    if target_index != () && target_index > 0 {
        handle_buy_by_index(shopkeeper, char, target_index, connection_id);
        return;
    }

    let keyword = args.to_lower();

    // Search stock first (to match list order)
    let stock = shopkeeper.shop_stock;
    for vnum in stock {
        let proto = get_item_by_vnum(vnum);
        if proto != () && matches_keywords(keyword, proto) {
            buy_stock_item(shopkeeper, char, vnum, sell_rate, connection_id);
            return;
        }
    }

    // Then search shop inventory (items bought from players - finite)
    let inventory = get_shop_inventory(shopkeeper.id);
    for item in inventory {
        if matches_keywords(keyword, item) {
            buy_inventory_item(shopkeeper, char, item, sell_rate, connection_id);
            return;
        }
    }

    send_client_message(connection_id, shopkeeper.name + " doesn't have that for sale.");
}

fn handle_buy_by_index(shopkeeper, char, target_index, connection_id) {
    let sell_rate = shop_charisma_rate(shopkeeper.shop_sell_rate, get_effective_charisma(char.name), true, shopkeeper.flags.no_charm);
    let stock = shopkeeper.shop_stock;
    let inventory = get_shop_inventory(shopkeeper.id);
    let current_index = 1;

    // Search base stock
    for vnum in stock {
        let proto = get_item_by_vnum(vnum);
        if proto == () { continue; } // Skip invalid items as list.rhai does
        
        if current_index == target_index {
            buy_stock_item(shopkeeper, char, vnum, sell_rate, connection_id);
            return;
        }
        current_index += 1;
    }

    // Group bought inventory (must match list.rhai grouping logic)
    let inv_groups = #{};
    let inv_keys = [];
    for item in inventory {
        let key = get_grouping_key(item);
        if key in inv_groups {
            inv_groups[key].count += 1;
        } else {
            inv_groups[key] = #{ item: item, count: 1 };
            inv_keys.push(key);
        }
    }

    // Search grouped inventory
    for key in inv_keys {
        if current_index == target_index {
            let item = inv_groups[key].item;
            buy_inventory_item(shopkeeper, char, item, sell_rate, connection_id);
            return;
        }
        current_index += 1;
    }

    send_client_message(connection_id, "There is no item with index " + target_index + " for sale.");
}

fn buy_stock_item(shopkeeper, char, vnum, sell_rate, connection_id) {
    let proto = get_item_by_vnum(vnum);
    if proto == () {
        send_client_message(connection_id, "There was a problem with that item. Please inform a builder.");
        return;
    }

    let price = calculate_sell_price(proto.value, sell_rate);

    if char.gold < price {
        let msg = "You can't afford " + proto.name;
        msg += " (costs " + price + " gold";
        msg += ", you have " + char.gold + ").";
        send_client_message(connection_id, msg);
        return;
    }

    // Spawn item from prototype and give to player
    let item = spawn_item_from_prototype(vnum);
    if item == () {
        send_client_message(connection_id, "There was a problem with that item. Please inform a builder.");
        return;
    }

    // Deduct gold and give item
    add_character_gold(char.name, -price);
    move_item_to_inventory(item.id, char.name);

    let msg = "You buy " + item.name;
    msg += " for " + price + " gold.";
    send_client_message(connection_id, msg);
    broadcast_to_room(char.current_room_id, char.name + " buys " + item.name + ".", char.name);
}

fn buy_inventory_item(shopkeeper, char, item, sell_rate, connection_id) {
    let price = calculate_sell_price(item.value, sell_rate);

    if char.gold < price {
        let msg = "You can't afford " + item.name;
        msg += " (costs " + price + " gold";
        msg += ", you have " + char.gold + ").";
        send_client_message(connection_id, msg);
        return;
    }

    // Deduct gold and transfer item
    add_character_gold(char.name, -price);
    remove_from_shop_inventory(shopkeeper.id, item.id);
    move_item_to_inventory(item.id, char.name);

    let msg = "You buy " + item.name;
    msg += " for " + price + " gold.";
    send_client_message(connection_id, msg);
    broadcast_to_room(char.current_room_id, char.name + " buys " + item.name + ".", char.name);
}

fn buy_from_vending(vending, char, args, connection_id) {
    let sell_rate = vending.vending_sell_rate;

    // Check if args is a numeric index
    let target_index = try_parse_int(args);
    if target_index != () && target_index > 0 {
        let stock = vending.vending_stock;
        let current_index = 1;
        for vnum in stock {
            let proto = get_item_by_vnum(vnum);
            if proto == () { continue; } // Skip invalid items as list.rhai does
            
            if current_index == target_index {
                perform_vending_buy(vending, char, vnum, sell_rate, connection_id);
                return;
            }
            current_index += 1;
        }
        send_client_message(connection_id, "There is no item with index " + target_index + " for sale.");
        return;
    }

    let keyword = args.to_lower();

    // Vending machines only have infinite stock
    let stock = vending.vending_stock;
    for vnum in stock {
        let proto = get_item_by_vnum(vnum);
        if proto != () && matches_keywords(keyword, proto) {
            perform_vending_buy(vending, char, vnum, sell_rate, connection_id);
            return;
        }
    }

    send_client_message(connection_id, vending.name + " doesn't have that for sale.");
}

fn perform_vending_buy(vending, char, vnum, sell_rate, connection_id) {
    let proto = get_item_by_vnum(vnum);
    if proto == () {
        send_client_message(connection_id, "There was a problem with that item. Please inform a builder.");
        return;
    }

    let price = calculate_sell_price(proto.value, sell_rate);

    if char.gold < price {
        let msg = "You can't afford " + proto.name;
        msg += " (costs " + price + " gold";
        msg += ", you have " + char.gold + ").";
        send_client_message(connection_id, msg);
        return;
    }

    // Spawn item from prototype and give to player
    let item = spawn_item_from_prototype(vnum);
    if item == () {
        send_client_message(connection_id, "There was a problem with that item. Please inform a builder.");
        return;
    }

    // Deduct gold and give item
    add_character_gold(char.name, -price);
    move_item_to_inventory(item.id, char.name);

    let msg = "You buy " + proto.name;
    msg += " from " + vending.name;
    msg += " for " + price + " gold.";
    send_client_message(connection_id, msg);
    broadcast_to_room(char.current_room_id, char.name + " buys something from " + vending.name + ".", char.name);
}

fn matches_keywords(keyword, item) {
    // Check item name
    if item.name.to_lower().contains(keyword) {
        return true;
    }
    // Check keywords list
    for kw in item.keywords {
        if kw.to_lower().contains(keyword) || keyword.contains(kw.to_lower()) {
            return true;
        }
    }
    return false;
}

fn get_grouping_key(item) {
    // Group by vnum if available, otherwise use name+value
    if item.vnum != () && item.vnum != "" {
        return item.vnum;
    }
    return item.name + "_" + item.value;
}
