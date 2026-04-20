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
