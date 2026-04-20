// scripts/commands/wield.rhai
// wield <item> - Wield a weapon or hold an item in your hand
// Works for items with wear locations: wielded, offhand, off-hand
// Also works for fishing rods

fn has_wieldable_location(item) {
    for loc in item.wear_locations {
        let loc_lower = loc.to_lower();
        if loc_lower == "wielded" { return true; }
        if loc_lower == "offhand" { return true; }
        if loc_lower == "off-hand" { return true; }
    }
    return false;
}

fn find_wield_conflict(char_name) {
    let equipped = get_equipped_items(char_name);
    for eq_item in equipped {
        if has_wieldable_location(eq_item) {
            return eq_item.name;
        }
    }
    return "";
}

fn build_conflict_message(conflicts) {
    let msg = "You need to remove ";
    let first = true;
    for conflict in conflicts {
        if !first { msg += ", "; }
        msg += conflict;
        first = false;
    }
    msg += " first.";
    return msg;
}

fn check_normal_conflicts(char_name, item_id) {
    let check = can_wear_item(char_name, item_id);
    if !check["can_wear"] {
        let conflicts = check["conflicts"];
        if conflicts.len() > 0 {
            return build_conflict_message(conflicts);
        }
    }
    return "";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Wield what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    // Check if item can be wielded/held
    let can_wield = has_wieldable_location(item);
    if item.is_fishing_rod { can_wield = true; }

    if !can_wield {
        send_client_message(connection_id, "You can't wield " + item.name + ".");
        return;
    }

    // Check arm wounds
    let right_disabled = is_arm_disabled(char.name, "right");
    let left_disabled = is_arm_disabled(char.name, "left");
    if right_disabled && left_disabled {
        send_client_message(connection_id, "Both your arms are disabled! You can't wield anything.");
        return;
    }
    if item.two_handed && (right_disabled || left_disabled) {
        send_client_message(connection_id, "You can't wield a two-handed weapon with a disabled arm!");
        return;
    }

    // Check level requirement
    if item.level_requirement > 0 && char.level < item.level_requirement {
        send_client_message(connection_id, "You must be level " + item.level_requirement + " to wield " + item.name + ".");
        return;
    }

    // Check for conflicts
    let conflict_msg = "";
    if item.is_fishing_rod && item.wear_locations.len() == 0 {
        let conflict = find_wield_conflict(char.name);
        if conflict != "" {
            conflict_msg = "You need to remove " + conflict + " first.";
        }
    } else {
        conflict_msg = check_normal_conflicts(char.name, item.id);
    }

    if conflict_msg != "" {
        send_client_message(connection_id, conflict_msg);
        return;
    }

    move_item_to_equipped(item.id, char.name);

    let verb = "wield";
    if item.is_fishing_rod { verb = "hold"; }

    send_client_message(connection_id, "You " + verb + " " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " " + verb + "s " + item.name + ".", char.name);
}
