// scripts/lib/olc_helpers.rhai
// Shared helper functions for OLC (Online Creation) editors
// Import in OLC scripts: import "olc_helpers" as olc;

// Color helper functions for consistent OLC display formatting
fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn c_label(text, conn_id) {
    color_or(ansi_cyan(text), text, conn_id)
}

fn c_value(text, conn_id) {
    let s = "" + text;  // Convert to string
    color_or(ansi_white(s), s, conn_id)
}

fn c_warn(text, conn_id) {
    color_or(ansi_yellow(text), text, conn_id)
}

// Join array elements with comma separator
fn join_array(arr) {
    let result = "";
    for item in arr {
        if result != "" { result += ", "; }
        result += item;
    }
    return result;
}

// Helper to parse boolean-like "on" values
fn is_on_value(val) {
    if val == "on" { return true; }
    if val == "true" { return true; }
    if val == "1" { return true; }
    if val == "yes" { return true; }
    return false;
}

// Helper to parse boolean-like "off" values
fn is_off_value(val) {
    if val == "off" { return true; }
    if val == "false" { return true; }
    if val == "0" { return true; }
    if val == "no" { return true; }
    return false;
}

// Helper to parse inherit/clear/default values
fn is_inherit_value(val) {
    if val == "inherit" { return true; }
    if val == "clear" { return true; }
    if val == "default" { return true; }
    return false;
}

// Helper to get area theme from player's current room
fn get_area_theme_for_player(connection_id) {
    let char = get_player_character(connection_id);
    if char == () { return ""; }
    let room = get_room_data(char.current_room_id);
    if room == () { return ""; }
    if room.area_id == "" { return ""; }
    let area = get_area_data(room.area_id);
    if area == () { return ""; }
    return area.theme;
}

// Helper to build a string from command parts starting at index
fn build_string_from_parts(parts, start_idx) {
    let result = "";
    for i in start_idx..parts.len() {
        if result != "" { result += " "; }
        result += parts[i];
    }
    return result;
}

// Reverse spawn lookup: render every spawn point that references `vnum`,
// either DIRECTly (the spawn's own entity) or as a spawn DEPendency
// (carried/equipped/contained by another spawned entity). Grouped by area.
// Read-only: callers use this without an area-edit permission check.
fn format_spawn_locations(vnum, conn_id) {
    let sps = find_spawn_points_for_vnum(vnum);
    if sps.len() == 0 {
        return "No spawn points reference " + vnum + ".";
    }

    // Group by area using parallel arrays: ids, resolved names, body strings.
    let area_ids = [];
    let area_names = [];
    let area_bodies = [];

    for sp in sps {
        let gi = -1;
        for i in 0..area_ids.len() {
            if area_ids[i] == sp.area_id { gi = i; }
        }
        if gi == -1 {
            area_ids.push(sp.area_id);
            let area = get_area_data(sp.area_id);
            let aname = if area != () && area.name != "" { area.name } else { sp.area_id };
            area_names.push(aname);
            area_bodies.push("");
            gi = area_ids.len() - 1;
        }
        area_bodies[gi] += format_one_spawn_location(sp, vnum, conn_id);
    }

    let output = c_header("=== Spawn locations for " + vnum + " ===", conn_id) + "\n\n";
    for i in 0..area_ids.len() {
        output += c_label(area_names[i], conn_id) + "\n";
        output += area_bodies[i];
        output += "\n";
    }
    output
}

// One spawn point may match both directly AND via a dependency; emit a line
// for each match. entity_type getter returns lowercase "mobile"/"item".
fn format_one_spawn_location(sp, vnum, conn_id) {
    let room = get_room_data(sp.room_id);
    let room_name = if room != () && room.title != "" { room.title } else { sp.room_id };

    let out = "";

    // Direct: the spawn point's own entity is the vnum.
    if sp.vnum == vnum {
        let status = "[OFF]";
        if sp.enabled { status = c_warn("[ON]", conn_id); }
        out += "  " + status + " " + c_label("DIRECT", conn_id) + " ";
        out += c_value(room_name, conn_id);
        out += "  " + c_label("Max: ", conn_id) + c_value("" + sp.max_count, conn_id) + "\n";
    }

    // Dependency: the vnum is carried/equipped/contained by this spawn's entity.
    let deps = get_spawn_dependencies(sp.id);
    for dep in deps {
        if dep.item_vnum == vnum {
            let count_str = if dep.count > 1 { " x" + dep.count } else { "" };
            out += "  " + c_label("DEP", conn_id) + "    ";
            out += c_value(room_name, conn_id);
            out += "  " + c_label("in ", conn_id) + c_value(sp.entity_type + ": " + sp.vnum, conn_id);
            out += " (" + dep.destination + count_str + ")\n";
        }
    }

    out
}
