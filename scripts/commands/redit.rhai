// scripts/commands/redit.rhai
// Room Editor - Edit properties of the current room

// Import shared OLC helpers
import "olc_helpers" as olc;

// Helper for cycling flag values (toggle on/off)
fn handle_flag_cycle(room_id, flag_name, connection_id) {
    let current = get_room_flag(room_id, flag_name);
    let new_value = !current;
    if set_room_flag(room_id, flag_name, new_value) {
        let status = if new_value { "ON" } else { "OFF" };
        send_client_message(connection_id, "Flag '" + flag_name + "' set to " + status + ".");
    } else {
        send_client_message(connection_id, "Error: Failed to set flag.");
    }
}

// Helper for flag setting with explicit value
fn handle_flag_explicit(room_id, flag_name, val_str, connection_id) {
    // Check for on value
    if olc::is_on_value(val_str) {
        if set_room_flag(room_id, flag_name, true) {
            send_client_message(connection_id, "Flag '" + flag_name + "' set to ON.");
        } else {
            send_client_message(connection_id, "Error: Failed to set flag.");
        }
        return;
    }

    // Check for off value
    if olc::is_off_value(val_str) {
        if set_room_flag(room_id, flag_name, false) {
            send_client_message(connection_id, "Flag '" + flag_name + "' set to OFF.");
        } else {
            send_client_message(connection_id, "Error: Failed to set flag.");
        }
        return;
    }

    // Invalid value
    send_client_message(connection_id, "Invalid value. Use 'on' or 'off'.");
}

// Helper - set room zone to inherit
fn zone_set_inherit(room_id, connection_id) {
    if !clear_room_combat_zone(room_id) {
        send_client_message(connection_id, "Error: Failed to clear combat zone.");
        return;
    }
    let effective = get_room_combat_zone(room_id);
    let msg = "Combat zone set to INHERIT (uses area default: " + effective + ").";
    send_client_message(connection_id, msg);
}

// Helper - set room zone to explicit value
fn zone_set_explicit(room_id, val, connection_id) {
    if !set_room_combat_zone(room_id, val) {
        send_client_message(connection_id, "Error: Failed to set combat zone.");
        return;
    }
    let desc = "PvP (attack mobiles and players)";
    if val == "pve" { desc = "PvE (attack mobiles only)"; }
    if val == "safe" { desc = "Safe (no combat)"; }
    let msg = "Combat zone set to " + val.to_upper() + ".\n" + desc;
    send_client_message(connection_id, msg);
}

// Helper - get next zone in cycle
fn get_next_zone(current) {
    if current == "inherit" { return "pve"; }
    if current == "pve" { return "safe"; }
    if current == "safe" { return "pvp"; }
    return "inherit";
}

// Helper for zone subcommand handling
fn handle_zone_subcommand(room_id, parts, connection_id) {
    // With explicit value
    if parts.len() >= 2 {
        let val = parts[1].to_lower();
        if val == "inherit" {
            zone_set_inherit(room_id, connection_id);
            return;
        }
        // Check for valid zone types
        let is_valid = false;
        if val == "pve" { is_valid = true; }
        if val == "safe" { is_valid = true; }
        if val == "pvp" { is_valid = true; }
        if is_valid {
            zone_set_explicit(room_id, val, connection_id);
            return;
        }
        // Invalid value
        send_client_message(connection_id, "Invalid zone type. Valid: pve, safe, pvp, inherit");
        return;
    }

    // No value - cycle: inherit -> pve -> safe -> pvp -> inherit
    let current = get_room_combat_zone_raw(room_id);
    let next = get_next_zone(current);
    if next == "inherit" {
        zone_set_inherit(room_id, connection_id);
    } else {
        zone_set_explicit(room_id, next, connection_id);
    }
}

// Usage: redit [subcommand]
// Subcommands:
//   (none) or show  - Display room properties
//   create [title]  - Create a new room (teleports you there)
//   title <text>    - Set room title
//   desc            - Edit description with line editor
//   desc <text>     - Set description directly (shortcut)
//   flags           - Show all room flags
//   flag <name> [on|off] - Toggle or set a room flag
//   zone [pve|safe|pvp|inherit] - Set combat zone type
//   extra list      - List all extra descriptions
//   extra add <keywords> - Add extra description with line editor
//   extra edit <keyword> - Edit existing extra description
//   extra remove <keyword> - Remove extra description by keyword
//   vnum [alias|clear] - Set or clear room vnum
//   area [id|clear]   - Set or clear room area
//   trigger list      - List all triggers on this room
//   trigger add <type> <script> - Add trigger (types below)
//   trigger remove <index> - Remove trigger by index
//   trigger enable <index> - Enable a trigger
//   trigger disable <index> - Disable a trigger
//   trigger interval <index> <secs> - Set periodic interval
//   trigger chance <index> <1-100> - Set trigger fire chance
//   door add <dir> <name>   - Add door to exit
//   door remove <dir>       - Remove door from exit
//   door name/desc/key/keywords/open/close/lock/unlock/sync <dir> ...
//   seasonal <season> <text|clear|help|rephrase> - Seasonal descriptions
//   dynamic <text|clear>    - Set temporary dynamic description
//   water <none|freshwater|saltwater|magical> - Water type for fishing
//   catch add/remove/list   - Manage fishing catch table
//   capacity <n>            - Set living capacity (migrant slots)
//
// Trigger Types: enter, exit, look, periodic, on_time_change,
//                on_weather_change, on_season_change, on_month_change
//
// Line Editor Commands (when in desc/extra editing mode):
//   .l            - List buffer with line numbers
//   .d N          - Delete line N
//   .i N <text>   - Insert text before line N
//   .r N <text>   - Replace line N with text
//   .s /old/new/  - Substitute: replace all 'old' with 'new'
//   .c            - Clear entire buffer
//   .u            - Undo last change
//   .p            - Preview without line numbers
//   .h            - Show help for all commands
//   .             - Save and exit
//   @             - Cancel and exit
//   <text>        - Append text as new line

fn get_room_title_safe(room_id) {
    let room = get_room_data(room_id);
    if room == () {
        return "???";
    }
    return room.title;
}

fn format_keywords(kws) {
    let result = "";
    for kw in kws {
        if result != "" { result += ", "; }
        result += kw;
    }
    return result;
}

fn format_extra_list(extras) {
    if extras.len() == 0 {
        return "  (none)\n";
    }
    let output = "";
    let idx = 1;
    for extra in extras {
        let kw_str = format_keywords(extra.keywords);
        output += idx + ". " + kw_str + "\n";
        idx += 1;
    }
    return output;
}

fn collect_keywords(parts, start_idx) {
    let keywords = [];
    for i in start_idx..parts.len() {
        keywords.push(parts[i].to_lower());
    }
    return keywords;
}

// AI helper: build prompt from parts starting at index
fn build_ai_prompt(parts, start_idx) {
    let prompt = "";
    for i in start_idx..parts.len() {
        if prompt != "" { prompt += " "; }
        prompt += parts[i];
    }
    return prompt;
}

// AI helper: handle desc help command
fn handle_ai_desc_help(room, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit desc help <description prompt>\nExample: redit desc help a dark cave with glowing mushrooms");
        return true;
    }
    let prompt = build_ai_prompt(parts, 2);
    let area_info = get_room_area_info(room.area_id);
    let context = #{
        room_title: room.title,
        area_name: area_info.name,
        theme: area_info.theme
    };
    let request_id = ai_help_write(connection_id, "room_desc", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "room", room.id, "description");
    send_client_message(connection_id, "Generating description... please wait.");
    return true;
}

// AI helper: handle desc rephrase command
fn handle_ai_desc_rephrase(room, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if room.description == "" {
        send_client_message(connection_id, "Room has no description to rephrase.");
        return true;
    }
    let area_info = get_room_area_info(room.area_id);
    let context = #{
        room_title: room.title,
        area_name: area_info.name,
        theme: area_info.theme
    };
    let request_id = ai_rephrase(connection_id, "room_desc", room.description, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "room", room.id, "description");
    send_client_message(connection_id, "Rephrasing description... please wait.");
    return true;
}

