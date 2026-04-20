// scripts/commands/ospawn.rhai
// ospawn <vnum> - Spawn an item from a prototype and place in room or inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: ospawn <vnum> [room|inv]");
        send_client_message(connection_id, "  room - spawn item in current room (default)");
        send_client_message(connection_id, "  inv  - spawn item in your inventory");
        return;
    }

    let parts = args.split(" ");
    let vnum = parts[0];
    let dest = "room";
    if parts.len() >= 2 {
        dest = parts[1].to_lower();
    }

    // Find the prototype
    let prototype = get_item_by_vnum(vnum);
    if prototype == () {
        send_client_message(connection_id, "No prototype found with vnum: " + vnum);
        return;
    }

    // Check unique flag - prevent duplicate spawns
    if prototype.flags.unique {
        let existing = count_items_by_vnum(vnum);
        if existing > 0 {
            send_client_message(connection_id, "Cannot spawn: unique item '" + prototype.name + "' already exists in world.");
            return;
        }
    }

    // Spawn the item
    let item = spawn_item_from_prototype(vnum);
    if item == () {
        send_client_message(connection_id, "Failed to spawn item from prototype: " + vnum);
        return;
    }

    // Place the item
    if dest == "inv" || dest == "inventory" {
        move_item_to_inventory(item.id, char.name);
        send_client_message(connection_id, "Spawned " + item.name + " into your inventory.");
    } else {
        move_item_to_room(item.id, char.current_room_id);
        send_client_message(connection_id, "Spawned " + item.name + " in the room.");
        broadcast_to_room(char.current_room_id, item.name + " appears out of thin air!", char.name);
    }
}
