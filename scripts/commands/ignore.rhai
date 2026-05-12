// scripts/commands/ignore.rhai
// Block messages and notifications from specific players
// Usage: ignore [list|remove] [name]

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    let parts = args.split(" ");
    let subcommand = if parts.len() > 0 { parts[0].to_lower() } else { "" };

    if subcommand == "" || subcommand == "list" {
        show_ignored(char, connection_id);
        return;
    }

    if subcommand == "remove" || subcommand == "unignore" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: ignore remove <name>");
            return;
        }
        remove_ignored(char, parts[1], connection_id);
        return;
    }

    // Default: treat as 'ignore <name>'
    add_ignored(char, parts[0], connection_id);
}

fn show_ignored(char, connection_id) {
    let ignored = char.ignored;
    if ignored.len() == 0 {
        send_client_message(connection_id, "Your ignore list is empty.");
        return;
    }

    let output = "\n=== Ignored Players ===\n\n";
    for name in ignored {
        output += "  " + name + "\n";
    }
    send_client_message(connection_id, output);
}

fn add_ignored(char, name, connection_id) {
    let target_name = name.trim();
    if target_name == "" { return; }

    let lower_target = target_name.to_lower();
    if char.name.to_lower() == lower_target {
        send_client_message(connection_id, "You cannot ignore yourself.");
        return;
    }

    for i in char.ignored {
        if i.to_lower() == lower_target {
            send_client_message(connection_id, target_name + " is already being ignored.");
            return;
        }
    }

    // Verify character exists
    let target_data = get_character_data(target_name);
    if target_data == () {
        send_client_message(connection_id, "No character named '" + target_name + "' found.");
        return;
    }

    let actual_name = target_data.name;
    let ignored = char.ignored;
    ignored.push(actual_name);
    char.ignored = ignored;
    save_character_data(char);

    send_client_message(connection_id, "You are now ignoring " + actual_name + ".");
}

fn remove_ignored(char, name, connection_id) {
    let lower_target = name.to_lower();
    let ignored = char.ignored;
    let new_ignored = [];
    let found = false;

    for i in ignored {
        if i.to_lower() == lower_target {
            found = true;
        } else {
            new_ignored.push(i);
        }
    }

    if found {
        char.ignored = new_ignored;
        save_character_data(char);
        send_client_message(connection_id, name + " removed from ignore list.");
    } else {
        send_client_message(connection_id, name + " is not on your ignore list.");
    }
}
