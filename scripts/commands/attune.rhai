// scripts/commands/attune.rhai
// Replicant signature item: designate a carried memento as the anchor for
// the `focus` recovery ritual. Replacing a previous attunement — whether the
// old memento is lost or simply set aside — costs Resolve in grief, and the
// new bond needs 24 real-time hours to mature before focus works.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_replicant(connection_id) {
        send_client_message(connection_id, "Objects are just objects to you.");
        return;
    }

    let arg = args;
    if arg == "" {
        let current = get_pc_signature_item_id(connection_id);
        if current == "" {
            send_client_message(connection_id, "Usage: attune <item>\nYou have no signature item. Choose something that matters.");
        } else {
            let item = get_item_data(current);
            let item_name = "something you no longer carry";
            if item != () { item_name = item.name; }
            let bonded = is_signature_item_bonded(connection_id);
            if bonded {
                send_client_message(connection_id, "Your signature item is " + item_name + ". The bond is set.");
            } else {
                send_client_message(connection_id, "Your signature item is " + item_name + ". It doesn't carry the weight of memory yet.");
            }
        }
        return;
    }

    // Optional trailing "confirm" for the grief warning flow.
    let confirmed = false;
    let keyword = arg;
    if keyword.ends_with(" confirm") {
        confirmed = true;
        keyword = keyword.sub_string(0, keyword.len() - 8);
    }

    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        // Fall back to worn/wielded gear.
        let kw_lower = keyword.to_lower();
        for eq in get_equipped_items(char.name) {
            if eq.name.to_lower().contains(kw_lower) {
                item = eq;
                break;
            }
        }
    }
    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    let result = attune_pc_signature_item(connection_id, item.id, confirmed);
    if result["success"] {
        if result["grief"] {
            send_client_message(connection_id, "\x1b[35mLetting go of the old memory tears something loose. (-2 Resolve)\x1b[0m");
            let new_resolve = get_pc_resolve(connection_id);
            if new_resolve == 0 {
                let bd = trigger_pc_breakdown(connection_id);
                if bd["success"] {
                    send_client_message(connection_id, "\x1b[1;31m" + bd["message"] + "\x1b[0m");
                    let room_msg = bd["room_message"];
                    room_msg.replace("{name}", char.name);
                    broadcast_to_room(char.current_room_id, room_msg, char.name);
                }
            }
        }
        send_client_message(connection_id, "\x1b[36mYou turn " + item.name + " over in your hands and decide: this one is yours. This one is real.\x1b[0m");
        send_client_message(connection_id, "The bond will take a day to settle before you can focus on it.");
        return;
    }

    let error = result["error"];
    if error == "needs confirm" {
        send_client_message(connection_id, "\x1b[33mYou already carry a memory. Replacing it will cost you a piece of yourself. (-2 Resolve)\x1b[0m");
        send_client_message(connection_id, "Type 'attune " + keyword + " confirm' if you mean it.");
        return;
    }
    if error == "already attuned" {
        send_client_message(connection_id, "That is already your signature item.");
        return;
    }
    send_client_message(connection_id, "You can't attune that right now.");
}
