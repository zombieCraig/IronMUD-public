// scripts/commands/ungroup.rhai
// Leave or disband a group

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        // If we're a follower, leave the group
        if char.following != "" {
            let leader_name = char.following;

            if !char.is_grouped {
                send_client_message(connection_id, "You are not in a group (only following). Use 'follow self' to stop following.");
                return;
            }

            set_grouped(char.name, false);
            send_client_message(connection_id, "You leave the group.");

            let leader_conn = find_player_connection_by_name(leader_name);
            if leader_conn != "" {
                send_client_message(leader_conn, char.name + " has left the group.");
            }
            return;
        }

        // We're the leader - remove all from group
        let members = get_group_members(char.name);
        if members.len() == 0 {
            send_client_message(connection_id, "You have no group members to disband.");
            return;
        }

        for member in members {
            set_grouped(member, false);
            let member_conn = find_player_connection_by_name(member);
            if member_conn != "" {
                send_client_message(member_conn, char.name + " has disbanded the group.");
            }
        }

        send_client_message(connection_id, "You disband your group.");
        return;
    }

    // Ungroup specific player (must be leader)
    if char.following != "" {
        send_client_message(connection_id, "You must be the group leader to remove others.");
        return;
    }

    let target_conn = find_player_connection_by_name(args);
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

    if !target_char.is_grouped {
        send_client_message(connection_id, target_char.name + " is not grouped (only following).");
        return;
    }

    set_grouped(target_char.name, false);
    send_client_message(connection_id, target_char.name + " has been removed from the group.");
    send_client_message(target_conn, char.name + " has removed you from the group.");
}
