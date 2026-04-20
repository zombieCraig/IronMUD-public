// scripts/commands/ilist.rhai
// ilist - List items (builder command)
// Usage: ilist [search]      - Search items in current area by name/vnum
//        ilist detail        - Show verbose output for current area
//        ilist all [search]  - Search all items across all areas
//        ilist all detail    - Verbose output globally

fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Parse arguments
    let words = args.split(" ");
    let show_all = false;
    let show_detail = false;
    let search_term = "";

    for word in words {
        let word_lower = word.to_lower();
        if word_lower == "all" {
            show_all = true;
        } else if word_lower == "detail" || word_lower == "details" || word_lower == "-v" {
            show_detail = true;
        } else if word != "" {
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

    // Get all items
    let all_items = list_all_items();

    if all_items.len() == 0 {
        send_client_message(connection_id, "No items found. Use 'oedit create' to make one.");
        return;
    }

    // Determine which items to show based on area scope
    let items;

    if show_all || current_area_id == "" {
        items = all_items;
        area_name = "All Areas";
    } else {
        let area = get_area_data(current_area_id);
        area_name = if area != () { area.name } else { "Current Area" };

        // Get spawn points to find item vnums in this area
        let area_vnums = [];
        let spawns = list_spawn_points_in_area(current_area_id);
        for sp in spawns {
            if (sp.entity_type == "item" || sp.entity_type == "object") && sp.vnum != "" {
                if !area_vnums.contains(sp.vnum) {
                    area_vnums.push(sp.vnum);
                }
            }
        }

        // Filter prototypes by spawn point vnums
        items = [];
        for item in all_items {
            if item.is_prototype {
                if item.vnum != "" && area_vnums.contains(item.vnum) {
                    items.push(item);
                }
            }
        }
    }

    // Filter by search term if provided
    if search_term != "" {
        let filtered = [];
        let term_lower = search_term.to_lower();
        for item in items {
            if item.name.to_lower().contains(term_lower) || item.vnum.to_lower().contains(term_lower) {
                filtered.push(item);
            }
        }

        if filtered.len() == 0 {
            let scope = if show_all || current_area_id == "" { "" } else { " in " + area_name };
            send_client_message(connection_id, "No items found matching '" + search_term + "'" + scope + ".");
            return;
        }

        show_detailed_list(filtered, search_term, connection_id, area_name);
        return;
    }

    // Check for detail flag
    if show_detail {
        show_detailed_list(items, "", connection_id, area_name);
        return;
    }

    // Compact columnar display
    let term_width = get_terminal_width(connection_id);

    let proto_vnums = [];
    let instance_count = 0;

    for item in items {
        if item.is_prototype && item.vnum != "" {
            let vnum_link = mxp_or(
                mxp_send("oedit " + item.vnum, item.vnum),
                item.vnum,
                connection_id
            );
            proto_vnums.push(vnum_link);
        } else if !item.is_prototype {
            instance_count += 1;
        }
    }

    let output = c_header("=== Item Prototypes in " + area_name + " (" + proto_vnums.len() + " vnums) ===", connection_id) + "\n";

    if proto_vnums.len() > 0 {
        output += format_columns(proto_vnums, term_width, 2);
    } else {
        output += "(none)\n";
    }

    if instance_count > 0 {
        output += "\n(" + instance_count + " spawned instances)\n";
    }

    if !show_all && current_area_id != "" {
        output += "\nUse 'ilist all' to see all items across all areas.";
    }
    output += "\nUse 'ilist <search>' to search, 'ilist detail' for verbose output.";

    send_mxp_message(connection_id, output);
}

fn show_detailed_list(items, search_term, connection_id, area_name) {
    let header_text = if search_term != "" {
        "=== Items matching '" + search_term + "' in " + area_name + " (" + items.len() + " found) ==="
    } else {
        "=== Item List in " + area_name + " (" + items.len() + " items) ==="
    };
    let output = c_header(header_text, connection_id) + "\n\n";

    for item in items {
        let display_name = item.name;
        if item.is_prototype {
            display_name = "[PROTO] " + display_name;
            if item.vnum != "" {
                display_name += " (" + item.vnum + ")";
            }
        }
        let name_link = mxp_or(
            mxp_menu(["oedit " + item.id, "idelete " + item.id], ["Edit Item", "Delete Item"], display_name),
            display_name,
            connection_id
        );
        output += name_link + "\n";
        output += "  ID: " + item.id + "\n";
        output += "  Type: " + item.item_type + "\n\n";
    }

    send_mxp_message(connection_id, output);
}
