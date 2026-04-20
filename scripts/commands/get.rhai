// scripts/commands/get.rhai
// get <item> - Pick up an item from the room
// get all - Pick up all items from the room
// get all.<keyword> - Pick up all matching items from the room
// get <item> from <container> - Get an item from a container
// get <item> <container> - Shorthand for above (without "from")
// get all from <container> - Get all items from a container
// get all.<keyword> from <container> - Get all matching items from a container
// get <item> all.<container> - Get item from every matching container in room
// get all all.<container> - Get all items from every matching container in room

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if is_on_tour(connection_id) {
        send_client_message(connection_id, "You cannot pick up items while on a property tour.");
        return;
    }

    if are_both_arms_disabled(char.name) {
        send_client_message(connection_id, "Both your arms are disabled! You can't pick anything up.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Get what?");
        return;
    }

    // Check for "get <item> from <container>" syntax (explicit "from")
    let parsed = parse_from_syntax(args);
    if parsed.source != "" {
        get_from_container(char, parsed.item, parsed.source, connection_id);
        return;
    }

    // Check for shorthand syntax: "get <item> <container>" (last word is container)
    let shorthand = try_shorthand_syntax(char, args);
    if shorthand.found {
        get_from_container(char, shorthand.item, shorthand.container, connection_id);
        return;
    }

    // Normal room-based get
    get_from_room(char, args, connection_id);
}

fn get_from_room(char, keyword, connection_id) {
    let room_items = get_items_in_room(char.current_room_id);

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(keyword);
    if all_parsed.is_all {
        get_all_from_room(char, all_parsed.keyword, room_items, connection_id);
        return;
    }

    // Single item get
    let item = find_item_by_keyword(keyword, room_items);

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if item.flags.no_get && !char.god_mode {
        send_client_message(connection_id, "You can't pick that up.");
        return;
    }

    // Check encumbrance before picking up
    if !can_carry_item(char.name, item.id) {
        send_client_message(connection_id, "You can't carry that much weight!");
        return;
    }

    // Fire on_get trigger - can cancel the pickup
    let trigger_result = fire_item_trigger(item.id, "on_get", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "from_container": false
    });
    if trigger_result == "cancel" {
        return;
    }

    // Special handling for gold - add to balance directly (gold is weightless)
    if is_gold_item(item.id) {
        let amount = item.value;
        add_character_gold(char.name, amount);
        delete_item(item.id);
        sync_char_to_session(char.name, connection_id);
        send_client_message(connection_id, "You pick up " + amount + " gold coins.");
        broadcast_to_room(char.current_room_id, char.name + " picks up some gold.", char.name);
        return;
    }

    move_item_to_inventory(item.id, char.name);
    send_client_message(connection_id, "You pick up " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " picks up " + item.name + ".", char.name);
}

fn get_all_from_room(char, filter_keyword, room_items, connection_id) {
    // Get items to pick up (filter by keyword if provided)
    let items_to_get = [];
    if filter_keyword == "" {
        items_to_get = room_items;
    } else {
        items_to_get = find_items_by_keyword(filter_keyword, room_items);
    }

    if items_to_get.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "There's nothing here to pick up.");
        } else {
            send_client_message(connection_id, "You don't see any " + filter_keyword + " here.");
        }
        return;
    }

    let picked_up_names = [];
    let gold_total = 0;
    let skipped = 0;
    let too_heavy = 0;

    for item in items_to_get {
        // Skip no_get items
        if item.flags.no_get && !char.god_mode {
            skipped += 1;
            continue;
        }

        // Check encumbrance (gold is weightless, so skip check for gold)
        if !is_gold_item(item.id) && !can_carry_item(char.name, item.id) {
            too_heavy += 1;
            continue;
        }

        // Fire on_get trigger - can cancel the pickup
        let trigger_result = fire_item_trigger(item.id, "on_get", connection_id, #{
            "char_name": char.name,
            "room_id": char.current_room_id,
            "from_container": false
        });
        if trigger_result == "cancel" {
            skipped += 1;
            continue;
        }

        // Special handling for gold (weightless)
        if is_gold_item(item.id) {
            gold_total += item.value;
            add_character_gold(char.name, item.value);
            delete_item(item.id);
        } else {
            move_item_to_inventory(item.id, char.name);
            picked_up_names.push(item.name);
        }
    }

    if too_heavy > 0 {
        send_client_message(connection_id, "Some items were too heavy to carry.");
    }

    // Sync session if gold was picked up (DB-only changes need session sync)
    if gold_total > 0 {
        sync_char_to_session(char.name, connection_id);
    }

    // Build result message
    let msg = build_pickup_message(picked_up_names, gold_total, filter_keyword);
    if msg != "" {
        send_client_message(connection_id, msg);
        broadcast_to_room(char.current_room_id, char.name + " picks up several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't pick up any of those items.");
    }
}

