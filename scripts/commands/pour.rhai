// scripts/commands/pour.rhai
// pour <container> - Pour out a liquid container

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Pour what?");
        return;
    }

    let keyword = args.to_lower();

    // Look in inventory first
    let item = find_item_in_inventory(char.name, keyword);

    // Then look in room
    if item == () {
        item = find_item_in_room(char.current_room_id, keyword);
    }

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !item.is_liquid_container {
        send_client_message(connection_id, "That doesn't contain liquid.");
        return;
    }

    if item.liquid_current <= 0 {
        send_client_message(connection_id, "It's already empty.");
        return;
    }

    let liquid_name = item.liquid_type;

    // Empty the container
    set_liquid(item.id, item.liquid_type, 0, item.liquid_max);

    send_client_message(connection_id, "You pour out the " + liquid_name + " from " + item.name + ".");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " pours out " + item.name + ".");
}
