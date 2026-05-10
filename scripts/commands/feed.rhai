// scripts/commands/feed.rhai
//
// Vampire feed — drain blood from a mortal in your room.
// Underlying mechanics live in `vampire_feed_on_mobile` (src/script/vampire.rs)
// so the bite, blood transfer, wound stamping, masquerade trip, and humanity
// loss all happen atomically. This script handles target selection,
// validation, broadcasts, and the player-facing messaging.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to feed.");
        return;
    }

    if !is_pc_vampire(connection_id) {
        send_client_message(connection_id, "You have no fangs. The mortal world is yours alone.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't feed in your sleep.");
        return;
    }

    let target_keyword = args;
    if target_keyword == "" {
        send_client_message(connection_id, "Feed on whom? (feed <target>)");
        return;
    }

    let room_id = char.current_room_id;
    let mobiles = get_mobiles_in_room(room_id);
    let target = find_mobile_by_keyword(target_keyword, mobiles);
    if target == () {
        send_client_message(connection_id, "There's no '" + target_keyword + "' here to feed on.");
        return;
    }

    let result = vampire_feed_on_mobile(connection_id, target.id);
    if !result["success"] {
        let reason = result["error"];
        send_client_message(connection_id, "You cannot feed: " + reason + ".");
        return;
    }

    let damage = result["damage"];
    let blood = result["blood"];
    let killed = result["killed"];
    let humanity_loss = result["humanity_loss"];
    let masquerade_break = result["masquerade_break"];

    // Self-message + room broadcast.
    if killed {
        send_client_message(connection_id, "You drain " + target.short_desc + " dry. (+" + blood + " blood)");
        broadcast_to_room_except(room_id, connection_id, char.name + " sinks fangs into " + target.short_desc + " and drains them dry.");
    } else {
        send_client_message(connection_id, "You sink your fangs into " + target.short_desc + " and drink. (+" + blood + " blood, " + damage + " hp drained)");
        broadcast_to_room_except(room_id, connection_id, char.name + " sinks fangs into " + target.short_desc + ".");
    }

    if humanity_loss > 0 {
        send_client_message(connection_id, "\x1b[1;31mYour humanity wavers — you took an innocent life. (-" + humanity_loss + " humanity)\x1b[0m");
    }
    if masquerade_break {
        send_client_message(connection_id, "\x1b[1;33mThe masquerade slips. They saw the fangs.\x1b[0m");
    }
}
