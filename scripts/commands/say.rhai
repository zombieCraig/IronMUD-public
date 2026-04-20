// scripts/commands/say.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to speak.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    // Cannot speak underwater
    let room = get_room_data(char.current_room_id);
    if room != () && room.flags.underwater && !char.god_mode {
        send_client_message(connection_id, "You can't speak underwater!");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Say what?");
        return;
    }

    let char_name = char.name;
    let room_id = char.current_room_id;

    // Speaking breaks stealth
    if is_stealthy(char.name) {
        break_stealth(char.name);
        send_client_message(connection_id, "You reveal yourself by speaking.");
    }

    // Garble speech if drunk
    let spoken = args;
    let drunk = get_drunk_level(char.name);
    if drunk > 30 {
        spoken = garble_drunk_speech(args, drunk);
    }

    // Jaw wound garbles outgoing speech
    let jaw_penalty = get_jaw_wound_penalty(char.name);
    if jaw_penalty > 0 {
        spoken = garble_jaw_speech(spoken, jaw_penalty);
    }

    // Show to speaker (show original to self, garbled to others)
    send_client_message(connection_id, "You say: " + args);

    // Broadcast per-listener (ear wounds garble incoming speech)
    let chars_in_room = get_characters_in_room(room_id);
    for listener_name in chars_in_room {
        if listener_name.to_lower() == char_name.to_lower() { continue; }
        let listener_conn = find_player_connection_by_name(listener_name);
        if listener_conn == "" { continue; }
        let heard = spoken;
        let ear_penalty = get_ear_wound_penalty(listener_name);
        if ear_penalty > 0 { heard = garble_hearing(heard, ear_penalty); }
        let listener_char = get_player_character(listener_conn);
        if listener_char != () && listener_char.position == "sleeping" {
            send_client_message(listener_conn, "Someone says in your dream: " + heard);
        } else {
            send_client_message(listener_conn, char_name + " says: " + heard);
        }
    }

    // Check for NPC dialogue responses (regular dialogue first)
    let handled = check_npc_dialogue(room_id, args, connection_id);

    // If no regular dialogue matched, check for healer interactions
    if !handled {
        handled = check_healer_dialogue(room_id, args, char, connection_id);
    }

    // Only fire on_say triggers if neither dialogue system handled it
    if !handled {
        let mobiles = get_mobiles_in_room(room_id);
        for mobile in mobiles {
            fire_mobile_trigger(mobile.id, "on_say", connection_id, #{
                "char_name": char_name,
                "message": args,
                "room_id": room_id,
                "mobile_name": mobile.name
            });
        }
    }
    return;
}

fn check_npc_dialogue(room_id, message, connection_id) {
    let mobiles = get_mobiles_in_room(room_id);
    if mobiles.len() == 0 {
        return false;
    }

    // Get words from the message (lowercase, simple split)
    let msg_lower = message.to_lower();
    let words = msg_lower.split(" ");

    // Check each mobile for matching dialogue
    for mobile in mobiles {
        let activity = get_mobile_activity(mobile.id);

        // Check simulated NPC needs dialogue for status-related keywords
        if is_npc_simulated(mobile.id) {
            if has_needs_keyword(words) {
                let response = get_npc_needs_dialogue(mobile.id);
                if response != "" {
                    let npc_msg = mobile.name + " says: " + response;
                    send_client_message(connection_id, npc_msg);
                    broadcast_to_room_except(room_id, connection_id, npc_msg);
                    return true;
                }
            }
        }

        // Check routine dialogue overrides first (time-of-day responses)
        for word in words {
            let clean = clean_word(word);
            if clean == "" { continue; }
            let routine_response = get_routine_dialogue(mobile.id, clean);
            if routine_response != "" {
                let npc_msg = mobile.name + " says: " + routine_response;
                send_client_message(connection_id, npc_msg);
                broadcast_to_room_except(room_id, connection_id, npc_msg);
                return true;
            }
        }

        // Check if sleeping - show sleeping message if they would have had a match
        if activity == "sleeping" {
            let response = check_mobile_words(mobile.id, words);
            if response != "" {
                send_client_message(connection_id, mobile.name + " is sleeping and doesn't respond.");
                return true;
            }
            continue;
        }

        let response = check_mobile_words(mobile.id, words);
        if response != "" {
            // NPC responds
            let npc_msg = mobile.name + " says: " + response;
            send_client_message(connection_id, npc_msg);
            broadcast_to_room_except(room_id, connection_id, npc_msg);
            return true; // Handled
        }
    }
    return false;
}