fn get_from_container(char, item_keyword, container_keyword, connection_id) {
    // Check for "all." prefix on container keyword (e.g., "all.corpse")
    if is_all_container_syntax(container_keyword) {
        let filter = container_keyword.sub_string(4);
        get_from_all_containers(char, item_keyword, filter, connection_id);
        return;
    }

    // Find container in inventory or room
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

    // Corpses can't be closed, so skip closed check for corpses
    if !container.flags.is_corpse && container.container_closed {
        send_client_message(connection_id, "It's closed.");
        return;
    }

    // Special handling for getting gold from corpses
    if container.flags.is_corpse && (item_keyword == "gold" || item_keyword == "coins" || item_keyword == "gold coins") {
        let gold = take_corpse_gold(container.id, char.name);
        if gold > 0 {
            sync_char_to_session(char.name, connection_id);
            send_client_message(connection_id, "You get " + gold + " gold coins from " + container.name + ".");
            let msg = char.name + " gets some gold from " + container.name + ".";
            broadcast_to_room(char.current_room_id, msg, char.name);
        } else {
            send_client_message(connection_id, "There's no gold in " + container.name + ".");
        }
        return;
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(item_keyword);
    if all_parsed.is_all {
        get_all_from_container(char, all_parsed.keyword, container, connection_id);
        return;
    }

    // Single item get
    let contents = get_items_in_container(container.id);
    let item = find_item_by_keyword(item_keyword, contents);

    if item == () {
        send_client_message(connection_id, "You don't see that in " + container.name + ".");
        return;
    }

    // Check encumbrance before getting (gold is weightless, so skip check for gold)
    if !is_gold_item(item.id) && !can_carry_item(char.name, item.id) {
        send_client_message(connection_id, "You can't carry that much weight!");
        return;
    }

    // Fire on_get trigger - can cancel the pickup
    let trigger_result = fire_item_trigger(item.id, "on_get", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "from_container": true,
        "container_id": container.id,
        "container_name": container.name
    });
    if trigger_result == "cancel" {
        return;
    }

    // Special handling for gold - add to balance directly
    if is_gold_item(item.id) {
        let amount = item.value;
        remove_item_from_container(item.id);
        add_character_gold(char.name, amount);
        delete_item(item.id);
        sync_char_to_session(char.name, connection_id);
        send_client_message(connection_id, "You get " + amount + " gold coins from " + container.name + ".");
        let msg = char.name + " gets some gold from " + container.name + ".";
        broadcast_to_room(char.current_room_id, msg, char.name);
        return;
    }

    // Move item from container to inventory
    remove_item_from_container(item.id);
    move_item_to_inventory(item.id, char.name);
    send_client_message(connection_id, "You get " + item.name + " from " + container.name + ".");
    let msg = char.name + " gets something from " + container.name + ".";
    broadcast_to_room(char.current_room_id, msg, char.name);
}

