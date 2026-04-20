// scripts/commands/forage.rhai
// forage - Search the area for useful items
// Requires: Room with city, dirt_floor, or water flag, 1 stamina
// Optional: foraging_tool equipped for bonus

fn get_level_up_message(level) {
    if level == 1 { return "You're learning to identify useful plants."; }
    if level == 2 { return "Your eye for edible items improves."; }
    if level == 3 { return "You recognize more of what nature offers."; }
    if level == 4 { return "You've learned where to look for hidden treasures."; }
    if level == 5 { return "You're becoming a skilled forager!"; }
    if level == 6 { return "Your knowledge of local flora grows."; }
    if level == 7 { return "You can spot valuable items others would miss."; }
    if level == 8 { return "Expert foragers would respect your technique."; }
    if level == 9 { return "You're approaching mastery of the forager's art."; }
    if level == 10 { return "You have achieved MASTERY in Foraging! Nature reveals all her secrets to you."; }
    return "";
}

fn get_forage_table_for_type(area, forage_type) {
    if forage_type == "city" { return area.city_forage_table; }
    if forage_type == "shallow_water" { return area.shallow_water_forage_table; }
    if forage_type == "deep_water" { return area.deep_water_forage_table; }
    if forage_type == "underwater" { return area.underwater_forage_table; }
    return area.wilderness_forage_table;
}

fn get_rarity_xp_multiplier(rarity) {
    if rarity == "common" { return 10; }      // 1.0x but using int
    if rarity == "uncommon" { return 15; }    // 1.5x
    if rarity == "rare" { return 25; }        // 2.5x
    if rarity == "legendary" { return 50; }   // 5.0x
    return 10;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "You are sitting down. Use 'stand' to get up first.");
        return;
    }

    // Check stamina (god mode bypasses)
    if !char.god_mode && char.stamina < 1 {
        send_client_message(connection_id, "You are too exhausted to forage.");
        return;
    }

    // Get room data
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }

    // Check room supports foraging (water, city, or dirt_floor flag)
    let is_city = room.flags.city;
    let is_wilderness = room.flags.dirt_floor;
    let is_shallow_water = room.flags.shallow_water;
    let is_deep_water = room.flags.deep_water;
    let is_underwater_room = room.flags.underwater;

    if !is_city && !is_wilderness && !is_shallow_water && !is_deep_water && !is_underwater_room {
        send_client_message(connection_id, "There's nothing to forage here. Look for natural areas or city streets.");
        return;
    }

    // Determine forage type (water takes priority, then city, then wilderness)
    let forage_type = "wilderness";
    if is_city {
        forage_type = "city";
    }
    if is_shallow_water {
        forage_type = "shallow_water";
    }
    if is_deep_water {
        forage_type = "deep_water";
    }
    if is_underwater_room {
        forage_type = "underwater";
    }

    // Check per-room cooldown
    let cooldown = get_forage_cooldown(char.name, char.current_room_id);
    if cooldown > 0 {
        send_client_message(connection_id, "You've already searched this area recently. Try exploring elsewhere. (" + cooldown + " seconds remaining)");
        return;
    }

    // Get room's area
    let area_id = room.area_id;
    if area_id == "" {
        send_client_message(connection_id, "This area has no foraging configured.");
        return;
    }

    let area = get_area_data(area_id);
    if area == () {
        send_client_message(connection_id, "This area has no foraging configured.");
        return;
    }

    // Check if area has forage table for this type
    let forage_table = get_forage_table_for_type(area, forage_type);

    if forage_table.len() == 0 {
        send_client_message(connection_id, "There's nothing to find here.");
        return;
    }

    // Find equipped foraging tool (optional)
    let equipped_items = get_equipped_items(char.name);
    let tool = ();
    let tool_quality = 0;
    for item in equipped_items {
        if item.is_foraging_tool {
            tool = item;
            tool_quality = item.quality;
        }
    }

    // Get skill level
    let skill_level = get_skill_level(char.name, "foraging");

    // Spend stamina (1 point + heat/thirst penalty) - god mode bypasses
    if !char.god_mode {
        let heat_penalty = get_heat_stamina_penalty(connection_id);
        let thirst_penalty = get_thirst_stamina_penalty(connection_id);
        char.stamina = char.stamina - 1 - heat_penalty - thirst_penalty;
        if char.stamina < 0 { char.stamina = 0; }
        save_character_data(char);
        set_player_character(connection_id, char);

        // Show warning messages for penalties
        if heat_penalty > 0 {
            send_client_message(connection_id, "The heat is draining.");
        }
        if thirst_penalty > 0 {
            send_client_message(connection_id, "Your thirst is sapping your strength.");
        }
    }

    // Set cooldown for this room
    set_forage_timestamp(char.name, char.current_room_id);

    // Calculate find chance
    // base = 35% + skill_level * 5% + tool_quality / 2 + tracking_bonus
    let tracking_bonus = get_skill_level(char.name, "tracking") * 2;
    let find_chance = 35 + (skill_level * 5) + (tool_quality / 2) + tracking_bonus;
    if find_chance < 10 { find_chance = 10; }
    if find_chance > 95 { find_chance = 95; }

    let find_roll = random_int(1, 100);

    // Send search message
    let search_msg = "You search the area";
    if tool != () {
        search_msg += " with your " + tool.name;
    }
    search_msg += "...";
    send_client_message(connection_id, search_msg);
    broadcast_to_room(char.current_room_id, char.name + " searches the area for useful items.", char.name);

    if find_roll > find_chance {
        // Failed to find anything
        send_client_message(connection_id, "You search but find nothing useful.");
        return;
    }

    // Success! Select an item from the forage table
    let forage_result = select_area_forage(area_id, forage_type, skill_level);

    if forage_result == () {
        // No valid item (shouldn't normally happen)
        send_client_message(connection_id, "You search but find nothing useful.");
        return;
    }

    let forage_vnum = forage_result.vnum;
    let forage_rarity = forage_result.rarity;

    // Spawn the item to inventory
    let found_item = spawn_item_to_inventory(forage_vnum, char.name);

    if found_item == () {
        // Failed to spawn (vnum doesn't exist)
        send_client_message(connection_id, "You found something interesting, but it crumbled in your hands! (Item vnum '" + forage_vnum + "' not found)");
        return;
    }

    // Calculate and award XP
    let base_xp = 10;
    let rarity_mult = get_rarity_xp_multiplier(forage_rarity);
    let skill_diminish = 10 - (skill_level / 2); // 10, 9.5, 9, ... down to 5
    if skill_diminish < 5 { skill_diminish = 5; }

    let xp_gained = (base_xp * rarity_mult * skill_diminish) / 100;
    if xp_gained < 1 { xp_gained = 1; }

    let leveled_up = add_skill_experience(char.name, "foraging", xp_gained);

    // Refresh session with updated character data (including new skills)
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Send success messages
    let rarity_prefix = "";
    if forage_rarity == "uncommon" { rarity_prefix = "a nice "; }
    if forage_rarity == "rare" { rarity_prefix = "an excellent "; }
    if forage_rarity == "legendary" { rarity_prefix = "an incredible "; }

    send_client_message(connection_id, "You found " + rarity_prefix + found_item.name + "! (+" + xp_gained + " Foraging XP)");
    broadcast_to_room(char.current_room_id, char.name + " finds something interesting!", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "foraging");
        let level_msg = get_level_up_message(new_level);
        send_client_message(connection_id, "\n*** Your Foraging skill increased to level " + new_level + "! ***");
        if level_msg != "" {
            send_client_message(connection_id, level_msg);
        }
    }
}