fn check_mobile_words(mobile_id, words) {
    for word in words {
        let clean = clean_word(word);
        if clean == "" { continue; }
        let response = get_mobile_dialogue(mobile_id, clean);
        if response != "" {
            return response;
        }
    }
    return "";
}

fn clean_word(word) {
    // Simple cleanup - just return alphanumeric chars
    let result = "";
    for ch in word.chars() {
        if is_letter(ch) {
            result += ch;
        }
    }
    return result;
}

fn is_letter(ch) {
    let letters = "abcdefghijklmnopqrstuvwxyz";
    for l in letters.chars() {
        if ch == l { return true; }
    }
    return false;
}

// ========== Simulated NPC Needs Keywords ==========

fn has_needs_keyword(words) {
    let keywords = ["how", "doing", "feeling", "okay", "alright", "hungry", "tired", "status"];
    for word in words {
        let clean = clean_word(word);
        if clean == "" { continue; }
        for kw in keywords {
            if clean == kw { return true; }
        }
    }
    return false;
}

// ========== Healer Dialogue System ==========

fn check_healer_dialogue(room_id, message, char, connection_id) {
    let mobiles = get_mobiles_in_room(room_id);
    if mobiles.len() == 0 {
        return false;
    }

    let msg_lower = message.to_lower();
    let words = msg_lower.split(" ");

    // Find first healer in room that responds to the keyword
    for mobile in mobiles {
        if !is_healer(mobile.id) {
            continue;
        }

        // Skip healers that are not working (sleeping, off duty, etc.)
        let activity = get_mobile_activity(mobile.id);
        if activity != "working" {
            let healer_type = get_healer_type(mobile.id);
            let keyword = find_healer_keyword(words, healer_type);
            if keyword != "" {
                if activity == "sleeping" {
                    send_client_message(connection_id, mobile.name + " is sleeping and can't help you right now.");
                } else {
                    send_client_message(connection_id, mobile.name + " is not available right now.");
                }
                return true;
            }
            continue;
        }

        let healer_type = get_healer_type(mobile.id);
        let keyword = find_healer_keyword(words, healer_type);

        if keyword != "" {
            handle_healer_keyword(mobile, healer_type, keyword, msg_lower, char, room_id, connection_id);
            return true;
        }
    }
    return false;
}

fn find_healer_keyword(words, healer_type) {
    // First pass: look for specific action keywords (bleeding, bandage, poison, etc.)
    let specific = find_specific_keyword(words, healer_type);
    if specific != "" { return specific; }
    // Second pass: look for generic keywords (treat, heal, cure, cost, help, etc.)
    let generic = find_generic_keyword(words, healer_type);
    return generic;
}

fn find_specific_keyword(words, healer_type) {
    for word in words {
        let clean = clean_word(word);
        if clean == "" { continue; }
        if is_healer_keyword(clean, healer_type) && is_specific_healer_keyword(clean, healer_type) {
            return clean;
        }
    }
    return "";
}

fn find_generic_keyword(words, healer_type) {
    for word in words {
        let clean = clean_word(word);
        if clean == "" { continue; }
        if is_healer_keyword(clean, healer_type) && !is_specific_healer_keyword(clean, healer_type) {
            return clean;
        }
    }
    return "";
}

