// scripts/commands/unlock.rhai
// unlock <direction|door|container> - Unlock a door or container with a key

fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "down"; }
    if dir == "down" { return "up"; }
    return "";
}

fn normalize_direction(arg) {
    let d = arg.to_lower();
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

fn is_direction(arg) {
    let lower = arg.to_lower();
    if lower == "north" || lower == "n" { return true; }
    if lower == "south" || lower == "s" { return true; }
    if lower == "east" || lower == "e" { return true; }
    if lower == "west" || lower == "w" { return true; }
    if lower == "up" || lower == "u" { return true; }
    if lower == "down" || lower == "d" { return true; }
    return false;
}

fn find_door_by_keyword(room_id, keyword) {
    let directions = ["north", "south", "east", "west", "up", "down"];
    for direction in directions {
        let door = get_door(room_id, direction);
        if door == () { continue; }
        if door.name.to_lower() == keyword { return direction; }
        for kw in door.keywords {
            if kw.to_lower() == keyword { return direction; }
        }
    }
    return "";
}

fn sync_door_unlock(room_id, direction) {
    let target_room_id = get_exit_target(room_id, direction);
    if target_room_id == "" { return; }
    let opposite = get_opposite_direction(direction);
    if opposite == "" { return; }
    if !has_door(target_room_id, opposite) { return; }
    set_door_locked(target_room_id, opposite, false);
}

fn check_key_and_unlock_door(char, room_id, direction, door, connection_id) {
    // God mode and build mode bypass key requirement
    if char.god_mode || is_in_build_mode(char.name, room_id) {
        set_door_locked(room_id, direction, false);
        send_client_message(connection_id, "You magically unlock the " + door.name + ".");
        broadcast_to_room_except(room_id, connection_id, char.name + " waves a hand and the " + door.name + " unlocks.");
        sync_door_unlock(room_id, direction);
        return true;
    }

    let key_vnum = door.key_vnum;
    if key_vnum != "" {
        let player_key = find_inventory_item_by_vnum(char.name, key_vnum);
        if player_key == () {
            send_client_message(connection_id, "You don't have the right key.");
            return false;
        }
        send_client_message(connection_id, "You unlock the " + door.name + " with " + player_key.name + ".");
    } else {
        send_client_message(connection_id, "You unlock the " + door.name + ".");
    }
    set_door_locked(room_id, direction, false);
    broadcast_to_room_except(room_id, connection_id, char.name + " unlocks the " + door.name + ".");
    sync_door_unlock(room_id, direction);
    return true;
}

fn unlock_door_in_direction(char, room_id, direction, connection_id) {
    let door = get_door(room_id, direction);
    if door == () {
        send_client_message(connection_id, "There's no door in that direction.");
        return;
    }
    if !door.is_locked {
        send_client_message(connection_id, "The " + door.name + " is not locked.");
        return;
    }
    check_key_and_unlock_door(char, room_id, direction, door, connection_id);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Unlock what?");
        return;
    }

    let keyword = args.to_lower();
    let room_id = char.current_room_id;

    // Check if it's a direction
    if is_direction(keyword) {
        let direction = normalize_direction(keyword);
        unlock_door_in_direction(char, room_id, direction, connection_id);
        return;
    }

    // Check if keyword matches a door name or keyword
    let found_direction = find_door_by_keyword(room_id, keyword);
    if found_direction != "" {
        unlock_door_in_direction(char, room_id, found_direction, connection_id);
        return;
    }

    // Fall back to container logic
    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        item = find_item_in_room(room_id, keyword);
    }

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !item.is_container {
        send_client_message(connection_id, "That's not a container.");
        return;
    }

    if !item.container_locked {
        send_client_message(connection_id, "It's not locked.");
        return;
    }

    // God mode and build mode bypass key requirement for containers
    if char.god_mode || is_in_build_mode(char.name, room_id) {
        set_container_locked(item.id, false);
        send_client_message(connection_id, "You magically unlock " + item.name + ".");
        broadcast_to_room_except(room_id, connection_id, char.name + " waves a hand and " + item.name + " unlocks.");
        return;
    }

    // Check if container has a key requirement
    let key_vnum = item.container_key_vnum;
    if key_vnum != () && key_vnum != "" {
        let player_key = find_inventory_item_by_vnum(char.name, key_vnum);
        if player_key == () {
            send_client_message(connection_id, "You don't have the right key.");
            return;
        }
        send_client_message(connection_id, "You unlock " + item.name + " with " + player_key.name + ".");
    } else {
        send_client_message(connection_id, "You unlock " + item.name + ".");
    }

    set_container_locked(item.id, false);
    broadcast_to_room_except(room_id, connection_id, char.name + " unlocks " + item.name + ".");
}
