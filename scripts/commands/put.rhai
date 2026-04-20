// scripts/commands/put.rhai
// put <item> in <container> - Put an item into a container
// put <item> <container> - Shorthand for above (without "in")
// put all in <container> - Put all items in a container
// put all.<keyword> in <container> - Put all matching items in a container
// put <amount> gold in <container> - Put gold from balance into a container

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: put <item> in <container>");
        send_client_message(connection_id, "       put all in <container>");
        send_client_message(connection_id, "       put <amount> gold in <container>");
        return;
    }

    // Try parsing with "in" keyword first
    let parsed = parse_in_syntax(args);

    // If no "in" found, try shorthand (last word as container)
    if parsed.item == "" || parsed.container == "" {
        parsed = try_shorthand_syntax(char, args);
    }

    if parsed.item == "" || parsed.container == "" {
        send_client_message(connection_id, "Usage: put <item> in <container>");
        send_client_message(connection_id, "       put all in <container>");
        return;
    }

    let item_keyword = parsed.item;
    let container_keyword = parsed.container;

    // Check for "put <amount> gold in <container>" syntax
    let item_parts = item_keyword.split(" ");
    if item_parts.len() >= 2 {
        let last_word = item_parts[item_parts.len() - 1].to_lower();
        if last_word == "gold" || last_word == "coins" {
            let amount = parse_int(item_parts[0]);
            if amount != () && amount > 0 {
                put_gold_in_container(char, amount, container_keyword, connection_id);
                return;
            }
        }
    }

    // Find the container (inventory first, then room)
    let container = find_item_in_inventory(char.name, container_keyword);
    if container == () {
        container = find_item_in_room(char.current_room_id, container_keyword);
    }

    if container == () {
        send_client_message(connection_id, "You don't see that container here.");
        return;
    }

    if !container.is_container {
        send_client_message(connection_id, "That's not a container.");
        return;
    }

    if container.container_closed {
        send_client_message(connection_id, "It's closed.");
        return;
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(item_keyword);
    if all_parsed.is_all {
        put_all_in_container(char, all_parsed.keyword, container, connection_id);
        return;
    }

    // Single item put
    let item = find_item_in_inventory(char.name, item_keyword);
    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    // Don't put container in itself
    if item.id == container.id {
        send_client_message(connection_id, "You can't put something inside itself.");
        return;
    }

    // Check if we can add to container
    let result = can_add_to_container(container.id, item.id);
    if !result.allowed {
        send_client_message(connection_id, result.reason);
        return;
    }

    // Move item to container
    if move_item_to_container(item.id, container.id) {
        send_client_message(connection_id, "You put " + item.name + " in " + container.name + ".");
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " puts something in " + container.name + ".");
    } else {
        send_client_message(connection_id, "You can't put that there.");
    }
}

fn put_all_in_container(char, filter_keyword, container, connection_id) {
    let inv_items = get_items_in_inventory(char.name);

    // Filter by keyword if provided
    let items_to_put = [];
    if filter_keyword == "" {
        items_to_put = inv_items;
    } else {
        items_to_put = find_items_by_keyword(filter_keyword, inv_items);
    }

    if items_to_put.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "You don't have anything to put in " + container.name + ".");
        } else {
            send_client_message(connection_id, "You don't have any " + filter_keyword + ".");
        }
        return;
    }

    let put_count = 0;
    let skipped = 0;

    for item in items_to_put {
        // Don't put container in itself
        if item.id == container.id {
            skipped += 1;
            continue;
        }

        // Check if we can add to container
        let result = can_add_to_container(container.id, item.id);
        if !result.allowed {
            skipped += 1;
            continue;
        }

        // Move item to container
        if move_item_to_container(item.id, container.id) {
            put_count += 1;
        } else {
            skipped += 1;
        }
    }

    // Build result message
    if put_count > 0 {
        if put_count == 1 {
            send_client_message(connection_id, "You put 1 item in " + container.name + ".");
        } else {
            send_client_message(connection_id, "You put " + put_count + " items in " + container.name + ".");
        }
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " puts several items in " + container.name + ".");
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't put any of those items in " + container.name + ".");
    }
}

fn try_shorthand_syntax(char, args) {
    let result = #{ item: "", container: "" };

    let words = args.split(" ");
    if words.len() < 2 {
        return result;
    }

    // Try the last word as a container name
    let potential_container = words[words.len() - 1];

    // Look for container in inventory first, then room
    let container = find_item_in_inventory(char.name, potential_container);
    if container == () {
        container = find_item_in_room(char.current_room_id, potential_container);
    }

    // If we found a container, use shorthand
    if container == () { return result; }
    if !container.is_container { return result; }

    result.container = potential_container;
    result.item = get_all_but_last(words);
    return result;
}

fn get_all_but_last(words) {
    let item_words = [];
    for word in words {
        item_words.push(word);
    }
    if item_words.len() > 0 {
        item_words.pop();
    }
    return join_words(item_words);
}

fn parse_in_syntax(args) {
    let result = #{ item: "", container: "" };
    let parts = args.split(" in ");
    if parts.len() >= 2 {
        result.item = parts[0].to_lower();
        result.container = parts[1].to_lower();
        return result;
    }
    // Try word-by-word
    let words = args.split(" ");
    let item_words = [];
    let container_words = [];
    let found_in = false;
    for word in words {
        if word.to_lower() == "in" && !found_in {
            found_in = true;
        } else {
            if found_in {
                container_words.push(word);
            } else {
                item_words.push(word);
            }
        }
    }
    if found_in {
        result.item = join_words(item_words);
        result.container = join_words(container_words);
    }
    return result;
}

fn join_words(words) {
    let result = "";
    for word in words {
        if result != "" { result += " "; }
        result += word;
    }
    return result;
}

fn put_gold_in_container(char, amount, container_keyword, connection_id) {
    // Check if player has enough gold
    let current_gold = get_character_gold(char.name);
    if current_gold < amount {
        if current_gold <= 0 {
            send_client_message(connection_id, "You don't have any gold.");
        } else {
            send_client_message(connection_id, "You only have " + current_gold + " gold.");
        }
        return;
    }

    // Find the container (inventory first, then room)
    let container = find_item_in_inventory(char.name, container_keyword);
    if container == () {
        container = find_item_in_room(char.current_room_id, container_keyword);
    }

    if container == () {
        send_client_message(connection_id, "You don't see that container here.");
        return;
    }

    if !container.is_container {
        send_client_message(connection_id, "That's not a container.");
        return;
    }

    if container.container_closed {
        send_client_message(connection_id, "It's closed.");
        return;
    }

    // Subtract from balance
    add_character_gold(char.name, -amount);

    // Spawn gold in container (auto-merges with existing gold)
    let gold = spawn_gold_in_container(amount, container.id);
    if gold == () {
        // Refund on failure
        add_character_gold(char.name, amount);
        send_client_message(connection_id, "You can't put gold in there.");
        return;
    }

    send_client_message(connection_id, "You put " + amount + " gold coins in " + container.name + ".");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " puts something in " + container.name + ".");
}
