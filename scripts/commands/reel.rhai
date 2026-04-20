// scripts/commands/reel.rhai
// reel - Reel in your fishing line
// Can be used to cancel fishing early or to pull in after a bite

fn get_level_up_message(level) {
    if level == 1 { return "You're getting the hang of casting a line."; }
    if level == 2 { return "You're learning to read the water better."; }
    if level == 3 { return "Your patience at the water's edge is improving."; }
    if level == 4 { return "You've learned to feel the subtle tugs on your line."; }
    if level == 5 { return "You're becoming a skilled angler!"; }
    if level == 6 { return "Your knowledge of fish behavior grows."; }
    if level == 7 { return "You can sense where the fish are hiding."; }
    if level == 8 { return "Expert fishermen would respect your technique."; }
    if level == 9 { return "You're approaching mastery of the angler's art."; }
    if level == 10 { return "You have achieved MASTERY in Fishing! The waters hold no secrets from you."; }
    return "";
}

fn get_rarity_xp_multiplier(rarity) {
    if rarity == "common" { return 10; }      // 1.0x but using int
    if rarity == "uncommon" { return 15; }    // 1.5x
    if rarity == "rare" { return 25; }        // 2.5x
    if rarity == "legendary" { return 50; }   // 5.0x
    return 10;
}

// Calculate weight variance (75-125% = ±25%)
fn calculate_weight_variance() {
    return random_int(75, 125);
}

// Get size descriptor based on variance percentage
fn get_size_descriptor(variance_pct) {
    if variance_pct <= 80 { return "tiny "; }
    if variance_pct <= 90 { return "small "; }
    if variance_pct <= 110 { return ""; }  // average - no descriptor
    if variance_pct <= 120 { return "large "; }
    return "huge ";
}

// Get size description for item long_desc
fn get_size_description(variance_pct) {
    if variance_pct <= 80 { return "This is a tiny specimen."; }
    if variance_pct <= 90 { return "This is a small specimen."; }
    if variance_pct <= 110 { return ""; }  // average - no extra text
    if variance_pct <= 120 { return "This is a large specimen."; }
    return "This is a huge specimen!";
}

