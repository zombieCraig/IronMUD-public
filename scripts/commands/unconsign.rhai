// scripts/commands/unconsign.rhai
//
//   unconsign <number>  - pull one of your listings back off a broker's shelf
//
// The same withdrawal `consignments take <n>` performs. Two spellings because
// "unconsign" is the verb a player reaches for and the subcommand is the one
// they find while looking at the list.
//
// Both go through `unconsign_item`, which requires the broker holding that
// listing to be in the room. Checked there rather than here so the two
// spellings cannot end up with two rules.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let word = text::normalise(args);
    if word == "" {
        send_client_message(connection_id, "Unconsign which one? Use the number from 'consignments'.");
        return;
    }

    let index = try_parse_int(word);
    if index == () || index < 1 {
        send_client_message(connection_id, "Which one? Use the number from 'consignments'.");
        return;
    }

    let listings = my_consignments(char.name);
    if listings.len() == 0 {
        send_client_message(connection_id, "You have nothing out with any broker.");
        return;
    }
    if index > listings.len() {
        send_client_message(connection_id, "You do not have that many out.");
        return;
    }

    let outcome = unconsign_item(char.name, char.current_room_id, listings[index - 1].id);
    send_client_message(connection_id, outcome.message);
}
