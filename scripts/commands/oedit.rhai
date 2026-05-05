// scripts/commands/oedit.rhai
// oedit <item_id> [subcommand] or oedit create
// Object Editor for builders

// Import shared OLC helpers
import "olc_helpers" as olc;

// AI helper: handle short help command
fn handle_ai_item_short_help(item, uuid, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> short help <description prompt>");
        return true;
    }
    let prompt = build_string_from_parts(parts, 3);
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: item.name,
        entity_type: item.item_type,
        theme: theme
    };
    let request_id = ai_help_write(connection_id, "item_short", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "item", uuid, "short_desc");
    send_client_message(connection_id, "Generating short description... please wait.");
    return true;
}

// AI helper: handle short rephrase command
fn handle_ai_item_short_rephrase(item, uuid, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if item.short_desc == "" {
        send_client_message(connection_id, "Item has no short description to rephrase.");
        return true;
    }
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: item.name,
        entity_type: item.item_type,
        theme: theme
    };
    let request_id = ai_rephrase(connection_id, "item_short", item.short_desc, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "item", uuid, "short_desc");
    send_client_message(connection_id, "Rephrasing short description... please wait.");
    return true;
}

// AI helper: handle long help command
fn handle_ai_item_long_help(item, uuid, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> long help <description prompt>");
        return true;
    }
    let prompt = build_string_from_parts(parts, 3);
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: item.name,
        entity_type: item.item_type,
        theme: theme
    };
    let request_id = ai_help_write(connection_id, "item_long", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "item", uuid, "long_desc");
    send_client_message(connection_id, "Generating long description... please wait.");
    return true;
}

// AI helper: handle long rephrase command
fn handle_ai_item_long_rephrase(item, uuid, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if item.long_desc == "" {
        send_client_message(connection_id, "Item has no long description to rephrase.");
        return true;
    }
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: item.name,
        entity_type: item.item_type,
        theme: theme
    };
    let request_id = ai_rephrase(connection_id, "item_long", item.long_desc, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "item", uuid, "long_desc");
    send_client_message(connection_id, "Rephrasing long description... please wait.");
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let parts = split_quoted_args(args);

    // Handle create first - oedit create <name>
    if parts.len() >= 1 && parts[0].to_lower() == "create" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: oedit create <name>\nExample: oedit create Rusty Sword");
            return;
        }
        // Join remaining parts as name
        let name = build_string_from_parts(parts, 1);
        let vnum = generate_unique_item_vnum(name);

        let short_desc = "A " + name + " lies here.";
        let item = new_item(name, short_desc, "This item has no description yet.");
        save_item_data(item);  // Save first to persist
        set_item_vnum(item.id, vnum);
        set_item_prototype(item.id, true);

        // Re-fetch to get updated data for display
        let item = get_item_data(item.id);
        send_client_message(connection_id, "Created item prototype with vnum: " + vnum + "\n");
        show_item_properties(item, item.id, connection_id);
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: oedit <item_id|vnum> [subcommand] or oedit create <name>");
        return;
    }

    let item_id = parts[0];
    let item = get_item_data(item_id);

    // If not found by ID, try by vnum
    if item == () {
        item = get_item_by_vnum(item_id);
    }

    // If still not found, try by keyword in current room
    if item == () {
        item = find_item_in_room(char.current_room_id, item_id);
    }

    if item == () {
        send_client_message(connection_id, "Item not found: " + item_id);
        return;
    }

    let subcommand = "";
    if parts.len() >= 2 {
        subcommand = parts[1].to_lower();
    }

    // Show properties if no subcommand
    if subcommand == "" || subcommand == "show" {
        show_item_properties(item, item_id, connection_id);
        return;
    }

    // Handle subcommands
    handle_subcommand(item, item_id, subcommand, parts, char, connection_id);
}

fn handle_subcommand(item, item_id, subcommand, parts, char, connection_id) {
    // Use item.id (UUID) for setter functions, item_id for display (may be vnum)
    let uuid = item.id;

    if subcommand == "name" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: oedit " + item_id + " name <text>");
            return;
        }
        let name = build_string_from_parts(parts, 2);
        set_item_name(uuid, name);
        send_client_message(connection_id, "Name set to: " + name);
        return;
    }

    if subcommand == "short" {
        // Check for AI subcommands first
        if parts.len() >= 3 {
            let subcmd = parts[2].to_lower();
            if subcmd == "help" { handle_ai_item_short_help(item, uuid, parts, connection_id); return; }
            if subcmd == "rephrase" { handle_ai_item_short_rephrase(item, uuid, connection_id); return; }
        }
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: oedit " + item_id + " short <text>\nAI: short help <prompt> | short rephrase");
            return;
        }
        let desc = build_string_from_parts(parts, 2);
        set_item_short_desc(uuid, desc);
        send_client_message(connection_id, "Short description set to: " + desc);
        return;
    }

    if subcommand == "long" {
        // Check for AI subcommands first
        if parts.len() >= 3 {
            let subcmd = parts[2].to_lower();
            if subcmd == "help" { handle_ai_item_long_help(item, uuid, parts, connection_id); return; }
            if subcmd == "rephrase" { handle_ai_item_long_rephrase(item, uuid, connection_id); return; }
        }
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: oedit " + item_id + " long <text>\nAI: long help <prompt> | long rephrase");
            return;
        }
        let desc = build_string_from_parts(parts, 2);
        set_item_long_desc(uuid, desc);
        send_client_message(connection_id, "Long description set.");
        return;
    }

    if subcommand == "keywords" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: oedit " + item_id + " keywords <kw1> <kw2> ...");
            return;
        }
        let kws = collect_parts(parts, 2);
        set_item_keywords(uuid, kws);
        send_client_message(connection_id, "Keywords set: " + olc::join_array(kws));
        return;
    }

    if subcommand == "type" {
        handle_type_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "wear" {
        handle_wear_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "ac" {
        handle_ac_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "protects" {
        handle_protects_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "weight" {
        handle_weight_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "value" {
        handle_value_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "world_max" || subcommand == "worldmax" || subcommand == "world_max_count" {
        handle_world_max_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "flags" {
        show_item_flags(item, uuid, connection_id);
        return;
    }

    if subcommand == "flag" {
        handle_flag_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "category" {
        handle_category_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "teaches" {
        handle_teaches_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "teaches_spell" {
        handle_teaches_spell_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "transport" {
        handle_transport_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "note" {
        handle_note_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "spawn" {
        handle_spawn_subcommand(item, char, connection_id);
        return;
    }

    if subcommand == "damage" {
        handle_damage_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "damtype" {
        handle_damtype_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "twohanded" {
        handle_twohanded_subcommand(uuid, parts, connection_id);
        return;
    }

    if subcommand == "wskill" {
        handle_wskill_subcommand(uuid, parts, connection_id);
        return;
    }

    // Container subcommands
    if subcommand == "capacity" {
        handle_capacity_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "closed" {
        handle_closed_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "locked" {
        handle_locked_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "key" {
        handle_key_subcommand(uuid, parts, connection_id);
        return;
    }

    // Liquid container subcommands
    if subcommand == "liquid" {
        handle_liquid_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "fill" {
        handle_fill_subcommand(uuid, connection_id);
        return;
    }
    if subcommand == "empty" {
        handle_empty_subcommand(uuid, connection_id);
        return;
    }
    if subcommand == "liqpoison" {
        handle_liqpoison_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "liqeffect" {
        handle_liqeffect_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "clearliqeffects" {
        handle_clearliqeffects_subcommand(uuid, connection_id);
        return;
    }

    // Food subcommands
    if subcommand == "nutrition" {
        handle_nutrition_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "spoil" {
        handle_spoil_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "foodpoison" {
        handle_foodpoison_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "foodeffect" {
        handle_foodeffect_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "clearfoodeffects" {
        handle_clearfoodeffects_subcommand(uuid, connection_id);
        return;
    }
    if subcommand == "resetfresh" {
        handle_resetfresh_subcommand(uuid, connection_id);
        return;
    }
    if subcommand == "preservation" {
        handle_preservation_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "spoilage" {
        handle_spoilage_subcommand(uuid, parts, connection_id);
        return;
    }

    // Level and stat subcommands
    if subcommand == "level" {
        handle_level_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "stat" {
        handle_stat_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "insulation" || subcommand == "insul" {
        handle_insulation_subcommand(uuid, parts, connection_id);
        return;
    }

    // Prototype subcommands
    if subcommand == "prototype" {
        handle_prototype_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "vnum" {
        handle_vnum_subcommand(uuid, parts, connection_id);
        return;
    }

    // Trigger subcommands
    if subcommand == "trigger" {
        handle_trigger_subcommand(item, uuid, parts, connection_id);
        return;
    }

    // Vending machine subcommands
    if subcommand == "vending" {
        handle_vending_subcommand(item, uuid, parts, connection_id);
        return;
    }

    // Generic quality subcommand (for fishing rods, bait, etc.)
    if subcommand == "quality" {
        handle_quality_subcommand(uuid, parts, connection_id);
        return;
    }

    // Bait subcommands
    if subcommand == "baituses" {
        handle_baituses_subcommand(uuid, parts, connection_id);
        return;
    }

    // Container weight reduction (for backpacks etc.)
    if subcommand == "weightreduction" || subcommand == "weightreduce" || subcommand == "wr" {
        handle_weightreduction_subcommand(uuid, parts, connection_id);
        return;
    }

    // Medical tool subcommands
    if subcommand == "medical" {
        handle_medical_subcommand(item, uuid, parts, connection_id);
        return;
    }

    // Ammunition subcommands
    if subcommand == "caliber" {
        handle_caliber_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "ammocount" || subcommand == "count" {
        handle_ammocount_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "ammobonus" {
        handle_ammobonus_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "rangedtype" || subcommand == "rtype" {
        handle_rangedtype_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "magsize" {
        handle_magsize_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "loadedammo" {
        handle_loadedammo_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "firemode" {
        handle_firemode_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "firemodes" {
        handle_firemodes_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "noise" || subcommand == "noiselevel" {
        handle_noise_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "ammoeffect" {
        handle_ammoeffect_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "attachment" {
        handle_attachment_subcommand(uuid, parts, connection_id);
        return;
    }

    // Gardening item fields
    if subcommand == "plantproto" {
        handle_plantproto_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "fertduration" {
        handle_fertduration_subcommand(uuid, parts, connection_id);
        return;
    }
    if subcommand == "treats" {
        handle_treats_subcommand(uuid, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
}

fn build_string_from_parts(parts, start_idx) {
    let result = "";
    for i in start_idx..parts.len() {
        if result != "" { result += " "; }
        result += parts[i];
    }
    return result;
}

fn collect_parts(parts, start_idx) {
    let result = [];
    for i in start_idx..parts.len() {
        result.push(parts[i]);
    }
    return result;
}

fn handle_type_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " type <armor|weapon|ammunition|container|liquid_container|food|key|misc>");
        return;
    }
    let type_str = parts[2].to_lower();
    let valid_types = ["armor", "weapon", "ammunition", "misc", "container", "liquid_container", "food", "key"];
    let is_valid = false;
    for t in valid_types {
        if type_str == t { is_valid = true; }
    }
    if !is_valid {
        send_client_message(connection_id, "Invalid type. Use: armor, weapon, ammunition, container, liquid_container, food, key, misc");
        return;
    }
    set_item_type(item_id, type_str);

    // Automatically add "wielded" wear location for weapons
    if type_str == "weapon" {
        set_item_wear_locations(item_id, ["wielded"]);
        send_client_message(connection_id, "Item type set to: " + type_str + " (wear location set to: wielded)");
    } else if type_str == "ammunition" {
        set_item_wear_locations(item_id, ["ready"]);
        send_client_message(connection_id, "Item type set to: " + type_str + " (wear location set to: ready)");
    } else {
        send_client_message(connection_id, "Item type set to: " + type_str);
    }
}

fn handle_wear_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let all_locs = get_all_wear_locations();
        let locs_str = olc::join_array(all_locs);
        send_client_message(connection_id, "Usage: oedit " + item_id + " wear <loc1>, <loc2>, ...\nAvailable: " + locs_str);
        return;
    }
    let raw = build_string_from_parts(parts, 2);
    let locs = [];
    let split = raw.split(",");
    for s in split {
        let loc = s;
        // Remove leading/trailing spaces manually
        while loc.len() > 0 && loc.sub_string(0, 1) == " " {
            loc = loc.sub_string(1);
        }
        while loc.len() > 0 && loc.sub_string(loc.len() - 1) == " " {
            loc = loc.sub_string(0, loc.len() - 1);
        }
        if loc != "" { locs.push(loc); }
    }
    if locs.len() == 0 {
        send_client_message(connection_id, "No valid wear locations provided.");
        return;
    }
    set_item_wear_locations(item_id, locs);
    // Re-fetch to confirm what Rust accepted
    let item = get_item_data(item_id);
    let actual = item.wear_locations;
    if actual.len() > 0 {
        send_client_message(connection_id, "Wear locations set: " + olc::join_array(actual));
    } else {
        let all_locs = get_all_wear_locations();
        send_client_message(connection_id, "No valid wear locations were accepted.\nAvailable: " + olc::join_array(all_locs));
    }
}

fn handle_ac_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " ac <value> (use -1 to clear)");
        return;
    }
    let ac = parse_int(parts[2]);
    if ac == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_armor_class(item_id, ac);
    if ac < 0 {
        send_client_message(connection_id, "Armor class cleared.");
    } else {
        send_client_message(connection_id, "Armor class set to: " + ac);
    }
}

// Helper to join body part from command parts
fn join_body_part_from_parts(parts) {
    let body_part = "";
    for i in 3..parts.len() {
        if body_part != "" { body_part += " "; }
        body_part += parts[i];
    }
    return body_part.to_lower();
}

fn handle_protects_add(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: protects add <body_part>");
        return;
    }
    let body_part = join_body_part_from_parts(parts);
    if add_item_protects(item_id, body_part) {
        let updated = get_item_protects(item_id);
        let msg = "Added protection for: " + body_part + "\nNow protects: " + updated.join(", ");
        send_client_message(connection_id, msg);
    } else {
        let msg = "Invalid body part. Valid: head, neck, torso, left arm, right arm, ";
        msg += "left leg, right leg, left hand, right hand, left foot, right foot";
        send_client_message(connection_id, msg);
    }
}

fn handle_protects_remove(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: protects remove <body_part>");
        return;
    }
    let body_part = join_body_part_from_parts(parts);
    if remove_item_protects(item_id, body_part) {
        let updated = get_item_protects(item_id);
        let protects_str = "(none)";
        if updated.len() > 0 { protects_str = updated.join(", "); }
        let msg = "Removed protection for: " + body_part + "\nNow protects: " + protects_str;
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Body part not found in protection list.");
    }
}

fn show_protects_help(current, connection_id) {
    let protects_str = "(none)";
    if current.len() > 0 { protects_str = current.join(", "); }
    let help = "Current protection: " + protects_str + "\n";
    help += "Usage:\n";
    help += "  protects add <part>    - Add body part protection\n";
    help += "  protects remove <part> - Remove body part protection\n";
    help += "  protects clear         - Clear all protection\n";
    help += "Body parts: head, neck, torso, left arm, right arm, left leg, right leg,\n";
    help += "            left hand, right hand, left foot, right foot";
    send_client_message(connection_id, help);
}

fn handle_protects_subcommand(item_id, parts, connection_id) {
    let current = get_item_protects(item_id);

    if parts.len() < 3 {
        show_protects_help(current, connection_id);
        return;
    }

    let action = parts[2].to_lower();
    if action == "add" { handle_protects_add(item_id, parts, connection_id); return; }
    if action == "remove" { handle_protects_remove(item_id, parts, connection_id); return; }
    if action == "clear" {
        clear_item_protects(item_id);
        send_client_message(connection_id, "All body part protection cleared.");
        return;
    }
    send_client_message(connection_id, "Unknown action. Use: add, remove, or clear");
}

fn handle_weight_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " weight <value>");
        return;
    }
    let weight = parse_int(parts[2]);
    if weight == () {
        send_client_message(connection_id, "Weight must be a whole number (e.g., 1, 5, 10).");
        return;
    }
    set_item_weight(item_id, weight);
    send_client_message(connection_id, "Weight set to: " + weight);
}

fn handle_value_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " value <gold>");
        return;
    }
    let val = parse_int(parts[2]);
    if val == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_value(item_id, val);
    send_client_message(connection_id, "Value set to: " + val + " gold");
}

fn handle_world_max_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " world_max <n>  (0 = unlimited)");
        return;
    }
    let n = parse_int(parts[2]);
    if n == () {
        send_client_message(connection_id, "Cap must be a whole number (0 to clear).");
        return;
    }
    set_item_world_max_count(item_id, n);
    if n <= 0 {
        send_client_message(connection_id, "World cap cleared (unlimited copies).");
    } else {
        send_client_message(connection_id, "World cap set to " + n + " live copies.");
    }
}

fn handle_damage_subcommand(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " damage <dice_count> <dice_sides> (e.g., 2 6 for 2d6)");
        return;
    }
    let count = parse_int(parts[2]);
    let sides = parse_int(parts[3]);
    if count == () || sides == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    set_item_damage(item_id, count, sides);
    send_client_message(connection_id, "Damage set to: " + count + "d" + sides);
}

fn handle_damtype_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let all_types = get_all_damage_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Usage: oedit " + item_id + " damtype <type>\nAvailable: " + types_str);
        return;
    }
    let type_str = parts[2].to_lower();
    if set_item_damage_type(item_id, type_str) {
        send_client_message(connection_id, "Damage type set to: " + type_str);
    } else {
        let all_types = get_all_damage_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Invalid damage type. Available: " + types_str);
    }
}

fn handle_twohanded_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_item_two_handed(item_id, new_val);
    let status = "OFF";
    if new_val { status = "ON"; }
    send_client_message(connection_id, "Two-handed: " + status);
}

fn handle_wskill_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let all_skills = get_all_weapon_skills();
        let skills_str = olc::join_array(all_skills);
        send_client_message(connection_id, "Usage: oedit " + item_id + " wskill <skill|none>\nAvailable: " + skills_str);
        return;
    }
    // Join all parts from index 2 onwards to support multi-word skills like "short blades"
    let remaining = [];
    for i in 2..parts.len() {
        if parts[i] != "" {
            remaining.push(parts[i]);
        }
    }
    let skill_str = remaining.join(" ").to_lower();
    if skill_str == "none" || skill_str == "clear" {
        set_item_weapon_skill(item_id, "");
        send_client_message(connection_id, "Weapon skill cleared.");
        return;
    }
    if set_item_weapon_skill(item_id, skill_str) {
        send_client_message(connection_id, "Weapon skill set to: " + skill_str);
    } else {
        let all_skills = get_all_weapon_skills();
        let skills_str = olc::join_array(all_skills);
        send_client_message(connection_id, "Invalid weapon skill. Available: " + skills_str);
    }
}

// ========== Ammunition Subcommands ==========

fn handle_caliber_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " caliber <type|none>\nExamples: arrow, bolt, 9mm, 5.56mm");
        return;
    }
    let cal = parts[2].to_lower();
    if cal == "none" || cal == "clear" {
        set_item_caliber(item_id, "");
        send_client_message(connection_id, "Caliber cleared.");
        return;
    }
    set_item_caliber(item_id, cal);
    // Auto-set weapon skill and damage type for weapon types
    let auto_msgs = auto_set_ranged_defaults(item_id);
    if auto_msgs != "" {
        send_client_message(connection_id, "Caliber set to: " + cal + " (" + auto_msgs + ")");
        return;
    }
    send_client_message(connection_id, "Caliber set to: " + cal);
}

fn auto_set_ranged_defaults(item_id) {
    let item = get_item_data(item_id);
    if item == () { return ""; }
    if item.item_type != "weapon" { return ""; }
    let changes = [];
    let wskill = get_item_weapon_skill(item_id);
    if wskill == () || wskill == "" || wskill == "unarmed" {
        set_item_weapon_skill(item_id, "ranged");
        changes.push("weapon skill set to: ranged");
    }
    if item.damage_type == "bludgeoning" {
        set_item_damage_type(item_id, "ballistic");
        changes.push("damage type set to: ballistic");
    }
    return changes.join(", ");
}

fn handle_ammocount_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " ammocount <amount>");
        return;
    }
    let count = parse_int(parts[2]);
    if count == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_ammo_count(item_id, count);
    send_client_message(connection_id, "Ammo count set to: " + count);
}

fn handle_ammobonus_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " ammobonus <bonus>");
        return;
    }
    let bonus = parse_int(parts[2]);
    if bonus == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_ammo_damage_bonus(item_id, bonus);
    send_client_message(connection_id, "Ammo damage bonus set to: " + bonus);
}

fn handle_rangedtype_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " rangedtype <bow|crossbow|firearm|none>");
        return;
    }
    let rtype = parts[2].to_lower();
    if rtype == "none" || rtype == "clear" {
        set_item_ranged_type(item_id, "");
        // Clear magazine fields
        set_item_magazine_size(item_id, 0);
        set_item_loaded_ammo(item_id, 0);
        set_item_loaded_ammo_bonus(item_id, 0);
        set_item_fire_mode(item_id, "");
        set_item_supported_fire_modes(item_id, "");
        send_client_message(connection_id, "Ranged type cleared (magazine fields reset).");
        return;
    }
    if rtype != "bow" && rtype != "crossbow" && rtype != "firearm" {
        send_client_message(connection_id, "Invalid ranged type. Use: bow, crossbow, firearm, or none.");
        return;
    }
    set_item_ranged_type(item_id, rtype);
    let auto_msg = "";
    // Auto-set defaults based on type
    if rtype == "crossbow" {
        set_item_magazine_size(item_id, 1);
        set_item_fire_mode(item_id, "single");
        set_item_supported_fire_modes(item_id, "single");
        auto_msg = " (auto: magsize=1, firemode=single)";
    }
    if rtype == "firearm" {
        let current_mode = get_item_fire_mode(item_id);
        if current_mode == "" {
            set_item_fire_mode(item_id, "single");
            auto_msg = " (auto: firemode=single)";
        }
    }
    if rtype == "bow" {
        // Bows don't use magazines
        set_item_magazine_size(item_id, 0);
        set_item_loaded_ammo(item_id, 0);
        set_item_loaded_ammo_bonus(item_id, 0);
        set_item_fire_mode(item_id, "");
        set_item_supported_fire_modes(item_id, "");
        auto_msg = " (magazine fields cleared - bows use ready slot)";
    }
    send_client_message(connection_id, "Ranged type set to: " + rtype + auto_msg);
}

fn handle_magsize_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " magsize <capacity>");
        return;
    }
    let size = parse_int(parts[2]);
    if size == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_magazine_size(item_id, size);
    send_client_message(connection_id, "Magazine size set to: " + size);
}

