// scripts/commands/peek.rhai
// peek <target> - Secretly view a target's inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Peek at whom?");
        return;
    }

    if char.stamina < 3 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 3 stamina)");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 3;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    let target_keyword = args.to_lower();

    // Find target mobile (filter invisible ones the peeker can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    let target_mobile = find_mobile_by_keyword(target_keyword, mobiles);
    let target_mobile_id = "";
    let target_name = "";
    if target_mobile != () {
        target_mobile_id = target_mobile.id;
        target_name = target_mobile.name;
    }

    // Check for player target
    let target_is_player = false;
    if target_mobile_id == "" {
        let players = get_characters_in_room(room_id);
        for pname in players {
            if pname == char.name { continue; }
            if pname.to_lower() == target_keyword {
                target_is_player = true;
                target_name = pname;
            }
        }
    }

    if target_mobile_id == "" && !target_is_player {
        send_client_message(connection_id, "You don't see '" + args + "' here.");
        return;
    }

    // Stealth check
    let detected = false;
    if target_is_player {
        detected = !stealth_check_vs_player(char.name, target_name, room_id);
    } else {
        detected = !stealth_check_vs_mobile(char.name, target_mobile_id, room_id);
    }

    if detected {
        send_client_message(connection_id, target_name + " catches you looking at their belongings!");
        if target_is_player {
            let target_conn = find_player_connection_by_name(target_name);
            if target_conn != "" {
                send_client_message(target_conn, char.name + " was caught peeking at your inventory!");
            }
        }
        // Award small XP on failure to allow progression from level 0
        let leveled = add_skill_experience(char.name, "thievery", 1);
        if leveled {
            send_client_message(connection_id, "Your thievery skill has improved!");
        }
        return;
    }

    // Success - show inventory
    let output = "You discreetly peek at " + target_name + "'s belongings:\n";
    let items = [];
    if target_is_player {
        items = get_items_in_inventory(target_name);
    } else {
        items = get_items_in_mobile_inventory(target_mobile_id);
    }

    if items.len() == 0 {
        output += "  (nothing)\n";
    } else {
        for item in items {
            output += "  " + item.name + "\n";
        }
    }

    // Show gold
    if target_is_player {
        let target_char = get_character_data(target_name);
        if target_char != () && target_char.gold > 0 {
            output += "  [" + target_char.gold + " gold coins]\n";
        }
    } else {
        let mob = get_mobile_data(target_mobile_id);
        if mob != () && mob.gold > 0 {
            output += "  [" + mob.gold + " gold coins]\n";
        }
    }

    send_client_message(connection_id, output);

    let leveled = add_skill_experience(char.name, "thievery", 3);
    if leveled {
        send_client_message(connection_id, "Your thievery skill has improved!");
    }
}
