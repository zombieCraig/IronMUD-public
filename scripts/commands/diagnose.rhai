// scripts/commands/diagnose.rhai
// Medical diagnosis command for viewing wounds and conditions
// @description View medical status of a target
// @category skills

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Default to self if no argument
    let target_arg = args;
    if target_arg == "" {
        target_arg = "self";
    }
    target_arg = target_arg.to_lower();

    // Try to find and diagnose target
    let result = find_diagnose_target(target_arg, char, connection_id);
    if result.done { return; }

    let target_name = result.target_name;
    let target_conn = result.target_conn;
    let is_self = result.is_self;

    // Build diagnosis output
    let output = "";

    if is_self {
        output = "\n=== Medical Diagnosis: Yourself ===\n";
    } else {
        output = "\n=== Medical Diagnosis: " + target_name + " ===\n";
    }

    // Get wounds
    let wounds = get_character_wounds(target_name);

    output += "\n--- Wounds ---\n";
    if wounds.len() == 0 {
        output += "  No wounds.\n";
    } else {
        for wound in wounds {
            let bleed_text = "";
            if wound.bleeding_severity > 0 {
                bleed_text = " [BLEEDING: " + wound.bleeding_severity + "]";
            }
            let penalty_text = "";
            if wound.penalty > 0 {
                penalty_text = " (-" + wound.penalty + "%)";
            }
            output += "  " + pad_right(wound.body_part, 12) + " ";
            output += pad_right(wound.level, 10) + " ";
            output += wound.wound_type + penalty_text + bleed_text + "\n";
        }

        // Treatment recommendations
        output += "\n--- Treatment Recommendations ---\n";
        for wound in wounds {
            let tool_name = get_tool_recommendation(wound.wound_type, wound.body_part);
            let tier_needed = get_required_tier_for_wound(wound.level);
            output += "  " + wound.body_part + ": " + tool_name + " (tier " + tier_needed + "+)\n";
        }
    }

    // Check conditions (only if target is online, use connection_id)
    output += "\n--- Conditions ---\n";
    let has_conditions = false;

    if has_condition(target_conn, "hypothermia") {
        output += "  [HYPOTHERMIA] Shivering, reduced dexterity and strength\n";
        has_conditions = true;
    }

    if has_condition(target_conn, "frostbite") {
        output += "  [FROSTBITE] Tissue damage from extreme cold\n";
        has_conditions = true;
    }

    if has_condition(target_conn, "heat_exhaustion") {
        output += "  [HEAT EXHAUSTION] Dehydrated, reduced constitution\n";
        has_conditions = true;
    }

    if has_condition(target_conn, "heat_stroke") {
        output += "  [HEAT STROKE] Critical - risk of collapse\n";
        has_conditions = true;
    }

    if has_condition(target_conn, "illness") {
        let progress = get_illness_progress(target_conn);
        output += "  [ILLNESS] Sick with cold/flu (severity: " + progress + "%)\n";
        has_conditions = true;
    }

    if !has_conditions {
        output += "  No active conditions.\n";
    }

    // Check ongoing effects
    let ongoing = get_ongoing_effects(target_name);
    if ongoing.len() > 0 {
        output += "\n--- Ongoing Effects ---\n";
        for eff in ongoing {
            let eff_label = get_ongoing_effect_label(eff.effect_type);
            output += "  " + pad_right(eff.body_part, 12) + " ";
            output += pad_right(eff_label, 12) + " ";
            output += eff.rounds_remaining + " rounds remaining";
            output += " (" + eff.damage_per_round + " damage/round)\n";
        }
    }

    // Check exposure levels
    output += "\n--- Exposure Status ---\n";
    let wet_level = get_wet_level(target_conn);
    let cold_exposure = get_cold_exposure(target_conn);
    let heat_exposure = get_heat_exposure(target_conn);

    if wet_level > 0 {
        output += "  Wet: " + wet_level + "%";
        if wet_level >= 75 {
            output += " (soaked)";
        } else if wet_level >= 50 {
            output += " (very wet)";
        } else if wet_level >= 25 {
            output += " (damp)";
        }
        output += "\n";
    }

    if cold_exposure > 0 {
        output += "  Cold exposure: " + cold_exposure + "%";
        if cold_exposure >= 80 {
            output += " (CRITICAL)";
        } else if cold_exposure >= 50 {
            output += " (dangerous)";
        } else if cold_exposure >= 25 {
            output += " (uncomfortable)";
        }
        output += "\n";
    }

    if heat_exposure > 0 {
        output += "  Heat exposure: " + heat_exposure + "%";
        if heat_exposure >= 80 {
            output += " (CRITICAL)";
        } else if heat_exposure >= 50 {
            output += " (dangerous)";
        } else if heat_exposure >= 25 {
            output += " (uncomfortable)";
        }
        output += "\n";
    }

    if wet_level == 0 && cold_exposure == 0 && heat_exposure == 0 {
        output += "  Normal - no environmental exposure.\n";
    }

    // Calculate overall status
    output += "\n--- Overall Assessment ---\n";
    let total_bleeding = get_character_total_bleeding(target_name);
    let wound_count = wounds.len();

    // Get HP status
    let target_char = get_character_data(target_name);
    let hp_percent = 100;
    if target_char != () && target_char.max_hp > 0 {
        hp_percent = (target_char.hp * 100) / target_char.max_hp;
    }

    // Determine overall status based on HP, wounds, bleeding, and conditions
    let status = get_overall_status(hp_percent, wound_count, total_bleeding, has_conditions);
    output += "  Status: " + status + "\n";

    send_client_message(connection_id, output);
}

