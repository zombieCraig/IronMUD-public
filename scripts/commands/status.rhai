// scripts/commands/status.rhai
// status [detailed] - View your character status
// status <mobile> - (Builder/Admin) View mobile debug status

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to view your status.");
        return;
    }

    let arg = args.to_lower();

    // Player status modes
    if arg == "" {
        show_basic_status(char, connection_id);
        return;
    }
    if is_detail_keyword(arg) {
        show_detailed_status(char, connection_id);
        return;
    }

    // Non-empty arg that isn't a detail keyword - try mobile lookup for builders/admins
    if !char.is_builder && !char.is_admin {
        show_basic_status(char, connection_id);
        return;
    }

    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(arg, mobiles);
    if mobile == () {
        send_client_message(connection_id, "You don't see a mobile matching '" + args + "' here.");
        return;
    }

    show_mobile_status(mobile, connection_id);
}

fn is_detail_keyword(arg) {
    if arg == "detailed" { return true; }
    if arg == "detail" { return true; }
    if arg == "full" { return true; }
    return false;
}

// Repeat a character count times
fn repeat_char(ch, count) {
    let result = "";
    let i = 0;
    while i < count {
        result += ch;
        i += 1;
    }
    return result;
}

// Build a visual bar like [########--]
fn build_bar(current, max, width) {
    if max <= 0 {
        return "[" + repeat_char("-", width) + "]";
    }
    let filled = (current * width) / max;
    if filled > width { filled = width; }
    if filled < 0 { filled = 0; }
    let empty = width - filled;
    return "[" + repeat_char("#", filled) + repeat_char("-", empty) + "]";
}

// Color HP string based on percentage
fn get_hp_colored(text, pct, connection_id) {
    if pct >= 70 {
        return color_or(ansi_green(text), text, connection_id);
    }
    if pct >= 30 {
        return color_or(ansi_yellow(text), text, connection_id);
    }
    return color_or(ansi_red(text), text, connection_id);
}

// Color stamina string based on percentage
fn get_stamina_colored(text, pct, connection_id) {
    if pct >= 50 {
        return color_or(ansi_green(text), text, connection_id);
    }
    if pct >= 25 {
        return color_or(ansi_yellow(text), text, connection_id);
    }
    return color_or(ansi_red(text), text, connection_id);
}

// Color mana string based on percentage
fn get_mana_colored(text, pct, connection_id) {
    if pct >= 70 {
        return color_or(ansi_bright_blue(text), text, connection_id);
    }
    if pct >= 30 {
        return color_or(ansi_blue(text), text, connection_id);
    }
    return color_or(ansi_magenta(text), text, connection_id);
}

// Show [X] for filled slot, [ ] for empty
fn slot_indicator(letter, filled, connection_id) {
    if filled {
        return color_or(ansi_green("[" + letter + "]"), "[" + letter + "]", connection_id);
    }
    return "[ ]";
}

// Capitalize first letter of a string
fn capitalize_word(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1);
    return first.to_upper() + rest;
}

// Get spawn point display text
fn get_spawn_point_display(char, connection_id) {
    // First check for rental property in current area
    let current_room = get_room_data(char.current_room_id);
    if current_room != () {
        let area_id = current_room.area_id;
        if area_id != "" {
            let lease = get_player_lease_in_area(char.name, area_id);
            if lease != () && !lease.is_evicted {
                let entrance_id = get_property_entrance(lease.id);
                if entrance_id != "" {
                    let entrance_room = get_room_data(entrance_id);
                    if entrance_room != () {
                        return entrance_room.title + " (rental property)";
                    }
                }
            }
        }
    }

    // Fall back to character's manual spawn room
    let spawn_room_id = char.spawn_room_id;
    if spawn_room_id != "" {
        let spawn_room = get_room_data(spawn_room_id);
        if spawn_room != () {
            return spawn_room.title;
        }
    }

    // Fall back to default starting room
    let default_spawn_id = get_character_spawn_room(char.name);
    let default_room = get_room_data(default_spawn_id);
    if default_room != () {
        return default_room.title + " (default)";
    }

    return "(unknown)";
}