fn handle_loadedammo_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " loadedammo <count>");
        return;
    }
    let count = parse_int(parts[2]);
    if count == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_item_loaded_ammo(item_id, count);
    send_client_message(connection_id, "Loaded ammo set to: " + count);
}

fn handle_firemode_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " firemode <single|burst|auto>");
        return;
    }
    let mode = parts[2].to_lower();
    if mode != "single" && mode != "burst" && mode != "auto" {
        send_client_message(connection_id, "Invalid fire mode. Use: single, burst, or auto.");
        return;
    }
    set_item_fire_mode(item_id, mode);
    send_client_message(connection_id, "Fire mode set to: " + mode);
}

fn handle_firemodes_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " firemodes <mode1> [mode2] [mode3]\nValid modes: single, burst, auto");
        return;
    }
    // Collect all remaining parts as modes
    let modes_str = "";
    let i = 2;
    while i < parts.len() {
        if i > 2 { modes_str += " "; }
        modes_str += parts[i];
        i += 1;
    }
    set_item_supported_fire_modes(item_id, modes_str);
    // Read back to confirm
    let saved = get_item_supported_fire_modes(item_id);
    let display = "";
    let j = 0;
    while j < saved.len() {
        if j > 0 { display += ", "; }
        display += saved[j];
        j += 1;
    }
    if display == "" { display = "(none)"; }
    send_client_message(connection_id, "Supported fire modes set to: " + display);
}

fn handle_noise_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let current = get_item_noise_level(item_id);
        send_client_message(connection_id, "Current noise level: " + current + "\nUsage: oedit " + item_id + " noise <silent|quiet|normal|loud|clear>");
        return;
    }
    let level = parts[2].to_lower();
    if level == "clear" {
        set_item_noise_level(item_id, "");
        send_client_message(connection_id, "Noise level cleared (will use default for ranged type).");
        return;
    }
    if level != "silent" && level != "quiet" && level != "normal" && level != "loud" {
        send_client_message(connection_id, "Invalid noise level. Use: silent, quiet, normal, loud, or clear.");
        return;
    }
    set_item_noise_level(item_id, level);
    send_client_message(connection_id, "Noise level set to: " + level);
}

fn handle_ammoeffect_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let etype = get_item_ammo_effect_type(item_id);
        let edur = get_item_ammo_effect_duration(item_id);
        let edmg = get_item_ammo_effect_damage(item_id);
        let display = "(none)";
        if etype != "" { display = etype + " (duration: " + edur + ", damage: " + edmg + ")"; }
        send_client_message(connection_id, "Ammo effect: " + display + "\nUsage: oedit " + item_id + " ammoeffect <fire|cold|poison|acid> <duration> <damage>\n       oedit " + item_id + " ammoeffect clear");
        return;
    }
    let arg = parts[2].to_lower();
    if arg == "clear" {
        set_item_ammo_effect_type(item_id, "");
        set_item_ammo_effect_duration(item_id, 0);
        set_item_ammo_effect_damage(item_id, 0);
        send_client_message(connection_id, "Ammo effect cleared.");
        return;
    }
    if arg != "fire" && arg != "cold" && arg != "poison" && arg != "acid" {
        send_client_message(connection_id, "Invalid effect type. Use: fire, cold, poison, acid, or clear.");
        return;
    }
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " ammoeffect <type> <duration> <damage>");
        return;
    }
    let dur = parse_int(parts[3]);
    let dmg = parse_int(parts[4]);
    if dur == () || dmg == () {
        send_client_message(connection_id, "Duration and damage must be numbers.");
        return;
    }
    set_item_ammo_effect_type(item_id, arg);
    set_item_ammo_effect_duration(item_id, dur);
    set_item_ammo_effect_damage(item_id, dmg);
    send_client_message(connection_id, "Ammo effect set: " + arg + " (duration: " + dur + ", damage: " + dmg + ")");
}

fn build_attachment_help(item_id) {
    let slot = get_item_attachment_slot(item_id);
    let acc = get_item_attachment_accuracy_bonus(item_id);
    let noise = get_item_attachment_noise_reduction(item_id);
    let mag = get_item_attachment_magazine_bonus(item_id);
    let compat = get_item_attachment_compatible_types(item_id);
    let compat_str = "";
    let ci = 0;
    while ci < compat.len() {
        if ci > 0 { compat_str += ", "; }
        compat_str += compat[ci];
        ci += 1;
    }
    if compat_str == "" { compat_str = "(none)"; }
    let slot_display = slot;
    if slot_display == "" { slot_display = "(none)"; }
    let msg = "Attachment slot: " + slot_display;
    msg += "\n  Accuracy bonus: " + acc;
    msg += "\n  Noise reduction: " + noise;
    msg += "\n  Magazine bonus: " + mag + "%";
    msg += "\n  Compatible types: " + compat_str;
    msg += "\n\nUsage:";
    msg += "\n  oedit " + item_id + " attachment slot <scope|suppressor|magazine|accessory|clear>";
    msg += "\n  oedit " + item_id + " attachment accuracy <number>";
    msg += "\n  oedit " + item_id + " attachment noise <number>";
    msg += "\n  oedit " + item_id + " attachment magazine <number>";
    msg += "\n  oedit " + item_id + " attachment compatible <type1,type2,...>";
    return msg;
}

fn handle_attachment_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, build_attachment_help(item_id));
        return;
    }
    let sub = parts[2].to_lower();
    if sub == "slot" {
        return handle_attachment_slot(item_id, parts, connection_id);
    }
    if sub == "accuracy" {
        return handle_attachment_int_field(item_id, parts, "accuracy", connection_id);
    }
    if sub == "noise" {
        return handle_attachment_int_field(item_id, parts, "noise", connection_id);
    }
    if sub == "magazine" {
        return handle_attachment_int_field(item_id, parts, "magazine", connection_id);
    }
    if sub == "compatible" {
        return handle_attachment_compatible(item_id, parts, connection_id);
    }
    send_client_message(connection_id, "Unknown attachment subcommand: " + sub);
}

fn handle_attachment_slot(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " attachment slot <scope|suppressor|magazine|accessory|clear>");
        return;
    }
    let val = parts[3].to_lower();
    if val == "clear" { val = ""; }
    if set_item_attachment_slot(item_id, val) {
        let display = val;
        if display == "" { display = "(cleared)"; }
        send_client_message(connection_id, "Attachment slot set to: " + display);
    } else {
        send_client_message(connection_id, "Invalid slot. Use: scope, suppressor, magazine, accessory, or clear.");
    }
}

