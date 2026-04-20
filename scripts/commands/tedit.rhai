// scripts/commands/tedit.rhai
// Usage: tedit <vnum> [subcommand]
// Edit transport properties (elevators, buses, trains)

// Import shared OLC helpers
import "olc_helpers" as olc;

// Dispatch subcommand to handler
fn dispatch_subcommand(subcommand, transport, parts, start, connection_id) {
    if subcommand == "vnum" { return handle_vnum(transport, parts, start, connection_id); }
    if subcommand == "name" { return handle_name(transport, parts, start, connection_id); }
    if subcommand == "type" { return handle_type(transport, parts, start, connection_id); }
    if subcommand == "interior" { return handle_interior(transport, parts, start, connection_id); }
    if subcommand == "traveltime" { return handle_traveltime(transport, parts, start, connection_id); }
    if subcommand == "schedule" { return handle_schedule(transport, parts, start, connection_id); }
    if subcommand == "stop" { return handle_stop(transport, parts, start, connection_id); }
    if subcommand == "connect" { return handle_connect(transport, connection_id); }
    if subcommand == "disconnect" { return handle_disconnect(transport, connection_id); }
    if subcommand == "delete" { return handle_delete(transport, connection_id); }
    return false;
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

    if args == "" {
        show_help(connection_id);
        return;
    }

    let parts = args.split(" ");

    // Handle create as special case
    if parts[0].to_lower() == "create" {
        handle_create(parts, char, connection_id);
        return;
    }

    // First arg is transport vnum or UUID
    let transport = get_transport_by_vnum(parts[0]);
    if transport == () {
        // Try by UUID
        transport = get_transport_data(parts[0]);
    }
    if transport == () {
        send_client_message(connection_id, "Transport not found: " + parts[0]);
        return;
    }

    // Get subcommand
    let subcommand = "";
    if parts.len() > 1 {
        subcommand = parts[1].to_lower();
    }

    // Show properties if no subcommand
    if subcommand == "" || subcommand == "show" {
        show_transport(transport, connection_id);
        return;
    }

    // Dispatch to handler
    if dispatch_subcommand(subcommand, transport, parts, 1, connection_id) {
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
}

fn show_help(connection_id) {
    let help = "Usage: tedit <vnum_or_id> [subcommand]\n";
    help += "  tedit create <vnum>               - Create new transport\n";
    help += "  tedit <vnum>                      - Show transport properties\n";
    help += "  tedit <id> vnum <new_vnum>        - Set/change vnum\n";
    help += "  tedit <vnum> name <text>          - Set display name\n";
    help += "  tedit <vnum> type <type>          - Set type\n";
    help += "  tedit <vnum> interior <room_vnum> - Set interior room\n";
    help += "  tedit <vnum> traveltime <secs>    - Set travel time\n";
    help += "  tedit <vnum> schedule ondemand    - Set on-demand mode\n";
    help += "  tedit <vnum> schedule gametime <freq> <start> <end> <dwell>\n";
    help += "  tedit <vnum> stop add/remove/list - Manage stops\n";
    help += "  tedit <vnum> connect              - Connect to current stop\n";
    help += "  tedit <vnum> disconnect           - Disconnect from stop\n";
    help += "  tedit <vnum> delete               - Delete transport\n";
    send_client_message(connection_id, help);
}

// Format a single stop line
fn format_stop_line(stop, index, current_index, connection_id) {
    let marker = "";
    if index == current_index {
        marker = " " + olc::c_warn("(current)", connection_id);
    }
    let line = "  [" + (index + 1) + "] " + stop.name;
    line += " - exit: " + stop.exit_direction + marker + "\n";
    let room = get_room_data(stop.room_id);
    if room != () {
        line += "      Room: " + room.title + "\n";
    }
    return line;
}

// Show transport header info
fn show_transport_header(transport, connection_id) {
    let output = olc::c_header("=== Transport Editor ===", connection_id) + "\n";
    output += olc::c_label("Vnum: ", connection_id) + olc::c_value(transport.vnum, connection_id) + "\n";
    output += olc::c_label("Name: ", connection_id) + olc::c_value(transport.name, connection_id) + "\n";
    output += olc::c_label("ID: ", connection_id) + olc::c_value(transport.id, connection_id) + "\n";
    output += olc::c_label("Type: ", connection_id) + olc::c_value(transport.transport_type, connection_id) + "\n";
    return output;
}

// Show transport details
fn show_transport_details(transport, connection_id) {
    let output = olc::c_label("Interior Room: ", connection_id);
    output += olc::c_value(transport.interior_room_id, connection_id) + "\n";

    let interior = get_room_data(transport.interior_room_id);
    if interior != () {
        output += olc::c_label("  (Room: ", connection_id) + interior.title + ")\n";
    }

    output += olc::c_label("Travel Time: ", connection_id);
    output += olc::c_value(transport.travel_time_secs + " seconds", connection_id) + "\n";
    output += olc::c_label("State: ", connection_id);
    output += olc::c_value(transport.state, connection_id) + "\n";
    output += olc::c_label("Current Stop: ", connection_id);
    output += olc::c_value(transport.current_stop_index + 1, connection_id) + "\n";
    return output;
}

// Show schedule info
fn show_schedule_info(transport, connection_id) {
    let schedule_text = "Game-Time scheduled";
    if is_transport_ondemand(transport.id) {
        schedule_text = "On-Demand (elevator mode)";
    }
    return olc::c_label("Schedule: ", connection_id) + olc::c_value(schedule_text, connection_id) + "\n";
}

fn show_transport(transport, connection_id) {
    let output = show_transport_header(transport, connection_id);
    output += show_transport_details(transport, connection_id);
    output += show_schedule_info(transport, connection_id);

    // Stops section
    output += "\n" + olc::c_header("Stops:", connection_id) + "\n";
    let stops = transport.stops;
    if stops.len() == 0 {
        output += "  (none)\n";
    } else {
        for i in 0..stops.len() {
            output += format_stop_line(stops[i], i, transport.current_stop_index, connection_id);
        }
    }

    output += "\n" + olc::c_label("Commands: ", connection_id);
    output += "name, type, interior, traveltime, schedule, stop, connect, disconnect, delete";
    send_client_message(connection_id, output);
}

fn handle_create(parts, char, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: tedit create <vnum>");
        return true;
    }

    let vnum = parts[1];
    let existing = get_transport_by_vnum(vnum);
    if existing != () {
        send_client_message(connection_id, "Transport '" + vnum + "' already exists.");
        return true;
    }

    let transport = new_transport("New Transport", char.current_room_id);
    if transport == () {
        send_client_message(connection_id, "Failed to create transport.");
        return true;
    }

    // Save first so it exists in database, then set vnum
    save_transport_data(transport);
    set_transport_vnum(transport.id, vnum);
    send_client_message(connection_id, "Created transport '" + vnum + "'.");
    return true;
}

