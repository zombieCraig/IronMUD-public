// scripts/commands/inv.rhai
// inv - Alias for inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let items = get_items_in_inventory(char.name);

    if items.len() == 0 {
        send_client_message(connection_id, "You aren't carrying anything.");
        return;
    }

    let output = "You are carrying:\n";
    for item in items {
        output += "  " + item.name + "\n";
    }

    send_client_message(connection_id, output);
}