fn is_specific_healer_keyword(word, healer_type) {
    if healer_type == "medic" {
        if word == "bleeding" { return true; }
        if word == "bandage" { return true; }
        return false;
    }
    if healer_type == "herbalist" {
        if word == "poison" { return true; }
        if word == "antidote" { return true; }
        if word == "illness" { return true; }
        if word == "sick" { return true; }
        if word == "cold" { return true; }
        if word == "burn" { return true; }
        if word == "frostbite" { return true; }
        return false;
    }
    if healer_type == "cleric" {
        if word == "revive" { return true; }
        if word == "resurrection" { return true; }
        return false;
    }
    return false;
}

fn handle_healer_keyword(mobile, healer_type, keyword, msg, char, room_id, connection_id) {
    // Route to appropriate handler based on keyword
    if keyword == "help" || keyword == "cost" || keyword == "price" {
        show_healer_services(mobile, healer_type, room_id, connection_id);
        return;
    }

    // Action keywords - route based on healer type
    if healer_type == "medic" {
        handle_medic_action(mobile, keyword, msg, char, room_id, connection_id);
        return;
    }
    if healer_type == "herbalist" {
        handle_herbalist_action(mobile, keyword, char, room_id, connection_id);
        return;
    }
    if healer_type == "cleric" {
        handle_cleric_action(mobile, keyword, char, room_id, connection_id);
        return;
    }
}

fn show_healer_services(mobile, healer_type, room_id, connection_id) {
    let is_free = is_healing_free(mobile.id);
    let multiplier = 100; // Default

    healer_say(mobile.name, get_healer_greeting(mobile.id), room_id, connection_id);

    if healer_type == "medic" {
        show_medic_prices(mobile, is_free, room_id, connection_id);
        return;
    }
    if healer_type == "herbalist" {
        show_herbalist_prices(mobile, is_free, room_id, connection_id);
        return;
    }
    if healer_type == "cleric" {
        show_cleric_prices(mobile, is_free, room_id, connection_id);
        return;
    }
}

fn show_medic_prices(mobile, is_free, room_id, connection_id) {
    if is_free {
        healer_say(mobile.name, "My services are free. Say 'treat' and I will tend your wounds.", room_id, connection_id);
        return;
    }
    let msg = "My rates: Minor wound - " + get_healing_cost(mobile.id, "minor_wound") + " gold, ";
    msg += "Moderate - " + get_healing_cost(mobile.id, "moderate_wound") + " gold, ";
    msg += "Severe - " + get_healing_cost(mobile.id, "severe_wound") + " gold, ";
    msg += "Stop bleeding - " + get_healing_cost(mobile.id, "bleeding") + " gold.";
    healer_say(mobile.name, msg, room_id, connection_id);
    let tip = "Say 'treat' for your worst wound, or target a specific injury: ";
    tip += "'treat left arm' - treat wound on a body part, ";
    tip += "'treat fracture' - treat a specific wound type, ";
    tip += "'bleeding' - stop bleeding.";
    healer_say(mobile.name, tip, room_id, connection_id);
}

fn show_herbalist_prices(mobile, is_free, room_id, connection_id) {
    if is_free {
        healer_say(mobile.name, "I offer my remedies freely. Say 'cure' to begin treatment.", room_id, connection_id);
        return;
    }
    let msg = "My remedies: Cure poison - " + get_healing_cost(mobile.id, "poison") + " gold, ";
    msg += "Cure illness - " + get_healing_cost(mobile.id, "illness") + " gold.";
    healer_say(mobile.name, msg, room_id, connection_id);
    healer_say(mobile.name, "Say 'cure' when you need treatment.", room_id, connection_id);
}