fn pad_right(text, width) {
    let result = text;
    let padding = width - text.len();
    if padding > 0 {
        for i in 0..padding {
            result += " ";
        }
    }
    return result;
}

// Determine overall status based on HP, wounds, bleeding, and conditions
fn get_overall_status(hp_percent, wound_count, total_bleeding, has_conditions) {
    // Critical conditions first (most urgent)
    if hp_percent <= 0 {
        return "DEAD";
    }
    if total_bleeding >= 4 {
        return "CRITICAL - Heavy bleeding!";
    }
    if hp_percent <= 10 {
        return "CRITICAL - Near death!";
    }

    // Serious conditions
    if hp_percent <= 25 {
        return "Serious - Barely holding on";
    }
    if wound_count >= 3 || has_conditions {
        return "Serious - Needs immediate attention";
    }

    // Moderate conditions
    if hp_percent <= 50 {
        return "Wounded - Has significant injuries";
    }
    if total_bleeding > 0 {
        return "Moderate - Bleeding requires treatment";
    }

    // Minor conditions
    if hp_percent <= 75 {
        return "Bruised - Has minor injuries";
    }
    if wound_count > 0 {
        return "Stable - Minor injuries";
    }

    // Healthy
    if hp_percent >= 100 {
        return "Healthy - In perfect condition";
    }
    return "Good - A few scratches";
}

fn find_diagnose_target(target_arg, char, connection_id) {
    // Check for self-diagnosis
    if target_arg == "self" || target_arg == "me" || target_arg == char.name.to_lower() {
        return #{
            target_name: char.name,
            target_conn: connection_id,
            is_self: true,
            done: false
        };
    }

    // Try to find a player in the room
    let player_result = find_player_target(target_arg, char);
    if player_result.found {
        return #{
            target_name: player_result.name,
            target_conn: player_result.conn,
            is_self: player_result.name.to_lower() == char.name.to_lower(),
            done: false
        };
    }

    // Try to find a mobile in the room
    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(target_arg, mobiles);
    if mobile != () {
        diagnose_mobile(mobile, char, connection_id);
        return #{ done: true };
    }

    // Neither player nor mobile found
    send_client_message(connection_id, "You don't see '" + target_arg + "' here.");
    return #{ done: true };
}

fn find_player_target(target_arg, char) {
    let target_conn = find_player_connection_by_name(target_arg);
    if target_conn == "" {
        return #{ found: false };
    }

    let target_char = get_player_character(target_conn);
    if target_char == () {
        return #{ found: false };
    }

    if target_char.current_room_id != char.current_room_id {
        return #{ found: false };
    }

    return #{
        found: true,
        name: target_char.name,
        conn: target_conn
    };
}

