// scripts/commands/remove.rhai
// remove <item> - Remove equipped item to inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Remove what?");
        return;
    }

    let eq_items = get_equipped_items(char.name);
    let item = find_item_by_keyword(args, eq_items);

    if item == () {
        send_client_message(connection_id, "You aren't wearing that.");
        return;
    }

    if item.flags.no_remove && !char.god_mode {
        send_client_message(connection_id, "You can't remove " + item.name + "!");
        return;
    }

    move_item_to_inventory(item.id, char.name);
    send_client_message(connection_id, "You remove " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " removes " + item.name + ".", char.name);
}
