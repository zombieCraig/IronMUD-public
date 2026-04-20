// scripts/commands/spedit.rhai
// Spawn Point Editor
// Usage: spedit <subcommand> [args...]
// @description Edit spawn points in current area
// @category building

// Import shared OLC helpers
import "olc_helpers" as olc;

// Resolve spawn point from index or UUID within a filtered list
fn resolve_spawn_point(spawns, identifier) {
    let idx = parse_int(identifier);
    if idx != () {
        if idx >= 0 && idx < spawns.len() {
            return spawns[idx];
        }
        return ();
    }
    // Fall back to UUID lookup
    get_spawn_point(identifier)
}

// Get filtered spawn points based on filter type
fn get_filtered_spawns(area, filter_type, room_id) {
    let all_spawns = list_spawn_points_in_area(area.id);
    if filter_type == "all" { return all_spawns; }

    let filtered = [];
    for sp in all_spawns {
        if filter_type == "room" && sp.room_id == room_id {
            filtered.push(sp);
        } else if filter_type == "mobs" && (sp.entity_type == "mobile" || sp.entity_type == "mob") {
            filtered.push(sp);
        } else if filter_type == "items" && (sp.entity_type == "item" || sp.entity_type == "object") {
            filtered.push(sp);
        }
    }
    filtered
}

// Parse filter context from command parts
// Returns: [filter_type, arg_offset, room_id]
fn parse_filter_context(parts, start_idx, default_room_id) {
    let filter_type = "room";
    let offset = 0;
    let room_id = default_room_id;

    if parts.len() > start_idx {
        let maybe_filter = parts[start_idx].to_lower();
        if maybe_filter == "all" {
            filter_type = "all";
            offset = 1;
        } else if maybe_filter == "mobs" || maybe_filter == "mobiles" {
            filter_type = "mobs";
            offset = 1;
        } else if maybe_filter == "items" || maybe_filter == "objects" {
            filter_type = "items";
            offset = 1;
        } else if maybe_filter == "room" {
            filter_type = "room";
            offset = 1;
            // Check for optional room vnum/id
            if parts.len() > start_idx + 1 {
                let room_arg = parts[start_idx + 1];
                if room_arg != "." {
                    // Try to resolve room by vnum
                    let room = get_room_by_vnum(room_arg);
                    if room != () {
                        room_id = room.id;
                        offset = 2;
                    }
                }
            }
        }
    }
    [filter_type, offset, room_id]
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Get area from current room
    let room = get_room_data(char.current_room_id);
    if room == () || room.area_id == () {
        send_client_message(connection_id, "You must be in a room assigned to an area.");
        return;
    }
    let area = get_area_data(room.area_id);
    if area == () {
        send_client_message(connection_id, "Error: Could not load area data.");
        return;
    }

    // Permission check
    if !can_edit_area(area.id, char.name) {
        send_client_message(connection_id, "You don't have permission to edit spawn points in this area.");
        return;
    }

    // Parse subcommand (no area argument anymore)
    let parts = args.split(" ");
    let subcommand = "list";
    if args != "" && parts.len() >= 1 {
        subcommand = parts[0].to_lower();
    }

    // Get room title for display
    let room_title = if room != () && room.title != "" { room.title } else { "Current Room" };

    if subcommand == "list" || subcommand == "" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        list_spawn_points(area, spawns, filter_type, room_title, connection_id);
        return;
    }

    if subcommand == "create" {
        handle_create(area, parts, connection_id);
        return;
    }

    if subcommand == "delete" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_delete(spawns, parts, 1 + offset, connection_id);
        return;
    }

    if subcommand == "enable" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_enable(spawns, parts, 1 + offset, true, connection_id);
        return;
    }

    if subcommand == "disable" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_enable(spawns, parts, 1 + offset, false, connection_id);
        return;
    }

    if subcommand == "max" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_max(spawns, parts, 1 + offset, connection_id);
        return;
    }

    if subcommand == "interval" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_interval(spawns, parts, 1 + offset, connection_id);
        return;
    }

    if subcommand == "dep" {
        let ctx = parse_filter_context(parts, 1, char.current_room_id);
        let filter_type = ctx[0];
        let offset = ctx[1];
        let room_id = ctx[2];
        let spawns = get_filtered_spawns(area, filter_type, room_id);
        handle_dep(spawns, parts, 1 + offset, connection_id);
        return;
    }

    show_usage(connection_id);
}