// Helper to get area info (name and theme) from area_id
fn get_room_area_info(area_id) {
    if area_id == "" { return #{ name: "", theme: "" }; }
    let area = get_area_data(area_id);
    if area == () { return #{ name: "", theme: "" }; }
    return #{ name: area.name, theme: area.theme };
}

fn format_exit(dir, room_id, connection_id) {
    let title = get_room_title_safe(room_id);
    // Make exit direction clickable with popup menu for unlink/goto
    let dir_link = mxp_or(
        mxp_menu(["unlink " + dir, "rgoto " + room_id], ["Unlink Exit", "Go to Room"], dir),
        dir,
        connection_id
    );
    return "  " + dir_link + " -> " + title + "\n";
}

fn format_line_num(n) {
    if n < 10 {
        return "  " + n + ": ";
    }
    if n < 100 {
        return " " + n + ": ";
    }
    return n + ": ";
}

fn format_buffer_lines(lines) {
    let output = "";
    for i in 0..lines.len() {
        output += format_line_num(i + 1) + lines[i] + "\n";
    }
    return output;
}

fn find_extra_by_keyword(extras, keyword) {
    for extra in extras {
        for kw in extra.keywords {
            if kw.to_lower() == keyword {
                return extra;
            }
        }
    }
    return ();
}

fn format_trigger_type(trigger) {
    let ttype = get_trigger_type(trigger);
    return ttype;
}

fn format_trigger_list(triggers, connection_id) {
    if triggers.len() == 0 {
        return "  (none)\n";
    }
    let output = "";
    let idx = 0;
    for trigger in triggers {
        let ttype = format_trigger_type(trigger);
        let status = if trigger.enabled { "ON" } else { "off" };
        let toggle_cmd = if trigger.enabled { "redit trigger disable " + idx } else { "redit trigger enable " + idx };
        let status_link = mxp_or(mxp_send(toggle_cmd, "[" + status + "]"), status, connection_id);

        output += "  " + idx + ". " + ttype + " -> " + trigger.script_name;
        output += " " + status_link;

        if ttype == "periodic" {
            output += " (every " + trigger.interval_secs + "s)";
        }
        if trigger.chance < 100 {
            output += " (" + trigger.chance + "% chance)";
        }
        output += "\n";
        idx += 1;
    }
    return output;
}

fn handle_trigger_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show trigger list
        let triggers = get_room_triggers(room.id);
        let output = olc::c_header("=== Room Triggers ===", connection_id) + "\n";
        output += format_trigger_list(triggers, connection_id);
        output += "\nCommands:\n";
        output += "  redit trigger add <type> <script> - Add trigger\n";
        output += "    Types: enter, exit, look, periodic, on_time_change, on_weather_change, on_season_change, on_month_change\n";
        output += "  redit trigger remove <index> - Remove trigger\n";
        output += "  redit trigger enable/disable <index> - Toggle\n";
        output += "  redit trigger interval <index> <secs> - Set interval\n";
        output += "  redit trigger chance <index> <1-100> - Set chance\n";
        output += "  redit trigger test <index> - Test trigger\n";
        output += "  redit trigger view <script_name> - View script source\n";
        output += "\nAvailable scripts: " + list_available_trigger_scripts().join(", ");
        send_mxp_message(connection_id, output);
        return;
    }

    let trigger_cmd = parts[1].to_lower();

    if trigger_cmd == "list" {
        let triggers = get_room_triggers(room.id);
        let output = olc::c_header("=== Room Triggers ===", connection_id) + "\n";
        output += format_trigger_list(triggers, connection_id);
        send_mxp_message(connection_id, output);
        return;
    }

    if trigger_cmd == "add" {
        if parts.len() < 4 {
            let help = "Usage: redit trigger add <type> <script> [args...]\n";
            help += "Types: enter, exit, look, periodic, on_time_change, on_weather_change, on_season_change, on_month_change\n";
            help += "Templates: @room_message, @time_message, @weather_message, @season_message\n\n";
            help += "Examples:\n  redit trigger add enter trapped_room\n";
            help += "  redit trigger add on_time_change @room_message \"Torches light up.\"";
            send_client_message(connection_id, help);
            return;
        }
        let ttype = parts[2].to_lower();
        let script_name = parts[3];

        // Validate type
        let valid_types = ["enter", "exit", "look", "periodic", "on_enter", "on_exit", "on_look", "on_time_change", "time_change", "on_weather_change", "weather_change", "on_season_change", "season_change", "on_month_change", "month_change"];
        let is_valid = false;
        for vt in valid_types {
            if vt == ttype {
                is_valid = true;
                break;
            }
        }
        if !is_valid {
            send_client_message(connection_id, "Invalid trigger type: " + ttype + "\nValid types: enter, exit, look, periodic, on_time_change, on_weather_change, on_season_change, on_month_change");
            return;
        }

        // Validate script before adding
        let validation = validate_trigger_script(script_name);
        if !validation.valid {
            send_client_message(connection_id, olc::c_warn("Script validation failed: ", connection_id) + validation.error);
            send_client_message(connection_id, "Available scripts: " + list_available_trigger_scripts().join(", "));
            return;
        }

        // Add trigger (with optional args starting at parts[4])
        let success = add_room_trigger_with_args_from(room.id, ttype, script_name, parts, 4);
        if success {
            send_client_message(connection_id, "Trigger added: " + ttype + " -> " + script_name);
        } else {
            send_client_message(connection_id, "Error: Failed to add trigger.");
        }
        return;
    }

    if trigger_cmd == "remove" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: redit trigger remove <index>");
            return;
        }
        let idx = parse_int(parts[2]);
        if idx < 0 {
            send_client_message(connection_id, "Invalid index.");
            return;
        }
        if remove_room_trigger(room.id, idx) {
            send_client_message(connection_id, "Trigger " + idx + " removed.");
        } else {
            send_client_message(connection_id, "Error: Failed to remove trigger (invalid index?).");
        }
        return;
    }

    if trigger_cmd == "enable" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: redit trigger enable <index>");
            return;
        }
        let idx = parse_int(parts[2]);
        if set_trigger_enabled(room.id, idx, true) {
            send_client_message(connection_id, "Trigger " + idx + " enabled.");
        } else {
            send_client_message(connection_id, "Error: Invalid trigger index.");
        }
        return;
    }

    if trigger_cmd == "disable" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: redit trigger disable <index>");
            return;
        }
        let idx = parse_int(parts[2]);
        if set_trigger_enabled(room.id, idx, false) {
            send_client_message(connection_id, "Trigger " + idx + " disabled.");
        } else {
            send_client_message(connection_id, "Error: Invalid trigger index.");
        }
        return;
    }

    if trigger_cmd == "interval" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: redit trigger interval <index> <seconds>");
            return;
        }
        let idx = parse_int(parts[2]);
        let secs = parse_int(parts[3]);
        if secs < 1 {
            send_client_message(connection_id, "Interval must be at least 1 second.");
            return;
        }
        if set_trigger_interval(room.id, idx, secs) {
            send_client_message(connection_id, "Trigger " + idx + " interval set to " + secs + " seconds.");
        } else {
            send_client_message(connection_id, "Error: Invalid trigger index.");
        }
        return;
    }

    if trigger_cmd == "chance" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: redit trigger chance <index> <1-100>");
            return;
        }
        let idx = parse_int(parts[2]);
        let chance = parse_int(parts[3]);
        if chance < 1 || chance > 100 {
            send_client_message(connection_id, "Chance must be between 1 and 100.");
            return;
        }
        if set_trigger_chance(room.id, idx, chance) {
            send_client_message(connection_id, "Trigger " + idx + " chance set to " + chance + "%.");
        } else {
            send_client_message(connection_id, "Error: Invalid trigger index.");
        }
        return;
    }

    if trigger_cmd == "test" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: redit trigger test <index>");
            return;
        }
        let idx = parse_int(parts[2]);
        if idx == () {
            send_client_message(connection_id, "Invalid index.");
            return;
        }

        send_client_message(connection_id, "Testing trigger " + idx + "...");

        let result = test_room_trigger(room.id, idx, connection_id);

        if result.success {
            send_client_message(connection_id, olc::c_header("Trigger executed successfully!", connection_id));
            send_client_message(connection_id, "Result: " + result.result);
        } else {
            send_client_message(connection_id, olc::c_warn("Trigger test failed: ", connection_id) + result.error);
        }
        return;
    }

    if trigger_cmd == "view" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: redit trigger view <script_name>\nAvailable: " + list_available_trigger_scripts().join(", "));
            return;
        }
        let script_name = parts[2];
        let result = get_trigger_script_content(script_name);
        if result.success {
            let output = olc::c_header("=== Script: " + script_name + ".rhai ===", connection_id) + "\n";
            output += result.content;
            send_client_message(connection_id, output);
        } else {
            send_client_message(connection_id, olc::c_warn("Error: ", connection_id) + result.error);
            send_client_message(connection_id, "Available scripts: " + list_available_trigger_scripts().join(", "));
        }
        return;
    }

    send_client_message(connection_id, "Unknown trigger subcommand: " + trigger_cmd + "\nUse 'redit trigger' for help.");
}

// Helper: Set description directly from command line
fn handle_direct_desc_set(room, parts, connection_id) {
    let desc_parts = [];
    for i in 1..parts.len() {
        desc_parts.push(parts[i]);
    }
    let new_desc = desc_parts.reduce(|a, b| a + " " + b);
    if set_room_description(room.id, new_desc) {
        send_client_message(connection_id, "Description set to: " + new_desc);
    } else {
        send_client_message(connection_id, "Error: Failed to set description.");
    }
}