fn handle_vnum(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <id> vnum <new_vnum>");
        return true;
    }
    let new_vnum = parts[start + 1];
    let existing = get_transport_by_vnum(new_vnum);
    if existing != () && existing.id != transport.id {
        send_client_message(connection_id, "Vnum '" + new_vnum + "' is already in use.");
        return true;
    }
    set_transport_vnum(transport.id, new_vnum);
    send_client_message(connection_id, "Vnum set to: " + new_vnum);
    return true;
}

fn handle_name(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> name <text>");
        return true;
    }
    let name = "";
    for i in (start + 1)..parts.len() {
        if name != "" { name += " "; }
        name += parts[i];
    }
    set_transport_name(transport.id, name);
    send_client_message(connection_id, "Name set to: " + name);
    return true;
}

fn handle_type(transport, parts, start, connection_id) {
    let types = get_all_transport_types();
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> type <" + types.join("|") + ">");
        return true;
    }
    let type_str = parts[start + 1].to_lower();
    if !set_transport_type(transport.id, type_str) {
        send_client_message(connection_id, "Invalid type. Valid: " + types.join(", "));
        return true;
    }
    send_client_message(connection_id, "Type set to: " + type_str);
    return true;
}

fn handle_interior(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> interior <room_vnum>");
        return true;
    }
    let room = get_room_by_vnum(parts[start + 1]);
    if room == () {
        send_client_message(connection_id, "Room not found: " + parts[start + 1]);
        return true;
    }
    set_transport_interior_room(transport.id, room.id);
    send_client_message(connection_id, "Interior room set to: " + room.title);
    return true;
}

fn handle_traveltime(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> traveltime <seconds>");
        return true;
    }
    let seconds = parse_int(parts[start + 1]);
    if seconds < 1 { seconds = 1; }
    set_transport_travel_time(transport.id, seconds);
    send_client_message(connection_id, "Travel time set to: " + seconds + " seconds");
    return true;
}

fn handle_schedule(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> schedule ondemand");
        send_client_message(connection_id, "       tedit <vnum> schedule gametime <freq> <start> <end> <dwell>");
        return true;
    }
    let stype = parts[start + 1].to_lower();
    if stype == "ondemand" {
        set_transport_schedule_ondemand(transport.id);
        send_client_message(connection_id, "Schedule set to: On-Demand");
        return true;
    }
    if stype == "gametime" {
        return handle_gametime_schedule(transport, parts, start, connection_id);
    }
    send_client_message(connection_id, "Unknown schedule type: " + stype);
    return true;
}

