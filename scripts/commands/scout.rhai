// scripts/commands/scout.rhai
// scout [direction] - Scan adjacent rooms for occupants

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

    let stealth_skill = get_skill_level(char.name, "stealth");
    if stealth_skill < 2 {
        send_client_message(connection_id, "You need stealth level 2 to scout. You need the stealth skill.");
        return;
    }

    if char.stamina < 5 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 5 stamina)");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 5;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    let room = get_room_data(room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }

    let direction = args.to_lower();

    if direction == "" {
        // Scan all exits briefly
        send_client_message(connection_id, "You carefully scout the surrounding area...");
        let found_any = false;
        found_any = scout_direction(room, "north", char, connection_id, false) || found_any;
        found_any = scout_direction(room, "south", char, connection_id, false) || found_any;
        found_any = scout_direction(room, "east", char, connection_id, false) || found_any;
        found_any = scout_direction(room, "west", char, connection_id, false) || found_any;
        found_any = scout_direction(room, "up", char, connection_id, false) || found_any;
        found_any = scout_direction(room, "down", char, connection_id, false) || found_any;
        if !found_any {
            send_client_message(connection_id, "The surrounding areas appear empty.");
        }
    } else {
        // Detailed scan of one direction
        send_client_message(connection_id, "You carefully scout to the " + direction + "...");
        let found = scout_direction(room, direction, char, connection_id, true);
        if !found {
            send_client_message(connection_id, "Nothing of note to the " + direction + ".");
        }
    }

    let leveled = add_skill_experience(char.name, "stealth", 5);
    if leveled {
        send_client_message(connection_id, "Your stealth skill has improved!");
    }
}

fn get_exit_room_id(room, direction) {
    if direction == "north" { return room.exits.north; }
    if direction == "south" { return room.exits.south; }
    if direction == "east" { return room.exits.east; }
    if direction == "west" { return room.exits.west; }
    if direction == "up" { return room.exits.up; }
    if direction == "down" { return room.exits.down; }
    return "";
}

fn scout_direction(room, direction, char, connection_id, detailed) {
    let target_room_id = get_exit_room_id(room, direction);
    if target_room_id == "" { return false; }

    // Check for closed doors blocking line of sight
    let door = get_door(room.id, direction);
    if door != () && door.is_closed {
        if detailed {
            send_client_message(connection_id, "A " + door.name + " blocks your view to the " + direction + ".");
        }
        return false;
    }

    let target_room = get_room_data(target_room_id);
    if target_room == () { return false; }

    let found = false;

    // List mobiles
    let mobiles = get_mobiles_in_room(target_room_id);
    for mobile in mobiles {
        found = true;
        if detailed {
            send_client_message(connection_id, "  " + direction + ": " + mobile.name + " - " + mobile.short_desc);
        } else {
            send_client_message(connection_id, "  " + direction + ": " + mobile.name);
        }
    }

    // List visible players
    let players = get_characters_in_room(target_room_id);
    for pname in players {
        if pname == char.name { continue; }

        // Hidden players require stealth check
        if is_hidden(pname) || is_sneaking(pname) || is_camouflaged(pname) {
            if stealth_check_vs_player(pname, char.name, target_room_id) {
                // Target's stealth beats our perception - skip
                continue;
            }
            // We detected them
            if detailed {
                send_client_message(connection_id, "  " + direction + ": " + pname + " (lurking in shadows)");
            } else {
                send_client_message(connection_id, "  " + direction + ": " + pname + " (hidden)");
            }
            found = true;
        } else {
            found = true;
            if detailed {
                send_client_message(connection_id, "  " + direction + ": " + pname);
            } else {
                send_client_message(connection_id, "  " + direction + ": " + pname);
            }
        }
    }

    // If detailed, also show room title
    if detailed && found {
        send_client_message(connection_id, "  (Room: " + target_room.title + ")");
    }

    return found;
}
