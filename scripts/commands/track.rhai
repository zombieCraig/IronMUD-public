// scripts/commands/track.rhai
// track <name> - Search for tracks of a target in the current room

fn show_blood_trail(connection_id, trail_name, trail_dir, show_name) {
    if show_name {
        if trail_dir != "" {
            send_client_message(connection_id, "\x1b[1;31mYou find a trail of " + trail_name + "'s blood leading " + trail_dir + ".\x1b[0m");
        } else {
            send_client_message(connection_id, "\x1b[1;31mYou find " + trail_name + "'s blood staining the ground here.\x1b[0m");
        }
    } else {
        if trail_dir != "" {
            send_client_message(connection_id, "\x1b[1;31mYou notice blood on the ground leading " + trail_dir + ".\x1b[0m");
        } else {
            send_client_message(connection_id, "\x1b[1;31mYou notice blood staining the ground here.\x1b[0m");
        }
    }
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    let tracking_skill = get_skill_level(char.name, "tracking");

    if args == "" {
        send_client_message(connection_id, "Track whom? Usage: track <name>");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    let target_name = args;

    let room = get_room_data(room_id);
    if room != () && room.flags.notrack && !char.god_mode {
        send_client_message(connection_id, "The ground here yields no trails.");
        return;
    }

    // Get departure records for this room
    let departures = get_recent_departures(room_id, tracking_skill);
    let target_lower = target_name.to_lower();

    let found = false;
    for dep in departures {
        if dep["name"].to_lower() == target_lower || dep["name"].to_lower().contains(target_lower) {
            let dir = dep["direction"];
            if tracking_skill >= 2 {
                // Full detail: name, direction, freshness, sneaking
                let freshness = dep["freshness"];
                let sneaking_note = "";
                if dep["is_sneaking"] {
                    sneaking_note = " The tracks are faint, as if they were trying to move quietly.";
                }
                send_client_message(connection_id, "You find tracks leading " + dir + ". They look " + freshness + "." + sneaking_note);
            } else if tracking_skill == 1 {
                // Partial: direction + freshness, no name or sneaking info
                let freshness = dep["freshness"];
                send_client_message(connection_id, "Someone passed through here heading " + dir + ". The tracks look " + freshness + ".");
            } else {
                // Vague: direction only
                send_client_message(connection_id, "You notice some disturbances in the ground heading " + dir + ".");
            }
            found = true;
        }
    }

    // Check blood trails for the target (requires tracking level 1+)
    if tracking_skill >= 1 {
        let blood_trails = get_blood_trails(room_id);
        for trail in blood_trails {
            if trail["name"].to_lower() == target_lower || trail["name"].to_lower().contains(target_lower) {
                let show_name = tracking_skill >= 2;
                show_blood_trail(connection_id, trail["name"], trail["direction"], show_name);
                found = true;
            }
        }
    }

    if !found {
        // Also check if target is in the current room
        let players = get_characters_in_room(room_id);
        for pname in players {
            if pname.to_lower() == target_lower || pname.to_lower().contains(target_lower) {
                if pname != char.name {
                    send_client_message(connection_id, target_name + " is right here!");
                    found = true;
                }
            }
        }
        let mobiles = get_mobiles_in_room(room_id);
        for mob in mobiles {
            if mob.name.to_lower().contains(target_lower) {
                send_client_message(connection_id, mob.name + " is right here!");
                found = true;
            }
        }
    }

    if !found {
        send_client_message(connection_id, "You cannot find any trace of " + target_name + ".");
        // Award small XP for the attempt
        let leveled = add_skill_experience(char.name, "tracking", 2);
        if leveled {
            send_client_message(connection_id, "Your tracking skill has improved!");
        }
        return;
    }

    let leveled = add_skill_experience(char.name, "tracking", 5);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
