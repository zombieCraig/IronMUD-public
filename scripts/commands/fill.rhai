// scripts/commands/fill.rhai
// fill <container> [from <source>] - Fill a liquid container

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Fill what?");
        return;
    }

    // Parse "container from source" syntax
    let parsed = parse_from_syntax(args);
    let container_keyword = parsed.container;
    let source_keyword = parsed.source;

    // Find the target container
    let container = find_container(char, container_keyword);
    if container == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !container.is_liquid_container {
        send_client_message(connection_id, "You can't fill that with liquid.");
        return;
    }

    if container.liquid_current >= container.liquid_max {
        send_client_message(connection_id, "It's already full.");
        return;
    }

    // If source specified, fill from that
    if source_keyword != "" {
        fill_from_source(char, container, source_keyword, connection_id);
    } else {
        fill_from_room(char, container, connection_id);
    }
}

fn find_container(char, keyword) {
    let container = find_item_in_inventory(char.name, keyword);
    if container == () {
        container = find_item_in_room(char.current_room_id, keyword);
    }
    return container;
}

fn fill_from_source(char, container, source_keyword, connection_id) {
    let source = find_item_in_inventory(char.name, source_keyword);
    if source == () {
        source = find_item_in_room(char.current_room_id, source_keyword);
    }

    if source == () {
        send_client_message(connection_id, "You don't see that source here.");
        return;
    }

    if !source.is_liquid_container {
        send_client_message(connection_id, "That's not a liquid source.");
        return;
    }

    if source.liquid_current <= 0 {
        send_client_message(connection_id, source.name + " is empty.");
        return;
    }

    if source.id == container.id {
        send_client_message(connection_id, "You can't fill something from itself.");
        return;
    }

    do_fill(char, container, source, connection_id);
}

fn fill_from_room(char, container, connection_id) {
    let found_source = find_room_water_source(char.current_room_id);

    if found_source != () {
        do_fill(char, container, found_source, connection_id);
    } else {
        send_client_message(connection_id, "Fill from what? (Usage: fill <container> from <source>)");
    }
}

fn do_fill(char, container, source, connection_id) {
    let result = fill_liquid_container(container.id, source.id);
    if result.filled > 0 {
        let msg = build_fill_message(container.name, source.name);
        send_client_message(connection_id, msg);
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " fills " + container.name + ".");
    } else {
        send_client_message(connection_id, result.message);
    }
}

fn build_fill_message(container_name, source_name) {
    return "You fill " + container_name + " from " + source_name + ".";
}

fn find_room_water_source(room_id) {
    let room_items = get_items_in_room(room_id);
    for item in room_items {
        if is_water_source(item) {
            return item;
        }
    }
    return ();
}

fn is_water_source(item) {
    if !item.is_liquid_container { return false; }
    if item.liquid_current <= 0 { return false; }
    let name_lower = item.name.to_lower();
    if name_lower.contains("fountain") { return true; }
    if name_lower.contains("well") { return true; }
    if name_lower.contains("spring") { return true; }
    return false;
}

fn parse_from_syntax(args) {
    let result = #{ container: "", source: "" };
    let parts = args.split(" from ");
    if parts.len() >= 2 {
        result.container = parts[0].to_lower();
        result.source = parts[1].to_lower();
        return result;
    }
    // Try word-by-word
    let words = args.split(" ");
    let container_words = [];
    let source_words = [];
    let found_from = false;
    for word in words {
        if word.to_lower() == "from" && !found_from {
            found_from = true;
        } else {
            if found_from {
                source_words.push(word);
            } else {
                container_words.push(word);
            }
        }
    }
    result.container = join_words(container_words);
    if found_from {
        result.source = join_words(source_words);
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
