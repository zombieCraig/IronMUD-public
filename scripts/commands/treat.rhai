// scripts/commands/treat.rhai
// Medical treatment command for treating wounds
// @description Treat wounds using medical supplies
// @category skills

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't treat injuries while in combat!");
        return;
    }

    if args == "" {
        show_usage(connection_id);
        return;
    }

    // Parse arguments: treat [target] [body_part, wound_type, or condition]
    let parts = args.split(" ");
    let target_arg = parts[0].to_lower();
    // Join remaining words to support multi-word body parts like "left arm"
    let second_arg = "";
    if parts.len() > 1 {
        second_arg = join_remaining_args(parts, 1);
    }

    // Find target
    let target_result = find_treatment_target(char, target_arg, connection_id);
    if target_result.error != "" {
        send_client_message(connection_id, target_result.error);
        return;
    }

    let target_name = target_result.name;
    let target_conn = target_result.conn;
    let target_type = target_result.target_type;
    let target_id = target_result.target_id;
    let is_self = target_result.is_self;

    // Check if second argument is a condition name
    if is_condition_name(second_arg) {
        // Only players can have conditions treated (not mobiles)
        if target_type == "mobile" {
            send_client_message(connection_id, "You can only treat conditions on players, not creatures.");
            return;
        }
        treat_condition(char, target_name, target_conn, second_arg, is_self, connection_id);
        return;
    }

    let body_part_arg = second_arg;

    // Get target's wounds based on target type
    let wounds = [];
    if target_type == "mobile" {
        wounds = get_mobile_wounds(target_id);
    } else {
        wounds = get_character_wounds(target_name);
    }
    if wounds.len() == 0 {
        send_no_wounds_message(connection_id, target_name, is_self);
        return;
    }

    // Find the wound to treat
    let wound_result = find_wound_to_treat(wounds, body_part_arg);
    if wound_result.error != "" {
        send_wound_not_found_message(connection_id, target_name, body_part_arg, wounds, is_self);
        return;
    }

    let wound_to_treat = wound_result.wound;

    // Find appropriate medical tool in inventory
    let tool = find_medical_tool_for_wound(char.name, wound_to_treat);
    if tool == () {
        send_client_message(connection_id, "You don't have any medical supplies that can treat a " + wound_to_treat.wound_type + " wound.");
        send_client_message(connection_id, "You need appropriate medical supplies in your inventory.");
        return;
    }

    // Check tool tier vs wound level
    let tool_tier = get_medical_tool_tier(tool.id);
    let required_tier = get_required_tier_for_wound(wound_to_treat.level);
    if tool_tier < required_tier {
        send_client_message(connection_id, "Your " + tool.name + " is not advanced enough to treat a " + wound_to_treat.level + " wound.");
        send_client_message(connection_id, "You need a tier " + required_tier + " medical tool for " + wound_to_treat.level + " wounds.");
        return;
    }

    // Get medical skill level
    let skill_level = get_skill_level(char.name, "medical");

    // Check minimum skill for tool tier
    let min_skill = (tool_tier - 1) * 3;
    if skill_level < min_skill {
        send_client_message(connection_id, "You need medical skill level " + min_skill + " to use your " + tool.name + ".");
        return;
    }

    // Calculate success chance and roll
    let tool_quality = get_tool_quality_bonus(tool.id);
    let success_chance = calculate_treatment_success(skill_level, tool_tier, tool_quality, wound_to_treat.level, is_self);
    let roll = random_int(1, 100);
    let success = (roll <= success_chance);

    // Consume the tool
    consume_medical_tool(tool.id);

    // Process result
    process_treatment_result(success, char, target_name, target_conn, target_type, target_id, wound_to_treat, tool, is_self, connection_id);
}

fn find_treatment_target(char, target_arg, connection_id) {
    let result = #{
        name: "",
        conn: "",
        target_type: "player",
        target_id: "",
        is_self: false,
        error: ""
    };

    // Check for self-treatment
    if target_arg == "self" || target_arg == "me" || target_arg == char.name.to_lower() {
        result.name = char.name;
        result.conn = connection_id;
        result.target_type = "player";
        result.is_self = true;
        return result;
    }

    // Try to find a player
    let target_conn = find_player_connection_by_name(target_arg);
    if target_conn != "" {
        let target_char = get_player_character(target_conn);
        if target_char != () && target_char.current_room_id == char.current_room_id {
            result.name = target_char.name;
            result.conn = target_conn;
            result.target_type = "player";
            result.is_self = (target_char.name.to_lower() == char.name.to_lower());
            return result;
        }
    }

    // Try to find a mobile in the room
    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(target_arg, mobiles);
    if mobile != () {
        result.name = mobile.name;
        result.target_type = "mobile";
        result.target_id = mobile.id;
        result.is_self = false;
        return result;
    }

    result.error = "You don't see '" + target_arg + "' here.";
    return result;
}

