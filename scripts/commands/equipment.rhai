// scripts/commands/equipment.rhai
// equipment - Show what you're wearing

fn format_location(loc) {
    // Capitalize first letter
    if loc.len() == 0 { return ""; }
    let first = loc.sub_string(0, 1);
    let rest = loc.sub_string(1);
    return "<" + first.to_upper() + rest + ">";
}

fn get_armor_display(item) {
    if !item.has_armor_class { return ""; }

    let holes = get_item_holes(item.id);
    if holes > 0 {
        return get_damaged_armor_display(item, holes);
    }
    return " (AC " + item.armor_class + ")";
}

fn get_damaged_armor_display(item, holes) {
    let effective_ac = calculate_effective_ac(item.id);
    let condition = get_armor_condition(item.id);
    return " (AC " + effective_ac + "/" + item.armor_class + " - " + condition + ")";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let items = get_equipped_items(char.name);

    if items.len() == 0 {
        send_client_message(connection_id, "You aren't wearing anything.");
        return;
    }

    let output = "You are wearing:\n";
    for item in items {
        let locs = item.wear_locations;
        let loc_str = "";
        for loc in locs {
            if loc_str != "" { loc_str += ", "; }
            loc_str += format_location(loc);
        }
        output += "  " + loc_str + " " + item.name;
        output += get_armor_display(item);
        if item.is_ammunition && item.ammo_count > 0 {
            output += " (" + item.ammo_count + " rounds)";
        }
        output += "\n";
    }

    send_client_message(connection_id, output);
}
