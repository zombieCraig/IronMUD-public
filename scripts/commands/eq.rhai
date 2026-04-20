// scripts/commands/eq.rhai
// eq - Alias for equipment

fn format_location(loc) {
    // Capitalize first letter
    if loc.len() == 0 { return ""; }
    let first = loc.sub_string(0, 1);
    let rest = loc.sub_string(1);
    return "<" + first.to_upper() + rest + ">";
}

fn get_damage_display(item) {
    if !item.has_damage { return ""; }
    let wskill = get_item_weapon_skill(item.id);
    if wskill == "ranged" {
        return " (" + item.damage_dice_count + "d" + item.damage_dice_sides + " ranged)";
    }
    return " (" + item.damage_dice_count + "d" + item.damage_dice_sides + " " + item.damage_type + ")";
}

fn get_ammo_display(item) {
    if item.is_ammunition && item.ammo_count > 0 {
        return " (" + item.ammo_count + " rounds)";
    }
    if !item.has_ranged_type { return ""; }
    let rtype = get_item_ranged_type(item.id);
    if rtype != "crossbow" && rtype != "firearm" { return ""; }
    let loaded = get_item_loaded_ammo(item.id);
    let mag_size = get_item_magazine_size(item.id);
    if mag_size <= 0 { return ""; }
    return " (" + loaded + "/" + mag_size + " loaded)";
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
        if item.has_armor_class {
            output += " (AC " + item.armor_class + ")";
        }
        output += get_damage_display(item);
        output += get_ammo_display(item);
        output += "\n";
    }

    send_client_message(connection_id, output);
}
