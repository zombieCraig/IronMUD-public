// scripts/commands/aedit.rhai
// Usage: aedit [area_prefix] [subcommand]
// Edit area properties - defaults to current room's area

// Import shared OLC helpers
import "olc_helpers" as olc;

// Subcommands:
//   (none)               - Show area properties
//   name <text>          - Set area name
//   desc <text>          - Set area description
//   prefix <text>        - Set area prefix
//   theme <text>         - Set area theme
//   levels <min> <max>   - Set level range
//   owner <name|clear>   - Set or clear area owner
//   permission <level>   - Set permission level (owner_only/trusted/all_builders)
//   trust <name>         - Add character to trusted builders
//   untrust <name>       - Remove character from trusted builders
//   trustees             - List trusted builders
//   forage               - Show forage tables
//   forage city add <vnum> <min_skill> <rarity>     - Add city forage entry
//   forage city remove <vnum>                        - Remove city forage entry
//   forage wilderness add <vnum> <min_skill> <rarity> - Add wilderness forage entry
//   forage wilderness remove <vnum>                  - Remove wilderness forage entry

// Extract role from a migrant vnum.
//   "migrant:<prefix>"        -> "common"
//   "migrant:<role>:<prefix>" -> "<role>"
fn migrant_role(vnum) {
    let parts = vnum.split(":");
    if parts.len() >= 3 { return parts[1]; }
    return "common";
}

// Does this vnum belong to a migrant of the given area?
fn is_area_migrant(vnum, area_prefix) {
    if !vnum.starts_with("migrant:") { return false; }
    let parts = vnum.split(":");
    return parts[parts.len() - 1] == area_prefix;
}

// Tally migrants currently living in this area, by role.
fn count_migrants_in_area(area_prefix) {
    let counts = #{ total: 0, common: 0, guard: 0, healer: 0, scavenger: 0 };
    let all = list_all_mobiles();
    for m in all {
        if is_area_migrant(m.vnum, area_prefix) {
            counts.total += 1;
            let role = migrant_role(m.vnum);
            if role == "common" { counts.common += 1; }
            else if role == "guard" { counts.guard += 1; }
            else if role == "healer" { counts.healer += 1; }
            else if role == "scavenger" { counts.scavenger += 1; }
        }
    }
    return counts;
}

// Format a migrant-count breakdown like "3 (2 common, 1 guard)" — empty roles omitted.
fn format_migrant_counts(counts) {
    let s = "" + counts.total;
    if counts.total > 0 {
        let parts = [];
        if counts.common > 0    { parts.push("" + counts.common + " common"); }
        if counts.guard > 0     { parts.push("" + counts.guard + " guard"); }
        if counts.healer > 0    { parts.push("" + counts.healer + " healer"); }
        if counts.scavenger > 0 { parts.push("" + counts.scavenger + " scav"); }
        if parts.len() > 0 { s = s + " (" + parts.join(", ") + ")"; }
    }
    return s;
}

// Format interval with parenthetical days-until-next-check.
// Uses the same 30-day/12-month calendar as src/migration/mod.rs::absolute_game_day.
fn format_migration_interval(area) {
    let s = "" + area.migration_interval_days + " game days";
    if area.migration_interval_days <= 0 {
        return s;
    }
    let gt = get_game_time();
    if gt == () {
        return s;
    }
    let current_day = gt.year * 360 + (gt.month - 1) * 30 + (gt.day - 1);
    let last = area.last_migration_check_day;
    if last < 0 {
        return s + " (due now)";
    }
    let next_due = last + area.migration_interval_days;
    let remaining = next_due - current_day;
    if remaining <= 0 {
        return s + " (due now)";
    }
    let unit = if remaining == 1 { "day" } else { "days" };
    return s + " (" + remaining + " " + unit + " until next check)";
}

// Helper to find area by prefix or UUID
fn find_area(identifier) {
    // First try by prefix (case-insensitive)
    let area = get_area_by_prefix(identifier);
    if area != () {
        return area;
    }
    // Fall back to UUID
    return get_area_data(identifier);
}

