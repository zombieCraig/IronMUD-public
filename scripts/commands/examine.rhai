// scripts/commands/examine.rhai
// examine <item> - Look at an item in detail

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Examine what?");
        return;
    }

    // Check inventory first
    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);

    // Then equipment
    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(args, eq_items);
    }

    // Then room
    if item == () {
        let room_items = get_items_in_room(char.current_room_id);
        item = find_item_by_keyword(args, room_items);
    }

    // Then mobiles in room (filter invisible ones the examiner can't see)
    if item == () {
        let mobiles = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
        let mobile = find_mobile_by_keyword(args, mobiles);
        if mobile != () {
            examine_show_mobile(mobile, connection_id);
            return;
        }
    }

    // Then plants in room
    if item == () {
        let room_plants = get_plants_in_room(char.current_room_id);
        let plant_keyword = args.to_lower();
        for p in room_plants {
            let proto = get_plant_prototype_by_vnum(p.prototype_vnum);
            if proto == () { continue; }
            let name_lower = proto.name.to_lower();
            let matched = false;
            if name_lower.contains(plant_keyword) { matched = true; }
            if !matched {
                for kw in proto.keywords {
                    if kw.to_lower().contains(plant_keyword) {
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                // Display plant info
                let examine_desc = get_plant_examine_description(p.id);
                let output = proto.name + "\n";
                output += "--------------------\n";
                if examine_desc != "" {
                    output += examine_desc + "\n";
                }
                output += "\nStage: " + p.stage;
                output += "\nWater: " + p.water_level + "%";
                output += "\nHealth: " + p.health + "%";
                if p.fertilized {
                    output += "\nFertilized: Yes (" + p.fertilizer_hours_remaining + " hours remaining)";
                }
                output += build_infestation_label(p);
                if p.is_potted {
                    output += "\nPlanted in: a pot";
                } else {
                    output += "\nPlanted in: the ground";
                }
                output += "\nPlanted by: " + p.planter_name;
                if p.times_harvested > 0 {
                    output += "\nHarvested: " + p.times_harvested + " time(s)";
                }
                send_client_message(connection_id, output);
                return;
            }
        }
    }

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    // Fire on_examine trigger (runs after item found, doesn't cancel)
    fire_item_trigger(item.id, "on_examine", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id
    });

    let output = item.name + "\n";
    output += "--------------------\n";
    output += item.long_desc + "\n";

    if get_item_note_content(item.id) != "" {
        output += "(You could read this.)\n";
    }

    // Item type
    output += "\nType: " + item.item_type;

    if item.has_armor_class {
        output += "\nArmor Class: " + item.armor_class;
        // Show armor condition if it has any holes
        let condition = get_armor_condition(item.id);
        let holes = get_item_holes(item.id);
        if holes > 0 {
            output += " [" + condition.to_upper() + " - " + holes + " hole";
            if holes > 1 { output += "s"; }
            output += "]";
        }
        // Show body parts protected
        let protects = get_item_protects(item.id);
        if protects.len() > 0 {
            output += "\nProtects: " + protects.join(", ");
        }
    }

    if item.has_damage {
        let wskill = get_item_weapon_skill(item.id);
        output += "\nDamage: " + item.damage_dice_count + "d" + item.damage_dice_sides;
        if wskill == "ranged" {
            output += " (ranged)";
        } else {
            output += " (" + item.damage_type + ")";
        }
        if item.two_handed {
            output += " [Two-Handed]";
        }
        if wskill != () && wskill != "" {
            output += "\nWeapon Skill: " + wskill;
        }
    }

    let wear_locs = item.wear_locations;
    if wear_locs.len() > 0 {
        output += "\nWorn on: ";
        let first = true;
        for loc in wear_locs {
            if !first { output += ", "; }
            output += loc;
            first = false;
        }
    }

    if item.weight > 0 {
        output += "\nWeight: " + item.weight;
    }

    if item.value > 0 {
        output += "\nValue: " + item.value + " gold";
    }

    // Flags
    let flag_str = "";
    if item.flags.glow { flag_str += "glowing "; }
    if item.flags.hum { flag_str += "humming "; }
    if item.flags.invisible { flag_str += "invisible "; }
    if item.flags.waterproof { flag_str += "waterproof "; }
    if item.flags.provides_warmth { flag_str += "radiating warmth "; }
    if item.flags.medical_tool { flag_str += "medical tool "; }
    if item.flags.magical && (has_buff(char.name, "detect_magic") || char.is_admin) {
        flag_str += "magical ";
    }
    if flag_str != "" {
        output += "\nProperties: " + flag_str;
    }

    // Stat bonuses
    if item.has_stats {
        output += "\n" + build_stat_bonuses(item);
    }

    // Level requirement
    if item.level_requirement > 0 {
        output += "\nLevel Required: " + item.level_requirement;
    }

    // Insulation
    if item.insulation > 0 {
        output += "\nInsulation: " + item.insulation;
    }

    // Container details
    if item.is_container {
        output += build_container_details(item);
    }

    // Liquid container details
    if item.is_liquid_container {
        output += build_liquid_details(item);
    }

    // Food details
    if item.is_food {
        output += build_food_details(item);
    }

    // Fishing rod details
    if item.is_fishing_rod {
        output += build_fishing_rod_details(item);
    }

    // Bait details
    if item.is_bait {
        output += build_bait_details(item);
    }

    // Foraging tool details
    if item.is_foraging_tool {
        output += build_foraging_tool_details(item);
    }

    // Medical tool details
    if item.is_medical_tool {
        output += build_medical_tool_details(item);
    }

    // Ranged weapon magazine info
    if item.has_ranged_type {
        output += build_magazine_display(item);
    }

    // Weapon attachments display
    if item.has_ranged_type {
        output += build_weapon_attachments_display(item);
    }

    // Plant pot details
    if item.is_plant_pot {
        output += build_potted_plant_section(item, char.current_room_id);
    }

    send_client_message(connection_id, output);
}

