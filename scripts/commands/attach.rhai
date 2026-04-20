// scripts/commands/attach.rhai
// Attach an attachment item to a ranged weapon
// Usage: attach <attachment_keyword> <weapon_keyword>

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () { send_client_message(connection_id, "You must be logged in."); return; }
    if char.position == "sleeping" { send_client_message(connection_id, "You are asleep!"); return; }

    let parts = split_args(args);
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: attach <attachment> <weapon>");
        return;
    }

    let att_keyword = parts[0];
    let wpn_keyword = parts[1];

    // Find attachment in inventory
    let attachment = find_item_in_inventory(char.name, att_keyword);
    if attachment == () {
        send_client_message(connection_id, "You don't have '" + att_keyword + "' in your inventory.");
        return;
    }

    // Find weapon: check wielded first, then inventory
    let weapon = find_weapon_by_keyword(char.name, wpn_keyword);
    if weapon == () {
        send_client_message(connection_id, "You don't have a weapon matching '" + wpn_keyword + "'.");
        return;
    }

    // Validate attachment
    let check = can_attach_to_weapon(attachment.id, weapon.id);
    if !check.allowed {
        send_client_message(connection_id, check.reason);
        return;
    }

    // Attach: move attachment into weapon container
    move_item_to_container(attachment.id, weapon.id);
    send_client_message(connection_id, "You attach " + attachment.name + " to " + weapon.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " attaches something to " + weapon.name + ".", char.name);
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
    // Check equipped items first (wielded weapons)
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
    // Check inventory
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
