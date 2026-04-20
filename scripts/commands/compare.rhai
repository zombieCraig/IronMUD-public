// scripts/commands/compare.rhai
// compare <item> - Compare an item to your currently equipped item in the same slot

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Compare what? Usage: compare <item>");
        return;
    }

    let item = find_target_item(char, args);
    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    let wear_locs = item.wear_locations;
    if wear_locs.len() == 0 {
        send_client_message(connection_id, "You can only compare equipment.");
        return;
    }

    let equipped = find_equipped_in_slot(char.name, wear_locs);
    if equipped == () {
        let output = build_no_equipped_display(item);
        send_client_message(connection_id, output);
        return;
    }

    // Check if comparing item to itself
    if item.id == equipped.id {
        send_client_message(connection_id, "That item is already equipped.");
        return;
    }

    let output = build_comparison(item, equipped);
    send_client_message(connection_id, output);
}

fn find_target_item(char, args) {
    let keyword = args.to_lower();
    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(keyword, inv_items);
    if item != () { return item; }
    let room_items = get_items_in_room(char.current_room_id);
    item = find_item_by_keyword(keyword, room_items);
    return item;
}

fn find_equipped_in_slot(char_name, wear_locs) {
    let eq_items = get_equipped_items(char_name);
    for eq in eq_items {
        let eq_locs = eq.wear_locations;
        for loc in wear_locs {
            for eq_loc in eq_locs {
                if loc == eq_loc { return eq; }
            }
        }
    }
    return ();
}

fn build_comparison(item, equipped) {
    let item_type = item.item_type;
    if item_type == "weapon" { return build_weapon_full(item, equipped); }
    if item_type == "armor" { return build_armor_full(item, equipped); }
    return build_generic_full(item, equipped);
}

fn build_weapon_full(item, equipped) {
    let verdict = get_weapon_verdict(item, equipped);
    let output = build_header(item, equipped, verdict);
    output += build_weapon_compare(item, equipped);
    if item.has_ranged_type { output += build_ranged_compare(item, equipped); }
    output += build_stat_compare(item, equipped);
    output += build_weight_line(item, equipped);
    return output;
}

fn build_armor_full(item, equipped) {
    let verdict = get_armor_verdict(item, equipped);
    let output = build_header(item, equipped, verdict);
    output += build_armor_compare(item, equipped);
    output += build_stat_compare(item, equipped);
    output += build_weight_line(item, equipped);
    return output;
}

fn build_generic_full(item, equipped) {
    let verdict = get_generic_verdict(item, equipped);
    let output = build_header(item, equipped, verdict);
    output += build_stat_compare(item, equipped);
    output += build_weight_line(item, equipped);
    return output;
}

fn build_header(item, equipped, verdict) {
    let line = "=== Comparing: " + item.name + " vs " + equipped.name + " (equipped) ===";
    line += "\nVerdict: " + verdict;
    line += "\n";
    line += "\n  " + pad_right("Stat", 20) + pad_right("New", 15) + pad_right("Equipped", 15) + "Diff";
    line += "\n  " + pad_right("----", 20) + pad_right("---", 15) + pad_right("--------", 15) + "----";
    return line;
}

fn build_weapon_compare(item, equipped) {
    let output = "";
    let new_avg = calc_damage_avg(item.damage_dice_count, item.damage_dice_sides);
    let eq_avg = calc_damage_avg(equipped.damage_dice_count, equipped.damage_dice_sides);
    let new_dice = "" + item.damage_dice_count + "d" + item.damage_dice_sides;
    let eq_dice = "" + equipped.damage_dice_count + "d" + equipped.damage_dice_sides;
    output += format_diff_str("Damage", new_dice, eq_dice);
    output += format_avg_diff(new_avg, eq_avg);
    output += format_diff_str("Damage Type", item.damage_type, equipped.damage_type);
    let new_ws = get_item_weapon_skill(item.id);
    let eq_ws = get_item_weapon_skill(equipped.id);
    output += format_diff_str("Weapon Skill", new_ws, eq_ws);
    output += format_diff_str("Two-Handed", bool_str(item.two_handed), bool_str(equipped.two_handed));
    return output;
}