// Build encumbrance display line with appropriate coloring
fn build_encumbrance_display(char_name, connection_id) {
    let current = get_character_carry_weight(char_name);
    let max = get_player_max_carry_weight(connection_id);
    let percent = get_encumbrance_percent(char_name);
    let level = get_encumbrance_level(char_name);

    let level_display = capitalize_word(level);
    let base_str = "Encumbrance: " + level_display + " (" + current + "/" + max + " lbs - " + percent + "%)";

    // Add penalty warning for medium/heavy/overloaded
    let penalty_str = "";
    if level == "medium" {
        penalty_str = " [Movement -25%, Stamina +25%]";
    }
    if level == "heavy" {
        penalty_str = " [Movement -50%, Stamina +50%]";
    }
    if level == "overloaded" {
        penalty_str = " [Cannot move!]";
    }

    let full_str = base_str + penalty_str;

    // Color based on encumbrance level
    if level == "light" {
        return color_or(ansi_green(full_str), full_str, connection_id);
    }
    if level == "medium" {
        return color_or(ansi_yellow(full_str), full_str, connection_id);
    }
    // heavy or overloaded
    return color_or(ansi_red(full_str), full_str, connection_id);
}

// Build equipment summary showing filled slots
fn build_equipment_summary(char_name, connection_id) {
    let items = get_equipped_items(char_name);

    // Track which key slots are filled
    let has_wielded = false;
    let has_head = false;
    let has_torso = false;
    let has_hands = false;
    let has_legs = false;
    let has_feet = false;

    for item in items {
        for loc in item.wear_locations {
            let l = loc.to_lower();
            if l == "wielded" || l == "offhand" { has_wielded = true; }
            if l == "head" { has_head = true; }
            if l == "torso" { has_torso = true; }
            if l == "hands" { has_hands = true; }
            if l == "legs" { has_legs = true; }
            if l == "feet" { has_feet = true; }
        }
    }

    let output = "Equipment: ";
    output += slot_indicator("W", has_wielded, connection_id) + " ";
    output += slot_indicator("H", has_head, connection_id) + " ";
    output += slot_indicator("T", has_torso, connection_id) + " ";
    output += slot_indicator("G", has_hands, connection_id) + " ";
    output += slot_indicator("L", has_legs, connection_id) + " ";
    output += slot_indicator("F", has_feet, connection_id);

    return output;
}

