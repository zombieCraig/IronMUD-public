// scripts/commands/rlist.rhai
// Usage: rlist [search]      - Search rooms in current area by title/vnum
//        rlist detail        - Show verbose output for current area
//        rlist all [search]  - Search all rooms across all areas

fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Parse arguments
    let args_lower = args.to_lower();
    let words = args.split(" ");
    let show_all = false;
    let show_detail = false;
    let search_term = "";

    // Parse each word
    for word in words {
        let word_lower = word.to_lower();
        if word_lower == "all" {
            show_all = true;
        } else if word_lower == "detail" || word_lower == "details" || word_lower == "-v" {
            show_detail = true;
        } else if word != "" {
            // Anything else is a search term
            if search_term != "" {
                search_term += " " + word;
            } else {
                search_term = word;
            }
        }
    }

    // Get current room's area
    let current_room = get_room_data(char.current_room_id);
    let current_area_id = if current_room != () { current_room.area_id } else { "" };
    let area_name = "";

    // Get rooms based on filtering
    let rooms;
    if show_all || current_area_id == "" {
        rooms = list_all_rooms();
        area_name = "All Areas";
    } else {
        rooms = get_rooms_in_area(current_area_id);
        let area = get_area_data(current_area_id);
        area_name = if area != () { area.name } else { "Current Area" };
    }

    if rooms.len() == 0 {
        send_client_message(connection_id, "No rooms found.");
        return;
    }

    // Filter by search term if provided
    if search_term != "" {
        let filtered = [];
        let term_lower = search_term.to_lower();
        for room in rooms {
            if room.title.to_lower().contains(term_lower) || room.vnum.to_lower().contains(term_lower) {
                filtered.push(room);
            }
        }

        if filtered.len() == 0 {
            send_client_message(connection_id, "No rooms found matching '" + search_term + "' in " + area_name + ".");
            return;
        }

        show_detailed_list(filtered, char, connection_id, area_name, show_all, search_term);
        return;
    }

    // Check for detail flag
    if show_detail {
        show_detailed_list(rooms, char, connection_id, area_name, show_all, "");
        return;
    }

    // Compact columnar display
    let term_width = get_terminal_width(connection_id);

    // Collect room vnums
    let room_vnums = [];

    for room in rooms {
        if room.vnum != "" {
            // Create MXP link for vnum
            let vnum_link = mxp_or(
                mxp_send("rgoto " + room.vnum, room.vnum),
                room.vnum,
                connection_id
            );
            room_vnums.push(vnum_link);
        }
    }

    let output = c_header("=== Room Vnums in " + area_name + " (" + room_vnums.len() + " rooms) ===", connection_id) + "\n";

    if room_vnums.len() > 0 {
        output += format_columns(room_vnums, term_width, 2);
    } else {
        output += "(no rooms with vnums)\n";
    }

    if !show_all && current_area_id != "" {
        output += "\nUse 'rlist all' to see all rooms across all areas.";
    }
    output += "\nUse 'rlist <search>' to search, 'rlist detail' for verbose output.";

    send_mxp_message(connection_id, output);
}

fn show_detailed_list(rooms, char, connection_id, area_name, show_all, search_term) {
    let header_text = if search_term != "" {
        "=== Rooms matching '" + search_term + "' in " + area_name + " (" + rooms.len() + " found) ==="
    } else {
        "=== Room List in " + area_name + " (" + rooms.len() + " rooms) ==="
    };
    let output = c_header(header_text, connection_id) + "\n";

    for room in rooms {
        // Mark current room
        let marker = "";
        if room.id == char.current_room_id {
            marker = " <- You are here";
        }
        // Make room title clickable with popup menu for goto/redit
        let title_link = mxp_or(
            mxp_menu(["rgoto " + room.id, "redit"], ["Go To Room", "Edit Current Room"], room.title),
            room.title,
            connection_id
        );
        output += title_link + marker + "\n";
        let vnum_str = room.vnum;
        if vnum_str != "" {
            // Make vnum clickable to goto
            let vnum_link = mxp_or(mxp_send("rgoto " + vnum_str, vnum_str), vnum_str, connection_id);
            output += "  Vnum: " + vnum_link + "\n";
        }
        output += "  ID: " + room.id + "\n";
    }

    if !show_all {
        output += "\nUse 'rlist all detail' to see all rooms across all areas.";
    }

    send_mxp_message(connection_id, output);
}
