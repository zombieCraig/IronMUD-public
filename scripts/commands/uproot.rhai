// scripts/commands/uproot.rhai
// uproot <plant> - Remove a planted plant

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
        send_client_message(connection_id, "Usage: uproot <plant>");
        return;
    }

    // Find plant in room
    let plants = get_plants_in_room(char.current_room_id);
    let target_plant = ();
    let target_proto = ();

    for p in plants {
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

    // Permission check: planter or admin
    let name_lower = char.name.to_lower();
    let planter_lower = target_plant.planter_name.to_lower();
    let has_permission = false;

    if name_lower == planter_lower { has_permission = true; }
    if char.is_admin { has_permission = true; }
    if char.is_builder { has_permission = true; }

    if !has_permission {
        send_client_message(connection_id, "This " + target_proto.name + " was planted by " + target_plant.planter_name + ". Only the planter or staff can uproot it.");
        return;
    }

    // Delete the plant
    let plant_name = target_proto.name;
    let was_potted = target_plant.is_potted;
    delete_plant_instance(target_plant.id);

    send_client_message(connection_id, "You uproot the " + plant_name + ".");
    broadcast_to_room(char.current_room_id, char.name + " uproots a " + plant_name + ".", char.name);
}
