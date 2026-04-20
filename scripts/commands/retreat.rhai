// scripts/commands/retreat.rhai
// Retreat from combat target to create distance

fn is_leg_part(body_part) {
    if body_part == "left leg" { return true; }
    if body_part == "right leg" { return true; }
    if body_part == "left foot" { return true; }
    if body_part == "right foot" { return true; }
    return false;
}

fn get_leg_wound_penalty(char_name) {
    let wounds = get_character_wounds(char_name);
    let penalty = 0;
    for wound in wounds {
        if is_leg_part(wound.body_part) {
            penalty = penalty + get_wound_level_penalty(wound.level);
        }
    }
    return penalty;
}

fn handle_retreat_blocked(char, target_name, connection_id) {
    send_client_message(connection_id, target_name + " blocks your retreat! (stamina spent, turn wasted)");

    // Message to room
    let others_msg = char.name + " tries to retreat but is blocked by " + target_name + ".";
    broadcast_to_room_awake(char.current_room_id, others_msg, char.name);
}

fn has_disabled_legs(char_name) {
    let wounds = get_character_wounds(char_name);
    for wound in wounds {
        if is_leg_part(wound.body_part) && wound.level == "disabled" {
            return true;
        }
    }
    return false;
}

fn run_command(args, connection_id) {
    let RETREAT_STAMINA_COST = 15;
    let MIN_STAMINA_RESTORE = 5;

    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Must be in combat
    if !is_in_combat(char.name) {
        send_client_message(connection_id, "You're not in combat.");
        return;
    }

    // Can't retreat while incapacitated
    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't do that while asleep.");
        return;
    }
    if char.position == "unconscious" {
        send_client_message(connection_id, "You can't do that while unconscious.");
        return;
    }

    // Get primary target
    let target = get_primary_target(char.name);
    if target == () {
        send_client_message(connection_id, "You have no combat target.");
        return;
    }

    let target_id = target.target_id;
    let target_type = target.target_type;

    // Get current distance
    let current_distance = get_combat_distance(char.name, target_id);

    // Check if can retreat
    if !can_retreat(char.name, target_id) {
        send_client_message(connection_id, "You're already at maximum range!");
        return;
    }

    // Check stamina - if exhausted, rest this turn instead
    if char.stamina <= 0 {
        char.stamina = MIN_STAMINA_RESTORE;
        save_character_data(char);
        send_client_message(connection_id, "You are too exhausted to retreat! You catch your breath...");
        return;
    }

    // Check if have enough stamina for the retreat
    if char.stamina < RETREAT_STAMINA_COST {
        send_client_message(connection_id, "You don't have enough stamina to retreat! (need " + RETREAT_STAMINA_COST + ", have " + char.stamina + ")");
        return;
    }

    // Deduct stamina regardless of success
    char.stamina = char.stamina - RETREAT_STAMINA_COST;
    save_character_data(char);

    // Get target info for DEX contest
    let target_dex = 10;
    let target_name = "your opponent";

    if target_type == "mobile" {
        let mobile = get_mobile_data(target_id);
        if mobile != () {
            target_dex = mobile.stat_dex;
            target_name = mobile.name;
        }
    }

    // Calculate modifiers
    let char_dex_mod = get_stat_modifier(get_effective_stat(char.name, "dexterity"));
    let target_dex_mod = get_stat_modifier(target_dex);

    // Apply leg wound penalties to retreater
    let leg_penalty = get_leg_wound_penalty(char.name);
    let retreat_mod = char_dex_mod - (leg_penalty / 10);

    // DEX contest: d20 + attacker_dex_mod vs 10 + defender_dex_mod
    let roll = roll_dice(1, 20);
    let retreat_total = roll + retreat_mod;
    let dc = 10 + target_dex_mod;

    if retreat_total < dc {
        handle_retreat_blocked(char, target_name, connection_id);
        return;
    }

    // Success - actually move back
    let result = attempt_retreat(char.name, target_id);
    if result != "success" {
        send_client_message(connection_id, "You fail to retreat.");
        return;
    }

    let new_distance = get_combat_distance(char.name, target_id);
    let step_desc = get_distance_step_name(current_distance, new_distance);

    // Message to player
    let msg = "You " + step_desc + " from " + target_name + "! (now at " + new_distance + " range)";
    send_client_message(connection_id, msg);

    // Message to room
    let others_msg = char.name + " " + step_desc + " from " + target_name + ".";
    broadcast_to_room_awake(char.current_room_id, others_msg, char.name);
}
