// scripts/commands/fertilize.rhai
// fertilize <plant> - Apply fertilizer to a plant

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

    let plant_keyword = args.to_lower();
    if plant_keyword == "" {
        send_client_message(connection_id, "Usage: fertilize <plant>");
        return;
    }

    // Find fertilizer in inventory
    let inventory = get_items_in_inventory(char.name);
    let fertilizer = ();
    for item in inventory {
        if item.fertilizer_duration > 0 {
            fertilizer = item;
            break;
        }
    }

    if fertilizer == () {
        send_client_message(connection_id, "You don't have any fertilizer in your inventory.");
        return;
    }

    // Find plant in room
    let plants = get_plants_in_room(char.current_room_id);
    let target_plant = ();
    let target_proto = ();

    for p in plants {
        if p.stage == "dead" || p.stage == "seed" { continue; }
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
        send_client_message(connection_id, "You don't see a plant matching '" + plant_keyword + "' here.");
        return;
    }

    // Check not already fertilized
    if target_plant.fertilized {
        send_client_message(connection_id, "The " + target_proto.name + " is already fertilized.");
        return;
    }

    // Apply fertilizer
    fertilize_plant(target_plant.id, fertilizer.fertilizer_duration);

    // Consume fertilizer
    delete_item(fertilizer.id);

    // Award XP
    let xp = 3;
    let leveled_up = add_skill_experience(char.name, "gardening", xp);
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_client_message(connection_id, "You apply " + fertilizer.name + " to the " + target_proto.name + ". Growth will be boosted for " + fertilizer.fertilizer_duration + " game hours. (+" + xp + " Gardening XP)");
    broadcast_to_room(char.current_room_id, char.name + " fertilizes a " + target_proto.name + ".", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "gardening");
        send_client_message(connection_id, "\n*** Your Gardening skill increased to level " + new_level + "! ***");
    }
}
