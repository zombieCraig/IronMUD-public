// scripts/commands/idelete.rhai
// idelete <item_id> - Delete an item from the database (builder command)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: idelete <item_id>");
        return;
    }

    let item = get_item_data(args);
    if item == () {
        send_client_message(connection_id, "Item not found: " + args);
        return;
    }

    // Sandbox: stamped prototypes require can_edit_area on their owner.
    // Orphans (area_id == "") fall through and remain editable by any builder.
    let item_area_id = item.area_id;
    if item_area_id != "" {
        if !can_edit_area(item_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to delete items in that area.");
            return;
        }
    }

    let name = item.name;
    if delete_item(args) {
        send_client_message(connection_id, "Deleted item: " + name);
    } else {
        send_client_message(connection_id, "Error: Failed to delete item.");
    }
}