fn find_wound_to_treat(wounds, body_part_arg) {
    let result = #{
        wound: (),
        error: ""
    };

    if body_part_arg != "" {
        // Try matching by body part name first
        for w in wounds {
            if w.body_part.to_lower() == body_part_arg {
                result.wound = w;
                return result;
            }
        }
        // Fall back to matching by wound type name
        for w in wounds {
            if w.wound_type.to_lower() == body_part_arg {
                result.wound = w;
                return result;
            }
        }
        result.error = "wound_not_found";
        return result;
    }

    // Find worst wound
    result.wound = wounds[0];
    for i in 1..wounds.len() {
        let w = wounds[i];
        if get_wound_severity_score(w.level) > get_wound_severity_score(result.wound.level) {
            result.wound = w;
        }
    }
    return result;
}

fn send_no_wounds_message(connection_id, target_name, is_self) {
    if is_self {
        send_client_message(connection_id, "You don't have any wounds to treat.");
    } else {
        send_client_message(connection_id, target_name + " doesn't have any wounds to treat.");
    }
}

fn send_wound_not_found_message(connection_id, target_name, body_part, wounds, is_self) {
    if is_self {
        send_client_message(connection_id, "You don't have a wound matching '" + body_part + "'.");
    } else {
        send_client_message(connection_id, target_name + " doesn't have a wound matching '" + body_part + "'.");
    }
    let listing = "Current injuries: ";
    for i in 0..wounds.len() {
        if i > 0 { listing += ", "; }
        listing += wounds[i].level + " " + wounds[i].wound_type + " (" + wounds[i].body_part + ")";
    }
    send_client_message(connection_id, listing);
}

fn process_treatment_result(success, char, target_name, target_conn, target_type, target_id, wound, tool, is_self, connection_id) {
    let body_part_display = wound.body_part;
    let wound_type_display = wound.wound_type;

    if success {
        process_successful_treatment(char, target_name, target_conn, target_type, target_id, wound, tool, body_part_display, wound_type_display, is_self, connection_id);
    } else {
        process_failed_treatment(char, target_name, target_conn, target_type, tool, body_part_display, wound_type_display, is_self, connection_id);
    }
}

