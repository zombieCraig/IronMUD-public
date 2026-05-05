// scripts/commands/donate.rhai
// donate <item> — Send an inventory item to the current area's donation room.
// Donations are optional per-area; if no donation room is configured the
// command refuses. Donated items decay (default 30 minutes) via the
// donation-decay tick — picking one up clears the timer.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't donate while asleep.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Donate what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    if item.flags.no_drop {
        send_client_message(connection_id, "You can't bring yourself to part with " + item.short_desc + ".");
        return;
    }

    if item.flags.no_donate {
        send_client_message(connection_id, "That cannot be donated.");
        return;
    }

    // Containers can be donated only when empty.
    if item.item_type == "container" && item.container_contents.len() > 0 {
        send_client_message(connection_id, "Empty " + item.short_desc + " before donating it.");
        return;
    }

    // Resolve the destination from the player's current area.
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "Donations are not accepted here.");
        return;
    }

    let dest_vnum = get_area_donation_room_vnum(room.area_id);
    if dest_vnum == "" {
        send_client_message(connection_id, "Donations are not accepted here.");
        return;
    }

    let dest_room = get_room_by_vnum(dest_vnum);
    if dest_room == () {
        send_client_message(connection_id, "The donation chute is broken — tell a builder.");
        return;
    }

    move_item_to_room(item.id, dest_room.id);
    mark_item_donated(item.id);

    send_client_message(connection_id, "You donate " + item.short_desc + ".");
    broadcast_to_room_awake(
        dest_room.id,
        item.short_desc + " appears in a flash of charity.",
        ""
    );
}
