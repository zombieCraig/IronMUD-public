// scripts/commands/write.rhai
// write <paper> — Author or revise a note in player-held paper.
// Requires a Pen-type item anywhere in the writer's inventory.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't write while asleep.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Write on what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    if item.item_type != "note" {
        send_client_message(connection_id, "You can't write on that.");
        return;
    }

    let has_pen = false;
    for inv in inv_items {
        if inv.item_type == "pen" {
            has_pen = true;
            break;
        }
    }
    if !has_pen {
        send_client_message(connection_id, "You'll need a pen to write with.");
        return;
    }

    let existing = get_item_note_content(item.id);
    let lines = if existing != "" { existing.split("\n") } else { [] };

    set_olc_edit_item(connection_id, item.id);
    set_olc_buffer(connection_id, lines);
    set_olc_mode(connection_id, "collecting_note");

    let output = "Writing on: " + item.short_desc + "\n";
    if lines.len() == 0 {
        output += "(blank)\n";
    } else {
        for i in 0..lines.len() {
            output += "  " + (i + 1) + ": " + lines[i] + "\n";
        }
    }
    output += "\nCommands: .h help | .l list | .c clear | .d N | .r N <text> | . save | @ cancel\n";
    output += "Type lines to append. Blank lines and whitespace are preserved.";
    send_client_message(connection_id, output);
}