fn build_magazine_display(item) {
    let rtype = get_item_ranged_type(item.id);
    if rtype != "crossbow" && rtype != "firearm" { return ""; }
    let loaded = get_item_loaded_ammo(item.id);
    let mag_size = get_item_magazine_size(item.id);
    if mag_size <= 0 { return ""; }
    return "\nAmmo: " + loaded + "/" + mag_size + " loaded";
}

fn build_weapon_attachments_display(item) {
    let contents = item.container_contents;
    let attachments = [];
    for content_id in contents {
        let att = get_item_data(content_id);
        if att == () { continue; }
        if att.attachment_slot == "" { continue; }
        attachments.push(att);
    }
    if attachments.len() == 0 { return ""; }
    let output = "\n\n--- Attachments ---";
    for att in attachments {
        output += "\n  " + att.attachment_slot + ": " + att.name;
    }
    return output;
}

fn build_container_details(item) {
    let output = "\n\n--- Container ---";
    output += build_container_status(item);
    output += build_container_contents(item);
    return output;
}

fn build_container_status(item) {
    let output = "";
    if item.container_closed {
        if item.container_locked {
            output += "\nStatus: Closed and locked";
        } else {
            output += "\nStatus: Closed";
        }
    } else {
        output += "\nStatus: Open";
    }
    return output;
}

fn build_container_contents(item) {
    let contents = get_items_in_container(item.id);
    if contents.len() == 0 {
        return "\nContents: Empty";
    }
    let output = "\nContents:";
    for i in contents {
        output += "\n  " + i.name;
    }
    return output;
}

fn build_liquid_details(item) {
    let output = "\n\n--- Liquid Container ---";
    output += "\nContains: " + item.liquid_type;
    output += " (" + item.liquid_current + "/" + item.liquid_max + " sips)";
    if item.liquid_poisoned {
        output += "\n[POISONED!]";
    }
    return output;
}

fn build_food_details(item) {
    let output = "\n\n--- Food ---";
    output += "\nNutrition: " + item.food_nutrition;
    output += build_food_freshness(item);
    if item.food_poisoned {
        output += "\n[POISONED!]";
    }
    return output;
}

fn build_food_freshness(item) {
    let label = item.freshness_label;
    if label == "spoiled" {
        return "\nFreshness: SPOILED!";
    }
    if label == "nearly spoiled" {
        return "\nFreshness: Nearly Spoiled";
    }
    if label == "stale" {
        return "\nFreshness: Stale";
    }
    if label == "slightly aged" {
        return "\nFreshness: Slightly Aged";
    }
    if label == "preserved" {
        return "\nFreshness: Preserved";
    }
    return "\nFreshness: Fresh";
}

fn build_fishing_rod_details(item) {
    let output = "\n\n--- Fishing Rod ---";
    output += "\nQuality: " + item.quality + "/100";
    if item.quality > 0 {
        output += " (+" + item.quality + "% catch bonus)";
    }
    output += "\n\nTip: Use 'hold " + item.keywords[0] + "' to equip, then 'fish' near water.";
    output += "\nBait in your inventory will automatically be used when fishing.";
    return output;
}