// Helper: Enter the multi-line description editor
fn enter_desc_editor(room, connection_id) {
    set_olc_mode(connection_id, "collecting_desc");
    set_olc_edit_room(connection_id, room.id);
    let existing_desc = room.description;
    let lines = if existing_desc != "" { existing_desc.split("\n") } else { [] };
    set_olc_buffer(connection_id, lines);

    let output = "Editing description for: " + room.title + "\n";
    output += if lines.len() == 0 { "(empty)\n" } else { format_buffer_lines(lines) };
    output += "\nCommands: .h help | .l list | .s /old/new/ | . save | @ cancel\n";
    output += "Or type text to append new lines.\n";
    output += "AI: 'redit desc help <prompt>' or 'redit desc rephrase'\n";
    output += "Shortcut: 'redit desc <text>' to set description directly";

    send_client_message(connection_id, output);
}

fn handle_area_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show current area
        let area_id = room.area_id;
        if area_id == "" {
            let msg = "This room is not assigned to any area.\n";
            msg += "Usage: redit area <area_id> or redit area clear";
            send_client_message(connection_id, msg);
        } else {
            let area = get_area_data(area_id);
            let msg = "";
            if area != () {
                msg = "Area: " + area.name;
                msg += " [" + area.prefix + "]\n";
            } else {
                msg = "Area ID: " + area_id;
                msg += " (not found)\n";
            }
            msg += "Usage: redit area <area_id> or redit area clear";
            send_client_message(connection_id, msg);
        }
        return;
    }

    let area_arg = parts[1].to_lower();

    if area_arg == "clear" {
        if clear_room_area(room.id) {
            send_client_message(connection_id, "Room removed from area.");
        } else {
            send_client_message(connection_id, "This room is not assigned to any area.");
        }
        return;
    }

    // Set area - try UUID first, then prefix
    let area = get_area_data(parts[1]);
    if area == () {
        area = get_area_by_prefix(parts[1]);
    }
    if area == () {
        send_client_message(connection_id, "Area not found: " + parts[1] + "\nUse area UUID or prefix. See 'alist' for available areas.");
        return;
    }

    if set_room_area(room.id, area.id) {
        send_client_message(connection_id, "Room assigned to area: " + area.name);
    } else {
        send_client_message(connection_id, "Error: Failed to assign room to area.");
    }
}

fn handle_vnum_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show current vnum
        let current_vnum = room.vnum;
        if current_vnum == "" {
            let msg = "This room has no vnum.\n";
            msg += "Usage: redit vnum <alias> or redit vnum clear";
            send_client_message(connection_id, msg);
        } else {
            let msg = "Vnum: " + current_vnum + "\n";
            msg += "Usage: redit vnum <alias> or redit vnum clear";
            send_client_message(connection_id, msg);
        }
        return;
    }

    let vnum_arg = parts[1].to_lower();

    if vnum_arg == "clear" {
        if clear_room_vnum(room.id) {
            send_client_message(connection_id, "Vnum cleared from this room.");
        } else {
            send_client_message(connection_id, "This room has no vnum to clear.");
        }
        return;
    }

    // Set new vnum
    if set_room_vnum(room.id, vnum_arg) {
        send_client_message(connection_id, "Vnum set to: " + vnum_arg);
    } else {
        send_client_message(connection_id, "Error: That vnum is already in use by another room.");
    }
}

