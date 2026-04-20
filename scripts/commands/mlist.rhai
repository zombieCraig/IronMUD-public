// scripts/commands/mlist.rhai
// mlist - List mobiles (builder command)
// Usage: mlist [search]      - Search mobiles in current area by name/vnum
//        mlist detail        - Show verbose output for current area
//        mlist all [search]  - Search all mobiles across all areas
//        mlist all detail    - Verbose output globally

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
        } else if word_lower == "spawnable" {
            // Legacy alias - area-scoped is now the default behavior
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

    // Get all mobiles
    let all_mobiles = list_all_mobiles();

    if all_mobiles.len() == 0 {
        send_client_message(connection_id, "No mobiles found. Use 'medit create' to make one.");
        return;
    }

    // Determine which mobiles to show based on area scope
    let mobiles;

    if show_all || current_area_id == "" {
        mobiles = all_mobiles;
        area_name = "All Areas";
    } else {
        let area = get_area_data(current_area_id);
        area_name = if area != () { area.name } else { "Current Area" };

        // Get spawn points to find mobile vnums in this area
        let area_vnums = [];
        let spawns = list_spawn_points_in_area(current_area_id);
        for sp in spawns {
            if (sp.entity_type == "mobile" || sp.entity_type == "mob") && sp.vnum != "" {
                if !area_vnums.contains(sp.vnum) {
                    area_vnums.push(sp.vnum);
                }
            }
        }

        // Get area room IDs for instance filtering
        let area_rooms = get_rooms_in_area(current_area_id);
        let area_room_ids = [];
        for r in area_rooms {
            area_room_ids.push(r.id);
        }

        // Filter: prototypes by spawn point vnums, instances by room location
        mobiles = [];
        for mobile in all_mobiles {
            if mobile.is_prototype {
                if mobile.vnum != "" && area_vnums.contains(mobile.vnum) {
                    mobiles.push(mobile);
                }
            } else {
                if area_room_ids.contains(mobile.current_room_id) {
                    mobiles.push(mobile);
                }
            }
        }
    }

    // Filter by search term if provided
    if search_term != "" {
        let filtered = [];
        let term_lower = search_term.to_lower();
        for mobile in mobiles {
            if mobile.name.to_lower().contains(term_lower) || mobile.vnum.to_lower().contains(term_lower) {
                filtered.push(mobile);
            }
        }

        if filtered.len() == 0 {
            let scope = if show_all || current_area_id == "" { "" } else { " in " + area_name };
            send_client_message(connection_id, "No mobiles found matching '" + search_term + "'" + scope + ".");
            return;
        }

        show_detailed_list(filtered, search_term, connection_id, area_name);
        return;
    }

    // Check for detail flag
    if show_detail {
        show_detailed_list(mobiles, "", connection_id, area_name);
        return;
    }

    // Compact columnar display
    let term_width = get_terminal_width(connection_id);

    let proto_vnums = [];
    let instance_count = 0;

    for mobile in mobiles {
        if mobile.is_prototype && mobile.vnum != "" {
            let vnum_link = mxp_or(
                mxp_send("medit " + mobile.vnum, mobile.vnum),
                mobile.vnum,
                connection_id
            );
            proto_vnums.push(vnum_link);
        } else if !mobile.is_prototype {
            instance_count += 1;
        }
    }

    let output = c_header("=== Mobile Prototypes in " + area_name + " (" + proto_vnums.len() + " vnums) ===", connection_id) + "\n";

    if proto_vnums.len() > 0 {
        output += format_columns(proto_vnums, term_width, 2);
    } else {
        output += "(none)\n";
    }

    if instance_count > 0 {
        output += "\n(" + instance_count + " spawned instances)\n";
    }

    if !show_all && current_area_id != "" {
        output += "\nUse 'mlist all' to see all mobiles across all areas.";
    }
    output += "\nUse 'mlist <search>' to search, 'mlist detail' for verbose output.";

    send_mxp_message(connection_id, output);
}

fn get_mobile_display_name(mobile) {
    if mobile.is_prototype {
        let name = "[PROTO] " + mobile.name;
        if mobile.vnum != "" {
            name += " (" + mobile.vnum + ")";
        }
        return name;
    }
    mobile.name + " (instance of " + mobile.vnum + ")"
}

fn get_room_info(room) {
    if room == () {
        return "";
    }
    let info = room.title;
    if room.vnum != "" {
        info += " (" + room.vnum + ")";
    }
    info
}

fn format_mobile_entry(mobile, connection_id) {
    let display_name = get_mobile_display_name(mobile);
    let name_link = mxp_or(
        mxp_menu(["medit " + mobile.id, "mdelete " + mobile.id], ["Edit Mobile", "Delete/Despawn"], display_name),
        display_name,
        connection_id
    );

    let output = name_link + "\n";
    output += "  ID: " + mobile.id + "\n";

    if mobile.is_prototype {
        output += "  Level: " + mobile.level + "  HP: " + mobile.max_hp + "\n\n";
        return output;
    }

    // Instance: show current/max HP and room location
    output += "  Level: " + mobile.level + "  HP: " + mobile.current_hp + "/" + mobile.max_hp + "\n";
    let room = get_room_data(mobile.current_room_id);
    let room_info = get_room_info(room);
    if room_info != "" {
        output += "  Room: " + room_info + "\n";
    }
    output += "\n";
    output
}

fn show_detailed_list(mobiles, search_term, connection_id, area_name) {
    let header_text = if search_term != "" {
        "=== Mobiles matching '" + search_term + "' in " + area_name + " (" + mobiles.len() + " found) ==="
    } else {
        "=== Mobile List in " + area_name + " (" + mobiles.len() + " mobiles) ==="
    };
    let output = c_header(header_text, connection_id) + "\n\n";

    for mobile in mobiles {
        output += format_mobile_entry(mobile, connection_id);
    }

    send_mxp_message(connection_id, output);
}