fn handle_attachment_int_field(item_id, parts, field, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " attachment " + field + " <number>");
        return;
    }
    let val = parse_int(parts[3]);
    if val == () {
        send_client_message(connection_id, "Value must be a number.");
        return;
    }
    let ok = false;
    if field == "accuracy" { ok = set_item_attachment_accuracy_bonus(item_id, val); }
    if field == "noise" { ok = set_item_attachment_noise_reduction(item_id, val); }
    if field == "magazine" { ok = set_item_attachment_magazine_bonus(item_id, val); }
    if ok {
        send_client_message(connection_id, "Attachment " + field + " set to: " + val);
    } else {
        send_client_message(connection_id, "Failed to set attachment " + field + ".");
    }
}

fn handle_attachment_compatible(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " attachment compatible <type1,type2,...>\n  Types: bow, crossbow, firearm");
        return;
    }
    let types_str = parts[3];
    if types_str.to_lower() == "clear" { types_str = ""; }
    set_item_attachment_compatible_types(item_id, types_str);
    let saved = get_item_attachment_compatible_types(item_id);
    let display = "";
    let ci = 0;
    while ci < saved.len() {
        if ci > 0 { display += ", "; }
        display += saved[ci];
        ci += 1;
    }
    if display == "" { display = "(none)"; }
    send_client_message(connection_id, "Compatible types set to: " + display);
}

// Mirrors the alias set in src/script/items.rs set_item_flag/get_item_flag.
fn is_valid_item_flag(name) {
    let valid = ["no_drop", "nodrop", "no_get", "noget", "no_remove", "noremove",
        "invisible", "glow", "hum", "magical", "magic", "no_sell", "nosell", "unique",
        "quest_item", "questitem", "quest", "vending",
        "provides_light", "provideslight", "light",
        "night_vision", "nightvision", "infravision",
        "fishing_rod", "fishingrod", "rod", "bait",
        "foraging_tool", "foragingtool", "forage", "waterproof",
        "provides_warmth", "provideswarmth", "warmth",
        "reduces_glare", "reducesglare", "glare",
        "medical_tool", "medicaltool", "medical",
        "preserves_contents", "preservescontents", "preserves",
        "death_only", "deathonly", "atm", "lockpick",
        "is_skinned", "skinned", "boat",
        "buried", "can_dig", "candig", "detect_buried", "detectburied"];
    for f in valid { if f == name { return true; } }
    return false;
}

fn handle_flag_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " flag <name> [<name>...] [on|off]");
        return;
    }

    // Split into flag names and optional trailing on/off value.
    let flag_names = [];
    for i in 2..parts.len() { flag_names.push(parts[i].to_lower()); }
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

    // Validate all names before changing anything.
    for name in flag_names {
        if !is_valid_item_flag(name) {
            send_client_message(connection_id, "Unknown flag: " + name + "\nAvailable: no_drop, no_get, no_remove, invisible, glow, hum, magical, no_sell, unique, quest_item, vending, provides_light, night_vision, fishing_rod, bait, foraging_tool, waterproof, provides_warmth, reduces_glare, medical_tool, preserves_contents, death_only, atm, broken, plant_pot, lockpick, is_skinned, boat, buried, can_dig, detect_buried");
            return;
        }
    }

    for name in flag_names {
        let new_val = if explicit_val == () {
            !get_item_flag(item_id, name)
        } else {
            explicit_val
        };
        if set_item_flag(item_id, name, new_val) {
            let status = if new_val { "ON" } else { "OFF" };
            send_client_message(connection_id, "Flag '" + name + "' is now " + status + ".");
        } else {
            send_client_message(connection_id, "Failed to set flag '" + name + "'.");
        }
    }
}

fn show_category_help(item_id, connection_id) {
    let categories = get_item_categories(item_id);
    if categories.len() == 0 {
        send_client_message(connection_id, "Categories: (none)");
    } else {
        send_client_message(connection_id, "Categories: " + categories.join(", "));
    }
    send_client_message(connection_id, "\nUsage:");
    send_client_message(connection_id, "  oedit <item> category add <name>      - Add a category");
    send_client_message(connection_id, "  oedit <item> category remove <name>   - Remove a category");
    send_client_message(connection_id, "  oedit <item> category clear           - Remove all categories");
    send_client_message(connection_id, "  oedit <item> category list-all        - Show all categories in use");
    send_client_message(connection_id, "  oedit <item> category <name>          - Set as sole category");
}

fn handle_category_add(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <item> category add <name>");
        return;
    }
    let cat = parts[3].to_lower();
    if add_item_category(item_id, cat) {
        let updated = get_item_categories(item_id);
        send_client_message(connection_id, "Category '" + cat + "' added. Categories: " + updated.join(", "));
    } else {
        send_client_message(connection_id, "Category '" + cat + "' already exists or could not be added.");
    }
}

fn format_category_remove_msg(cat, updated) {
    if updated.len() == 0 {
        return "Category '" + cat + "' removed. No categories remaining.";
    }
    return "Category '" + cat + "' removed. Categories: " + updated.join(", ");
}

fn handle_category_remove(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <item> category remove <name>");
        return;
    }
    let cat = parts[3].to_lower();
    if !remove_item_category(item_id, cat) {
        send_client_message(connection_id, "Category '" + cat + "' not found on this item.");
        return;
    }
    let updated = get_item_categories(item_id);
    send_client_message(connection_id, format_category_remove_msg(cat, updated));
}

fn handle_category_list_all(connection_id) {
    let all_cats = get_all_item_categories();
    if all_cats.len() == 0 {
        send_client_message(connection_id, "No categories in use on any items.");
    } else {
        send_client_message(connection_id, "All categories in use (" + all_cats.len() + "):");
        send_client_message(connection_id, "  " + all_cats.join(", "));
    }
}

fn handle_category_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        show_category_help(item_id, connection_id);
        return;
    }

    let subcmd = parts[2].to_lower();

    if subcmd == "clear" || subcmd == "none" {
        set_item_categories(item_id, []);
        send_client_message(connection_id, "All categories cleared.");
        return;
    }
    if subcmd == "list-all" {
        handle_category_list_all(connection_id);
        return;
    }
    if subcmd == "add" {
        handle_category_add(item_id, parts, connection_id);
        return;
    }
    if subcmd == "remove" {
        handle_category_remove(item_id, parts, connection_id);
        return;
    }

    // Backward compat: bare name sets as sole category
    set_item_category(item_id, subcmd);
    send_client_message(connection_id, "Category set to: " + subcmd);
}

fn format_teaches_recipe_display(recipe_id) {
    let recipe = get_recipe(recipe_id);
    if recipe != () {
        return "Teaches Recipe: " + recipe_id + " (" + recipe.name + ")";
    }
    return "Teaches Recipe: " + recipe_id + " (WARNING: recipe not found!)";
}

fn format_teaches_set_message(recipe_id) {
    let recipe = get_recipe(recipe_id);
    if recipe != () {
        return "Teaches recipe set to: " + recipe_id + " (" + recipe.name + ")";
    }
    return "Teaches recipe set to: " + recipe_id;
}

fn handle_teaches_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let current = get_item_teaches_recipe(item_id);
        if current == () {
            send_client_message(connection_id, "Teaches Recipe: (none)");
            send_client_message(connection_id, "Usage: oedit <item> teaches <recipe_id> - Set recipe this item teaches");
            send_client_message(connection_id, "       oedit <item> teaches clear - Remove recipe teaching");
            return;
        }
        send_client_message(connection_id, format_teaches_recipe_display(current));
        return;
    }

    let recipe_id = parts[2];

    if recipe_id == "clear" || recipe_id == "none" {
        set_item_teaches_recipe(item_id, "");
        send_client_message(connection_id, "Teaches recipe cleared.");
        return;
    }

    // Validate recipe exists
    let recipe = get_recipe(recipe_id);
    if recipe == () {
        send_client_message(connection_id, "Warning: Recipe '" + recipe_id + "' not found.");
        send_client_message(connection_id, "Setting anyway - make sure the recipe exists (use reclist to check)");
    }

    set_item_teaches_recipe(item_id, recipe_id);
    send_client_message(connection_id, format_teaches_set_message(recipe_id));
}

fn handle_teaches_spell_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let current = get_item_teaches_spell(item_id);
        if current == () {
            send_client_message(connection_id, "Teaches Spell: (none)");
            send_client_message(connection_id, "Usage: oedit <item> teaches_spell <spell_id> - Set spell this scroll teaches");
            send_client_message(connection_id, "       oedit <item> teaches_spell clear - Remove spell teaching");
            return;
        }
        let info = get_spell_info(current);
        if info.len() > 0 {
            send_client_message(connection_id, "Teaches Spell: " + current + " (" + info["name"] + ")");
        } else {
            send_client_message(connection_id, "Teaches Spell: " + current + " (WARNING: spell not found!)");
        }
        return;
    }

    let spell_id = parts[2];

    if spell_id == "clear" || spell_id == "none" {
        set_item_teaches_spell(item_id, "");
        send_client_message(connection_id, "Teaches spell cleared.");
        return;
    }

    // Validate spell exists
    let info = get_spell_info(spell_id);
    if info.len() == 0 {
        send_client_message(connection_id, "Warning: Spell '" + spell_id + "' not found.");
        send_client_message(connection_id, "Setting anyway - make sure the spell ID is correct.");
    }

    set_item_teaches_spell(item_id, spell_id);
    if info.len() > 0 {
        send_client_message(connection_id, "Teaches spell set to: " + spell_id + " (" + info["name"] + ")");
    } else {
        send_client_message(connection_id, "Teaches spell set to: " + spell_id);
    }
}

fn get_transport_display_id(transport) {
    if transport.vnum != () && transport.vnum != "" {
        return transport.vnum;
    }
    return transport.id;
}

fn handle_transport_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        show_transport_link_info(item_id, connection_id);
        return;
    }

    let transport_ref = parts[2];

    if transport_ref == "clear" || transport_ref == "none" {
        set_item_transport_link(item_id, "");
        send_client_message(connection_id, "Transport link cleared.");
        return;
    }

    set_transport_link(item_id, transport_ref, connection_id);
}

fn show_transport_link_info(item_id, connection_id) {
    let current = get_item_transport_link(item_id);
    if current == "" {
        send_client_message(connection_id, "Transport Link: (none)");
        send_client_message(connection_id, "Usage: oedit <item> transport <transport_vnum> - Link to transport for status sign");
        send_client_message(connection_id, "       oedit <item> transport clear - Remove transport link");
        return;
    }
    let transport = get_transport_data(current);
    if transport == () {
        send_client_message(connection_id, "Transport Link: " + current + " (NOT FOUND!)");
        return;
    }
    let vnum_display = get_transport_display_id(transport);
    send_client_message(connection_id, "Transport Link: " + vnum_display + " (" + transport.name + ")");
}

fn set_transport_link(item_id, transport_ref, connection_id) {
    // Try to find transport by vnum first, then by ID
    let transport = get_transport_by_vnum(transport_ref);
    if transport == () {
        transport = get_transport_data(transport_ref);
    }

    if transport == () {
        send_client_message(connection_id, "Transport '" + transport_ref + "' not found.");
        send_client_message(connection_id, "Use 'tlist' to see available transports.");
        return;
    }

    set_item_transport_link(item_id, transport.id);
    let vnum_display = get_transport_display_id(transport);
    send_client_message(connection_id, "Transport link set to: " + vnum_display + " (" + transport.name + ")");
    send_client_message(connection_id, "This item will now show transport status when read.");
}

fn handle_note_subcommand(item_id, parts, connection_id) {
    // `oedit <id> note clear` wipes the body without entering the editor.
    if parts.len() >= 3 {
        let sub = parts[2].to_lower();
        if sub == "clear" || sub == "none" {
            set_item_note_content(item_id, "");
            send_client_message(connection_id, "Note cleared.");
            return;
        }
    }

    let item = get_item_data(item_id);
    if item == () {
        send_client_message(connection_id, "Item not found.");
        return;
    }

    let existing = get_item_note_content(item_id);
    let lines = if existing != "" { existing.split("\n") } else { [] };

    set_olc_edit_item(connection_id, item_id);
    set_olc_buffer(connection_id, lines);
    set_olc_mode(connection_id, "collecting_note");

    let output = "Editing note for: " + item.short_desc + "\n";
    if lines.len() == 0 {
        output += "(empty)\n";
    } else {
        for i in 0..lines.len() {
            output += "  " + (i + 1) + ": " + lines[i] + "\n";
        }
    }
    output += "\nCommands: .h help | .l list | .c clear | .d N | .r N <text> | . save | @ cancel\n";
    output += "Type lines to append. Blank lines, ANSI, and whitespace are preserved.";
    send_client_message(connection_id, output);
}

