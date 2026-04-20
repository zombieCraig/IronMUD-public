// scripts/commands/ready.rhai
// Ready ammunition or equipment in the Ready slot

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Can't manage equipment while sleeping
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }

    let trimmed = args.to_lower();

    // No args - show currently readied item
    if trimmed == "" {
        show_readied_item(char.name, connection_id);
        return;
    }

    // "remove" - unready the item
    if trimmed == "remove" || trimmed == "unready" {
        unready_item(char, connection_id);
        return;
    }

    // Ready an item from inventory
    ready_item(char, trimmed, connection_id);
}

// Show what's currently in the Ready slot
fn show_readied_item(char_name, connection_id) {
    let readied = get_readied_item(char_name);
    if readied == () {
        send_client_message(connection_id, "You have nothing readied.");
        return;
    }

    let msg = "Readied: " + readied.name;
    if readied.is_ammunition && readied.ammo_count > 0 {
        msg += " (" + readied.ammo_count + " remaining)";
    }
    send_client_message(connection_id, msg);
}

// Remove readied item back to inventory
fn unready_item(char, connection_id) {
    let readied = get_readied_item(char.name);
    if readied == () {
        send_client_message(connection_id, "You have nothing readied.");
        return;
    }

    if move_item_to_inventory(readied.id, char.name) {
        send_client_message(connection_id, "You put away " + readied.name + ".");
        broadcast_to_room_awake(char.current_room_id,
            char.name + " puts away " + readied.name + ".", char.name);
    } else {
        send_client_message(connection_id, "Failed to unready " + readied.name + ".");
    }
}

// Ready an item from inventory
fn ready_item(char, keyword, connection_id) {
    // Check if something is already readied
    let current = get_readied_item(char.name);
    if current != () {
        send_client_message(connection_id, "You already have " + current.name + " readied. Use `ready remove` first.");
        return;
    }

    // Find item in inventory
    let inventory = get_items_in_inventory(char.name);
    let found_item = ();

    for item in inventory {
        if matches_keyword(item, keyword) {
            found_item = item;
            break;
        }
    }

    if found_item == () {
        send_client_message(connection_id, "You don't have '" + keyword + "' in your inventory.");
        return;
    }

    // Check if the item can be worn in the Ready slot
    if !can_wear_at_location(found_item, "ready") {
        send_client_message(connection_id, found_item.name + " cannot be readied.");
        return;
    }

    // Equip to Ready slot
    if move_item_to_equipped(found_item.id, char.name) {
        send_client_message(connection_id, "You ready " + found_item.name + ".");
        broadcast_to_room_awake(char.current_room_id,
            char.name + " readies " + found_item.name + ".", char.name);
    } else {
        send_client_message(connection_id, "Failed to ready " + found_item.name + ".");
    }
}

// Check if item matches a keyword
fn matches_keyword(item, keyword) {
    // Check item name
    if item.name.to_lower().contains(keyword) { return true; }
    // Check keywords
    let kws = item.keywords;
    for kw in kws {
        if kw.to_lower() == keyword || kw.to_lower().contains(keyword) { return true; }
    }
    return false;
}

// Check if item has the ready wear location
fn can_wear_at_location(item, location) {
    let locs = item.wear_locations;
    for loc in locs {
        if loc.to_lower() == location { return true; }
    }
    return false;
}