fn process_successful_treatment(char, target_name, target_conn, target_type, target_id, wound, tool, body_part_display, wound_type_display, is_self, connection_id) {
    // Heal the wound based on target type
    if target_type == "mobile" {
        heal_mobile_wound(target_id, wound.body_part);
        // Add scar to mobile
        add_mobile_scar(target_id, wound.body_part);
    } else {
        heal_character_wound_by_part(target_conn, wound.body_part);
        // Add scar to player
        add_scar(target_name, wound.body_part);

        // Clear frostbite condition if treating a frostbite wound
        if wound.wound_type.to_lower() == "frostbite" {
            clear_frostbite_condition(target_conn, wound.body_part);
        }
    }

    let base_xp = 20;
    let difficulty_bonus = get_wound_severity_score(wound.level) * 5;
    let xp_gained = base_xp + difficulty_bonus;

    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_success_messages(char, target_name, target_conn, target_type, tool, body_part_display, wound_type_display, xp_gained, is_self, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_success_messages(char, target_name, target_conn, target_type, tool, body_part, wound_type, xp_gained, is_self, connection_id) {
    if is_self {
        send_client_message(connection_id, "You carefully treat your " + wound_type + " on your " + body_part + " with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment is successful! Your wound has been healed. (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " treats their own wounds.", char.name);
        return;
    }

    send_client_message(connection_id, "You carefully treat " + target_name + "'s " + wound_type + " on their " + body_part + " with your " + tool.name + ".");
    send_client_message(connection_id, "The treatment is successful! The wound has been healed. (+" + xp_gained + " Medical XP)");

    // Only send messages to target if it's a player
    if target_type == "player" {
        send_client_message(target_conn, char.name + " carefully treats your " + wound_type + " on your " + body_part + ".");
        send_client_message(target_conn, "The treatment is successful! Your wound has been healed.");
    }

    broadcast_to_room(char.current_room_id, char.name + " treats " + target_name + "'s wounds.", char.name);
}

fn process_failed_treatment(char, target_name, target_conn, target_type, tool, body_part_display, wound_type_display, is_self, connection_id) {
    let xp_gained = 5;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_failure_messages(char, target_name, target_conn, target_type, tool, body_part_display, wound_type_display, xp_gained, is_self, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_failure_messages(char, target_name, target_conn, target_type, tool, body_part, wound_type, xp_gained, is_self, connection_id) {
    if is_self {
        send_client_message(connection_id, "You attempt to treat your " + wound_type + " on your " + body_part + " with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment was unsuccessful. Your supplies were used but the wound remains. (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " struggles to treat their own wounds.", char.name);
        return;
    }

    send_client_message(connection_id, "You attempt to treat " + target_name + "'s " + wound_type + " on their " + body_part + " with your " + tool.name + ".");
    send_client_message(connection_id, "The treatment was unsuccessful. Your supplies were used but the wound remains. (+" + xp_gained + " Medical XP)");

    // Only send messages to target if it's a player
    if target_type == "player" {
        send_client_message(target_conn, char.name + " attempts to treat your " + wound_type + " on your " + body_part + ".");
        send_client_message(target_conn, "Unfortunately, the treatment was not successful.");
    }

    broadcast_to_room(char.current_room_id, char.name + " struggles to treat " + target_name + "'s wounds.", char.name);
}

fn show_usage(connection_id) {
    send_client_message(connection_id, "Usage: treat <target> [body part, wound type, or condition]");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Wound Examples:");
    send_client_message(connection_id, "  treat self           - Treat your worst wound");
    send_client_message(connection_id, "  treat self left arm  - Treat a wound on your left arm");
    send_client_message(connection_id, "  treat self fracture  - Treat a fracture wound");
    send_client_message(connection_id, "  treat Bob            - Treat another player's worst wound");
    send_client_message(connection_id, "  treat Bob right leg  - Treat Bob's right leg wound");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Condition Examples:");
    send_client_message(connection_id, "  treat self illness      - Treat an illness");
    send_client_message(connection_id, "  treat self hypothermia  - Treat hypothermia");
    send_client_message(connection_id, "  treat Bob heat_stroke   - Treat heat stroke");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Conditions: illness, hypothermia, heat_exhaustion, heat_stroke");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Requires appropriate medical supplies in your inventory.");
    send_client_message(connection_id, "Use 'diagnose <target>' to see wounds/conditions and required supplies.");
}

fn join_remaining_args(parts, start) {
    let result = "";
    for i in start..parts.len() {
        if result != "" { result += " "; }
        result += parts[i].to_lower();
    }
    return result;
}

fn get_wound_severity_score(level) {
    if level == "minor" { return 1; }
    if level == "moderate" { return 2; }
    if level == "severe" { return 3; }
    if level == "critical" { return 4; }
    if level == "disabled" { return 5; }
    return 0;
}

fn get_required_tier_for_wound(level) {
    if level == "minor" { return 1; }
    if level == "moderate" { return 2; }
    if level == "severe" { return 3; }
    if level == "critical" { return 3; }
    if level == "disabled" { return 3; }
    return 1;
}

fn find_medical_tool_for_wound(char_name, wound) {
    let items = get_items_in_inventory(char_name);
    let wound_type = wound.wound_type.to_lower();

    for item in items {
        if is_medical_tool(item.id) {
            if can_tool_treat_wound_type(item.id, wound_type) {
                return item;
            }
        }
    }

    return ();
}

// ========== Condition Treatment Functions ==========

fn is_condition_name(name) {
    if name == "illness" { return true; }
    if name == "hypothermia" { return true; }
    if name == "heat_exhaustion" { return true; }
    if name == "heat_stroke" { return true; }
    return false;
}

fn treat_condition(char, target_name, target_conn, condition, is_self, connection_id) {
    // Check if target has the condition
    if !has_condition(target_conn, condition) {
        if is_self {
            send_client_message(connection_id, "You don't have " + get_condition_display(condition) + ".");
        } else {
            send_client_message(connection_id, target_name + " doesn't have " + get_condition_display(condition) + ".");
        }
        return;
    }

    // Find medical tool that can treat this condition
    let tool = find_medical_tool_for_condition(char.name, condition);
    if tool == () {
        send_client_message(connection_id, "You don't have any medical supplies that can treat " + get_condition_display(condition) + ".");
        return;
    }

    // Get medical skill level
    let skill_level = get_skill_level(char.name, "medical");
    let tool_tier = get_medical_tool_tier(tool.id);

    // Check minimum skill for tool tier
    let min_skill = (tool_tier - 1) * 3;
    if skill_level < min_skill {
        send_client_message(connection_id, "You need medical skill level " + min_skill + " to use your " + tool.name + ".");
        return;
    }

    // Conditions are easier to treat than wounds - higher base success
    let tool_quality = get_tool_quality_bonus(tool.id);
    let base_chance = 60 + (skill_level * 3) + (tool_tier * 5) + tool_quality;
    if !is_self {
        base_chance += 10;  // Easier to treat others
    }
    let success_chance = base_chance;
    if success_chance > 95 { success_chance = 95; }

    let roll = random_int(1, 100);
    let success = (roll <= success_chance);

    // Consume the tool
    consume_medical_tool(tool.id);

    // Process result
    if success {
        process_condition_success(char, target_name, target_conn, condition, tool, is_self, connection_id);
    } else {
        process_condition_failure(char, target_name, target_conn, condition, tool, is_self, connection_id);
    }
}

fn find_medical_tool_for_condition(char_name, condition) {
    let items = get_items_in_inventory(char_name);

    for item in items {
        if is_medical_tool(item.id) {
            if can_tool_treat_wound_type(item.id, condition) {
                return item;
            }
        }
    }

    return ();
}

fn get_condition_display(condition) {
    if condition == "illness" { return "an illness"; }
    if condition == "hypothermia" { return "hypothermia"; }
    if condition == "heat_exhaustion" { return "heat exhaustion"; }
    if condition == "heat_stroke" { return "heat stroke"; }
    return condition;
}

fn process_condition_success(char, target_name, target_conn, condition, tool, is_self, connection_id) {
    // Apply the cure
    if condition == "illness" {
        cure_character_illness(target_conn);
    }
    if condition == "hypothermia" {
        cure_hypothermia(target_conn);
    }
    if condition == "heat_exhaustion" {
        cure_heat_exhaustion(target_conn);
    }
    if condition == "heat_stroke" {
        cure_heat_stroke(target_conn);
    }

    // Award XP
    let xp_gained = 25;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    // Refresh character data
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Send messages
    let condition_display = get_condition_display(condition);
    if is_self {
        send_client_message(connection_id, "You treat yourself with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment is successful! Your " + condition_display + " has been cured. (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " treats themselves with medical supplies.", char.name);
    } else {
        send_client_message(connection_id, "You treat " + target_name + " with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment is successful! Their " + condition_display + " has been cured. (+" + xp_gained + " Medical XP)");
        send_client_message(target_conn, char.name + " treats you with medical supplies.");
        send_client_message(target_conn, "The treatment is successful! Your " + condition_display + " has been cured.");
        broadcast_to_room(char.current_room_id, char.name + " treats " + target_name + " with medical supplies.", char.name);
    }

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn process_condition_failure(char, target_name, target_conn, condition, tool, is_self, connection_id) {
    // Award reduced XP for attempt
    let xp_gained = 5;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    // Refresh character data
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Send messages
    let condition_display = get_condition_display(condition);
    if is_self {
        send_client_message(connection_id, "You attempt to treat yourself with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment was unsuccessful. Your supplies were used but the condition remains. (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " struggles to treat themselves.", char.name);
    } else {
        send_client_message(connection_id, "You attempt to treat " + target_name + " with your " + tool.name + ".");
        send_client_message(connection_id, "The treatment was unsuccessful. Your supplies were used but the condition remains. (+" + xp_gained + " Medical XP)");
        send_client_message(target_conn, char.name + " attempts to treat you with medical supplies.");
        send_client_message(target_conn, "Unfortunately, the treatment was not successful.");
        broadcast_to_room(char.current_room_id, char.name + " struggles to treat " + target_name + ".", char.name);
    }

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}