fn show_cleric_prices(mobile, is_free, room_id, connection_id) {
    if is_free {
        healer_say(mobile.name, "The divine healing is offered freely. Say 'heal' or 'revive'.", room_id, connection_id);
        return;
    }
    let msg = "Blessings available: Restore HP - " + get_healing_cost(mobile.id, "restore_hp") + " gold per 10 HP, ";
    msg += "Full heal - " + get_healing_cost(mobile.id, "full_heal") + " gold, ";
    msg += "Revive fallen - " + get_healing_cost(mobile.id, "revive") + " gold.";
    healer_say(mobile.name, msg, room_id, connection_id);
    healer_say(mobile.name, "Say 'heal' to restore health, or 'revive <name>' to bring back the fallen.", room_id, connection_id);
}

fn handle_medic_action(mobile, keyword, msg, char, room_id, connection_id) {
    if keyword == "treat" || keyword == "wound" || keyword == "wounds" || keyword == "heal" {
        treat_player_wounds(mobile, char, msg, room_id, connection_id);
        return;
    }
    if keyword == "bleeding" || keyword == "bandage" {
        stop_player_bleeding(mobile, char, room_id, connection_id);
        return;
    }
}

fn handle_herbalist_action(mobile, keyword, char, room_id, connection_id) {
    if keyword == "cure" || keyword == "poison" || keyword == "antidote" {
        cure_player_poison(mobile, char, room_id, connection_id);
        return;
    }
    if keyword == "illness" || keyword == "sick" || keyword == "cold" {
        cure_player_illness(mobile, char, room_id, connection_id);
        return;
    }
    if keyword == "burn" || keyword == "frostbite" {
        treat_elemental_wounds(mobile, char, keyword, room_id, connection_id);
        return;
    }
}

fn handle_cleric_action(mobile, keyword, char, room_id, connection_id) {
    if keyword == "heal" || keyword == "restore" || keyword == "bless" || keyword == "prayer" {
        restore_player_hp(mobile, char, room_id, connection_id);
        return;
    }
    if keyword == "revive" || keyword == "resurrection" {
        healer_say(mobile.name, "To revive someone, they must be unconscious and present. Say 'revive <name>'.", room_id, connection_id);
        return;
    }
}

// ========== Medic Actions ==========

fn treat_player_wounds(mobile, char, msg, room_id, connection_id) {
    let wounds = get_character_wounds(char.name);
    if wounds.len() == 0 {
        healer_say(mobile.name, "You have no wounds that need treatment.", room_id, connection_id);
        return;
    }

    // Try to find a targeted wound from the message
    let target = find_wound_from_message(wounds, msg);
    let is_free = is_healing_free(mobile.id);

    if target == () {
        // No specific wound matched — check if player was trying to target
        if has_targeting_intent(msg) {
            show_no_wound_found(mobile, wounds, room_id, connection_id);
            return;
        }
        // Bare "treat" — fall back to worst wound
        target = find_worst_wound(wounds);
    }

    let cost = get_wound_cost(mobile.id, target.level);

    if !is_free && char.gold < cost {
        show_cant_afford_with_alternatives(mobile, char, target, cost, wounds, room_id, connection_id);
        return;
    }

    do_wound_treatment(mobile, char, target, cost, is_free, room_id, connection_id);
}

fn do_wound_treatment(mobile, char, wound, cost, is_free, room_id, connection_id) {
    if !is_free {
        remove_player_gold(connection_id, cost);
    }

    heal_character_wound_by_part(connection_id, wound.body_part);

    // Clear frostbite condition if treating a frostbite wound
    if wound.wound_type.to_lower() == "frostbite" {
        clear_frostbite_condition(connection_id, wound.body_part);
    }

    let examine_msg = "Let me examine that " + wound.wound_type + " on your " + wound.body_part + "...";
    healer_say(mobile.name, examine_msg, room_id, connection_id);

    send_wound_healed_msg(mobile.name, cost, is_free, connection_id);
    broadcast_to_room(room_id, mobile.name + " treats " + char.name + "'s wounds.", char.name);

    check_remaining_wounds(mobile.name, char.name, room_id, connection_id);
}

