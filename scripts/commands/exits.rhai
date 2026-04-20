// scripts/commands/exits.rhai

fn format_exit_with_door(dest, dir_link, door) {
    if door.is_locked {
        return "  " + dir_link + " [locked " + door.name + "]\n";
    }
    if door.is_closed {
        return "  " + dir_link + " [closed " + door.name + "]\n";
    }
    return "  " + dir_link + " [open " + door.name + "] - " + dest.title + "\n";
}

fn format_exit_line(room_id, direction, target_id, connection_id) {
    let dest = get_room_data(target_id);
    if dest == () { return ""; }

    let dir_cap = direction;
    if direction == "north" { dir_cap = "North"; }
    if direction == "south" { dir_cap = "South"; }
    if direction == "east" { dir_cap = "East"; }
    if direction == "west" { dir_cap = "West"; }
    if direction == "up" { dir_cap = "Up"; }
    if direction == "down" { dir_cap = "Down"; }

    let dir_link = mxp_or(mxp_send("go " + direction, dir_cap), dir_cap, connection_id);
    let door = get_door(room_id, direction);

    if door != () {
        return format_exit_with_door(dest, dir_link, door);
    }
    return "  " + dir_link + " - " + dest.title + "\n";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to see exits.");
        return;
    }

    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are in a void. Something is wrong.");
        return;
    }

    let exits = room.exits;
    let room_id = room.id;
    let output = "Obvious exits:\n";
    let has_exits = false;

    if exits.north != "" {
        let line = format_exit_line(room_id, "north", exits.north, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }
    if exits.east != "" {
        let line = format_exit_line(room_id, "east", exits.east, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }
    if exits.south != "" {
        let line = format_exit_line(room_id, "south", exits.south, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }
    if exits.west != "" {
        let line = format_exit_line(room_id, "west", exits.west, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }
    if exits.up != "" {
        let line = format_exit_line(room_id, "up", exits.up, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }
    if exits.down != "" {
        let line = format_exit_line(room_id, "down", exits.down, connection_id);
        if line != "" {
            output += line;
            has_exits = true;
        }
    }

    if !has_exits {
        output = "There are no obvious exits.";
    }

    send_mxp_message(connection_id, output);
}
