// scripts/commands/appraise.rhai
// appraise <item> - Ask a shopkeeper what they'll pay for an item

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Appraise what? Use 'inventory' to see what you're carrying.");
        return;
    }

    // Check for vending machine first and reject
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        send_client_message(connection_id, "Vending machines don't buy items.");
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

    // Find item in inventory
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

    // Check if shopkeeper buys this type of item
    if !shop_will_buy_type(shopkeeper.id, item.item_type) {
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

    send_client_message(connection_id, shopkeeper.name + " offers " + price + " gold for " + item.name + ".");
}