// Try to consume bait with a given chance (1-100), returns message if consumed
fn try_consume_bait(bait_id, chance, message_template) {
    if bait_id == "" { return ""; }
    if random_int(1, 100) > chance { return ""; }

    let bait = get_item_data(bait_id);
    if bait == () { return ""; }

    if consume_bait(bait_id) {
        return message_template.replace("{bait}", bait.name);
    }
    return "";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check if fishing
    if !is_fishing(connection_id) {
        send_client_message(connection_id, "You're not fishing.");
        return;
    }

    // Check stamina (god mode bypasses)
    if !char.god_mode && char.stamina < 1 {
        send_client_message(connection_id, "You are too exhausted to reel in your line.");
        return;
    }

    // Get fishing state info
    let bite_time = get_fishing_bite_time(connection_id);
    let now = get_current_time();
    let room_id = get_fishing_room(connection_id);
    let bait_id = get_fishing_bait(connection_id);
    let rod_id = get_fishing_rod(connection_id);

    // Check if player moved rooms (fishing should be canceled)
    if room_id != char.current_room_id {
        cancel_fishing(connection_id);
        send_client_message(connection_id, "Your fishing line snapped when you moved away from the water!");
        return;
    }

    // Spend base stamina (1 point + heat/thirst penalty for reeling attempt) - god mode bypasses
    if !char.god_mode {
        let heat_penalty = get_heat_stamina_penalty(connection_id);
        let thirst_penalty = get_thirst_stamina_penalty(connection_id);
        char.stamina = char.stamina - 1 - heat_penalty - thirst_penalty;
        if char.stamina < 0 { char.stamina = 0; }
        save_character_data(char);
        set_player_character(connection_id, char);

        // Show warning messages for penalties
        if heat_penalty > 0 {
            send_client_message(connection_id, "The heat is draining.");
        }
        if thirst_penalty > 0 {
            send_client_message(connection_id, "Your thirst is sapping your strength.");
        }
    }

    // Check if bite hasn't happened yet (early reel)
    if now < bite_time {
        cancel_fishing(connection_id);

        // Small chance to lose bait when reeling early (15%)
        let bait_msg = try_consume_bait(bait_id, 15, "Your {bait} slips off the hook as you reel in.");
        if bait_msg != "" {
            send_client_message(connection_id, bait_msg);
        }

        send_client_message(connection_id, "You reel in your line. Nothing this time.");
        broadcast_to_room(char.current_room_id, char.name + " reels in their fishing line.", char.name);
        return;
    }

    // Bite has happened - calculate bite chance
    let skill_level = get_skill_level(char.name, "fishing");

    // Get rod and bait quality
    let rod = get_item_data(rod_id);
    let rod_quality = 0;
    if rod != () {
        rod_quality = rod.quality;
    }

    let bait_quality = 0;
    let bait = ();
    if bait_id != "" {
        bait = get_item_data(bait_id);
        if bait != () {
            bait_quality = bait.quality;
        }
    }

    // Bite chance formula
    let water_type = get_room_water_type(char.current_room_id);
    let water_bonus = 0;
    if water_type == "saltwater" { water_bonus = 5; }
    if water_type == "magical" { water_bonus = 15; }

    let bite_chance = 40 + (skill_level * 4) + bait_quality + water_bonus;
    if bite_chance < 10 { bite_chance = 10; }
    if bite_chance > 95 { bite_chance = 95; }

    let bite_roll = random_int(1, 100);

    if bite_roll > bite_chance {
        // No bite
        cancel_fishing(connection_id);

        // Medium chance to lose bait when nothing bites (25%)
        let bait_msg = try_consume_bait(bait_id, 25, "Your {bait} was nibbled away by something.");
        if bait_msg != "" {
            send_client_message(connection_id, bait_msg);
        }

        send_client_message(connection_id, "You reel in your line, but nothing took the bait.");
        broadcast_to_room(char.current_room_id, char.name + " reels in an empty line.", char.name);
        return;
    }

    // Something bit! Now consume bait
    if bait != () {
        if !consume_bait(bait_id) {
            // Bait was consumed (all uses gone) - let player know
            send_client_message(connection_id, "Your " + bait.name + " has been used up.");
        }
    }

    // Select what was caught
    let catch_result = select_catch(char.current_room_id, skill_level);

    if catch_result == () {
        // No valid catch (shouldn't happen normally)
        cancel_fishing(connection_id);
        send_client_message(connection_id, "Something tugged your line but got away!");
        broadcast_to_room(char.current_room_id, char.name + "'s catch got away!", char.name);
        return;
    }

    let catch_vnum = catch_result.vnum;
    let catch_rarity = catch_result.rarity;

    // Catch chance based on rod quality and rarity
    let rarity_penalty = 0;
    if catch_rarity == "uncommon" { rarity_penalty = 10; }
    if catch_rarity == "rare" { rarity_penalty = 20; }
    if catch_rarity == "legendary" { rarity_penalty = 35; }

    let catch_chance = 50 + (skill_level * 5) + rod_quality - rarity_penalty;
    if catch_chance < 5 { catch_chance = 5; }
    if catch_chance > 98 { catch_chance = 98; }

    let catch_roll = random_int(1, 100);

    if catch_roll > catch_chance {
        // Fish got away
        cancel_fishing(connection_id);

        // High chance to lose bait when fish escapes (50%)
        let bait_msg = try_consume_bait(bait_id, 50, "The fish stole your {bait} as it escaped!");
        if bait_msg != "" {
            send_client_message(connection_id, bait_msg);
        }

        let msg = "You feel a tug, but the fish slips off the hook!";
        if catch_rarity == "rare" || catch_rarity == "legendary" {
            msg = "You feel a powerful tug, but the fish breaks free!";
        }
        send_client_message(connection_id, msg);
        broadcast_to_room(char.current_room_id, char.name + " struggles with something on the line, but it gets away.", char.name);
        return;
    }

    // Success! Spawn the catch
    let caught_item = spawn_item_to_inventory(catch_vnum, char.name);

    if caught_item == () {
        // Failed to spawn (vnum doesn't exist)
        cancel_fishing(connection_id);
        send_client_message(connection_id, "You caught something, but it wriggled away! (Item vnum '" + catch_vnum + "' not found)");
        return;
    }

    // Apply weight variance (±25%) - each fish has a random size
    let variance_pct = calculate_weight_variance();
    let base_weight = caught_item.weight;
    let new_weight = (base_weight * variance_pct) / 100;
    if new_weight < 1 { new_weight = 1; }
    set_item_weight(caught_item.id, new_weight);

    // Apply same variance to value
    let base_value = caught_item.value;
    let new_value = (base_value * variance_pct) / 100;
    set_item_value(caught_item.id, new_value);

    // Apply variance to nutrition if it's food
    if caught_item.is_food {
        let base_nutrition = caught_item.food_nutrition;
        let new_nutrition = (base_nutrition * variance_pct) / 100;
        if new_nutrition < 1 { new_nutrition = 1; }
        set_food_properties(caught_item.id, new_nutrition, caught_item.food_spoil_duration);
    }

    // Get size descriptor for messages
    let size_desc = get_size_descriptor(variance_pct);

    // Update long_desc to include size if not average
    let size_text = get_size_description(variance_pct);
    if size_text != "" {
        let new_long_desc = size_text + " " + caught_item.long_desc;
        set_item_long_desc(caught_item.id, new_long_desc);
    }

    // Re-fetch item to get updated weight for stamina calculation
    caught_item = get_item_data(caught_item.id);

    // Clear fishing state
    cancel_fishing(connection_id);

    // Extra stamina cost based on catch weight (now uses varied weight) - god mode bypasses
    if !char.god_mode {
        let extra_stamina = caught_item.weight / 5;
        if extra_stamina > 0 {
            // Refresh char data and spend extra stamina
            char = get_player_character(connection_id);
            char.stamina = char.stamina - extra_stamina;
            if char.stamina < 0 { char.stamina = 0; }
            save_character_data(char);
            set_player_character(connection_id, char);
        }
    }

    // Calculate and award XP
    let base_xp = 15;
    let rarity_mult = get_rarity_xp_multiplier(catch_rarity);
    let skill_diminish = 10 - (skill_level / 2); // 10, 9.5, 9, ... down to 5
    if skill_diminish < 5 { skill_diminish = 5; }

    let xp_gained = (base_xp * rarity_mult * skill_diminish) / 100;
    if xp_gained < 1 { xp_gained = 1; }

    let leveled_up = add_skill_experience(char.name, "fishing", xp_gained);

    // Refresh session with updated character data (including new skills)
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    // Send success messages
    let rarity_prefix = "";
    if catch_rarity == "uncommon" { rarity_prefix = "a nice "; }
    if catch_rarity == "rare" { rarity_prefix = "an impressive "; }
    if catch_rarity == "legendary" { rarity_prefix = "an incredible "; }

    send_client_message(connection_id, "You caught " + size_desc + rarity_prefix + caught_item.name + "! (+" + xp_gained + " Fishing XP)");
    broadcast_to_room(char.current_room_id, char.name + " reels in " + size_desc + rarity_prefix + caught_item.name + "!", char.name);

    if leveled_up {
        let new_level = get_skill_level(char.name, "fishing");
        let level_msg = get_level_up_message(new_level);
        send_client_message(connection_id, "\n*** Your Fishing skill increased to level " + new_level + "! ***");
        if level_msg != "" {
            send_client_message(connection_id, level_msg);
        }
    }
}
