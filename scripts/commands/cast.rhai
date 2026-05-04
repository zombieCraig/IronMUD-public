// cast.rhai - Cast a spell
// Usage: cast <spell> [target]

fn resolve_spell_id(input, char_name) {
    let available = get_available_spells(char_name);
    let lower = input.to_lower();

    // Exact match first
    for spell_id in available {
        if spell_id == lower {
            return spell_id;
        }
    }

    // Match by spell name
    for spell_id in available {
        let info = get_spell_info(spell_id);
        if info.len() > 0 {
            if info["name"].to_lower() == lower {
                return spell_id;
            }
        }
    }

    // Partial match on id
    for spell_id in available {
        if spell_id.starts_with(lower) {
            return spell_id;
        }
    }

    // Partial match on name
    for spell_id in available {
        let info = get_spell_info(spell_id);
        if info.len() > 0 {
            if info["name"].to_lower().starts_with(lower) {
                return spell_id;
            }
        }
    }

    // Multi-word partial match (e.g., "magic m" matches "magic_missile")
    let words = lower.split(' ');
    let joined = "";
    for w in words {
        if joined == "" {
            joined = w;
        } else {
            joined = joined + "_" + w;
        }
    }
    for spell_id in available {
        if spell_id.starts_with(joined) {
            return spell_id;
        }
    }

    return "";
}

fn find_mob_target(char, target_arg, room_id) {
    let mobs = get_mobiles_in_room(room_id);

    // If no target arg and in combat, use primary target
    if target_arg == "" {
        if is_in_combat(char.name) {
            let targets = get_combat_targets(char.name);
            if targets.len() > 0 {
                return targets[0];
            }
        }
        return "";
    }

    let lower = target_arg.to_lower();
    for mob_id in mobs {
        let mob = get_mobile_data(mob_id);
        if type_of(mob) != "()" {
            if mob.name.to_lower().contains(lower) {
                return mob_id;
            }
            for kw in mob.keywords {
                if kw.to_lower().starts_with(lower) {
                    return mob_id;
                }
            }
        }
    }
    return "";
}

fn find_player_target(char, target_arg, room_id) {
    if target_arg == "" || target_arg.to_lower() == "self" || target_arg.to_lower() == char.name.to_lower() {
        return char.name;
    }

    let players = get_characters_in_room(room_id);
    let lower = target_arg.to_lower();
    for p in players {
        if p.name.to_lower().starts_with(lower) {
            return p.name;
        }
    }
    return "";
}

fn cast_damage(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;
    let mob_id = find_mob_target(char, target_arg, room_id);

    if mob_id == "" {
        send_client_message(connection_id, "Cast at whom?");
        return false;
    }

    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "That target is no longer here.");
        return false;
    }

    if mob.flags.no_attack {
        send_client_message(connection_id, "You cannot attack " + mob.name + ".");
        return false;
    }

    // Calculate damage
    let magic_level = get_skill_level(char.name, "magic");
    let effective_int = get_effective_stat(char.name, "int");
    let int_bonus = if effective_int > 10 { effective_int - 10 } else { 0 };

    let damage = spell["damage_base"] + (spell["damage_per_skill"] * magic_level) + (spell["damage_int_scaling"] * int_bonus);

    // Apply spell damage trait modifier
    let spell_dmg_mod = get_trait_effect_sum(char.name, "spell_damage_bonus");
    if spell_dmg_mod != 0 {
        damage = damage * (100 + spell_dmg_mod) / 100;
    }
    if damage < 1 { damage = 1; }

    // Apply damage
    mob.hp -= damage;
    save_mobile_data(mob);

    // Start combat if not already
    if !is_in_combat(char.name) {
        enter_combat(char.name, mob_id, "mobile");
    }
    enter_mobile_combat(mob_id, "player", char.name);

    // Messages
    let dtype = spell["damage_type"];
    let verb = get_damage_verb(dtype);

    send_client_message(connection_id, "You cast " + spell["name"] + "! Your " + dtype + " energy " + verb + " " + mob.name + " for " + damage + " damage!");
    broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + "! " + dtype.to_upper() + " energy " + verb + " " + mob.name + "!", char.name);

    return true;
}

fn get_damage_verb(dtype) {
    if dtype == "arcane" { return "strikes"; }
    if dtype == "fire" { return "scorches"; }
    if dtype == "lightning" { return "shocks"; }
    return "hits";
}