// Helper to inherit area settings from current room
fn inherit_area_for_new_room(char, new_room_id, title) {
    let current_room = get_room_data(char.current_room_id);
    if current_room == () { return #{ vnum: "", area_id: "" }; }

    let room_area_id = current_room.area_id;
    if room_area_id == "" { return #{ vnum: "", area_id: "" }; }

    set_room_area(new_room_id, room_area_id);
    apply_area_default_room_flags(new_room_id);

    // Only auto-generate vnum if a custom title was provided (not default)
    if title == "A New Room" { return #{ vnum: "", area_id: room_area_id }; }

    let area = get_area_data(room_area_id);
    if area == () { return #{ vnum: "", area_id: room_area_id }; }

    let inherited_vnum = generate_unique_vnum(area.prefix, title);
    set_room_vnum(new_room_id, inherited_vnum);
    return #{ vnum: inherited_vnum, area_id: room_area_id };
}

// Handle creating a new detached room
fn handle_create_room(char, parts, connection_id) {
    // redit create [title]
    let title = "A New Room";
    if parts.len() >= 2 {
        let title_parts = [];
        for i in 1..parts.len() {
            title_parts.push(parts[i]);
        }
        title = title_parts.reduce(|a, b| a + " " + b);
    }

    let new_room = create_room(title, "This room has no description yet.");
    let new_room_id = new_room.id;

    // Inherit area from current room and auto-generate vnum (like dig does)
    let inherited = inherit_area_for_new_room(char, new_room_id, title);

    let output = olc::c_header("=== Created Detached Room ===", connection_id) + "\n";
    output += olc::c_label("Title: ", connection_id) + olc::c_value(title, connection_id) + "\n";
    output += olc::c_label("ID: ", connection_id) + olc::c_value(new_room_id, connection_id) + "\n";
    if inherited.vnum != "" {
        output += olc::c_label("Vnum: ", connection_id) + olc::c_value(inherited.vnum, connection_id) + "\n";
    }
    if inherited.area_id != "" {
        let area = get_area_data(inherited.area_id);
        if area != () {
            output += olc::c_label("Area: ", connection_id) + olc::c_value(area.name, connection_id);
            output += " [" + area.prefix + "]\n";
        }
    }
    output += "\nThis room has no exits. Use 'rgoto " + new_room_id + "' to visit it.";
    output += "\nOr use 'link' to connect it to other rooms.";

    send_client_message(connection_id, output);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to edit rooms.");
        return;
    }

    // Handle create before getting current room
    // Use split_quoted_args to preserve quoted strings as single arguments
    let parts = split_quoted_args(args);
    if parts.len() >= 1 && parts[0].to_lower() == "create" {
        handle_create_room(char, parts, connection_id);
        return;
    }

    // Get current room
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check area permissions if room belongs to an area
    let room_area_id = room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to edit rooms in this area.");
            return;
        }
    }

    // Parse subcommand
    // Use split_quoted_args to preserve quoted strings as single arguments
    let parts = split_quoted_args(args);
    let subcommand = "";
    if parts.len() >= 1 {
        subcommand = parts[0].to_lower();
    }

    // Handle subcommands
    if subcommand == "" || subcommand == "show" {
        // Display room properties
        let output = olc::c_header("=== Room Editor ===", connection_id) + "\n";
        output += olc::c_label("Title: ", connection_id) + olc::c_value(room.title, connection_id) + "\n";
        output += olc::c_label("ID: ", connection_id) + olc::c_value(room.id, connection_id) + "\n";
        let vnum_display = room.vnum;
        if vnum_display == "" {
            vnum_display = "(none)";
        }
        output += olc::c_label("Vnum: ", connection_id) + olc::c_value(vnum_display, connection_id) + "\n";
        output += olc::c_label("Description:", connection_id) + "\n  " + room.description + "\n";

        let exits = room.exits;
        output += "\n" + olc::c_label("Exits:", connection_id) + "\n";
        let has_exits = false;

        if exits.north != "" {
            output += format_exit("north", exits.north, connection_id);
            has_exits = true;
        }
        if exits.south != "" {
            output += format_exit("south", exits.south, connection_id);
            has_exits = true;
        }
        if exits.east != "" {
            output += format_exit("east", exits.east, connection_id);
            has_exits = true;
        }
        if exits.west != "" {
            output += format_exit("west", exits.west, connection_id);
            has_exits = true;
        }
        if exits.up != "" {
            output += format_exit("up", exits.up, connection_id);
            has_exits = true;
        }
        if exits.down != "" {
            output += format_exit("down", exits.down, connection_id);
            has_exits = true;
        }

        if !has_exits {
            output += "  (none)\n";
        }

        // Display combat zone
        let zone_raw = get_room_combat_zone_raw(room.id);
        let zone_effective = get_room_combat_zone(room.id);
        let zone_display = if zone_raw == "inherit" { zone_effective + " (inherit)" } else { zone_raw };
        output += "\n" + olc::c_label("Combat Zone: ", connection_id) + olc::c_value("[" + zone_display.to_upper() + "]", connection_id) + "\n";

        // Display flags
        let flags = room.flags;
        output += olc::c_label("Flags:", connection_id) + "\n";
        let flag_str = "";
        if flags.dark { flag_str += "dark "; }
        if flags.no_mob { flag_str += "no_mob "; }
        if flags.indoors { flag_str += "indoors "; }
        if flags.underwater { flag_str += "underwater "; }
        if flags.city { flag_str += "city "; }
        if flags.no_windows { flag_str += "no_windows "; }
        if flags.difficult_terrain { flag_str += "difficult_terrain "; }
        if flags.dirt_floor { flag_str += "dirt_floor "; }
        if flags.post_office { flag_str += "post_office "; }
        if flags.bank { flag_str += "bank "; }
        if flags.garden { flag_str += "garden "; }
        if flags.spawn_point { flag_str += "spawn_point "; }
        if flags.shallow_water { flag_str += "shallow_water "; }
        if flags.deep_water { flag_str += "deep_water "; }
        if flags.liveable { flag_str += "liveable "; }
        if flags.private_room { flag_str += "private "; }
        if flags.tunnel { flag_str += "tunnel "; }
        if flags.death { flag_str += "death "; }
        if flags.no_magic { flag_str += "no_magic "; }

        if flag_str == "" {
            output += "  (none)\n";
        } else {
            output += "  " + flag_str + "\n";
        }

        // Display residents if this is a liveable room (or has stale residents)
        let resident_count = get_room_resident_count(room.id);
        if flags.liveable || resident_count > 0 {
            let capacity = get_room_living_capacity(room.id);
            output += "\n" + olc::c_label("Residents: ", connection_id);
            output += olc::c_value("" + resident_count + "/" + capacity, connection_id) + "\n";
            if resident_count > 0 {
                let resident_names = get_room_resident_names(room.id);
                for name in resident_names {
                    output += "  " + name + "\n";
                }
            } else {
                output += "  (none)\n";
            }
        }

        // Display extra descriptions count
        let extras = room.extra_descs;
        output += "\n" + olc::c_label("Extra Descriptions: ", connection_id) + olc::c_value("" + extras.len(), connection_id) + "\n";

        // Display area
        let area_id = room.area_id;
        if area_id != "" {
            let area = get_area_data(area_id);
            if area != () {
                output += olc::c_label("Area: ", connection_id) + olc::c_value(area.name, connection_id);
                output += " [" + area.prefix + "]\n";
            } else {
                output += olc::c_label("Area: ", connection_id) + olc::c_value(area_id, connection_id);
                output += olc::c_warn(" (not found)", connection_id) + "\n";
            }
        } else {
            output += olc::c_label("Area: ", connection_id) + "(none)\n";
        }

        // Display water type for fishing
        let water_type = get_room_water_type(room.id);
        output += olc::c_label("Water: ", connection_id);
        if water_type == "none" {
            output += "(none)\n";
        } else {
            output += olc::c_value(water_type, connection_id);
            // Show catch table count
            let catch_table = get_catch_table(room.id);
            output += " (" + catch_table.len() + " catchable items)\n";
        }

        output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
        // Make commands clickable with MXP
        let cmd_desc = mxp_or(mxp_send("redit desc", "redit desc"), "redit desc", connection_id);
        let cmd_flags = mxp_or(mxp_send("redit flags", "redit flags"), "redit flags", connection_id);
        let cmd_extra = mxp_or(mxp_send("redit extra list", "redit extra list"), "redit extra list", connection_id);
        let cmd_vnum = mxp_or(mxp_send("redit vnum", "redit vnum"), "redit vnum", connection_id);
        let cmd_area = mxp_or(mxp_send("redit area", "redit area"), "redit area", connection_id);

        output += "  redit title <text>  - Set room title\n";
        output += "  " + cmd_desc + "          - Edit description (AI: help <prompt> | rephrase)\n";
        output += "  " + cmd_flags + "         - Show all flags with status\n";
        output += "  redit flag <name> [on|off] - Toggle/set flag\n";
        output += "  " + cmd_extra + "    - List extra descriptions\n";
        output += "  redit extra add <keywords> - Add extra description\n";
        output += "  redit extra edit <keyword> - Edit extra description\n";
        output += "  redit extra remove <keyword> - Remove extra description\n";
        output += "  " + cmd_vnum + " [alias|clear] - Set or clear room vnum\n";
        output += "  " + cmd_area + " [id|clear] - Set or clear room area\n";
        let cmd_trigger = mxp_or(mxp_send("redit trigger", "redit trigger"), "redit trigger", connection_id);
        output += "  " + cmd_trigger + " - Manage room triggers\n";
        let cmd_door = mxp_or(mxp_send("redit door", "redit door"), "redit door", connection_id);
        output += "  " + cmd_door + "    - Manage doors on exits\n";
        let cmd_seasonal = mxp_or(mxp_send("redit seasonal", "redit seasonal"), "redit seasonal", connection_id);
        output += "  " + cmd_seasonal + " - Seasonal descriptions (auto-display by season)\n";
        let cmd_dynamic = mxp_or(mxp_send("redit dynamic", "redit dynamic"), "redit dynamic", connection_id);
        output += "  " + cmd_dynamic + "  - Dynamic description (trigger-set content)\n";
        let cmd_water = mxp_or(mxp_send("redit water", "redit water"), "redit water", connection_id);
        output += "  " + cmd_water + "    - Configure water type for fishing\n";
        let cmd_catch = mxp_or(mxp_send("redit catch", "redit catch"), "redit catch", connection_id);
        output += "  " + cmd_catch + "    - Configure catch table for fishing\n";
        let cmd_capacity = mxp_or(mxp_send("redit capacity", "redit capacity"), "redit capacity", connection_id);
        output += "  " + cmd_capacity + " <n> - Set living capacity (migrant slots; requires liveable flag)";

        send_mxp_message(connection_id, output);
        return;
    }

    if subcommand == "title" {
        // Set room title
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: redit title <new title>");
            return;
        }

        // Join remaining parts as title
        let title_parts = [];
        for i in 1..parts.len() {
            title_parts.push(parts[i]);
        }
        let new_title = title_parts.reduce(|a, b| a + " " + b);

        if set_room_title(room.id, new_title) {
            send_client_message(connection_id, "Title updated to: " + new_title);
        } else {
            send_client_message(connection_id, "Error: Failed to update title.");
        }
        return;
    }

    if subcommand == "desc" {
        // Check for AI or direct text subcommands first
        if parts.len() >= 2 {
            let desc_subcmd = parts[1].to_lower();
            if desc_subcmd == "help" {
                handle_ai_desc_help(room, parts, connection_id);
                return;
            }
            if desc_subcmd == "rephrase" {
                handle_ai_desc_rephrase(room, connection_id);
                return;
            }
            // Direct description set - treat remaining args as the description
            handle_direct_desc_set(room, parts, connection_id);
            return;
        }

        // Default: Enter multi-line description mode with line editor
        enter_desc_editor(room, connection_id);
        return;
    }

    if subcommand == "flags" {
        // Show detailed flags status with clickable toggles
        let flags = room.flags;
        let output = olc::c_header("=== Room Flags ===", connection_id) + "\n";

        // Combat zone display
        let zone_raw = get_room_combat_zone_raw(room.id);
        let zone_effective = get_room_combat_zone(room.id);
        let zone_text = if zone_raw == "inherit" { "[" + zone_effective + " (inherit)]" } else { "[" + zone_raw + "]" };
        let zone_toggle = mxp_or(mxp_send("redit zone", zone_text), zone_text, connection_id);

        // Create clickable flag toggles (simplified to avoid Rhai complexity limits)
        let dark_text = if flags.dark { "[ON]" } else { "[off]" };
        let dark_plain = if flags.dark { "ON" } else { "off" };
        let dark_toggle = mxp_or(mxp_send("redit flag dark", dark_text), dark_plain, connection_id);

        let nomob_text = if flags.no_mob { "[ON]" } else { "[off]" };
        let nomob_plain = if flags.no_mob { "ON" } else { "off" };
        let nomob_toggle = mxp_or(mxp_send("redit flag no_mob", nomob_text), nomob_plain, connection_id);

        let indoors_text = if flags.indoors { "[ON]" } else { "[off]" };
        let indoors_plain = if flags.indoors { "ON" } else { "off" };
        let indoors_toggle = mxp_or(mxp_send("redit flag indoors", indoors_text), indoors_plain, connection_id);

        let underwater_text = if flags.underwater { "[ON]" } else { "[off]" };
        let underwater_plain = if flags.underwater { "ON" } else { "off" };
        let underwater_toggle = mxp_or(mxp_send("redit flag underwater", underwater_text), underwater_plain, connection_id);

        let city_text = if flags.city { "[ON]" } else { "[off]" };
        let city_plain = if flags.city { "ON" } else { "off" };
        let city_toggle = mxp_or(mxp_send("redit flag city", city_text), city_plain, connection_id);

        let nowindows_text = if flags.no_windows { "[ON]" } else { "[off]" };
        let nowindows_plain = if flags.no_windows { "ON" } else { "off" };
        let nowindows_toggle = mxp_or(mxp_send("redit flag no_windows", nowindows_text), nowindows_plain, connection_id);

        let difficult_text = if flags.difficult_terrain { "[ON]" } else { "[off]" };
        let difficult_plain = if flags.difficult_terrain { "ON" } else { "off" };
        let difficult_toggle = mxp_or(mxp_send("redit flag difficult_terrain", difficult_text), difficult_plain, connection_id);

        let dirt_floor_text = if flags.dirt_floor { "[ON]" } else { "[off]" };
        let dirt_floor_plain = if flags.dirt_floor { "ON" } else { "off" };
        let dirt_floor_toggle = mxp_or(mxp_send("redit flag dirt_floor", dirt_floor_text), dirt_floor_plain, connection_id);

        let post_office_text = if flags.post_office { "[ON]" } else { "[off]" };
        let post_office_plain = if flags.post_office { "ON" } else { "off" };
        let post_office_toggle = mxp_or(mxp_send("redit flag post_office", post_office_text), post_office_plain, connection_id);

        let bank_text = if flags.bank { "[ON]" } else { "[off]" };
        let bank_plain = if flags.bank { "ON" } else { "off" };
        let bank_toggle = mxp_or(mxp_send("redit flag bank", bank_text), bank_plain, connection_id);

        let garden_text = if flags.garden { "[ON]" } else { "[off]" };
        let garden_plain = if flags.garden { "ON" } else { "off" };
        let garden_toggle = mxp_or(mxp_send("redit flag garden", garden_text), garden_plain, connection_id);

        let spawn_point_text = if flags.spawn_point { "[ON]" } else { "[off]" };
        let spawn_point_plain = if flags.spawn_point { "ON" } else { "off" };
        let spawn_point_toggle = mxp_or(mxp_send("redit flag spawn_point", spawn_point_text), spawn_point_plain, connection_id);

        let shallow_water_text = if flags.shallow_water { "[ON]" } else { "[off]" };
        let shallow_water_plain = if flags.shallow_water { "ON" } else { "off" };
        let shallow_water_toggle = mxp_or(mxp_send("redit flag shallow_water", shallow_water_text), shallow_water_plain, connection_id);

        let deep_water_text = if flags.deep_water { "[ON]" } else { "[off]" };
        let deep_water_plain = if flags.deep_water { "ON" } else { "off" };
        let deep_water_toggle = mxp_or(mxp_send("redit flag deep_water", deep_water_text), deep_water_plain, connection_id);

        let liveable_text = if flags.liveable { "[ON]" } else { "[off]" };
        let liveable_plain = if flags.liveable { "ON" } else { "off" };
        let liveable_toggle = mxp_or(mxp_send("redit flag liveable", liveable_text), liveable_plain, connection_id);

        let private_text = if flags.private_room { "[ON]" } else { "[off]" };
        let private_plain = if flags.private_room { "ON" } else { "off" };
        let private_toggle = mxp_or(mxp_send("redit flag private", private_text), private_plain, connection_id);

        let tunnel_text = if flags.tunnel { "[ON]" } else { "[off]" };
        let tunnel_plain = if flags.tunnel { "ON" } else { "off" };
        let tunnel_toggle = mxp_or(mxp_send("redit flag tunnel", tunnel_text), tunnel_plain, connection_id);

        let death_text = if flags.death { "[ON]" } else { "[off]" };
        let death_plain = if flags.death { "ON" } else { "off" };
        let death_toggle = mxp_or(mxp_send("redit flag death", death_text), death_plain, connection_id);

        let no_magic_text = if flags.no_magic { "[ON]" } else { "[off]" };
        let no_magic_plain = if flags.no_magic { "ON" } else { "off" };
        let no_magic_toggle = mxp_or(mxp_send("redit flag no_magic", no_magic_text), no_magic_plain, connection_id);

        output += "Combat Zone:         " + zone_toggle + "  (cycles: inherit->pve->safe->pvp)\n\n";
        output += "  dark:              " + dark_toggle + "  (room is too dark to see without light)\n";
        output += "  no_mob:            " + nomob_toggle + "  (NPCs cannot enter)\n";
        output += "  indoors:           " + indoors_toggle + "  (weather does not affect room)\n";
        output += "  underwater:        " + underwater_toggle + "  (requires water breathing)\n";
        output += "  shallow_water:     " + shallow_water_toggle + "  (surface water - extra stamina cost)\n";
        output += "  deep_water:        " + deep_water_toggle + "  (requires boat or swimming 5+)\n";
        output += "  city:              " + city_toggle + "  (stays lit at night - city streets)\n";
        output += "  no_windows:        " + nowindows_toggle + "  (no day/night messages - caves, deep buildings)\n";
        output += "  difficult_terrain: " + difficult_toggle + "  (costs 2 stamina to traverse)\n";
        output += "  dirt_floor:        " + dirt_floor_toggle + "  (allows wilderness foraging)\n";
        output += "  post_office:       " + post_office_toggle + "  (allows sending mail from this room)\n";
        output += "  bank:              " + bank_toggle + "  (allows banking commands here)\n";
        output += "  garden:            " + garden_toggle + "  (thematic garden flag for look output)\n";
        output += "  spawn_point:       " + spawn_point_toggle + "  (players can bind spawn here - inns, safe rooms)\n";
        output += "  liveable:          " + liveable_toggle + "  (migrants can reside here)\n";
        output += "  private:           " + private_toggle + "  (caps player occupancy at 2 - inn rooms)\n";
        output += "  tunnel:            " + tunnel_toggle + "  (caps player occupancy at 1 - chokepoints)\n";
        output += "  death:             " + death_toggle + "  (instant-kill on entry - death traps)\n";
        output += "  no_magic:          " + no_magic_toggle + "  (suppresses spellcasting from this room)\n";
        output += "\nClick a flag to toggle, or use 'redit flag <name> [on|off]'.";
        output += "\nUse 'redit zone [pve|safe|pvp|inherit]' for combat zone.";

        send_mxp_message(connection_id, output);
        return;
    }

    if subcommand == "zone" {
        handle_zone_subcommand(room.id, parts, connection_id);
        return;
    }

    if subcommand == "flag" {
        // Set or toggle one or more room flags
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: redit flag <name> [<name>...] [on|off]\nValid flags: dark, no_mob, indoors, underwater, shallow_water, deep_water, city, no_windows, difficult_terrain, dirt_floor, post_office, bank, garden, spawn_point, liveable");
            return;
        }

        let valid_flags = ["dark", "no_mob", "nomob", "indoors", "underwater", "shallow_water", "deep_water", "city", "no_windows", "nowindows", "difficult_terrain", "dirt_floor", "post_office", "bank", "garden", "spawn_point", "liveable", "livable", "private", "private_room", "tunnel", "death", "no_magic", "nomagic"];

        // Split into flag names and optional trailing on/off value.
        let flag_names = [];
        for i in 1..parts.len() { flag_names.push(parts[i].to_lower()); }
        let explicit_val = ();
        if flag_names.len() >= 2 {
            let last = flag_names[flag_names.len() - 1];
            if olc::is_on_value(last) {
                explicit_val = true;
                flag_names.pop();
            } else if olc::is_off_value(last) {
                explicit_val = false;
                flag_names.pop();
            }
        }

        // Validate every name before toggling anything.
        for name in flag_names {
            let is_valid = false;
            for f in valid_flags {
                if f == name { is_valid = true; break; }
            }
            if !is_valid {
                send_client_message(connection_id, "Unknown flag: " + name + "\nValid flags: dark, no_mob, indoors, underwater, shallow_water, deep_water, city, no_windows, difficult_terrain, dirt_floor, post_office, bank, garden, spawn_point, liveable\nFor combat zone, use 'redit zone [pve|safe|pvp|inherit]'");
                return;
            }
        }

        for name in flag_names {
            if explicit_val == () {
                handle_flag_cycle(room.id, name, connection_id);
            } else if explicit_val {
                handle_flag_explicit(room.id, name, "on", connection_id);
            } else {
                handle_flag_explicit(room.id, name, "off", connection_id);
            }
        }
        return;
    }

    if subcommand == "extra" {
        // Handle extra description subcommands
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: redit extra <list|add|edit|remove> [args]");
            return;
        }

        let extra_cmd = parts[1].to_lower();

        if extra_cmd == "list" {
            let output = olc::c_header("=== Extra Descriptions ===", connection_id) + "\n";
            output += format_extra_list(room.extra_descs);
            output += "\nUse 'look <keyword>' to view, 'redit extra edit <keyword>' to edit.";
            send_client_message(connection_id, output);
            return;
        }

        if extra_cmd == "add" {
            if parts.len() < 3 {
                send_client_message(connection_id, "Usage: redit extra add <keyword1> [keyword2] ...");
                return;
            }
            let keywords = collect_keywords(parts, 2);
            set_olc_mode(connection_id, "collecting_extra_desc");
            set_olc_edit_room(connection_id, room.id);
            clear_olc_buffer(connection_id);
            set_olc_extra_keywords(connection_id, keywords);
            let kw_str = format_keywords(keywords);
            let output = "Adding extra description for: " + kw_str + "\n";
            output += "(empty)\n";
            output += "\nCommands: .h help | .l list | .s /old/new/ | . save | @ cancel\n";
            output += "Or type text to append new lines.";
            send_client_message(connection_id, output);
            return;
        }

        if extra_cmd == "edit" {
            if parts.len() < 3 {
                send_client_message(connection_id, "Usage: redit extra edit <keyword>");
                return;
            }
            let keyword = parts[2].to_lower();

            // Find the extra description with this keyword
            let found_extra = find_extra_by_keyword(room.extra_descs, keyword);

            if found_extra == () {
                send_client_message(connection_id, "No extra description found for: " + keyword);
                return;
            }

            let found_keywords = found_extra.keywords;

            // Remove the old extra description (we'll save the new one on completion)
            remove_room_extra_desc(room.id, keyword);

            // Pre-populate buffer with existing description
            let existing_desc = found_extra.description;
            let lines = [];
            if existing_desc != "" {
                lines = existing_desc.split("\n");
            }

            set_olc_mode(connection_id, "collecting_extra_desc");
            set_olc_edit_room(connection_id, room.id);
            set_olc_buffer(connection_id, lines);
            set_olc_extra_keywords(connection_id, found_keywords);

            let kw_str = format_keywords(found_keywords);
            let output = "Editing extra description for: " + kw_str + "\n";

            // Show current content with line numbers
            if lines.len() == 0 {
                output += "(empty)\n";
            } else {
                output += format_buffer_lines(lines);
            }

            output += "\nCommands: .h help | .l list | .s /old/new/ | . save | @ cancel\n";
            output += "Or type text to append new lines.";
            send_client_message(connection_id, output);
            return;
        }

        if extra_cmd == "remove" {
            if parts.len() < 3 {
                send_client_message(connection_id, "Usage: redit extra remove <keyword>");
                return;
            }
            let keyword = parts[2].to_lower();
            let removed = remove_room_extra_desc(room.id, keyword);
            if removed {
                send_client_message(connection_id, "Extra description removed for: " + keyword);
            } else {
                send_client_message(connection_id, "No extra description for: " + keyword);
            }
            return;
        }

        send_client_message(connection_id, "Usage: redit extra <list|add|edit|remove>");
        return;
    }

    if subcommand == "vnum" {
        handle_vnum_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "capacity" {
        if parts.len() < 2 {
            let current = get_room_living_capacity(room.id);
            let resident_count = get_room_resident_count(room.id);
            let liveable_str = if room.flags.liveable { "[liveable]" } else { "[not liveable — set 'redit flag liveable on' to enable]" };
            send_client_message(connection_id,
                "Living capacity: " + current + "  (residents: " + resident_count + ")  " + liveable_str +
                "\nUsage: redit capacity <n>");
            return;
        }
        let cap = parse_int(parts[1]);
        if cap < 0 {
            send_client_message(connection_id, "Capacity must be non-negative.");
            return;
        }
        set_room_living_capacity(room.id, cap);
        send_client_message(connection_id, "Room living capacity set to " + cap + ".");
        return;
    }

    if subcommand == "area" {
        handle_area_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "trigger" {
        handle_trigger_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "door" {
        handle_door_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "seasonal" {
        handle_seasonal_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "dynamic" {
        handle_dynamic_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "water" {
        handle_water_subcommand(connection_id, room, parts);
        return;
    }

    if subcommand == "catch" {
        handle_catch_subcommand(connection_id, room, parts);
        return;
    }

    // Unknown subcommand
    send_client_message(connection_id, "Unknown subcommand: " + subcommand + "\nUse 'redit' to see available commands.");
}

// Helper function to get opposite direction
fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "down"; }
    if dir == "down" { return "up"; }
    return "";
}

// Normalize short direction to full name
fn normalize_direction(arg) {
    let d = arg.to_lower();
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

// Helper for forcing door open/close/lock/unlock state
fn handle_door_force_state(room_id, parts, action, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit door " + action + " <direction>");
        return;
    }
    let direction = normalize_direction(parts[2]);
    let door = get_door(room_id, direction);
    if door == () {
        send_client_message(connection_id, "No door in that direction.");
        return;
    }

    if action == "open" {
        set_door_closed(room_id, direction, false);
        send_client_message(connection_id, "Door forced open.");
    }
    if action == "close" {
        set_door_closed(room_id, direction, true);
        send_client_message(connection_id, "Door forced closed.");
    }
    if action == "lock" {
        set_door_locked(room_id, direction, true);
        send_client_message(connection_id, "Door forced locked.");
    }
    if action == "unlock" {
        set_door_locked(room_id, direction, false);
        send_client_message(connection_id, "Door forced unlocked.");
    }
}

fn get_door_status_text(door) {
    if door.is_locked { return "locked"; }
    if door.is_closed { return "closed"; }
    return "open";
}

fn format_door_list_entry(direction, door) {
    let status = get_door_status_text(door);
    let key_info = "";
    if door.key_vnum != "" {
        key_info = " (key: " + door.key_vnum + ")";
    }
    return "  " + direction + ": " + door.name + " [" + status + "]" + key_info + "\n";
}

fn build_door_list(room_id) {
    let output = "";
    let has_doors = false;
    let directions = ["north", "south", "east", "west", "up", "down"];
    for direction in directions {
        let door = get_door(room_id, direction);
        if door != () {
            has_doors = true;
            output += format_door_list_entry(direction, door);
        }
    }
    if !has_doors {
        output = "  (no doors)\n";
    }
    return output;
}

fn handle_door_add(room_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: redit door add <direction> <name>\nExample: redit door add north door");
        return;
    }
    let direction = normalize_direction(parts[2]);
    let name = parts[3];

    let target = get_exit_target(room_id, direction);
    if target == "" {
        send_client_message(connection_id, "There's no exit in that direction. Create one first with 'dig' or 'link'.");
        return;
    }
    if has_door(room_id, direction) {
        send_client_message(connection_id, "There's already a door in that direction. Use 'redit door remove " + direction + "' first.");
        return;
    }
    if add_door(room_id, direction, name) {
        send_client_message(connection_id, "Door '" + name + "' added to " + direction + " exit.");
    } else {
        send_client_message(connection_id, "Failed to add door.");
    }
}

fn handle_door_remove(room_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit door remove <direction>");
        return;
    }
    let direction = normalize_direction(parts[2]);
    if remove_door(room_id, direction) {
        send_client_message(connection_id, "Door removed from " + direction + " exit.");
    } else {
        send_client_message(connection_id, "No door in that direction.");
    }
}

fn handle_door_name(room_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: redit door name <direction> <new_name>");
        return;
    }
    let direction = normalize_direction(parts[2]);
    let new_name = parts[3];
    if set_door_name(room_id, direction, new_name) {
        send_client_message(connection_id, "Door name changed to: " + new_name);
    } else {
        send_client_message(connection_id, "No door in that direction.");
    }
}

