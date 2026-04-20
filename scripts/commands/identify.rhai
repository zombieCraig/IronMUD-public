// scripts/commands/identify.rhai
// identify <item> - Ask a shopkeeper to fully identify an item (costs gold)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Identify what? Usage: identify <item>");
        return;
    }

    let result = validate_shopkeeper(char, connection_id);
    if result == () { return; }
    let shopkeeper = result;

    let keyword = args.to_lower();
    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        send_client_message(connection_id, "You aren't carrying that.");
        return;
    }

    if !shop_will_buy_type(shopkeeper.id, item.item_type) {
        send_client_message(connection_id, shopkeeper.name + " doesn't deal in that type of item.");
        return;
    }

    let cost = calc_identify_cost(item);
    let gold = get_character_gold(char.name);
    if gold < cost {
        send_client_message(connection_id, shopkeeper.name + " charges " + cost + " gold to identify that. You only have " + gold + " gold.");
        return;
    }

    add_character_gold(char.name, -cost);
    broadcast_to_room(char.current_room_id, connection_id,
        shopkeeper.name + " carefully examines " + char.name + "'s " + item.name + ".");

    let output = build_identify_output(item, shopkeeper, cost);
    send_client_message(connection_id, output);
}

fn validate_shopkeeper(char, connection_id) {
    let vending = find_vending_machine_in_room(char.current_room_id);
    if vending != () {
        send_client_message(connection_id, "Vending machines can't identify items.");
        return ();
    }
    let shopkeeper = find_shopkeeper_in_room(char.current_room_id);
    if shopkeeper == () {
        send_client_message(connection_id, "There is no shopkeeper here.");
        return ();
    }
    let activity = get_mobile_activity(shopkeeper.id);
    if activity == "sleeping" {
        send_client_message(connection_id, shopkeeper.name + " is sleeping and can't serve you right now.");
        return ();
    }
    if activity != "working" {
        send_client_message(connection_id, shopkeeper.name + " is not available for business right now.");
        return ();
    }
    return shopkeeper;
}

fn calc_identify_cost(item) {
    let base = item.value / 10;
    if base < 5 { return 5; }
    return base;
}

fn build_identify_output(item, shopkeeper, cost) {
    let output = shopkeeper.name + " identifies your " + item.name + " for " + cost + " gold:";
    output += "\n";
    output += "\n" + item.name;
    output += "\n====================";
    output += "\n" + item.long_desc;
    output += "\n";
    output += "\nType: " + item.item_type;
    output += build_weapon_section(item);
    output += build_armor_section(item);
    output += build_wear_section(item);
    output += "\nWeight: " + item.weight;
    output += "\nValue: " + item.value + " gold";
    output += build_level_section(item);
    output += build_insulation_section(item);
    output += build_all_stats(item);
    output += build_flags_section(item);
    output += build_ranged_section(item);
    return output;
}

fn build_weapon_section(item) {
    if !item.has_damage { return ""; }
    let output = "\nDamage: " + item.damage_dice_count + "d" + item.damage_dice_sides + " (" + item.damage_type + ")";
    let wskill = get_item_weapon_skill(item.id);
    if wskill != () && wskill != "" { output += "\nWeapon Skill: " + wskill; }
    if item.two_handed { output += "\nTwo-Handed: Yes"; }
    return output;
}

fn build_armor_section(item) {
    if !item.has_armor_class { return ""; }
    let output = "\nArmor Class: " + item.armor_class;
    let protects = get_item_protects(item.id);
    if protects.len() > 0 { output += "\nProtects: " + protects.join(", "); }
    return output;
}

fn build_wear_section(item) {
    let wear_locs = item.wear_locations;
    if wear_locs.len() == 0 { return ""; }
    return "\nWorn on: " + wear_locs.join(", ");
}

fn build_level_section(item) {
    if item.level_requirement <= 0 { return ""; }
    return "\nLevel Required: " + item.level_requirement;
}

fn build_insulation_section(item) {
    if item.insulation <= 0 { return ""; }
    return "\nInsulation: " + item.insulation;
}

fn build_all_stats(item) {
    let output = "\n\n--- Stat Bonuses ---";
    output += "\n  STR: " + fmt_bonus(item.stat_str);
    output += "  DEX: " + fmt_bonus(item.stat_dex);
    output += "  CON: " + fmt_bonus(item.stat_con);
    output += "\n  INT: " + fmt_bonus(item.stat_int);
    output += "  WIS: " + fmt_bonus(item.stat_wis);
    output += "  CHA: " + fmt_bonus(item.stat_cha);
    return output;
}

fn build_flags_section(item) {
    let flags = [];
    if item.flags.glow { flags.push("glowing"); }
    if item.flags.hum { flags.push("humming"); }
    if item.flags.invisible { flags.push("invisible"); }
    if item.flags.no_drop { flags.push("cursed (no drop)"); }
    if item.flags.no_get { flags.push("fixed in place"); }
    if item.flags.waterproof { flags.push("waterproof"); }
    if item.flags.provides_warmth { flags.push("provides warmth"); }
    if item.flags.medical_tool { flags.push("medical tool"); }
    if item.flags.lockpick { flags.push("lockpick"); }
    if item.flags.boat { flags.push("boat"); }
    if flags.len() == 0 { return "\nProperties: none"; }
    return "\nProperties: " + flags.join(", ");
}

fn build_ranged_section(item) {
    if !item.has_ranged_type { return ""; }
    let output = "\n\n--- Ranged Details ---";
    let rtype = get_item_ranged_type(item.id);
    output += "\nRanged Type: " + rtype;
    let cal = get_item_caliber(item.id);
    if cal != "" { output += "\nCaliber: " + cal; }
    let mag = get_item_magazine_size(item.id);
    if mag > 0 { output += "\nMagazine: " + mag; }
    let noise = get_item_noise_level(item.id);
    output += "\nNoise Level: " + noise;
    let modes = get_item_supported_fire_modes(item.id);
    if modes.len() > 0 { output += "\nFire Modes: " + modes.join(", "); }
    let fire_mode = get_item_fire_mode(item.id);
    if fire_mode != "" { output += "\nCurrent Mode: " + fire_mode; }
    return output;
}

fn fmt_bonus(value) {
    if value > 0 { return "+" + value; }
    return "" + value;
}
