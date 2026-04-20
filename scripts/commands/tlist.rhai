// scripts/commands/tlist.rhai
// Usage: tlist [keyword]
// Usage: tlist details <vnum_or_id>
// List all transports, optionally filtered by keyword

fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn c_label(text, conn_id) {
    color_or(ansi_cyan(text), text, conn_id)
}

fn c_value(text, conn_id) {
    let s = "" + text;
    color_or(ansi_white(s), s, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if !char.is_builder && !char.is_admin {
        send_client_message(connection_id, "You must be a builder to use this command.");
        return;
    }

    let parts = args.split(" ");

    // Handle details subcommand
    if parts.len() > 0 && parts[0].to_lower() == "details" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: tlist details <vnum_or_id>");
            return;
        }
        show_details(parts[1], connection_id);
        return;
    }

    show_list(args, connection_id);
}

fn show_list(args, connection_id) {
    let transports = [];
    if args == "" {
        transports = list_all_transports();
    } else {
        transports = search_transports(args);
    }

    if transports.len() == 0 {
        if args == "" {
            send_client_message(connection_id, "No transports found.\nUse 'tedit create <vnum>' to create one.");
        } else {
            send_client_message(connection_id, "No transports found matching '" + args + "'.");
        }
        return;
    }

    let output = c_header("=== Transports ===", connection_id) + "\n";

    for transport in transports {
        output += format_transport_line(transport, connection_id);
    }

    output += "\nTotal: " + transports.len() + " transport(s)";
    output += "\nUse 'tedit <vnum>' to edit, 'tlist details <vnum>' for more info.";

    send_client_message(connection_id, output);
}

fn format_transport_line(transport, connection_id) {
    let vnum_str = "(no vnum)";
    if transport.vnum != () {
        vnum_str = "" + transport.vnum;
    }

    let state_str = "" + transport.state;
    let schedule_str = "on-demand";
    if !is_transport_ondemand(transport.id) {
        schedule_str = "scheduled";
    }

    let line = c_label(vnum_str, connection_id) + " - " + transport.name;
    line += " [" + transport.transport_type + ", " + schedule_str + ", " + state_str + "]";
    line += " (" + transport.stops.len() + " stops)";
    line += "\n  ID: " + transport.id + "\n";
    return line;
}

fn show_details(identifier, connection_id) {
    let transport = get_transport_by_vnum(identifier);
    if transport == () {
        transport = get_transport_data(identifier);
    }

    if transport == () {
        send_client_message(connection_id, "Transport not found: " + identifier);
        return;
    }

    let output = c_header("=== Transport Details ===", connection_id) + "\n";
    output += format_details_header(transport, connection_id);
    output += format_details_interior(transport, connection_id);
    output += format_details_stops(transport, connection_id);

    send_client_message(connection_id, output);
}

fn format_details_header(transport, connection_id) {
    let vnum_str = "(none)";
    if transport.vnum != () {
        vnum_str = "" + transport.vnum;
    }

    let output = c_label("Vnum: ", connection_id) + c_value(vnum_str, connection_id) + "\n";
    output += c_label("Name: ", connection_id) + c_value(transport.name, connection_id) + "\n";
    output += c_label("ID: ", connection_id) + c_value(transport.id, connection_id) + "\n";
    output += c_label("Type: ", connection_id) + c_value(transport.transport_type, connection_id) + "\n";
    output += c_label("State: ", connection_id) + c_value(transport.state, connection_id) + "\n";
    output += c_label("Current Stop: ", connection_id) + c_value(transport.current_stop_index + 1, connection_id) + "\n";
    output += c_label("Travel Time: ", connection_id) + c_value(transport.travel_time_secs, connection_id) + "s\n";
    return output;
}

fn format_details_interior(transport, connection_id) {
    let output = c_label("Interior Room: ", connection_id) + transport.interior_room_id + "\n";
    let interior = get_room_data(transport.interior_room_id);
    if interior != () {
        output += "  (" + interior.title + ")\n";
    }

    let schedule_str = "On-Demand";
    if !is_transport_ondemand(transport.id) {
        schedule_str = "Game-Time scheduled";
    }
    output += c_label("Schedule: ", connection_id) + schedule_str + "\n";
    return output;
}

fn format_details_stops(transport, connection_id) {
    let output = "\n" + c_label("Stops:", connection_id) + "\n";
    let stops = transport.stops;
    if stops.len() == 0 {
        output += "  (none)\n";
        return output;
    }
    for i in 0..stops.len() {
        output += format_stop_detail(stops[i], i, transport.current_stop_index);
    }
    return output;
}

fn format_stop_detail(stop, index, current_index) {
    let marker = "";
    if index == current_index {
        marker = " (current)";
    }
    let line = "  [" + (index + 1) + "] " + stop.name + " - " + stop.exit_direction + marker + "\n";
    line += "      Room: " + stop.room_id + "\n";
    let room = get_room_data(stop.room_id);
    if room != () {
        line += "      (" + room.title + ")\n";
    }
    return line;
}
