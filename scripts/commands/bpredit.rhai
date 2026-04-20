// scripts/commands/bpredit.rhai
// Buy Preset Editor
// Usage: bpredit <subcommand> [args...]
// @description Create and manage shop buy presets
// @category building

// Import shared OLC helpers
import "olc_helpers" as olc;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }
    if !char.is_builder && !char.is_admin {
        send_client_message(connection_id, "You must be a builder to use this command.");
        return;
    }

    if args == "" {
        show_usage(connection_id);
        return;
    }

    let parts = args.split(" ");
    let subcmd = parts[0].to_lower();

    // Dispatch top-level commands first
    if dispatch_toplevel(subcmd, parts, connection_id) {
        return;
    }

    // All other subcommands require a vnum as first arg
    let preset = get_shop_preset(subcmd);
    if preset == () {
        send_client_message(connection_id, "Buy preset '" + subcmd + "' not found. Use 'bpredit list' to see presets.");
        return;
    }

    if parts.len() < 2 {
        show_preset_detail(preset, connection_id);
        return;
    }

    dispatch_preset_action(preset, parts, connection_id);
}

fn dispatch_toplevel(subcmd, parts, connection_id) {
    if subcmd == "list" { handle_list(connection_id); return true; }
    if subcmd == "create" { handle_create(parts, connection_id); return true; }
    if subcmd == "delete" { handle_delete(parts, connection_id); return true; }
    return false;
}

fn dispatch_preset_action(preset, parts, connection_id) {
    let action = parts[1].to_lower();
    if action == "name" { handle_name(preset, parts, connection_id); return; }
    if action == "desc" { handle_desc(preset, parts, connection_id); return; }
    if action == "type" { handle_type(preset, parts, connection_id); return; }
    if action == "category" { handle_category(preset, parts, connection_id); return; }
    if try_value_action(action, preset, parts, connection_id) { return; }
    show_preset_detail(preset, connection_id);
}

fn try_value_action(action, preset, parts, connection_id) {
    if action == "minvalue" { handle_minvalue(preset, parts, connection_id); return true; }
    if action == "maxvalue" { handle_maxvalue(preset, parts, connection_id); return true; }
    return false;
}

fn show_usage(connection_id) {
    let msg = olc::c_header("=== Buy Preset Editor ===", connection_id) + "\n\n";
    msg += "Commands:\n";
    msg += "  bpredit list                           - List all presets\n";
    msg += "  bpredit create <vnum> <name>           - Create a new preset\n";
    msg += "  bpredit <vnum>                         - Show preset details\n";
    msg += "  bpredit <vnum> name <text>             - Set name\n";
    msg += "  bpredit <vnum> desc <text>             - Set description\n";
    msg += "  bpredit <vnum> type add|remove <type>  - Manage buy types\n";
    msg += "  bpredit <vnum> category add|remove <cat> - Manage buy categories\n";
    msg += "  bpredit <vnum> minvalue <amount|clear> - Set/clear min value\n";
    msg += "  bpredit <vnum> maxvalue <amount|clear> - Set/clear max value\n";
    msg += "  bpredit delete <vnum>                  - Delete a preset\n";
    send_client_message(connection_id, msg);
}

fn format_preset_details(p) {
    let details = [];
    if p.buy_types.len() > 0 {
        details.push("types: " + p.buy_types.join(","));
    }
    if p.buy_categories.len() > 0 {
        details.push("cats: " + p.buy_categories.join(","));
    }
    if p.min_value > 0 {
        details.push("min:" + p.min_value);
    }
    if p.max_value > 0 {
        details.push("max:" + p.max_value);
    }
    return details;
}

fn handle_list(connection_id) {
    let presets = list_shop_presets();
    if presets.len() == 0 {
        send_client_message(connection_id, "No buy presets defined. Use 'bpredit create <vnum> <name>' to create one.");
        return;
    }

    let output = olc::c_header("=== Buy Presets ===", connection_id) + "\n\n";
    for p in presets {
        output += "  " + olc::c_value(p.vnum, connection_id) + " - " + p.name;
        let details = format_preset_details(p);
        if details.len() > 0 {
            output += " [" + details.join(", ") + "]";
        }
        output += "\n";
    }
    send_client_message(connection_id, output);
}

fn handle_create(parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: bpredit create <vnum> <name>");
        return;
    }

    let vnum = parts[1].to_lower();
    // Build name from remaining parts
    let name_parts = [];
    let i = 2;
    while i < parts.len() {
        name_parts.push(parts[i]);
        i += 1;
    }
    let name = name_parts.join(" ");

    // Check if vnum already exists
    let existing = get_shop_preset(vnum);
    if existing != () {
        send_client_message(connection_id, "Buy preset '" + vnum + "' already exists.");
        return;
    }

    let preset = new_shop_preset(vnum, name);
    if save_shop_preset(preset) {
        send_client_message(connection_id, "Created buy preset '" + vnum + "' (" + name + ").");
    } else {
        send_client_message(connection_id, "Failed to create preset.");
    }
}

fn handle_delete(parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: bpredit delete <vnum>");
        return;
    }

    let vnum = parts[1].to_lower();
    let preset = get_shop_preset(vnum);
    if preset == () {
        send_client_message(connection_id, "Buy preset '" + vnum + "' not found.");
        return;
    }

    if delete_shop_preset(vnum) {
        send_client_message(connection_id, "Deleted buy preset '" + vnum + "'.");
    } else {
        send_client_message(connection_id, "Failed to delete preset.");
    }
}

