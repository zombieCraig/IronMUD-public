// scripts/commands/plantedit.rhai
// plantedit <vnum> [subcommand] or plantedit create <vnum>
// Plant Prototype Editor for builders

import "olc_helpers" as olc;

fn build_show_display(proto, connection_id) {
    let output = olc::c_header("=== Plant Prototype: " + proto.name + " ===", connection_id) + "\n";
    output += olc::c_label("Vnum:", connection_id) + " " + proto.vnum + "\n";
    output += olc::c_label("Name:", connection_id) + " " + proto.name + "\n";
    output += olc::c_label("Keywords:", connection_id) + " ";
    let kws = proto.keywords;
    if kws.len() == 0 { output += "(none)"; }
    else { output += kws.join(", "); }
    output += "\n";
    output += olc::c_label("Category:", connection_id) + " " + proto.category + "\n";
    output += olc::c_label("Seed Vnum:", connection_id) + " " + proto.seed_vnum + "\n";
    output += olc::c_label("Harvest Vnum:", connection_id) + " " + proto.harvest_vnum + "\n";
    output += olc::c_label("Harvest Min/Max:", connection_id) + " " + proto.harvest_min + " / " + proto.harvest_max + "\n";
    output += olc::c_label("Multi-Harvest:", connection_id) + " " + proto.multi_harvest + "\n";
    output += olc::c_label("Indoor Only:", connection_id) + " " + proto.indoor_only + "\n";
    output += olc::c_label("Min Skill:", connection_id) + " " + proto.min_skill_to_plant + "\n";
    output += olc::c_label("Base XP:", connection_id) + " " + proto.base_xp + "\n";
    output += olc::c_label("Pest Resistance:", connection_id) + " " + proto.pest_resistance + "\n";
    output += olc::c_label("Water Consumption:", connection_id) + " " + proto.water_consumption_per_hour + "/hr\n";
    output += olc::c_label("Water Capacity:", connection_id) + " " + proto.water_capacity + "\n";

    output += "\n" + olc::c_label("Preferred Seasons:", connection_id) + " ";
    let pref = proto.preferred_seasons;
    if pref.len() == 0 { output += "(none)"; }
    else { output += pref.join(", "); }
    output += "\n";

    output += olc::c_label("Forbidden Seasons:", connection_id) + " ";
    let forb = proto.forbidden_seasons;
    if forb.len() == 0 { output += "(none)"; }
    else { output += forb.join(", "); }
    output += "\n";

    // Stages
    output += "\n" + olc::c_label("Growth Stages:", connection_id) + "\n";
    let stages = proto.stages;
    if stages.len() == 0 {
        output += "  (none - plant will use default 24hr per stage)\n";
    } else {
        let idx = 0;
        for stage in stages {
            output += "  " + idx + ". [" + stage.stage + "] " + stage.duration_game_hours + " game hrs - " + stage.description + "\n";
            if stage.examine_desc != "" {
                output += "     Examine: " + stage.examine_desc + "\n";
            }
            idx += 1;
        }
    }

    return output;
}

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

    let parts = split_quoted_args(args);
    if parts.len() == 0 {
        send_client_message(connection_id, "Usage: plantedit create <vnum> | plantedit <vnum> [subcommand]");
        return;
    }

    // Handle create
    if parts[0].to_lower() == "create" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: plantedit create <vnum>");
            return;
        }
        let vnum = parts[1];

        // Check if vnum already exists
        let existing = get_plant_prototype_by_vnum(vnum);
        if existing != () {
            send_client_message(connection_id, "Plant prototype with vnum '" + vnum + "' already exists.");
            return;
        }

        let proto = new_plant_prototype(vnum, vnum);
        if save_plant_prototype(proto) {
            send_client_message(connection_id, "Created plant prototype with vnum '" + vnum + "'. Use 'plantedit " + vnum + "' to edit it.");
        } else {
            send_client_message(connection_id, "Failed to create plant prototype.");
        }
        return;
    }

    // Handle list
    if parts[0].to_lower() == "list" {
        let protos = list_plant_prototypes();
        if protos.len() == 0 {
            send_client_message(connection_id, "No plant prototypes exist yet. Use 'plantedit create <vnum>' to create one.");
            return;
        }
        let output = olc::c_header("=== Plant Prototypes ===", connection_id) + "\n";
        for p in protos {
            output += "  " + p.vnum + " - " + p.name + " (" + p.category + ")\n";
        }
        send_client_message(connection_id, output);
        return;
    }

    // Load prototype by vnum
    let vnum = parts[0];
    let proto = get_plant_prototype_by_vnum(vnum);
    if proto == () {
        send_client_message(connection_id, "No plant prototype found with vnum '" + vnum + "'. Use 'plantedit create " + vnum + "' to create one, or 'plantedit list' to see all.");
        return;
    }

    // No subcommand = show
    if parts.len() < 2 {
        send_client_message(connection_id, build_show_display(proto, connection_id));
        return;
    }

    let uuid = proto.id;
    let subcommand = parts[1].to_lower();

    // Show
    if subcommand == "show" {
        send_client_message(connection_id, build_show_display(proto, connection_id));
        return;
    }

    // Delete
    if subcommand == "delete" {
        if delete_plant_prototype(uuid) {
            send_client_message(connection_id, "Deleted plant prototype '" + vnum + "'.");
        } else {
            send_client_message(connection_id, "Failed to delete plant prototype.");
        }
        return;
    }

    // Name
    if subcommand == "name" {
        handle_name(proto, uuid, parts, connection_id);
        return;
    }

    // Seed vnum
    if subcommand == "seed_vnum" || subcommand == "seed" {
        handle_string_field(proto, uuid, "seed_vnum", parts, connection_id);
        return;
    }

    // Harvest vnum
    if subcommand == "harvest_vnum" || subcommand == "harvest" {
        handle_string_field(proto, uuid, "harvest_vnum", parts, connection_id);
        return;
    }

    // Category
    if subcommand == "category" || subcommand == "cat" {
        handle_category(proto, uuid, parts, connection_id);
        return;
    }

    // Harvest min/max
    if subcommand == "harvest_min" {
        handle_int_field(proto, uuid, "harvest_min", parts, connection_id);
        return;
    }
    if subcommand == "harvest_max" {
        handle_int_field(proto, uuid, "harvest_max", parts, connection_id);
        return;
    }

    // Skill
    if subcommand == "skill" || subcommand == "min_skill" {
        handle_int_field(proto, uuid, "min_skill_to_plant", parts, connection_id);
        return;
    }

    // XP
    if subcommand == "xp" || subcommand == "base_xp" {
        handle_int_field(proto, uuid, "base_xp", parts, connection_id);
        return;
    }

    // Pest resistance
    if subcommand == "pest_resistance" || subcommand == "resist" {
        handle_int_field(proto, uuid, "pest_resistance", parts, connection_id);
        return;
    }

    // Multi-harvest toggle
    if subcommand == "multi_harvest" || subcommand == "multi" {
        let proto_data = get_plant_prototype(uuid);
        if proto_data != () {
            proto_data.multi_harvest = !proto_data.multi_harvest;
            save_plant_prototype(proto_data);
            send_client_message(connection_id, "Multi-harvest: " + proto_data.multi_harvest);
        }
        return;
    }

    // Indoor only toggle
    if subcommand == "indoor_only" || subcommand == "indoor" {
        set_plant_prototype_indoor_only(uuid, !proto.indoor_only);
        send_client_message(connection_id, "Indoor only: " + (!proto.indoor_only));
        return;
    }

    // Water settings
    if subcommand == "water" {
        handle_water(proto, uuid, parts, connection_id);
        return;
    }

    // Season management
    if subcommand == "season" {
        handle_season(proto, uuid, parts, connection_id);
        return;
    }

    // Stage management
    if subcommand == "stage" {
        handle_stage(proto, uuid, parts, connection_id);
        return;
    }

    // Keyword management
    if subcommand == "keyword" || subcommand == "kw" {
        handle_keyword(proto, uuid, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand + "\nAvailable: show, delete, name, seed_vnum, harvest_vnum, category, harvest_min, harvest_max, skill, xp, pest_resistance, multi_harvest, indoor_only, water, season, stage, keyword");
}

