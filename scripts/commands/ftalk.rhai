// scripts/commands/ftalk.rhai
// Private communication channel for mutual friends
// Usage: ftalk <message>

fn run_command(args, connection_id) {
    if args == "" {
        send_client_message(connection_id, "What do you want to say to your friends?");
        return;
    }

    let char = get_player_character(connection_id);
    let friends = char.friends;
    let count = 0;
    // Track why a friend didn't hear us, so we can give useful feedback.
    let pending_online = [];   // online, but they haven't added us back (not mutual)

    for friend_name in friends {
        // Check if mutual and not ignored
        if is_mutual_friend(char.name, friend_name) {
            let target_conn = find_player_connection_by_name(friend_name);
            if target_conn != "" {
                send_client_message(target_conn, "\x1b[1;36m[Friend] " + char.name + " talks: " + args + "\x1b[0m");
                count += 1;
            }
        } else if is_player_online(friend_name) && is_friend_of(char.name, friend_name) {
            // We have them, they're online, but it isn't mutual (they haven't added us,
            // or one side ignores the other). Surface this so the user isn't confused.
            pending_online.push(friend_name);
        }
    }

    if count > 0 {
        send_client_message(connection_id, "\x1b[1;36mYou tell your friends: " + args + "\x1b[0m");
        return;
    }

    // Nobody heard us — explain why as specifically as we can.
    if friends.len() == 0 {
        send_client_message(connection_id, "You haven't added any friends yet. Use 'friend add <name>'.");
        return;
    }

    if pending_online.len() > 0 {
        let names = pending_online[0];
        for i in 1..pending_online.len() { names += ", " + pending_online[i]; }
        send_client_message(connection_id, names + " " + (if pending_online.len() == 1 { "is" } else { "are" }) + " online, but ftalk only reaches mutual friends. They need to 'friend add " + char.name + "' back.");
        return;
    }

    send_client_message(connection_id, "None of your mutual friends are online to hear you.");
}

// Helper: does A have B on their friend list? (one-directional)
fn is_friend_of(a_name, b_name) {
    let a = get_character_data(a_name);
    if a == () { return false; }
    let lower_b = b_name.to_lower();
    for f in a.friends { if f.to_lower() == lower_b { return true; } }
    return false;
}

// Helper to check if two players are mutual friends and neither ignores the other
fn is_mutual_friend(p1_name, p2_name) {
    let p1 = get_character_data(p1_name);
    let p2 = get_character_data(p2_name);
    if p1 == () || p2 == () { return false; }

    // p1 has p2 as friend?
    let p1_has_p2 = false;
    for f in p1.friends { if f.to_lower() == p2_name.to_lower() { p1_has_p2 = true; break; } }
    if !p1_has_p2 { return false; }

    // p2 has p1 as friend?
    let p2_has_p1 = false;
    for f in p2.friends { if f.to_lower() == p1_name.to_lower() { p2_has_p1 = true; break; } }
    if !p2_has_p1 { return false; }

    // p1 ignores p2?
    for i in p1.ignored { if i.to_lower() == p2_name.to_lower() { return false; } }

    // p2 ignores p1?
    for i in p2.ignored { if i.to_lower() == p1_name.to_lower() { return false; } }

    return true;
}
