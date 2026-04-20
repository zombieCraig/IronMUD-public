// scripts/commands/flee.rhai
// Flee command - attempt to escape from combat

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to flee.");
        return;
    }

    // Check if in combat
    if !is_in_combat(char.name) {
        send_client_message(connection_id, "You're not in combat!");
        return;
    }

    // Check if stunned
    if get_stun_rounds(char.name) > 0 {
        send_client_message(connection_id, "You are stunned and cannot flee!");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't flee while sleeping!");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first!");
        return;
    }

    // Check stamina (configurable via settings)
    let stamina_cost_str = get_setting_or_default("stamina_cost_flee", "10");
    let stamina_cost = parse_int(stamina_cost_str);
    if stamina_cost == () { stamina_cost = 10; }
    if !char.god_mode && char.stamina < stamina_cost {
        send_client_message(connection_id, "You are too exhausted to flee!");
        return;
    }

    // Consume stamina (even on failed flee)
    if !char.god_mode {
        char.stamina = char.stamina - stamina_cost;
        if char.stamina < 0 {
            char.stamina = 0;
        }
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Get valid flee directions
    let directions = get_valid_flee_directions(char.current_room_id);
    if directions.len() == 0 {
        send_client_message(connection_id, "There's nowhere to flee!");
        return;
    }

    // Get primary target for flee roll
    let target = get_primary_target(char.name);
    if target == () {
        // No target, just exit combat
        exit_combat(char.name);
        send_client_message(connection_id, "Combat has ended.");
        return;
    }

    // Calculate flee chance
    let flee_chance = calculate_flee_chance(char.name, target.target_id);

    // Roll d100
    let roll = roll_d100();

    if roll <= flee_chance {
        // Success! Pick random direction
        let dir_index = random_flee_index(directions.len());
        let direction = directions[dir_index];

        // Get target room
        let target_room_id = get_exit_target(char.current_room_id, direction);
        if target_room_id == "" {
            send_client_message(connection_id, "Something went wrong with your escape!");
            return;
        }

        let old_room_id = char.current_room_id;

        // Remove this target from our combat list
        remove_combat_target(char.name, target.target_id);

        // Check if we have more targets
        let remaining = get_combat_targets(char.name);
        if remaining.len() == 0 {
            exit_combat(char.name);
        }

        // Remove us from the mobile's target list
        if target.target_type == "mobile" {
            remove_mobile_combat_target(target.target_id, char.name);
        }

        // Announce departure (sleeping players don't notice)
        broadcast_to_room_awake(old_room_id, char.name + " flees " + direction + "!", char.name);

        // Move the character
        char.current_room_id = target_room_id;
        save_character_data(char);
        set_player_character(connection_id, char);

        // Announce arrival (sleeping players don't notice)
        let arrival_dir = get_opposite_direction(direction);
        broadcast_to_room_awake(target_room_id, char.name + " arrives from the " + arrival_dir + ", fleeing!", char.name);

        // Tell the player
        send_client_message(connection_id, "You flee " + direction + "!");

        // Display the new room
        display_room(target_room_id, connection_id, char.name);

        // Fire on_greet triggers for NPCs in new room
        let mobiles = get_mobiles_in_room(target_room_id);
        for mobile in mobiles {
            fire_mobile_trigger(mobile.id, "on_greet", connection_id, #{
                "char_name": char.name,
                "direction": direction,
                "source_room": old_room_id,
                "mobile_name": mobile.name,
                "fled": true
            });
        }
    } else {
        // Failed to flee - sleeping bystanders don't see combat
        let target_name = get_target_display_name(target);
        send_client_message(connection_id, "You try to flee but " + target_name + " blocks your escape!");
        broadcast_to_room_awake(char.current_room_id, char.name + " tries to flee but fails!", char.name);
    }
}

// Get display name for target
fn get_target_display_name(target) {
    if target.target_type == "mobile" {
        let mobile = get_mobile_data(target.target_id);
        if mobile != () {
            return mobile.name;
        }
        return "your opponent";
    }
    // Player target
    return target.target_id;
}

// Get opposite direction for arrival message
fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "below"; }
    if dir == "down" { return "above"; }
    return "somewhere";
}

// Random index for flee direction
fn random_flee_index(max) {
    if max <= 1 { return 0; }
    return random_int(0, max - 1);
}
