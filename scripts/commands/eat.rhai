// scripts/commands/eat.rhai
// eat <food> - Eat a food item

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Eat what?");
        return;
    }

    let keyword = args.to_lower();

    // Look in inventory only - you can only eat what you're carrying
    let item = find_item_in_inventory(char.name, keyword);

    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    if !item.is_food {
        send_client_message(connection_id, "You can't eat that.");
        return;
    }

    // Fire on_use trigger - can cancel the eat
    let trigger_result = fire_item_trigger(item.id, "on_use", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "use_type": "eat"
    });
    if trigger_result == "cancel" {
        return;
    }

    let food_name = item.name;

    // Check if spoiled
    if item.is_spoiled {
        send_client_message(connection_id, "Ugh! " + food_name + " has gone bad!");
        // Still consume it, but with bad effects
        consume_food(item.id);
        send_client_message(connection_id, "You feel sick from eating spoiled food.");
        broadcast_to_room_except(char.current_room_id, connection_id, char.name + " eats something rotten and looks ill.");
        // Apply food sickness
        char.food_sick = true;
        char.has_illness = true;
        char.illness_progress = 50;
        save_character_data(char);
        set_player_character(connection_id, char);
        return;
    }

    // Warn if food is nearly spoiled
    if item.food_spoilage_points >= 0.75 {
        send_client_message(connection_id, "This food smells a bit off...");
    }

    // Eat the food
    send_client_message(connection_id, "You eat " + food_name + ".");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " eats " + food_name + ".");

    // Check for poisoned food
    if item.food_poisoned {
        send_client_message(connection_id, "You feel a burning sensation! The food was poisoned!");
    }

    // Apply nutrition to hunger
    let nutrition = item.food_nutrition;
    if nutrition > 0 {
        restore_hunger(connection_id, nutrition);
        apply_nutrition_hp(connection_id, nutrition);
    }

    // Apply effects
    let effects = item.food_effects;
    let source = food_name;
    for effect in effects {
        apply_effect(char, effect, connection_id, source);
    }

    // Remove the food item
    consume_food(item.id);
}

fn apply_effect(char, effect, connection_id, source) {
    let etype = effect.effect_type;
    let magnitude = effect.magnitude;
    let duration = effect.duration;

    // Instant effects
    if etype == "heal" { return apply_heal(char, magnitude, connection_id); }
    if etype == "poison" { return apply_poison(char, magnitude, duration, connection_id); }
    if etype == "stamina_restore" { return apply_stamina(char, magnitude, connection_id); }
    if etype == "mana_restore" { return apply_mana(magnitude, connection_id); }
    if etype == "quenched" { return apply_quenched(magnitude, connection_id); }
    if etype == "satiated" { return apply_satiated(magnitude, connection_id); }
    if etype == "drunk" { return apply_drunk(char.name, magnitude, connection_id); }
    // Buff effects (timed)
    return apply_buff_effect(char.name, etype, magnitude, duration, source, connection_id);
}

fn apply_heal(char, magnitude, connection_id) {
    let old_hp = char.hp;
    char.hp = char.hp + magnitude;
    if char.hp > char.max_hp { char.hp = char.max_hp; }
    let healed = char.hp - old_hp;
    if healed > 0 {
        save_character_data(char);
        set_player_character(connection_id, char);
        send_client_message(connection_id, "You feel refreshed. (" + healed + " HP restored)");
    } else {
        send_client_message(connection_id, "You feel refreshed.");
    }
}

fn apply_poison(char, magnitude, duration, connection_id) {
    // magnitude = damage-per-round; duration = number of rounds (defaults to 3 if unset).
    let rounds = duration;
    if rounds <= 0 { rounds = 3; }
    apply_ongoing_effect(char.name, "poison", rounds, magnitude, "torso");
    send_client_message(connection_id, "\x1b[1;31mYou feel sick as the poison takes hold!\x1b[0m");
}

fn apply_stamina(char, magnitude, connection_id) {
    char.stamina = char.stamina + magnitude;
    if char.stamina > char.max_stamina { char.stamina = char.max_stamina; }
    save_character_data(char);
    set_player_character(connection_id, char);
    send_client_message(connection_id, "You feel energized!");
}

fn apply_mana(magnitude, connection_id) {
    let result = restore_mana(connection_id, magnitude);
    if result {
        send_client_message(connection_id, "Your magical energy is restored. (+" + magnitude + " mana)");
    }
}

fn apply_quenched(magnitude, connection_id) {
    restore_thirst(connection_id, magnitude);
    if magnitude >= 80 {
        send_client_message(connection_id, "Your thirst is quenched.");
    }
}

fn apply_satiated(magnitude, connection_id) {
    restore_hunger(connection_id, magnitude);
    send_client_message(connection_id, "You feel a bit fuller.");
}

fn apply_drunk(char_name, magnitude, connection_id) {
    let new_level = add_drunk(char_name, magnitude);
    if new_level >= 80 {
        send_client_message(connection_id, "The world spins around you... *hic*");
    } else if new_level >= 50 {
        send_client_message(connection_id, "You feel quite drunk... *hic*");
    } else if new_level >= 30 {
        send_client_message(connection_id, "You feel a pleasant buzz.");
    } else {
        send_client_message(connection_id, "You feel a little woozy...");
    }
}

fn apply_buff_effect(char_name, etype, magnitude, duration, source, connection_id) {
    if duration <= 0 { duration = 300; } // Default 5 minutes
    apply_buff(char_name, etype, magnitude, duration, source);
    let msg = get_buff_message(etype);
    if msg != "" {
        send_client_message(connection_id, msg);
    }
}

fn get_buff_message(etype) {
    if etype == "strength_boost" { return "You feel stronger!"; }
    if etype == "dexterity_boost" { return "You feel more agile!"; }
    if etype == "constitution_boost" { return "You feel more resilient!"; }
    if etype == "intelligence_boost" { return "Your mind feels sharper!"; }
    return get_buff_message_2(etype);
}

fn get_buff_message_2(etype) {
    if etype == "wisdom_boost" { return "You feel more perceptive!"; }
    if etype == "charisma_boost" { return "You feel more charming!"; }
    if etype == "haste" { return "You feel faster!"; }
    if etype == "slow" { return "Your movements feel sluggish..."; }
    return get_buff_message_3(etype);
}

fn get_buff_message_3(etype) {
    if etype == "invisibility" { return "You fade from view."; }
    if etype == "detect_invisible" { return "Your eyes tingle with enhanced sight."; }
    if etype == "regeneration" { return "You feel your wounds beginning to heal."; }
    return "";
}

fn apply_nutrition_hp(connection_id, nutrition) {
    let hp_boost = nutrition / 5;
    if hp_boost <= 0 {
        send_client_message(connection_id, "That was satisfying.");
        return;
    }
    let char2 = get_player_character(connection_id);
    let old_hp = char2.hp;
    char2.hp = char2.hp + hp_boost;
    if char2.hp > char2.max_hp {
        char2.hp = char2.max_hp;
    }
    let healed = char2.hp - old_hp;
    save_character_data(char2);
    set_player_character(connection_id, char2);
    if healed > 0 {
        send_client_message(connection_id, "That was a satisfying meal. (" + healed + " HP restored)");
    } else {
        send_client_message(connection_id, "That was a satisfying meal.");
    }
}