fn handle_spawn_subcommand(item, char, connection_id) {
    // Check unique flag - prevent duplicate spawns
    if item.flags.unique && item.vnum != () && item.vnum != "" {
        let existing = count_items_by_vnum(item.vnum);
        if existing > 0 {
            let msg = "Cannot spawn: unique item '" + item.name + "' already exists in world.";
            send_client_message(connection_id, msg);
            return;
        }
    }

    // Create a copy and place in current room
    let new_item = new_item(item.name, item.short_desc, item.long_desc);
    // Save immediately so setters can find it in the database
    save_item_data(new_item);
    // Copy properties
    set_item_keywords(new_item.id, item.keywords);
    // Copy vnum so orefresh can update this instance from prototype
    if item.vnum != () && item.vnum != "" {
        set_item_vnum(new_item.id, item.vnum);
    }
    set_item_type(new_item.id, item.item_type);
    set_item_wear_locations(new_item.id, item.wear_locations);
    if item.has_armor_class {
        set_item_armor_class(new_item.id, item.armor_class);
    }
    // Copy body part protection
    let source_protects = get_item_protects(item.id);
    if source_protects.len() > 0 {
        set_item_protects(new_item.id, source_protects);
    }
    // Copy weapon properties
    if item.has_damage {
        set_item_damage(new_item.id, item.damage_dice_count, item.damage_dice_sides);
        set_item_damage_type(new_item.id, item.damage_type);
    }
    set_item_two_handed(new_item.id, item.two_handed);
    // Copy weapon skill
    let source_wskill = get_item_weapon_skill(item.id);
    if source_wskill != () && source_wskill != "" {
        set_item_weapon_skill(new_item.id, source_wskill);
    }
    // Copy caliber (for both weapons and ammunition)
    if item.has_caliber {
        set_item_caliber(new_item.id, item.caliber);
    }
    // Copy ammunition properties
    if item.is_ammunition {
        set_item_ammo_count(new_item.id, item.ammo_count);
        set_item_ammo_damage_bonus(new_item.id, item.ammo_damage_bonus);
    }
    set_item_weight(new_item.id, item.weight);
    set_item_value(new_item.id, item.value);
    if item.has_world_max_count {
        set_item_world_max_count(new_item.id, item.world_max_count);
    }
    // Copy flags
    copy_item_flags(item, new_item.id);

    // Copy container properties
    if item.is_container {
        set_container_capacity(new_item.id, item.container_max_items, item.container_max_weight);
        set_container_closed(new_item.id, item.container_closed);
        set_container_locked(new_item.id, item.container_locked);
        if item.container_key_vnum != () && item.container_key_vnum != "" {
            set_container_key_vnum(new_item.id, item.container_key_vnum);
        }
        // Copy weight reduction for worn containers
        if item.weight_reduction > 0 {
            set_item_weight_reduction(new_item.id, item.weight_reduction);
        }
        // Copy preservation level for preserving containers
        if item.preservation_level > 0 {
            set_preservation_level(new_item.id, item.preservation_level);
        }
        // Note: container_contents is NOT copied - spawned containers start empty
    }

    // Copy liquid container properties
    if item.is_liquid_container {
        set_liquid(new_item.id, item.liquid_type, item.liquid_current, item.liquid_max);
        set_liquid_poisoned(new_item.id, item.liquid_poisoned);
        // Copy liquid effects
        for effect in item.liquid_effects {
            add_liquid_effect(new_item.id, effect.effect_type, effect.magnitude, effect.duration);
        }
    }

    // Copy food properties
    if item.is_food {
        set_food_properties(new_item.id, item.food_nutrition, item.food_spoil_duration);
        set_food_poisoned(new_item.id, item.food_poisoned);
        set_food_created_at(new_item.id, 0);  // Fresh food on spawn
        // Copy food effects
        for effect in item.food_effects {
            add_food_effect(new_item.id, effect.effect_type, effect.magnitude, effect.duration);
        }
    }

    // Copy vending properties
    if item.flags.vending {
        set_vending_sell_rate(new_item.id, item.vending_sell_rate);
        for vnum in item.vending_stock {
            add_vending_stock(new_item.id, vnum);
        }
    }

    // Copy quality (used by fishing rods, bait, etc.)
    if item.quality > 0 {
        set_item_quality(new_item.id, item.quality);
    }

    // Copy bait uses
    if item.is_bait {
        set_bait_uses(new_item.id, item.bait_uses);
    }

    // Copy medical tool properties
    if item.flags.medical_tool {
        set_medical_tier(new_item.id, item.medical_tier);
        set_medical_uses(new_item.id, item.medical_uses);
        for wound_type in item.treats_wound_types {
            add_treats_wound_type(new_item.id, wound_type);
        }
        if item.max_treatable_wound != "" {
            set_max_treatable_wound(new_item.id, item.max_treatable_wound);
        }
    }

    // Copy recipe-related properties
    let categories = get_item_categories(item.id);
    if categories.len() > 0 {
        set_item_categories(new_item.id, categories);
    }
    let teaches = get_item_teaches_recipe(item.id);
    if teaches != () {
        set_item_teaches_recipe(new_item.id, teaches);
    }
    let teaches_spell = get_item_teaches_spell(item.id);
    if teaches_spell != () {
        set_item_teaches_spell(new_item.id, teaches_spell);
    }
    let transport_link = get_item_transport_link(item.id);
    if transport_link != "" {
        set_item_transport_link(new_item.id, transport_link);
    }
    let note_body = get_item_note_content(item.id);
    if note_body != "" {
        set_item_note_content(new_item.id, note_body);
    }

    // Copy triggers
    let triggers = get_item_triggers(item.id);
    let idx = 0;
    for trigger in triggers {
        let ttype = get_item_trigger_type(trigger);
        if trigger.args.len() > 0 {
            add_item_trigger_with_args(new_item.id, ttype, trigger.script_name, trigger.args);
        } else {
            add_item_trigger(new_item.id, ttype, trigger.script_name);
        }
        // Set chance and enabled state if different from defaults
        if trigger.chance != 100 {
            set_item_trigger_chance(new_item.id, idx, trigger.chance);
        }
        if !trigger.enabled {
            set_item_trigger_enabled(new_item.id, idx, false);
        }
        idx += 1;
    }

    move_item_to_room(new_item.id, char.current_room_id);
    send_client_message(connection_id, "Spawned " + item.name + " in room.");
}

fn copy_item_flags(item, new_item_id) {
    if item.flags.no_drop { set_item_flag(new_item_id, "no_drop", true); }
    if item.flags.no_get { set_item_flag(new_item_id, "no_get", true); }
    if item.flags.no_remove { set_item_flag(new_item_id, "no_remove", true); }
    if item.flags.invisible { set_item_flag(new_item_id, "invisible", true); }
    if item.flags.glow { set_item_flag(new_item_id, "glow", true); }
    if item.flags.hum { set_item_flag(new_item_id, "hum", true); }
    if item.flags.magical { set_item_flag(new_item_id, "magical", true); }
    if item.flags.no_sell { set_item_flag(new_item_id, "no_sell", true); }
    if item.flags.unique { set_item_flag(new_item_id, "unique", true); }
    if item.flags.quest_item { set_item_flag(new_item_id, "quest_item", true); }
    if item.flags.vending { set_item_flag(new_item_id, "vending", true); }
    if item.flags.provides_light { set_item_flag(new_item_id, "provides_light", true); }
    if item.flags.night_vision { set_item_flag(new_item_id, "night_vision", true); }
    if item.flags.fishing_rod { set_item_flag(new_item_id, "fishing_rod", true); }
    if item.flags.bait { set_item_flag(new_item_id, "bait", true); }
    if item.flags.foraging_tool { set_item_flag(new_item_id, "foraging_tool", true); }
    if item.flags.waterproof { set_item_flag(new_item_id, "waterproof", true); }
    if item.flags.provides_warmth { set_item_flag(new_item_id, "provides_warmth", true); }
    if item.flags.reduces_glare { set_item_flag(new_item_id, "reduces_glare", true); }
    if item.flags.medical_tool { set_item_flag(new_item_id, "medical_tool", true); }
    if item.flags.preserves_contents { set_item_flag(new_item_id, "preserves_contents", true); }
    if item.flags.lockpick { set_item_flag(new_item_id, "lockpick", true); }
    if item.flags.is_skinned { set_item_flag(new_item_id, "is_skinned", true); }
    if item.flags.boat { set_item_flag(new_item_id, "boat", true); }
    if item.flags.buried { set_item_flag(new_item_id, "buried", true); }
    if item.flags.can_dig { set_item_flag(new_item_id, "can_dig", true); }
    if item.flags.detect_buried { set_item_flag(new_item_id, "detect_buried", true); }
}

// === Container Subcommands ===

fn handle_capacity_subcommand(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " capacity <max_items> <max_weight>\n(Use 0 for unlimited)");
        return;
    }
    let max_items = parse_int(parts[2]);
    let max_weight = parse_int(parts[3]);
    if max_items == () || max_weight == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    set_container_capacity(item_id, max_items, max_weight);
    let items_str = "unlimited";
    if max_items > 0 { items_str = "" + max_items; }
    let weight_str = "unlimited";
    if max_weight > 0 { weight_str = "" + max_weight; }
    send_client_message(connection_id, "Container capacity: " + items_str + " items, " + weight_str + " weight");
}

fn handle_closed_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" || setting == "open" {
            new_val = false;
        }
    }
    set_container_closed(item_id, new_val);
    let status = "open";
    if new_val { status = "closed"; }
    send_client_message(connection_id, "Container is now: " + status);
}

fn handle_locked_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_container_locked(item_id, new_val);
    let status = "unlocked";
    if new_val { status = "locked"; }
    send_client_message(connection_id, "Container is now: " + status);
}

fn handle_key_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " key <key_vnum|none>");
        return;
    }
    let key_val = parts[2].to_lower();
    if key_val == "none" || key_val == "clear" {
        set_container_key_vnum(item_id, "");
        send_client_message(connection_id, "Container key cleared.");
    } else {
        let proto = get_item_by_vnum(parts[2]);
        if proto == () {
            send_client_message(connection_id, "Item prototype not found with vnum: " + parts[2]);
            return;
        }
        if !proto.is_key {
            send_client_message(connection_id, "Warning: " + proto.name + " is not a key-type item (but setting it anyway).");
        }
        set_container_key_vnum(item_id, parts[2]);
        send_client_message(connection_id, "Container key set to: " + proto.name + " (vnum: " + parts[2] + ")");
    }
}

// === Liquid Container Subcommands ===

fn handle_liquid_subcommand(item_id, parts, connection_id) {
    if parts.len() < 5 {
        let all_types = get_all_liquid_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Usage: oedit " + item_id + " liquid <type> <current> <max>\nTypes: " + types_str);
        return;
    }
    let type_str = parts[2].to_lower();
    let current = parse_int(parts[3]);
    let max = parse_int(parts[4]);
    if current == () || max == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if set_liquid(item_id, type_str, current, max) {
        // Auto-set default effects based on liquid type
        let auto_msg = auto_set_liquid_defaults(item_id, type_str);
        send_client_message(connection_id, "Liquid set: " + type_str + " (" + current + "/" + max + " sips)" + auto_msg);
    } else {
        let all_types = get_all_liquid_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Invalid liquid type. Available: " + types_str);
    }
}

fn auto_set_liquid_defaults(item_id, liquid_type) {
    // Clear existing effects first
    clear_liquid_effects(item_id);
    let effects = get_defaults_for_liquid(liquid_type);
    let msg_parts = [];
    for e in effects {
        add_liquid_effect(item_id, e[0], e[1], e[2]);
        msg_parts.push(e[0] + "(" + e[1] + ")");
    }
    if msg_parts.len() > 0 {
        return " (auto-effects: " + msg_parts.join(", ") + ")";
    }
    return "";
}

fn get_defaults_for_liquid(lt) {
    // Returns array of [effect_type, magnitude, duration]
    if lt == "water" { return [["quenched", 100, 0]]; }
    if lt == "ale" { return [["drunk", 2, 300], ["quenched", 50, 0]]; }
    if lt == "wine" { return [["drunk", 4, 300], ["quenched", 30, 0]]; }
    if lt == "beer" { return get_defaults_beer(); }
    if lt == "alcohol" { return [["drunk", 6, 300], ["quenched", 20, 0]]; }
    if lt == "spirits" { return [["drunk", 5, 300], ["quenched", 25, 0]]; }
    if lt == "milk" { return [["satiated", 20, 0], ["quenched", 80, 0]]; }
    if lt == "juice" { return get_defaults_juice(); }
    return get_defaults_for_liquid_2(lt);
}

fn get_defaults_for_liquid_2(lt) {
    if lt == "tea" { return [["stamina_restore", 3, 0], ["quenched", 90, 0]]; }
    if lt == "coffee" { return [["stamina_restore", 8, 0], ["quenched", 70, 0]]; }
    if lt == "poison" { return [["poison", 10, 0]]; }
    if lt == "healing_potion" { return [["heal", 20, 0], ["quenched", 30, 0]]; }
    if lt == "mana_potion" { return [["mana_restore", 20, 0], ["quenched", 30, 0]]; }
    if lt == "blood" { return [["satiated", 10, 0]]; }
    if lt == "oil" { return [["poison", 3, 0]]; }
    return [];
}

fn get_defaults_beer() {
    return [["drunk", 2, 300], ["quenched", 50, 0], ["satiated", 10, 0]];
}

fn get_defaults_juice() {
    return [["stamina_restore", 5, 0], ["quenched", 80, 0]];
}

fn handle_fill_subcommand(item_id, connection_id) {
    let item = get_item_data(item_id);
    if item == () {
        send_client_message(connection_id, "Item not found.");
        return;
    }
    set_liquid(item_id, item.liquid_type, item.liquid_max, item.liquid_max);
    send_client_message(connection_id, "Container filled to maximum (" + item.liquid_max + " sips).");
}

fn handle_empty_subcommand(item_id, connection_id) {
    let item = get_item_data(item_id);
    if item == () {
        send_client_message(connection_id, "Item not found.");
        return;
    }
    set_liquid(item_id, item.liquid_type, 0, item.liquid_max);
    send_client_message(connection_id, "Container emptied.");
}

fn handle_liqpoison_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_liquid_poisoned(item_id, new_val);
    let status = "not poisoned";
    if new_val { status = "poisoned"; }
    send_client_message(connection_id, "Liquid is now: " + status);
}

fn handle_liqeffect_subcommand(item_id, parts, connection_id) {
    if parts.len() < 5 {
        let all_types = get_all_effect_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Usage: oedit " + item_id + " liqeffect <type> <magnitude> <duration>\nTypes: " + types_str);
        return;
    }
    let type_str = parts[2].to_lower();
    let magnitude = parse_int(parts[3]);
    let duration = parse_int(parts[4]);
    if magnitude == () || duration == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if add_liquid_effect(item_id, type_str, magnitude, duration) {
        send_client_message(connection_id, "Added liquid effect: " + type_str + " (magnitude: " + magnitude + ", duration: " + duration + "s)");
    } else {
        let all_types = get_all_effect_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Invalid effect type. Available: " + types_str);
    }
}

fn handle_clearliqeffects_subcommand(item_id, connection_id) {
    clear_liquid_effects(item_id);
    send_client_message(connection_id, "Liquid effects cleared.");
}

// === Food Subcommands ===

fn handle_nutrition_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " nutrition <value>");
        return;
    }
    let nutrition = parse_int(parts[2]);
    if nutrition == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    let item = get_item_data(item_id);
    let spoil_dur = 0;
    if item != () { spoil_dur = item.food_spoil_duration; }
    set_food_properties(item_id, nutrition, spoil_dur);
    send_client_message(connection_id, "Nutrition set to: " + nutrition);
}

fn handle_spoil_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " spoil <hours> (0 = never spoils, 1 hour ~ 1 game day)");
        return;
    }
    let hours = parse_float(parts[2]);
    if hours == () || hours < 0.0 {
        send_client_message(connection_id, "Invalid number. Provide hours (e.g. 2, 0.5).");
        return;
    }
    let spoil_dur = (hours * 3600.0).to_int();
    let item = get_item_data(item_id);
    let nutrition = 0;
    if item != () { nutrition = item.food_nutrition; }
    set_food_properties(item_id, nutrition, spoil_dur);
    if spoil_dur == 0 {
        send_client_message(connection_id, "Food will never spoil.");
    } else {
        send_client_message(connection_id, "Food spoils after: " + hours + " hours (~" + hours + " game days)");
    }
}

fn handle_foodpoison_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_food_poisoned(item_id, new_val);
    let status = "not poisoned";
    if new_val { status = "poisoned"; }
    send_client_message(connection_id, "Food is now: " + status);
}

fn handle_foodeffect_subcommand(item_id, parts, connection_id) {
    if parts.len() < 5 {
        let all_types = get_all_effect_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Usage: oedit " + item_id + " foodeffect <type> <magnitude> <duration>\nTypes: " + types_str);
        return;
    }
    let type_str = parts[2].to_lower();
    let magnitude = parse_int(parts[3]);
    let duration = parse_int(parts[4]);
    if magnitude == () || duration == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if add_food_effect(item_id, type_str, magnitude, duration) {
        send_client_message(connection_id, "Added food effect: " + type_str + " (magnitude: " + magnitude + ", duration: " + duration + "s)");
    } else {
        let all_types = get_all_effect_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Invalid effect type. Available: " + types_str);
    }
}

