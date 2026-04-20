// scripts/commands/assist.rhai
// Assist command - join an ongoing fight by attacking your ally's target

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to assist.");
        return;
    }

    // Can't assist while sleeping
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }

    // Can't assist while resting
    if char.position == "resting" || char.position == "sitting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    // Already in combat?
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You are already in combat! Use 'attack' to switch targets.");
        return;
    }

    // Determine who to assist
    let assist_target = "";
    if args == "" {
        // Default to group leader if following someone
        let leader = get_group_leader(char.name);
        if leader == "" {
            send_client_message(connection_id, "Assist whom? You're not following anyone.");
            send_client_message(connection_id, "Usage: assist <player>");
            return;
        }
        assist_target = leader;
    } else {
        assist_target = args;
    }

    // Find the player to assist in the room
    let room_id = char.current_room_id;
    let characters = get_characters_in_room(room_id);
    let found_player = "";

    for char_name in characters {
        if char_name.to_lower() == assist_target.to_lower() || char_name.to_lower().starts_with(assist_target.to_lower()) {
            found_player = char_name;
        }
    }

    if found_player == "" {
        send_client_message(connection_id, "You don't see " + assist_target + " here.");
        return;
    }

    if found_player.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "You can't assist yourself.");
        return;
    }

    // Check if the player is in combat
    if !is_in_combat(found_player) {
        send_client_message(connection_id, found_player + " is not in combat.");
        return;
    }

    // Get their primary target
    let target_info = get_primary_target_info(found_player);
    if target_info == () {
        send_client_message(connection_id, found_player + " doesn't have a target.");
        return;
    }

    // Check combat zone
    let zone = get_combat_zone(room_id);
    if zone == "safe" {
        send_client_message(connection_id, "This is a safe zone. Combat is not allowed here.");
        return;
    }

    // Check stamina (configurable via settings)
    let stamina_cost_str = get_setting_or_default("stamina_cost_attack", "5");
    let stamina_cost = parse_int(stamina_cost_str);
    if stamina_cost == () { stamina_cost = 5; }
    if char.stamina < stamina_cost {
        send_client_message(connection_id, "You are too exhausted to join the fight. Rest to recover stamina.");
        return;
    }

    // Consume stamina
    if !consume_stamina_for_attack(char.name, stamina_cost) {
        send_client_message(connection_id, "Failed to consume stamina.");
        return;
    }

    // Enter combat based on target type
    if target_info.target_type == "mobile" {
        assist_vs_mobile(char, target_info, found_player, connection_id);
    } else if target_info.target_type == "player" {
        // PvP assist - only allowed in PvP zones
        if zone != "pvp" {
            send_client_message(connection_id, "You can only assist in PvP combat in PvP zones.");
            return;
        }
        assist_vs_player(char, target_info, found_player, connection_id);
    }
}

fn assist_vs_mobile(char, target_info, ally_name, connection_id) {
    let mobile = get_mobile_data(target_info.target_id);
    if mobile == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // Check if mobile has no_attack flag
    if mobile.flags.no_attack {
        send_client_message(connection_id, mobile.name + " cannot be attacked.");
        return;
    }

    // Enter combat with the mobile
    enter_combat(char.name, "mobile", target_info.target_id);

    // Mobile adds the assisting player as a target too
    enter_mobile_combat(target_info.target_id, "player", char.name);

    // Send messages
    send_client_message(connection_id, "You join " + ally_name + " in combat against " + mobile.name + "!");
    broadcast_to_room_awake(char.current_room_id, char.name + " joins the fight against " + mobile.name + "!", char.name);
}

fn assist_vs_player(char, target_info, ally_name, connection_id) {
    let target_name = target_info.target_id; // For player targets, target_id is the name
    let target = get_character_data(target_name);
    if target == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // Enter combat with the player
    enter_combat(char.name, "player", target_name);

    // Target enters combat with the assisting player too
    enter_combat(target_name, "player", char.name);

    // Send messages
    send_client_message(connection_id, "You join " + ally_name + " in combat against " + target_name + "!");
    broadcast_to_room_awake(char.current_room_id, char.name + " joins the fight against " + target_name + "!", char.name);
}