fn get_all_from_container(char, filter_keyword, container, connection_id) {
    let contents = get_items_in_container(container.id);

    // Get corpse gold if this is a corpse and no filter (get all) or gold filter
    let corpse_gold = 0;
    if container.flags.is_corpse {
        if filter_keyword == "" || filter_keyword == "gold" || filter_keyword == "coins" {
            corpse_gold = take_corpse_gold(container.id, char.name);
        }
    }

    // Get items to pick up (filter by keyword if provided)
    let items_to_get = [];
    if filter_keyword == "" {
        items_to_get = contents;
    } else {
        items_to_get = find_items_by_keyword(filter_keyword, contents);
    }

    if items_to_get.len() == 0 && corpse_gold == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, container.name + " is empty.");
        } else {
            send_client_message(connection_id, "You don't see any " + filter_keyword + " in " + container.name + ".");
        }
        return;
    }

    let picked_up_names = [];
    let gold_total = 0;
    let skipped = 0;
    let too_heavy = 0;

    for item in items_to_get {
        // Check encumbrance (gold is weightless, so skip check for gold)
        if !is_gold_item(item.id) && !can_carry_item(char.name, item.id) {
            too_heavy += 1;
            continue;
        }

        // Fire on_get trigger - can cancel the pickup
        let trigger_result = fire_item_trigger(item.id, "on_get", connection_id, #{
            "char_name": char.name,
            "room_id": char.current_room_id,
            "from_container": true,
            "container_id": container.id,
            "container_name": container.name
        });
        if trigger_result == "cancel" {
            skipped += 1;
            continue;
        }

        // Special handling for gold
        if is_gold_item(item.id) {
            gold_total += item.value;
            remove_item_from_container(item.id);
            add_character_gold(char.name, item.value);
            delete_item(item.id);
        } else {
            remove_item_from_container(item.id);
            move_item_to_inventory(item.id, char.name);
            picked_up_names.push(item.name);
        }
    }

    if too_heavy > 0 {
        send_client_message(connection_id, "Some items were too heavy to carry.");
    }

    // Add corpse gold to the total
    gold_total += corpse_gold;

    // Sync session if gold was picked up (DB-only changes need session sync)
    if gold_total > 0 {
        sync_char_to_session(char.name, connection_id);
    }

    // Build result message listing item names
    let msg = build_container_pickup_message(picked_up_names, gold_total, filter_keyword, container.name);
    if msg != "" {
        send_client_message(connection_id, msg);
        let broadcast_msg = char.name + " gets several items from " + container.name + ".";
        broadcast_to_room(char.current_room_id, broadcast_msg, char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't get any of those items.");
    }
}

fn build_pickup_message(names, gold_total, filter_keyword) {
    let parts = [];
    if names.len() > 0 {
        parts.push(build_item_names_msg(names, "pick up"));
    }
    parts.push(build_gold_pickup_msg(gold_total));
    return join_non_empty(parts);
}

fn build_container_pickup_message(names, gold_total, filter_keyword, container_name) {
    let parts = [];
    if names.len() > 0 {
        parts.push(build_container_item_names_msg(names, container_name));
    }
    parts.push(build_container_gold_msg(gold_total, container_name));
    return join_non_empty(parts);
}

fn build_item_names_msg(names, verb) {
    let list = format_item_list(names);
    return "You " + verb + " " + list + ".";
}

fn build_gold_pickup_msg(gold_total) {
    if gold_total <= 0 { return ""; }
    return "You pick up " + gold_total + " gold coins.";
}

fn build_container_item_names_msg(names, container_name) {
    let list = format_item_list(names);
    return "You get " + list + " from " + container_name + ".";
}

fn build_container_gold_msg(gold_total, container_name) {
    if gold_total <= 0 { return ""; }
    return "You get " + gold_total + " gold coins from " + container_name + ".";
}

fn format_item_list(names) {
    if names.len() == 1 { return names[0]; }
    if names.len() == 2 { return names[0] + " and " + names[1]; }
    // 3+ items: "a, b, and c"
    let result = "";
    let i = 0;
    for name in names {
        if i > 0 {
            if i == names.len() - 1 {
                result += ", and ";
            } else {
                result += ", ";
            }
        }
        result += name;
        i += 1;
    }
    return result;
}

fn join_non_empty(parts) {
    let result = [];
    for part in parts {
        if part != "" { result.push(part); }
    }
    return join_words(result);
}

