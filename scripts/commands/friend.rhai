// scripts/commands/friend.rhai
// Manage your friend list and see their status
// Usage: friend [add|remove|online] [name]

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    let parts = args.split(" ");
    let subcommand = if parts.len() > 0 { parts[0].to_lower() } else { "" };

    if subcommand == "" {
        show_friends(char, connection_id);
        return;
    }

    if subcommand == "online" {
        show_friends_online(char, connection_id);
        return;
    }

    if subcommand == "add" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: friend add <name>");
            return;
        }
        add_friend(char, parts[1], connection_id);
        return;
    }

    if subcommand == "remove" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: friend remove <name>");
            return;
        }
        remove_friend(char, parts[1], connection_id);
        return;
    }

    // Default: treat as 'friend add <name>' if single arg
    add_friend(char, parts[0], connection_id);
}

fn show_friends(char, connection_id) {
    let friends = char.friends;
    if friends.len() == 0 {
        send_client_message(connection_id, "Your friend list is empty.");
        return;
    }

    let output = "\n=== Your Friends ===\n\n";
    for name in friends {
        let status = get_player_status_string(name);
        output += "  " + name + " - " + status + "\n";
    }
    send_client_message(connection_id, output);
}

fn show_friends_online(char, connection_id) {
    let friends = char.friends;
    let online_count = 0;
    let output = "\n=== Friends Online ===\n\n";
    
    for name in friends {
        if is_player_online(name) {
            let status = get_player_status_string(name);
            output += "  " + name + " - " + status + "\n";
            online_count += 1;
        }
    }

    if online_count == 0 {
        send_client_message(connection_id, "None of your friends are currently online.");
    } else {
        send_client_message(connection_id, output);
    }
}

fn add_friend(char, name, connection_id) {
    let target_name = name.trim();
    if target_name == "" { return; }
    
    // Case-insensitive check
    let lower_target = target_name.to_lower();
    if char.name.to_lower() == lower_target {
        send_client_message(connection_id, "You cannot friend yourself.");
        return;
    }

    for f in char.friends {
        if f.to_lower() == lower_target {
            send_client_message(connection_id, target_name + " is already on your friend list.");
            return;
        }
    }

    // Verify character exists
    let target_data = get_character_data(target_name);
    if target_data == () {
        send_client_message(connection_id, "No character named '" + target_name + "' found.");
        return;
    }

    // Persistent names are stored in their authored casing
    let actual_name = target_data.name;
    let friends = char.friends;
    friends.push(actual_name);
    char.friends = friends;
    save_character_data(char);

    send_client_message(connection_id, actual_name + " added to your friends.");
    
    // Notify if mutual
    if is_friend_of(actual_name, char.name) {
        send_client_message(connection_id, "You and " + actual_name + " are now mutual friends!");
        let target_conn = find_player_connection_by_name(actual_name);
        if target_conn != "" {
            send_client_message(target_conn, "You and " + char.name + " are now mutual friends!");
        }
    }
}

fn remove_friend(char, name, connection_id) {
    let lower_target = name.to_lower();
    let friends = char.friends;
    let new_friends = [];
    let found = false;

    for f in friends {
        if f.to_lower() == lower_target {
            found = true;
        } else {
            new_friends.push(f);
        }
    }

    if found {
        char.friends = new_friends;
        save_character_data(char);
        send_client_message(connection_id, name + " removed from your friends.");
    } else {
        send_client_message(connection_id, name + " is not on your friend list.");
    }
}

fn get_player_status_string(name) {
    if !is_player_online(name) {
        if is_player_disconnected(name) { return "\x1b[1;33m[Disconnected]\x1b[0m"; }
        return "\x1b[1;30mOffline\x1b[0m";
    }
    if is_player_idle(name) { return "\x1b[33mIdle\x1b[0m"; }
    if is_player_afk(name) { return "\x1b[35mAFK\x1b[0m"; }
    return "\x1b[1;32mOnline\x1b[0m";
}

// Helper to check if A has B as a friend
fn is_friend_of(player_name, target_name) {
    let data = get_character_data(player_name);
    if data == () { return false; }
    let lower_target = target_name.to_lower();
    for f in data.friends {
        if f.to_lower() == lower_target { return true; }
    }
    return false;
}