fn handle_door_desc(room_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit door desc <direction> [description]");
        return;
    }
    let direction = normalize_direction(parts[2]);
    if !has_door(room_id, direction) {
        send_client_message(connection_id, "No door in that direction.");
        return;
    }
    if parts.len() < 4 {
        let door = get_door(room_id, direction);
        let desc = door.description;
        if desc == "" {
            send_client_message(connection_id, "No description set. Use: redit door desc " + direction + " <text>");
        } else {
            send_client_message(connection_id, "Current description: " + desc);
        }
        return;
    }
    let desc_parts = [];
    for i in 3..parts.len() {
        desc_parts.push(parts[i]);
    }
    let new_desc = desc_parts.reduce(|a, b| a + " " + b);
    if set_door_description(room_id, direction, new_desc) {
        send_client_message(connection_id, "Door description set.");
    } else {
        send_client_message(connection_id, "Failed to set description.");
    }
}

fn handle_door_key(room_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: redit door key <direction> <vnum|clear>");
        return;
    }
    let direction = normalize_direction(parts[2]);
    let key_arg = parts[3];
    if !has_door(room_id, direction) {
        send_client_message(connection_id, "No door in that direction.");
        return;
    }
    let key_lower = key_arg.to_lower();
    if key_lower == "clear" || key_lower == "none" {
        if set_door_key(room_id, direction, "") {
            send_client_message(connection_id, "Key requirement cleared from door.");
        } else {
            send_client_message(connection_id, "Failed to clear key requirement.");
        }
        return;
    }
    let item = get_item_by_vnum(key_arg);
    if item == () {
        send_client_message(connection_id, "Item prototype not found with vnum: " + key_arg);
        return;
    }
    if !item.is_key {
        send_client_message(connection_id, "Warning: " + item.name + " is not a key-type item (but setting it anyway).");
    }
    if set_door_key(room_id, direction, key_arg) {
        send_client_message(connection_id, "Door now requires key: " + item.name + " (vnum: " + key_arg + ")");
    } else {
        send_client_message(connection_id, "Failed to set key requirement.");
    }
}

