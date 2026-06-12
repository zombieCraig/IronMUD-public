// scripts/commands/wear.rhai
// wear <item> - Equip an item from inventory

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Wear what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    // Cyberware is never worn — it goes under the skin at a ripperdoc's
    // table (the install_cyberware capability).
    if item.item_type == "cyberware" {
        send_client_message(connection_id, item.name + " is surgical hardware. It has to be installed, not worn.");
        return;
    }

    let wear_locs = item.wear_locations;
    if wear_locs.len() == 0 {
        send_client_message(connection_id, "You can't wear " + item.name + ".");
        return;
    }

    // Check level requirement
    if item.level_requirement > 0 && char.level < item.level_requirement {
        send_client_message(connection_id, "You must be level " + item.level_requirement + " to wear " + item.name + ".");
        return;
    }

    // Alignment gates (ANTI_EVIL/GOOD/NEUTRAL). Tier thresholds at +/-24 so any
    // tier beyond Neutral triggers the corresponding anti-flag.
    if item.flags.anti_evil && char.morality < -24 {
        send_client_message(connection_id, "The " + item.name + " burns your hand and refuses your grasp.");
        return;
    }
    if item.flags.anti_good && char.morality > 24 {
        send_client_message(connection_id, "The " + item.name + " feels cold and lifeless in your hand; it rejects you.");
        return;
    }
    if item.flags.anti_neutral && char.morality >= -24 && char.morality <= 24 {
        send_client_message(connection_id, "The " + item.name + " demands conviction you do not yet possess.");
        return;
    }

    // Check for conflicts
    let check = can_wear_item(char.name, item.id);
    if !check["can_wear"] {
        let conflicts = check["conflicts"];
        let msg = "You need to remove ";
        let first = true;
        for conflict in conflicts {
            if !first { msg += ", "; }
            msg += conflict;
            first = false;
        }
        msg += " first.";
        send_client_message(connection_id, msg);
        return;
    }

    move_item_to_equipped(item.id, char.name);
    send_client_message(connection_id, "You wear " + item.name + ".");
    broadcast_to_room(char.current_room_id, char.name + " wears " + item.name + ".", char.name);

    // Ad-hoc script side effects on equip. Buff stamping is handled in Rust by
    // db::stamp_item_buffs_on_character — this trigger is for emotes,
    // room messages, custom flavor, etc.
    fire_item_trigger(item.id, "on_wear", connection_id, "");
    return;
}
