// scripts/commands/focus.rhai
// Replicant downtime ritual: hold the bonded signature item and let the
// memory settle the mind. Restores Resolve. Per the Blade Runner source,
// this is a DOWNTIME action — it only works in a safe zone, out of combat.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_replicant(connection_id) {
        send_client_message(connection_id, "You squint and concentrate. Nothing happens.");
        return;
    }

    if is_pc_in_breakdown(connection_id) {
        send_client_message(connection_id, "Your mind is too loud to hold a single thought.");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "Not here. Not with your guard up.");
        return;
    }

    // Downtime only: the effective combat zone (room override or area
    // inheritance) must be "safe".
    let zone = get_combat_zone(char.current_room_id);
    if zone != "safe" {
        send_client_message(connection_id, "Not here. Not with your guard up. Find somewhere safe.");
        return;
    }

    let result = replicant_focus(connection_id);
    if result["success"] {
        let item = get_item_data(get_pc_signature_item_id(connection_id));
        let item_name = "your memento";
        if item != () { item_name = item.name; }
        let max_resolve = get_pc_max_resolve(connection_id);
        send_client_message(connection_id, "\x1b[36mYou hold " + item_name + " and let the memory come. It happened. It was yours. The static recedes.\x1b[0m");
        send_client_message(connection_id, "\x1b[36m(+" + result["restored"] + " Resolve — " + result["resolve"] + "/" + max_resolve + ")\x1b[0m");
        broadcast_to_room(char.current_room_id, char.name + " goes quiet, turning something small over in their hands.", char.name);
        return;
    }

    let error = result["error"];
    if error == "no signature item" {
        send_client_message(connection_id, "You have nothing to hold onto. Use 'attune <item>' to choose a memento.");
        return;
    }
    if error == "item gone" {
        send_client_message(connection_id, "\x1b[33mYou reach for it and it isn't there.\x1b[0m");
        send_client_message(connection_id, "When you're ready, attune a new memento. It will cost you.");
        return;
    }
    if error == "not bonded" {
        send_client_message(connection_id, "It doesn't carry the weight of memory yet. Give it time.");
        return;
    }
    if error == "cooldown" {
        send_client_message(connection_id, "The memory needs time to mean something again. (" + result["cooldown_remaining"] + "s)");
        return;
    }
    if error == "resolve full" {
        send_client_message(connection_id, "Your mind is as quiet as it gets.");
        return;
    }
    send_client_message(connection_id, "You can't focus right now.");
}
