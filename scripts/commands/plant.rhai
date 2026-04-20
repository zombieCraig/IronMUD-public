// scripts/commands/plant.rhai
// plant <seed> [in <pot>] - Plant a seed in the ground or a pot

fn get_level_up_message(level) {
    if level == 1 { return "You can feel the soil speaking to you."; }
    if level == 2 { return "Your green thumb is developing nicely."; }
    if level == 3 { return "You've learned to read the signs of the seasons."; }
    if level == 4 { return "Your garden knowledge grows with each planting."; }
    if level == 5 { return "You're becoming a skilled gardener!"; }
    if level == 6 { return "Even difficult plants thrive under your care."; }
    if level == 7 { return "Your reputation as a gardener precedes you."; }
    if level == 8 { return "The rarest seeds respond to your expert touch."; }
    if level == 9 { return "You're approaching mastery of the gardener's art."; }
    if level == 10 { return "You have achieved MASTERY in Gardening! All things grow at your command."; }
    return "";
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

    // Check stamina
    if !char.god_mode && char.stamina < 1 {
        send_client_message(connection_id, "You are too exhausted to plant anything.");
        return;
    }

    let parts = split_quoted_args(args);
    if parts.len() == 0 {
        send_client_message(connection_id, "Usage: plant <seed> [in <pot>]");
        return;
    }

    // Parse arguments: plant <seed keyword> [in <pot keyword>]
    let seed_keyword = "";
    let pot_keyword = "";
    let in_pot = false;

    // Find "in" separator
    let in_index = -1;
    let i = 0;
    for part in parts {
        if part.to_lower() == "in" {
            in_index = i;
        }
        i += 1;
    }

    if in_index > 0 {
        // Everything before "in" is seed keyword
        let seed_parts = [];
        let j = 0;
        for part in parts {
            if j < in_index {
                seed_parts.push(part);
            }
            j += 1;
        }
        seed_keyword = seed_parts.join(" ").to_lower();

        // Everything after "in" is pot keyword
        let pot_parts = [];
        let k = 0;
        for part in parts {
            if k > in_index {
                pot_parts.push(part);
            }
            k += 1;
        }
        if pot_parts.len() > 0 {
            pot_keyword = pot_parts.join(" ").to_lower();
            in_pot = true;
        }
    } else {
        seed_keyword = args.to_lower();
        if seed_keyword == "" {
            send_client_message(connection_id, "Usage: plant <seed> [in <pot>]");
            return;
        }
    }

    // Trim whitespace from keywords
    while seed_keyword.len() > 0 && seed_keyword.sub_string(0, 1) == " " {
        seed_keyword = seed_keyword.sub_string(1);
    }

    // Get room data
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }

    // Find seed in inventory
    let inventory = get_items_in_inventory(char.name);
    let seed = ();
    for item in inventory {
        if item.plant_prototype_vnum == "" {
            continue;
        }
        // Match by keyword or name
        let name_lower = item.name.to_lower();
        if name_lower.contains(seed_keyword) {
            seed = item;
            break;
        }
        for kw in item.keywords {
            if kw.to_lower().contains(seed_keyword) {
                seed = item;
                break;
            }
        }
        if seed != () { break; }
    }

    if seed == () {
        send_client_message(connection_id, "You don't have any seeds matching '" + seed_keyword + "' in your inventory.");
        return;
    }

    // Load the plant prototype
    let proto = get_plant_prototype_by_vnum(seed.plant_prototype_vnum);
    if proto == () {
        send_client_message(connection_id, "These seeds don't seem to be viable. (Plant prototype '" + seed.plant_prototype_vnum + "' not found)");
        return;
    }

    // Check gardening skill
    let skill_level = get_skill_level(char.name, "gardening");
    if skill_level < proto.min_skill_to_plant {
        send_client_message(connection_id, "You need Gardening skill level " + proto.min_skill_to_plant + " to plant " + proto.name + ". (You have " + skill_level + ")");
        return;
    }

    let pot_item = ();
    let pot_item_id = "";

    if in_pot {
        // Find pot in room or inventory
        let found_pot = false;

        // Check inventory first
        for item in inventory {
            if !item.is_plant_pot { continue; }
            let name_lower = item.name.to_lower();
            if name_lower.contains(pot_keyword) {
                pot_item = item;
                found_pot = true;
                break;
            }
            for kw in item.keywords {
                if kw.to_lower().contains(pot_keyword) {
                    pot_item = item;
                    found_pot = true;
                    break;
                }
            }
            if found_pot { break; }
        }

        // Check room items if not found in inventory
        if !found_pot {
            let room_items = get_items_in_room(char.current_room_id);
            for item in room_items {
                if !item.is_plant_pot { continue; }
                let name_lower = item.name.to_lower();
                if name_lower.contains(pot_keyword) {
                    pot_item = item;
                    found_pot = true;
                    break;
                }
                for kw in item.keywords {
                    if kw.to_lower().contains(pot_keyword) {
                        pot_item = item;
                        found_pot = true;
                        break;
                    }
                }
                if found_pot { break; }
            }
        }

        if !found_pot {
            send_client_message(connection_id, "You can't find a pot matching '" + pot_keyword + "'.");
            return;
        }
        pot_item_id = pot_item.id;
    } else {
        // Ground planting - check requirements
        if proto.indoor_only {
            send_client_message(connection_id, proto.name + " can only be planted in a pot.");
            return;
        }

        if !room.flags.dirt_floor {
            send_client_message(connection_id, "The ground here isn't suitable for planting. You need soft earth (dirt_floor rooms) or use a pot.");
            return;
        }

        // Check max ground plants (5 per room)
        let ground_count = count_ground_plants_in_room(char.current_room_id);
        if ground_count >= 5 {
            send_client_message(connection_id, "There's no more room to plant in the ground here. (Max 5 ground plants per room)");
            return;
        }
    }

    // Spend stamina
    if !char.god_mode {
        char.stamina = char.stamina - 1;
        if char.stamina < 0 { char.stamina = 0; }
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Create the plant instance
    let plant_id = create_plant_instance(seed.plant_prototype_vnum, char.current_room_id, char.name, in_pot, pot_item_id);
    if plant_id == "" {
        send_client_message(connection_id, "Something went wrong creating the plant.");
        return;
    }

    // Consume the seed
    delete_item(seed.id);

    // Award gardening XP
    let xp = 5;
    let leveled_up = add_skill_experience(char.name, "gardening", xp);
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Send messages
    let location_msg = "";
    if in_pot {
        location_msg = " in " + pot_item.name;
    } else {
        location_msg = " in the ground";
    }
    send_client_message(connection_id, "You carefully plant " + seed.name + location_msg + ". (+" + xp + " Gardening XP)");
    broadcast_to_room(char.current_room_id, char.name + " plants some seeds" + location_msg + ".", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "gardening");
        let level_msg = get_level_up_message(new_level);
        send_client_message(connection_id, "\n*** Your Gardening skill increased to level " + new_level + "! ***");
        if level_msg != "" {
            send_client_message(connection_id, level_msg);
        }
    }
}
