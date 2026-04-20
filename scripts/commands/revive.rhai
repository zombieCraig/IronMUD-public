// scripts/commands/revive.rhai
// Medical revival command for unconscious players
// @description Revive unconscious players using advanced medical skills
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

    if args == "" {
        show_usage(connection_id);
        return;
    }

    let target_arg = args.to_lower();

    // Find target
    let target_result = find_revive_target(char, target_arg, connection_id);
    if target_result.error != "" {
        send_client_message(connection_id, target_result.error);
        return;
    }

    let target_name = target_result.name;
    let target_conn = target_result.conn;
    let target_char = target_result.char;

    // Check if target is unconscious
    if !target_char.is_unconscious {
        send_client_message(connection_id, target_name + " is not unconscious.");
        return;
    }

    // Check medical skill level (requires level 6+)
    let skill_level = get_skill_level(char.name, "medical");
    if skill_level < 6 {
        send_client_message(connection_id, "You need medical skill level 6 to attempt revival.");
        send_client_message(connection_id, "Your current medical skill: " + skill_level);
        return;
    }

    // Find surgery kit (tier 3 tool required)
    let tool = find_surgery_kit(char.name);
    if tool == () {
        send_client_message(connection_id, "You need a surgery kit to perform revival.");
        send_client_message(connection_id, "Surgery kits are advanced medical tools that can be found or purchased.");
        return;
    }

    // Calculate success chance
    let base_chance = 40 + (skill_level * 4);
    let tool_quality = get_tool_quality_bonus(tool.id);
    let success_chance = base_chance + tool_quality;
    if success_chance > 90 { success_chance = 90; }
    if success_chance < 25 { success_chance = 25; }

    let roll = random_int(1, 100);
    let success = (roll <= success_chance);

    // Process result (surgery kit is reusable, not consumed)
    process_revive_result(success, char, target_name, target_conn, tool, connection_id);
}

fn find_revive_target(char, target_arg, connection_id) {
    let result = #{
        name: "",
        conn: "",
        char: (),
        error: ""
    };

    // Cannot revive self
    if target_arg == "self" || target_arg == "me" || target_arg == char.name.to_lower() {
        result.error = "You cannot revive yourself.";
        return result;
    }

    let target_conn = find_player_connection_by_name(target_arg);
    if target_conn == "" {
        result.error = "Player '" + target_arg + "' is not online.";
        return result;
    }

    let target_char = get_player_character(target_conn);
    if target_char == () {
        result.error = "Player '" + target_arg + "' is not online.";
        return result;
    }

    if target_char.current_room_id != char.current_room_id {
        result.error = target_char.name + " is not here.";
        return result;
    }

    result.name = target_char.name;
    result.conn = target_conn;
    result.char = target_char;
    return result;
}

fn find_surgery_kit(char_name) {
    let items = get_items_in_inventory(char_name);

    for item in items {
        if is_medical_tool(item.id) {
            let tier = get_medical_tool_tier(item.id);
            if tier >= 3 {
                return item;
            }
        }
    }

    return ();
}

fn process_revive_result(success, char, target_name, target_conn, tool, connection_id) {
    if success {
        process_revive_success(char, target_name, target_conn, tool, connection_id);
    } else {
        process_revive_failure(char, target_name, target_conn, tool, connection_id);
    }
}

fn process_revive_success(char, target_name, target_conn, tool, connection_id) {
    // Revive the target
    revive_unconscious_character(target_conn);

    // Award significant XP for revival
    let xp_gained = 50;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_revive_success_messages(char, target_name, target_conn, tool, xp_gained, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_revive_success_messages(char, target_name, target_conn, tool, xp_gained, connection_id) {
    send_client_message(connection_id, "You carefully work on " + target_name + " with your " + tool.name + "...");
    send_client_message(connection_id, "After tense moments, " + target_name + " gasps and regains consciousness!");
    send_client_message(connection_id, "Revival successful! (+" + xp_gained + " Medical XP)");
    send_client_message(target_conn, "Everything was dark... then you feel hands working on you...");
    send_client_message(target_conn, "With a gasp, you return to consciousness! " + char.name + " has revived you.");
    broadcast_to_room(char.current_room_id, char.name + " frantically works to revive " + target_name + "... SUCCESS!", char.name);
}

fn process_revive_failure(char, target_name, target_conn, tool, connection_id) {
    let xp_gained = 15;
    let leveled_up = add_skill_experience(char.name, "medical", xp_gained);

    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_revive_failure_messages(char, target_name, target_conn, tool, xp_gained, connection_id);

    if leveled_up {
        send_client_message(connection_id, "Your medical skill has improved!");
    }
}

fn send_revive_failure_messages(char, target_name, target_conn, tool, xp_gained, connection_id) {
    send_client_message(connection_id, "You carefully work on " + target_name + " with your " + tool.name + "...");
    send_client_message(connection_id, "Despite your efforts, " + target_name + " remains unconscious.");
    send_client_message(connection_id, "The revival attempt failed. Try again quickly! (+" + xp_gained + " Medical XP)");
    broadcast_to_room(char.current_room_id, char.name + " frantically works to revive " + target_name + "... no response.", char.name);
}

fn show_usage(connection_id) {
    send_client_message(connection_id, "Usage: revive <target>");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Attempt to revive an unconscious player.");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Requirements:");
    send_client_message(connection_id, "  - Medical skill level 6 or higher");
    send_client_message(connection_id, "  - Surgery kit (tier 3 medical tool)");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "On success, the target regains consciousness with 10% HP.");
    send_client_message(connection_id, "The surgery kit is reusable and not consumed on use.");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Example:");
    send_client_message(connection_id, "  revive Bob    - Attempt to revive Bob");
}