fn show_basic_status(char, connection_id) {
    let output = "";

    // Header line: Name, Race, Class, Level
    let header = "=== " + char.name;
    if char.race != "" {
        header += " the " + get_race_name(char.race);
    }
    if char.class_name != "" && char.class_name != "unemployed" {
        header += " " + char.class_name;
    }
    let eff_level = get_effective_level(char.name);
    header += " (Level " + eff_level + ") ===";
    output += color_or(ansi_cyan(header), header, connection_id) + "\n";

    // Description
    if char.short_description != "" {
        output += "Description: " + char.short_description + "\n";
    } else {
        output += "Description: (none set - use 'describe <text>' to add one)\n";
    }
    output += "\n";

    // HP bar with color based on percentage
    let hp_pct = 100;
    if char.max_hp > 0 {
        hp_pct = (char.hp * 100) / char.max_hp;
    }
    let hp_bar = build_bar(char.hp, char.max_hp, 10);
    let hp_str = "HP: " + hp_bar + " " + char.hp + "/" + char.max_hp;
    let hp_colored = get_hp_colored(hp_str, hp_pct, connection_id);

    // Stamina bar with color based on percentage
    let stamina_pct = 100;
    if char.max_stamina > 0 {
        stamina_pct = (char.stamina * 100) / char.max_stamina;
    }
    let stamina_bar = build_bar(char.stamina, char.max_stamina, 10);
    let stamina_str = "Stamina: " + stamina_bar + " " + char.stamina + "/" + char.max_stamina;
    let stamina_colored = get_stamina_colored(stamina_str, stamina_pct, connection_id);

    // Thirst bar
    let thirst_bar = build_bar(char.thirst, char.max_thirst, 10);
    let thirst_str = "Thirst: " + thirst_bar + " " + char.thirst + "/" + char.max_thirst;
    let thirst_colored = color_or(ansi_cyan(thirst_str), thirst_str, connection_id);

    // Hunger bar
    let hunger_bar = build_bar(char.hunger, char.max_hunger, 10);
    let hunger_str = "Hunger: " + hunger_bar + " " + char.hunger + "/" + char.max_hunger;
    let hunger_colored = color_or(ansi_cyan(hunger_str), hunger_str, connection_id);

    output += hp_colored + "   " + stamina_colored + "\n";

    // Mana bar (only if mana is enabled for this character)
    if char.mana_enabled {
        let mana_pct = 100;
        if char.max_mana > 0 {
            mana_pct = (char.mana * 100) / char.max_mana;
        }
        let mana_bar = build_bar(char.mana, char.max_mana, 10);
        let mana_str = "Mana: " + mana_bar + " " + char.mana + "/" + char.max_mana;
        let mana_colored = get_mana_colored(mana_str, mana_pct, connection_id);
        output += mana_colored + "\n";
    }

    output += thirst_colored + "   " + hunger_colored + "\n";

    // Position indicator
    let pos_str = "Position: " + char.position;
    output += pos_str + "\n";

    // Conditions warning
    let conditions = [];
    if char.has_hypothermia { conditions.push("Hypothermia"); }
    if char.has_frostbite.len() > 0 { conditions.push("Frostbite"); }
    if char.has_heat_exhaustion { conditions.push("Heat Exhaustion"); }
    if char.has_heat_stroke { conditions.push("Heat Stroke"); }
    if char.has_illness { conditions.push("Illness"); }

    if conditions.len() > 0 {
        let cond_str = "Conditions: " + conditions.join(", ") + " (use 'diagnose' for details)";
        output += color_or(ansi_red(cond_str), cond_str, connection_id) + "\n";
    }

    // Gold
    let gold_str = "Gold: " + char.gold;
    output += color_or(ansi_yellow(gold_str), gold_str, connection_id) + "\n\n";

    // Equipment summary
    output += build_equipment_summary(char.name, connection_id) + "\n";

    // Inventory count
    let inv_items = get_items_in_inventory(char.name);
    output += "Inventory: " + inv_items.len() + " items\n";

    // Encumbrance
    output += build_encumbrance_display(char.name, connection_id) + "\n";

    // Spawn Point
    output += "Spawn Point: " + get_spawn_point_display(char, connection_id) + "\n";

    // Hints
    output += "\nHints: Use 'skills' to view abilities, 'traits' to view/spend trait points.\n";

    send_client_message(connection_id, output);
}

fn format_location(loc) {
    // Capitalize first letter
    if loc.len() == 0 { return ""; }
    let first = loc.sub_string(0, 1);
    let rest = loc.sub_string(1);
    return first.to_upper() + rest;
}

fn format_traits_detailed(traits, connection_id) {
    if traits.len() == 0 {
        return "  (none)";
    }
    let output = "";
    for trait_id in traits {
        let info = get_trait_info(trait_id);
        if info["name"] != () && info["name"] != "" {
            output += "  " + info["name"];
            if info["description"] != () && info["description"] != "" {
                output += " - " + info["description"];
            }
            output += "\n";
        } else {
            output += "  " + trait_id + "\n";
        }
    }
    return output;
}

fn format_equipment_detailed(char_name, connection_id) {
    let items = get_equipped_items(char_name);
    if items.len() == 0 {
        return "  (nothing equipped)";
    }

    let output = "";
    for item in items {
        let locs = item.wear_locations;
        let loc_str = "";
        for loc in locs {
            if loc_str != "" { loc_str += ", "; }
            loc_str += format_location(loc);
        }
        output += "  [" + loc_str + "] " + item.name;
        if item.has_armor_class {
            output += " (AC " + item.armor_class + ")";
        }
        output += "\n";
    }
    return output;
}

