// scripts/commands/tell.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to send tells.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: tell <player> <message>");
        return;
    }

    // Split args into target name and message
    let space_pos = args.index_of(" ");
    if space_pos < 0 {
        send_client_message(connection_id, "Usage: tell <player> <message>");
        return;
    }

    let target_name = args.sub_string(0, space_pos);
    let message = args.sub_string(space_pos + 1);

    if message == "" {
        send_client_message(connection_id, "Tell them what?");
        return;
    }

    // Spam guard: a 2s cooldown still allows back-and-forth conversation
    // but stops scripted tell-flooding of any single target.
    if !char.god_mode {
        let wait = try_throttle_command(char.name, "tell", 2);
        if wait > 0 {
            send_client_message(connection_id, "You're sending tells too fast. (" + wait + "s)");
            return;
        }
    }

    // Can't tell yourself
    if target_name.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "Talking to yourself?");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's actual name (proper capitalization)
    let target_char = get_player_character(target_conn);
    let actual_name = target_char.name;

    // Apply jaw garbling to outgoing speech
    let spoken = message;
    let jaw_penalty = get_jaw_wound_penalty(char.name);
    if jaw_penalty > 0 {
        spoken = garble_jaw_speech(spoken, jaw_penalty);
    }

    // Language garble: worse-of-(speaker, listener) skill.
    let language_key = char.current_language;
    let heard = spoken;
    if !is_lingua_franca(language_key) {
        let speaker_skill = get_skill_level(char.name, language_key);
        let listener_skill = get_skill_level(actual_name, language_key);
        let effective = if speaker_skill < listener_skill { speaker_skill } else { listener_skill };
        heard = garble_language(heard, language_key, effective);
        if listener_skill >= 1 {
            add_skill_experience(actual_name, language_key, 1);
        }
        add_skill_experience(char.name, language_key, 1);
    }

    // Apply ear garbling for recipient
    let ear_penalty = get_ear_wound_penalty(actual_name);
    if ear_penalty > 0 {
        heard = garble_hearing(heard, ear_penalty);
    }

    // Send message to target
    send_client_message(target_conn, char.name + " tells you: " + heard);

    // Confirm to sender (show original)
    send_client_message(connection_id, "You tell " + actual_name + ": " + message);
}
