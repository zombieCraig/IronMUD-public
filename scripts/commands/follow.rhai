// scripts/commands/follow.rhai
// Follow another player or mobile - followers auto-move when leader moves

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep!");
        return;
    }

    if args == "" {
        // Show current follow status
        if char.following != "" {
            send_client_message(connection_id, "You are following " + char.following + ".");
        } else if char.following_mobile_id != "" {
            let mob = get_mobile_data(char.following_mobile_id);
            if mob != () {
                send_client_message(connection_id, "You are following " + mob.name + ".");
            } else {
                send_client_message(connection_id, "You are following something, but it has gone.");
            }
        } else {
            send_client_message(connection_id, "You are not following anyone.");
        }
        return;
    }

    let target_name = args;

    // "follow self" stops following
    if target_name.to_lower() == "self" || target_name.to_lower() == char.name.to_lower() {
        if char.following == "" && char.following_mobile_id == "" {
            send_client_message(connection_id, "You are not following anyone.");
            return;
        }

        let old_leader = char.following;
        let old_mobile_id = char.following_mobile_id;
        clear_following(char.name);

        if old_leader != "" {
            send_client_message(connection_id, "You stop following " + old_leader + ".");
            let leader_conn = find_player_connection_by_name(old_leader);
            if leader_conn != "" {
                send_client_message(leader_conn, char.name + " stops following you.");
            }
        } else if old_mobile_id != "" {
            let mob = get_mobile_data(old_mobile_id);
            let name = if mob != () { mob.name } else { "your quarry" };
            send_client_message(connection_id, "You stop following " + name + ".");
        }
        return;
    }

    // Try player first
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn != "" {
        let target_char = get_player_character(target_conn);
        if target_char != () {
            // Must be in same room to start following
            if target_char.current_room_id != char.current_room_id {
                send_client_message(connection_id, target_char.name + " is not here.");
                return;
            }

            // Already following them?
            if char.following != "" && char.following.to_lower() == target_char.name.to_lower() {
                send_client_message(connection_id, "You are already following " + target_char.name + ".");
                return;
            }

            // Notify old leader if we were following someone
            if char.following != "" {
                let old_leader_conn = find_player_connection_by_name(char.following);
                if old_leader_conn != "" {
                    send_client_message(old_leader_conn, char.name + " stops following you.");
                }
            }

            let success = set_following(char.name, target_char.name);
            if !success {
                send_client_message(connection_id, "You cannot follow " + target_char.name + " (would create a follow loop).");
                return;
            }

            send_client_message(connection_id, "You now follow " + target_char.name + ".");
            send_client_message(target_conn, char.name + " now follows you.");
            broadcast_to_room(char.current_room_id, char.name + " starts following " + target_char.name + ".", char.name);
            return;
        }
    }

    // Try a mobile in the current room
    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(target_name, mobiles);
    if mobile != () {
        if char.following_mobile_id == mobile.id {
            send_client_message(connection_id, "You are already following " + mobile.name + ".");
            return;
        }

        // Notify old player leader if switching from player-follow to mobile-follow
        if char.following != "" {
            let old_leader_conn = find_player_connection_by_name(char.following);
            if old_leader_conn != "" {
                send_client_message(old_leader_conn, char.name + " stops following you.");
            }
        }

        let ok = set_following_mobile(char.name, mobile.id);
        if !ok {
            send_client_message(connection_id, "You cannot follow " + mobile.name + ".");
            return;
        }

        send_client_message(connection_id, "You now follow " + mobile.name + ".");
        broadcast_to_room(char.current_room_id, char.name + " starts following " + mobile.name + ".", char.name);
        return;
    }

    send_client_message(connection_id, "There is no '" + target_name + "' here to follow.");
}
