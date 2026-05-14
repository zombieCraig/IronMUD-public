// scripts/commands/roster.rhai
// Switch between characters owned by the same account, or step out to chargen
// for a new one. The flow: save the current character, drop the connection
// back into the post-auth roster mode that login.rhai already handles.

fn run_command(args, connection_id) {
    let account_id = get_authenticated_account_id(connection_id);
    if account_id == "" {
        send_client_message(connection_id, "Your session is missing account context. Try `quit` and log in again.");
        return;
    }

    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You aren't currently playing a character.");
        return;
    }

    // Save current character state and announce departure to the room — the
    // session is about to swap out of "playing" mode.
    save_character_data(char);
    broadcast_to_room(char.current_room_id, char.name + " fades away into the ether.", char.name);

    // Drop the active character; auth (account_id) stays on the session.
    clear_player_character(connection_id);

    // Re-enter the roster prompt that login.rhai already handles.
    let summaries = get_account_character_summaries(account_id);
    if summaries.len() == 0 {
        // Edge case: every character was deleted. Surface the name prompt.
        set_olc_mode(connection_id, "chargen_name_prompt");
        send_client_message(connection_id, "\nYour account has no characters. What should your new character be named?\n> ");
        return;
    }


    set_olc_mode(connection_id, "select_character");
    show_account_roster(connection_id, summaries);
}

fn show_account_roster(connection_id, summaries) {
    let output = "\n=== Your characters ===\n\n";
    let idx = 1;
    for s in summaries {
        let class_str = s.class_name;
        if class_str == "" { class_str = "unemployed"; }
        let race_str = s.race;
        if race_str == "" { race_str = "unknown"; }
        let where_str = s.room_title;
        if where_str == "" { where_str = "elsewhere"; }
        output += "  [" + idx + "] " + s.name + " — level " + s.level + " " + race_str + " " + class_str + ", in " + where_str + "\n";
        idx += 1;
    }
    output += "  [R] Retire a character (permanent)\n";
    output += "  [X] Delete entire account (permanent)\n";
    output += "\nType a number to play that character, `new` to create another, or `quit` to log out.\n> ";
    send_client_message(connection_id, output);
}
