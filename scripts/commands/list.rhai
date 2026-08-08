// scripts/commands/list.rhai
// list - Show items for sale at a shopkeeper or vending machine in the room

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // A consignment broker is the third kind of counter, and it does not
    // displace the other two: a mob can be both a shopkeeper and a broker, and
    // a broker can simply be standing next to an unrelated merchant. So the
    // shelf is *appended* to whichever page we build, under one continuous
    // numbering that `buy <n>` reads the same way.
    //
    // This used to be an early return per branch, which meant any shopkeeper in
    // the room made every shelf in it unreachable — including the shelf of a
    // mob that was itself the shopkeeper, which the builder guide documents as
    // supported.
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
        show_shopkeeper_list(shopkeeper, char, broker, connection_id);
        return;
    }

    // Next, check for a vending machine
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        show_vending_list(vending, char, broker, connection_id);
        return;
    }

    if broker != () {
        show_consignment_only(broker, connection_id);
        return;
    }

    send_client_message(connection_id, "There is no shopkeeper or vending machine here.");
}

fn show_shopkeeper_list(shopkeeper, char, broker, connection_id) {
    let sell_rate = shop_rate_for(shopkeeper.shop_sell_rate, char.name, shopkeeper.id, true);
    let stock = shopkeeper.shop_stock;
    let inventory = get_shop_inventory(shopkeeper.id);

    let output = "=== " + shopkeeper.name + " ===\n";
    output += "Items for sale:\n\n";

    let has_items = false;
    let index = 1;

    // Show base stock
    for vnum in stock {
        let line = format_stock_item(index, vnum, sell_rate);
        if line != "" {
            output += line;
            has_items = true;
            index += 1;
        }
    }

    // Group bought inventory by prototype_vnum (or name+value fallback)
    let inv_groups = #{};
    let inv_keys = []; // To keep track of order
    for item in inventory {
        let key = get_grouping_key(item);
        if key in inv_groups {
            inv_groups[key].count += 1;
        } else {
            inv_groups[key] = #{ item: item, count: 1 };
            inv_keys.push(key);
        }
    }

    // Show grouped bought inventory
    for key in inv_keys {
        let group = inv_groups[key];
        let line = format_inv_item(index, group.item, sell_rate, group.count);
        output += line;
        has_items = true;
        index += 1;
    }

    if !has_items {
        output += "  (nothing for sale)\n";
    }

    output += consignment_section(broker, index);

    output += "\nYour gold: " + char.gold;
    output += "\n\nUse 'buy <item>' or 'buy <index>' to purchase.";

    send_client_message(connection_id, output);
}

fn show_vending_list(vending, char, broker, connection_id) {
    let sell_rate = vending.vending_sell_rate;
    let stock = vending.vending_stock;

    let output = "=== " + vending.name + " ===\n";
    output += "Items for sale:\n\n";

    let has_items = false;
    let index = 1;

    // Vending machines only have infinite stock
    for vnum in stock {
        let line = format_stock_item(index, vnum, sell_rate);
        if line != "" {
            output += line;
            has_items = true;
            index += 1;
        }
    }

    if !has_items {
        output += "  (nothing for sale)\n";
    }

    output += consignment_section(broker, index);

    output += "\nYour gold: " + char.gold;
    output += "\n\nUse 'buy <item>' or 'buy <index>' to purchase.";

    send_client_message(connection_id, output);
}

fn format_stock_item(index, vnum, sell_rate) {
    let proto = get_item_by_vnum(vnum);
    if proto == () {
        return "";
    }
    let price = calculate_sell_price(proto.value, sell_rate);
    let suffix = get_item_info_suffix(proto);
    let idx_str = index.to_string();
    if index < 10 { idx_str = " " + idx_str; }
    return idx_str + ") " + proto.name + " - " + price + " gold" + suffix + "\n";
}

fn format_inv_item(index, item, sell_rate, count) {
    let price = calculate_sell_price(item.value, sell_rate);
    let suffix = get_item_info_suffix(item);
    let idx_str = index.to_string();
    if index < 10 { idx_str = " " + idx_str; }
    let name = item.name;
    if count > 1 {
        return idx_str + ") " + name + " - " + price + " gold" + suffix + " (used, x" + count + ")\n";
    }
    return idx_str + ") " + name + " - " + price + " gold" + suffix + " (used)\n";
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

// The consignment shelf, rendered as a section of a larger page.
//
// `start_index` is where the enclosing page's numbering left off, so the shelf
// continues it rather than restarting at 1 — `buy <n>` resolves the same way,
// walking the shop's own entries first and handing anything past them here.
// Returns "" when there is no broker or nothing on the shelf, so a caller can
// append unconditionally.
fn consignment_section(broker, start_index) {
    if broker == () {
        return "";
    }
    let listings = consignments_at(broker.id);
    if listings.len() == 0 {
        return "";
    }

    let output = "\nOn consignment at " + broker.name + ":\n";
    let n = start_index;
    for listing in listings {
        output += "  " + n + ". " + listing.item_name + " — " + listing.price + " gold (from " + listing.seller + ")\n";
        n += 1;
    }
    return output;
}

// A broker with no shop or vending machine beside it gets a page of its own.
fn show_consignment_only(broker, connection_id) {
    let section = consignment_section(broker, 1);
    if section == "" {
        send_client_message(connection_id, broker.name + " has nothing on consignment right now.");
        return;
    }
    send_client_message(connection_id, "=== " + broker.name + " ===" + section + "\nUse 'buy <number>' to take one.");
}