fn show_usage(connection_id) {
    let msg = "Usage: spedit <subcommand> [args...]\n";
    msg += "(Uses current room's area automatically)\n\n";
    msg += "Subcommands:\n";
    msg += "  spedit list [filter]           - List spawn points\n";
    msg += "  spedit create <room|.> <mobile|item> <vnum> <max> <interval>\n\n";
    msg += "  # Modification commands (all support optional [filter] before index):\n";
    msg += "  spedit delete [filter] <index> - Delete spawn point\n";
    msg += "  spedit enable [filter] <index> - Enable spawn point\n";
    msg += "  spedit disable [filter] <index> - Disable spawn point\n";
    msg += "  spedit max [filter] <index> <count> - Set max count\n";
    msg += "  spedit interval [filter] <index> <secs> - Set interval\n";
    msg += "  spedit dep [filter] <action> <index> ...\n\n";
    msg += "Filters (default: current room):\n";
    msg += "  (none)      - Current room only\n";
    msg += "  all         - All spawns in area\n";
    msg += "  mobs        - Mobile spawns in area\n";
    msg += "  items       - Item spawns in area\n";
    msg += "  room <vnum> - Spawns in specific room\n\n";
    msg += "Dependency actions:\n";
    msg += "  spedit dep [filter] add <index> inv <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] add <index> equip <item_vnum> <slot> [chance%]\n";
    msg += "  spedit dep [filter] add <index> wield <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] add <index> contain <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] remove <index> <dep_index>\n";
    msg += "  spedit dep [filter] clear <index>\n\n";
    msg += "Examples:\n";
    msg += "  spedit list                    - Spawns in current room\n";
    msg += "  spedit list mobs               - All mobile spawns in area\n";
    msg += "  spedit delete 0                - Delete index 0 from current room\n";
    msg += "  spedit delete mobs 0           - Delete index 0 from mobile filter\n";
    msg += "  spedit dep add all 0 inv sword - Add dependency to area-wide index 0\n";
    msg += "  spedit create . mobile guard 1 60";
    send_client_message(connection_id, msg);
}

fn list_spawn_points(area, spawns, filter_type, room_title, connection_id) {
    // Build header based on filter type
    let header = "";
    if filter_type == "room" {
        header = "=== Spawn Points in " + room_title + " ===";
    } else if filter_type == "all" {
        header = "=== Spawn Points in " + area.name + " (all) ===";
    } else if filter_type == "mobs" {
        header = "=== Mobile Spawns in " + area.name + " ===";
    } else if filter_type == "items" {
        header = "=== Item Spawns in " + area.name + " ===";
    }

    let output = olc::c_header(header, connection_id) + "\n\n";

    if spawns.len() == 0 {
        output += "(no spawn points";
        if filter_type == "room" {
            output += " in this room";
        } else if filter_type == "mobs" {
            output += " for mobiles";
        } else if filter_type == "items" {
            output += " for items";
        }
        output += ")\n";
    } else {
        let idx = 0;
        for sp in spawns {
            output += format_spawn_entry(sp, idx, connection_id);
            idx += 1;
        }
    }

    // Add summary footer for room-only view
    if filter_type == "room" {
        let total_in_area = list_spawn_points_in_area(area.id).len();
        if total_in_area > spawns.len() {
            output += "\nShowing " + spawns.len() + " spawn(s) in this room. ";
            output += "Use 'spedit list all' to see all " + total_in_area + " in area.\n";
        }
    }

    output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
    output += "  spedit create <room|.> <mobile|item> <vnum> <max> <interval>\n";
    output += "  spedit delete/enable/disable [filter] <index>\n";
    output += "  spedit dep [filter] add <index> inv|equip|wield|contain <item_vnum> ...\n\n";
    output += olc::c_label("Filters:", connection_id) + " (none)=current room, all, mobs, items, room <vnum>";

    send_client_message(connection_id, output);
}

