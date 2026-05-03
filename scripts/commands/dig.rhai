// scripts/commands/dig.rhai
// Player usage: dig                       (unearth buried items in a dirt_floor room)
// Builder usage: dig <direction> [title]  (creates a new linked room)

fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "down"; }
    if dir == "down" { return "up"; }
    return "";
}

fn normalize_direction(dir) {
    let d = dir.to_lower();
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

fn can_player_dig(char) {
    let r = char.race.to_lower();
    if r == "canine" || r == "gnoll" || r == "kobold" {
        return #{ ok: true, source: "your claws" };
    }
    let inv = get_items_in_inventory(char.name);
    for item in inv {
        if item.flags.can_dig {
            return #{ ok: true, source: item.short_desc };
        }
    }
    let eq = get_equipped_items(char.name);
    for item in eq {
        if item.flags.can_dig {
            return #{ ok: true, source: item.short_desc };
        }
    }
    return #{ ok: false, source: "" };
}

fn run_treasure_dig(char, connection_id) {
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }
    if !room.flags.dirt_floor {
        send_client_message(connection_id, "The ground here is too hard to dig.");
        return;
    }
    let gate = can_player_dig(char);
    if !gate.ok {
        send_client_message(connection_id, "You need something to dig with.");
        return;
    }
    let buried = get_buried_items_in_room(char.current_room_id);
    if buried.len() == 0 {
        send_client_message(connection_id, "You dig around with " + gate.source + " but find nothing.");
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " digs around in the dirt.");
        return;
    }
    for item in buried {
        set_item_buried(item.id, false);
        send_client_message(connection_id, "You unearth " + item.short_desc + "!");
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " digs up " + item.short_desc + ".");
    }
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to dig.");
        return;
    }

    let parts = args.split(" ");
    let first = "";
    if parts.len() >= 1 { first = normalize_direction(parts[0]); }
    let valid_dirs = ["north", "south", "east", "west", "up", "down"];
    let is_dir = false;
    for v in valid_dirs {
        if first == v { is_dir = true; }
    }

    // No-arg or non-direction arg: route to treasure dig
    if !is_dir {
        run_treasure_dig(char, connection_id);
        return;
    }

    let direction = first;

    // Get current room
    let current_room = get_room_data(char.current_room_id);
    if current_room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check area permissions if current room belongs to an area
    let room_area_id = current_room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to dig from rooms in this area.");
            return;
        }
    }

    // Check if exit already exists
    let exits = current_room.exits;
    let existing_exit = "";
    if direction == "north" { existing_exit = exits.north; }
    if direction == "south" { existing_exit = exits.south; }
    if direction == "east" { existing_exit = exits.east; }
    if direction == "west" { existing_exit = exits.west; }
    if direction == "up" { existing_exit = exits.up; }
    if direction == "down" { existing_exit = exits.down; }

    if existing_exit != "" {
        send_client_message(connection_id, "An exit already exists to the " + direction + ". Use 'unlink " + direction + "' first.");
        return;
    }

    // Parse title (remaining args joined)
    let title = "A New Room";
    if parts.len() >= 2 {
        let title_parts = [];
        for i in 1..parts.len() {
            title_parts.push(parts[i]);
        }
        title = title_parts.reduce(|a, b| a + " " + b);
    }

    // Create the new room
    let new_room = create_room(title, "This room has no description yet.");
    let new_room_id = new_room.id;

    // Check if current room is a property template room
    let is_template = current_room.is_property_template;
    let template_id = current_room.property_template_id;

    // If digging from a template room, link new room to the same template
    if is_template && template_id != "" && template_id != () {
        mark_room_as_template(new_room_id, template_id);
    }

    // Inherit area from current room and auto-generate vnum
    let inherited_vnum = "";
    if room_area_id != "" {
        set_room_area(new_room_id, room_area_id);
        apply_area_default_room_flags(new_room_id);

        // Only auto-generate vnum if a custom title was provided (not default)
        // and this is not a template room (template rooms don't need vnums)
        if title != "A New Room" && !is_template {
            let area = get_area_data(room_area_id);
            if area != () {
                inherited_vnum = generate_unique_vnum(area.prefix, title);
                set_room_vnum(new_room_id, inherited_vnum);
            }
        }
    }

    // Link current room to new room
    set_room_exit(char.current_room_id, direction, new_room_id);

    // Link new room back to current room
    let opposite = get_opposite_direction(direction);
    if opposite != "" {
        set_room_exit(new_room_id, opposite, char.current_room_id);
    }

    // Build response
    let output = "Created room: " + title + "\n";
    output += "Room ID: " + new_room_id + "\n";
    if inherited_vnum != "" {
        output += "Vnum: " + inherited_vnum + "\n";
    }
    if is_template {
        output += "(Room linked to property template)\n";
    }
    if room_area_id != "" {
        let area = get_area_data(room_area_id);
        if area != () {
            output += "Area: " + area.name + " [" + area.prefix + "]\n";
        }
    }
    output += "Exit " + direction + " -> " + title + "\n";
    if opposite != "" {
        output += "Reverse exit " + opposite + " -> " + current_room.title;
    }

    send_client_message(connection_id, output);
}
