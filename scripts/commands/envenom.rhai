// scripts/commands/envenom.rhai
// envenom - Apply poison to your wielded weapon

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
    if thievery_skill < 4 {
        send_client_message(connection_id, "You need thievery level 4 to envenom weapons.");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    // Check if wielding a weapon
    let has_weapon = false;
    let weapon_name = "";
    let equipped = get_equipped_items(char.name);
    for item in equipped {
        let locs = item.wear_locations;
        for loc in locs {
            if loc.to_lower() == "wielded" {
                has_weapon = true;
                weapon_name = item.name;
            }
        }
    }

    if !has_weapon {
        send_client_message(connection_id, "You need to be wielding a weapon to envenom it.");
        return;
    }

    // Check for already envenomed
    let charges = get_envenomed_charges(char.name);
    if charges > 0 {
        send_client_message(connection_id, "Your weapon is already envenomed (" + charges + " charges remaining).");
        return;
    }

    // Check for poison vial in inventory
    let inv_items = get_items_in_inventory(char.name);
    let poison_id = "";
    let poison_name = "";
    for item in inv_items {
        for keyword in item.keywords {
            if keyword == "poison_vial" || keyword == "poison" || keyword == "venom" {
                poison_id = item.id;
                poison_name = item.name;
            }
        }
    }

    if poison_id == "" {
        send_client_message(connection_id, "You need a poison vial to envenom your weapon.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Consume the poison vial
    delete_item(poison_id);

    // Apply 3-5 envenomed charges
    let charges = 3 + (thievery_skill / 3);
    if charges > 5 { charges = 5; }
    set_envenomed_charges(char.name, charges);

    send_client_message(connection_id, "You carefully apply " + poison_name + " to your " + weapon_name + ". (" + charges + " envenomed charges)");
    broadcast_to_room_awake(char.current_room_id, char.name + " carefully applies something to their weapon.", char.name);

    let leveled = add_skill_experience(char.name, "thievery", 8);
    if leveled {
        send_client_message(connection_id, "Your thievery skill has improved!");
    }
}
