// scripts/commands/advance.rhai
// Advance toward combat target to close distance

fn is_leg_part(body_part) {
    if body_part == "left leg" { return true; }
    if body_part == "right leg" { return true; }
    if body_part == "left foot" { return true; }
    if body_part == "right foot" { return true; }
    return false;
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
    let ADVANCE_STAMINA_COST = 10;
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

    // Can't advance while incapacitated
    if char.position == "sleeping" || char.position == "unconscious" {
        send_client_message(connection_id, "You can't do that while incapacitated.");
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

    // Check if can advance
    if !can_advance(char.name, target_id) {
        send_client_message(connection_id, "You're already at melee range!");
        return;
    }

    // Check if disabled legs require stamina to advance
    let legs_disabled = has_disabled_legs(char.name);
    if legs_disabled {
        if char.stamina <= 0 {
            // Too exhausted - rest this turn
            char.stamina = MIN_STAMINA_RESTORE;
            save_character_data(char);
            send_client_message(connection_id, "You are too exhausted to advance! You catch your breath...");
            return;
        }
        // Deduct stamina for advancing with injured legs
        char.stamina = char.stamina - ADVANCE_STAMINA_COST;
        if char.stamina < 0 { char.stamina = 0; }
        save_character_data(char);
        send_client_message(connection_id, "You struggle forward on your injured legs. (-" + ADVANCE_STAMINA_COST + " stamina)");
    }

    // Perform advance
    if attempt_advance(char.name, target_id) {
        let new_distance = get_combat_distance(char.name, target_id);
        let step_desc = get_distance_step_name(current_distance, new_distance);

        // Get target name for messages
        let target_name = "your target";
        if target_type == "mobile" {
            let mobile = get_mobile_data(target_id);
            if mobile != () {
                target_name = mobile.name;
            }
        }

        // Message to player
        let msg = "You " + step_desc + " toward " + target_name + "! (now at " + new_distance + " range)";
        send_client_message(connection_id, msg);

        // Message to room
        let others_msg = char.name + " " + step_desc + " toward " + target_name + ".";
        broadcast_to_room_awake(char.current_room_id, others_msg, char.name);
    } else {
        send_client_message(connection_id, "You fail to advance.");
    }
}