fn handle_name(proto, uuid, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: plantedit <vnum> name <name>");
        return;
    }
    let name = olc::build_string_from_parts(parts, 2);
    let proto_data = get_plant_prototype(uuid);
    if proto_data != () {
        proto_data.name = name;
        save_plant_prototype(proto_data);
        send_client_message(connection_id, "Name set to: " + name);
    }
}

fn handle_string_field(proto, uuid, field, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: plantedit <vnum> " + field + " <value>");
        return;
    }
    let value = parts[2];
    let proto_data = get_plant_prototype(uuid);
    if proto_data != () {
        if field == "seed_vnum" { proto_data.seed_vnum = value; }
        if field == "harvest_vnum" { proto_data.harvest_vnum = value; }
        save_plant_prototype(proto_data);
        send_client_message(connection_id, field + " set to: " + value);
    }
}

fn handle_int_field(proto, uuid, field, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: plantedit <vnum> " + field + " <number>");
        return;
    }
    let value = parse_int(parts[2]);
    if value == () {
        send_client_message(connection_id, "Invalid number: " + parts[2]);
        return;
    }
    let proto_data = get_plant_prototype(uuid);
    if proto_data != () {
        if field == "harvest_min" { proto_data.harvest_min = value; }
        if field == "harvest_max" { proto_data.harvest_max = value; }
        if field == "min_skill_to_plant" { proto_data.min_skill_to_plant = value; }
        if field == "base_xp" { proto_data.base_xp = value; }
        if field == "pest_resistance" { proto_data.pest_resistance = value; }
        save_plant_prototype(proto_data);
        send_client_message(connection_id, field + " set to: " + value);
    }
}