fn format_spawn_entry(sp, idx, conn_id) {
    let status = "[OFF]";
    if sp.enabled { status = olc::c_warn("[ON]", conn_id); }

    let idx_str = "[" + idx + "]";
    let type_vnum = sp.entity_type + ": " + sp.vnum;
    let output = idx_str + " " + status + " " + olc::c_value(type_vnum, conn_id) + "\n";

    // Get room name
    let room = get_room_data(sp.room_id);
    let room_name = if room != () { room.title } else { sp.room_id };

    output += "    " + olc::c_label("Room: ", conn_id) + olc::c_value(room_name, conn_id);
    output += "  " + olc::c_label("Max: ", conn_id) + olc::c_value("" + sp.max_count, conn_id);
    output += "  " + olc::c_label("Interval: ", conn_id) + olc::c_value(sp.respawn_interval_secs + "s", conn_id);
    output += "  " + olc::c_label("Active: ", conn_id) + olc::c_value(sp.spawned_count + "/" + sp.max_count, conn_id) + "\n";

    // Show inline dependencies
    output += format_inline_deps(sp, conn_id);
    output
}

fn format_inline_deps(sp, conn_id) {
    let deps = get_spawn_dependencies(sp.id);
    if deps.len() == 0 {
        return "    (no dependencies)\n\n";
    }

    let output = "    " + olc::c_label("Dependencies:", conn_id) + "\n";
    let dep_idx = 0;
    for dep in deps {
        let count_str = if dep.count > 1 { " x" + dep.count } else { "" };
        let chance_str = if dep.chance < 100 { " " + dep.chance + "%" } else { "" };
        output += "      " + dep_idx + ". " + dep.item_vnum + count_str + " (" + dep.destination + ")" + chance_str + "\n";
        dep_idx += 1;
    }
    output += "\n";
    output
}

fn handle_create(area, parts, connection_id) {
    // spedit create <room> <type> <vnum> <max> <interval>
    // Use "." for room to specify current room
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: spedit create <room_id|.> <mobile|item> <vnum> <max_count> <interval_secs>\nUse '.' for room_id to specify your current room.");
        return;
    }

    let room_id = parts[1];

    // Handle "." as current room
    if room_id == "." {
        let char = get_player_character(connection_id);
        if char != () {
            room_id = char.current_room_id;
        } else {
            send_client_message(connection_id, "Error: Unable to determine current room.");
            return;
        }
    }

    let entity_type = parts[2];
    let vnum = parts[3];
    let max_count = parse_int(parts[4]);
    let interval_secs = parse_int(parts[5]);

    if max_count == () || interval_secs == () {
        send_client_message(connection_id, "Invalid numbers for max_count or interval.");
        return;
    }

    // Validate room exists - try UUID first, then vnum
    let room = get_room_data(room_id);
    if room == () {
        room = get_room_by_vnum(room_id);
    }
    if room == () {
        send_client_message(connection_id, "Room not found: " + room_id);
        return;
    }

    // Validate prototype exists
    let type_lower = entity_type.to_lower();
    if type_lower == "mobile" || type_lower == "mob" {
        let proto = get_mobile_by_vnum(vnum);
        if proto == () {
            send_client_message(connection_id, "Mobile prototype not found: " + vnum);
            return;
        }
        if !proto.is_prototype {
            send_client_message(connection_id, "Mobile '" + vnum + "' is not marked as a prototype.");
            return;
        }
    } else if type_lower == "item" || type_lower == "object" {
        let proto = get_item_by_vnum(vnum);
        if proto == () {
            send_client_message(connection_id, "Item prototype not found: " + vnum);
            return;
        }
        if !proto.is_prototype {
            send_client_message(connection_id, "Item '" + vnum + "' is not marked as a prototype.");
            return;
        }
    } else {
        send_client_message(connection_id, "Invalid entity type. Use: mobile or item");
        return;
    }

    let sp = create_spawn_point(area.id, room.id, entity_type, vnum, max_count, interval_secs);
    if sp == () {
        send_client_message(connection_id, "Failed to create spawn point.");
        return;
    }

    send_client_message(connection_id, "Created spawn point: " + sp.id + "\nType: " + sp.entity_type + ", VNUM: " + sp.vnum + ", Max: " + sp.max_count + ", Interval: " + sp.respawn_interval_secs + "s");
}