fn handle_clearfoodeffects_subcommand(item_id, connection_id) {
    clear_food_effects(item_id);
    send_client_message(connection_id, "Food effects cleared.");
}

fn handle_resetfresh_subcommand(item_id, connection_id) {
    set_food_created_at(item_id, 0);  // 0 means "now"
    send_client_message(connection_id, "Food freshness reset to current time.");
}

fn handle_preservation_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <id> preservation <0|1|2>\n  0 = none, 1 = cool/fridge, 2 = frozen/freezer");
        return;
    }
    let level = parse_int(parts[2]);
    if level < 0 || level > 2 {
        send_client_message(connection_id, "Preservation level must be 0, 1, or 2.");
        return;
    }
    set_preservation_level(item_id, level);
    let labels = ["none", "cool/fridge", "frozen/freezer"];
    send_client_message(connection_id, "Preservation level set to " + level + " (" + labels[level] + ").");
}

fn handle_spoilage_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <id> spoilage <0.0-1.0>  (manually set spoilage points)");
        return;
    }
    let points = parse_float(parts[2]);
    if points < 0.0 || points > 1.0 {
        send_client_message(connection_id, "Spoilage points must be between 0.0 and 1.0.");
        return;
    }
    set_food_spoilage_points(item_id, points);
    send_client_message(connection_id, "Spoilage points set to " + points + ".");
}

fn show_item_properties(item, item_id, connection_id) {
    let output = olc::c_header("=== Object Editor ===", connection_id) + "\n";
    output += olc::c_label("Name: ", connection_id) + olc::c_value(item.name, connection_id) + "\n";
    output += olc::c_label("ID: ", connection_id) + olc::c_value(item.id, connection_id) + "\n";
    output += olc::c_label("Short: ", connection_id) + olc::c_value(item.short_desc, connection_id) + "\n";
    output += olc::c_label("Type: ", connection_id) + olc::c_value(item.item_type, connection_id) + "\n";

    let kws = item.keywords;
    output += olc::c_label("Keywords: ", connection_id);
    if kws.len() == 0 {
        output += "(none)\n";
    } else {
        output += olc::c_value(olc::join_array(kws), connection_id) + "\n";
    }

    let wear_locs = item.wear_locations;
    output += olc::c_label("Wear Locations: ", connection_id);
    if wear_locs.len() == 0 {
        output += "(not wearable)\n";
    } else {
        output += olc::c_value(olc::join_array(wear_locs), connection_id) + "\n";
    }

    // Armor-specific properties
    if item.item_type == "armor" {
        if item.has_armor_class {
            output += olc::c_label("Armor Class: ", connection_id) + olc::c_value("" + item.armor_class, connection_id) + "\n";
        } else {
            output += olc::c_label("Armor Class: ", connection_id) + "(none)\n";
        }

        let protects = get_item_protects(item.id);
        output += olc::c_label("Protects: ", connection_id);
        if protects.len() == 0 {
            output += "(none)\n";
        } else {
            output += olc::c_value(protects.join(", "), connection_id) + "\n";
        }
    }

    // Weapon-specific properties
    if item.item_type == "weapon" {
        output += build_weapon_display(item, connection_id);
    }

    // Ammunition-specific properties
    if item.is_ammunition {
        output += olc::c_label("--- Ammunition ---\n", connection_id);
        if item.has_caliber {
            output += olc::c_label("Caliber: ", connection_id) + olc::c_value(item.caliber, connection_id) + "\n";
        } else {
            output += olc::c_label("Caliber: ", connection_id) + "(none)\n";
        }
        output += olc::c_label("Ammo Count: ", connection_id) + olc::c_value("" + item.ammo_count, connection_id) + "\n";
        output += olc::c_label("Damage Bonus: ", connection_id) + olc::c_value("" + item.ammo_damage_bonus, connection_id) + "\n";
        let eff_type = item.ammo_effect_type;
        if eff_type != "" {
            let eff_detail = eff_type + " (dur: " + item.ammo_effect_duration + ", dmg: " + item.ammo_effect_damage + ")";
            output += olc::c_label("Ammo Effect: ", connection_id) + olc::c_value(eff_detail, connection_id) + "\n";
        }
    }

    // Type-specific properties
    if item.is_container {
        output += build_container_display(item);
    }
    if item.is_liquid_container {
        output += build_liquid_display(item);
    }
    if item.is_food {
        output += build_food_display(item);
    }
    if item.is_fishing_rod {
        output += build_fishing_rod_display(item);
    }
    if item.is_bait {
        output += build_bait_display(item);
    }

    output += olc::c_label("Weight: ", connection_id) + olc::c_value("" + item.weight, connection_id) + "\n";
    output += olc::c_label("Value: ", connection_id) + olc::c_value("" + item.value + " gold", connection_id) + "\n";
    if item.has_world_max_count {
        output += olc::c_label("World Max: ", connection_id) + olc::c_value("" + item.world_max_count + " live copies", connection_id) + "\n";
    }

    // Recipe-related properties
    let categories = get_item_categories(item.id);
    if categories.len() > 0 {
        output += olc::c_label("Categories: ", connection_id) + olc::c_value(categories.join(", "), connection_id) + "\n";
    }
    let teaches = get_item_teaches_recipe(item.id);
    if teaches != () {
        let recipe = get_recipe(teaches);
        if recipe != () {
            output += olc::c_label("Teaches: ", connection_id) + olc::c_value(teaches + " (" + recipe.name + ")", connection_id) + "\n";
        } else {
            output += olc::c_label("Teaches: ", connection_id) + olc::c_warn(teaches + " (NOT FOUND!)", connection_id) + "\n";
        }
    }

    let teaches_spell = get_item_teaches_spell(item.id);
    if teaches_spell != () {
        let spell_info = get_spell_info(teaches_spell);
        if spell_info.len() > 0 {
            output += olc::c_label("Teaches Spell: ", connection_id) + olc::c_value(teaches_spell + " (" + spell_info["name"] + ")", connection_id) + "\n";
        } else {
            output += olc::c_label("Teaches Spell: ", connection_id) + olc::c_warn(teaches_spell + " (NOT FOUND!)", connection_id) + "\n";
        }
    }

    // Note body (readable via `read`)
    let note_body = get_item_note_content(item.id);
    if note_body != "" {
        let line_count = note_body.split("\n").len();
        let suffix = if line_count == 1 { " line" } else { " lines" };
        output += olc::c_label("Note: ", connection_id) + olc::c_value("" + line_count + suffix + " (oedit <id> note to edit)", connection_id) + "\n";
    }

    // Transport link (for status signs)
    let transport_link = get_item_transport_link(item.id);
    if transport_link != "" {
        let transport = get_transport_data(transport_link);
        if transport != () {
            let vnum_display = get_transport_display_id(transport);
            output += olc::c_label("Transport: ", connection_id) + olc::c_value(vnum_display + " (" + transport.name + ")", connection_id) + "\n";
        } else {
            output += olc::c_label("Transport: ", connection_id) + olc::c_warn(transport_link + " (NOT FOUND!)", connection_id) + "\n";
        }
    }

    // Generic quality (shown if > 0)
    if item.quality > 0 {
        output += olc::c_label("Quality: ", connection_id) + olc::c_value("" + item.quality + "/100", connection_id) + "\n";
    }

    // Level requirement
    if item.level_requirement > 0 {
        output += olc::c_label("Level Required: ", connection_id) + olc::c_value("" + item.level_requirement, connection_id) + "\n";
    } else {
        output += olc::c_label("Level Required: ", connection_id) + "(none)\n";
    }

    // Stat bonuses
    output += build_stats_display(item);

    output += build_flags_display(item);

    // Triggers
    output += build_triggers_display(item_id);

    // Prototype info
    if item.is_prototype {
        output += "\n" + olc::c_warn("*** PROTOTYPE TEMPLATE ***", connection_id) + "\n";
        output += olc::c_label("VNUM: ", connection_id) + olc::c_value(if item.vnum != "" { item.vnum } else { "(not set)" }, connection_id) + "\n";
    }

    output += "\n" + olc::c_label("Long Description:", connection_id) + "\n" + item.long_desc + "\n";
    output += build_commands_help(item_id, item);

    send_client_message(connection_id, output);
}

fn build_weapon_display(item, connection_id) {
    let output = "";
    if item.has_damage {
        output += olc::c_label("Damage: ", connection_id) + olc::c_value(item.damage_dice_count + "d" + item.damage_dice_sides, connection_id);
        output += " (" + item.damage_type + ")\n";
    } else {
        output += olc::c_label("Damage: ", connection_id) + "(none)\n";
    }

    output += olc::c_label("Two-Handed: ", connection_id);
    if item.two_handed { output += olc::c_value("Yes", connection_id) + "\n"; } else { output += "No\n"; }

    let wskill = get_item_weapon_skill(item.id);
    output += olc::c_label("Weapon Skill: ", connection_id);
    if wskill == () || wskill == "" {
        output += "(none)\n";
    } else {
        output += olc::c_value(wskill, connection_id) + "\n";
    }

    if item.has_caliber {
        output += olc::c_label("Caliber: ", connection_id) + olc::c_value(item.caliber, connection_id) + "\n";
    }
    if item.has_ranged_type {
        output += build_ranged_weapon_display(item, connection_id);
    }

    // Attachment properties (if this item is an attachment)
    if item.attachment_slot != "" {
        output += "\n--- Attachment Properties ---\n";
        output += olc::c_label("Slot: ", connection_id) + olc::c_value(item.attachment_slot, connection_id) + "\n";
        output += olc::c_label("Accuracy Bonus: ", connection_id) + olc::c_value("" + item.attachment_accuracy_bonus, connection_id) + "\n";
        output += olc::c_label("Noise Reduction: ", connection_id) + olc::c_value("" + item.attachment_noise_reduction, connection_id) + "\n";
        output += olc::c_label("Magazine Bonus: ", connection_id) + olc::c_value("" + item.attachment_magazine_bonus + "%", connection_id) + "\n";
        let compat = item.attachment_compatible_types;
        let compat_str = "";
        let ci = 0;
        while ci < compat.len() {
            if ci > 0 { compat_str += ", "; }
            compat_str += compat[ci];
            ci += 1;
        }
        if compat_str == "" { compat_str = "(any)"; }
        output += olc::c_label("Compatible: ", connection_id) + olc::c_value(compat_str, connection_id) + "\n";
    }

    return output;
}

fn build_ranged_weapon_display(item, connection_id) {
    let output = olc::c_label("Ranged Type: ", connection_id) + olc::c_value(item.ranged_type, connection_id) + "\n";
    if item.ranged_type != "bow" {
        output += olc::c_label("Magazine Size: ", connection_id) + olc::c_value("" + item.magazine_size, connection_id) + "\n";
        output += olc::c_label("Loaded Ammo: ", connection_id) + olc::c_value("" + item.loaded_ammo + "/" + item.magazine_size, connection_id) + "\n";
        output += olc::c_label("Loaded Bonus: ", connection_id) + olc::c_value("" + item.loaded_ammo_bonus, connection_id) + "\n";
    }
    if item.ranged_type == "firearm" {
        output += build_firearm_modes_display(item, connection_id);
    }
    let noise = get_item_noise_level(item.id);
    let noise_explicit = item.noise_level;
    let noise_display = noise;
    if noise_explicit == "" { noise_display = noise + " (default)"; }
    output += olc::c_label("Noise Level: ", connection_id) + olc::c_value(noise_display, connection_id) + "\n";
    return output;
}

fn build_firearm_modes_display(item, connection_id) {
    let output = olc::c_label("Fire Mode: ", connection_id) + olc::c_value(item.fire_mode, connection_id) + "\n";
    let modes = item.supported_fire_modes;
    let modes_str = "";
    let mi = 0;
    while mi < modes.len() {
        if mi > 0 { modes_str += ", "; }
        modes_str += modes[mi];
        mi += 1;
    }
    if modes_str == "" { modes_str = "(none)"; }
    output += olc::c_label("Supported Modes: ", connection_id) + olc::c_value(modes_str, connection_id) + "\n";
    return output;
}

fn build_flags_display(item) {
    let flags = item.flags;
    let output = "\nFlags: ";
    let flag_str = "";
    if flags.no_drop { flag_str += "no_drop "; }
    if flags.no_get { flag_str += "no_get "; }
    if flags.no_remove { flag_str += "no_remove "; }
    if flags.invisible { flag_str += "invisible "; }
    if flags.glow { flag_str += "glow "; }
    if flags.hum { flag_str += "hum "; }
    if flags.magical { flag_str += "magical "; }
    if flags.no_sell { flag_str += "no_sell "; }
    if flags.unique { flag_str += "unique "; }
    if flags.quest_item { flag_str += "quest_item "; }
    if flags.vending { flag_str += "vending "; }
    if flags.provides_light { flag_str += "provides_light "; }
    if flags.night_vision { flag_str += "night_vision "; }
    if flags.fishing_rod { flag_str += "fishing_rod "; }
    if flags.bait { flag_str += "bait "; }
    if flags.foraging_tool { flag_str += "foraging_tool "; }
    if flags.waterproof { flag_str += "waterproof "; }
    if flags.provides_warmth { flag_str += "provides_warmth "; }
    if flags.reduces_glare { flag_str += "reduces_glare "; }
    if flags.medical_tool { flag_str += "medical_tool "; }
    if flags.preserves_contents { flag_str += "preserves_contents "; }
    if flags.death_only { flag_str += "death_only "; }
    if flags.atm { flag_str += "atm "; }
    if flags.broken { flag_str += "broken "; }
    // watering_can removed - use liquid_container with water instead
    if flags.plant_pot { flag_str += "plant_pot "; }
    if flags.lockpick { flag_str += "lockpick "; }
    if flags.is_skinned { flag_str += "is_skinned "; }
    if flags.boat { flag_str += "boat "; }
    if flags.buried { flag_str += "buried "; }
    if flags.can_dig { flag_str += "can_dig "; }
    if flags.detect_buried { flag_str += "detect_buried "; }
    if flag_str == "" { flag_str = "(none)"; }
    output += flag_str + "\n";
    return output;
}

fn build_triggers_display(item_id) {
    let triggers = get_item_triggers(item_id);
    if triggers.len() == 0 {
        return "Triggers: (none)\n";
    }
    let output = "Triggers:\n";
    let idx = 0;
    for trigger in triggers {
        let enabled_str = "[ON] ";
        if !trigger.enabled { enabled_str = "[OFF]"; }
        let ttype = get_item_trigger_type(trigger);
        output += "  " + idx + ": " + enabled_str + " " + ttype + " -> " + trigger.script_name;
        if trigger.chance < 100 {
            output += " (" + trigger.chance + "%)";
        }
        output += "\n";
        idx += 1;
    }
    return output;
}

