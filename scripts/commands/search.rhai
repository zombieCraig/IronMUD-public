// scripts/commands/search.rhai
// search - Detect hidden characters and traps in the room

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

    // Check stamina
    if char.stamina < 5 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted to search. (Need 5 stamina)");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 5;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    let found_anyone = false;

    // Search for hidden players
    let players = get_characters_in_room(room_id);
    for player_name in players {
        if player_name == char.name { continue; }

        let is_target_hidden = is_hidden(player_name);
        let is_target_sneaking = is_sneaking(player_name);
        let is_target_camo = is_camouflaged(player_name);

        if is_target_hidden || is_target_sneaking || is_target_camo {
            // Perception check vs their stealth
            if !stealth_check_vs_player(player_name, char.name, room_id) {
                // Searcher wins - target detected!
                break_stealth(player_name);
                broadcast_to_room(room_id, char.name + " discovers " + player_name + " lurking in the shadows!");
                found_anyone = true;
            }
        }
    }

    // Search for hidden mobiles (future: if we add stealth to mobiles)

    // Search for traps
    let traps = get_room_traps(room_id);
    let found_traps = 0;
    for trap in traps {
        let perception_score = calculate_perception_score_player(char.name);
        if perception_score >= trap["detect_difficulty"] {
            found_traps += 1;
            send_client_message(connection_id, "You notice a " + trap["trap_type"] + " trap!");
        }
    }

    if !found_anyone && found_traps == 0 {
        send_client_message(connection_id, "You search the area carefully but find nothing hidden.");
    }

    // Award tracking XP
    let leveled = add_skill_experience(char.name, "tracking", 3);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
