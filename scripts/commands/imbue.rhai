// scripts/commands/imbue.rhai
// imbue <ammo> [with <vial>] - Coat a stack of ammunition with poison.
//
// Mirrors envenom: thievery 4 hard gate, costs 10 stamina, consumes the entire
// vial. The whole ammo stack is imbued in one action — players think in
// quivers, not arrows. shoot.rhai / snipe.rhai already pull ammo_effect_type
// off the projectile and apply it on hit.

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
        send_client_message(connection_id, "You need thievery level 4 to imbue ammunition.");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Imbue what? (imbue <ammo> [with <vial>])");
        return;
    }

    // Parse "<ammo> with <vial>" or just "<ammo>"
    let ammo_keyword = "";
    let vial_keyword = "";
    let lower = args.to_lower();
    let with_idx = lower.index_of(" with ");
    if with_idx >= 0 {
        ammo_keyword = args.sub_string(0, with_idx);
        vial_keyword = args.sub_string(with_idx + 6);
    } else {
        ammo_keyword = args;
    }
    ammo_keyword = ammo_keyword.to_lower();
    vial_keyword = vial_keyword.to_lower();

    // Resolve ammo target.
    let ammo = find_item_in_inventory(char.name, ammo_keyword);
    if ammo == () {
        send_client_message(connection_id, "You don't have '" + ammo_keyword + "'.");
        return;
    }
    if !ammo.is_ammunition {
        send_client_message(connection_id, "That isn't ammunition.");
        return;
    }
    if ammo.ammo_effect_type != "" {
        send_client_message(connection_id, ammo.name + " is already imbued with " + ammo.ammo_effect_type + ".");
        return;
    }

    // Resolve poison vial.
    let vial = ();
    if vial_keyword != "" {
        vial = find_item_in_inventory(char.name, vial_keyword);
        if vial == () {
            send_client_message(connection_id, "You don't have '" + vial_keyword + "'.");
            return;
        }
        if !vial.is_liquid_container || vial.liquid_type != "poison" {
            send_client_message(connection_id, "That isn't poison.");
            return;
        }
    } else {
        let inv = get_items_in_inventory(char.name);
        for item in inv {
            if item.is_liquid_container && item.liquid_type == "poison" && item.liquid_current > 0 {
                vial = item;
                break;
            }
        }
        if vial == () {
            send_client_message(connection_id, "You aren't carrying any poison.");
            return;
        }
    }

    if vial.liquid_current <= 0 {
        send_client_message(connection_id, vial.name + " is empty.");
        return;
    }

    // Pay the stamina cost.
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Imbue the whole stack with a poison DoT — values match envenom (3 rounds,
    // 3 dpr) so melee and ranged poison hit the same.
    set_item_ammo_effect_type(ammo.id, "poison");
    set_item_ammo_effect_duration(ammo.id, 3);
    set_item_ammo_effect_damage(ammo.id, 3);

    // Consume the entire vial — same convention as envenom.
    delete_item(vial.id);

    let count_msg = "";
    if ammo.ammo_count > 1 {
        count_msg = " (" + ammo.ammo_count + " imbued)";
    }
    send_client_message(connection_id, "\x1b[0;32mYou carefully coat " + ammo.name + " with " + vial.name + "." + count_msg + "\x1b[0m");
    broadcast_to_room_awake(char.current_room_id, char.name + " carefully coats their ammunition with something.", char.name);

    let leveled = add_skill_experience(char.name, "thievery", 8);
    if leveled {
        send_client_message(connection_id, "\x1b[1;33mYour thievery skill has improved!\x1b[0m");
    }
}