fn try_shorthand_syntax(char, args) {
    let result = #{ found: false, item: "", container: "" };

    let words = args.split(" ");
    if words.len() < 2 {
        return result;
    }

    // Try the last word as a container name
    let potential_container = words[words.len() - 1];

    // Check for all.keyword container syntax (e.g., "get all all.corpse")
    if is_all_container_syntax(potential_container) {
        let filter = potential_container.sub_string(4);
        let room_items = get_items_in_room(char.current_room_id);
        let matching = find_items_by_keyword(filter, room_items);
        if has_any_container(matching) {
            result.found = true;
            result.container = potential_container;
            result.item = get_all_but_last(words);
            return result;
        }
        return result;
    }

    // Look for container in inventory first, then room
    let container = find_item_in_inventory(char.name, potential_container);
    if container == () {
        container = find_item_in_room(char.current_room_id, potential_container);
    }

    // If we found a container, use shorthand
    if container == () { return result; }
    if !container.is_container { return result; }

    result.found = true;
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

fn parse_from_syntax(args) {
    let result = #{ item: "", source: "" };
    let parts = args.split(" from ");
    if parts.len() >= 2 {
        result.item = parts[0].to_lower();
        result.source = parts[1].to_lower();
        return result;
    }
    // Try word-by-word
    let words = args.split(" ");
    let item_words = [];
    let source_words = [];
    let found_from = false;
    for word in words {
        if word.to_lower() == "from" && !found_from {
            found_from = true;
        } else {
            if found_from {
                source_words.push(word);
            } else {
                item_words.push(word);
            }
        }
    }
    result.item = join_words(item_words);
    if found_from {
        result.source = join_words(source_words);
    }
    return result;
}

fn sync_char_to_session(char_name, connection_id) {
    let updated = get_character_data(char_name);
    if updated != () {
        set_player_character(connection_id, updated);
    }
}

fn join_words(words) {
    let result = "";
    for word in words {
        if result != "" { result += " "; }
        result += word;
    }
    return result;
}

fn is_all_container_syntax(keyword) {
    if keyword.len() <= 4 { return false; }
    if keyword.to_lower().starts_with("all.") { return true; }
    return false;
}

fn has_any_container(items) {
    for item in items {
        if item.is_container { return true; }
    }
    return false;
}

fn get_from_all_containers(char, item_keyword, container_filter, connection_id) {
    let room_items = get_items_in_room(char.current_room_id);
    let matching = find_items_by_keyword(container_filter, room_items);

    // Filter to open containers only
    let containers = [];
    for item in matching {
        if !item.is_container { continue; }
        // Skip closed non-corpse containers
        if !item.flags.is_corpse && item.container_closed { continue; }
        containers.push(item);
    }

    if containers.len() == 0 {
        send_client_message(connection_id, "You don't see any " + container_filter + " containers here.");
        return;
    }

    let all_parsed = parse_all_syntax(item_keyword);

    for container in containers {
        // Reload char data between containers (gold/encumbrance changes)
        char = get_player_character(connection_id);
        if char == () { return; }

        if all_parsed.is_all {
            get_all_from_container(char, all_parsed.keyword, container, connection_id);
        } else {
            get_single_from_specific_container(char, item_keyword, container, connection_id);
        }
    }
}

fn get_single_from_specific_container(char, item_keyword, container, connection_id) {
    // Special handling for getting gold from corpses
    if container.flags.is_corpse {
        if item_keyword == "gold" || item_keyword == "coins" || item_keyword == "gold coins" {
            let gold = take_corpse_gold(container.id, char.name);
            if gold > 0 {
                sync_char_to_session(char.name, connection_id);
                send_client_message(connection_id, "You get " + gold + " gold coins from " + container.name + ".");
                broadcast_to_room(char.current_room_id, char.name + " gets some gold from " + container.name + ".", char.name);
            }
            return;
        }
    }

    let contents = get_items_in_container(container.id);
    let item = find_item_by_keyword(item_keyword, contents);

    if item == () { return; }

    // Check encumbrance
    if !is_gold_item(item.id) && !can_carry_item(char.name, item.id) {
        send_client_message(connection_id, "You can't carry that much weight!");
        return;
    }

    // Fire on_get trigger
    let trigger_result = fire_item_trigger(item.id, "on_get", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "from_container": true,
        "container_id": container.id,
        "container_name": container.name
    });
    if trigger_result == "cancel" { return; }

    // Gold handling
    if is_gold_item(item.id) {
        let amount = item.value;
        remove_item_from_container(item.id);
        add_character_gold(char.name, amount);
        delete_item(item.id);
        sync_char_to_session(char.name, connection_id);
        send_client_message(connection_id, "You get " + amount + " gold coins from " + container.name + ".");
        broadcast_to_room(char.current_room_id, char.name + " gets some gold from " + container.name + ".", char.name);
        return;
    }

    remove_item_from_container(item.id);
    move_item_to_inventory(item.id, char.name);
    send_client_message(connection_id, "You get " + item.name + " from " + container.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " gets something from " + container.name + ".", char.name);
}
