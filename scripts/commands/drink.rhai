// scripts/commands/drink.rhai
// drink <container> - Drink from a liquid container

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Drink from what?");
        return;
    }

    let keyword = args.to_lower();

    // Look in inventory first
    let item = find_item_in_inventory(char.name, keyword);

    // Then look in room
    if item == () {
        item = find_item_in_room(char.current_room_id, keyword);
    }

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !item.is_liquid_container {
        send_client_message(connection_id, "You can't drink from that.");
        return;
    }

    if item.liquid_current <= 0 {
        send_client_message(connection_id, "It's empty.");
        return;
    }

    // Fire on_use trigger - can cancel the drink
    let trigger_result = fire_item_trigger(item.id, "on_use", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "use_type": "drink"
    });
    if trigger_result == "cancel" {
        return;
    }

    // Take a drink (1 sip)
    let sips = drink_from(item.id, 1);

    let liquid_name = item.liquid_type;
    send_client_message(connection_id, "You drink " + liquid_name + " from " + item.name + ".");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " drinks from " + item.name + ".");

    // Check for poisoned drink
    if item.liquid_poisoned {
        send_client_message(connection_id, "You feel a burning sensation! The drink was poisoned!");
    }

    // Apply effects
    let effects = item.liquid_effects;
    let source = liquid_name;
    if effects.len() == 0 {
        // No effects configured - apply default quench for any drink
        apply_quenched(25, connection_id);
    }
    for effect in effects {
        apply_effect(char, effect, connection_id, source);
    }

    // Check if now empty
    let updated = get_item_data(item.id);
    if updated != () && updated.liquid_current <= 0 {
        send_client_message(connection_id, item.name + " is now empty.");
    }
}

fn apply_effect(char, effect, connection_id, source) {
    let etype = effect.effect_type;
    let magnitude = effect.magnitude;
    let duration = effect.duration;

    // Instant effects
    if etype == "heal" { return apply_heal(char, magnitude, connection_id); }
    if etype == "poison" { return apply_poison(char, magnitude, connection_id); }
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

fn apply_poison(char, magnitude, connection_id) {
    char.hp = char.hp - magnitude;
    if char.hp < 0 { char.hp = 0; }
    save_character_data(char);
    set_player_character(connection_id, char);
    send_client_message(connection_id, "\x1b[1;31mYou feel sick! (" + magnitude + " poison damage)\x1b[0m");
    if char.hp <= 0 {
        send_client_message(connection_id, "\x1b[1;31mThe poison overwhelms you!\x1b[0m");
    }
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
        send_client_message(connection_id, "Your thirst is fully quenched.");
    } else if magnitude >= 40 {
        send_client_message(connection_id, "That was refreshing.");
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