fn send_wound_healed_msg(healer_name, cost, is_free, connection_id) {
    if is_free {
        send_client_message(connection_id, healer_name + " carefully treats your wound. It has been healed!");
        return;
    }
    let msg = healer_name + " carefully treats your wound. It has been healed! (Cost: " + cost + " gold)";
    send_client_message(connection_id, msg);
}

fn check_remaining_wounds(healer_name, char_name, room_id, connection_id) {
    let remaining = get_character_wounds(char_name);
    if remaining.len() > 0 {
        let hint = build_remaining_wound_hint(remaining);
        healer_say(healer_name, hint, room_id, connection_id);
    }
}

fn stop_player_bleeding(mobile, char, room_id, connection_id) {
    let total_bleeding = get_character_total_bleeding(char.name);
    if total_bleeding == 0 {
        healer_say(mobile.name, "You're not bleeding.", room_id, connection_id);
        return;
    }

    let cost = get_healing_cost(mobile.id, "bleeding");
    let is_free = is_healing_free(mobile.id);

    if !is_free && char.gold < cost {
        healer_say(mobile.name, "Stopping your bleeding costs " + cost + " gold. You don't have enough.", room_id, connection_id);
        return;
    }

    if !is_free {
        remove_player_gold(connection_id, cost);
    }

    stop_character_bleeding(connection_id);

    healer_say(mobile.name, "Hold still while I stop that bleeding...", room_id, connection_id);

    if is_free {
        send_client_message(connection_id, mobile.name + " expertly bandages your wounds. The bleeding has stopped!");
    } else {
        send_client_message(connection_id, mobile.name + " expertly bandages your wounds. The bleeding has stopped! (Cost: " + cost + " gold)");
    }
    broadcast_to_room(room_id, mobile.name + " stops " + char.name + "'s bleeding.", char.name);
}

// ========== Herbalist Actions ==========

fn cure_player_poison(mobile, char, room_id, connection_id) {
    if !has_condition(connection_id, "poisoned") {
        healer_say(mobile.name, "You're not poisoned.", room_id, connection_id);
        return;
    }

    let cost = get_healing_cost(mobile.id, "poison");
    let is_free = is_healing_free(mobile.id);

    if !is_free && char.gold < cost {
        healer_say(mobile.name, "Curing your poison costs " + cost + " gold. You don't have enough.", room_id, connection_id);
        return;
    }

    if !is_free {
        remove_player_gold(connection_id, cost);
    }

    cure_character_poison(connection_id);

    healer_say(mobile.name, "Drink this antidote...", room_id, connection_id);

    if is_free {
        send_client_message(connection_id, mobile.name + " administers an antidote. The poison has been neutralized!");
    } else {
        send_client_message(connection_id, mobile.name + " administers an antidote. The poison has been neutralized! (Cost: " + cost + " gold)");
    }
    broadcast_to_room(room_id, mobile.name + " cures " + char.name + " of poison.", char.name);
}

fn cure_player_illness(mobile, char, room_id, connection_id) {
    if !has_condition(connection_id, "illness") {
        healer_say(mobile.name, "You're not ill.", room_id, connection_id);
        return;
    }

    let cost = get_healing_cost(mobile.id, "illness");
    let is_free = is_healing_free(mobile.id);

    if !is_free && char.gold < cost {
        healer_say(mobile.name, "Curing your illness costs " + cost + " gold. You don't have enough.", room_id, connection_id);
        return;
    }

    if !is_free {
        remove_player_gold(connection_id, cost);
    }

    cure_character_illness(connection_id);

    healer_say(mobile.name, "Take this remedy for your ailment...", room_id, connection_id);

    if is_free {
        send_client_message(connection_id, mobile.name + " gives you herbal medicine. Your illness has been cured!");
    } else {
        send_client_message(connection_id, mobile.name + " gives you herbal medicine. Your illness has been cured! (Cost: " + cost + " gold)");
    }
    broadcast_to_room(room_id, mobile.name + " cures " + char.name + " of illness.", char.name);
}