fn get_required_tier_for_wound(level) {
    if level == "minor" { return 1; }
    if level == "moderate" { return 2; }
    if level == "severe" { return 3; }
    if level == "critical" { return 3; }
    if level == "disabled" { return 3; }
    return 1;
}

fn get_tool_recommendation(wound_type, body_part) {
    let wt = wound_type.to_lower();
    let bp = body_part.to_lower();
    if wt == "fracture" && (bp == "torso" || bp == "neck" || bp == "head") {
        return "Bandage";  // Can't splint torso/neck/head - wrap/bind instead
    }
    if wt == "fracture" && bp == "jaw" {
        return "Splint";  // Jaw can be splinted
    }
    if wt == "cut" { return "Bandage"; }
    if wt == "puncture" { return "Bandage"; }
    if wt == "bruise" { return "Bruise Salve"; }
    if wt == "fracture" { return "Splint"; }
    if wt == "burn" { return "Burn Cream"; }
    if wt == "frostbite" { return "Warming Salve"; }
    if wt == "poisoned" { return "Antidote"; }
    if wt == "corroded" { return "Neutralizer"; }
    // Eye and ear wounds default to bandage
    if bp == "left eye" || bp == "right eye" { return "Bandage"; }
    if bp == "left ear" || bp == "right ear" { return "Bandage"; }
    return "Medical Supplies";
}

fn diagnose_mobile(mobile, char, connection_id) {
    let output = "\n=== Medical Diagnosis: " + mobile.name + " ===\n";

    // Get wounds using mobile-specific function
    let wounds = get_mobile_wounds(mobile.id);

    output += "\n--- Wounds ---\n";
    if wounds.len() == 0 {
        output += "  No wounds.\n";
    } else {
        for wound in wounds {
            let bleed_text = "";
            if wound.bleeding_severity > 0 {
                bleed_text = " [BLEEDING: " + wound.bleeding_severity + "]";
            }
            let penalty_text = "";
            if wound.penalty > 0 {
                penalty_text = " (-" + wound.penalty + "%)";
            }
            output += "  " + pad_right(wound.body_part, 12) + " ";
            output += pad_right(wound.level, 10) + " ";
            output += wound.wound_type + penalty_text + bleed_text + "\n";
        }

        // Treatment recommendations
        output += "\n--- Treatment Recommendations ---\n";
        for wound in wounds {
            let tool_name = get_tool_recommendation(wound.wound_type, wound.body_part);
            let tier_needed = get_required_tier_for_wound(wound.level);
            output += "  " + wound.body_part + ": " + tool_name + " (tier " + tier_needed + "+)\n";
        }
    }

    // Check ongoing effects for mobile
    let ongoing = get_mobile_ongoing_effects(mobile.id);
    if ongoing.len() > 0 {
        output += "\n--- Ongoing Effects ---\n";
        for eff in ongoing {
            let eff_label = get_ongoing_effect_label(eff.effect_type);
            output += "  " + pad_right(eff.body_part, 12) + " ";
            output += pad_right(eff_label, 12) + " ";
            output += eff.rounds_remaining + " rounds remaining";
            output += " (" + eff.damage_per_round + " damage/round)\n";
        }
    }

    // Overall assessment based on HP, wounds, and bleeding (mobiles don't have environmental conditions)
    output += "\n--- Overall Assessment ---\n";
    let total_bleeding = get_mobile_total_bleeding(mobile.id);
    let wound_count = wounds.len();

    // Calculate HP percentage
    let hp_percent = 100;
    if mobile.max_hp > 0 {
        hp_percent = (mobile.hp * 100) / mobile.max_hp;
    }

    // Determine overall status based on HP, wounds, and bleeding
    let status = get_overall_status(hp_percent, wound_count, total_bleeding, false);
    output += "  Status: " + status + "\n";

    send_client_message(connection_id, output);
}

fn get_ongoing_effect_label(effect_type) {
    if effect_type == "fire" { return "BURNING"; }
    if effect_type == "cold" { return "FROSTBITTEN"; }
    if effect_type == "poison" { return "POISONED"; }
    if effect_type == "acid" { return "CORRODING"; }
    return effect_type.to_upper();
}