fn show_detailed_status(char, connection_id) {
    let output = "";

    // Header
    let header = "=== " + char.name;
    if char.race != "" {
        header += " the " + get_race_name(char.race);
    }
    if char.class_name != "" && char.class_name != "unemployed" {
        header += " " + char.class_name;
    }
    let eff_level = get_effective_level(char.name);
    header += " (Level " + eff_level + ") ===";
    output += color_or(ansi_cyan(header), header, connection_id) + "\n\n";

    // Description
    if char.short_description != "" {
        output += "Description: " + char.short_description + "\n\n";
    } else {
        output += "Description: (none set - use 'describe <text>' to add one)\n\n";
    }

    // Vitals section
    output += color_or(ansi_green("--- Vitals ---"), "--- Vitals ---", connection_id) + "\n";

    let hp_pct = 100;
    if char.max_hp > 0 {
        hp_pct = (char.hp * 100) / char.max_hp;
    }
    let hp_bar = build_bar(char.hp, char.max_hp, 10);
    let hp_str = "HP: " + hp_bar + " " + char.hp + "/" + char.max_hp;
    output += get_hp_colored(hp_str, hp_pct, connection_id) + "\n";

    let stamina_pct = 100;
    if char.max_stamina > 0 {
        stamina_pct = (char.stamina * 100) / char.max_stamina;
    }
    let stamina_bar = build_bar(char.stamina, char.max_stamina, 10);
    let stamina_str = "Stamina: " + stamina_bar + " " + char.stamina + "/" + char.max_stamina;
    output += get_stamina_colored(stamina_str, stamina_pct, connection_id) + "\n";

    // Mana bar (only if mana is enabled for this character)
    if char.mana_enabled {
        let mana_pct = 100;
        if char.max_mana > 0 {
            mana_pct = (char.mana * 100) / char.max_mana;
        }
        let mana_bar = build_bar(char.mana, char.max_mana, 10);
        let mana_str = "Mana: " + mana_bar + " " + char.mana + "/" + char.max_mana;
        output += get_mana_colored(mana_str, mana_pct, connection_id) + "\n";
    }

    let thirst_bar = build_bar(char.thirst, char.max_thirst, 10);
    output += "Thirst: " + thirst_bar + " " + char.thirst + "/" + char.max_thirst + "\n";

    let hunger_bar = build_bar(char.hunger, char.max_hunger, 10);
    output += "Hunger: " + hunger_bar + " " + char.hunger + "/" + char.max_hunger + "\n";

    output += "Position: " + char.position + "\n";

    // Conditions warning
    let conditions = [];
    if char.has_hypothermia { conditions.push("Hypothermia"); }
    if char.has_frostbite.len() > 0 { conditions.push("Frostbite"); }
    if char.has_heat_exhaustion { conditions.push("Heat Exhaustion"); }
    if char.has_heat_stroke { conditions.push("Heat Stroke"); }
    if char.has_illness { conditions.push("Illness"); }

    if conditions.len() > 0 {
        let cond_str = "Conditions: " + conditions.join(", ") + " (use 'diagnose' for details)";
        output += color_or(ansi_red(cond_str), cond_str, connection_id) + "\n";
    }

    let gold_str = "Gold: " + char.gold;
    output += color_or(ansi_yellow(gold_str), gold_str, connection_id) + "\n\n";

    // Character section
    output += color_or(ansi_green("--- Character ---"), "--- Character ---", connection_id) + "\n";
    if char.race != "" {
        output += "Race: " + get_race_name(char.race) + "\n";
    } else {
        output += "Race: (none)\n";
    }
    if char.class_name != "" && char.class_name != "unemployed" {
        output += "Class: " + char.class_name + "\n";
    } else {
        output += "Class: (none)\n";
    }

    // Racial effects section
    output += format_racial_effects(char, connection_id);
    output += "\n";

    // Traits section
    output += color_or(ansi_green("--- Traits ---"), "--- Traits ---", connection_id) + "\n";
    output += format_traits_detailed(char.traits, connection_id);
    if char.traits.len() > 0 {
        output += "\n";
    } else {
        output += "\n";
    }

    // Skills placeholder
    output += color_or(ansi_green("--- Skills ---"), "--- Skills ---", connection_id) + "\n";
    output += "  (Skills not yet implemented)\n\n";

    // Equipment details
    output += color_or(ansi_green("--- Equipment ---"), "--- Equipment ---", connection_id) + "\n";
    output += format_equipment_detailed(char.name, connection_id);
    output += "\n";

    // Inventory & Encumbrance
    output += color_or(ansi_green("--- Inventory ---"), "--- Inventory ---", connection_id) + "\n";
    let inv_items = get_items_in_inventory(char.name);
    output += inv_items.len() + " items carried\n";
    output += build_encumbrance_display(char.name, connection_id) + "\n\n";

    // Spawn Point
    output += color_or(ansi_green("--- Recall ---"), "--- Recall ---", connection_id) + "\n";
    output += "Spawn Point: " + get_spawn_point_display(char, connection_id) + "\n";

    send_client_message(connection_id, output);
}

