// scripts/commands/list.rhai
// list - Show items for sale at a shopkeeper or vending machine in the room

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
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
        show_shopkeeper_list(shopkeeper, char, connection_id);
        return;
    }

    // Next, check for a vending machine
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        show_vending_list(vending, char, connection_id);
        return;
    }

    send_client_message(connection_id, "There is no shopkeeper or vending machine here.");
}

fn show_shopkeeper_list(shopkeeper, char, connection_id) {
    let sell_rate = shopkeeper.shop_sell_rate;
    let stock = shopkeeper.shop_stock;
    let inventory = get_shop_inventory(shopkeeper.id);

    let output = "=== " + shopkeeper.name + " ===\n";
    output += "Items for sale:\n\n";

    let has_items = false;

    // Show base stock
    for vnum in stock {
        let line = format_stock_item(vnum, sell_rate);
        if line != "" {
            output += line;
            has_items = true;
        }
    }

    // Group bought inventory by prototype_vnum (or name+value fallback)
    let inv_groups = #{};
    for item in inventory {
        let key = get_grouping_key(item);
        if key in inv_groups {
            inv_groups[key].count += 1;
        } else {
            inv_groups[key] = #{ item: item, count: 1 };
        }
    }

    // Show grouped bought inventory
    for key in inv_groups.keys() {
        let group = inv_groups[key];
        let line = format_inv_item(group.item, sell_rate, group.count);
        output += line;
        has_items = true;
    }

    if !has_items {
        output += "  (nothing for sale)\n";
    }

    output += "\nYour gold: " + char.gold;
    output += "\n\nUse 'buy <item>' to purchase.";

    send_client_message(connection_id, output);
}

fn show_vending_list(vending, char, connection_id) {
    let sell_rate = vending.vending_sell_rate;
    let stock = vending.vending_stock;

    let output = "=== " + vending.name + " ===\n";
    output += "Items for sale:\n\n";

    let has_items = false;

    // Vending machines only have infinite stock
    for vnum in stock {
        let line = format_stock_item(vnum, sell_rate);
        if line != "" {
            output += line;
            has_items = true;
        }
    }

    if !has_items {
        output += "  (nothing for sale)\n";
    }

    output += "\nYour gold: " + char.gold;
    output += "\n\nUse 'buy <item>' to purchase.";

    send_client_message(connection_id, output);
}

fn format_stock_item(vnum, sell_rate) {
    let proto = get_item_by_vnum(vnum);
    if proto == () {
        return "";
    }
    let price = calculate_sell_price(proto.value, sell_rate);
    let suffix = get_item_info_suffix(proto);
    return "  " + proto.name + " - " + price + " gold" + suffix + "\n";
}

fn format_inv_item(item, sell_rate, count) {
    let price = calculate_sell_price(item.value, sell_rate);
    let suffix = get_item_info_suffix(item);
    if count > 1 {
        return "  " + item.name + " - " + price + " gold" + suffix + " (used, x" + count + ")\n";
    }
    return "  " + item.name + " - " + price + " gold" + suffix + " (used)\n";
}

fn get_grouping_key(item) {
    // Group by vnum if available, otherwise use name+value
    if item.vnum != () && item.vnum != "" {
        return item.vnum;
    }
    return item.name + "_" + item.value;
}

fn get_item_info_suffix(item) {
    if !item.is_medical_tool { return ""; }
    let tier_name = "Tier " + item.medical_tier;
    if item.medical_tier == 1 { tier_name = "Basic"; }
    if item.medical_tier == 2 { tier_name = "Intermediate"; }
    if item.medical_tier == 3 { tier_name = "Advanced"; }
    return " [" + tier_name + "]";
}
