// scripts/commands/give.rhai
// give <item> to <player|mobile> - Give an item from inventory or equipment
// give <item> <target>          - Shorthand for above (without "to")
// give all to <target>          - Give all items
// give all.<keyword> to <target>- Give all matching items
// give <amount> gold to <target>- Give gold

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

    if args == "" {
        send_client_message(connection_id, "Usage: give <item> to <target>");
        send_client_message(connection_id, "       give all to <target>");
        send_client_message(connection_id, "       give <amount> gold to <target>");
        return;
    }

    // Parse "X to Y" syntax first
    let parsed = parse_give_syntax(args);

    // If no "to" found, try shorthand (last word as target name)
    if parsed.target == "" {
        parsed = try_shorthand_syntax(char, args);
    }

    if parsed.target == "" {
        send_client_message(connection_id, "Usage: give <item> to <target>");
        send_client_message(connection_id, "       give all to <target>");
        return;
    }

    // Resolve target: try player first, then mobile in the same room
    let target_conn = find_player_connection_by_name(parsed.target);
    let target_char = ();
    let target_mobile = ();

    if target_conn != "" {
        target_char = get_player_character(target_conn);
    }

    if target_char == () {
        let mobiles = get_mobiles_in_room(char.current_room_id);
        target_mobile = find_mobile_by_keyword(parsed.target, mobiles);
    }

    if target_char == () && target_mobile == () {
        send_client_message(connection_id, "There is no '" + parsed.target + "' here.");
        return;
    }

    // Players must be in the same room; mobiles already are by construction
    if target_char != () {
        if target_char.current_room_id != char.current_room_id {
            send_client_message(connection_id, target_char.name + " is not here.");
            return;
        }
        if target_char.name.to_lower() == char.name.to_lower() {
            send_client_message(connection_id, "You can't give things to yourself.");
            return;
        }
    }

    // Check for gold syntax: "<amount> gold"
    let item_parts = parsed.item.split(" ");
    if item_parts.len() >= 2 {
        let last_word = item_parts[item_parts.len() - 1].to_lower();
        if last_word == "gold" || last_word == "coins" {
            let amount = parse_int(item_parts[0]);
            if amount != () && amount > 0 {
                if target_char != () {
                    give_gold_to_player(char, target_char, amount, connection_id, target_conn);
                } else {
                    give_gold_to_mobile(char, target_mobile, amount, connection_id);
                }
                return;
            }
        }
    }

    // Check for "all" syntax
    let all_parsed = parse_all_syntax(parsed.item);
    if all_parsed.is_all {
        if target_char != () {
            give_all_items_to_player(char, target_char, all_parsed.keyword, connection_id, target_conn);
        } else {
            give_all_items_to_mobile(char, target_mobile, all_parsed.keyword, connection_id);
        }
        return;
    }

    // Give a single item
    if target_char != () {
        give_item_to_player(char, target_char, parsed.item, connection_id, target_conn);
    } else {
        give_item_to_mobile(char, target_mobile, parsed.item, connection_id);
    }
}

// ========== Gold ==========

fn give_gold_to_player(char, target_char, amount, connection_id, target_conn) {
    let current_gold = get_character_gold(char.name);
    if current_gold < amount {
        if current_gold <= 0 {
            send_client_message(connection_id, "You don't have any gold.");
        } else {
            send_client_message(connection_id, "You only have " + current_gold + " gold.");
        }
        return;
    }

    add_character_gold(char.name, -amount);
    add_character_gold(target_char.name, amount);

    send_client_message(connection_id, "You give " + target_char.name + " " + amount + " gold coins.");
    send_client_message(target_conn, char.name + " gives you " + amount + " gold coins.");
    broadcast_to_room(char.current_room_id, char.name + " gives " + target_char.name + " some gold.", char.name);
}

fn give_gold_to_mobile(char, target_mobile, amount, connection_id) {
    let current_gold = get_character_gold(char.name);
    if current_gold < amount {
        if current_gold <= 0 {
            send_client_message(connection_id, "You don't have any gold.");
        } else {
            send_client_message(connection_id, "You only have " + current_gold + " gold.");
        }
        return;
    }

    add_character_gold(char.name, -amount);
    set_mobile_gold(target_mobile.id, target_mobile.gold + amount);

    send_client_message(connection_id, "You give " + target_mobile.name + " " + amount + " gold coins.");
    broadcast_to_room(char.current_room_id, char.name + " gives " + target_mobile.name + " some gold.", char.name);
}

// ========== Single item ==========

fn resolve_giveable_item(char, item_keyword, connection_id) {
    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(item_keyword, inv_items);
    let is_equipped = false;

    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(item_keyword, eq_items);
        if item != () { is_equipped = true; }
    }

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return ();
    }

    if item.flags.no_drop && !char.god_mode {
        send_client_message(connection_id, "You can't give that away!");
        return ();
    }

    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot give away quest items.");
        return ();
    }

    if is_equipped && item.flags.no_remove && !char.god_mode {
        send_client_message(connection_id, "You can't remove " + item.name + "!");
        return ();
    }

    return item;
}