fn build_bait_details(item) {
    let output = "\n\n--- Bait ---";
    output += "\nQuality: " + item.quality + "/100";
    if item.quality > 0 {
        output += " (+" + item.quality + "% bite bonus)";
    }
    if item.bait_uses == 0 {
        output += "\nUses: Infinite";
    } else {
        output += "\nUses: " + item.bait_uses + " remaining";
    }
    output += "\n\nTip: Keep bait in your inventory while fishing.";
    return output;
}

fn build_foraging_tool_details(item) {
    let output = "\n\n--- Foraging Tool ---";
    output += "\nQuality: " + item.quality + "/100";
    if item.quality > 0 {
        output += " (+" + item.quality / 2 + "% find bonus)";
    }
    output += "\n\nTip: Equip this tool, then use 'forage' in wilderness or city areas.";
    return output;
}

fn build_medical_tool_details(item) {
    let output = "\n\n--- Medical Tool ---";

    // Tier display
    let tier_name = "Unknown";
    if item.medical_tier == 1 { tier_name = "Basic"; }
    if item.medical_tier == 2 { tier_name = "Intermediate"; }
    if item.medical_tier == 3 { tier_name = "Advanced"; }
    output += "\nTier: " + tier_name;

    // Uses display
    if item.medical_uses == 0 {
        output += "\nUses: Reusable";
    } else {
        output += "\nUses: " + item.medical_uses + " remaining";
    }

    // Wound types
    let wound_types = item.treats_wound_types;
    if wound_types.len() > 0 {
        output += "\nTreats: " + wound_types.join(", ");
    }

    // Max severity
    if item.max_treatable_wound != "" {
        output += "\nMax Severity: " + item.max_treatable_wound;
    }

    output += "\n\nTip: Use 'treat <target>' to apply medical treatment.";
    return output;
}

fn build_infestation_label(plant) {
    if plant.infestation == "none" { return ""; }
    let severity = "mild";
    if plant.infestation_severity >= 0.7 { severity = "severe"; }
    else if plant.infestation_severity >= 0.4 { severity = "moderate"; }
    return "\nInfestation: " + severity + " " + plant.infestation;
}

fn build_plant_details(plant) {
    let proto = get_plant_prototype_by_vnum(plant.prototype_vnum);
    if proto == () { return "\n\nA plant is growing here, but you can't identify it."; }
    let output = "\n\n--- Potted Plant: " + proto.name + " ---";
    let examine_desc = get_plant_examine_description(plant.id);
    if examine_desc != "" {
        output += "\n" + examine_desc;
    }
    output += "\nStage: " + plant.stage;
    output += "\nWater: " + plant.water_level + "%";
    output += "\nHealth: " + plant.health + "%";
    if plant.fertilized {
        output += "\nFertilized: Yes (" + plant.fertilizer_hours_remaining + " hours remaining)";
    }
    output += build_infestation_label(plant);
    output += "\nPlanted by: " + plant.planter_name;
    if plant.times_harvested > 0 {
        output += "\nHarvested: " + plant.times_harvested + " time(s)";
    }
    return output;
}

fn build_potted_plant_section(item, room_id) {
    let plants = get_plants_in_room(room_id);
    for p in plants {
        if p.pot_item_id == item.id {
            return build_plant_details(p);
        }
    }
    return "\n\nNothing is planted in this pot.";
}

fn build_stat_bonuses(item) {
    let parts = [];
    if item.stat_str != 0 { parts.push(format_stat_bonus("STR", item.stat_str)); }
    if item.stat_dex != 0 { parts.push(format_stat_bonus("DEX", item.stat_dex)); }
    if item.stat_con != 0 { parts.push(format_stat_bonus("CON", item.stat_con)); }
    if item.stat_int != 0 { parts.push(format_stat_bonus("INT", item.stat_int)); }
    if item.stat_wis != 0 { parts.push(format_stat_bonus("WIS", item.stat_wis)); }
    if item.stat_cha != 0 { parts.push(format_stat_bonus("CHA", item.stat_cha)); }
    return "Stat Bonuses: " + parts.join(", ");
}

fn format_stat_bonus(label, value) {
    if value > 0 {
        return "+" + value + " " + label;
    }
    return "" + value + " " + label;
}