fn show_preset_detail(preset, connection_id) {
    let output = olc::c_header("=== Buy Preset: " + preset.vnum + " ===", connection_id) + "\n\n";
    output += olc::c_label("Name: ", connection_id) + preset.name + "\n";
    if preset.description != "" {
        output += olc::c_label("Description: ", connection_id) + preset.description + "\n";
    }
    output += olc::c_label("Buy Types: ", connection_id);
    if preset.buy_types.len() == 0 {
        output += "(none)";
    } else {
        output += preset.buy_types.join(", ");
    }
    output += "\n";
    output += olc::c_label("Buy Categories: ", connection_id);
    if preset.buy_categories.len() == 0 {
        output += "(none)";
    } else {
        output += preset.buy_categories.join(", ");
    }
    output += "\n";
    output += olc::c_label("Min Value: ", connection_id);
    if preset.min_value > 0 {
        output += "" + preset.min_value + " gold";
    } else {
        output += "(none)";
    }
    output += "\n";
    output += olc::c_label("Max Value: ", connection_id);
    if preset.max_value > 0 {
        output += "" + preset.max_value + " gold";
    } else {
        output += "(none)";
    }
    output += "\n";

    output += "\nCommands:\n";
    output += "  bpredit " + preset.vnum + " name <text>\n";
    output += "  bpredit " + preset.vnum + " desc <text>\n";
    output += "  bpredit " + preset.vnum + " type add|remove <type>\n";
    output += "  bpredit " + preset.vnum + " category add|remove <cat>\n";
    output += "  bpredit " + preset.vnum + " minvalue <amount|clear>\n";
    output += "  bpredit " + preset.vnum + " maxvalue <amount|clear>\n";
    send_client_message(connection_id, output);
}

fn handle_name(preset, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " name <text>");
        return;
    }
    let name_parts = [];
    let i = 2;
    while i < parts.len() {
        name_parts.push(parts[i]);
        i += 1;
    }
    preset.name = name_parts.join(" ");
    if save_shop_preset(preset) {
        send_client_message(connection_id, "Name set to: " + preset.name);
    } else {
        send_client_message(connection_id, "Failed to save.");
    }
}

fn handle_desc(preset, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " desc <text>");
        return;
    }
    let desc_parts = [];
    let i = 2;
    while i < parts.len() {
        desc_parts.push(parts[i]);
        i += 1;
    }
    preset.description = desc_parts.join(" ");
    if save_shop_preset(preset) {
        send_client_message(connection_id, "Description set.");
    } else {
        send_client_message(connection_id, "Failed to save.");
    }
}

fn handle_type(preset, parts, connection_id) {
    let valid_types = ["all", "misc", "armor", "weapon", "container", "liquid_container", "food", "key", "gold"];

    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " type add|remove <type>");
        send_client_message(connection_id, "Valid types: " + valid_types.join(", "));
        return;
    }

    let action = parts[2].to_lower();
    let type_name = parts[3].to_lower();

    if action == "add" {
        // Validate
        let type_valid = false;
        for vt in valid_types {
            if vt == type_name { type_valid = true; }
        }
        if !type_valid {
            send_client_message(connection_id, "Invalid type. Valid: " + valid_types.join(", "));
            return;
        }
        let new_types = [];
        for t in preset.buy_types {
            if t.to_lower() != type_name { new_types.push(t); }
        }
        new_types.push(type_name);
        preset.buy_types = new_types;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Added type '" + type_name + "'.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    if action == "remove" {
        let new_types = [];
        let found = false;
        for t in preset.buy_types {
            if t.to_lower() == type_name {
                found = true;
            } else {
                new_types.push(t);
            }
        }
        if !found {
            send_client_message(connection_id, "Type '" + type_name + "' not found.");
            return;
        }
        preset.buy_types = new_types;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Removed type '" + type_name + "'.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " type add|remove <type>");
}

fn handle_category(preset, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " category add|remove <category>");
        return;
    }

    let action = parts[2].to_lower();
    let cat = parts[3].to_lower();

    if action == "add" {
        let new_cats = [];
        for c in preset.buy_categories {
            if c.to_lower() != cat { new_cats.push(c); }
        }
        new_cats.push(cat);
        preset.buy_categories = new_cats;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Added category '" + cat + "'.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    if action == "remove" {
        let new_cats = [];
        let found = false;
        for c in preset.buy_categories {
            if c.to_lower() == cat {
                found = true;
            } else {
                new_cats.push(c);
            }
        }
        if !found {
            send_client_message(connection_id, "Category '" + cat + "' not found.");
            return;
        }
        preset.buy_categories = new_cats;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Removed category '" + cat + "'.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " category add|remove <category>");
}

fn handle_minvalue(preset, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " minvalue <amount|clear>");
        return;
    }

    if parts[2].to_lower() == "clear" {
        preset.min_value = 0;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Min value cleared.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    let val = parse_int(parts[2]);
    if val == () || val < 0 {
        send_client_message(connection_id, "Invalid value. Use a positive number or 'clear'.");
        return;
    }
    preset.min_value = val;
    if save_shop_preset(preset) {
        send_client_message(connection_id, "Min value set to " + val + " gold.");
    } else {
        send_client_message(connection_id, "Failed to save.");
    }
}

fn handle_maxvalue(preset, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: bpredit " + preset.vnum + " maxvalue <amount|clear>");
        return;
    }

    if parts[2].to_lower() == "clear" {
        preset.max_value = 0;
        if save_shop_preset(preset) {
            send_client_message(connection_id, "Max value cleared.");
        } else {
            send_client_message(connection_id, "Failed to save.");
        }
        return;
    }

    let val = parse_int(parts[2]);
    if val == () || val < 0 {
        send_client_message(connection_id, "Invalid value. Use a positive number or 'clear'.");
        return;
    }
    preset.max_value = val;
    if save_shop_preset(preset) {
        send_client_message(connection_id, "Max value set to " + val + " gold.");
    } else {
        send_client_message(connection_id, "Failed to save.");
    }
}
