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
    let keyword = args.to_lower();
    let sell_rate = shopkeeper.shop_sell_rate;

    // First, search shop inventory (items bought from players - finite)
    let inventory = get_shop_inventory(shopkeeper.id);
    for item in inventory {
        if matches_keywords(keyword, item) {
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
            return;
        }
    }

    // Next, search base stock (prototypes - infinite)
    let stock = shopkeeper.shop_stock;
    for vnum in stock {
        let proto = get_item_by_vnum(vnum);
        if proto != () && matches_keywords(keyword, proto) {
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
            return;
        }
    }

    send_client_message(connection_id, shopkeeper.name + " doesn't have that for sale.");
}

fn buy_from_vending(vending, char, args, connection_id) {
    let keyword = args.to_lower();
    let sell_rate = vending.vending_sell_rate;

    // Vending machines only have infinite stock
    let stock = vending.vending_stock;
    for vnum in stock {
        let proto = get_item_by_vnum(vnum);
        if proto != () && matches_keywords(keyword, proto) {
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
            return;
        }
    }

    send_client_message(connection_id, vending.name + " doesn't have that for sale.");
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
