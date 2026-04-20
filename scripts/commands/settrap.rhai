// scripts/commands/settrap.rhai
// settrap <type> - Place a trap in the current room
// Types: spike, alarm, snare, poison_dart

fn get_trap_damage(trap_type) {
    if trap_type == "spike" { return "2d6"; }
    if trap_type == "poison_dart" { return "1d4"; }
    return "0";
}

fn get_trap_effect(trap_type) {
    if trap_type == "spike" { return "damage"; }
    if trap_type == "alarm" { return "alarm"; }
    if trap_type == "snare" { return "slow"; }
    if trap_type == "poison_dart" { return "poison"; }
    return "damage";
}

fn get_trap_detect_difficulty(trap_type, thievery_skill) {
    let base = 40;
    if trap_type == "alarm" { base = 30; }
    if trap_type == "spike" { base = 45; }
    if trap_type == "snare" { base = 35; }
    if trap_type == "poison_dart" { base = 50; }
    return base + (thievery_skill * 3);
}

fn get_trap_disarm_difficulty(trap_type, thievery_skill) {
    let base = 35;
    if trap_type == "alarm" { base = 25; }
    if trap_type == "spike" { base = 40; }
    if trap_type == "snare" { base = 30; }
    if trap_type == "poison_dart" { base = 45; }
    return base + (thievery_skill * 3);
}

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
    if thievery_skill < 3 {
        send_client_message(connection_id, "You need thievery level 3 to set traps.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: settrap <spike|alarm|snare|poison_dart>");
        return;
    }

    let trap_type = args.to_lower();
    if trap_type != "spike" && trap_type != "alarm" && trap_type != "snare" && trap_type != "poison_dart" {
        send_client_message(connection_id, "Invalid trap type. Types: spike, alarm, snare, poison_dart");
        return;
    }

    if char.stamina < 15 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 15 stamina)");
        return;
    }

    // Check for trap kit in inventory
    let inv_items = get_items_in_inventory(char.name);
    let kit_id = "";
    for item in inv_items {
        for keyword in item.keywords {
            if keyword == "trap_kit" || keyword == "trapkit" {
                kit_id = item.id;
            }
        }
    }

    if kit_id == "" {
        send_client_message(connection_id, "You need a trap kit to set traps.");
        return;
    }

    let room_id = char.current_room_id;

    // Limit traps per room
    let existing_traps = get_room_traps(room_id);
    if existing_traps.len() >= 3 {
        send_client_message(connection_id, "This room already has too many traps.");
        return;
    }

    // Consume stamina and trap kit
    if !char.god_mode {
        char.stamina = char.stamina - 15;
        save_character_data(char);
        set_player_character(connection_id, char);
    }
    delete_item(kit_id);

    // Calculate trap properties
    let damage = 0;
    if trap_type == "spike" {
        damage = random_int(2, 12); // 2d6 approximation as flat value
    }
    if trap_type == "poison_dart" {
        damage = random_int(1, 4); // 1d4
    }

    let detect_diff = get_trap_detect_difficulty(trap_type, thievery_skill);
    let disarm_diff = get_trap_disarm_difficulty(trap_type, thievery_skill);
    let effect = get_trap_effect(trap_type);

    add_room_trap(room_id, trap_type, char.name, damage, detect_diff, disarm_diff, 1, effect);

    send_client_message(connection_id, "You carefully set a " + trap_type + " trap.");
    broadcast_to_room_awake(room_id, char.name + " is doing something on the ground.", char.name);

    let leveled = add_skill_experience(char.name, "thievery", 10);
    if leveled {
        send_client_message(connection_id, "Your thievery skill has improved!");
    }
}