fn treat_elemental_wounds(mobile, char, wound_type, room_id, connection_id) {
    let wounds = get_character_wounds(char.name);
    let target_type = wound_type;
    if wound_type == "burn" { target_type = "burn"; }
    if wound_type == "frostbite" { target_type = "frostbite"; }

    // Find matching elemental wound
    let found_wound = ();
    for w in wounds {
        if w.wound_type.to_lower() == target_type {
            found_wound = w;
            break;
        }
    }

    if found_wound == () {
        healer_say(mobile.name, "You don't have any " + wound_type + " wounds.", room_id, connection_id);
        return;
    }

    let cost = get_wound_cost(mobile.id, found_wound.level);
    let is_free = is_healing_free(mobile.id);

    if !is_free && char.gold < cost {
        healer_say(mobile.name, "Treating your " + wound_type + " costs " + cost + " gold. You don't have enough.", room_id, connection_id);
        return;
    }

    if !is_free {
        remove_player_gold(connection_id, cost);
    }

    heal_character_wound_by_part(connection_id, found_wound.body_part);

    // Also clear the frostbite condition if treating frostbite
    if wound_type == "frostbite" {
        clear_frostbite_condition(connection_id, found_wound.body_part);
    }

    healer_say(mobile.name, "Let me apply a soothing salve to that " + wound_type + "...", room_id, connection_id);

    if is_free {
        send_client_message(connection_id, mobile.name + " applies healing salve. Your " + wound_type + " has been treated!");
    } else {
        send_client_message(connection_id, mobile.name + " applies healing salve. Your " + wound_type + " has been treated! (Cost: " + cost + " gold)");
    }
    broadcast_to_room(room_id, mobile.name + " treats " + char.name + "'s " + wound_type + ".", char.name);
}

// ========== Cleric Actions ==========

fn restore_player_hp(mobile, char, room_id, connection_id) {
    if char.hp >= char.max_hp {
        healer_say(mobile.name, "You are already at full health.", room_id, connection_id);
        return;
    }

    let is_free = is_healing_free(mobile.id);

    if is_free {
        do_free_heal(mobile, char, room_id, connection_id);
        return;
    }

    do_paid_heal(mobile, char, room_id, connection_id);
}

fn do_free_heal(mobile, char, room_id, connection_id) {
    full_heal_character(connection_id);
    healer_say(mobile.name, "May the light restore you...", room_id, connection_id);
    send_client_message(connection_id, mobile.name + " channels divine energy. You have been fully healed!");
    broadcast_to_room(room_id, mobile.name + " heals " + char.name + " with divine light.", char.name);
}

fn do_paid_heal(mobile, char, room_id, connection_id) {
    let cost = get_healing_cost(mobile.id, "full_heal");

    if char.gold >= cost {
        do_full_paid_heal(mobile, char, cost, room_id, connection_id);
        return;
    }

    try_partial_heal(mobile, char, cost, room_id, connection_id);
}

fn do_full_paid_heal(mobile, char, cost, room_id, connection_id) {
    remove_player_gold(connection_id, cost);
    full_heal_character(connection_id);
    healer_say(mobile.name, "May the light restore you...", room_id, connection_id);
    let msg = mobile.name + " channels divine energy. You have been fully healed! (Cost: " + cost + " gold)";
    send_client_message(connection_id, msg);
    broadcast_to_room(room_id, mobile.name + " heals " + char.name + " with divine light.", char.name);
}

