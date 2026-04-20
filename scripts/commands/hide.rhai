// scripts/commands/hide.rhai
// hide - Attempt to hide in the current room

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    // Can't hide in combat
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't hide while in combat!");
        return;
    }

    // Already hidden?
    if is_hidden(char.name) {
        send_client_message(connection_id, "You are already hiding.");
        return;
    }

    // Check stamina
    if char.stamina < 5 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted to hide. (Need 5 stamina)");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 5;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Calculate stealth score vs room difficulty
    let room_id = char.current_room_id;
    let stealth_score = calculate_stealth_score(char.name, room_id);

    // Base difficulty 40, modified by room
    let room = get_room_data(room_id);
    let difficulty = 40;
    if room != () {
        // Harder to hide in well-lit city rooms
        if room.flags.city { difficulty += 10; }
        // Easier in dark rooms
        if room.flags.dark { difficulty -= 15; }
        // Easier in wilderness
        if room.flags.dirt_floor && !room.flags.indoors { difficulty -= 5; }
    }

    if stealth_score >= difficulty {
        set_hidden(char.name, true);
        send_client_message(connection_id, "You conceal yourself in the shadows.");

        // Award stealth XP
        let leveled = add_skill_experience(char.name, "stealth", 5);
        if leveled {
            send_client_message(connection_id, "Your stealth skill has improved!");
        }
    } else {
        send_client_message(connection_id, "You try to hide but can't find adequate concealment.");

        // Award small XP on failure to allow progression from level 0
        let leveled = add_skill_experience(char.name, "stealth", 1);
        if leveled {
            send_client_message(connection_id, "Your stealth skill has improved!");
        }
    }
}