fn examine_show_mobile(mobile, connection_id) {
    let output = mobile.name + "\n";
    output += "--------------------\n";
    let desc = mobile.long_desc;
    if desc == "" {
        desc = "You see nothing special about " + mobile.name + ".";
    }
    output += desc;

    output += examine_mobile_health(mobile);
    output += examine_mobile_wounds(mobile);
    output += examine_mobile_scars(mobile);
    output += examine_mobile_equipment(mobile);

    if mobile_has_buff(mobile.id, "damage_reduction") {
        output += "\n\n" + mobile.name + " is surrounded by a faint white aura.";
    }
    if mobile_has_buff(mobile.id, "sleep") {
        output += "\n" + mobile.name + " is fast asleep.";
    }
    if mobile_has_buff(mobile.id, "blind") {
        output += "\n" + mobile.name + "'s eyes are clouded and unseeing.";
    }
    if mobile_has_buff(mobile.id, "curse") {
        output += "\n" + mobile.name + " is wreathed in a faint shadow of ill fortune.";
    }

    let charm_master = mobile_charm_master(mobile.id);
    if charm_master != "" {
        let viewer = get_player_character(connection_id);
        let is_master = type_of(viewer) != "()" && viewer.name.to_lower() == charm_master.to_lower();
        if is_master {
            let stay = get_charm_stay(mobile.id);
            let follow_other = get_charm_follow_player(mobile.id);
            if stay {
                output += "\n" + mobile.name + " is charmed by you and waits where you left it.";
            } else if follow_other != "" {
                output += "\n" + mobile.name + " is charmed by you and following " + follow_other + ".";
            } else {
                output += "\n" + mobile.name + " is charmed by you and follows your will.";
            }
        } else {
            output += "\n" + mobile.name + " seems strangely compliant.";
        }
    }

    // Visual cues for simulated NPCs
    if is_npc_simulated(mobile.id) {
        let cues = get_npc_visual_cues(mobile.id);
        if cues != "" {
            output += "\n" + cues;
        }
        let social_cues = get_npc_social_cues(mobile.id);
        if social_cues != "" {
            output += "\n" + social_cues;
        }
    }

    send_client_message(connection_id, output);
}

fn examine_mobile_health(mobile) {
    if mobile.max_hp <= 0 { return ""; }
    let percent = (mobile.hp * 100) / mobile.max_hp;
    if percent >= 100 { return "\n\n" + mobile.name + " is in perfect health."; }
    if percent >= 75 { return "\n\n" + mobile.name + " has a few scratches."; }
    if percent >= 50 { return "\n\n" + mobile.name + " has some wounds."; }
    if percent >= 25 { return "\n\n" + mobile.name + " is badly wounded."; }
    if percent > 0 { return "\n\n" + mobile.name + " is barely holding on."; }
    return "\n\n" + mobile.name + " appears to be dead.";
}

fn examine_mobile_wounds(mobile) {
    let wounds = get_mobile_wounds(mobile.id);
    if wounds.len() == 0 { return ""; }
    let texts = [];
    for wound in wounds {
        if wound.level == "bruise" || wound.level == "light" { continue; }
        let text = "a " + wound.level + " " + wound.wound_type + " on the " + wound.body_part;
        if wound.bleeding_severity > 0 { text += " (bleeding)"; }
        texts.push(text);
    }
    if texts.len() == 0 { return ""; }
    return "\n" + examine_join_wound_texts(texts, mobile.name);
}

fn examine_join_wound_texts(texts, name) {
    let result = name + " has " + texts[0];
    for i in 1..texts.len() {
        if i == texts.len() - 1 { result += " and "; }
        else { result += ", "; }
        result += texts[i];
    }
    return result + ".";
}

fn examine_mobile_scars(mobile) {
    let scars_map = get_mobile_scars(mobile.id);
    let keys = scars_map.keys();
    if keys.len() == 0 { return ""; }
    let texts = [];
    for key in keys {
        let count = scars_map[key];
        if count <= 0 { continue; }
        let desc = examine_scar_desc(count);
        texts.push(desc + " on their " + key);
    }
    if texts.len() == 0 { return ""; }
    return "\n" + examine_join_wound_texts(texts, mobile.name);
}

fn examine_scar_desc(count) {
    if count == 1 { return "a scar"; }
    if count == 2 { return "a couple of scars"; }
    if count == 3 { return "a few scars"; }
    if count <= 5 { return "several scars"; }
    return "many scars";
}

fn examine_mobile_equipment(mobile) {
    let all_items = get_items_equipped_on_mobile(mobile.id);
    if all_items.len() == 0 { return ""; }
    let visible = [];
    for item in all_items {
        if item.flags.invisible { continue; }
        if get_item_flag(item.id, "death_only") { continue; }
        visible.push(item);
    }
    if visible.len() == 0 { return ""; }
    let output = "\n\n" + mobile.name + " is using:";
    for item in visible {
        let locs = item.wear_locations;
        let loc_str = "";
        if locs.len() > 0 { loc_str = locs[0]; }
        output += "\n  <" + loc_str + "> " + item.name;
    }
    return output;
}
