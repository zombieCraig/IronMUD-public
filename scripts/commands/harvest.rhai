// scripts/commands/harvest.rhai
// harvest <plant> - Harvest a mature plant

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

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "You are sitting down. Use 'stand' to get up first.");
        return;
    }

    let plant_keyword = args.to_lower();
    if plant_keyword == "" {
        send_client_message(connection_id, "Usage: harvest <plant>");
        return;
    }

    // Find plant in room
    let plants = get_plants_in_room(char.current_room_id);
    let target_plant = ();
    let target_proto = ();

    for p in plants {
        if p.stage != "flowering" { continue; }
        let proto = get_plant_prototype_by_vnum(p.prototype_vnum);
        if proto == () { continue; }

        let name_lower = proto.name.to_lower();
        if name_lower.contains(plant_keyword) {
            target_plant = p;
            target_proto = proto;
            break;
        }
        for kw in proto.keywords {
            if kw.to_lower().contains(plant_keyword) {
                target_plant = p;
                target_proto = proto;
                break;
            }
        }
        if target_plant != () { break; }
    }

    if target_plant == () {
        // Check if there's a matching plant that's not flowering
        for p in plants {
            let proto = get_plant_prototype_by_vnum(p.prototype_vnum);
            if proto == () { continue; }
            let name_lower = proto.name.to_lower();
            if name_lower.contains(plant_keyword) {
                if p.stage == "dead" {
                    send_client_message(connection_id, "That " + proto.name + " is dead and cannot be harvested.");
                } else {
                    send_client_message(connection_id, "The " + proto.name + " is not ready for harvest yet. (Stage: " + p.stage + ")");
                }
                return;
            }
        }
        send_client_message(connection_id, "You don't see a harvestable plant matching '" + plant_keyword + "' here.");
        return;
    }

    // Permission check: planter or group member
    let name_lower = char.name.to_lower();
    let planter_lower = target_plant.planter_name.to_lower();
    let has_permission = false;

    if name_lower == planter_lower { has_permission = true; }
    if char.is_admin { has_permission = true; }

    if !has_permission {
        for member in target_plant.group_members {
            if member.to_lower() == name_lower {
                has_permission = true;
                break;
            }
        }
    }

    if !has_permission {
        send_client_message(connection_id, "This " + target_proto.name + " was planted by " + target_plant.planter_name + ". Only the planter or their group can harvest it.");
        return;
    }

    // Calculate yield via the action function
    let skill_level = get_skill_level(char.name, "gardening");
    let yield_count = harvest_plant(target_plant.id, skill_level);

    if yield_count <= 0 {
        send_client_message(connection_id, "Something went wrong during the harvest.");
        return;
    }

    // Spawn harvested items to inventory
    let items_given = 0;
    let harvest_item_name = "produce";
    let y = 0;
    while y < yield_count {
        let item = spawn_item_to_inventory(target_proto.harvest_vnum, char.name);
        if item != () {
            items_given += 1;
            harvest_item_name = item.name;
        }
        y += 1;
    }

    if items_given == 0 {
        send_client_message(connection_id, "The harvest produced nothing usable. (Harvest vnum '" + target_proto.harvest_vnum + "' not found)");
        return;
    }

    // Award XP
    let xp = target_proto.base_xp * items_given;
    if xp < 1 { xp = 1; }
    let leveled_up = add_skill_experience(char.name, "gardening", xp);
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Multi-harvest message
    let transition_msg = "";
    if target_proto.multi_harvest {
        transition_msg = " The plant returns to its growing stage.";
    }

    let count_msg = "";
    if items_given > 1 {
        count_msg = "" + items_given + "x ";
    }

    send_client_message(connection_id, "You harvest " + count_msg + harvest_item_name + " from the " + target_proto.name + "!" + transition_msg + " (+" + xp + " Gardening XP)");
    broadcast_to_room(char.current_room_id, char.name + " harvests from a " + target_proto.name + ".", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "gardening");
        let level_msg = get_level_up_message(new_level);
        send_client_message(connection_id, "\n*** Your Gardening skill increased to level " + new_level + "! ***");
        if level_msg != "" {
            send_client_message(connection_id, level_msg);
        }
    }
}
