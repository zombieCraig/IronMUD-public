// scripts/commands/steal.rhai
// steal <gold|item> from <target> - Attempt to steal from a target

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
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    let thievery_skill = get_skill_level(char.name, "thievery");

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't steal while in combat!");
        return;
    }

    // Parse: steal gold from <target> OR steal <item> from <target> OR steal from <target>
    let parts = args.split(" ");
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: steal <gold|item> from <target>  OR  steal from <target>");
        return;
    }

    let steal_type = "gold";
    let target_keyword = "";

    // Parse "steal from <target>" (default to gold)
    if parts[0] == "from" {
        target_keyword = parts[1];
    } else if parts.len() >= 3 && parts[1] == "from" {
        steal_type = parts[0].to_lower();
        target_keyword = parts[2];
    } else {
        // Simple: steal <target> = steal gold from target
        target_keyword = parts[0];
    }

    if target_keyword == "" {
        send_client_message(connection_id, "Steal from whom?");
        return;
    }

    // Check for items: need thievery >= 3
    if steal_type != "gold" && thievery_skill < 3 {
        send_client_message(connection_id, "You need thievery level 3 to steal items.");
        return;
    }

    // Check stamina
    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted to steal. (Need 10 stamina)");
        return;
    }

    let room_id = char.current_room_id;

    // Check zone rules
    let zone = get_combat_zone(room_id);
    if zone == "safe" {
        send_client_message(connection_id, "Stealing is not allowed in safe zones.");
        return;
    }

    // Find target - check mobiles first (filter invisible ones the thief can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    let target_mobile_id = "";
    let target_mobile_name = "";
    let target_is_player = false;
    let target_player_name = "";

    for mobile in mobiles {
        for keyword in mobile.keywords {
            if keyword == target_keyword.to_lower() {
                target_mobile_id = mobile.id;
                target_mobile_name = mobile.name;
            }
        }
        if mobile.name.to_lower() == target_keyword.to_lower() {
            target_mobile_id = mobile.id;
            target_mobile_name = mobile.name;
        }
    }

    // Check for player target (PvP zones only)
    if target_mobile_id == "" {
        if zone == "pvp" {
            let players = get_characters_in_room(room_id);
            for pname in players {
                if pname == char.name { continue; }
                if pname.to_lower() == target_keyword.to_lower() {
                    target_is_player = true;
                    target_player_name = pname;
                }
            }
        }
        if !target_is_player && target_mobile_id == "" {
            if zone != "pvp" {
                send_client_message(connection_id, "You don't see '" + target_keyword + "' here. (Stealing from players requires a PvP zone)");
            } else {
                send_client_message(connection_id, "You don't see '" + target_keyword + "' here.");
            }
            return;
        }
    }

    // Check cooldown
    let cooldown_key = target_mobile_id;
    if target_is_player { cooldown_key = target_player_name; }
    let remaining = get_theft_cooldown(char.name, cooldown_key);
    if remaining > 0 {
        send_client_message(connection_id, "You must wait " + remaining + " seconds before stealing from them again.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Calculate success chance: 30 + (thievery * 7) + (dex_mod * 2) - (target_level * 3), cap 95
    let dex_mod = (char.stat_dex - 10) / 2;
    let target_level = 1;
    if target_is_player {
        let target_char = get_character_data(target_player_name);
        if target_char != () { target_level = target_char.level; }
    } else {
        let target_mob = get_mobile_data(target_mobile_id);
        if target_mob != () { target_level = target_mob.level; }
    }

    let success_chance = 30 + (thievery_skill * 7) + (dex_mod * 2) - (target_level * 3);
    if success_chance > 95 { success_chance = 95; }
    if success_chance < 5 { success_chance = 5; }

    let roll = random_int(1, 100);
    let success = roll <= success_chance;

    // Set cooldown regardless
    set_theft_cooldown(char.name, cooldown_key, 30);

    if success {
        // Successful steal
        if steal_type == "gold" {
            attempt_steal_gold(char, connection_id, target_is_player, target_player_name, target_mobile_id, target_mobile_name);
        } else {
            attempt_steal_item(char, connection_id, target_is_player, target_player_name, target_mobile_id, target_mobile_name);
        }
        // Award XP
        let leveled = add_skill_experience(char.name, "thievery", 10);
        if leveled {
            send_client_message(connection_id, "Your thievery skill has improved!");
        }
    } else {
        // Failed steal
        break_stealth(char.name);
        if target_is_player {
            send_client_message(connection_id, "You fumble the attempt and " + target_player_name + " notices!");
            // Notify the target
            let target_conn = find_player_connection_by_name(target_player_name);
            if target_conn != "" {
                send_client_message(target_conn, char.name + " tried to steal from you!");
            }
        } else {
            send_client_message(connection_id, "You fumble the attempt and " + target_mobile_name + " catches you!");
            broadcast_to_room_awake(room_id, target_mobile_name + " catches " + char.name + " trying to steal!", char.name);
            // Mobile enters combat with thief
            enter_combat(char.name, "mobile", target_mobile_id);
            enter_mobile_combat(target_mobile_id, "player", char.name);
        }

        // Award small XP on failure to allow progression from level 0
        let leveled = add_skill_experience(char.name, "thievery", 1);
        if leveled {
            send_client_message(connection_id, "Your thievery skill has improved!");
        }
    }
}

fn attempt_steal_gold(char, connection_id, is_player, player_name, mobile_id, mobile_name) {
    if is_player {
        let target = get_character_data(player_name);
        if target == () || target.gold <= 0 {
            send_client_message(connection_id, "They don't seem to have any gold.");
            return;
        }
        let amount = random_int(1, target.gold / 4);
        if amount < 1 { amount = 1; }
        // Transfer gold
        add_character_gold(char.name, amount);
        add_character_gold(player_name, -amount);
        send_client_message(connection_id, "You deftly lift " + amount + " gold from " + player_name + "'s purse.");
    } else {
        let mob = get_mobile_data(mobile_id);
        if mob == () || mob.gold <= 0 {
            send_client_message(connection_id, "They don't seem to have any gold.");
            return;
        }
        let amount = random_int(1, mob.gold / 4);
        if amount < 1 { amount = 1; }
        add_character_gold(char.name, amount);
        set_mobile_gold(mobile_id, mob.gold - amount);
        send_client_message(connection_id, "You deftly lift " + amount + " gold from " + mobile_name + ".");
    }
}

fn attempt_steal_item(char, connection_id, is_player, player_name, mobile_id, mobile_name) {
    // Get a random non-equipped item from target inventory
    let items = [];
    if is_player {
        items = get_items_in_inventory(player_name);
    } else {
        items = get_items_in_mobile_inventory(mobile_id);
    }

    if items.len() == 0 {
        send_client_message(connection_id, "They don't seem to have anything to steal.");
        return;
    }

    // Pick a random item
    let idx = random_int(0, items.len() - 1);
    let stolen_item = items[idx];

    // Move item to thief's inventory
    move_item_to_inventory(stolen_item.id, char.name);
    send_client_message(connection_id, "You slip " + stolen_item.name + " from " + if is_player { player_name } else { mobile_name } + "'s belongings.");
}
