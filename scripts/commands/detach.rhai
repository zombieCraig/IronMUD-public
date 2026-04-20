// scripts/commands/detach.rhai
// Detach an attachment from a ranged weapon
// Usage: detach <attachment_keyword> <weapon_keyword>

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () { send_client_message(connection_id, "You must be logged in."); return; }
    if char.position == "sleeping" { send_client_message(connection_id, "You are asleep!"); return; }

    let parts = split_args(args);
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: detach <attachment> <weapon>");
        return;
    }

    let att_keyword = parts[0];
    let wpn_keyword = parts[1];

    // Find weapon: check wielded first, then inventory
    let weapon = find_weapon_by_keyword(char.name, wpn_keyword);
    if weapon == () {
        send_client_message(connection_id, "You don't have a weapon matching '" + wpn_keyword + "'.");
        return;
    }

    // Find attachment in weapon's container_contents
    let attachment = find_attachment_in_weapon(weapon, att_keyword);
    if attachment == () {
        send_client_message(connection_id, weapon.name + " doesn't have an attachment matching '" + att_keyword + "'.");
        return;
    }

    // Detach: remove from container, move to inventory
    remove_item_from_container(attachment.id);
    move_item_to_inventory(attachment.id, char.name);
    send_client_message(connection_id, "You detach " + attachment.name + " from " + weapon.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " detaches something from " + weapon.name + ".", char.name);
}

fn split_args(args) {
    let result = [];
    let current = "";
    for ch in args.to_chars() {
        if ch == ' ' {
            if current != "" {
                result.push(current);
                current = "";
            }
        } else {
            current += ch;
        }
    }
    if current != "" { result.push(current); }
    return result;
}

fn find_weapon_by_keyword(char_name, keyword) {
    let kw_lower = keyword.to_lower();
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        let matched = false;
        for kw in item.keywords {
            if kw.to_lower() == kw_lower { matched = true; }
        }
        if !matched {
            if item.name.to_lower().contains(kw_lower) { matched = true; }
        }
        if matched && item.has_ranged_type { return item; }
    }
    let inv = get_items_in_inventory(char_name);
    for item in inv {
        let matched = false;
        for kw in item.keywords {
            if kw.to_lower() == kw_lower { matched = true; }
        }
        if !matched {
            if item.name.to_lower().contains(kw_lower) { matched = true; }
        }
        if matched && item.has_ranged_type { return item; }
    }
    return ();
}

fn find_attachment_in_weapon(weapon, keyword) {
    let kw_lower = keyword.to_lower();
    let contents = weapon.container_contents;
    for content_id in contents {
        let item = get_item_data(content_id);
        if item == () { continue; }
        if item.attachment_slot == "" { continue; }
        let matched = false;
        for kw in item.keywords {
            if kw.to_lower() == kw_lower { matched = true; }
        }
        if !matched {
            if item.name.to_lower().contains(kw_lower) { matched = true; }
        }
        if !matched {
            if item.attachment_slot.to_lower() == kw_lower { matched = true; }
        }
        if matched { return item; }
    }
    return ();
}