fn build_ranged_compare(item, equipped) {
    let output = "";
    let new_mag = get_item_magazine_size(item.id);
    let eq_mag = get_item_magazine_size(equipped.id);
    output += format_diff_num("Magazine", new_mag, eq_mag, "");
    let new_cal = get_item_caliber(item.id);
    let eq_cal = get_item_caliber(equipped.id);
    if new_cal != "" || eq_cal != "" {
        output += format_diff_str("Caliber", new_cal, eq_cal);
    }
    let new_noise = get_item_noise_level(item.id);
    let eq_noise = get_item_noise_level(equipped.id);
    output += format_diff_str("Noise", new_noise, eq_noise);
    let new_modes = get_item_supported_fire_modes(item.id);
    let eq_modes = get_item_supported_fire_modes(equipped.id);
    output += format_diff_str("Fire Modes", new_modes.join(", "), eq_modes.join(", "));
    return output;
}

fn build_armor_compare(item, equipped) {
    let output = "";
    let new_ac = item.armor_class;
    let eq_ac = calculate_effective_ac(equipped.id);
    output += format_diff_num("Armor Class", new_ac, eq_ac, "");
    let eq_condition = get_armor_condition(equipped.id);
    let eq_holes = get_item_holes(equipped.id);
    if eq_holes > 0 {
        output += "\n  " + pad_right("Condition", 20) + pad_right("New", 15) + eq_condition + " (" + eq_holes + " holes)";
    }
    let new_protects = get_item_protects(item.id);
    let eq_protects = get_item_protects(equipped.id);
    output += format_diff_str("Protects", new_protects.join(", "), eq_protects.join(", "));
    output += format_insulation(item, equipped);
    return output;
}

fn format_insulation(item, equipped) {
    let new_ins = item.insulation;
    let eq_ins = equipped.insulation;
    if new_ins == 0 && eq_ins == 0 { return ""; }
    return format_diff_num("Insulation", new_ins, eq_ins, "");
}

fn build_stat_compare(item, equipped) {
    if !item.has_stats && !equipped.has_stats { return ""; }
    let output = "\n  Stat Bonuses:";
    output += build_stat_line("STR", item.stat_str, equipped.stat_str);
    output += build_stat_line("DEX", item.stat_dex, equipped.stat_dex);
    output += build_stat_line("CON", item.stat_con, equipped.stat_con);
    output += build_stat_line("INT", item.stat_int, equipped.stat_int);
    output += build_stat_line("WIS", item.stat_wis, equipped.stat_wis);
    output += build_stat_line("CHA", item.stat_cha, equipped.stat_cha);
    return output;
}

fn build_stat_line(label, new_val, eq_val) {
    if new_val == 0 && eq_val == 0 { return ""; }
    return format_diff_num(label, new_val, eq_val, "");
}

fn build_weight_line(item, equipped) {
    let diff = item.weight - equipped.weight;
    let suffix = "";
    if diff > 0 { suffix = " (heavier)"; }
    if diff < 0 { suffix = " (lighter)"; }
    return format_diff_num("Weight", item.weight, equipped.weight, suffix);
}

fn build_no_equipped_display(item) {
    let output = "=== " + item.name + " (nothing equipped in slot) ===\n";
    output += "\n  Type: " + item.item_type;
    if item.has_damage {
        output += "\n  Damage: " + item.damage_dice_count + "d" + item.damage_dice_sides + " (" + item.damage_type + ")";
        let wskill = get_item_weapon_skill(item.id);
        if wskill != () && wskill != "" { output += "\n  Weapon Skill: " + wskill; }
        if item.two_handed { output += "\n  Two-Handed: Yes"; }
    }
    if item.has_armor_class {
        output += "\n  Armor Class: " + item.armor_class;
        let protects = get_item_protects(item.id);
        if protects.len() > 0 { output += "\n  Protects: " + protects.join(", "); }
    }
    output += build_no_equip_stats(item);
    output += "\n  Weight: " + item.weight;
    if item.insulation > 0 { output += "\n  Insulation: " + item.insulation; }
    return output;
}

