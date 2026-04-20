// scripts/commands/group.rhai
// Manage party/group membership

fn try_add_follower_to_group(follower_name, leader_name) {
    let follower_conn = find_player_connection_by_name(follower_name);
    if follower_conn == "" { return false; }

    let follower = get_player_character(follower_conn);
    if follower == () { return false; }
    if follower.is_grouped { return false; }

    set_grouped(follower_name, true);
    send_client_message(follower_conn, leader_name + " has added you to the group.");
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // If following someone, we can only show status
    if char.following != "" {
        if args != "" {
            send_client_message(connection_id, "You must be the group leader to add members.");
            return;
        }
        show_group_status(char, connection_id);
        return;
    }

    if args == "" {
        // Show group status
        show_group_status(char, connection_id);
        return;
    }

    let target_name = args.to_lower();

    // "group all" - add all followers
    if target_name == "all" {
        let followers = get_followers(char.name);
        if followers.len() == 0 {
            send_client_message(connection_id, "You have no followers.");
            return;
        }

        let added = 0;
        for follower_name in followers {
            if try_add_follower_to_group(follower_name, char.name) {
                added += 1;
            }
        }

        if added > 0 {
            send_client_message(connection_id, "You add " + added + " follower(s) to your group.");
        } else {
            send_client_message(connection_id, "All your followers are already in the group.");
        }
        return;
    }

    // Group specific player
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + args + "' is not online.");
        return;
    }

    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Player '" + args + "' is not online.");
        return;
    }

    // Must be following us
    if target_char.following == "" || target_char.following.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, target_char.name + " is not following you.");
        return;
    }

    // Toggle group status
    if target_char.is_grouped {
        set_grouped(target_char.name, false);
        send_client_message(connection_id, target_char.name + " has been removed from your group.");
        send_client_message(target_conn, char.name + " has removed you from the group.");
    } else {
        set_grouped(target_char.name, true);
        send_client_message(connection_id, target_char.name + " is now in your group.");
        send_client_message(target_conn, char.name + " has added you to the group.");
    }
}

fn show_group_status(char, connection_id) {
    let leader_name = get_group_leader(char.name);

    if leader_name == "" {
        // We are the leader (or not in any group)
        let followers = get_followers(char.name);
        let members = get_group_members(char.name);

        if followers.len() == 0 {
            send_client_message(connection_id, "You are not in a group.");
            return;
        }

        let output = "Your group:\n";
        output += "  " + char.name + " (Leader)\n";

        for follower in followers {
            let is_member = false;
            for m in members {
                if m.to_lower() == follower.to_lower() {
                    is_member = true;
                }
            }

            if is_member {
                output += "  " + follower + " (Member)\n";
            } else {
                output += "  " + follower + " (Following)\n";
            }
        }

        send_client_message(connection_id, output);
        return;
    }

    // We are a follower
    let output = "Group led by " + leader_name + ":\n";
    output += "  " + leader_name + " (Leader)\n";

    let followers = get_followers(leader_name);
    let members = get_group_members(leader_name);

    for follower in followers {
        let is_member = false;
        for m in members {
            if m.to_lower() == follower.to_lower() {
                is_member = true;
            }
        }

        let status = if is_member { "(Member)" } else { "(Following)" };
        let you_tag = if follower.to_lower() == char.name.to_lower() { " <-- You" } else { "" };
        output += "  " + follower + " " + status + you_tag + "\n";
    }

    send_client_message(connection_id, output);
}