fn format_racial_effects(char, connection_id) {
    if char.race == "" { return ""; }
    let info = get_race_info(char.race);
    if info.len() == 0 { return ""; }

    let output = "";
    let has_effects = false;

    // Stat modifiers
    let mods = info["stat_modifiers"];
    if mods.len() > 0 {
        let parts = "";
        for k in mods.keys() {
            let v = mods[k];
            if parts != "" { parts += ", "; }
            let stat_upper = k.to_upper();
            if v > 0 { parts += "+" + v + " " + stat_upper; }
            else { parts += v + " " + stat_upper; }
        }
        output += "  Stat Modifiers: " + parts + "\n";
        has_effects = true;
    }

    // Resistances
    let resists = info["resistances"];
    if resists.len() > 0 {
        let parts = "";
        for k in resists.keys() {
            let v = resists[k];
            if parts != "" { parts += ", "; }
            if v > 0 { parts += k + " +" + v + "%"; }
            else { parts += k + " " + v + "%"; }
        }
        output += "  Resistances: " + parts + "\n";
        has_effects = true;
    }

    // Granted traits
    let granted = info["granted_traits"];
    if granted.len() > 0 {
        let parts = "";
        for t in granted {
            if parts != "" { parts += ", "; }
            let tinfo = get_trait_info(t);
            if tinfo["name"] != () && tinfo["name"] != "" { parts += tinfo["name"]; }
            else { parts += t; }
        }
        output += "  Racial Traits: " + parts + "\n";
        has_effects = true;
    }

    // Passives
    let passives = info["passive_abilities"];
    if passives.len() > 0 {
        for p in passives {
            output += "  Passive: " + p["name"] + " - " + p["description"] + "\n";
        }
        has_effects = true;
    }

    // Actives
    let actives = info["active_abilities"];
    if actives.len() > 0 {
        for a in actives {
            output += "  Ability: " + a["name"] + " - " + a["description"];
            output += " (CD: " + a["cooldown_secs"] + "s";
            if a["stamina_cost"] > 0 { output += ", " + a["stamina_cost"] + " stamina"; }
            if a["mana_cost"] > 0 { output += ", " + a["mana_cost"] + " mana"; }
            output += ")\n";
        }
        has_effects = true;
    }

    if has_effects {
        return color_or(ansi_green("--- Racial Effects ---"), "--- Racial Effects ---", connection_id) + "\n" + output;
    }
    return "";
}

// ============================================================
// Mobile Debug Status (Builder/Admin only)
// ============================================================

fn show_mobile_status(mobile, connection_id) {
    let output = "";
    output += mob_status_header(mobile, connection_id);
    output += mob_status_identity(mobile, connection_id);
    output += mob_status_combat_stats(mobile, connection_id);
    output += mob_status_attributes(mobile, connection_id);
    output += mob_status_flags(mobile, connection_id);
    output += mob_status_behavior(mobile, connection_id);
    output += mob_status_state(mobile, connection_id);
    output += mob_status_items(mobile, connection_id);
    send_client_message(connection_id, output);
}

fn mob_section(title, connection_id) {
    let s = "--- " + title + " ---";
    return color_or(ansi_cyan(s), s, connection_id) + "\n";
}

fn mob_status_header(mobile, connection_id) {
    let banner = "======== [MOBILE DEBUG STATUS] ========";
    return color_or(ansi_magenta(banner), banner, connection_id) + "\n\n";
}

fn mob_status_identity(mobile, connection_id) {
    let output = mob_section("Identity", connection_id);
    output += "Name:      " + mobile.name + "\n";
    output += "UUID:      " + mobile.id + "\n";
    output += "VNUM:      " + mobile.vnum + "\n";
    output += "Level:     " + mobile.level + "\n";
    output += "Keywords:  " + mobile.keywords.join(" ") + "\n";

    let room = get_room_data(mobile.current_room_id);
    if room != () {
        output += "Room:      " + room.title + " (" + mobile.current_room_id + ")\n";
    } else {
        output += "Room:      (" + mobile.current_room_id + ")\n";
    }

    if mobile.short_desc != "" {
        output += "Short:     " + mobile.short_desc + "\n";
    }
    if mobile.long_desc != "" {
        let desc = mobile.long_desc;
        if desc.len() > 200 {
            desc = desc.sub_string(0, 200) + "...";
        }
        output += "Long:      " + desc + "\n";
    }
    return output + "\n";
}