fn build_container_display(item) {
    let output = "\n--- Container Properties ---\n";
    let items_str = "unlimited";
    if item.container_max_items > 0 { items_str = "" + item.container_max_items; }
    let weight_str = "unlimited";
    if item.container_max_weight > 0 { weight_str = "" + item.container_max_weight; }
    output += "Capacity: " + items_str + " items, " + weight_str + " weight\n";
    // Weight reduction when worn
    if item.weight_reduction > 0 {
        let effective = 100 - item.weight_reduction;
        output += "Weight Reduction: " + item.weight_reduction + "% (contents weigh " + effective + "% when worn)\n";
    } else {
        output += "Weight Reduction: none (use 'weightreduction 50' for backpack-style reduction)\n";
    }
    output += "Closed: ";
    if item.container_closed { output += "Yes\n"; } else { output += "No\n"; }
    output += "Locked: ";
    if item.container_locked { output += "Yes\n"; } else { output += "No\n"; }
    output += build_key_display(item);
    if item.flags.preserves_contents {
        let labels = ["none", "cool/fridge", "frozen/freezer"];
        let plevel = item.preservation_level;
        if plevel < 0 || plevel > 2 { plevel = 0; }
        output += "Preservation: " + labels[plevel] + " (level " + plevel + ")\n";
    }
    let contents = item.container_contents;
    output += "Contents: " + contents.len() + " items\n";
    return output;
}

fn build_key_display(item) {
    if item.container_key_vnum != () && item.container_key_vnum != "" {
        let proto = get_item_by_vnum(item.container_key_vnum);
        if proto != () {
            return "Key: " + proto.name + " (vnum: " + item.container_key_vnum + ")\n";
        } else {
            return "Key: " + item.container_key_vnum + " (prototype not found)\n";
        }
    }
    return "Key: (none)\n";
}

fn build_liquid_display(item) {
    let output = "\n--- Liquid Container Properties ---\n";
    output += "Liquid: " + item.liquid_type;
    output += " (" + item.liquid_current + "/" + item.liquid_max + " sips)\n";
    output += "Poisoned: ";
    if item.liquid_poisoned { output += "Yes\n"; } else { output += "No\n"; }
    output += build_effects_display(item.liquid_effects);
    return output;
}

fn build_food_display(item) {
    let output = "\n--- Food Properties ---\n";
    output += "Nutrition: " + item.food_nutrition + "\n";
    output += "Poisoned: ";
    if item.food_poisoned { output += "Yes\n"; } else { output += "No\n"; }
    output += build_spoil_display(item);
    output += build_effects_display(item.food_effects);
    return output;
}

fn build_fishing_rod_display(item) {
    let output = "\n--- Fishing Rod Properties ---\n";
    output += "Quality: " + item.quality + "/100 (+" + item.quality + "% catch bonus)\n";
    return output;
}

fn build_bait_display(item) {
    let output = "\n--- Bait Properties ---\n";
    output += "Quality: " + item.quality + "/100 (+" + item.quality + "% bite bonus)\n";
    if item.bait_uses <= 1 {
        output += "Uses: Single use\n";
    } else {
        output += "Uses: " + item.bait_uses + " remaining\n";
    }
    return output;
}

fn build_spoil_display(item) {
    if item.food_spoil_duration == 0 {
        return "Spoilage: Never\n";
    }
    let hours = item.food_spoil_duration / 3600.0;
    let output = "Spoil Duration: " + hours + " hours (~" + hours + " game days)\n";
    let pct = (item.food_spoilage_points * 100.0);
    output += "Spoilage Points: " + pct + "%\n";
    output += "Freshness: " + item.freshness_label + "\n";
    return output;
}

fn build_effects_display(effects) {
    if effects.len() == 0 {
        return "Effects: (none)\n";
    }
    let output = "Effects:\n";
    for effect in effects {
        output += "  - " + effect.effect_type;
        output += " (mag: " + effect.magnitude;
        output += ", dur: " + effect.duration + "s)\n";
    }
    return output;
}

fn build_stats_display(item) {
    if !item.has_stats {
        return "Stats: (none)\n";
    }
    let output = "Stats:";
    if item.stat_str != 0 { output += " STR " + format_stat(item.stat_str); }
    if item.stat_dex != 0 { output += " DEX " + format_stat(item.stat_dex); }
    if item.stat_con != 0 { output += " CON " + format_stat(item.stat_con); }
    if item.stat_int != 0 { output += " INT " + format_stat(item.stat_int); }
    if item.stat_wis != 0 { output += " WIS " + format_stat(item.stat_wis); }
    if item.stat_cha != 0 { output += " CHA " + format_stat(item.stat_cha); }
    if item.insulation != 0 { output += " Insulation: " + item.insulation; }
    output += "\n";
    return output;
}

fn format_stat(value) {
    if value > 0 {
        return "+" + value;
    }
    return "" + value;
}

fn build_commands_help(item_id, item) {
    // Use vnum if available for cleaner command examples
    let display_id = item_id;
    if item.vnum != () && item.vnum != "" {
        display_id = item.vnum;
    }

    let output = "\nCommands:\n";
    output += "  oedit " + display_id + " name <text>\n";
    output += "  oedit " + display_id + " short <text>          (AI: help <prompt> | rephrase)\n";
    output += "  oedit " + display_id + " long <text>           (AI: help <prompt> | rephrase)\n";
    output += "  oedit " + display_id + " keywords <kw1> <kw2> ...\n";
    output += "  oedit " + display_id + " type <armor|weapon|ammunition|container|liquid_container|food|key|misc>\n";
    output += "  oedit " + display_id + " wear <loc1> <loc2> ...\n";
    output += "  oedit " + display_id + " flags\n";
    output += "  oedit " + display_id + " flag <name> [on|off]\n";
    output += "  oedit " + display_id + " weight <value>\n";
    output += "  oedit " + display_id + " value <gold>\n";
    output += "  oedit " + display_id + " world_max <n>              (cap live copies world-wide; 0 = unlimited)\n";
    output += "  oedit " + display_id + " level <value>\n";
    output += "  oedit " + display_id + " stat <str|dex|con|int|wis|cha> <value>\n";
    output += "  oedit " + display_id + " category                    (recipe category management)\n";
    output += "  oedit " + display_id + " teaches <recipe_id|clear>  (recipe book/scroll)\n";
    output += "  oedit " + display_id + " teaches_spell <spell_id|clear>  (spell scroll)\n";
    output += "  oedit " + display_id + " transport <vnum|clear>     (transport status sign)\n";
    output += "  oedit " + display_id + " note [clear]               (multi-line readable body)\n";

    // Weapon commands
    if item.item_type == "weapon" {
        output += "\nWeapon Commands:\n";
        output += "  oedit " + display_id + " damage <count> <sides>\n";
        output += "  oedit " + display_id + " damtype <type>\n";
        output += "  oedit " + display_id + " twohanded [on|off]\n";
        output += "  oedit " + display_id + " wskill <skill|none>\n";
        output += "  oedit " + display_id + " caliber <type|none>      (ammo type: arrow, bolt, etc.)\n";
        output += "  oedit " + display_id + " rangedtype <bow|crossbow|firearm|none>\n";
        output += "  oedit " + display_id + " magsize <capacity>       (magazine capacity)\n";
        output += "  oedit " + display_id + " loadedammo <count>       (current loaded rounds)\n";
        output += "  oedit " + display_id + " firemode <single|burst|auto>\n";
        output += "  oedit " + display_id + " firemodes <modes...>     (supported fire modes)\n";
        output += "  oedit " + display_id + " noise <silent|quiet|normal|loud|clear>\n";
    }

    // Ammunition commands
    if item.item_type == "ammunition" {
        output += "\nAmmunition Commands:\n";
        output += "  oedit " + display_id + " caliber <type|none>      (ammo type: arrow, bolt, etc.)\n";
        output += "  oedit " + display_id + " ammocount <amount>       (stack size)\n";
        output += "  oedit " + display_id + " ammobonus <bonus>        (damage bonus per shot)\n";
        output += "  oedit " + display_id + " ammoeffect <type> <dur> <dmg>  (fire/cold/poison/acid)\n";
        output += "  oedit " + display_id + " ammoeffect clear         (remove ammo effect)\n";
    }

    // Attachment commands (any item can be an attachment)
    if item.attachment_slot != "" {
        output += "\nAttachment Commands:\n";
        output += "  oedit " + display_id + " attachment slot <scope|suppressor|magazine|accessory|clear>\n";
        output += "  oedit " + display_id + " attachment accuracy <bonus>\n";
        output += "  oedit " + display_id + " attachment noise <reduction>\n";
        output += "  oedit " + display_id + " attachment magazine <bonus%>\n";
        output += "  oedit " + display_id + " attachment compatible <bow,crossbow,firearm>\n";
    } else {
        output += "  oedit " + display_id + " attachment             - Configure as weapon attachment\n";
    }

    // Armor commands
    if item.item_type == "armor" {
        output += "\nArmor Commands:\n";
        output += "  oedit " + display_id + " ac <value>\n";
        output += "  oedit " + display_id + " protects [add|remove|clear] <body_part>\n";
        output += "  oedit " + display_id + " insulation <0-100>\n";
    }

    // Fishing/bait/tool commands
    if item.flags.fishing_rod || item.flags.bait || item.flags.foraging_tool {
        output += "\nTool Commands:\n";
        output += "  oedit " + display_id + " quality <0-100>            (fishing rod/bait/tool quality)\n";
    }

    // Bait commands
    if item.flags.bait {
        output += "  oedit " + display_id + " baituses <count>           (0-1 = single use)\n";
    }

    // Medical commands
    if item.flags.medical_tool {
        output += "\nMedical Commands:\n";
        output += "  oedit " + display_id + " medical                    (medical tool config)\n";
    }

    // Vending commands
    if item.flags.vending {
        output += "\nVending Commands:\n";
        output += "  oedit " + display_id + " vending                    (vending machine config)\n";
    }

    // Container commands
    if item.is_container {
        output += "\nContainer Commands:\n";
        output += "  oedit " + display_id + " capacity <max_items> <max_weight>\n";
        output += "  oedit " + display_id + " weightreduction <0-100>  (50=backpack default)\n";
        output += "  oedit " + display_id + " closed [on|off]\n";
        output += "  oedit " + display_id + " locked [on|off]\n";
        output += "  oedit " + display_id + " key <key_item_id|none>\n";
    }

    // Liquid container commands
    if item.is_liquid_container {
        output += "\nLiquid Container Commands:\n";
        output += "  oedit " + display_id + " liquid <type> <current> <max>\n";
        output += "  oedit " + display_id + " fill\n";
        output += "  oedit " + display_id + " empty\n";
        output += "  oedit " + display_id + " liqpoison [on|off]\n";
        output += "  oedit " + display_id + " liqeffect <type> <magnitude> <duration>\n";
        output += "  oedit " + display_id + " clearliqeffects\n";
    }

    // Food commands
    if item.is_food {
        output += "\nFood Commands:\n";
        output += "  oedit " + display_id + " nutrition <value>\n";
        output += "  oedit " + display_id + " spoil <hours>\n";
        output += "  oedit " + display_id + " foodpoison [on|off]\n";
        output += "  oedit " + display_id + " foodeffect <type> <magnitude> <duration>\n";
        output += "  oedit " + display_id + " clearfoodeffects\n";
        output += "  oedit " + display_id + " resetfresh\n";
        output += "  oedit " + display_id + " spoilage <0.0-1.0>     (set spoilage points)\n";
    }

    // Container preservation commands
    if item.is_container {
        output += "  oedit " + display_id + " preservation <0|1|2>   (0=none, 1=fridge, 2=freezer)\n";
    }

    // Prototype commands
    output += "\nPrototype Commands:\n";
    output += "  oedit " + display_id + " prototype [on|off]\n";
    output += "  oedit " + display_id + " vnum <name|none>\n";

    // Trigger commands
    let triggers = get_item_triggers(item.id);
    if triggers.len() > 0 {
        output += "\nTrigger Commands:\n";
        output += "  oedit " + display_id + " trigger list\n";
        output += "  oedit " + display_id + " trigger add <get|drop|use|examine|on_prompt> <script>\n";
        output += "  oedit " + display_id + " trigger remove <index>\n";
        output += "  oedit " + display_id + " trigger enable/disable <index>\n";
        output += "  oedit " + display_id + " trigger chance <index> <1-100>\n";
    } else {
        output += "  oedit " + display_id + " trigger list|add      - Manage triggers\n";
    }

    output += "\n  oedit " + display_id + " spawn";
    return output;
}

fn show_item_flags(item, item_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = item_id;
    if item.vnum != () && item.vnum != "" {
        display_id = item.vnum;
    }

    let flags = item.flags;
    let output = olc::c_header("=== Item Flags for " + item.name + " ===", connection_id) + "\n\n";

    output += build_flag_line("no_drop", flags.no_drop, "Cannot be dropped", display_id, connection_id);
    output += build_flag_line("no_get", flags.no_get, "Cannot be picked up", display_id, connection_id);
    output += build_flag_line("no_remove", flags.no_remove, "Cannot be removed once worn", display_id, connection_id);
    output += build_flag_line("invisible", flags.invisible, "Hidden from normal view", display_id, connection_id);
    output += build_flag_line("glow", flags.glow, "Emits light", display_id, connection_id);
    output += build_flag_line("hum", flags.hum, "Makes humming sound", display_id, connection_id);
    output += build_flag_line("magical", flags.magical, "Surrounded by a magical aura (visible with detect_magic)", display_id, connection_id);
    output += build_flag_line("no_sell", flags.no_sell, "Cannot be sold to shops", display_id, connection_id);
    output += build_flag_line("unique", flags.unique, "Only one can exist in world", display_id, connection_id);
    output += build_flag_line("quest_item", flags.quest_item, "Special quest item", display_id, connection_id);
    output += build_flag_line("vending", flags.vending, "Functions as a vending machine (configure via 'vending' subcommand)", display_id, connection_id);
    output += build_flag_line("provides_light", flags.provides_light, "Provides light when equipped/wielded", display_id, connection_id);
    output += build_flag_line("night_vision", flags.night_vision, "Grants night vision when equipped (CircleMUD AFF_INFRAVISION)", display_id, connection_id);
    output += build_flag_line("fishing_rod", flags.fishing_rod, "Can be used for fishing when held", display_id, connection_id);
    output += build_flag_line("bait", flags.bait, "Can be used as fishing bait", display_id, connection_id);
    output += build_flag_line("foraging_tool", flags.foraging_tool, "Can be used as foraging tool (uses quality for bonus)", display_id, connection_id);
    output += build_flag_line("waterproof", flags.waterproof, "Protects wearer from rain/water when equipped", display_id, connection_id);
    output += build_flag_line("provides_warmth", flags.provides_warmth, "Radiates warmth to room (campfire, fireplace)", display_id, connection_id);
    output += build_flag_line("reduces_glare", flags.reduces_glare, "Reduces bright light penalty (sunglasses)", display_id, connection_id);
    output += build_flag_line("medical_tool", flags.medical_tool, "Functions as a medical tool (configure via 'medical' subcommand)", display_id, connection_id);
    output += build_flag_line("preserves_contents", flags.preserves_contents, "Container preserves food inside (fridge/freezer)", display_id, connection_id);
    output += build_flag_line("death_only", flags.death_only, "Only visible in corpse after death", display_id, connection_id);
    output += build_flag_line("atm", flags.atm, "Functions as an ATM for banking", display_id, connection_id);
    output += build_flag_line("broken", flags.broken, "Broken (cannot be used as ammo)", display_id, connection_id);
    output += build_flag_line("plant_pot", flags.plant_pot, "Can be used as a planting container", display_id, connection_id);
    output += build_flag_line("lockpick", flags.lockpick, "Can be used to pick locks", display_id, connection_id);
    output += build_flag_line("is_skinned", flags.is_skinned, "Corpse has been butchered/skinned", display_id, connection_id);
    output += build_flag_line("boat", flags.boat, "Allows traversing deep_water rooms when in inventory", display_id, connection_id);
    output += build_flag_line("buried", flags.buried, "Hidden in a dirt_floor room until dug up", display_id, connection_id);
    output += build_flag_line("can_dig", flags.can_dig, "Held/equipped, lets the player dig", display_id, connection_id);
    output += build_flag_line("detect_buried", flags.detect_buried, "Hints at buried items in the room when held", display_id, connection_id);

    send_mxp_message(connection_id, output);
}

