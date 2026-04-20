// scripts/commands/wear.rhai
// wear <item> - Equip an item from inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Wear what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    let wear_locs = item.wear_locations;
    if wear_locs.len() == 0 {
        send_client_message(connection_id, "You can't wear " + item.name + ".");
        return;
    }

    // Check level requirement
    if item.level_requirement > 0 && char.level < item.level_requirement {
        send_client_message(connection_id, "You must be level " + item.level_requirement + " to wear " + item.name + ".");
        return;
    }

    // Check for conflicts
    let check = can_wear_item(char.name, item.id);
    if !check["can_wear"] {
        let conflicts = check["conflicts"];
        let msg = "You need to remove ";
        let first = true;
        for conflict in conflicts {
            if !first { msg += ", "; }
            msg += conflict;
            first = false;
        }
        msg += " first.";
        send_client_message(connection_id, msg);
        return;
    }

    move_item_to_equipped(item.id, char.name);
    send_client_message(connection_id, "You wear " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " wears " + item.name + ".", char.name);
}