fn give_item_to_player(char, target_char, item_keyword, connection_id, target_conn) {
    let item = resolve_giveable_item(char, item_keyword, connection_id);
    if item == () { return; }

    move_item_to_inventory(item.id, target_char.name);

    send_client_message(connection_id, "You give " + item.name + " to " + target_char.name + ".");
    send_client_message(target_conn, char.name + " gives you " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " gives " + target_char.name + " something.", char.name);
}

fn give_item_to_mobile(char, target_mobile, item_keyword, connection_id) {
    let item = resolve_giveable_item(char, item_keyword, connection_id);
    if item == () { return; }

    move_item_to_mobile_inventory(item.id, target_mobile.id);

    send_client_message(connection_id, "You give " + item.name + " to " + target_mobile.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " gives " + target_mobile.name + " something.", char.name);
}

// ========== Give all ==========

fn collect_give_all_items(char, filter_keyword, connection_id) {
    let inv_items = get_items_in_inventory(char.name);
    let items_to_give = [];
    if filter_keyword == "" {
        items_to_give = inv_items;
    } else {
        items_to_give = find_items_by_keyword(filter_keyword, inv_items);
    }

    if items_to_give.len() == 0 {
        if filter_keyword == "" {
            send_client_message(connection_id, "You don't have anything to give.");
        } else {
            send_client_message(connection_id, "You don't have any " + filter_keyword + ".");
        }
    }
    return items_to_give;
}

fn give_all_items_to_player(char, target_char, filter_keyword, connection_id, target_conn) {
    let items_to_give = collect_give_all_items(char, filter_keyword, connection_id);
    if items_to_give.len() == 0 { return; }

    let given = 0;
    let skipped = 0;
    for item in items_to_give {
        if item.flags.no_drop && !char.god_mode { skipped += 1; continue; }
        if item.flags.quest_item && !char.god_mode { skipped += 1; continue; }
        move_item_to_inventory(item.id, target_char.name);
        given += 1;
    }

    if given > 0 {
        if given == 1 {
            send_client_message(connection_id, "You give 1 item to " + target_char.name + ".");
            send_client_message(target_conn, char.name + " gives you 1 item.");
        } else {
            send_client_message(connection_id, "You give " + given + " items to " + target_char.name + ".");
            send_client_message(target_conn, char.name + " gives you " + given + " items.");
        }
        broadcast_to_room(char.current_room_id, char.name + " gives " + target_char.name + " several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't give any of those items.");
    }
}

fn give_all_items_to_mobile(char, target_mobile, filter_keyword, connection_id) {
    let items_to_give = collect_give_all_items(char, filter_keyword, connection_id);
    if items_to_give.len() == 0 { return; }

    let given = 0;
    let skipped = 0;
    for item in items_to_give {
        if item.flags.no_drop && !char.god_mode { skipped += 1; continue; }
        if item.flags.quest_item && !char.god_mode { skipped += 1; continue; }
        move_item_to_mobile_inventory(item.id, target_mobile.id);
        given += 1;
    }

    if given > 0 {
        if given == 1 {
            send_client_message(connection_id, "You give 1 item to " + target_mobile.name + ".");
        } else {
            send_client_message(connection_id, "You give " + given + " items to " + target_mobile.name + ".");
        }
        broadcast_to_room(char.current_room_id, char.name + " gives " + target_mobile.name + " several items.", char.name);
    } else if skipped > 0 {
        send_client_message(connection_id, "You couldn't give any of those items.");
    }
}

// ========== Parsing ==========

fn try_shorthand_syntax(char, args) {
    let result = #{ item: "", target: "" };

    let words = args.split(" ");
    if words.len() < 2 {
        return result;
    }

    let potential_target = words[words.len() - 1];

    // Player in same room?
    let target_conn = find_player_connection_by_name(potential_target);
    if target_conn != "" {
        let target_char = get_player_character(target_conn);
        if target_char != () && target_char.current_room_id == char.current_room_id {
            result.target = potential_target;
            result.item = get_all_but_last(words);
            return result;
        }
    }

    // Mobile in current room?
    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(potential_target, mobiles);
    if mobile != () {
        result.target = potential_target;
        result.item = get_all_but_last(words);
    }

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

fn parse_give_syntax(args) {
    let result = #{ item: "", target: "" };

    let parts = args.split(" to ");
    if parts.len() >= 2 {
        result.item = parts[0];
        result.target = parts[1];
        return result;
    }

    let words = args.split(" ");
    let item_words = [];
    let target_words = [];
    let found_to = false;

    for word in words {
        if word.to_lower() == "to" && !found_to {
            found_to = true;
        } else {
            if found_to {
                target_words.push(word);
            } else {
                item_words.push(word);
            }
        }
    }

    if found_to && item_words.len() > 0 && target_words.len() > 0 {
        result.item = join_words(item_words);
        result.target = join_words(target_words);
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