fn build_flag_line(name, value, desc, item_id, connection_id) {
    let status = "[OFF]";
    if value { status = "[ON] "; }

    let toggle_link = mxp_or(
        mxp_send("oedit " + item_id + " flag " + name, status),
        status,
        connection_id
    );
    return toggle_link + " " + name + " - " + desc + "\n";
}

// === Level and Stat Subcommands ===

fn handle_level_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " level <value> (0 = no requirement)");
        return;
    }
    let level = parse_int(parts[2]);
    if level == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if level < 0 {
        send_client_message(connection_id, "Level requirement cannot be negative.");
        return;
    }
    set_item_level_requirement(item_id, level);
    if level == 0 {
        send_client_message(connection_id, "Level requirement removed.");
    } else {
        send_client_message(connection_id, "Level requirement set to: " + level);
    }
}

fn handle_stat_subcommand(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " stat <str|dex|con|int|wis|cha> <value>");
        return;
    }
    let stat_name = parts[2].to_lower();
    let value = parse_int(parts[3]);
    if value == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    let valid_stats = ["str", "dex", "con", "int", "wis", "cha"];
    let is_valid = false;
    for s in valid_stats {
        if stat_name == s { is_valid = true; }
    }
    if !is_valid {
        send_client_message(connection_id, "Invalid stat. Use: str, dex, con, int, wis, cha");
        return;
    }
    set_item_stat(item_id, stat_name, value);
    let full_name = get_stat_full_name(stat_name);
    if value == 0 {
        send_client_message(connection_id, full_name + " bonus removed.");
    } else if value > 0 {
        send_client_message(connection_id, full_name + " bonus set to: +" + value);
    } else {
        send_client_message(connection_id, full_name + " bonus set to: " + value);
    }
}

fn get_stat_full_name(stat) {
    if stat == "str" { return "Strength"; }
    if stat == "dex" { return "Dexterity"; }
    if stat == "con" { return "Constitution"; }
    if stat == "int" { return "Intelligence"; }
    if stat == "wis" { return "Wisdom"; }
    if stat == "cha" { return "Charisma"; }
    return stat;
}

fn handle_insulation_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <item> insulation <0-100>");
        return;
    }
    let value = parse_int(parts[2]);
    if value == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if value < 0 || value > 100 {
        send_client_message(connection_id, "Insulation must be between 0 and 100.");
        return;
    }
    set_item_insulation(item_id, value);
    if value == 0 {
        send_client_message(connection_id, "Insulation removed.");
    } else {
        send_client_message(connection_id, "Insulation set to: " + value);
    }
}

// === Prototype Subcommands ===

fn handle_prototype_subcommand(item_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_item_prototype(item_id, new_val);
    let status = "a prototype template";
    if !new_val { status = "a normal item"; }
    send_client_message(connection_id, "Item is now: " + status);
}

fn handle_vnum_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit " + item_id + " vnum <name> (or 'none' to clear)");
        return;
    }
    let vnum = parts[2];
    if vnum.to_lower() == "none" || vnum == "" {
        set_item_vnum(item_id, "");
        send_client_message(connection_id, "VNUM cleared.");
    } else {
        // Check if vnum is already in use
        let existing = get_item_by_vnum(vnum);
        if existing != () && existing.id != item_id {
            send_client_message(connection_id, "VNUM '" + vnum + "' is already in use by: " + existing.name);
            return;
        }
        set_item_vnum(item_id, vnum);
        send_client_message(connection_id, "VNUM set to: " + vnum);
    }
}

// === Trigger Subcommands ===

fn handle_trigger_subcommand(item, item_id, parts, connection_id) {
    // oedit <id> trigger <action> [args...]
    // actions: list, add, remove, enable, disable, chance

    if parts.len() < 3 {
        show_trigger_help(item_id, connection_id);
        return;
    }

    let action = parts[2].to_lower();

    if action == "list" {
        list_item_triggers(item, item_id, connection_id);
        return;
    }

    if action == "add" {
        add_trigger(item_id, parts, connection_id);
        return;
    }

    if action == "remove" {
        remove_trigger(item_id, parts, connection_id);
        return;
    }

    if action == "enable" {
        set_trigger_state(item_id, parts, true, connection_id);
        return;
    }

    if action == "disable" {
        set_trigger_state(item_id, parts, false, connection_id);
        return;
    }

    if action == "chance" {
        set_trigger_chance_cmd(item_id, parts, connection_id);
        return;
    }

    if action == "test" {
        test_item_trigger_cmd(item_id, parts, connection_id);
        return;
    }

    if action == "view" {
        view_trigger_script(parts, connection_id);
        return;
    }

    show_trigger_help(item_id, connection_id);
}

fn show_trigger_help(item_id, connection_id) {
    let output = "Item Trigger Commands:\n";
    output += "  oedit " + item_id + " trigger list\n";
    output += "  oedit " + item_id + " trigger add <type> <script|@template> [args...]\n";
    output += "  oedit " + item_id + " trigger remove <index>\n";
    output += "  oedit " + item_id + " trigger enable <index>\n";
    output += "  oedit " + item_id + " trigger disable <index>\n";
    output += "  oedit " + item_id + " trigger chance <index> <1-100>\n";
    output += "  oedit " + item_id + " trigger test <index>\n";
    output += "  oedit " + item_id + " trigger view <script_name>\n";
    output += "\nTrigger types: get, drop, use, examine, on_prompt\n";
    output += "\nScript triggers: scripts/triggers/<script>.rhai\n";
    output += "  Available: " + list_available_trigger_scripts().join(", ") + "\n";
    output += "\nTemplate triggers (no script needed):\n";
    output += "  @message <text>              - Display message to user\n";
    output += "  @random_message <msg1|msg2>  - Display random message\n";
    output += "  @block_message <text>        - Block action and show message\n";
    send_client_message(connection_id, output);
}

fn test_item_trigger_cmd(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> trigger test <index>");
        return;
    }
    let idx = parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }

    send_client_message(connection_id, "Testing trigger " + idx + "...");

    let result = test_item_trigger(item_id, idx, connection_id);

    if result.success {
        send_client_message(connection_id, olc::c_header("Trigger executed successfully!", connection_id));
        send_client_message(connection_id, "Result: " + result.result);
    } else {
        send_client_message(connection_id, olc::c_warn("Trigger test failed: ", connection_id) + result.error);
    }
}

fn view_trigger_script(parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> trigger view <script_name>\nAvailable: " + list_available_trigger_scripts().join(", "));
        return;
    }
    let script_name = parts[3];
    let result = get_trigger_script_content(script_name);
    if result.success {
        let output = olc::c_header("=== Script: " + script_name + ".rhai ===", connection_id) + "\n";
        output += result.content;
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, olc::c_warn("Error: ", connection_id) + result.error);
        send_client_message(connection_id, "Available scripts: " + list_available_trigger_scripts().join(", "));
    }
}

fn list_item_triggers(item, item_id, connection_id) {
    let triggers = get_item_triggers(item_id);
    if triggers.len() == 0 {
        send_client_message(connection_id, "No triggers on this item.");
        return;
    }

    let output = olc::c_header("=== Triggers on " + item.name + " ===", connection_id) + "\n";
    let idx = 0;
    for trigger in triggers {
        let enabled_str = "[ON] ";
        if !trigger.enabled { enabled_str = "[OFF]"; }
        let ttype = get_item_trigger_type(trigger);
        output += idx + ": " + enabled_str + " " + ttype + " -> " + trigger.script_name;
        if trigger.chance < 100 {
            output += " (" + trigger.chance + "% chance)";
        }
        output += "\n";
        idx += 1;
    }
    send_client_message(connection_id, output);
}

fn is_valid_item_trigger_type(trigger_type) {
    trigger_type == "get" || trigger_type == "drop" || trigger_type == "use" || trigger_type == "examine" || trigger_type == "on_get" || trigger_type == "on_drop" || trigger_type == "on_use" || trigger_type == "on_examine" || trigger_type == "on_prompt" || trigger_type == "prompt"
}

fn add_item_template_trigger(item_id, trigger_type, script_name, parts, connection_id) {
    let template_name = script_name.sub_string(1);  // Remove @ prefix

    // Collect remaining parts as args
    let args = [];
    if parts.len() > 5 {
        let i = 5;
        while i < parts.len() {
            args.push(parts[i]);
            i += 1;
        }
    }

    // For @random_message, parse pipe-separated values from first arg
    if template_name == "random_message" && args.len() > 0 {
        let first_arg = args[0];
        if first_arg.contains("|") {
            args = first_arg.split("|");
        }
    }

    // Combine remaining args for templates that take a message
    if template_name == "message" || template_name == "block_message" {
        if args.len() > 0 {
            let combined = "";
            let i = 0;
            for arg in args {
                if i > 0 { combined += " "; }
                combined += arg;
                i += 1;
            }
            args = [combined];
        }
    }

    if args.len() == 0 {
        send_client_message(connection_id, "Template " + script_name + " requires arguments.");
        return;
    }

    if add_item_trigger_with_args(item_id, trigger_type, script_name, args) {
        send_client_message(connection_id, "Added " + trigger_type + " template: " + script_name);
    } else {
        send_client_message(connection_id, "Failed to add template trigger.");
    }
}

fn add_trigger(item_id, parts, connection_id) {
    // oedit <id> trigger add <type> <script|@template> [args...]
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: oedit <id> trigger add <get|drop|use|examine|on_prompt> <script_name>");
        return;
    }
    let trigger_type = parts[3].to_lower();
    let script_name = parts[4];

    if !is_valid_item_trigger_type(trigger_type) {
        send_client_message(connection_id, "Invalid trigger type. Use: get, drop, use, examine, on_prompt");
        return;
    }

    // Check if template (starts with @)
    if script_name.starts_with("@") {
        add_item_template_trigger(item_id, trigger_type, script_name, parts, connection_id);
        return;
    }

    // Regular script - validate before adding
    let validation = validate_trigger_script(script_name);
    if !validation.valid {
        send_client_message(connection_id, olc::c_warn("Script validation failed: ", connection_id) + validation.error);
        send_client_message(connection_id, "Available scripts: " + list_available_trigger_scripts().join(", "));
        return;
    }

    if add_item_trigger(item_id, trigger_type, script_name) {
        send_client_message(connection_id, "Added " + trigger_type + " trigger: " + script_name);
    } else {
        send_client_message(connection_id, "Failed to add trigger.");
    }
}

fn remove_trigger(item_id, parts, connection_id) {
    // oedit <id> trigger remove <index>
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> trigger remove <index>");
        return;
    }
    let idx = parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }
    if remove_item_trigger(item_id, idx) {
        send_client_message(connection_id, "Removed trigger at index " + idx + ".");
    } else {
        send_client_message(connection_id, "Failed to remove trigger. Check the index.");
    }
}

fn set_trigger_state(item_id, parts, enabled, connection_id) {
    // oedit <id> trigger enable/disable <index>
    if parts.len() < 4 {
        let action = "enable";
        if !enabled { action = "disable"; }
        send_client_message(connection_id, "Usage: oedit <id> trigger " + action + " <index>");
        return;
    }
    let idx = parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }
    if set_item_trigger_enabled(item_id, idx, enabled) {
        let state = "enabled";
        if !enabled { state = "disabled"; }
        send_client_message(connection_id, "Trigger at index " + idx + " " + state + ".");
    } else {
        send_client_message(connection_id, "Failed to update trigger. Check the index.");
    }
}

fn set_trigger_chance_cmd(item_id, parts, connection_id) {
    // oedit <id> trigger chance <index> <1-100>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: oedit <id> trigger chance <index> <1-100>");
        return;
    }
    let idx = parse_int(parts[3]);
    let chance = parse_int(parts[4]);
    if idx == () || chance == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if chance < 1 || chance > 100 {
        send_client_message(connection_id, "Chance must be between 1 and 100.");
        return;
    }
    if set_item_trigger_chance(item_id, idx, chance) {
        send_client_message(connection_id, "Trigger at index " + idx + " now has " + chance + "% chance to fire.");
    } else {
        send_client_message(connection_id, "Failed to update trigger. Check the index.");
    }
}

// === Vending Machine Subcommands ===

fn handle_vending_subcommand(item, item_id, parts, connection_id) {
    // oedit <id> vending                     - Show vending config
    // oedit <id> vending stock add <vnum>    - Add item to stock
    // oedit <id> vending stock remove <vnum> - Remove from stock
    // oedit <id> vending sellrate <percent>  - Set sell rate (>=50)

    if parts.len() < 3 {
        show_vending_config(item, item_id, connection_id);
        return;
    }

    let vending_cmd = parts[2].to_lower();

    if vending_cmd == "stock" {
        handle_vending_stock(item_id, parts, connection_id);
        return;
    }

    if vending_cmd == "sellrate" {
        handle_vending_sellrate(item_id, parts, connection_id);
        return;
    }

    show_vending_config(item, item_id, connection_id);
}