fn mob_status_combat_stats(mobile, connection_id) {
    let output = mob_section("Combat Stats", connection_id);

    let hp_pct = 100;
    if mobile.max_hp > 0 {
        hp_pct = (mobile.hp * 100) / mobile.max_hp;
    }
    let hp_bar = build_bar(mobile.hp, mobile.max_hp, 10);
    let hp_str = "HP:        " + hp_bar + " " + mobile.hp + "/" + mobile.max_hp;
    output += get_hp_colored(hp_str, hp_pct, connection_id) + "\n";

    let stam_pct = 100;
    if mobile.max_stamina > 0 {
        stam_pct = (mobile.stamina * 100) / mobile.max_stamina;
    }
    let stam_bar = build_bar(mobile.stamina, mobile.max_stamina, 10);
    let stam_str = "Stamina:   " + stam_bar + " " + mobile.stamina + "/" + mobile.max_stamina;
    output += get_stamina_colored(stam_str, stam_pct, connection_id) + "\n";

    output += "AC:        " + mobile.armor_class + "\n";
    output += "Hit Mod:   " + mobile.hit_modifier + "\n";
    output += "Damage:    " + mobile.damage_dice + " (" + mobile.damage_type + ")\n";

    let gold_str = "Gold:      " + mobile.gold;
    output += color_or(ansi_yellow(gold_str), gold_str, connection_id) + "\n";
    return output + "\n";
}

fn mob_status_attributes(mobile, connection_id) {
    let output = mob_section("Attributes", connection_id);
    output += "STR: " + mobile.stat_str;
    output += "   DEX: " + mobile.stat_dex;
    output += "   CON: " + mobile.stat_con + "\n";
    output += "INT: " + mobile.stat_int;
    output += "   WIS: " + mobile.stat_wis;
    output += "   CHA: " + mobile.stat_cha + "\n";
    output += "Perception: " + mobile.perception + "\n";
    return output + "\n";
}

fn mob_status_flags(mobile, connection_id) {
    let output = mob_section("Flags", connection_id);
    let flags = [];
    let f = mobile.flags;
    if f.aggressive { flags.push("aggressive"); }
    if f.sentinel { flags.push("sentinel"); }
    if f.scavenger { flags.push("scavenger"); }
    if f.shopkeeper { flags.push("shopkeeper"); }
    if f.no_attack { flags.push("no_attack"); }
    if f.healer { flags.push("healer"); }
    if f.leasing_agent { flags.push("leasing_agent"); }
    if f.cowardly { flags.push("cowardly"); }
    if f.can_open_doors { flags.push("can_open_doors"); }
    if f.guard { flags.push("guard"); }
    if f.thief { flags.push("thief"); }
    if f.cant_swim { flags.push("cant_swim"); }

    if flags.len() == 0 {
        output += "  (none)\n";
    } else {
        let flags_str = flags.join(" ");
        output += "  " + color_or(ansi_yellow(flags_str), flags_str, connection_id) + "\n";
    }
    return output + "\n";
}

// Behavior section: routine, dialogue, pursuit, transport
fn mob_status_behavior(mobile, connection_id) {
    let output = "";
    output += mob_status_routine(mobile, connection_id);
    output += mob_status_dialogue(mobile, connection_id);
    output += mob_status_pursuit(mobile, connection_id);
    output += mob_status_simulation(mobile, connection_id);
    output += mob_status_migration(mobile, connection_id);
    output += mob_status_transport(mobile, connection_id);
    return output;
}

fn format_game_hour(hour) {
    if hour < 6 { return hour + ":00 (Night)"; }
    if hour < 8 { return hour + ":00 (Dawn)"; }
    if hour < 12 { return hour + ":00 (Morning)"; }
    if hour < 14 { return hour + ":00 (Midday)"; }
    if hour < 18 { return hour + ":00 (Afternoon)"; }
    if hour < 20 { return hour + ":00 (Dusk)"; }
    return hour + ":00 (Night)";
}