fn try_partial_heal(mobile, char, full_cost, room_id, connection_id) {
    let hp_cost = get_healing_cost(mobile.id, "restore_hp");
    let affordable_heals = char.gold / hp_cost;

    if affordable_heals == 0 {
        let msg = "A full heal costs " + full_cost + " gold, or " + hp_cost + " gold per 10 HP. You don't have enough.";
        healer_say(mobile.name, msg, room_id, connection_id);
        return;
    }

    let hp_to_restore = affordable_heals * 10;
    let actual_cost = affordable_heals * hp_cost;
    remove_player_gold(connection_id, actual_cost);
    restore_character_hp(connection_id, hp_to_restore);

    healer_say(mobile.name, "May the light restore you...", room_id, connection_id);
    let msg = mobile.name + " channels divine energy. You recover " + hp_to_restore + " HP! (Cost: " + actual_cost + " gold)";
    send_client_message(connection_id, msg);
    broadcast_to_room(room_id, mobile.name + " heals " + char.name + " with divine light.", char.name);
}

// ========== Helper Functions ==========

fn healer_say(healer_name, message, room_id, connection_id) {
    let full_msg = healer_name + " says: " + message;
    send_client_message(connection_id, full_msg);
    // Use broadcast_to_room_except to avoid sending to the player twice
    broadcast_to_room_except(room_id, connection_id, full_msg);
}

fn find_worst_wound(wounds) {
    let worst = wounds[0];
    for i in 1..wounds.len() {
        let w = wounds[i];
        if get_wound_severity(w.level) > get_wound_severity(worst.level) {
            worst = w;
        }
    }
    return worst;
}

fn get_wound_severity(level) {
    if level == "minor" { return 1; }
    if level == "moderate" { return 2; }
    if level == "severe" { return 3; }
    if level == "critical" { return 4; }
    if level == "disabled" { return 5; }
    return 0;
}

fn get_wound_cost(mobile_id, level) {
    if level == "minor" { return get_healing_cost(mobile_id, "minor_wound"); }
    if level == "moderate" { return get_healing_cost(mobile_id, "moderate_wound"); }
    if level == "severe" { return get_healing_cost(mobile_id, "severe_wound"); }
    if level == "critical" { return get_healing_cost(mobile_id, "severe_wound"); }
    if level == "disabled" { return get_healing_cost(mobile_id, "severe_wound"); }
    return get_healing_cost(mobile_id, "minor_wound");
}

fn remove_player_gold(connection_id, amount) {
    let char = get_player_character(connection_id);
    if char != () {
        char.gold -= amount;
        set_player_character(connection_id, char);
        save_character_data(char);
    }
}

// ========== Wound Targeting ==========

fn find_wound_from_message(wounds, msg) {
    // Try body part match first (multi-word parts before single-word)
    let by_part = find_wound_by_body_part(wounds, msg);
    if by_part != () { return by_part; }
    // Fall back to wound type match
    let by_type = find_wound_by_type(wounds, msg);
    return by_type;
}

fn find_wound_by_body_part(wounds, msg) {
    // Check multi-word body parts first (more specific)
    let result = find_wound_with_body_part(wounds, msg, "left arm");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right arm");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "left leg");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right leg");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "left hand");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right hand");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "left foot");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right foot");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "left eye");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right eye");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "left ear");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "right ear");
    if result != () { return result; }
    // Single-word body parts
    result = find_wound_with_body_part(wounds, msg, "head");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "neck");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "torso");
    if result != () { return result; }
    result = find_wound_with_body_part(wounds, msg, "jaw");
    if result != () { return result; }
    return ();
}

fn find_wound_with_body_part(wounds, msg, part_name) {
    if !msg.contains(part_name) { return (); }
    for w in wounds {
        if w.body_part == part_name { return w; }
    }
    return ();
}

fn find_wound_by_type(wounds, msg) {
    let result = find_wound_with_type(wounds, msg, "fracture");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "cut");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "puncture");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "bruise");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "burn");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "frostbite");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "poisoned");
    if result != () { return result; }
    result = find_wound_with_type(wounds, msg, "corroded");
    if result != () { return result; }
    return ();
}

fn find_wound_with_type(wounds, msg, type_name) {
    if !msg.contains(type_name) { return (); }
    for w in wounds {
        if w.wound_type.to_lower() == type_name { return w; }
    }
    return ();
}

