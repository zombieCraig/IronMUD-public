// scripts/commands/ifind.rhai
// ifind <keyword> - Search items by keyword (builder command)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: ifind <keyword>");
        return;
    }

    let results = search_items(args);

    if results.len() == 0 {
        send_client_message(connection_id, "No items found matching: " + args);
        return;
    }

    let output = "=== Item Search: \"" + args + "\" ===\n";
    output += "Found " + results.len() + " item(s):\n\n";

    for item in results {
        let name_link = mxp_or(
            mxp_menu(["oedit " + item.id, "idelete " + item.id], ["Edit Item", "Delete Item"], item.name),
            item.name,
            connection_id
        );
        output += name_link + "\n";
        output += "  ID: " + item.id + "\n";
        output += "  Type: " + item.item_type + "\n\n";
    }

    send_mxp_message(connection_id, output);
}