fn handle_door_keywords(room_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: redit door keywords <direction> <kw1> [kw2]...");
        return;
    }
    let direction = normalize_direction(parts[2]);
    if !has_door(room_id, direction) {
        send_client_message(connection_id, "No door in that direction.");
        return;
    }
    let keywords = [];
    for i in 3..parts.len() {
        keywords.push(parts[i].to_lower());
    }
    if set_door_keywords(room_id, direction, keywords) {
        let kw_str = "";
        for kw in keywords {
            if kw_str != "" { kw_str += ", "; }
            kw_str += kw;
        }
        send_client_message(connection_id, "Door keywords set: " + kw_str);
    } else {
        send_client_message(connection_id, "Failed to set keywords.");
    }
}

fn handle_door_sync(room_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit door sync <direction>");
        return;
    }
    let direction = normalize_direction(parts[2]);
    let door = get_door(room_id, direction);
    if door == () {
        send_client_message(connection_id, "No door in that direction.");
        return;
    }
    let target_room_id = get_exit_target(room_id, direction);
    if target_room_id == "" {
        send_client_message(connection_id, "No exit in that direction.");
        return;
    }
    let opposite = get_opposite_direction(direction);
    if opposite == "" {
        send_client_message(connection_id, "Cannot determine opposite direction.");
        return;
    }
    if !has_door(target_room_id, opposite) {
        add_door(target_room_id, opposite, door.name);
    }
    set_door_name(target_room_id, opposite, door.name);
    let desc = door.description;
    if desc != "" {
        set_door_description(target_room_id, opposite, desc);
    }
    let key_vnum = door.key_vnum;
    if key_vnum != "" {
        set_door_key(target_room_id, opposite, key_vnum);
    }
    set_door_closed(target_room_id, opposite, door.is_closed);
    set_door_locked(target_room_id, opposite, door.is_locked);
    send_client_message(connection_id, "Door synced to connected room (" + opposite + " exit).");
}

