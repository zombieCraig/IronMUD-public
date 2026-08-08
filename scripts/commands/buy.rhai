// scripts/commands/buy.rhai
// buy <item> - Buy an item from a shopkeeper, vending machine or consignment
//              broker in the room. All three share one numbering, so `buy <n>`
//              means whatever `list` printed against that number.

import "text" as text;

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

    // The consignment shelf shares `list`'s numbering, so it has to share
    // `buy`'s too: the shop's own entries are walked first and anything past
    // them lands on the shelf. Resolved inside each branch rather than by an
    // early return here — a shopkeeper in the room used to make every shelf in
    // it unbuyable.
    let broker = find_consignment_broker_in_room(char.current_room_id);

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
        buy_from_shopkeeper(shopkeeper, char, broker, args, connection_id);
        return;
    }

    // Next, check for a vending machine
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        buy_from_vending(vending, char, broker, args, connection_id);
        return;
    }

    // Consignment broker: goods other players left here, at their prices.
    if broker != () {
        buy_from_consignment(broker, char, 1, args, connection_id);
        return;
    }

    send_client_message(connection_id, "There is no shopkeeper or vending machine here.");
}

fn buy_from_shopkeeper(shopkeeper, char, broker, args, connection_id) {
    let sell_rate = shop_rate_for(shopkeeper.shop_sell_rate, char.name, shopkeeper.id, true);

    // Check if args is a numeric index
    let target_index = try_parse_int(args);
    if target_index != () && target_index > 0 {
        handle_buy_by_index(shopkeeper, char, broker, target_index, connection_id);
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

    // Not in the shop's own stock. The shelf is keyed by list position rather
    // than by keyword — two players can have the same item out at two prices —
    // so a keyword miss says so rather than guessing.
    if broker != () && consignments_at(broker.id).len() > 0 {
        send_client_message(
            connection_id,
            shopkeeper.name + " doesn't have that for sale. Check the consignment shelf in 'list' and buy by number.",
        );
        return;
    }
    send_client_message(connection_id, shopkeeper.name + " doesn't have that for sale.");
}

fn handle_buy_by_index(shopkeeper, char, broker, target_index, connection_id) {
    let sell_rate = shop_rate_for(shopkeeper.shop_sell_rate, char.name, shopkeeper.id, true);
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

    // Past the shop's own entries the numbering continues onto the consignment
    // shelf, exactly as `list` printed it. `current_index` is one past the last
    // shop row, which is the shelf's first number.
    if broker != () {
        buy_from_consignment(broker, char, current_index, "" + target_index, connection_id);
        return;
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
    notify_achievement_counter(char.name, "gold.spent", price);
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
    notify_achievement_counter(char.name, "gold.spent", price);
    remove_from_shop_inventory(shopkeeper.id, item.id);
    move_item_to_inventory(item.id, char.name);

    let msg = "You buy " + item.name;
    msg += " for " + price + " gold.";
    send_client_message(connection_id, msg);
    broadcast_to_room(char.current_room_id, char.name + " buys " + item.name + ".", char.name);
}

fn buy_from_vending(vending, char, broker, args, connection_id) {
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
        // Past the machine's own rows the numbering continues onto any
        // consignment shelf in the room, as `list` printed it.
        if broker != () {
            buy_from_consignment(broker, char, current_index, "" + target_index, connection_id);
            return;
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
    notify_achievement_counter(char.name, "gold.spent", price);
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

// Buying off the shelf is by list position, not by keyword: two players can
// have the same item out at two prices, and a keyword cannot say which one the
// buyer meant.
//
// `start_index` is the number `list` printed against the shelf's first row. It
// is 1 at a bare broker and one-past-the-shop's-last-row when the broker shares
// a counter with a shopkeeper or a vending machine.
fn buy_from_consignment(broker, char, start_index, args, connection_id) {
    let listings = consignments_at(broker.id);
    if listings.len() == 0 {
        send_client_message(connection_id, broker.name + " has nothing on consignment right now.");
        return;
    }

    // Normalised for the same reason `consignments` and `standing` are: the
    // dispatcher does not trim, and Rhai has no `trim`.
    let index = try_parse_int(text::normalise(args));
    if index == () {
        send_client_message(connection_id, "Buy which one? Use the number from 'list'.");
        return;
    }
    let offset = index - start_index + 1;
    if offset < 1 || offset > listings.len() {
        send_client_message(connection_id, "Buy which one? Use the number from 'list'.");
        return;
    }

    let outcome = buy_consignment(char.name, listings[offset - 1].id);
    send_client_message(connection_id, outcome.message);
    if outcome.ok {
        broadcast_to_room(char.current_room_id, char.name + " buys something from " + broker.name + ".", char.name);
    }
}