fn cast_buff(connection_id, char, spell, target_arg) {
    let target_name = "";
    let target_type = spell["target_type"];

    if target_type == "self" {
        target_name = char.name;
    } else {
        target_name = find_player_target(char, target_arg, char.current_room_id);
        if target_name == "" {
            send_client_message(connection_id, "Cast on whom?");
            return false;
        }
    }

    let effect = spell["buff_effect"];
    let magnitude = spell["buff_magnitude"];
    let duration = spell["buff_duration_secs"];

    apply_buff(target_name, effect, magnitude, duration, spell["name"]);

    if target_name == char.name {
        send_client_message(connection_id, "You cast " + spell["name"] + " on yourself.");
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + ".", char.name);
    } else {
        send_client_message(connection_id, "You cast " + spell["name"] + " on " + target_name + ".");
        // Notify target
        let target_conn = find_player_connection_by_name(target_name);
        if target_conn != "" {
            send_client_message(target_conn, char.name + " casts " + spell["name"] + " on you.");
        }
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + " on " + target_name + ".", char.name);
    }

    return true;
}

fn cast_heal(connection_id, char, spell, target_arg) {
    let target_name = find_player_target(char, target_arg, char.current_room_id);
    if target_name == "" {
        send_client_message(connection_id, "Cast on whom?");
        return false;
    }

    // Calculate heal amount
    let magic_level = get_skill_level(char.name, "magic");
    let effective_int = get_effective_stat(char.name, "int");
    let int_bonus = if effective_int > 10 { effective_int - 10 } else { 0 };

    let heal_amount = spell["heal_base"] + (spell["heal_per_skill"] * magic_level) + (spell["heal_int_scaling"] * int_bonus);
    if heal_amount < 1 { heal_amount = 1; }

    // Load target character and heal
    let target = get_character_data(target_name);
    if type_of(target) == "()" {
        send_client_message(connection_id, "That person is not here.");
        return false;
    }

    let old_hp = target.hp;
    target.hp += heal_amount;
    if target.hp > target.max_hp {
        target.hp = target.max_hp;
    }
    let actual_heal = target.hp - old_hp;
    save_character_data(target);

    // Sync target session
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn != "" {
        let updated = get_character_data(target_name);
        if type_of(updated) != "()" {
            set_player_character(target_conn, updated);
        }
    }

    if target_name == char.name {
        send_client_message(connection_id, "You cast " + spell["name"] + " and heal yourself for " + actual_heal + " HP.");
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + " and is bathed in healing light.", char.name);
    } else {
        send_client_message(connection_id, "You cast " + spell["name"] + " on " + target_name + ", healing them for " + actual_heal + " HP.");
        if target_conn != "" {
            send_client_message(target_conn, char.name + " casts " + spell["name"] + " on you, healing you for " + actual_heal + " HP.");
        }
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + " on " + target_name + ".", char.name);
    }

    return true;
}

fn cast_utility(connection_id, char, spell, target_arg) {
    let spell_id = spell["id"];

    if spell_id == "light" {
        return cast_light(connection_id, char, spell);
    }
    if spell_id == "dispel_magic" {
        return cast_dispel(connection_id, char, spell, target_arg);
    }

    send_client_message(connection_id, "You don't know how to cast that utility spell.");
    return false;
}

fn cast_light(connection_id, char, spell) {
    apply_buff(char.name, "magic_light", 1, spell["buff_duration_secs"], "Light");
    send_client_message(connection_id, "You cast Light! A warm glow surrounds you, illuminating your path.");
    broadcast_to_room_except(char.current_room_id, char.name + " casts Light, and a warm glow surrounds them.", char.name);
    return true;
}