fn show_door_help(room_id, connection_id) {
    let output = olc::c_header("=== Room Doors ===", connection_id) + "\n";
    output += build_door_list(room_id);
    output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
    output += "  redit door add <direction> <name>   - Add door to exit\n";
    output += "  redit door remove <direction>       - Remove door from exit\n";
    output += "  redit door name <direction> <name>  - Change door name\n";
    output += "  redit door desc <direction> <text>  - Set door description\n";
    output += "  redit door key <direction> <vnum|clear> - Set key (by vnum)\n";
    output += "  redit door keywords <direction> <kw1> [kw2]... - Set keywords\n";
    output += "  redit door open <direction>         - Force open door\n";
    output += "  redit door close <direction>        - Force close door\n";
    output += "  redit door lock <direction>         - Force lock door\n";
    output += "  redit door unlock <direction>       - Force unlock door\n";
    output += "  redit door sync <direction>         - Copy door to connected room\n";
    send_mxp_message(connection_id, output);
}

// ========== Seasonal Description Handlers ==========

fn handle_seasonal_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show all seasonal descriptions
        show_seasonal_overview(room, connection_id);
        return;
    }

    let season = parts[1].to_lower();
    let valid_seasons = ["spring", "summer", "autumn", "winter"];
    let is_valid_season = false;
    for s in valid_seasons {
        if s == season { is_valid_season = true; break; }
    }

    if !is_valid_season {
        send_client_message(connection_id, "Invalid season: " + season + "\nValid seasons: spring, summer, autumn, winter");
        return;
    }

    if parts.len() < 3 {
        // Show specific season description
        let desc = get_seasonal_desc(room, season);
        if desc == "" {
            send_client_message(connection_id, season + " description: (none)\nUsage: redit seasonal " + season + " <text>");
        } else {
            send_client_message(connection_id, season + " description: " + desc);
        }
        return;
    }

    let action = parts[2].to_lower();

    if action == "clear" {
        clear_seasonal_desc(room.id, season, connection_id);
        return;
    }

    if action == "help" {
        handle_seasonal_ai_help(room, season, parts, connection_id);
        return;
    }

    if action == "rephrase" {
        handle_seasonal_ai_rephrase(room, season, connection_id);
        return;
    }

    // Treat remaining args as the description text
    set_seasonal_desc_from_parts(room.id, season, parts, 2, connection_id);
}

fn show_seasonal_overview(room, connection_id) {
    let output = olc::c_header("=== Seasonal Descriptions ===", connection_id) + "\n";
    output += "These descriptions automatically append to the room description based on the current game season.\n\n";

    let spring = room.spring_desc;
    let summer = room.summer_desc;
    let autumn = room.autumn_desc;
    let winter = room.winter_desc;

    output += olc::c_label("Spring: ", connection_id);
    output += if spring == "" { "(none)" } else { olc::c_value(spring, connection_id) };
    output += "\n";

    output += olc::c_label("Summer: ", connection_id);
    output += if summer == "" { "(none)" } else { olc::c_value(summer, connection_id) };
    output += "\n";

    output += olc::c_label("Autumn: ", connection_id);
    output += if autumn == "" { "(none)" } else { olc::c_value(autumn, connection_id) };
    output += "\n";

    output += olc::c_label("Winter: ", connection_id);
    output += if winter == "" { "(none)" } else { olc::c_value(winter, connection_id) };
    output += "\n";

    // Show current season
    let current = get_current_season();
    output += "\n" + olc::c_label("Current season: ", connection_id) + olc::c_value(current, connection_id) + "\n";

    output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
    output += "  redit seasonal <season> <text>        - Set description\n";
    output += "  redit seasonal <season> clear         - Clear description\n";
    if ai_is_enabled() {
        output += "  redit seasonal <season> help <prompt> - AI generate\n";
        output += "  redit seasonal <season> rephrase      - AI rephrase existing\n";
    }
    output += "\nSeasons: spring, summer, autumn, winter";

    send_client_message(connection_id, output);
}

fn get_seasonal_desc(room, season) {
    if season == "spring" { return room.spring_desc; }
    if season == "summer" { return room.summer_desc; }
    if season == "autumn" { return room.autumn_desc; }
    if season == "winter" { return room.winter_desc; }
    return "";
}

fn clear_seasonal_desc(room_id, season, connection_id) {
    let success = false;
    if season == "spring" { success = set_room_spring_desc(room_id, ""); }
    if season == "summer" { success = set_room_summer_desc(room_id, ""); }
    if season == "autumn" { success = set_room_autumn_desc(room_id, ""); }
    if season == "winter" { success = set_room_winter_desc(room_id, ""); }

    if success {
        send_client_message(connection_id, season + " description cleared.");
    } else {
        send_client_message(connection_id, "Error: Failed to clear " + season + " description.");
    }
}

fn set_seasonal_desc_from_parts(room_id, season, parts, start_idx, connection_id) {
    let desc_parts = [];
    for i in start_idx..parts.len() {
        desc_parts.push(parts[i]);
    }
    let desc = desc_parts.reduce(|a, b| a + " " + b);

    let success = false;
    if season == "spring" { success = set_room_spring_desc(room_id, desc); }
    if season == "summer" { success = set_room_summer_desc(room_id, desc); }
    if season == "autumn" { success = set_room_autumn_desc(room_id, desc); }
    if season == "winter" { success = set_room_winter_desc(room_id, desc); }

    if success {
        send_client_message(connection_id, season + " description set to: " + desc);
    } else {
        send_client_message(connection_id, "Error: Failed to set " + season + " description.");
    }
}

fn handle_seasonal_ai_help(room, season, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: redit seasonal " + season + " help <description prompt>\nExample: redit seasonal spring help cherry blossoms in full bloom");
        return;
    }
    let prompt = build_ai_prompt(parts, 3);
    let area_info = get_room_area_info(room.area_id);
    let context = #{
        room_title: room.title,
        area_name: area_info.name,
        theme: area_info.theme,
        season: season,
        base_description: room.description
    };
    let request_id = ai_help_write(connection_id, "seasonal_desc", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return;
    }
    ai_set_target(connection_id, "room", room.id, season + "_desc");
    send_client_message(connection_id, "Generating " + season + " description... please wait.");
}

fn handle_seasonal_ai_rephrase(room, season, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return;
    }
    let current_desc = get_seasonal_desc(room, season);
    if current_desc == "" {
        send_client_message(connection_id, "No " + season + " description to rephrase.");
        return;
    }
    let area_info = get_room_area_info(room.area_id);
    let context = #{
        room_title: room.title,
        area_name: area_info.name,
        theme: area_info.theme,
        season: season,
        base_description: room.description
    };
    let request_id = ai_rephrase(connection_id, "seasonal_desc", current_desc, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return;
    }
    ai_set_target(connection_id, "room", room.id, season + "_desc");
    send_client_message(connection_id, "Rephrasing " + season + " description... please wait.");
}

// ========== Dynamic Description Handler ==========

