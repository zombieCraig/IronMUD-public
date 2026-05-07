// scripts/commands/languages.rhai
// languages
//
// Prints the player's known languages with skill level. The current
// language is marked with `*`. Lingua francas are always shown.

fn pad_right(s, width) {
    let out = s;
    while out.len() < width {
        out += " ";
    }
    return out;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let entries = get_languages_known(char.name);
    if entries.len() == 0 {
        send_client_message(connection_id, "You don't know any languages. (Something has gone wrong; report this.)");
        return;
    }

    let output = "\nKnown languages:\n";
    for entry in entries {
        let marker = if entry.is_current { "*" } else { " " };
        let level = entry.level;
        let label = if entry.is_lingua_franca && level >= 10 {
            "(everyone speaks this)"
        } else if level >= 10 {
            "(fluent)"
        } else {
            "level " + level + "/10"
        };
        let line = " " + marker + " " + pad_right(entry.display_name, 16) + label + "\n";
        output += line;
    }
    output += "\nSpoken via 'say'/'tell'/'whisper'/'shout'. Switch with 'speak <language>'.\n";
    send_client_message(connection_id, output);
}
