// scripts/commands/consign.rhai
//
//   consign <item> <price>  - leave an item with a broker for another player to buy
//
// The whole transaction — ownership check, refusal rules, price band, listing
// cap, the item move — happens in `consign_item` on the Rust side. This script
// finds the broker, resolves the item, and says what happened. Splitting a
// trade across several script steps would make a half-applied deal reachable
// from a typo, and this is the one system where that costs someone real value.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let broker = find_consignment_broker_in_room(char.current_room_id);
    if broker == () {
        send_client_message(connection_id, "There is nobody here who takes goods on consignment.");
        return;
    }

    let activity = get_mobile_activity(broker.id);
    if activity != "working" {
        send_client_message(connection_id, broker.name + " is not doing business right now.");
        return;
    }

    let trimmed = text::trim_spaces(args);
    if trimmed == "" {
        send_client_message(connection_id, "Consign what, and for how much? Usage: consign <item> <price>");
        return;
    }

    let words = trimmed.split(" ");
    if words.len() < 2 {
        send_client_message(connection_id, "You have to name a price. Usage: consign <item> <price>");
        return;
    }

    let price = try_parse_int(words[words.len() - 1]);
    if price == () || price <= 0 {
        send_client_message(connection_id, "That is not a price. Usage: consign <item> <price>");
        return;
    }

    // Everything but the last word is the item keyword, so multi-word names
    // work without quoting.
    let keyword = "";
    for i in 0..(words.len() - 1) {
        if keyword != "" { keyword += " "; }
        keyword += words[i];
    }

    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        send_client_message(connection_id, "You are not carrying anything like that.");
        return;
    }

    let outcome = consign_item(char.name, item.id, broker.id, price);
    send_client_message(connection_id, outcome.message);
    if outcome.ok {
        broadcast_to_room(char.current_room_id, char.name + " hands something to " + broker.name + ".", char.name);
    }
}
