// scripts/commands/close.rhai
// close <direction|door|container> - Close a door or container

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

fn sync_door_close(room_id, direction, door_name) {
    let target_room_id = get_exit_target(room_id, direction);
    if target_room_id == "" { return; }
    let opposite = get_opposite_direction(direction);
    if opposite == "" { return; }
    if !has_door(target_room_id, opposite) { return; }
    set_door_closed(target_room_id, opposite, true);
    broadcast_to_room_except(target_room_id, "", "The " + door_name + " closes from the other side.");
}

fn close_door_in_direction(char, room_id, direction, connection_id) {
    let door = get_door(room_id, direction);
    if door == () {
        send_client_message(connection_id, "There's no door in that direction.");
        return;
    }
    if door.is_closed {
        send_client_message(connection_id, "The " + door.name + " is already closed.");
        return;
    }
    set_door_closed(room_id, direction, true);
    send_client_message(connection_id, "You close the " + door.name + ".");
    broadcast_to_room_except(room_id, connection_id, char.name + " closes the " + door.name + ".");
    sync_door_close(room_id, direction, door.name);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Close what?");
        return;
    }

    let keyword = args.to_lower();
    let room_id = char.current_room_id;

    // Check if it's a direction
    if is_direction(keyword) {
        let direction = normalize_direction(keyword);
        close_door_in_direction(char, room_id, direction, connection_id);
        return;
    }

    // Check if keyword matches a door name or keyword
    let found_direction = find_door_by_keyword(room_id, keyword);
    if found_direction != "" {
        close_door_in_direction(char, room_id, found_direction, connection_id);
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

    if item.container_closed {
        send_client_message(connection_id, "It's already closed.");
        return;
    }

    set_container_closed(item.id, true);
    send_client_message(connection_id, "You close " + item.name + ".");
    broadcast_to_room_except(room_id, connection_id, char.name + " closes " + item.name + ".");
}