fn has_targeting_intent(msg) {
    // If the message is more than just a bare keyword, user is trying to target
    let words = msg.split(" ");
    let keyword_count = 0;
    for word in words {
        let clean = clean_word(word);
        if clean != "" { keyword_count += 1; }
    }
    return keyword_count > 1;
}

fn show_no_wound_found(mobile, wounds, room_id, connection_id) {
    let msg = "I don't see a matching wound. Your current injuries: ";
    for i in 0..wounds.len() {
        if i > 0 { msg += ", "; }
        msg += wounds[i].level + " " + wounds[i].wound_type + " (" + wounds[i].body_part + ")";
    }
    msg += ".";
    healer_say(mobile.name, msg, room_id, connection_id);
}

fn show_cant_afford_with_alternatives(mobile, char, wound, cost, wounds, room_id, connection_id) {
    let msg = "Treating your " + wound.level + " " + wound.wound_type;
    msg += " on your " + wound.body_part + " costs " + cost;
    msg += " gold. You don't have enough.";
    healer_say(mobile.name, msg, room_id, connection_id);

    let alternatives = build_affordable_list(mobile, char, wound, wounds);
    if alternatives != "" {
        healer_say(mobile.name, "But I can help with: " + alternatives + ".", room_id, connection_id);
        healer_say(mobile.name, "Say 'treat <body part>' to choose a specific wound.", room_id, connection_id);
    }
}

fn build_affordable_list(mobile, char, target_wound, wounds) {
    let is_free = is_healing_free(mobile.id);
    let parts = [];
    // Check other wounds the player can afford
    for w in wounds {
        if w.body_part == target_wound.body_part && w.wound_type == target_wound.wound_type {
            continue;
        }
        let w_cost = get_wound_cost(mobile.id, w.level);
        if is_free || char.gold >= w_cost {
            parts.push(w.level + " " + w.wound_type + " on " + w.body_part + " (" + w_cost + "g)");
        }
    }
    // Check if bleeding treatment is affordable
    let total_bleeding = get_character_total_bleeding(char.name);
    if total_bleeding > 0 {
        let bleed_cost = get_healing_cost(mobile.id, "bleeding");
        if is_free || char.gold >= bleed_cost {
            parts.push("stop bleeding (" + bleed_cost + "g)");
        }
    }
    let result = "";
    for i in 0..parts.len() {
        if i > 0 { result += ", "; }
        result += parts[i];
    }
    return result;
}

fn build_remaining_wound_hint(remaining) {
    let msg = "You still have " + remaining.len() + " wound(s) remaining: ";
    for i in 0..remaining.len() {
        if i > 0 { msg += ", "; }
        msg += remaining[i].wound_type + " (" + remaining[i].body_part + ")";
    }
    msg += ". Say 'treat' or 'treat <body part>' for more help.";
    return msg;
}

// ========== Drunk Speech Garbling ==========

fn garble_drunk_speech(text, drunk_level) {
    let result = "";
    let garble_chance = (drunk_level - 30) / 2; // 0-35% chance per char at max drunk
    let hic_chance = drunk_level / 5; // Up to 20% chance to insert *hic*
    let word_count = 0;

    for ch in text.chars() {
        let roll = random_int(1, 100);
        if roll <= garble_chance {
            result += garble_char(ch);
        } else {
            result += ch;
        }
        if ch == " " {
            word_count += 1;
            // Chance to insert *hic* between words
            if random_int(1, 100) <= hic_chance && word_count > 1 {
                result += "*hic* ";
            }
        }
    }
    return result;
}

fn garble_char(ch) {
    if ch == "s" { return "sh"; }
    if ch == "S" { return "Sh"; }
    if ch == "r" { return "rr"; }
    if ch == "t" { return "d"; }
    if ch == "T" { return "D"; }
    return ch;
}
