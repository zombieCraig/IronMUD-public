// scripts/commands/planttreat.rhai
// planttreat <plant> - Treat a plant infestation

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
        send_client_message(connection_id, "Usage: planttreat <plant>");
        return;
    }

    // Find plant in room
    let plants = get_plants_in_room(char.current_room_id);
    let target_plant = ();
    let target_proto = ();

    for p in plants {
        if p.infestation == "none" { continue; }
        if p.stage == "dead" { continue; }
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
        // Check if there's a plant without infestation
        for p in plants {
            let proto = get_plant_prototype_by_vnum(p.prototype_vnum);
            if proto == () { continue; }
            let name_lower = proto.name.to_lower();
            if name_lower.contains(plant_keyword) {
                send_client_message(connection_id, "The " + proto.name + " doesn't have any infestation to treat.");
                return;
            }
        }
        send_client_message(connection_id, "You don't see an infested plant matching '" + plant_keyword + "' here.");
        return;
    }

    // Find treatment in inventory
    let inventory = get_items_in_inventory(char.name);
    let treatment = ();
    for item in inventory {
        if item.treats_infestation == "" { continue; }
        // Check if treatment matches infestation type or is "all"
        if item.treats_infestation == "all" || item.treats_infestation == target_plant.infestation {
            treatment = item;
            break;
        }
    }

    if treatment == () {
        send_client_message(connection_id, "You don't have any treatment for " + target_plant.infestation + " in your inventory. You need an item with the correct treats_infestation type.");
        return;
    }

    // Skill check for severe infestations
    let skill_level = get_skill_level(char.name, "gardening");
    if target_plant.infestation_severity > 0.5 && skill_level < 3 {
        send_client_message(connection_id, "This " + target_plant.infestation + " infestation is too severe for your current skill. (Need Gardening 3+, you have " + skill_level + ")");
        return;
    }

    // Apply treatment
    treat_plant_infestation(target_plant.id);

    // Consume treatment item
    delete_item(treatment.id);

    // Award XP (scaled by severity)
    let severity_mult = target_plant.infestation_severity;
    if severity_mult < 0.1 { severity_mult = 0.1; }
    let xp = 10 * severity_mult;
    if xp < 1 { xp = 1; }

    let leveled_up = add_skill_experience(char.name, "gardening", xp);
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_client_message(connection_id, "You treat the " + target_proto.name + "'s " + target_plant.infestation + " infestation with " + treatment.name + ". (+" + xp + " Gardening XP)");
    broadcast_to_room(char.current_room_id, char.name + " treats a " + target_proto.name + " for infestation.", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "gardening");
        send_client_message(connection_id, "\n*** Your Gardening skill increased to level " + new_level + "! ***");
    }
}