// Check if a string looks like a subcommand (not an area identifier)
fn is_subcommand(arg) {
    let cmd = arg.to_lower();
    if cmd == "show" { return true; }
    if cmd == "name" { return true; }
    if cmd == "desc" { return true; }
    if cmd == "prefix" { return true; }
    if cmd == "theme" { return true; }
    if cmd == "levels" { return true; }
    if cmd == "owner" { return true; }
    if cmd == "permission" { return true; }
    if cmd == "trust" { return true; }
    if cmd == "untrust" { return true; }
    if cmd == "trustees" { return true; }
    if cmd == "forage" { return true; }
    if cmd == "zone" { return true; }
    if cmd == "flags" { return true; }
    if cmd == "default_flags" { return true; }
    if cmd == "immigration" { return true; }
    return false;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let parts = args.split(" ");
    let area = ();
    let subcommand_start = 0;

    // If no args or first arg looks like a subcommand, use current room's area
    if args == "" || is_subcommand(parts[0]) {
        // Get area from current room
        let room = get_room_data(char.current_room_id);
        if room == () {
            send_client_message(connection_id, "You are nowhere.");
            return;
        }
        if room.area_id == "" {
            send_client_message(connection_id, "This room is not assigned to an area.\nUsage: aedit <area_prefix> [subcommand]\nUse 'alist' to see available areas.");
            return;
        }
        area = get_area_data(room.area_id);
        if area == () {
            send_client_message(connection_id, "Could not find the area for this room.");
            return;
        }
        subcommand_start = 0;
    } else {
        // First arg is area identifier (prefix or UUID)
        area = find_area(parts[0]);
        if area == () {
            send_client_message(connection_id, "Area not found: " + parts[0] + "\nUse 'alist' to see available areas, or run 'aedit' with no args to edit current room's area.");
            return;
        }
        subcommand_start = 1;
    }

    // Permission check
    if !can_edit_area(area.id, char.name) {
        send_client_message(connection_id, "You don't have permission to edit this area.");
        return;
    }

    // For help output - use prefix if available, otherwise first few chars of ID
    let area_ref = if area.prefix != "" { area.prefix } else { area.id.sub_string(0, 8) };

    // Parse subcommand
    let subcommand = "";
    if parts.len() > subcommand_start {
        subcommand = parts[subcommand_start].to_lower();
    }

    // Show properties if no subcommand
    if subcommand == "" || subcommand == "show" {
        let output = olc::c_header("=== Area Editor ===", connection_id) + "\n";
        output += olc::c_label("Name: ", connection_id) + olc::c_value(area.name, connection_id) + "\n";
        output += olc::c_label("Prefix: ", connection_id) + olc::c_value(area.prefix, connection_id) + "\n";
        output += olc::c_label("ID: ", connection_id) + olc::c_value(area.id, connection_id) + "\n";
        output += olc::c_label("Description: ", connection_id);
        if area.description == "" {
            output += "(none)\n";
        } else {
            output += olc::c_value(area.description, connection_id) + "\n";
        }
        output += olc::c_label("Level Range: ", connection_id) + olc::c_value(area.level_min + " - " + area.level_max, connection_id) + "\n";
        output += olc::c_label("Theme: ", connection_id);
        if area.theme == "" {
            output += "(none)\n";
        } else {
            output += olc::c_value(area.theme, connection_id) + "\n";
        }

        // Ownership info
        output += "\n" + olc::c_label("--- Permissions ---", connection_id) + "\n";
        if area.owner == "" {
            output += olc::c_label("Owner: ", connection_id) + "(none - any builder can edit)\n";
        } else {
            output += olc::c_label("Owner: ", connection_id) + olc::c_value(area.owner, connection_id) + "\n";
        }
        output += olc::c_label("Permission: ", connection_id) + olc::c_value(area.permission_level, connection_id) + "\n";

        let trustees = area.trusted_builders;
        if trustees.len() > 0 {
            let label = olc::c_label("Trusted Builders: ", connection_id);
            let tlist = "";
            for i in 0..trustees.len() {
                if i > 0 { tlist += ", "; }
                tlist += trustees[i];
            }
            output += label + tlist + "\n";
        }

        // Combat zone type
        let zone = get_area_combat_zone(area.id);
        let zone_desc = if zone == "pve" { "PvE (attack mobiles only)" } else if zone == "safe" { "Safe (no combat)" } else { "PvP (attack mobiles and players)" };
        output += "\n" + olc::c_label("Combat Zone: ", connection_id) + olc::c_value("[" + zone.to_upper() + "]", connection_id) + " " + zone_desc + "\n";

        // Area flags
        output += "\n" + olc::c_label("--- Area Flags ---", connection_id) + "\n";
        let area_flags = area.flags;
        let climate_status = if area_flags.climate_controlled { "[ON]" } else { "[off]" };
        output += olc::c_label("climate_controlled: ", connection_id) + climate_status + "  (rooms inherit unless overridden)\n";

        let rooms = get_rooms_in_area(area.id);
        let rooms_label = olc::c_label("Rooms in area: ", connection_id);
        let rooms_val = olc::c_value("" + rooms.len(), connection_id);
        output += "\n" + rooms_label + rooms_val + "\n";
        let cmd_label = olc::c_label("Commands:", connection_id);
        output += "\n" + cmd_label + "\n";
        output += "  aedit name <text>\n";
        output += "  aedit desc <text>\n";
        output += "  aedit prefix <text>\n";
        output += "  aedit theme <text>\n";
        output += "  aedit levels <min> <max>\n";
        output += "  aedit owner <name|clear>\n";
        output += "  aedit permission <owner_only|trusted|all_builders>\n";
        output += "  aedit trust <name>\n";
        output += "  aedit untrust <name>\n";
        output += "  aedit trustees\n";
        output += "  aedit forage\n";
        output += "  aedit zone [pve|safe|pvp]\n";
        output += "  aedit flags [climate_controlled] [on|off]\n";
        output += "  aedit default_flags [<flag>] [on|off]  (template flags for new rooms)\n";
        output += "  aedit immigration [...]  (migrant spawning config)";

        // Immigration summary block
        output += "\n\n" + olc::c_label("--- Migrant Immigration ---", connection_id) + "\n";
        let imm_status = if area.immigration_enabled { "[ON]" } else { "[off]" };
        output += olc::c_label("enabled:   ", connection_id) + imm_status + "\n";
        output += olc::c_label("arrival:   ", connection_id) + olc::c_value(area.immigration_room_vnum, connection_id) + "\n";
        output += olc::c_label("names:     ", connection_id) + olc::c_value(area.immigration_name_pool, connection_id) + "\n";
        output += olc::c_label("visuals:   ", connection_id) + olc::c_value(area.immigration_visual_profile, connection_id) + "\n";
        output += olc::c_label("interval:  ", connection_id) + olc::c_value(format_migration_interval(area), connection_id) + "\n";
        output += olc::c_label("max/check: ", connection_id) + olc::c_value("" + area.migration_max_per_check, connection_id);
        let var_summary = format_variations_inline(area);
        if var_summary != "" {
            output += "\n" + olc::c_label("variations:", connection_id) + " " + olc::c_value(var_summary, connection_id);
        }
        let mcounts = count_migrants_in_area(area.prefix);
        output += "\n" + olc::c_label("current:   ", connection_id) + olc::c_value(format_migrant_counts(mcounts), connection_id);

        send_client_message(connection_id, output);
        return;
    }

    // Handle subcommands - collect remaining args (filter empty parts from double spaces)
    let remaining = [];
    let value_start = subcommand_start + 1;
    for i in value_start..parts.len() {
        if parts[i] != "" {
            remaining.push(parts[i]);
        }
    }
    let value = remaining.join(" ");

    // ========== Basic Property Subcommands ==========

    if subcommand == "name" {
        if value == "" {
            send_client_message(connection_id, "Usage: aedit name <text>");
            return;
        }
        if set_area_name(area.id, value) {
            send_client_message(connection_id, "Area name set to: " + value);
        } else {
            send_client_message(connection_id, "Error: Failed to update area name.");
        }
        return;
    }

    if subcommand == "desc" {
        if value == "" {
            send_client_message(connection_id, "Usage: aedit desc <text>");
            return;
        }
        if set_area_description(area.id, value) {
            send_client_message(connection_id, "Area description set to: " + value);
        } else {
            send_client_message(connection_id, "Error: Failed to update area description.");
        }
        return;
    }

    if subcommand == "prefix" {
        if value == "" {
            send_client_message(connection_id, "Usage: aedit prefix <text>");
            return;
        }
        if set_area_prefix(area.id, value) {
            send_client_message(connection_id, "Area prefix set to: " + value);
        } else {
            send_client_message(connection_id, "Error: Failed to update area prefix.");
        }
        return;
    }

    if subcommand == "theme" {
        if value == "" {
            send_client_message(connection_id, "Usage: aedit theme <text>");
            return;
        }
        if set_area_theme(area.id, value) {
            send_client_message(connection_id, "Area theme set to: " + value);
        } else {
            send_client_message(connection_id, "Error: Failed to update area theme.");
        }
        return;
    }

    if subcommand == "levels" {
        if remaining.len() < 2 {
            send_client_message(connection_id, "Usage: aedit levels <min> <max>");
            return;
        }
        let min = parse_int(remaining[0]);
        let max = parse_int(remaining[1]);
        if min < 0 || max < 0 {
            send_client_message(connection_id, "Level values must be non-negative integers.");
            return;
        }
        if min > max {
            send_client_message(connection_id, "Minimum level cannot be greater than maximum.");
            return;
        }
        if set_area_levels(area.id, min, max) {
            send_client_message(connection_id, "Level range set to: " + min + " - " + max);
        } else {
            send_client_message(connection_id, "Error: Failed to update level range.");
        }
        return;
    }

    // ========== Ownership Subcommands ==========

    if subcommand == "owner" {
        if remaining.len() < 1 {
            send_client_message(connection_id, "Usage: aedit owner <name|clear>");
            return;
        }
        let owner_arg = remaining[0];
        if owner_arg.to_lower() == "clear" {
            if set_area_owner(area.id, "") {
                send_client_message(connection_id, "Area owner cleared. Any builder can now edit.");
            } else {
                send_client_message(connection_id, "Error: Failed to clear area owner.");
            }
        } else {
            if set_area_owner(area.id, owner_arg) {
                send_client_message(connection_id, "Area owner set to: " + owner_arg);
            } else {
                send_client_message(connection_id, "Error: Failed to set area owner.");
            }
        }
        return;
    }

    if subcommand == "permission" {
        if remaining.len() < 1 {
            let msg = "Usage: aedit permission <level>\n";
            msg += "Levels:\n";
            msg += "  owner_only  - Only the owner can edit\n";
            msg += "  trusted     - Owner and trusted builders can edit\n";
            msg += "  all_builders - Any builder can edit (default)";
            send_client_message(connection_id, msg);
            return;
        }
        let level = remaining[0].to_lower();
        if set_area_permission(area.id, level) {
            send_client_message(connection_id, "Permission level set to: " + level);
        } else {
            send_client_message(connection_id, "Error: Invalid permission level. Use: owner_only, trusted, all_builders");
        }
        return;
    }

    if subcommand == "trust" {
        if remaining.len() < 1 {
            send_client_message(connection_id, "Usage: aedit trust <character_name>");
            return;
        }
        let name = remaining[0];
        if add_area_trustee(area.id, name) {
            send_client_message(connection_id, name + " added to trusted builders.");
        } else {
            send_client_message(connection_id, "Error: Could not add trustee (already trusted or error).");
        }
        return;
    }

    if subcommand == "untrust" {
        if remaining.len() < 1 {
            send_client_message(connection_id, "Usage: aedit untrust <character_name>");
            return;
        }
        let name = remaining[0];
        if remove_area_trustee(area.id, name) {
            send_client_message(connection_id, name + " removed from trusted builders.");
        } else {
            send_client_message(connection_id, "Error: Could not remove trustee (not in list or error).");
        }
        return;
    }

    if subcommand == "trustees" {
        let trustees = area.trusted_builders;
        if trustees.len() == 0 {
            send_client_message(connection_id, "No trusted builders for this area.");
        } else {
            let output = "Trusted builders:\n";
            for name in trustees {
                output += "  - " + name + "\n";
            }
            send_client_message(connection_id, output);
        }
        return;
    }

    // ========== Forage Table Subcommands ==========

    if subcommand == "forage" {
        handle_forage_subcommand(area, remaining, connection_id);
        return;
    }

    // ========== Combat Zone Flag Subcommands ==========

    if subcommand == "zone" {
        // Set combat zone type
        if value == "" {
            // Show current zone and cycle options
            let current = get_area_combat_zone(area.id);
            send_client_message(connection_id, "Current combat zone: " + current.to_upper() + "\nUsage: aedit zone <pve|safe|pvp>\n  pve  - Players can attack mobiles only (default)\n  safe - No combat allowed\n  pvp  - Players can attack mobiles and other players");
            return;
        }

        let val_lower = value.to_lower();
        let is_valid_zone = val_lower == "pve";
        if !is_valid_zone { is_valid_zone = val_lower == "safe"; }
        if !is_valid_zone { is_valid_zone = val_lower == "pvp"; }

        if !is_valid_zone {
            send_client_message(connection_id, "Invalid zone type: " + value + "\nValid types: pve, safe, pvp");
            return;
        }

        if !set_area_combat_zone(area.id, val_lower) {
            send_client_message(connection_id, "Failed to set combat zone.");
            return;
        }

        let desc = "PvE (attack mobiles only)";
        if val_lower == "safe" { desc = "Safe (no combat)"; }
        if val_lower == "pvp" { desc = "PvP (attack mobiles and players)"; }
        send_client_message(connection_id, "Combat zone set to " + val_lower.to_upper() + " for area '" + area.name + "'.\n" + desc + "\nNote: Rooms inherit this unless they have their own zone set.");
        return;
    }

    if subcommand == "flags" {
        handle_flags_subcommand(area, remaining, connection_id);
        return;
    }

    if subcommand == "default_flags" {
        handle_default_room_flags_subcommand(area, remaining, connection_id);
        return;
    }

    if subcommand == "immigration" {
        handle_immigration_subcommand(area, remaining, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
}

// === Immigration subcommand handler ===
//   aedit immigration                → show config
//   aedit immigration on|off
//   aedit immigration room <vnum>
//   aedit immigration namepool <name>
//   aedit immigration visuals <name>
//   aedit immigration interval <1-30>
//   aedit immigration max <n>
//   aedit immigration workhours <start> <end>
//   aedit immigration pay <gold>
//   aedit immigration gold <min> <max>
//   aedit immigration guardpay <gold/hour>
//   aedit immigration healerpay <gold/hour>
//   aedit immigration scavengerpay <gold/hour>
//   aedit immigration clear_sim
fn handle_immigration_subcommand(area, args, connection_id) {
    if args.len() < 1 {
        let out = "=== Immigration Config ===\n";
        let status = if area.immigration_enabled { "[ON]" } else { "[off]" };
        let out2 = out + "enabled:    " + status + "\n";
        let out3 = out2 + "arrival:    " + area.immigration_room_vnum + "\n";
        let out4 = out3 + "namepool:   " + area.immigration_name_pool + "\n";
        let out5 = out4 + "visuals:    " + area.immigration_visual_profile + "\n";
        let out6 = out5 + "interval:   " + format_migration_interval(area) + "\n";
        let out7 = out6 + "max/check:  " + area.migration_max_per_check + "\n";
        let mcounts = count_migrants_in_area(area.prefix);
        let out8 = out7 + "current:    " + format_migrant_counts(mcounts) + "\n";
        let gold_line = "starting gold: " + area.migrant_starting_gold_min + "-" + area.migrant_starting_gold_max + " gold\n";
        let wages_line = "role wages/hr: guard=" + area.guard_wage_per_hour
            + " healer=" + area.healer_wage_per_hour
            + " scavenger=" + area.scavenger_wage_per_hour + "\n";
        let variations_summary = format_variations_summary(area);
        let usage = "\nSubcommands:\n" +
            "  immigration on|off\n" +
            "  immigration room <vnum>\n" +
            "  immigration namepool <name>   (e.g. generic, japan)\n" +
            "  immigration visuals <name>    (e.g. human)\n" +
            "  immigration interval <1-30>\n" +
            "  immigration max <n>\n" +
            "  immigration workhours <start> <end>\n" +
            "  immigration pay <gold>          (per-shift wage at work_room)\n" +
            "  immigration gold <min> <max>    (starting purse range for new migrants)\n" +
            "  immigration guardpay <gold/hr>  (area-treasury wages for guard variation)\n" +
            "  immigration healerpay <gold/hr> (patient visit income for healer variation)\n" +
            "  immigration scavengerpay <gold/hr> (scrounging income for scavenger variation)\n" +
            "  immigration clear_sim\n" +
            "  immigration variations [...] (per-role spawn chances)\n" +
            "  immigration family_chances [...] (parent+child, sibling_pair)\n" +
            "  immigration list              (show migrants who've moved in)";
        send_client_message(connection_id, out8 + gold_line + wages_line + variations_summary + usage);
        return;
    }

    let sub = args[0].to_lower();

    if sub == "on" {
        set_area_immigration_enabled(area.id, true);
        send_client_message(connection_id, "Immigration enabled for area '" + area.name + "'.");
        return;
    }
    if sub == "off" {
        set_area_immigration_enabled(area.id, false);
        send_client_message(connection_id, "Immigration disabled for area '" + area.name + "'.");
        return;
    }

    if sub == "room" {
        if args.len() < 2 {
            send_client_message(connection_id, "Usage: aedit immigration room <vnum>");
            return;
        }
        if resolve_room_id(args[1]) == "" {
            send_client_message(
                connection_id,
                "No room with vnum '" + args[1] + "' exists. " +
                "Use `rlist <keyword>` to find the right vnum."
            );
            return;
        }
        set_area_immigration_room(area.id, args[1]);
        send_client_message(connection_id, "Immigration arrival room set to '" + args[1] + "'.");
        return;
    }

    if sub == "namepool" {
        if args.len() < 2 {
            let pools = list_name_pools();
            send_client_message(connection_id, "Usage: aedit immigration namepool <name>\nAvailable pools: " + pools);
            return;
        }
        if !is_valid_name_pool(args[1]) {
            let pools = list_name_pools();
            send_client_message(connection_id, "Unknown name pool '" + args[1] + "'.\nAvailable pools: " + pools);
            return;
        }
        set_area_immigration_name_pool(area.id, args[1]);
        send_client_message(connection_id, "Immigration name pool set to '" + args[1] + "'.");
        return;
    }

    if sub == "visuals" {
        if args.len() < 2 {
            let profiles = list_visual_profiles();
            send_client_message(connection_id, "Usage: aedit immigration visuals <name>\nAvailable profiles: " + profiles);
            return;
        }
        if !is_valid_visual_profile(args[1]) {
            let profiles = list_visual_profiles();
            send_client_message(connection_id, "Unknown visual profile '" + args[1] + "'.\nAvailable profiles: " + profiles);
            return;
        }
        set_area_immigration_visual_profile(area.id, args[1]);
        send_client_message(connection_id, "Immigration visual profile set to '" + args[1] + "'.");
        return;
    }

    if sub == "interval" {
        if args.len() < 2 {
            send_client_message(connection_id, "Usage: aedit immigration interval <1-30>");
            return;
        }
        let days = parse_int(args[1]);
        if days < 1 || days > 30 {
            send_client_message(connection_id, "Interval must be between 1 and 30 game days.");
            return;
        }
        set_area_migration_interval_days(area.id, days);
        send_client_message(connection_id, "Migration interval set to " + days + " game days.");
        return;
    }

    if sub == "max" {
        if args.len() < 2 {
            send_client_message(connection_id, "Usage: aedit immigration max <n>");
            return;
        }
        let n = parse_int(args[1]);
        if n < 0 {
            send_client_message(connection_id, "Max per check must be non-negative.");
            return;
        }
        set_area_migration_max_per_check(area.id, n);
        send_client_message(connection_id, "Max migrants per check set to " + n + ".");
        return;
    }

    if sub == "workhours" {
        if args.len() < 3 {
            send_client_message(connection_id, "Usage: aedit immigration workhours <start> <end>");
            return;
        }
        let start = parse_int(args[1]);
        let end = parse_int(args[2]);
        set_area_migrant_sim_work_hours(area.id, start, end);
        send_client_message(connection_id, "Migrant work hours set to " + start + "-" + end + ".");
        return;
    }

    if sub == "pay" {
        if args.len() < 2 {
            send_client_message(connection_id, "Usage: aedit immigration pay <gold>");
            return;
        }
        let pay = parse_int(args[1]);
        set_area_migrant_sim_work_pay(area.id, pay);
        send_client_message(connection_id, "Migrant work pay set to " + pay + " gold.");
        return;
    }

    if sub == "gold" {
        if args.len() < 3 {
            send_client_message(
                connection_id,
                "Usage: aedit immigration gold <min> <max>\n" +
                "  Starting purse range. Set both to 0 for the legacy broke-on-arrival default."
            );
            return;
        }
        let lo = parse_int(args[1]);
        let hi = parse_int(args[2]);
        if lo < 0 || hi < 0 {
            send_client_message(connection_id, "Gold values must be non-negative.");
            return;
        }
        set_area_migrant_starting_gold(area.id, lo, hi);
        send_client_message(connection_id, "Migrant starting gold range set to " + lo + "-" + hi + ".");
        return;
    }

    if sub == "guardpay" || sub == "healerpay" || sub == "scavengerpay" {
        let role = if sub == "guardpay" { "guard" } else if sub == "healerpay" { "healer" } else { "scavenger" };
        if args.len() < 2 {
            send_client_message(
                connection_id,
                "Usage: aedit immigration " + sub + " <gold/hour>\n" +
                "  Hourly area-treasury wage for migrants of variation '" + role + "'.\n" +
                "  Set to 0 to disable. Paid anywhere in this area (scavengers must be away from home)."
            );
            return;
        }
        let gold = parse_int(args[1]);
        if gold < 0 {
            send_client_message(connection_id, "Wage must be non-negative.");
            return;
        }
        set_area_role_wage(area.id, role, gold);
        send_client_message(connection_id, "Hourly " + role + " wage set to " + gold + " gold.");
        return;
    }

    if sub == "clear_sim" {
        clear_area_migrant_sim_defaults(area.id);
        send_client_message(connection_id, "Migrant sim defaults cleared.");
        return;
    }

    if sub == "variations" {
        handle_immigration_variations_subcommand(area, args, connection_id);
        return;
    }

    if sub == "family_chances" || sub == "families" {
        handle_immigration_family_chances_subcommand(area, args, connection_id);
        return;
    }

    if sub == "list" {
        handle_immigration_list_subcommand(area, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown immigration subcommand: " + sub);
}

// === Immigration family-chance sub-handler ===
//   aedit immigration family_chances                              → show
//   aedit immigration family_chances <shape> <0.0-1.0>            → set
// `shape`: parent_child | sibling_pair
fn handle_immigration_family_chances_subcommand(area, args, connection_id) {
    let known_shapes = ["parent_child", "sibling_pair"];
    let shapes_joined = "parent_child, sibling_pair";

    if args.len() < 2 {
        let pc_pct = (area.immigration_family_parent_child_chance * 100.0).to_int();
        let sp_pct = (area.immigration_family_sibling_pair_chance * 100.0).to_int();
        let out = "=== Immigration Family Chances ===\n" +
            "  parent_child:  " + pc_pct + "%\n" +
            "  sibling_pair:  " + sp_pct + "%\n" +
            "\nUsage:\n" +
            "  immigration family_chances <shape> <0.0-1.0>\n" +
            "Shapes: " + shapes_joined;
        send_client_message(connection_id, out);
        return;
    }

    let shape = args[1].to_lower();
    if args.len() < 3 {
        send_client_message(connection_id, "Usage: aedit immigration family_chances " + shape + " <0.0-1.0>");
        return;
    }

    let shape_known = false;
    for known in known_shapes {
        if known == shape { shape_known = true; }
    }
    if !shape_known {
        send_client_message(connection_id, "Unknown shape '" + shape + "'. Known: " + shapes_joined);
        return;
    }

    let chance = parse_float(args[2]);
    if chance == () || chance < 0.0 || chance > 1.0 {
        send_client_message(connection_id, "Chance must be a number between 0.0 and 1.0 (e.g. 0.1 = 10%).");
        return;
    }

    if !set_area_immigration_family_chance(area.id, shape, chance) {
        send_client_message(connection_id, "Failed to update '" + shape + "' chance.");
        return;
    }

    let pct = (chance * 100.0).to_int();
    send_client_message(connection_id, "Immigration family '" + shape + "' chance set to " + pct + "%.");
}

// Resolve a room vnum into "vnum (title)" — falls back to the vnum alone if lookup fails.
fn label_room_vnum(vnum) {
    if vnum == "" { return "?"; }
    let r = get_room_by_vnum(vnum);
    if r == () { return vnum; }
    return vnum + " (" + r.title + ")";
}

// Resolve a room id into "vnum (title)" — falls back to "?" if unknown.
fn label_room_id(room_id) {
    if room_id == "" { return "?"; }
    let r = get_room_data(room_id);
    if r == () { return "?"; }
    let v = r.vnum;
    if v == "" { return r.title; }
    return v + " (" + r.title + ")";
}

// === aedit immigration list ===
// Lists every migrant currently belonging to the area, grouped by role.
fn handle_immigration_list_subcommand(area, connection_id) {
    let migrants = [];
    let all = list_all_mobiles();
    for m in all {
        if is_area_migrant(m.vnum, area.prefix) {
            migrants.push(m);
        }
    }
    if migrants.len() == 0 {
        send_client_message(connection_id, "No migrants have moved into '" + area.name + "' yet.");
        return;
    }
    let counts = count_migrants_in_area(area.prefix);
    let out = "=== Migrants in '" + area.name + "': " + format_migrant_counts(counts) + " ===\n";
    let max_rows = 50;
    let shown = 0;
    for m in migrants {
        if shown >= max_rows { break; }
        let role = migrant_role(m.vnum);
        let home = label_room_vnum(m.resident_of);
        let cur = label_room_id(m.current_room_id);
        out = out + "  " + m.name + " [" + role + "] home=" + home + " at=" + cur + "\n";
        shown += 1;
    }
    if migrants.len() > max_rows {
        out = out + "  ... and " + (migrants.len() - max_rows) + " more\n";
    }
    send_client_message(connection_id, out);
}

// === Immigration variations sub-handler ===
//   aedit immigration variations                  → list per-role chances
//   aedit immigration variations <role> <0.0-1.0> → set role's chance
//
// Adding a new role: append it to known_roles below + getter in
// src/script/mod.rs + match arm in set_area_immigration_variation_chance.
fn handle_immigration_variations_subcommand(area, args, connection_id) {
    let known_roles = ["guard", "healer", "scavenger"];
    let roles_joined = "guard, healer, scavenger";

    if args.len() < 2 {
        let out = "=== Immigration Variations ===\n";
        let lines = out
            + format_variation_line("guard", area.immigration_guard_chance)
            + format_variation_line("healer", area.immigration_healer_chance)
            + format_variation_line("scavenger", area.immigration_scavenger_chance);
        let usage = "\nUsage:\n" +
            "  immigration variations <role> <0.0-1.0>\n" +
            "Roles: " + roles_joined;
        send_client_message(connection_id, lines + usage);
        return;
    }

    let role = args[1].to_lower();
    if args.len() < 3 {
        send_client_message(connection_id, "Usage: aedit immigration variations " + role + " <0.0-1.0>");
        return;
    }

    let role_known = false;
    for known in known_roles {
        if known == role { role_known = true; }
    }
    if !role_known {
        send_client_message(connection_id, "Unknown role '" + role + "'. Known: " + roles_joined);
        return;
    }

    let chance = parse_float(args[2]);
    if chance == () || chance < 0.0 || chance > 1.0 {
        send_client_message(connection_id, "Chance must be a number between 0.0 and 1.0 (e.g. 0.1 = 10%).");
        return;
    }

    if !set_area_immigration_variation_chance(area.id, role, chance) {
        send_client_message(connection_id, "Failed to update '" + role + "' chance.");
        return;
    }

    let pct = (chance * 100.0).to_int();
    send_client_message(connection_id, "Immigration '" + role + "' chance set to " + pct + "%.");
}

fn format_variation_line(role, chance) {
    let pct = (chance * 100.0).to_int();
    return "  " + role + ":  " + pct + "%\n";
}

// One-line summary of any non-zero variation chances; empty string if none.
fn format_variations_summary(area) {
    let joined = format_variations_inline(area);
    if joined == "" { return ""; }
    return "variations: " + joined + "\n";
}

// Just the joined "role N%, role N%" body — no prefix, no newline.
fn format_variations_inline(area) {
    let parts = [];
    if area.immigration_guard_chance > 0.0 {
        let pct = (area.immigration_guard_chance * 100.0).to_int();
        parts.push("guard " + pct + "%");
    }
    if area.immigration_healer_chance > 0.0 {
        let pct = (area.immigration_healer_chance * 100.0).to_int();
        parts.push("healer " + pct + "%");
    }
    if area.immigration_scavenger_chance > 0.0 {
        let pct = (area.immigration_scavenger_chance * 100.0).to_int();
        parts.push("scavenger " + pct + "%");
    }
    if parts.len() == 0 { return ""; }
    let joined = parts[0];
    for i in 1..parts.len() { joined += ", " + parts[i]; }
    return joined;
}

// === Forage Table Helper Functions ===

fn is_valid_forage_type(ft) {
    if ft == "city" { return true; }
    if ft == "wilderness" { return true; }
    if ft == "shallow_water" { return true; }
    if ft == "deep_water" { return true; }
    if ft == "underwater" { return true; }
    return false;
}

fn handle_forage_subcommand(area, args, connection_id) {
    // aedit forage [type] [add|remove] [args...]
    // types: city, wilderness, shallow_water, deep_water, underwater
    if args.len() < 1 {
        show_forage_tables(area, connection_id);
        return;
    }

    let forage_type = args[0].to_lower();
    if !is_valid_forage_type(forage_type) {
        show_forage_tables(area, connection_id);
        return;
    }

    if args.len() < 2 {
        show_forage_help(forage_type, connection_id);
        return;
    }

    let action = args[1].to_lower();
    handle_forage_action(area, forage_type, action, args, connection_id);
}

fn handle_forage_action(area, forage_type, action, args, connection_id) {
    if action == "add" {
        handle_forage_add(area, forage_type, args, connection_id);
        return;
    }
    if action == "remove" {
        handle_forage_remove(area, forage_type, args, connection_id);
        return;
    }
    if action == "list" {
        show_forage_list(area, forage_type, connection_id);
        return;
    }
    show_forage_help(forage_type, connection_id);
}

fn handle_forage_add(area, forage_type, args, connection_id) {
    // aedit forage <type> add <vnum> <min_skill> <rarity>
    // args = [type, add, vnum, min_skill, rarity]
    if args.len() < 5 {
        send_client_message(connection_id, "Usage: aedit forage " + forage_type + " add <vnum> <min_skill> <rarity>\nRarities: common, uncommon, rare, legendary");
        return;
    }

    let vnum = args[2];
    let min_skill = parse_int(args[3]);
    let rarity = args[4].to_lower();

    if min_skill == () || min_skill < 0 || min_skill > 10 {
        send_client_message(connection_id, "min_skill must be 0-10");
        return;
    }

    if !is_valid_rarity(rarity) {
        send_client_message(connection_id, "Invalid rarity. Use: common, uncommon, rare, legendary");
        return;
    }

    // Verify the vnum exists as a prototype
    let proto = get_item_by_vnum(vnum);
    if proto == () {
        send_client_message(connection_id, "Warning: No item prototype found for vnum '" + vnum + "'. Entry added anyway.");
    }

    let success = call_forage_add(area.id, forage_type, vnum, min_skill, rarity);

    if success {
        let msg = "Added " + vnum + " to " + forage_type + " forage table";
        msg += " (skill:" + min_skill + ", rarity:" + rarity + ")";
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Failed to add entry (may already exist).");
    }
}

fn handle_forage_remove(area, forage_type, args, connection_id) {
    // aedit forage <type> remove <vnum>
    // args = [type, remove, vnum]
    if args.len() < 3 {
        send_client_message(connection_id, "Usage: aedit forage " + forage_type + " remove <vnum>");
        return;
    }

    let vnum = args[2];

    let success = call_forage_remove(area.id, forage_type, vnum);

    if success {
        send_client_message(connection_id, "Removed " + vnum + " from " + forage_type + " forage table.");
    } else {
        send_client_message(connection_id, "Entry not found or failed to remove.");
    }
}

fn call_forage_add(area_id, forage_type, vnum, min_skill, rarity) {
    if forage_type == "city" { return add_area_city_forage(area_id, vnum, min_skill, rarity); }
    if forage_type == "wilderness" { return add_area_wilderness_forage(area_id, vnum, min_skill, rarity); }
    if forage_type == "shallow_water" { return add_area_shallow_water_forage(area_id, vnum, min_skill, rarity); }
    if forage_type == "deep_water" { return add_area_deep_water_forage(area_id, vnum, min_skill, rarity); }
    if forage_type == "underwater" { return add_area_underwater_forage(area_id, vnum, min_skill, rarity); }
    return false;
}

fn call_forage_remove(area_id, forage_type, vnum) {
    if forage_type == "city" { return remove_area_city_forage(area_id, vnum); }
    if forage_type == "wilderness" { return remove_area_wilderness_forage(area_id, vnum); }
    if forage_type == "shallow_water" { return remove_area_shallow_water_forage(area_id, vnum); }
    if forage_type == "deep_water" { return remove_area_deep_water_forage(area_id, vnum); }
    if forage_type == "underwater" { return remove_area_underwater_forage(area_id, vnum); }
    return false;
}

fn is_valid_rarity(rarity) {
    if rarity == "common" { return true; }
    if rarity == "uncommon" { return true; }
    if rarity == "rare" { return true; }
    if rarity == "legendary" { return true; }
    return false;
}

fn show_forage_section(table, label, output) {
    output += "\n--- " + label + " ---\n";
    if table.len() == 0 {
        output += "  (none)\n";
    } else {
        for entry in table {
            output += build_forage_entry_line(entry);
        }
    }
    return output;
}

fn show_forage_tables(area, connection_id) {
    let output = "=== Forage Tables for " + area.name + " ===\n";

    output = show_forage_section(area.city_forage_table, "City Forage (rooms with 'city' flag)", output);
    output = show_forage_section(area.wilderness_forage_table, "Wilderness Forage (rooms with 'dirt_floor' flag)", output);
    output = show_forage_section(area.shallow_water_forage_table, "Shallow Water Forage (rooms with 'shallow_water' flag)", output);
    output = show_forage_section(area.deep_water_forage_table, "Deep Water Forage (rooms with 'deep_water' flag)", output);
    output = show_forage_section(area.underwater_forage_table, "Underwater Forage (rooms with 'underwater' flag)", output);

    output += "\nCommands:\n";
    output += "  aedit forage <type> add <vnum> <skill> <rarity>\n";
    output += "  aedit forage <type> remove <vnum>\n";
    output += "  Types: city, wilderness, shallow_water, deep_water, underwater";

    send_client_message(connection_id, output);
}

fn build_forage_entry_line(entry) {
    let output = "  " + entry.vnum;
    // Pad to 20 chars
    let pad_needed = 20 - entry.vnum.len();
    if pad_needed < 1 { pad_needed = 1; }
    for i in 0..pad_needed { output += " "; }

    output += "skill:" + entry.min_skill + "  [" + entry.rarity + "]";

    // Try to get weight from prototype
    let proto = get_item_by_vnum(entry.vnum);
    if proto != () {
        output += "  (w:" + proto.weight + ")";
    }
    output += "\n";
    return output;
}

fn get_forage_table_by_type(area, forage_type) {
    if forage_type == "city" { return area.city_forage_table; }
    if forage_type == "shallow_water" { return area.shallow_water_forage_table; }
    if forage_type == "deep_water" { return area.deep_water_forage_table; }
    if forage_type == "underwater" { return area.underwater_forage_table; }
    return area.wilderness_forage_table;
}

fn show_forage_list(area, forage_type, connection_id) {
    let output = "=== " + forage_type + " Forage Table for " + area.name + " ===\n\n";

    let table = get_forage_table_by_type(area, forage_type);

    if table.len() == 0 {
        output += "(no entries)\n";
    } else {
        for entry in table {
            output += build_forage_entry_line(entry);
        }
    }

    send_client_message(connection_id, output);
}

fn show_forage_help(forage_type, connection_id) {
    let output = "Usage:\n";
    output += "  aedit forage " + forage_type + " add <vnum> <skill> <rarity>\n";
    output += "  aedit forage " + forage_type + " remove <vnum>\n";
    output += "  aedit forage " + forage_type + " list\n";
    output += "\nRarities: common, uncommon, rare, legendary\n";
    output += "Skill: 0-10 (minimum foraging skill level required)";
    send_client_message(connection_id, output);
}

// === Area Flags Helper Functions ===

fn bool_to_status(val) {
    if val { return "ON"; }
    return "OFF";
}

fn handle_climate_flag(area, args, connection_id) {
    // Toggle mode if no value provided
    if args.len() < 2 {
        let new_value = !area.flags.climate_controlled;
        if !set_area_flag(area.id, "climate_controlled", new_value) {
            send_client_message(connection_id, "Failed to toggle flag.");
            return;
        }
        send_client_message(connection_id, "Area climate_controlled toggled to " + bool_to_status(new_value) + ".");
        return;
    }

    // Explicit set mode
    let val = args[1].to_lower();
    if !olc::is_on_value(val) && !olc::is_off_value(val) {
        send_client_message(connection_id, "Usage: aedit flags climate_controlled [on|off]");
        return;
    }
    let new_value = olc::is_on_value(val);
    if !set_area_flag(area.id, "climate_controlled", new_value) {
        send_client_message(connection_id, "Failed to set flag.");
        return;
    }
    send_client_message(connection_id, "Area climate_controlled set to " + bool_to_status(new_value) + ".\nRooms will inherit this unless they override it.");
}

fn show_area_flags(area, connection_id) {
    let flags = area.flags;
    let output = olc::c_header("=== Area Flags ===", connection_id) + "\n";
    let climate_status = "[off]";
    if flags.climate_controlled { climate_status = "[ON]"; }
    output += "climate_controlled: " + climate_status + "\n";
    output += "  Rooms inherit this unless they have their own flag set.\n\n";
    output += "Usage: aedit flags climate_controlled [on|off]";
    send_client_message(connection_id, output);
}

fn handle_flags_subcommand(area, args, connection_id) {
    if args.len() < 1 {
        show_area_flags(area, connection_id);
        return;
    }

    let flag_name = args[0].to_lower();

    if flag_name == "climate_controlled" {
        handle_climate_flag(area, args, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown flag: " + flag_name + "\nAvailable flags: climate_controlled");
}

// === default_flags subcommand ===
// RoomFlags template applied to every room newly-created in this area.
// Existing rooms are not touched when this changes.
//   aedit default_flags                     → show which defaults are on
//   aedit default_flags <flag>              → toggle the default
//   aedit default_flags <flag> on|off       → set the default
fn known_default_room_flags() {
    return [
        "dark", "no_mob", "indoors", "underwater",
        "climate_controlled", "always_hot", "always_cold", "city", "no_windows",
        "difficult_terrain", "dirt_floor",
        "property_storage", "post_office", "bank", "garden", "spawn_point",
        "shallow_water", "deep_water", "liveable",
        "private_room", "tunnel", "death", "no_magic",
        "soundproof", "notrack",
    ];
}

fn show_area_default_room_flags(area, connection_id) {
    let flags = area.default_room_flags;
    let output = olc::c_header("=== Area Default Room Flags ===", connection_id) + "\n";
    output += "New rooms created in this area start with these flags set.\n";
    output += "Existing rooms are not updated when this changes.\n\n";

    let on_list = [];
    for name in known_default_room_flags() {
        if get_area_default_room_flag(area.id, name) {
            on_list.push(name);
        }
    }
    if on_list.len() == 0 {
        output += "  (all off)\n";
    } else {
        for name in on_list {
            output += "  [ON] " + name + "\n";
        }
    }
    output += "\nUsage: aedit default_flags <flag> [on|off]";
    send_client_message(connection_id, output);
}

fn handle_default_room_flags_subcommand(area, args, connection_id) {
    if args.len() < 1 {
        show_area_default_room_flags(area, connection_id);
        return;
    }

    let flag_name = args[0].to_lower();
    let valid = false;
    for name in known_default_room_flags() {
        if flag_name == name { valid = true; }
    }
    if !valid {
        let msg = "Unknown flag: " + flag_name + "\nAvailable flags: ";
        let first = true;
        for name in known_default_room_flags() {
            if first { msg += name; first = false; }
            else { msg += ", " + name; }
        }
        send_client_message(connection_id, msg);
        return;
    }

    let new_value = false;
    if args.len() < 2 {
        // Toggle
        new_value = !get_area_default_room_flag(area.id, flag_name);
    } else {
        let val = args[1].to_lower();
        if !olc::is_on_value(val) && !olc::is_off_value(val) {
            send_client_message(connection_id, "Usage: aedit default_flags " + flag_name + " [on|off]");
            return;
        }
        new_value = olc::is_on_value(val);
    }

    if !set_area_default_room_flag(area.id, flag_name, new_value) {
        send_client_message(connection_id, "Failed to set default flag.");
        return;
    }
    send_client_message(
        connection_id,
        "Area default_flags." + flag_name + " set to " + bool_to_status(new_value) +
        ".\nApplies to rooms created in this area from now on."
    );
}