fn handle_gametime_schedule(transport, parts, start, connection_id) {
    if parts.len() <= start + 5 {
        send_client_message(connection_id, "Usage: schedule gametime <freq_hours> <start_hour> <end_hour> <dwell_secs>");
        return true;
    }
    let freq = parse_int(parts[start + 2]);
    let sh = parse_int(parts[start + 3]);
    let eh = parse_int(parts[start + 4]);
    let dwell = parse_int(parts[start + 5]);
    if freq < 1 { freq = 1; }
    if sh < 0 { sh = 0; }
    if sh > 23 { sh = 23; }
    if eh < 0 { eh = 0; }
    if eh > 23 { eh = 23; }
    if dwell < 1 { dwell = 1; }
    set_transport_schedule_gametime(transport.id, freq, sh, eh, dwell);
    send_client_message(connection_id, "Schedule set to: every " + freq + "h, " + sh + ":00-" + eh + ":00");
    return true;
}

fn handle_stop(transport, parts, start, connection_id) {
    if parts.len() <= start + 1 {
        send_client_message(connection_id, "Usage: tedit <vnum> stop add|remove|list");
        return true;
    }
    let action = parts[start + 1].to_lower();
    if action == "list" { return handle_stop_list(transport, connection_id); }
    if action == "add" { return handle_stop_add(transport, parts, start, connection_id); }
    if action == "remove" { return handle_stop_remove(transport, parts, start, connection_id); }
    send_client_message(connection_id, "Unknown action: " + action);
    return true;
}

fn handle_stop_list(transport, connection_id) {
    let stops = transport.stops;
    if stops.len() == 0 {
        send_client_message(connection_id, "No stops configured.");
        return true;
    }
    let output = "Stops:\n";
    for i in 0..stops.len() {
        output += format_stop_line(stops[i], i, transport.current_stop_index, connection_id);
    }
    send_client_message(connection_id, output);
    return true;
}

fn normalize_dir(d) {
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

fn is_valid_dir(d) {
    let dirs = ["north", "south", "east", "west", "up", "down"];
    for dir in dirs {
        if d == dir { return true; }
    }
    return false;
}

fn handle_stop_add(transport, parts, start, connection_id) {
    if parts.len() <= start + 4 {
        send_client_message(connection_id, "Usage: stop add <room_vnum> <name> <exit_dir>");
        return true;
    }
    let room = get_room_by_vnum(parts[start + 2]);
    if room == () {
        send_client_message(connection_id, "Room not found: " + parts[start + 2]);
        return true;
    }
    let name = parts[start + 3];
    let exit_dir = normalize_dir(parts[parts.len() - 1].to_lower());

    // Handle multi-word names
    if parts.len() > start + 5 {
        name = "";
        for i in (start + 3)..(parts.len() - 1) {
            if name != "" { name += " "; }
            name += parts[i];
        }
    }

    if !is_valid_dir(exit_dir) {
        send_client_message(connection_id, "Invalid direction. Use: n/s/e/w/u/d");
        return true;
    }
    add_transport_stop(transport.id, room.id, name, exit_dir);
    send_client_message(connection_id, "Added stop: " + name);
    return true;
}

fn handle_stop_remove(transport, parts, start, connection_id) {
    if parts.len() <= start + 2 {
        send_client_message(connection_id, "Usage: stop remove <index>");
        return true;
    }
    let index = parse_int(parts[start + 2]);
    if index < 1 || index > transport.stops.len() {
        send_client_message(connection_id, "Invalid index.");
        return true;
    }
    let name = transport.stops[index - 1].name;
    remove_transport_stop(transport.id, index - 1);
    send_client_message(connection_id, "Removed stop: " + name);
    return true;
}

fn handle_connect(transport, connection_id) {
    let stops = transport.stops;
    if stops.len() == 0 {
        send_client_message(connection_id, "No stops configured.");
        return true;
    }
    let idx = transport.current_stop_index;
    if idx >= stops.len() { idx = 0; }
    connect_transport_to_stop(transport.id, idx);
    send_client_message(connection_id, "Connected to stop: " + stops[idx].name);
    return true;
}

fn handle_disconnect(transport, connection_id) {
    disconnect_transport_from_current_stop(transport.id);
    send_client_message(connection_id, "Disconnected from current stop.");
    return true;
}

fn handle_delete(transport, connection_id) {
    let vnum = transport.vnum;
    if transport.state == "stopped" {
        disconnect_transport_from_current_stop(transport.id);
    }
    delete_transport(transport.id);
    send_client_message(connection_id, "Deleted transport: " + vnum);
    return true;
}
