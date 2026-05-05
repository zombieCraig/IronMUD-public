// scripts/commands/stabilize.rhai
// Emergency stabilization command to stop bleeding
// @description Emergency stabilization to stop bleeding
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
        send_client_message(connection_id, "You can't stabilize injuries while in combat!");
        return;
    }

    if args == "" {
        show_usage(connection_id);
        return;
    }

    let target_arg = args.to_lower();

    // Find target
    let target_result = find_stabilize_target(char, target_arg, connection_id);
    if target_result.error != "" {
        send_client_message(connection_id, target_result.error);
        return;
    }

    let target_name = target_result.name;
    let target_conn = target_result.conn;
    let target_type = target_result.target_type;
    let target_id = target_result.target_id;
    let is_self = target_result.is_self;

    // Check for bleeding based on target type
    let total_bleeding = 0;
    if target_type == "mobile" {
        total_bleeding = get_mobile_total_bleeding(target_id);
    } else {
        total_bleeding = get_character_total_bleeding(target_name);
    }
    if total_bleeding == 0 {
        send_no_bleeding_message(connection_id, target_name, is_self);
        return;
    }

    // Find bandage or medical tool in inventory
    let tool = find_stabilization_tool(char.name);
    if tool == () {
        send_client_message(connection_id, "You don't have any bandages or medical supplies to stabilize bleeding.");
        return;
    }

    // Get medical skill level
    let skill_level = get_skill_level(char.name, "medical");

    // Calculate success chance - stabilization is easier than full treatment
    let base_chance = 50 + (skill_level * 5);
    let self_penalty = 0;
    if is_self {
        self_penalty = -15;
    }
    let success_chance = base_chance + self_penalty;
    if success_chance > 95 { success_chance = 95; }
    if success_chance < 20 { success_chance = 20; }

    let roll = random_int(1, 100);
    let success = (roll <= success_chance);

    // Consume the tool
    consume_medical_tool(tool.id);

    // Process result
    process_stabilize_result(success, char, target_name, target_conn, target_type, target_id, tool, total_bleeding, is_self, connection_id);
}

fn find_stabilize_target(char, target_arg, connection_id) {
    let result = #{
        name: "",
        conn: "",
        target_type: "player",
        target_id: "",
        is_self: false,
        error: ""
    };

    // Check for self-stabilization
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

    // Try to find a mobile in the room (filter invisible ones the medic can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
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

fn find_stabilization_tool(char_name) {
    let items = get_items_in_inventory(char_name);

    for item in items {
        if is_medical_tool(item.id) {
            // Any bandage-type tool works for stabilization
            if can_tool_treat_wound_type(item.id, "cut") {
                return item;
            }
            if can_tool_treat_wound_type(item.id, "puncture") {
                return item;
            }
        }
    }

    return ();
}

fn send_no_bleeding_message(connection_id, target_name, is_self) {
    if is_self {
        send_client_message(connection_id, "You aren't bleeding.");
    } else {
        send_client_message(connection_id, target_name + " isn't bleeding.");
    }
}

fn process_stabilize_result(success, char, target_name, target_conn, target_type, target_id, tool, bleeding_amount, is_self, connection_id) {
    if success {
        process_stabilize_success(char, target_name, target_conn, target_type, target_id, tool, bleeding_amount, is_self, connection_id);
    } else {
        process_stabilize_failure(char, target_name, target_conn, target_type, tool, is_self, connection_id);
    }
}

fn process_stabilize_success(char, target_name, target_conn, target_type, target_id, tool, bleeding_amount, is_self, connection_id) {
    // Stop all bleeding based on target type
    if target_type == "mobile" {
        stop_mobile_bleeding(target_id);
    } else {
        stop_character_bleeding(target_conn);
    }

    let base_xp = 15;
    let severity_bonus = bleeding_amount * 3;
    let xp_gained = base_xp + severity_bonus;

    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_stabilize_success_messages(char, target_name, target_conn, target_type, tool, xp_gained, is_self, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_stabilize_success_messages(char, target_name, target_conn, target_type, tool, xp_gained, is_self, connection_id) {
    if is_self {
        send_client_message(connection_id, "You quickly apply pressure and bandage your wounds with your " + tool.name + ".");
        send_client_message(connection_id, "The bleeding has stopped! (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " hastily bandages their wounds.", char.name);
        return;
    }

    send_client_message(connection_id, "You quickly apply pressure and bandage " + target_name + "'s wounds with your " + tool.name + ".");
    send_client_message(connection_id, "The bleeding has stopped! (+" + xp_gained + " Medical XP)");

    // Only send messages to target if it's a player
    if target_type == "player" {
        send_client_message(target_conn, char.name + " quickly bandages your wounds.");
        send_client_message(target_conn, "The bleeding has stopped!");
    }

    broadcast_to_room(char.current_room_id, char.name + " hastily bandages " + target_name + "'s wounds.", char.name);
}

fn process_stabilize_failure(char, target_name, target_conn, target_type, tool, is_self, connection_id) {
    let xp_gained = 3;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_stabilize_failure_messages(char, target_name, target_conn, target_type, tool, xp_gained, is_self, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_stabilize_failure_messages(char, target_name, target_conn, target_type, tool, xp_gained, is_self, connection_id) {
    if is_self {
        send_client_message(connection_id, "You try to bandage your wounds with your " + tool.name + ", but the bandage slips.");
        send_client_message(connection_id, "You couldn't stop the bleeding. (+" + xp_gained + " Medical XP)");
        broadcast_to_room(char.current_room_id, char.name + " fumbles with their bandages.", char.name);
        return;
    }

    send_client_message(connection_id, "You try to bandage " + target_name + "'s wounds with your " + tool.name + ", but the bandage slips.");
    send_client_message(connection_id, "You couldn't stop the bleeding. (+" + xp_gained + " Medical XP)");

    // Only send messages to target if it's a player
    if target_type == "player" {
        send_client_message(target_conn, char.name + " tries to bandage your wounds but fumbles.");
        send_client_message(target_conn, "You're still bleeding!");
    }

    broadcast_to_room(char.current_room_id, char.name + " fumbles trying to bandage " + target_name + "'s wounds.", char.name);
}

fn show_usage(connection_id) {
    send_client_message(connection_id, "Usage: stabilize <target>");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Emergency stabilization to stop bleeding.");
    send_client_message(connection_id, "Quicker and easier than full treatment, but only stops bleeding.");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Examples:");
    send_client_message(connection_id, "  stabilize self    - Stop your own bleeding");
    send_client_message(connection_id, "  stabilize Bob     - Stop Bob's bleeding");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Requires bandages or medical supplies in your inventory.");
}