fn handle_delete(spawns, parts, idx_pos, connection_id) {
    if parts.len() <= idx_pos {
        send_client_message(connection_id, "Usage: spedit delete [filter] <index>");
        return;
    }

    let sp = resolve_spawn_point(spawns, parts[idx_pos]);
    if sp == () {
        send_client_message(connection_id, "Spawn point not found: " + parts[idx_pos]);
        return;
    }

    if delete_spawn_point(sp.id) {
        send_client_message(connection_id, "Spawn point deleted.");
    } else {
        send_client_message(connection_id, "Failed to delete spawn point.");
    }
}

fn handle_enable(spawns, parts, idx_pos, enabled, connection_id) {
    if parts.len() <= idx_pos {
        send_client_message(connection_id, "Usage: spedit enable/disable [filter] <index>");
        return;
    }

    let sp = resolve_spawn_point(spawns, parts[idx_pos]);
    if sp == () {
        send_client_message(connection_id, "Spawn point not found: " + parts[idx_pos]);
        return;
    }

    if set_spawn_point_enabled(sp.id, enabled) {
        let status = "disabled";
        if enabled { status = "enabled"; }
        send_client_message(connection_id, "Spawn point " + status + ".");
    } else {
        send_client_message(connection_id, "Failed to update spawn point.");
    }
}

fn handle_max(spawns, parts, idx_pos, connection_id) {
    if parts.len() <= idx_pos + 1 {
        send_client_message(connection_id, "Usage: spedit max [filter] <index> <count>");
        return;
    }

    let sp = resolve_spawn_point(spawns, parts[idx_pos]);
    if sp == () {
        send_client_message(connection_id, "Spawn point not found: " + parts[idx_pos]);
        return;
    }

    let count = parse_int(parts[idx_pos + 1]);
    if count == () {
        send_client_message(connection_id, "Invalid count.");
        return;
    }

    if set_spawn_point_max_count(sp.id, count) {
        send_client_message(connection_id, "Max count set to: " + count);
    } else {
        send_client_message(connection_id, "Failed to update spawn point.");
    }
}

fn handle_interval(spawns, parts, idx_pos, connection_id) {
    if parts.len() <= idx_pos + 1 {
        send_client_message(connection_id, "Usage: spedit interval [filter] <index> <seconds>");
        return;
    }

    let sp = resolve_spawn_point(spawns, parts[idx_pos]);
    if sp == () {
        send_client_message(connection_id, "Spawn point not found: " + parts[idx_pos]);
        return;
    }

    let secs = parse_int(parts[idx_pos + 1]);
    if secs == () {
        send_client_message(connection_id, "Invalid interval.");
        return;
    }

    if set_spawn_point_interval(sp.id, secs) {
        send_client_message(connection_id, "Interval set to: " + secs + " seconds");
    } else {
        send_client_message(connection_id, "Failed to update spawn point.");
    }
}

fn handle_dep(spawns, parts, idx_offset, connection_id) {
    // spedit dep [filter] <action> <index> [args...]
    // idx_offset points to where the action should be (after "dep" and any filter)
    if parts.len() <= idx_offset + 1 {
        show_dep_usage(connection_id);
        return;
    }

    let action = parts[idx_offset].to_lower();
    let sp_identifier = parts[idx_offset + 1];

    // Validate spawn point exists
    let sp = resolve_spawn_point(spawns, sp_identifier);
    if sp == () {
        send_client_message(connection_id, "Spawn point not found: " + sp_identifier);
        return;
    }

    if action == "list" {
        handle_dep_list(sp, connection_id);
        return;
    }
    if action == "add" {
        handle_dep_add(sp, parts, idx_offset, connection_id);
        return;
    }
    if action == "remove" {
        handle_dep_remove(sp, parts, idx_offset, connection_id);
        return;
    }
    if action == "clear" {
        handle_dep_clear(sp, connection_id);
        return;
    }

    show_dep_usage(connection_id);
}