fn handle_category(proto, uuid, parts, connection_id) {
    if parts.len() < 3 {
        let cats = get_all_plant_categories();
        send_client_message(connection_id, "Usage: plantedit <vnum> category <" + cats.join("|") + ">");
        return;
    }
    if set_plant_prototype_category(uuid, parts[2]) {
        send_client_message(connection_id, "Category set to: " + parts[2]);
    } else {
        let cats = get_all_plant_categories();
        send_client_message(connection_id, "Invalid category. Available: " + cats.join(", "));
    }
}

fn handle_water(proto, uuid, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: plantedit <vnum> water <consumption_per_hour> <capacity>");
        send_client_message(connection_id, "Current: consumption=" + proto.water_consumption_per_hour + "/hr, capacity=" + proto.water_capacity);
        return;
    }
    let consumption = parse_float(parts[2]);
    let capacity = parse_float(parts[3]);
    if consumption == () || capacity == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    set_plant_prototype_water(uuid, consumption, capacity);
    send_client_message(connection_id, "Water consumption set to " + consumption + "/hr, capacity " + capacity);
}

fn handle_season(proto, uuid, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: plantedit <vnum> season <add|remove> <preferred|forbidden> <spring|summer|autumn|winter>");
        return;
    }
    let action = parts[2].to_lower();
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: plantedit <vnum> season " + action + " <preferred|forbidden> <spring|summer|autumn|winter>");
        return;
    }
    let list_type = parts[3].to_lower();
    let season_name = parts[4].to_lower();

    if action == "add" {
        if add_plant_prototype_season(uuid, season_name, list_type) {
            send_client_message(connection_id, "Added " + season_name + " to " + list_type + " seasons.");
        } else {
            send_client_message(connection_id, "Failed. Seasons: spring, summer, autumn, winter. Types: preferred, forbidden.");
        }
        return;
    }
    if action == "remove" {
        if remove_plant_prototype_season(uuid, season_name, list_type) {
            send_client_message(connection_id, "Removed " + season_name + " from " + list_type + " seasons.");
        } else {
            send_client_message(connection_id, "Failed to remove season.");
        }
        return;
    }
    send_client_message(connection_id, "Usage: plantedit <vnum> season <add|remove> <preferred|forbidden> <season>");
}

fn handle_stage(proto, uuid, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: plantedit <vnum> stage add <stage> <hours> <description> [examine_desc]");
        send_client_message(connection_id, "       plantedit <vnum> stage remove <index>");
        let stages = get_all_growth_stages();
        send_client_message(connection_id, "Stages: " + stages.join(", "));
        return;
    }

    let action = parts[2].to_lower();

    if action == "add" {
        if parts.len() < 6 {
            send_client_message(connection_id, "Usage: plantedit <vnum> stage add <stage_name> <hours> <description> [examine_desc]");
            return;
        }
        let stage_name = parts[3];
        let hours = parse_int(parts[4]);
        if hours == () {
            send_client_message(connection_id, "Invalid hours: " + parts[4]);
            return;
        }
        let desc = parts[5];
        let examine = "";
        if parts.len() >= 7 {
            examine = parts[6];
        }

        if add_plant_prototype_stage(uuid, stage_name, hours, desc, examine) {
            send_client_message(connection_id, "Added stage '" + stage_name + "' (" + hours + " game hrs).");
        } else {
            let stages = get_all_growth_stages();
            send_client_message(connection_id, "Failed. Valid stages: " + stages.join(", "));
        }
        return;
    }

    if action == "remove" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: plantedit <vnum> stage remove <index>");
            return;
        }
        let index = parse_int(parts[3]);
        if index == () {
            send_client_message(connection_id, "Invalid index.");
            return;
        }
        if remove_plant_prototype_stage(uuid, index) {
            send_client_message(connection_id, "Removed stage at index " + index + ".");
        } else {
            send_client_message(connection_id, "Failed to remove stage.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: plantedit <vnum> stage <add|remove> ...");
}

fn handle_keyword(proto, uuid, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: plantedit <vnum> keyword <add|remove> <keyword>");
        return;
    }
    let action = parts[2].to_lower();
    let keyword = parts[3].to_lower();

    if action == "add" {
        if add_plant_prototype_keyword(uuid, keyword) {
            send_client_message(connection_id, "Added keyword: " + keyword);
        } else {
            send_client_message(connection_id, "Failed to add keyword.");
        }
        return;
    }
    if action == "remove" {
        if remove_plant_prototype_keyword(uuid, keyword) {
            send_client_message(connection_id, "Removed keyword: " + keyword);
        } else {
            send_client_message(connection_id, "Failed to remove keyword.");
        }
        return;
    }
    send_client_message(connection_id, "Usage: plantedit <vnum> keyword <add|remove> <keyword>");
}
