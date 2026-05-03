// scripts/commands/bury.rhai
// bury <item> - Conceal a held item in the dirt floor of the current room.
//               Requires a dirt_floor room and a digging tool (or natural digging race).

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

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Bury what?");
        return;
    }

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

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    if item.flags.no_drop && !char.god_mode {
        send_client_message(connection_id, "You can't bury that!");
        return;
    }
    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot bury quest items.");
        return;
    }

    move_item_to_room(item.id, char.current_room_id);
    set_item_buried(item.id, true);

    send_client_message(connection_id, "You bury " + item.short_desc + " in the dirt.");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " buries something in the dirt.");
}