fn show_dep_usage(connection_id) {
    let msg = "Dependency subcommands (all support optional [filter] before action):\n";
    msg += "  spedit dep [filter] list <index>    - List dependencies\n";
    msg += "  spedit dep [filter] add <index> inv <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] add <index> equip <item_vnum> <slot> [chance%]\n";
    msg += "  spedit dep [filter] add <index> wield <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] add <index> contain <item_vnum> [count] [chance%]\n";
    msg += "  spedit dep [filter] remove <index> <dep_index>\n";
    msg += "  spedit dep [filter] clear <index>\n\n";
    msg += "Filters: (none)=current room, all, mobs, items, room <vnum>\n\n";
    msg += "Equipment slots: head, neck, shoulders, torso, back, arms, hands,\n";
    msg += "  waist, legs, feet, wrists, ankles, wielded, offhand, ears\n\n";
    msg += "Chance: 1-100 (default 100). Example: spedit dep add 0 inv gem 1 25";
    send_client_message(connection_id, msg);
}

fn format_dep_idx(idx) {
    let s = "" + idx;
    if idx < 10 { return " " + s; }
    s
}

fn format_dep_vnum(vnum) {
    let s = vnum;
    let padding = 20 - s.len();
    if padding > 0 {
        for i in 0..padding { s += " "; }
    }
    s
}

fn format_dep_count(count) {
    let s = "" + count;
    if count < 10 { return "    " + s; }
    if count < 100 { return "   " + s; }
    s
}

fn format_dep_row(idx, dep) {
    let chance_str = if dep.chance < 100 { "" + dep.chance + "%" } else { "100%" };
    // Pad chance to 6 chars
    while chance_str.len() < 6 { chance_str += " "; }
    format_dep_idx(idx) + "   " + format_dep_vnum(dep.item_vnum) + "  " + format_dep_count(dep.count) + "  " + chance_str + "  " + dep.destination + "\n"
}

fn handle_dep_list(sp, connection_id) {
    let deps = get_spawn_dependencies(sp.id);
    let output = build_dep_header(sp, connection_id);

    if deps.len() == 0 {
        output += "(no dependencies defined)\n\nWhen this entity spawns, no additional items will be created.\n";
    } else {
        output += build_dep_table(deps, connection_id);
    }

    output += build_dep_footer(sp);
    send_client_message(connection_id, output);
}

fn build_dep_header(sp, conn_id) {
    let output = olc::c_header("=== Dependencies for Spawn Point ===", conn_id) + "\n";
    output += olc::c_label("ID: ", conn_id) + olc::c_value(sp.id, conn_id) + "\n";
    output += olc::c_label("Entity: ", conn_id) + olc::c_value(sp.entity_type + " (" + sp.vnum + ")", conn_id) + "\n\n";
    output
}

fn build_dep_table(deps, conn_id) {
    let output = olc::c_label("Idx  Item VNUM              Count  Chance  Destination", conn_id) + "\n";
    output += "---  --------------------  -----  ------  -----------\n";
    let idx = 0;
    for dep in deps {
        output += format_dep_row(idx, dep);
        idx += 1;
    }
    output += "\nThese items will be spawned when the entity is created.\n";
    output
}

fn build_dep_footer(sp) {
    let output = "\nTo add: spedit dep add <index> inv|equip|wield|contain <item_vnum> ...\n";
    output += "To remove: spedit dep remove <index> <dep_index>";
    output
}

fn is_inv_type(t) {
    if t == "inv" { return true; }
    if t == "inventory" { return true; }
    false
}

fn is_equip_type(t) {
    if t == "equip" { return true; }
    if t == "equipped" { return true; }
    if t == "wear" { return true; }
    false
}

fn is_wield_type(t) {
    if t == "wield" { return true; }
    if t == "wielded" { return true; }
    false
}

fn is_contain_type(t) {
    if t == "contain" { return true; }
    if t == "container" { return true; }
    false
}

fn get_dep_success_msg(item_vnum, dest_type, wear_loc, count) {
    let msg = "Added dependency: " + item_vnum;
    if is_inv_type(dest_type) { return msg + " (inventory, x" + count + ")"; }
    if is_wield_type(dest_type) { return msg + " (wielded)"; }
    if is_equip_type(dest_type) { return msg + " (equipped at " + wear_loc + ")"; }
    if is_contain_type(dest_type) { return msg + " (in container, x" + count + ")"; }
    msg
}