fn cast_dispel(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;
    let mob_id = find_mob_target(char, target_arg, room_id);

    if mob_id == "" {
        send_client_message(connection_id, "Dispel magic on whom?");
        return false;
    }

    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "That target is no longer here.");
        return false;
    }

    send_client_message(connection_id, "You cast Dispel Magic on " + mob.name + "! A wave of anti-magic washes over them.");
    broadcast_to_room_except(room_id, char.name + " casts Dispel Magic on " + mob.name + "!", char.name);
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if type_of(char) == "()" { return; }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't cast spells in your sleep!");
        return;
    }

    // ROOM_NOMAGIC: suppress spellcasting from this room. god_mode bypasses.
    if !char.god_mode {
        let here = get_room_data(char.current_room_id);
        if here != () && here.flags.no_magic {
            send_client_message(connection_id, "Magic will not function here.");
            return;
        }
    }

    if args == "" {
        send_client_message(connection_id, "Cast what spell? Try 'spells' to see available spells.");
        return;
    }

    // Check magic skill
    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < 1 {
        send_client_message(connection_id, "You have no training in magic.");
        return;
    }

    // Parse spell name and optional target
    // Format: cast <spell_name> [target]
    // Spell names can be multi-word, so we try longest match first
    let spell_id = "";
    let target_arg = "";

    // Try the full args as spell name first
    spell_id = resolve_spell_id(args, char.name);

    // If no match, try splitting off last word as target
    if spell_id == "" {
        let parts = args.split(' ');
        if parts.len() >= 2 {
            // Try all but last word as spell, last as target
            let spell_words = [];
            let i = 0;
            for p in parts {
                if i < parts.len() - 1 {
                    spell_words.push(p);
                } else {
                    target_arg = p;
                }
                i += 1;
            }
            let spell_str = "";
            for w in spell_words {
                if spell_str == "" {
                    spell_str = w;
                } else {
                    spell_str = spell_str + " " + w;
                }
            }
            spell_id = resolve_spell_id(spell_str, char.name);
        }
    }

    if spell_id == "" {
        send_client_message(connection_id, "You don't know that spell. Try 'spells' to see available spells.");
        return;
    }

    let spell = get_spell_info(spell_id);
    if spell.len() == 0 {
        send_client_message(connection_id, "Spell data not found.");
        return;
    }

    // Calculate effective mana cost with trait modifiers
    let cost_reduction = get_trait_effect_sum(char.name, "spell_cost_reduction");
    let cost_increase = get_trait_effect_sum(char.name, "spell_cost_increase");
    let effective_mana_cost = spell["mana_cost"] * (100 - cost_reduction + cost_increase) / 100;
    if effective_mana_cost < 1 { effective_mana_cost = 1; }

    // Validate mana
    if char.mana < effective_mana_cost {
        send_client_message(connection_id, "You don't have enough mana. (Need " + effective_mana_cost + ", have " + char.mana + ")");
        return;
    }

    // Validate cooldown
    let cd_remaining = get_spell_cooldown_remaining(char.name, spell_id);
    if cd_remaining > 0 {
        send_client_message(connection_id, spell["name"] + " is on cooldown for " + cd_remaining + " more seconds.");
        return;
    }

    // Validate combat requirement
    if spell["requires_combat"] && !is_in_combat(char.name) {
        send_client_message(connection_id, "You must be in combat to cast " + spell["name"] + ".");
        return;
    }

    // Validate reagent
    let reagent = spell["reagent_vnum"];
    if reagent != "" {
        if !has_reagent(char.name, reagent) {
            send_client_message(connection_id, "You lack the required reagent to cast " + spell["name"] + ".");
            return;
        }
    }

    // Dispatch by spell type
    let success = false;
    let stype = spell["spell_type"];

    if stype == "damage" {
        success = cast_damage(connection_id, char, spell, target_arg);
    }
    if stype == "buff" {
        success = cast_buff(connection_id, char, spell, target_arg);
    }
    if stype == "heal" {
        success = cast_heal(connection_id, char, spell, target_arg);
    }
    if stype == "utility" {
        success = cast_utility(connection_id, char, spell, target_arg);
    }

    if !success {
        return;
    }

    // Deduct mana (use effective cost with trait modifiers)
    let updated_char = get_player_character(connection_id);
    if type_of(updated_char) != "()" {
        updated_char.mana -= effective_mana_cost;
        if updated_char.mana < 0 { updated_char.mana = 0; }
        save_character_data(updated_char);
        set_player_character(connection_id, get_character_data(char.name));
    }

    // Set cooldown (with trait modifier)
    let cd_reduction = get_trait_effect_sum(char.name, "spell_cooldown_reduction");
    let effective_cd = spell["cooldown_secs"] * (100 - cd_reduction) / 100;
    if effective_cd > 0 {
        set_spell_cooldown(char.name, spell_id, effective_cd);
    }

    // Consume reagent
    if reagent != "" {
        consume_reagent(char.name, reagent);
    }

    // Award magic XP
    let xp = spell["xp_award"];
    let old_level = get_skill_level(char.name, "magic");
    add_skill_experience(char.name, "magic", xp);
    let new_level = get_skill_level(char.name, "magic");

    if new_level > old_level {
        send_client_message(connection_id, "\x1b[1;33mYour magic skill has improved to level " + new_level + "!\x1b[0m");
        let new_max = recalculate_max_mana(char.name);
        send_client_message(connection_id, "\x1b[1;36mYour maximum mana has increased to " + new_max + ".\x1b[0m");

        // Refresh character to session
        let refreshed = get_character_data(char.name);
        if type_of(refreshed) != "()" {
            set_player_character(connection_id, refreshed);
        }
    }
}
