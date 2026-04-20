// scripts/commands/alist.rhai
// Usage: alist
// Lists all areas in the database

fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let areas = list_all_areas();

    if areas.len() == 0 {
        send_client_message(connection_id, "No areas found. Use 'acreate <prefix> <name>' to create one.");
        return;
    }

    let output = c_header("=== Area List (" + areas.len() + " areas) ===", connection_id) + "\n";

    for area in areas {
        // Make area name clickable with popup menu for edit/delete
        let name_link = mxp_or(
            mxp_menu(["aedit " + area.id, "adelete " + area.id], ["Edit Area", "Delete Area"], area.name),
            area.name,
            connection_id
        );
        output += name_link + " [" + area.prefix + "]\n";
        output += "  ID: " + area.id + "\n";
        if area.description != "" {
            output += "  " + area.description + "\n";
        }
        let rooms = get_rooms_in_area(area.id);
        output += "  Rooms: " + rooms.len() + "\n";
    }

    send_mxp_message(connection_id, output);
}
