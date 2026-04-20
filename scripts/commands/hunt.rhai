// scripts/commands/hunt.rhai
// hunt <name> - Begin tracking and auto-following a target
// hunt stop - Stop hunting

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
    if tracking_skill < 4 {
        send_client_message(connection_id, "You need tracking level 4 to hunt.");
        return;
    }

    let target_name = args;

    // Stop hunting
    if target_name.to_lower() == "stop" || target_name == "" {
        let current = get_hunting_target(char.name);
        if current == "" {
            send_client_message(connection_id, "You are not hunting anyone.");
        } else {
            set_hunting_target(char.name, "");
            send_client_message(connection_id, "You stop hunting " + current + ".");
        }
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't start a hunt while in combat!");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted to begin hunting. (Need 10 stamina)");
        return;
    }

    // Consume stamina to start the hunt
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    set_hunting_target(char.name, target_name);
    send_client_message(connection_id, "You begin hunting for " + target_name + ".");

    // Immediately check for trail in current room
    let room_id = char.current_room_id;
    let direction = find_target_direction(room_id, target_name);
    if direction != "" {
        send_client_message(connection_id, "You sense " + target_name + "'s trail leading " + direction + "...");
    } else {
        // Check if target is already here
        let players = get_characters_in_room(room_id);
        let found_here = false;
        for pname in players {
            if pname.to_lower() == target_name.to_lower() || pname.to_lower().contains(target_name.to_lower()) {
                if pname != char.name {
                    send_client_message(connection_id, target_name + " is right here!");
                    set_hunting_target(char.name, "");
                    found_here = true;
                }
            }
        }
        let mobiles = get_mobiles_in_room(room_id);
        for mob in mobiles {
            if mob.name.to_lower().contains(target_name.to_lower()) {
                send_client_message(connection_id, mob.name + " is right here!");
                set_hunting_target(char.name, "");
                found_here = true;
            }
        }
        if !found_here {
            send_client_message(connection_id, "You cannot detect a trail yet. The hunt tick will keep searching.");
        }
    }

    let leveled = add_skill_experience(char.name, "tracking", 5);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
