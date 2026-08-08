// scripts/commands/consignments.rhai
//
//   consignments             - what you currently have out with brokers
//   consignments take <n>    - pull listing #n back off the shelf
//
// `unconsign` is a separate command name for the same withdrawal, because
// "unconsign" is what a player reaches for and a subcommand is what a player
// finds. Both route through the same binding.

import "text" as text;
import "progress_ui" as ui;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let word = text::normalise(args);
    if word.starts_with("take ") || word.starts_with("withdraw ") {
        let parts = word.split(" ");
        take_back(char, parts[parts.len() - 1], connection_id);
        return;
    }
    if word != "" {
        send_client_message(connection_id, "Usage: consignments  |  consignments take <number>");
        return;
    }

    show_listings(char, connection_id);
}

fn show_listings(char, connection_id) {
    let listings = my_consignments(char.name);
    if listings.len() == 0 {
        send_client_message(connection_id, "You have nothing out with any broker.");
        return;
    }

    let output = "\x1b[1;37mYour consignments\x1b[0m\n";
    let n = 1;
    for listing in listings {
        output += "  " + text::pad_left("" + n, 2) + ". " + text::pad_right(listing.item_name, 28)
            + text::pad_left("" + listing.price, 7) + " gold"
            + "   expires in " + ui::format_playtime(listing.expires_in_secs) + "\n";
        // Withdrawal happens at the counter, so the counter is worth naming.
        output += "      with " + listing.broker + "\n";
        n += 1;
    }
    output += "\nProceeds land in your bank, so you do not have to be standing there when it sells.\n";
    output += "Taking one back means going to the broker holding it.\n";
    output += "An expired listing goes to escrow rather than being destroyed.";
    send_client_message(connection_id, output);
}

// Withdrawal is by list position rather than by name: two identical items at
// two different prices are a normal thing to have out, and a keyword cannot
// tell them apart.
fn take_back(char, index_word, connection_id) {
    let index = try_parse_int(index_word);
    if index == () || index < 1 {
        send_client_message(connection_id, "Which one? Use the number from 'consignments'.");
        return;
    }

    let listings = my_consignments(char.name);
    if index > listings.len() {
        send_client_message(connection_id, "You do not have that many out.");
        return;
    }

    let outcome = unconsign_item(char.name, char.current_room_id, listings[index - 1].id);
    send_client_message(connection_id, outcome.message);
}
