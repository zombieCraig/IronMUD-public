// scripts/commands/gtell.rhai
// Send a message to all group members (across rooms)

fn send_group_message(recipient, sender_name, message) {
    let recipient_conn = find_player_connection_by_name(recipient);
    if recipient_conn == "" { return; }

    if recipient.to_lower() == sender_name.to_lower() {
        send_client_message(recipient_conn, "You tell the group: " + message);
    } else {
        send_client_message(recipient_conn, sender_name + " tells the group: " + message);
    }
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: gtell <message>");
        return;
    }

    // Find our group leader
    let leader_name = get_group_leader(char.name);
    if leader_name == "" {
        leader_name = char.name; // We might be the leader
    }

    // Check if we're actually in a group
    let members = get_group_members(leader_name);
    let followers = get_followers(leader_name);

    // If we're the leader, check if we have any grouped members
    if char.following == "" {
        if members.len() == 0 {
            send_client_message(connection_id, "You don't have any group members.");
            return;
        }
    } else {
        // We're a follower - must be grouped
        if !char.is_grouped {
            send_client_message(connection_id, "You are not in a group (only following).");
            return;
        }
    }

    // Build recipient list: leader + all grouped members
    let recipients = [];
    recipients.push(leader_name);
    for m in members {
        // Avoid duplicate if leader is also in members list (shouldn't happen but be safe)
        if m.to_lower() != leader_name.to_lower() {
            recipients.push(m);
        }
    }

    // Send to all
    for recipient in recipients {
        send_group_message(recipient, char.name, args);
    }
}
