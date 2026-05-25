// scripts/commands/pour.rhai
// pour <source> [into <target>] - Pour liquid between containers or empty one

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Pour what? (Usage: pour <container> OR pour <source> into <target>)");
        return;
    }

    let parsed = parse_into_syntax(args);

    if parsed.target != "" {
        do_pour_transfer(char, parsed.source, parsed.target, connection_id);
    } else {
        do_pour_empty(char, parsed.source, connection_id);
    }
}

fn parse_into_syntax(args) {
    let result = #{ source: "", target: "" };

    let parts = args.split(" into ");
    if parts.len() >= 2 {
        result.source = parts[0].to_lower();
        result.target = parts[1].to_lower();
        return result;
    }

    let words = args.split(" ");
    let source_words = [];
    let target_words = [];
    let found_into = false;
    for word in words {
        if word.to_lower() == "into" && !found_into {
            found_into = true;
        } else {
            if found_into {
                target_words.push(word);
            } else {
                source_words.push(word);
            }
        }
    }

    if found_into {
        result.source = join_words(source_words);
        result.target = join_words(target_words);
    } else {
        if words.len() == 2 {
            result.source = words[0].to_lower();
            result.target = words[1].to_lower();
        } else {
            result.source = args.to_lower();
        }
    }
    return result;
}

fn find_item(char, keyword) {
    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        item = find_item_in_room(char.current_room_id, keyword);
    }
    return item;
}

fn do_pour_transfer(char, source_keyword, target_keyword, connection_id) {
    let source = find_item(char, source_keyword);
    if source == () {
        send_client_message(connection_id, "You don't see '" + source_keyword + "' here.");
        return;
    }

    if !source.is_liquid_container {
        send_client_message(connection_id, "That doesn't contain liquid.");
        return;
    }

    if source.liquid_max != -1 && source.liquid_current <= 0 {
        send_client_message(connection_id, source.name + " is empty.");
        return;
    }

    let target = find_item(char, target_keyword);
    if target == () {
        send_client_message(connection_id, "You don't see '" + target_keyword + "' here.");
        return;
    }

    if !target.is_liquid_container {
        send_client_message(connection_id, "You can't pour into that.");
        return;
    }

    if source.id == target.id {
        send_client_message(connection_id, "You can't pour something into itself.");
        return;
    }

    if target.liquid_current >= target.liquid_max {
        send_client_message(connection_id, target.name + " is already full.");
        return;
    }

    let result = fill_liquid_container(target.id, source.id);
    if result.filled {
        let msg = "You pour " + source.liquid_type + " from " + source.name + " into " + target.name + ".";
        send_client_message(connection_id, msg);
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " pours from " + source.name + " into " + target.name + ".");
    } else {
        send_client_message(connection_id, result.message);
    }
}

fn do_pour_empty(char, keyword, connection_id) {
    let item = find_item(char, keyword);
    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !item.is_liquid_container {
        send_client_message(connection_id, "That doesn't contain liquid.");
        return;
    }

    if item.liquid_max == -1 {
        send_client_message(connection_id, "You can't empty something that refills itself.");
        return;
    }

    if item.liquid_current <= 0 {
        send_client_message(connection_id, "It's already empty.");
        return;
    }

    let liquid_name = item.liquid_type;
    set_liquid(item.id, item.liquid_type, 0, item.liquid_max);
    send_client_message(connection_id, "You pour out the " + liquid_name + " from " + item.name + ".");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " pours out " + item.name + ".");
}

fn join_words(words) {
    let result = "";
    for word in words {
        if result != "" { result += " "; }
        result += word;
    }
    return result;
}