fn handle_dep_add(sp, parts, idx_offset, connection_id) {
    // spedit dep [filter] add <index> inv|equip|contain <item_vnum> [slot/count] [chance%]
    // idx_offset points to "add", so:
    //   parts[idx_offset] = "add"
    //   parts[idx_offset + 1] = <index>
    //   parts[idx_offset + 2] = inv|equip|contain
    //   parts[idx_offset + 3] = <item_vnum>
    //   etc.
    let base = idx_offset + 2; // position of dest_type
    if parts.len() <= base + 1 {
        send_client_message(connection_id, "Usage:\n  spedit dep [filter] add <index> inv <item_vnum> [count] [chance%]\n  spedit dep [filter] add <index> equip <item_vnum> <slot> [chance%]\n  spedit dep [filter] add <index> wield <item_vnum> [count] [chance%]\n  spedit dep [filter] add <index> contain <item_vnum> [count] [chance%]");
        return;
    }

    let dest_type = parts[base].to_lower();
    let item_vnum = parts[base + 1];

    let item_proto = get_item_by_vnum(item_vnum);
    if item_proto == () {
        send_client_message(connection_id, "Item prototype not found: " + item_vnum);
        return;
    }
    if !item_proto.is_prototype {
        send_client_message(connection_id, "Item '" + item_vnum + "' is not marked as a prototype.");
        return;
    }

    let wear_loc = "";
    let count = 1;
    let chance = 100;
    let orig_dest_type = dest_type;

    if is_wield_type(dest_type) {
        // Convenience: wield auto-sets wear_loc, no slot param needed
        wear_loc = "wielded";
        dest_type = "equip";
        count = parse_dep_count(parts, base + 2);
        chance = parse_dep_chance(parts, base + 3);
    } else if is_equip_type(dest_type) {
        if parts.len() <= base + 2 {
            send_client_message(connection_id, "Equip requires a slot: spedit dep [filter] add <index> equip <item_vnum> <slot> [chance%]");
            return;
        }
        wear_loc = parts[base + 2].to_lower();
        // Optional chance for equip: parts[base + 3]
        chance = parse_dep_chance(parts, base + 3);
    } else {
        count = parse_dep_count(parts, base + 2);
        // Optional chance for inv/contain: after count (parts[base + 3])
        chance = parse_dep_chance(parts, base + 3);
    }

    if count < 1 { count = 1; }
    if count > 100 {
        send_client_message(connection_id, "Count cannot exceed 100.");
        return;
    }

    if chance < 1 || chance > 100 {
        send_client_message(connection_id, "Chance must be 1-100.");
        return;
    }

    if add_spawn_dependency(sp.id, item_vnum, dest_type, wear_loc, count, chance) {
        let msg = get_dep_success_msg(item_vnum, orig_dest_type, wear_loc, count);
        if chance < 100 { msg += " (" + chance + "% chance)"; }
        send_client_message(connection_id, msg);
    } else {
        let err = "Failed to add dependency.";
        if is_equip_type(dest_type) { err += " Check that '" + wear_loc + "' is a valid wear location."; }
        send_client_message(connection_id, err);
    }
}

fn parse_dep_count(parts, idx) {
    // Parse optional count parameter at the given index
    if parts.len() <= idx { return 1; }
    let parsed = parse_int(parts[idx]);
    if parsed == () { return 1; }
    parsed
}

fn parse_dep_chance(parts, idx) {
    // Parse optional chance parameter at the given index
    if parts.len() <= idx { return 100; }
    let parsed = parse_int(parts[idx]);
    if parsed == () { return 100; }
    parsed
}

fn handle_dep_remove(sp, parts, idx_offset, connection_id) {
    // spedit dep [filter] remove <index> <dep_index>
    // idx_offset points to "remove", so:
    //   parts[idx_offset + 1] = <index>
    //   parts[idx_offset + 2] = <dep_index>
    if parts.len() <= idx_offset + 2 {
        send_client_message(connection_id, "Usage: spedit dep [filter] remove <index> <dep_index>");
        return;
    }

    let idx = parse_int(parts[idx_offset + 2]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }

    if remove_spawn_dependency(sp.id, idx) {
        send_client_message(connection_id, "Dependency removed at index " + idx + ".");
    } else {
        send_client_message(connection_id, "Failed to remove dependency. Check that the index is valid.");
    }
}

fn handle_dep_clear(sp, connection_id) {
    if clear_spawn_dependencies(sp.id) {
        send_client_message(connection_id, "All dependencies cleared from spawn point.");
    } else {
        send_client_message(connection_id, "Failed to clear dependencies.");
    }
}