fn show_vending_config(item, item_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = item_id;
    if item.vnum != () && item.vnum != "" {
        display_id = item.vnum;
    }

    let output = olc::c_header("=== Vending Configuration for " + item.name + " ===", connection_id) + "\n\n";

    // Check if vending flag is enabled
    let flags = item.flags;
    if !flags.vending {
        output += olc::c_warn("*** WARNING: 'vending' flag is OFF ***", connection_id) + "\n";
        output += "Enable with: oedit " + display_id + " flag vending on\n\n";
    }

    output += "Sell Rate: " + item.vending_sell_rate + "% (charged to players buying items)\n";

    output += "\nStock (infinite):\n";
    let stock = item.vending_stock;
    let sell_rate = item.vending_sell_rate;
    if stock.len() == 0 {
        output += "  (none)\n";
    } else {
        for vnum in stock {
            output += format_vending_stock_line(vnum, sell_rate);
        }
    }

    output += "\nCommands:\n";
    output += "  oedit " + display_id + " vending stock add <vnum>\n";
    output += "  oedit " + display_id + " vending stock remove <vnum>\n";
    output += "  oedit " + display_id + " vending sellrate <50+>\n";

    send_client_message(connection_id, output);
}

fn handle_vending_stock(item_id, parts, connection_id) {
    // oedit <id> vending stock add <vnum>
    // oedit <id> vending stock remove <vnum>

    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: oedit <id> vending stock add|remove <vnum>");
        return;
    }

    let action = parts[3].to_lower();
    let vnum = parts[4];

    if action == "add" {
        // Verify prototype exists
        let proto = get_item_by_vnum(vnum);
        if proto == () {
            send_client_message(connection_id, "Item prototype not found: " + vnum);
            return;
        }

        if !proto.is_prototype {
            send_client_message(connection_id, "Warning: '" + vnum + "' is not marked as a prototype.");
        }

        if add_vending_stock(item_id, vnum) {
            send_client_message(connection_id, "Added '" + proto.name + "' to vending stock.");
        } else {
            send_client_message(connection_id, "Failed to add item to stock.");
        }
        return;
    }

    if action == "remove" {
        if remove_vending_stock(item_id, vnum) {
            send_client_message(connection_id, "Removed '" + vnum + "' from vending stock.");
        } else {
            send_client_message(connection_id, "Failed to remove item from stock.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: oedit <id> vending stock add|remove <vnum>");
}

fn handle_vending_sellrate(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> vending sellrate <50+>");
        return;
    }

    let rate = parse_int(parts[3]);
    if rate == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }

    if rate < 50 {
        send_client_message(connection_id, "Sell rate must be at least 50%.");
        return;
    }

    if set_vending_sell_rate(item_id, rate) {
        send_client_message(connection_id, "Vending sell rate set to " + rate + "%.");
    } else {
        send_client_message(connection_id, "Failed to set sell rate.");
    }
}

fn format_vending_stock_line(vnum, sell_rate) {
    let proto = get_item_by_vnum(vnum);
    if proto != () {
        let price = calculate_sell_price(proto.value, sell_rate);
        return "  " + vnum + " - " + proto.name + " (" + price + " gold)\n";
    } else {
        return "  " + vnum + " - (prototype not found!)\n";
    }
}

// === Item Quality Subcommand ===

fn handle_quality_subcommand(item_id, parts, connection_id) {
    // oedit <id> quality <0-100>
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <id> quality <0-100>");
        return;
    }

    let quality = parse_int(parts[2]);
    if quality == () || quality < 0 || quality > 100 {
        send_client_message(connection_id, "Quality must be a number from 0-100.");
        return;
    }

    if set_item_quality(item_id, quality) {
        send_client_message(connection_id, "Item quality set to " + quality + ".");
    } else {
        send_client_message(connection_id, "Failed to set quality.");
    }
}

fn handle_baituses_subcommand(item_id, parts, connection_id) {
    // oedit <id> baituses <count>  (0-1 = single use)
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <id> baituses <count> (0-1 = single use)");
        return;
    }

    let uses = parse_int(parts[2]);
    if uses == () || uses < 0 {
        send_client_message(connection_id, "Uses must be a non-negative number (0-1 = single use).");
        return;
    }

    if set_bait_uses(item_id, uses) {
        if uses == 0 {
            send_client_message(connection_id, "Bait uses set to infinite.");
        } else {
            send_client_message(connection_id, "Bait uses set to " + uses + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to set bait uses.");
    }
}

fn handle_weightreduction_subcommand(item_id, parts, connection_id) {
    // oedit <id> weightreduction <0-100>
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: oedit <id> weightreduction <0-100>");
        send_client_message(connection_id, "When worn, container contents weigh (100 - reduction)% of normal.");
        send_client_message(connection_id, "Examples:");
        send_client_message(connection_id, "  50 = contents weigh 50% (backpack default)");
        send_client_message(connection_id, "  75 = contents weigh 25% (magical bag)");
        send_client_message(connection_id, "   0 = no weight reduction (chest, crate)");
        return;
    }

    let percent = parse_int(parts[2]);
    if percent == () || percent < 0 || percent > 100 {
        send_client_message(connection_id, "Weight reduction must be a number from 0-100.");
        return;
    }

    if set_item_weight_reduction(item_id, percent) {
        if percent == 0 {
            send_client_message(connection_id, "Weight reduction disabled (contents at full weight).");
        } else {
            let effective = 100 - percent;
            send_client_message(connection_id, "Weight reduction set to " + percent + "% (contents weigh " + effective + "% when worn).");
        }
    } else {
        send_client_message(connection_id, "Failed to set weight reduction.");
    }
}

// === Medical Tool Subcommands ===

fn handle_medical_subcommand(item, item_id, parts, connection_id) {
    // oedit <id> medical                      - Show medical config
    // oedit <id> medical tier <1/2/3>         - Set tier
    // oedit <id> medical uses <num>           - Set uses (0 = reusable)
    // oedit <id> medical treats add <type>    - Add wound type
    // oedit <id> medical treats remove <type> - Remove wound type
    // oedit <id> medical treats clear         - Clear all wound types
    // oedit <id> medical max <severity>       - Set max wound severity

    if parts.len() < 3 {
        show_medical_config(item, item_id, connection_id);
        return;
    }

    let medical_cmd = parts[2].to_lower();

    if medical_cmd == "tier" {
        handle_medical_tier(item_id, parts, connection_id);
        return;
    }
    if medical_cmd == "uses" {
        handle_medical_uses(item_id, parts, connection_id);
        return;
    }
    if medical_cmd == "treats" {
        handle_medical_treats(item_id, parts, connection_id);
        return;
    }
    if medical_cmd == "max" {
        handle_medical_max(item_id, parts, connection_id);
        return;
    }

    show_medical_config(item, item_id, connection_id);
}

fn show_medical_config(item, item_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = item_id;
    if item.vnum != () && item.vnum != "" {
        display_id = item.vnum;
    }

    let output = olc::c_header("=== Medical Tool Configuration for " + item.name + " ===", connection_id) + "\n\n";

    // Check if medical_tool flag is enabled
    let flags = item.flags;
    if !flags.medical_tool {
        output += olc::c_warn("*** WARNING: 'medical_tool' flag is OFF ***", connection_id) + "\n";
        output += "Enable with: oedit " + display_id + " flag medical_tool on\n\n";
    }

    // Tier display
    let tier_name = "Not Set";
    if item.medical_tier == 1 { tier_name = "Basic (1)"; }
    if item.medical_tier == 2 { tier_name = "Intermediate (2)"; }
    if item.medical_tier == 3 { tier_name = "Advanced (3)"; }
    output += "Tier: " + tier_name + "\n";

    // Uses display
    if item.medical_uses == 0 {
        output += "Uses: Reusable (0)\n";
    } else {
        output += "Uses: " + item.medical_uses + " (consumable)\n";
    }

    // Wound types
    let wound_types = item.treats_wound_types;
    if wound_types.len() == 0 {
        output += "Treats: (none)\n";
    } else {
        output += "Treats: " + wound_types.join(", ") + "\n";
    }

    // Max severity
    if item.max_treatable_wound == "" {
        output += "Max Severity: (not set)\n";
    } else {
        output += "Max Severity: " + item.max_treatable_wound + "\n";
    }

    output += "\nCommands:\n";
    output += "  oedit " + display_id + " medical tier <1|2|3>\n";
    output += "  oedit " + display_id + " medical uses <num>           (0 = reusable)\n";
    output += "  oedit " + display_id + " medical treats add <type>\n";
    output += "  oedit " + display_id + " medical treats remove <type>\n";
    output += "  oedit " + display_id + " medical treats clear\n";
    output += "  oedit " + display_id + " medical max <severity>\n";
    output += "\nWound types: cut, puncture, bruise, fracture, burn, frostbite, poisoned, corroded\n";
    output += "Conditions: illness, hypothermia, heat_exhaustion, heat_stroke\n";
    output += "Severities: minor, moderate, severe, critical\n";

    send_client_message(connection_id, output);
}

fn handle_medical_tier(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> medical tier <1|2|3>\n  1 = Basic (bandages, salves)\n  2 = Intermediate (splints, antidotes)\n  3 = Advanced (surgery kit)");
        return;
    }

    let tier = parse_int(parts[3]);
    if tier == () || tier < 1 || tier > 3 {
        send_client_message(connection_id, "Tier must be 1, 2, or 3.");
        return;
    }

    if set_medical_tier(item_id, tier) {
        let tier_name = "Unknown";
        if tier == 1 { tier_name = "Basic"; }
        if tier == 2 { tier_name = "Intermediate"; }
        if tier == 3 { tier_name = "Advanced"; }
        send_client_message(connection_id, "Medical tier set to " + tier_name + " (" + tier + ").");
    } else {
        send_client_message(connection_id, "Failed to set medical tier.");
    }
}

fn handle_medical_uses(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> medical uses <num>\n  0 = Reusable\n  >0 = Consumable (uses remaining)");
        return;
    }

    let uses = parse_int(parts[3]);
    if uses == () || uses < 0 {
        send_client_message(connection_id, "Uses must be a non-negative number (0 = reusable).");
        return;
    }

    if set_medical_uses(item_id, uses) {
        if uses == 0 {
            send_client_message(connection_id, "Medical tool set to reusable.");
        } else {
            send_client_message(connection_id, "Medical tool uses set to " + uses + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to set medical uses.");
    }
}

fn handle_medical_treats(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> medical treats add|remove|clear <type>\nWound types: cut, puncture, bruise, fracture, burn, frostbite, poisoned, corroded\nConditions: illness, hypothermia, heat_exhaustion, heat_stroke");
        return;
    }

    let action = parts[3].to_lower();

    if action == "clear" {
        if clear_treats_wound_types(item_id) {
            send_client_message(connection_id, "Cleared all treatable wound types.");
        } else {
            send_client_message(connection_id, "Failed to clear wound types.");
        }
        return;
    }

    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: oedit <id> medical treats add|remove <type>");
        return;
    }

    let wound_type = parts[4].to_lower();

    // Validate wound type
    let valid_types = ["cut", "puncture", "bruise", "fracture", "burn", "frostbite", "poisoned", "corroded",
                       "illness", "hypothermia", "heat_exhaustion", "heat_stroke"];
    let is_valid = false;
    for vt in valid_types {
        if vt == wound_type { is_valid = true; }
    }

    if !is_valid {
        send_client_message(connection_id, "Invalid wound type: " + wound_type + "\nValid types: cut, puncture, bruise, fracture, burn, frostbite, poisoned, corroded\nConditions: illness, hypothermia, heat_exhaustion, heat_stroke");
        return;
    }

    if action == "add" {
        if add_treats_wound_type(item_id, wound_type) {
            send_client_message(connection_id, "Added '" + wound_type + "' to treatable types.");
        } else {
            send_client_message(connection_id, "Failed to add wound type.");
        }
        return;
    }

    if action == "remove" {
        if remove_treats_wound_type(item_id, wound_type) {
            send_client_message(connection_id, "Removed '" + wound_type + "' from treatable types.");
        } else {
            send_client_message(connection_id, "Failed to remove wound type.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: oedit <id> medical treats add|remove|clear <type>");
}

fn handle_medical_max(item_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: oedit <id> medical max <severity>\nSeverities: minor, moderate, severe, critical");
        return;
    }

    let severity = parts[3].to_lower();
    let valid_severities = ["minor", "moderate", "severe", "critical"];
    let is_valid = false;
    for vs in valid_severities {
        if vs == severity { is_valid = true; }
    }

    if !is_valid {
        send_client_message(connection_id, "Invalid severity: " + severity + "\nValid severities: minor, moderate, severe, critical");
        return;
    }

    if set_max_treatable_wound(item_id, severity) {
        send_client_message(connection_id, "Maximum treatable wound severity set to '" + severity + "'.");
    } else {
        send_client_message(connection_id, "Failed to set max treatable wound.");
    }
}

// Gardening field handlers

fn handle_plantproto_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let item = get_item_data(item_id);
        if item != () {
            send_client_message(connection_id, "Current plant_prototype_vnum: " + item.plant_prototype_vnum);
        }
        send_client_message(connection_id, "Usage: oedit <id> plantproto <vnum>  (set plant prototype vnum for seed items)");
        return;
    }
    let vnum = parts[2];
    if set_item_plant_prototype_vnum(item_id, vnum) {
        send_client_message(connection_id, "Plant prototype vnum set to: " + vnum);
    } else {
        send_client_message(connection_id, "Failed to set plant prototype vnum.");
    }
}

fn handle_fertduration_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let item = get_item_data(item_id);
        if item != () {
            send_client_message(connection_id, "Current fertilizer_duration: " + item.fertilizer_duration + " game hours");
        }
        send_client_message(connection_id, "Usage: oedit <id> fertduration <hours>  (set fertilizer effect duration in game hours)");
        return;
    }
    let hours = parse_int(parts[2]);
    if hours == () {
        send_client_message(connection_id, "Invalid number: " + parts[2]);
        return;
    }
    if set_item_fertilizer_duration(item_id, hours) {
        send_client_message(connection_id, "Fertilizer duration set to: " + hours + " game hours");
    } else {
        send_client_message(connection_id, "Failed to set fertilizer duration.");
    }
}

fn handle_treats_subcommand(item_id, parts, connection_id) {
    if parts.len() < 3 {
        let item = get_item_data(item_id);
        if item != () {
            send_client_message(connection_id, "Current treats_infestation: " + item.treats_infestation);
        }
        send_client_message(connection_id, "Usage: oedit <id> treats <type>  (aphids, blight, root_rot, frost, all, or none)");
        return;
    }
    let treats = parts[2].to_lower();
    if treats == "none" { treats = ""; }
    if set_item_treats_infestation(item_id, treats) {
        if treats == "" {
            send_client_message(connection_id, "Treats infestation cleared.");
        } else {
            send_client_message(connection_id, "Treats infestation set to: " + treats);
        }
    } else {
        send_client_message(connection_id, "Failed to set treats infestation.");
    }
}
