// scripts/commands/disarm.rhai
// disarm trap - Attempt to disarm a trap in the room

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
        send_client_message(connection_id, "You need thievery level 3 to disarm traps.");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    let room_id = char.current_room_id;
    let traps = get_room_traps(room_id);

    if traps.len() == 0 {
        send_client_message(connection_id, "There are no visible traps here.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Try to disarm the first trap found
    let trap = traps[0];
    let trap_type = trap["trap_type"];
    let disarm_difficulty = trap["disarm_difficulty"];

    // Thievery check vs disarm difficulty
    let dex_mod = (char.stat_dex - 10) / 2;
    let disarm_skill = 30 + (thievery_skill * 7) + (dex_mod * 2);
    let roll = random_int(1, 100);

    if roll <= disarm_skill {
        // Success - remove the trap
        remove_room_trap(room_id, 0);
        send_client_message(connection_id, "You carefully disarm the " + trap_type + " trap.");
        broadcast_to_room_awake(room_id, char.name + " disarms a trap.", char.name);

        let leveled = add_skill_experience(char.name, "thievery", 10);
        if leveled {
            send_client_message(connection_id, "Your thievery skill has improved!");
        }
    } else if roll > 90 {
        // Critical fail - trap triggers on you
        send_client_message(connection_id, "You fumble and the " + trap_type + " trap triggers on you!");
        trigger_trap_on_character(char, connection_id, room_id, trap, 0);
    } else {
        // Normal fail
        send_client_message(connection_id, "You fail to disarm the " + trap_type + " trap.");
    }
}

fn trigger_trap_on_character(char, connection_id, room_id, trap, trap_index) {
    let effect = trap["effect"];
    let damage = trap["damage"];
    let trap_type = trap["trap_type"];

    if effect == "damage" {
        if damage > 0 {
            char.hp = char.hp - damage;
            save_character_data(char);
            set_player_character(connection_id, char);
            send_client_message(connection_id, "The " + trap_type + " trap deals " + damage + " damage to you!");
        }
    }
    if effect == "alarm" {
        broadcast_to_room_awake(room_id, "ALARM! A trap sounds a loud alarm!");
    }
    if effect == "slow" {
        apply_buff(char.name, "slow", 1, 30, "snare trap");
        send_client_message(connection_id, "The snare trap slows your movement!");
    }
    if effect == "poison" {
        if damage > 0 {
            char.hp = char.hp - damage;
            save_character_data(char);
            set_player_character(connection_id, char);
            send_client_message(connection_id, "A poison dart hits you for " + damage + " damage!");
        }
        apply_ongoing_effect(char.name, "poison", 3, 2, "body");
        send_client_message(connection_id, "You feel poison coursing through your veins!");
    }

    // Decrement trap charge
    decrement_trap_charge(room_id, trap_index);
}
