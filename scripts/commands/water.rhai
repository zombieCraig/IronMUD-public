// scripts/commands/water.rhai
// water <plant> [with <item>] - Water a plant using a liquid container

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }

    let parts = split_quoted_args(args);
    if parts.len() == 0 {
        send_client_message(connection_id, "Usage: water <plant> [with <item>]");
        return;
    }

    // Parse: water <plant keyword> [with <tool keyword>]
    let plant_keyword = "";
    let tool_keyword = "";
    let use_tool = false;

    let with_index = -1;
    let i = 0;
    for part in parts {
        if part.to_lower() == "with" {
            with_index = i;
        }
        i += 1;
    }

    if with_index > 0 {
        let plant_parts = [];
        let j = 0;
        for part in parts {
            if j < with_index { plant_parts.push(part); }
            j += 1;
        }
        plant_keyword = plant_parts.join(" ").to_lower();

        let tool_parts = [];
        let k = 0;
        for part in parts {
            if k > with_index { tool_parts.push(part); }
            k += 1;
        }
        if tool_parts.len() > 0 {
            tool_keyword = tool_parts.join(" ").to_lower();
            use_tool = true;
        }
    } else {
        plant_keyword = args.to_lower();
    }

    // Get room and plants
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere.");
        return;
    }

    let plants = get_plants_in_room(char.current_room_id);
    if plants.len() == 0 {
        send_client_message(connection_id, "There are no plants here to water.");
        return;
    }

    // Find matching plant
    let target_plant = find_target_plant(plants, plant_keyword);
    let target_proto = ();
    if target_plant != () {
        target_proto = get_plant_prototype_by_vnum(target_plant.prototype_vnum);
    }

    if target_plant == () {
        send_client_message(connection_id, "You don't see a plant matching '" + plant_keyword + "' here.");
        return;
    }

    // Find liquid source
    let source = find_water_source(char, use_tool, tool_keyword, connection_id);
    if source == () {
        return; // Error message already sent
    }

    let liquid_type = source.liquid_type;

    // Consume sips (3 per watering)
    drink_from(source.id, 3);

    // Determine effect on plant based on liquid type
    let skill_level = get_skill_level(char.name, "gardening");
    let effect = get_liquid_plant_effect(liquid_type, skill_level);

    // Apply effect
    if effect.water_amount > 0.0 {
        water_plant(target_plant.id, effect.water_amount);
    }
    if effect.damage > 0.0 {
        damage_plant(target_plant.id, effect.damage);
    }

    // Award small XP
    let xp = 2;
    let leveled_up = add_skill_experience(char.name, "gardening", xp);
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Build messages
    let tool_msg = " with " + source.name;
    let effect_msg = build_effect_message(liquid_type, effect);

    send_client_message(connection_id, "You water the " + target_proto.name + tool_msg + ". (+" + xp + " Gardening XP)" + effect_msg);
    broadcast_to_room(char.current_room_id, char.name + " waters a " + target_proto.name + ".", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "gardening");
        send_client_message(connection_id, "\n*** Your Gardening skill increased to level " + new_level + "! ***");
    }
}

fn find_target_plant(plants, keyword) {
    for p in plants {
        if p.stage == "dead" { continue; }
        let proto = get_plant_prototype_by_vnum(p.prototype_vnum);
        if proto == () { continue; }
        let name_lower = proto.name.to_lower();
        if name_lower.contains(keyword) { return p; }
        for kw in proto.keywords {
            if kw.to_lower().contains(keyword) { return p; }
        }
    }
    return ();
}

fn find_water_source(char, use_tool, tool_keyword, connection_id) {
    if use_tool {
        return find_explicit_source(char, tool_keyword, connection_id);
    }
    return find_auto_source(char, connection_id);
}

fn find_explicit_source(char, tool_keyword, connection_id) {
    // Find matching liquid_container in inventory with liquid
    let inventory = get_items_in_inventory(char.name);
    for item in inventory {
        if !item.is_liquid_container { continue; }
        if item.liquid_current <= 0 { continue; }
        if matches_keyword(item, tool_keyword) {
            return item;
        }
    }
    send_client_message(connection_id, "You don't have a container with liquid matching '" + tool_keyword + "'.");
    return ();
}

fn find_auto_source(char, connection_id) {
    // Prefer water containers in inventory
    let inventory = get_items_in_inventory(char.name);
    for item in inventory {
        if !item.is_liquid_container { continue; }
        if item.liquid_current <= 0 { continue; }
        if item.liquid_type == "water" { return item; }
    }
    // Check equipped
    let equipped = get_equipped_items(char.name);
    for item in equipped {
        if !item.is_liquid_container { continue; }
        if item.liquid_current <= 0 { continue; }
        if item.liquid_type == "water" { return item; }
    }
    // Check room for water sources (infinite sources have liquid_max == -1)
    let room_items = get_items_in_room(char.current_room_id);
    for item in room_items {
        if !item.is_liquid_container { continue; }
        if item.liquid_type == "water" && (item.liquid_current > 0 || item.liquid_max == -1) {
            return item;
        }
    }
    // Fall back to any liquid container with liquid in inventory
    for item in inventory {
        if !item.is_liquid_container { continue; }
        if item.liquid_current <= 0 { continue; }
        return item;
    }
    send_client_message(connection_id, "You need a container with water. Try 'fill <container>' at a water source.");
    return ();
}

fn matches_keyword(item, keyword) {
    let name_lower = item.name.to_lower();
    if name_lower.contains(keyword) { return true; }
    for kw in item.keywords {
        if kw.to_lower().contains(keyword) { return true; }
    }
    return false;
}

fn get_liquid_plant_effect(liquid_type, skill_level) {
    let result = #{
        "water_amount": 0.0,
        "damage": 0.0
    };
    let base = 25.0 + (skill_level * 2);

    if liquid_type == "water" {
        result.water_amount = base;
        return result;
    }
    // Beneficial liquids: 75% efficiency + health boost
    if is_beneficial_liquid(liquid_type) {
        result.water_amount = base * 0.75;
        return result;
    }
    // Harmful liquids: no water, deal damage
    if is_harmful_liquid(liquid_type) {
        result.damage = get_harm_amount(liquid_type);
        return result;
    }
    // Neutral (ale, wine, beer, coffee): 50% efficiency
    result.water_amount = base * 0.5;
    return result;
}

fn is_beneficial_liquid(lt) {
    if lt == "healing_potion" { return true; }
    if lt == "tea" { return true; }
    if lt == "juice" { return true; }
    if lt == "milk" { return true; }
    return false;
}

fn is_harmful_liquid(lt) {
    if lt == "alcohol" { return true; }
    if lt == "poison" { return true; }
    if lt == "oil" { return true; }
    if lt == "blood" { return true; }
    return false;
}

fn get_harm_amount(lt) {
    if lt == "poison" { return 15.0; }
    if lt == "alcohol" { return 10.0; }
    if lt == "oil" { return 8.0; }
    if lt == "blood" { return 5.0; }
    return 5.0;
}

fn build_effect_message(liquid_type, effect) {
    if liquid_type == "water" { return ""; }
    if effect.damage > 0.0 {
        return "\n\x1b[1;31mThe " + liquid_type + " harms the plant!\x1b[0m";
    }
    if effect.water_amount > 0.0 {
        return "\nThe " + liquid_type + " provides some hydration.";
    }
    return "";
}
