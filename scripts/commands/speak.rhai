// scripts/commands/speak.rhai
// speak [language]
//
// No arg: print the current language.
// With arg: switch to the named language if you know it (level >= 1).

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let trimmed = args;
    if trimmed == "" {
        let display = get_language_display_name(char.current_language);
        send_client_message(connection_id, "You are currently speaking " + display + ".");
        send_client_message(connection_id, "Use 'speak <language>' to switch, or 'languages' to see what you know.");
        return;
    }

    let resolved = resolve_language_key(trimmed);
    if resolved == "" {
        send_client_message(connection_id, "There is no such language.");
        return;
    }

    if resolved == char.current_language {
        let display = get_language_display_name(resolved);
        send_client_message(connection_id, "You are already speaking " + display + ".");
        return;
    }

    let known_level = get_skill_level(char.name, resolved);
    let lingua_franca = is_lingua_franca(resolved);
    if known_level < 1 && !lingua_franca {
        let display = get_language_display_name(resolved);
        send_client_message(connection_id, "You don't know any " + display + ". Type 'languages' to list what you know.");
        return;
    }

    char.current_language = resolved;
    save_character_data(char);
    let display = get_language_display_name(resolved);
    send_client_message(connection_id, "You begin speaking " + display + ".");
}
