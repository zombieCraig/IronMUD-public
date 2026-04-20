// scripts/commands/camouflage.rhai
// camouflage - Blend into the surrounding wilderness terrain

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

    let tracking_skill = get_skill_level(char.name, "tracking");
    if tracking_skill < 3 {
        send_client_message(connection_id, "You need tracking level 3 to use camouflage.");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't camouflage yourself while in combat!");
        return;
    }

    if is_camouflaged(char.name) {
        set_camouflaged(char.name, false);
        send_client_message(connection_id, "You step out of your camouflage.");
        return;
    }

    if char.stamina < 8 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 8 stamina)");
        return;
    }

    // Must be in wilderness (dirt_floor, not city/indoors)
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }

    if !room.flags.dirt_floor || room.flags.city || room.flags.indoors {
        send_client_message(connection_id, "There is nowhere to blend in here. Camouflage only works in wilderness areas.");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 8;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Stealth check with terrain bonus (+15 from wilderness, built into calculate_stealth_score)
    let stealth_score = calculate_stealth_score(char.name, char.current_room_id);
    let difficulty = 40;
    // Dark rooms are easier
    let lighting = get_lighting_level(char.current_room_id);
    if lighting == "dark" { difficulty = difficulty - 15; }
    if lighting == "dim" { difficulty = difficulty - 5; }

    if stealth_score >= difficulty {
        set_camouflaged(char.name, true);
        send_client_message(connection_id, "You blend into the surrounding terrain.");
    } else {
        send_client_message(connection_id, "You try to blend in, but can't find adequate cover.");
    }

    let leveled = add_skill_experience(char.name, "tracking", 5);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