fn mob_status_routine(mobile, connection_id) {
    let output = mob_section("Routine", connection_id);
    let activity = get_mobile_activity(mobile.id);
    output += "Activity:  " + activity + "\n";

    let count = get_mobile_routine_entry_count(mobile.id);
    if count == 0 {
        output += "Schedule:  (none)\n";
        return output + "\n";
    }

    output += "Schedule:  " + count + " entries\n";
    let entries = get_mobile_routine_entries(mobile.id);
    for entry in entries {
        let hour_str = format_game_hour(entry["start_hour"]);
        output += "  " + hour_str + " - " + entry["activity"];
        if entry["destination_vnum"] != "" {
            output += " @ " + entry["destination_vnum"];
        }
        if entry["suppress_wander"] {
            output += " [no wander]";
        }
        output += "\n";
    }
    return output + "\n";
}

fn mob_status_dialogue(mobile, connection_id) {
    let output = mob_section("Dialogue", connection_id);
    let dialogues = get_all_mobile_dialogues(mobile.id);
    let keys = dialogues.keys();
    if keys.len() == 0 {
        output += "  (none)\n";
        return output + "\n";
    }

    for key in keys {
        let response = dialogues[key];
        if response.len() > 60 {
            response = response.sub_string(0, 60) + "...";
        }
        output += "  " + key + " -> \"" + response + "\"\n";
    }
    output += "  (" + keys.len() + " dialogues total)\n";
    return output + "\n";
}

fn mob_status_pursuit(mobile, connection_id) {
    let output = mob_section("Pursuit", connection_id);
    let pursuing = is_mob_pursuing(mobile.id);
    if !pursuing {
        output += "  Not pursuing.\n";
        return output + "\n";
    }

    let warn = "  PURSUING: " + mobile.pursuit_target_name;
    output += color_or(ansi_red(warn), warn, connection_id) + "\n";
    output += "  Direction: " + mobile.pursuit_direction + "\n";
    output += "  Certain:   " + mobile.pursuit_certain + "\n";
    let target_room = mobile.pursuit_target_room;
    if target_room != "" && target_room != () {
        let room = get_room_data(target_room);
        if room != () {
            output += "  Target Room: " + room.title + " (" + target_room + ")\n";
        } else {
            output += "  Target Room: " + target_room + "\n";
        }
    }
    return output + "\n";
}

fn mob_status_simulation(mobile, connection_id) {
    let output = mob_section("Simulation", connection_id);
    if !is_npc_simulated(mobile.id) {
        output += "  Not simulated.\n";
        return output + "\n";
    }

    let config = get_npc_simulation_config(mobile.id);
    if config != () {
        output += "  Home:      " + config["home_room_vnum"] + "\n";
        output += "  Work:      " + config["work_room_vnum"] + "\n";
        output += "  Shop:      " + config["shop_room_vnum"] + "\n";
        output += "  Food VNUM: " + config["preferred_food_vnum"] + "\n";
        output += "  Pay:       " + config["work_pay"] + " gold\n";
        output += "  Hours:     " + config["work_start_hour"] + ":00 - " + config["work_end_hour"] + ":00\n";
        output += "  Decay:     hunger=" + config["hunger_decay_rate"];
        output += " energy=" + config["energy_decay_rate"];
        output += " comfort=" + config["comfort_decay_rate"] + "\n";
        output += "  Low Gold:  " + config["low_gold_threshold"] + "\n";
    }

    let needs = get_npc_needs(mobile.id);
    if needs != () {
        output += "  --- Needs ---\n";
        output += "  Hunger:  " + needs["hunger"] + "/100\n";
        output += "  Energy:  " + needs["energy"] + "/100\n";
        output += "  Comfort: " + needs["comfort"] + "/100\n";
        output += "  Goal:    " + needs["goal"] + "\n";
        output += "  Paid:    " + needs["paid"] + "\n";
    }
    return output + "\n";
}

fn mob_status_migration(mobile, connection_id) {
    let resident = mobile.resident_of;
    let household = mobile.household_id;
    let has_chars = mobile.has_characteristics;
    let rels = mobile.relationships;

    if resident == "" && household == "" && !has_chars && rels.len() == 0 {
        return "";
    }

    let output = mob_section("Migration", connection_id);
    if resident != "" {
        output += "  Resident Of: " + resident + "\n";
    }
    if household != "" {
        output += "  Household:   " + household + "\n";
    }
    if has_chars {
        output += "  Has Characteristics: yes\n";
    }
    if rels.len() > 0 {
        output += "  Relationships: " + rels.len() + "\n";
        for rel in rels {
            output += "    " + rel["kind"] + " -> " + rel["other_id"] + "\n";
        }
    }
    return output + "\n";
}

