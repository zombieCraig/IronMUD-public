// scripts/commands/pick.rhai
// pick <direction> - Attempt to pick a lock on a door

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

    let thievery_skill = get_skill_level(char.name, "thievery");
    if thievery_skill < 2 {
        send_client_message(connection_id, "You need thievery level 2 to pick locks.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: pick <direction>");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    // Check for lockpick in inventory
    let has_lockpick = false;
    let lockpick_id = "";
    let inv_items = get_items_in_inventory(char.name);
    for item in inv_items {
        if item.flags.lockpick {
            has_lockpick = true;
            lockpick_id = item.id;
        }
    }

    if !has_lockpick {
        send_client_message(connection_id, "You need a lockpick to pick locks.");
        return;
    }

    let room_id = char.current_room_id;
    let direction = args.to_lower();

    // Check if there's a locked door in that direction
    let door = get_door(room_id, direction);
    if door == () {
        send_client_message(connection_id, "There is no door in that direction.");
        return;
    }

    if !door.is_locked {
        if !door.is_closed {
            send_client_message(connection_id, "The " + door.name + " is already open.");
        } else {
            send_client_message(connection_id, "The " + door.name + " is not locked.");
        }
        return;
    }

    if door.pickproof && !char.god_mode {
        send_client_message(connection_id, "The lock on the " + door.name + " resists all your attempts. It cannot be picked.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Success check: 30 + (thievery * 7) + (dex_mod * 2) vs difficulty
    let dex_mod = (char.stat_dex - 10) / 2;
    let pick_skill = 30 + (thievery_skill * 7) + (dex_mod * 2);
    let roll = random_int(1, 100);

    if roll <= pick_skill {
        // Success - unlock the door
        set_door_locked(room_id, direction, false);
        send_client_message(connection_id, "Click! You successfully pick the lock on the " + door.name + ".");
        broadcast_to_room_awake(room_id, char.name + " fiddles with a lock.", char.name);

        let leveled = add_skill_experience(char.name, "thievery", 8);
        if leveled {
            send_client_message(connection_id, "Your thievery skill has improved!");
        }
    } else if roll < 10 {
        // Critical fail - lockpick breaks
        delete_item(lockpick_id);
        send_client_message(connection_id, "Your lockpick snaps in the lock! The lockpick is destroyed.");
    } else {
        // Normal fail
        send_client_message(connection_id, "You fail to pick the lock on the " + door.name + ".");
    }
}
