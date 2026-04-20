// scripts/commands/split.rhai
// Split gold among grouped members in the same room

fn give_split_share(recipient, splitter_name, share, total_split) {
    add_character_gold(recipient, share);

    // Notify others (not the splitter)
    if recipient.to_lower() == splitter_name.to_lower() { return; }

    let recipient_conn = find_player_connection_by_name(recipient);
    if recipient_conn == "" { return; }

    send_client_message(recipient_conn, splitter_name + " splits " + total_split + " gold; your share is " + share + " coins.");
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: split <amount>");
        return;
    }

    let amount = try_parse_int(args);
    if amount == () || amount <= 0 {
        send_client_message(connection_id, "You must specify a positive amount to split.");
        return;
    }

    // Find our group leader
    let leader_name = get_group_leader(char.name);
    if leader_name == "" {
        leader_name = char.name; // We are the leader
    }

    // Get group members in same room
    let members_here = get_group_members_in_room(leader_name, char.current_room_id);

    // Check if leader is in room
    let leader_conn = find_player_connection_by_name(leader_name);
    let leader_in_room = false;
    if leader_conn != "" {
        let leader_char = get_player_character(leader_conn);
        if leader_char != () && leader_char.current_room_id == char.current_room_id {
            leader_in_room = true;
        }
    }

    // Build recipient list: leader + all grouped members in room
    let recipients = [];
    if leader_in_room {
        recipients.push(leader_name);
    }
    for m in members_here {
        // Avoid duplicates if we're the leader
        if m.to_lower() != leader_name.to_lower() {
            recipients.push(m);
        }
    }

    // Must have leader in room or be the leader
    if !leader_in_room && char.name.to_lower() != leader_name.to_lower() {
        // We're a member but leader isn't here - still include ourselves
        let found_self = false;
        for r in recipients {
            if r.to_lower() == char.name.to_lower() {
                found_self = true;
            }
        }
        if !found_self {
            recipients.push(char.name);
        }
    }

    if recipients.len() < 2 {
        send_client_message(connection_id, "There's no one in your group here to split gold with.");
        return;
    }

    // Check if we have enough gold
    let current_gold = char.gold;
    if current_gold < amount {
        if current_gold <= 0 {
            send_client_message(connection_id, "You don't have any gold.");
        } else {
            send_client_message(connection_id, "You only have " + current_gold + " gold.");
        }
        return;
    }

    // Calculate share (integer division)
    let share = amount / recipients.len();
    if share <= 0 {
        send_client_message(connection_id, "Not enough to share among " + recipients.len() + " people.");
        return;
    }

    let total_split = share * recipients.len();

    // Deduct from splitter
    add_character_gold(char.name, -total_split);

    // Give to each recipient
    for recipient in recipients {
        give_split_share(recipient, char.name, share, total_split);
    }

    send_client_message(connection_id, "You split " + total_split + " gold among " + recipients.len() + " people; everyone receives " + share + " coins.");
}