fn build_no_equip_stats(item) {
    if !item.has_stats { return ""; }
    let output = "\n  Stat Bonuses:";
    if item.stat_str != 0 { output += " STR " + fmt_bonus(item.stat_str); }
    if item.stat_dex != 0 { output += " DEX " + fmt_bonus(item.stat_dex); }
    if item.stat_con != 0 { output += " CON " + fmt_bonus(item.stat_con); }
    if item.stat_int != 0 { output += " INT " + fmt_bonus(item.stat_int); }
    if item.stat_wis != 0 { output += " WIS " + fmt_bonus(item.stat_wis); }
    if item.stat_cha != 0 { output += " CHA " + fmt_bonus(item.stat_cha); }
    return output;
}

// ---- Verdict functions ----

fn get_weapon_verdict(item, equipped) {
    let score = 0;
    let new_avg = calc_damage_avg(item.damage_dice_count, item.damage_dice_sides);
    let eq_avg = calc_damage_avg(equipped.damage_dice_count, equipped.damage_dice_sides);
    if new_avg > eq_avg { score += 2; }
    if new_avg < eq_avg { score -= 2; }
    score += get_weight_score(item.weight, equipped.weight);
    score += get_stat_score(item, equipped);
    return verdict_label(score);
}

fn get_armor_verdict(item, equipped) {
    let score = 0;
    let new_ac = item.armor_class;
    let eq_ac = calculate_effective_ac(equipped.id);
    if new_ac > eq_ac { score += 2; }
    if new_ac < eq_ac { score -= 2; }
    score += get_weight_score(item.weight, equipped.weight);
    score += get_stat_score(item, equipped);
    return verdict_label(score);
}

fn get_generic_verdict(item, equipped) {
    let score = 0;
    score += get_weight_score(item.weight, equipped.weight);
    score += get_stat_score(item, equipped);
    return verdict_label(score);
}

fn get_weight_score(new_w, eq_w) {
    if new_w < eq_w { return 1; }
    if new_w > eq_w { return -1; }
    return 0;
}

fn get_stat_score(item, equipped) {
    let new_total = item.stat_str + item.stat_dex + item.stat_con + item.stat_int + item.stat_wis + item.stat_cha;
    let eq_total = equipped.stat_str + equipped.stat_dex + equipped.stat_con + equipped.stat_int + equipped.stat_wis + equipped.stat_cha;
    return new_total - eq_total;
}

fn verdict_label(score) {
    if score > 0 { return "UPGRADE"; }
    if score < 0 { return "DOWNGRADE"; }
    return "SIDEGRADE";
}

// ---- Formatting helpers ----

fn calc_damage_avg(count, sides) {
    // Average = count * (sides + 1) / 2.0
    // Use integer math: count * (sides + 1) * 10 / 20 to get tenths
    return count * (sides + 1) * 10 / 20;
}

fn format_avg_diff(new_avg, eq_avg) {
    let diff = new_avg - eq_avg;
    let new_str = fmt_avg(new_avg);
    let eq_str = fmt_avg(eq_avg);
    let diff_str = "";
    if diff > 0 { diff_str = "+" + fmt_avg(diff) + " avg"; }
    if diff < 0 { diff_str = fmt_avg(diff) + " avg"; }
    return "\n  " + pad_right("  (avg damage)", 20) + pad_right(new_str, 15) + pad_right(eq_str, 15) + diff_str;
}

fn fmt_avg(val) {
    let whole = val / 10;
    let frac = val % 10;
    if frac < 0 { frac = -frac; }
    return "" + whole + "." + frac;
}

fn format_diff_num(label, new_val, old_val, suffix) {
    let diff = new_val - old_val;
    let diff_str = "";
    if diff > 0 { diff_str = "+" + diff + suffix; }
    if diff < 0 { diff_str = "" + diff + suffix; }
    return "\n  " + pad_right(label, 20) + pad_right("" + new_val, 15) + pad_right("" + old_val, 15) + diff_str;
}

fn format_diff_str(label, new_val, old_val) {
    let diff_str = "";
    if new_val != old_val { diff_str = "(different)"; }
    return "\n  " + pad_right(label, 20) + pad_right(new_val, 15) + pad_right(old_val, 15) + diff_str;
}

fn pad_right(text, width) {
    let s = "" + text;
    let len = s.len();
    if len >= width { return s; }
    let padding = width - len;
    let i = 0;
    while i < padding {
        s += " ";
        i += 1;
    }
    return s;
}

fn bool_str(val) {
    if val { return "Yes"; }
    return "No";
}

fn fmt_bonus(value) {
    if value > 0 { return "+" + value; }
    return "" + value;
}