fn mob_status_transport(mobile, connection_id) {
    if !has_mobile_transport_route(mobile.id) {
        return "";
    }

    let output = mob_section("Transport", connection_id);
    let route = get_mobile_transport_route(mobile.id);
    if route != () {
        output += "  Transport:   " + route["transport_id"] + "\n";
        output += "  Home Stop:   " + route["home_stop"] + "\n";
        output += "  Dest Stop:   " + route["dest_stop"] + "\n";
        output += "  Schedule:    " + route["schedule_type"] + "\n";
        output += "  Depart Hour: " + route["depart_hour"] + "\n";
        output += "  Return Hour: " + route["return_hour"] + "\n";
        output += "  At Dest:     " + route["is_at_destination"] + "\n";
        output += "  On Transport:" + route["is_on_transport"] + "\n";
    }
    return output + "\n";
}

// State section: wounds, combat, ongoing effects
fn mob_status_state(mobile, connection_id) {
    let output = "";
    output += mob_status_wounds(mobile, connection_id);
    output += mob_status_combat(mobile, connection_id);
    return output;
}

fn mob_status_wounds(mobile, connection_id) {
    let output = mob_section("Wounds & Bleeding", connection_id);
    let wounds = get_mobile_wounds(mobile.id);
    if wounds.len() == 0 {
        output += "  No wounds.\n";
    } else {
        for w in wounds {
            let w_str = "  " + w["body_part"] + ": " + w["level"] + " " + w["wound_type"];
            if w["bleeding_severity"] > 0 {
                w_str += " (bleeding: " + w["bleeding_severity"] + ")";
                output += color_or(ansi_red(w_str), w_str, connection_id) + "\n";
            } else {
                output += w_str + "\n";
            }
        }
    }

    let bleeding = get_mobile_total_bleeding(mobile.id);
    if bleeding > 0 {
        let bleed_str = "  Total Bleeding: " + bleeding;
        output += color_or(ansi_red(bleed_str), bleed_str, connection_id) + "\n";
    }

    let scars = get_mobile_scars(mobile.id);
    let scar_keys = scars.keys();
    if scar_keys.len() > 0 {
        let parts = [];
        for k in scar_keys {
            parts.push(k + "(" + scars[k] + ")");
        }
        output += "  Scars: " + parts.join(", ") + "\n";
    }
    return output + "\n";
}

fn mob_status_combat(mobile, connection_id) {
    let output = mob_section("Combat", connection_id);
    let targets = get_mobile_combat_targets(mobile.id);
    if targets.len() == 0 {
        output += "  No active combat.\n";
    } else {
        let combat_str = "  IN COMBAT - " + targets.len() + " target(s)";
        output += color_or(ansi_red(combat_str), combat_str, connection_id) + "\n";
        for t in targets {
            output += "    " + t["target_type"] + ": " + t["target_id"] + "\n";
        }
    }

    let effects = get_mobile_ongoing_effects(mobile.id);
    if effects.len() > 0 {
        output += "  Ongoing Effects:\n";
        for e in effects {
            output += "    " + e + "\n";
        }
    }
    return output + "\n";
}

// Items section: equipment and inventory
fn mob_status_items(mobile, connection_id) {
    let output = mob_section("Inventory & Equipment", connection_id);

    let equipped = get_items_equipped_on_mobile(mobile.id);
    if equipped.len() > 0 {
        output += "  Equipment:\n";
        for item in equipped {
            let locs = item.wear_locations;
            let loc_str = "";
            for loc in locs {
                if loc_str != "" { loc_str += ", "; }
                loc_str += loc;
            }
            output += "    <" + loc_str + "> " + item.name;
            if item.has_armor_class {
                output += " (AC " + item.armor_class + ")";
            }
            output += "\n";
        }
    } else {
        output += "  Equipment: (none)\n";
    }

    let inv = get_items_in_mobile_inventory(mobile.id);
    if inv.len() > 0 {
        output += "  Carrying: " + inv.len() + " items\n";
        for item in inv {
            output += "    " + item.name + "\n";
        }
    } else {
        output += "  Carrying: (nothing)\n";
    }
    return output + "\n";
}
