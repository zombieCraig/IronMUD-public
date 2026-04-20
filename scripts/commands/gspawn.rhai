// scripts/commands/gspawn.rhai
// gspawn <amount> [container <id>] - Spawn gold for builders

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: gspawn <amount> [container <id>]");
        send_client_message(connection_id, "  gspawn 100              - Spawn 100 gold in current room");
        send_client_message(connection_id, "  gspawn 50 container bag - Spawn 50 gold in container");
        return;
    }

    let parts = args.split(" ");
    let amount = parse_int(parts[0]);
    if amount == () || amount <= 0 {
        send_client_message(connection_id, "Amount must be a positive number.");
        return;
    }

    // Simple spawn in current room
    if parts.len() == 1 {
        let gold = spawn_gold_in_room(amount, char.current_room_id);
        if gold == () {
            send_client_message(connection_id, "Failed to spawn gold.");
            return;
        }
        let desc = get_gold_description(amount);
        send_client_message(connection_id, "Spawned " + desc + " (" + amount + " gold) in room.");
        broadcast_to_room(char.current_room_id, "Gold coins appear out of thin air!", char.name);
        return;
    }

    // Check for container spawn syntax
    if parts.len() >= 3 && parts[1].to_lower() == "container" {
        let container_keyword = parts[2];

        // Find container in room or inventory
        let container = find_item_in_room(char.current_room_id, container_keyword);
        if container == () {
            container = find_item_in_inventory(char.name, container_keyword);
        }

        if container == () {
            send_client_message(connection_id, "Container not found: " + container_keyword);
            return;
        }

        if !container.is_container {
            send_client_message(connection_id, "That's not a container.");
            return;
        }

        let gold = spawn_gold_in_container(amount, container.id);
        if gold == () {
            send_client_message(connection_id, "Failed to spawn gold in container.");
            return;
        }
        send_client_message(connection_id, "Spawned " + amount + " gold in " + container.name + ".");
        return;
    }

    send_client_message(connection_id, "Usage: gspawn <amount> [container <id>]");
}