fn handle_dynamic_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show current dynamic description
        let desc = room.dynamic_desc;
        let output = olc::c_header("=== Dynamic Description ===", connection_id) + "\n";
        output += "This description is appended to the room description. It can be set by triggers for\n";
        output += "weather effects, events, or any dynamic content.\n\n";
        output += olc::c_label("Current: ", connection_id);
        output += if desc == "" { "(none)" } else { olc::c_value(desc, connection_id) };
        output += "\n\n" + olc::c_label("Commands:", connection_id) + "\n";
        output += "  redit dynamic <text>  - Set dynamic description\n";
        output += "  redit dynamic clear   - Clear dynamic description";
        send_client_message(connection_id, output);
        return;
    }

    let action = parts[1].to_lower();

    if action == "clear" {
        if clear_room_dynamic_desc(room.id) {
            send_client_message(connection_id, "Dynamic description cleared.");
        } else {
            send_client_message(connection_id, "Error: Failed to clear dynamic description.");
        }
        return;
    }

    // Treat remaining args as the description text
    let desc_parts = [];
    for i in 1..parts.len() {
        desc_parts.push(parts[i]);
    }
    let desc = desc_parts.reduce(|a, b| a + " " + b);

    if set_room_dynamic_desc(room.id, desc) {
        send_client_message(connection_id, "Dynamic description set to: " + desc);
    } else {
        send_client_message(connection_id, "Error: Failed to set dynamic description.");
    }
}

// ========== Water Type Handler ==========

fn handle_water_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show current water type
        let water_type = get_room_water_type(room.id);
        let output = olc::c_header("=== Room Water Configuration ===", connection_id) + "\n";
        output += "Water type determines if and how players can fish in this room.\n\n";
        output += olc::c_label("Current water type: ", connection_id);
        if water_type == "none" {
            output += "(none - fishing not allowed)\n";
        } else {
            output += olc::c_value(water_type, connection_id) + "\n";
        }
        output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
        output += "  redit water none       - Remove water (no fishing)\n";
        output += "  redit water freshwater - Rivers, lakes, ponds\n";
        output += "  redit water saltwater  - Oceans, seas (+5% bite chance, +2s delay)\n";
        output += "  redit water magical    - Enchanted waters (+15% bite chance, -2s delay)\n";
        output += "\nNote: You also need to set up a catch table with 'redit catch'.";
        send_client_message(connection_id, output);
        return;
    }

    let water_arg = parts[1].to_lower();
    let valid_types = ["none", "freshwater", "saltwater", "magical"];
    let is_valid = false;
    for t in valid_types {
        if t == water_arg { is_valid = true; }
    }

    if !is_valid {
        send_client_message(connection_id, "Invalid water type: " + water_arg + "\nValid types: none, freshwater, saltwater, magical");
        return;
    }

    if set_room_water_type(room.id, water_arg) {
        if water_arg == "none" {
            send_client_message(connection_id, "Water type cleared. Fishing is no longer possible in this room.");
        } else {
            send_client_message(connection_id, "Water type set to: " + water_arg);
        }
    } else {
        send_client_message(connection_id, "Error: Failed to set water type.");
    }
}

// ========== Catch Table Handler ==========

fn format_rarity_display(rarity) {
    if rarity == "common" { return "common"; }
    if rarity == "uncommon" { return "uncommon"; }
    if rarity == "rare" { return "rare"; }
    if rarity == "legendary" { return "LEGENDARY"; }
    return rarity;
}

fn get_item_name_or_unknown(vnum) {
    let item = get_item_by_vnum(vnum);
    if item == () { return "(unknown)"; }
    return item.name;
}

fn handle_catch_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        // Show catch table help and list
        show_catch_overview(room, connection_id);
        return;
    }

    let catch_cmd = parts[1].to_lower();

    if catch_cmd == "list" {
        show_catch_overview(room, connection_id);
        return;
    }

    if catch_cmd == "add" {
        handle_catch_add(room.id, parts, connection_id);
        return;
    }

    if catch_cmd == "remove" {
        handle_catch_remove(room.id, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown catch subcommand: " + catch_cmd + "\nUse 'redit catch' for help.");
}

fn show_catch_overview(room, connection_id) {
    let output = olc::c_header("=== Room Catch Table ===", connection_id) + "\n";
    output += "The catch table defines what items can be caught when fishing in this room.\n\n";

    let water_type = get_room_water_type(room.id);
    output += olc::c_label("Water type: ", connection_id);
    if water_type == "none" {
        output += "(none - set water type first with 'redit water')\n\n";
    } else {
        output += olc::c_value(water_type, connection_id) + "\n\n";
    }

    let catch_table = get_catch_table(room.id);
    output += olc::c_label("Catchable items:", connection_id) + "\n";

    if catch_table.len() == 0 {
        output += "  (none configured)\n";
    } else {
        // Calculate total weight for percentage display
        let total_weight = 0;
        for entry in catch_table {
            total_weight += entry.weight;
        }

        for entry in catch_table {
            let item_name = get_item_name_or_unknown(entry.vnum);
            let pct = (entry.weight * 100) / total_weight;
            let rarity_display = format_rarity_display(entry.rarity);
            output += "  " + entry.vnum + " - " + item_name;
            output += " [" + rarity_display + "]";
            output += " (wt:" + entry.weight + " ~" + pct + "%)";
            if entry.min_skill > 0 {
                output += " (skill " + entry.min_skill + "+)";
            }
            output += "\n";
        }
    }

    output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
    output += "  redit catch add <vnum> <weight> <min_skill> <rarity>\n";
    output += "    - Add item to catch table\n";
    output += "    - weight: relative chance (higher = more common)\n";
    output += "    - min_skill: minimum fishing skill required (0-10)\n";
    output += "    - rarity: common, uncommon, rare, or legendary\n";
    output += "  redit catch remove <vnum> - Remove item from table\n";
    output += "\nExample: redit catch add fish_trout 10 0 common";

    send_client_message(connection_id, output);
}

fn handle_catch_add(room_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: redit catch add <vnum> <weight> <min_skill> <rarity>\nExample: redit catch add fish_trout 10 0 common");
        return;
    }

    let vnum = parts[2];
    let weight = parse_int(parts[3]);
    let min_skill = parse_int(parts[4]);
    let rarity = parts[5].to_lower();

    // Validate weight
    if weight < 1 {
        send_client_message(connection_id, "Weight must be at least 1.");
        return;
    }

    // Validate min_skill
    if min_skill < 0 || min_skill > 10 {
        send_client_message(connection_id, "Min skill must be between 0 and 10.");
        return;
    }

    // Validate rarity
    let valid_rarities = ["common", "uncommon", "rare", "legendary"];
    let is_valid_rarity = false;
    for r in valid_rarities {
        if r == rarity { is_valid_rarity = true; }
    }
    if !is_valid_rarity {
        send_client_message(connection_id, "Invalid rarity: " + rarity + "\nValid rarities: common, uncommon, rare, legendary");
        return;
    }

    // Check if item exists
    let item = get_item_by_vnum(vnum);
    if item == () {
        send_client_message(connection_id, "Warning: No item found with vnum '" + vnum + "'. Adding anyway (create it later with oedit).");
    }

    if add_catch_entry(room_id, vnum, weight, min_skill, rarity) {
        let msg = "Added to catch table: " + vnum;
        if item != () {
            msg += " (" + item.name + ")";
        }
        msg += " [" + rarity + "] weight:" + weight;
        if min_skill > 0 {
            msg += " min_skill:" + min_skill;
        }
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Error: Failed to add catch entry.");
    }
}

fn handle_catch_remove(room_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: redit catch remove <vnum>");
        return;
    }

    let vnum = parts[2];

    if remove_catch_entry(room_id, vnum) {
        send_client_message(connection_id, "Removed from catch table: " + vnum);
    } else {
        send_client_message(connection_id, "No catch entry found with vnum: " + vnum);
    }
}

fn handle_door_subcommand(connection_id, room, parts) {
    if parts.len() < 2 {
        show_door_help(room.id, connection_id);
        return;
    }

    let door_cmd = parts[1].to_lower();

    if door_cmd == "add" { handle_door_add(room.id, parts, connection_id); return; }
    if door_cmd == "remove" { handle_door_remove(room.id, parts, connection_id); return; }
    if door_cmd == "name" { handle_door_name(room.id, parts, connection_id); return; }
    if door_cmd == "desc" { handle_door_desc(room.id, parts, connection_id); return; }
    if door_cmd == "key" { handle_door_key(room.id, parts, connection_id); return; }
    if door_cmd == "keywords" { handle_door_keywords(room.id, parts, connection_id); return; }
    if door_cmd == "open" { handle_door_force_state(room.id, parts, "open", connection_id); return; }
    if door_cmd == "close" { handle_door_force_state(room.id, parts, "close", connection_id); return; }
    if door_cmd == "lock" { handle_door_force_state(room.id, parts, "lock", connection_id); return; }
    if door_cmd == "unlock" { handle_door_force_state(room.id, parts, "unlock", connection_id); return; }
    if door_cmd == "sync" { handle_door_sync(room.id, parts, connection_id); return; }

    send_client_message(connection_id, "Unknown door subcommand: " + door_cmd + "\nUse 'redit door' for help.");
}
