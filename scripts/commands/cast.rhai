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
    let mobs = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);

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

// Award per-spell XP. `full` = true on a landed cast, false on a resisted /
// no-op cast (25% of base xp_award). Surfaces level-up and evolution messages
// to the caster. The unified `magic` skill XP is still awarded separately by
// run_command after dispatch.
fn award_per_spell_xp(connection_id, char_name, spell_id, full) {
    let info = get_spell_info(spell_id);
    if info.len() == 0 { return; }
    let base = info["xp_award"];
    let xp = if full { base } else { base / 4 };
    if xp < 1 { xp = 1; }
    let result = add_spell_experience(char_name, spell_id, xp);
    if result["evolved"] {
        let new_info = get_spell_info(result["new_spell_id"]);
        let new_name = if new_info.len() > 0 { new_info["name"] } else { result["new_spell_id"] };
        send_client_message(connection_id,
            "\x1b[1;35mYour mastery of " + info["name"] +
            " has transcended — it has evolved into " + new_name + "!\x1b[0m");
    } else if result["leveled_up"] {
        send_client_message(connection_id,
            "\x1b[1;36mYour mastery of " + info["name"] +
            " has reached level " + result["new_level"] + "!\x1b[0m");
    }
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

    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your magic here.");
        return false;
    }

    // Calculate damage
    let magic_level = get_skill_level(char.name, "magic");
    let spell_level = get_spell_level(char.name, spell["id"]);
    let effective_int = get_effective_stat(char.name, "int");
    let int_bonus = if effective_int > 10 { effective_int - 10 } else { 0 };

    let damage = spell["damage_base"]
        + (spell["damage_per_skill"] * magic_level)
        + (spell["damage_per_spell_level"] * spell_level)
        + (spell["damage_int_scaling"] * int_bonus);

    // Apply spell damage trait modifier
    let spell_dmg_mod = get_trait_effect_sum(char.name, "spell_damage_bonus");
    if spell_dmg_mod != 0 {
        damage = damage * (100 + spell_dmg_mod) / 100;
    }
    if damage < 1 { damage = 1; }

    // Apply damage
    mob.hp -= damage;
    save_mobile_data(mob);
    // Slice 3c: attribute damage so handle_mob_kill credits this caster
    // when the combat tick eventually finishes off the mob.
    record_mob_damaged_by(mob_id, char.name, damage);

    // Start combat if not already
    if !is_in_combat(char.name) {
        enter_combat(char.name, "mobile", mob_id);
    }
    enter_mobile_combat(mob_id, "player", char.name);

    // Messages
    let dtype = spell["damage_type"];
    let verb = get_damage_verb(dtype);

    send_client_message(connection_id, "You cast " + spell["name"] + "! Your " + dtype + " energy " + verb + " " + mob.name + " for " + damage + " damage!");
    broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + "! " + dtype.to_upper() + " energy " + verb + " " + mob.name + "!", char.name);

    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_debuff(connection_id, char, spell, target_arg) {
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

    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your magic here.");
        return false;
    }

    let effect = spell["buff_effect"];

    // Hard-immunity gate: certain MOB_NO* flags fully resist matching debuffs.
    let resisted = false;
    if effect == "sleep" && mob.flags.no_sleep { resisted = true; }
    if effect == "blind" && mob.flags.no_blind { resisted = true; }

    // Initiate combat regardless of resist (cast was hostile and committed).
    if !is_in_combat(char.name) {
        enter_combat(char.name, "mobile", mob_id);
    }
    enter_mobile_combat(mob_id, "player", char.name);

    if resisted {
        send_client_message(connection_id, "You cast " + spell["name"] + " on " + mob.name + ", but " + mob.name + " resists your magic!");
        broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " on " + mob.name + ", but it has no effect.", char.name);
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    // Graded resistance — equip-stamped status_resistance buffs (and any
    // wildcard "*" buffs) subtract from the apply chance. Base 80%.
    if !roll_status_application(mob_id, effect, 80) {
        send_client_message(connection_id, "You cast " + spell["name"] + " on " + mob.name + ", but " + mob.name + " shrugs off your magic!");
        broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " on " + mob.name + ", but the magic dissipates.", char.name);
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    let spell_level = get_spell_level(char.name, spell["id"]);
    let magnitude = spell["buff_magnitude"] + (spell["buff_magnitude_per_spell_level"] * spell_level);
    let duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);
    apply_buff_to_mobile(mob_id, effect, magnitude, duration, spell["name"]);

    let verb = if effect == "sleep" { "drops " + mob.name + " into a deep slumber!" }
               else if effect == "blind" { "robs " + mob.name + " of sight!" }
               else { "settles over " + mob.name + "." };

    send_client_message(connection_id, "You cast " + spell["name"] + " — your magic " + verb);
    broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " — the magic " + verb, char.name);

    award_per_spell_xp(connection_id, char.name, spell["id"], true);
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
    let spell_level = get_spell_level(char.name, spell["id"]);
    let magnitude = spell["buff_magnitude"] + (spell["buff_magnitude_per_spell_level"] * spell_level);
    let duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);

    // Courage strips any Feared buff inside apply_buff (Rust side) — capture
    // whether the target was afraid so we can narrate the cure.
    let cured_fear = effect == "courage" && has_buff(target_name, "feared");
    apply_buff(target_name, effect, magnitude, duration, spell["name"]);

    if target_name == char.name {
        send_client_message(connection_id, "You cast " + spell["name"] + " on yourself.");
        if cured_fear {
            send_client_message(connection_id, "Courage floods through you; your terror melts away.");
        }
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + ".", char.name);
    } else {
        send_client_message(connection_id, "You cast " + spell["name"] + " on " + target_name + ".");
        // Notify target
        let target_conn = find_player_connection_by_name(target_name);
        if target_conn != "" {
            send_client_message(target_conn, char.name + " casts " + spell["name"] + " on you.");
            if cured_fear {
                send_client_message(target_conn, "Courage floods through you; your terror melts away.");
            }
        }
        broadcast_to_room_except(char.current_room_id, char.name + " casts " + spell["name"] + " on " + target_name + ".", char.name);
    }

    award_per_spell_xp(connection_id, char.name, spell["id"], true);
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
    let spell_level = get_spell_level(char.name, spell["id"]);
    let effective_int = get_effective_stat(char.name, "int");
    let int_bonus = if effective_int > 10 { effective_int - 10 } else { 0 };

    let heal_amount = spell["heal_base"]
        + (spell["heal_per_skill"] * magic_level)
        + (spell["heal_per_spell_level"] * spell_level)
        + (spell["heal_int_scaling"] * int_bonus);
    if heal_amount < 1 { heal_amount = 1; }

    // Load target character and heal
    let target = get_character_data(target_name);
    if type_of(target) == "()" {
        send_client_message(connection_id, "That person is not here.");
        return false;
    }

    // Organic medicine barely takes on a polymer chassis: 25%, min 1.
    let target_is_synth = is_character_synth(target_name);
    if target_is_synth {
        heal_amount = scale_heal_for_synth(heal_amount);
    }

    let old_hp = target.hp;
    target.hp += heal_amount;
    let target_eff_max_hp = get_effective_max_hp(target.name);
    if target.hp > target_eff_max_hp {
        target.hp = target_eff_max_hp;
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

    if target_is_synth {
        let note = "\x1b[90mThe magic slides off the synthetic dermis; only a fraction takes. Repairs would serve better.\x1b[0m";
        if target_name == char.name {
            send_client_message(connection_id, note);
        } else if target_conn != "" {
            send_client_message(target_conn, note);
        }
    }

    award_per_spell_xp(connection_id, char.name, spell["id"], true);
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
    let spell_level = get_spell_level(char.name, spell["id"]);
    let duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);
    apply_buff(char.name, "magic_light", 1, duration, "Light");
    send_client_message(connection_id, "You cast Light! A warm glow surrounds you, illuminating your path.");
    broadcast_to_room_except(char.current_room_id, char.name + " casts Light, and a warm glow surrounds them.", char.name);
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_summon(connection_id, char, spell, target_arg) {
    if target_arg == "" {
        send_client_message(connection_id, "Summon whom?");
        return false;
    }

    let caster_room = char.current_room_id;

    // PC branch first: try to resolve target as a logged-in player.
    let target_conn = find_player_connection_by_name(target_arg);
    if target_conn != "" {
        let target_char = get_player_character(target_conn);
        if type_of(target_char) == "()" {
            send_client_message(connection_id, "You sense no such creature anywhere.");
            return false;
        }
        if target_char.name.to_lower() == char.name.to_lower() {
            send_client_message(connection_id, "You cannot summon yourself.");
            return false;
        }
        if target_char.current_room_id == caster_room {
            send_client_message(connection_id, target_char.name + " is already here.");
            return false;
        }
        if !target_char.summonable {
            send_client_message(connection_id, target_char.name + " is protected from summons.");
            award_per_spell_xp(connection_id, char.name, spell["id"], false);
            return true;
        }

        let old_room_id = target_char.current_room_id;

        if is_in_combat(target_char.name) {
            exit_combat(target_char.name);
            let mobiles = get_mobiles_in_room(old_room_id);
            for mobile in mobiles {
                exit_mobile_combat(mobile.id);
            }
            send_client_message(target_conn, "Your combat has ended due to being summoned.");
        }

        broadcast_to_room(old_room_id, target_char.name + " vanishes in a swirl of mist!", target_char.name);

        target_char.current_room_id = caster_room;
        save_character_data(target_char);
        set_player_character(target_conn, target_char);

        broadcast_to_room_except(caster_room, connection_id, target_char.name + " arrives, wrenched from elsewhere!");

        send_client_message(target_conn, "You feel a wrenching pull as " + char.name + " summons you!");
        display_room(caster_room, target_conn, target_char.name);

        send_client_message(connection_id, "You wrench " + target_char.name + " to your side!");
        award_per_spell_xp(connection_id, char.name, spell["id"], true);
        return true;
    }

    // Mob branch: world-wide non-prototype lookup.
    let target = find_mobile_by_keyword_anywhere(target_arg);
    if type_of(target) == "()" {
        send_client_message(connection_id, "You sense no such creature anywhere.");
        return false;
    }
    if target.current_room_id == caster_room {
        send_client_message(connection_id, target.name + " is already here.");
        return false;
    }
    if target.flags.no_summon {
        send_client_message(connection_id, target.name + " resists your summons.");
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    let old_room_id = target.current_room_id;

    if is_mobile_in_combat(target.id) {
        exit_mobile_combat(target.id);
    }

    broadcast_to_room(old_room_id, target.short_desc + " vanishes in a swirl of mist!", "");

    target.current_room_id = caster_room;
    save_mobile_data(target);

    broadcast_to_room_except(caster_room, connection_id, target.short_desc + " arrives, wrenched from elsewhere!");
    send_client_message(connection_id, "You wrench " + target.short_desc + " into the room!");
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_charm(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;
    let mob_id = find_mob_target(char, target_arg, room_id);
    if mob_id == "" {
        send_client_message(connection_id, "Charm whom?");
        return false;
    }

    let target = get_mobile_data(mob_id);
    if type_of(target) == "()" {
        send_client_message(connection_id, "Charm whom?");
        return false;
    }

    if target.flags.no_attack {
        send_client_message(connection_id, target.short_desc + " cannot be charmed.");
        return false;
    }

    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your magic here.");
        return false;
    }

    if target.flags.no_charm {
        send_client_message(connection_id, target.short_desc + " resists your charm.");
        broadcast_to_room_except(room_id, connection_id, target.short_desc + " shrugs off " + char.name + "'s charm.");
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    // Graded resistance: equip-stamped status_resistance buffs (and wildcard
    // "*" buffs) subtract from charm landing chance. Base 70%.
    if !roll_status_application(mob_id, "charmed", 70) {
        send_client_message(connection_id, target.short_desc + "'s will resists your charm.");
        broadcast_to_room_except(room_id, connection_id, target.short_desc + " shakes off " + char.name + "'s magic.");
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    let existing_master = mobile_charm_master(mob_id);
    if existing_master != "" && existing_master.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, target.short_desc + " is already under another's sway.");
        return false;
    }

    // Tameable mobs become permanent pets: indefinite buff + pet_owner stamp.
    // Other mobs get the temporary spell-duration charm.
    let is_pet_bond = target.flags.tameable;
    let spell_level = get_spell_level(char.name, spell["id"]);
    let base_duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);
    let duration = if is_pet_bond { -1 } else { base_duration };

    apply_buff_to_mobile(mob_id, "charmed", 0, duration, char.name);
    forget_player(mob_id, char.name);
    if is_mobile_in_combat(mob_id) {
        exit_mobile_combat(mob_id);
    }

    if is_pet_bond {
        set_mobile_pet_owner(mob_id, char.name);
        send_client_message(connection_id, target.short_desc + " bonds with you. " + target.name + " is now your pet.");
        broadcast_to_room_except(room_id, connection_id, target.short_desc + " bonds with " + char.name + ".");
    } else {
        send_client_message(connection_id, target.short_desc + "'s eyes glaze over and fall on you.");
        broadcast_to_room_except(room_id, connection_id, target.short_desc + "'s eyes glaze over.");
    }
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

// Vampire Dominate discipline. Mirrors cast_charm but:
//   - Bypasses MobileFlags.no_charm (kindred mind-control is stronger than mortal magic).
//   - Reads `buff_effect` from spell JSON ("charmed" for command/mesmerize, "dominated" for the dot-5 power).
//   - Skips the tameable-pet bond — domination is a temporary tool, not a permanent companion.
fn cast_dominate(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;
    let mob_id = find_mob_target(char, target_arg, room_id);
    if mob_id == "" {
        send_client_message(connection_id, "Dominate whom?");
        return false;
    }

    let target = get_mobile_data(mob_id);
    if type_of(target) == "()" {
        send_client_message(connection_id, "Dominate whom?");
        return false;
    }

    if target.flags.no_attack {
        send_client_message(connection_id, target.short_desc + " cannot be dominated.");
        return false;
    }

    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your blood here.");
        return false;
    }

    let existing_master = mobile_charm_master(mob_id);
    if existing_master != "" && existing_master.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, target.short_desc + " is already under another's sway.");
        return false;
    }

    let buff_effect = spell["buff_effect"];
    if buff_effect == "" { buff_effect = "dominated"; }
    let spell_level = get_spell_level(char.name, spell["id"]);
    let duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);

    apply_buff_to_mobile(mob_id, buff_effect, 0, duration, char.name);
    forget_player(mob_id, char.name);
    if is_mobile_in_combat(mob_id) {
        exit_mobile_combat(mob_id);
    }

    send_client_message(connection_id, target.short_desc + "'s gaze locks onto yours, will collapsing into yours.");
    broadcast_to_room_except(room_id, connection_id, target.short_desc + "'s eyes go glassy as " + char.name + " takes hold of their mind.");
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

// Fear spell. Tries an in-room mobile first; if no mob matches the keyword,
// falls back to a player target (PvP zones only). Application routes through
// the Rust chokepoint try_apply_fear so immunities (synths, no_fear /
// construct / undead mobs, courage, frenzy holders) and status resistance
// hold uniformly.
fn cast_fear(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;
    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your magic here.");
        return false;
    }

    let spell_level = get_spell_level(char.name, spell["id"]);
    let magnitude = spell["buff_magnitude"] + (spell["buff_magnitude_per_spell_level"] * spell_level);
    let duration = spell["buff_duration_secs"] + (spell["buff_duration_per_spell_level"] * spell_level);

    let mob_id = find_mob_target(char, target_arg, room_id);
    if mob_id != "" {
        return cast_fear_at_mobile(connection_id, char, spell, mob_id, magnitude, duration);
    }
    return cast_fear_at_player(connection_id, char, spell, target_arg, zone, magnitude, duration);
}

fn cast_fear_at_mobile(connection_id, char, spell, mob_id, magnitude, duration) {
    let room_id = char.current_room_id;
    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "That target is no longer here.");
        return false;
    }
    if mob.flags.no_attack {
        send_client_message(connection_id, "You cannot attack " + mob.name + ".");
        return false;
    }

    // Hostile cast commits — combat starts whether or not the terror lands.
    if !is_in_combat(char.name) {
        enter_combat(char.name, "mobile", mob_id);
    }
    enter_mobile_combat(mob_id, "player", char.name);

    let outcome = try_apply_fear(mob_id, 80, magnitude, duration, spell["name"]);
    if outcome == "immune" {
        send_client_message(connection_id, mob.name + " stands unshaken — your terror finds no purchase.");
        broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " on " + mob.name + ", but it has no effect.", char.name);
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }
    if outcome == "resisted" {
        send_client_message(connection_id, mob.name + " shrugs off the creeping dread.");
        broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " on " + mob.name + ", but the magic dissipates.", char.name);
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }
    if outcome != "applied" {
        send_client_message(connection_id, "That target is no longer here.");
        return false;
    }

    send_client_message(connection_id, "You cast " + spell["name"] + " — stark terror floods " + mob.name + "'s eyes!");
    broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " — stark terror floods " + mob.name + "'s eyes!", char.name);
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_fear_at_player(connection_id, char, spell, target_arg, zone, magnitude, duration) {
    let room_id = char.current_room_id;
    if target_arg == "" {
        send_client_message(connection_id, "Cast at whom?");
        return false;
    }

    let keyword = target_arg.to_lower();
    let victim_name = "";
    for room_char in get_characters_in_room(room_id) {
        if room_char.to_lower() == char.name.to_lower() { continue; }
        if room_char.to_lower() == keyword || room_char.to_lower().starts_with(keyword) {
            victim_name = room_char;
            break;
        }
    }
    if victim_name == "" {
        send_client_message(connection_id, "Cast at whom?");
        return false;
    }

    if zone != "pvp" && !char.god_mode {
        send_client_message(connection_id, "You cannot terrify other adventurers here.");
        return false;
    }

    let victim = get_character_data(victim_name);
    if type_of(victim) == "()" {
        send_client_message(connection_id, "Cast at whom?");
        return false;
    }
    if victim.god_mode {
        send_client_message(connection_id, victim.name + " is beyond your terror.");
        return false;
    }

    // Hostile cast commits — mutual PvP engagement (attack.rhai parity).
    enter_combat(char.name, "player", victim.name);
    enter_combat(victim.name, "player", char.name);

    let outcome = try_apply_fear(victim.name, 80, magnitude, duration, spell["name"]);
    let victim_conn = find_player_connection_by_name(victim.name);

    if outcome == "immune" {
        send_client_message(connection_id, victim.name + " stands unshaken — your terror finds no purchase.");
        if victim_conn != "" {
            send_client_message(victim_conn, char.name + " casts " + spell["name"] + " on you, but it finds no purchase. " + char.name + " attacks you!");
        }
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }
    if outcome == "resisted" {
        send_client_message(connection_id, victim.name + " shrugs off the creeping dread.");
        if victim_conn != "" {
            send_client_message(victim_conn, "You shrug off a wave of creeping dread. " + char.name + " attacks you!");
        }
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }
    if outcome != "applied" {
        send_client_message(connection_id, "Cast at whom?");
        return false;
    }

    send_client_message(connection_id, "You cast " + spell["name"] + " — stark terror floods " + victim.name + "'s eyes!");
    if victim_conn != "" {
        send_client_message(victim_conn, "\x1b[1;31mStark, unreasoning terror floods your mind as " + char.name + " attacks you!\x1b[0m");
    }
    broadcast_to_room_except(room_id, char.name + " casts " + spell["name"] + " — " + victim.name + " blanches in terror!", char.name);
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
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
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_animate_dead(connection_id, char, spell, target_arg) {
    let room_id = char.current_room_id;

    if target_arg == "" {
        send_client_message(connection_id, "Animate which corpse?");
        return false;
    }

    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "A peaceful aura suppresses your dark magic here.");
        return false;
    }

    let item = find_item_in_room(room_id, target_arg);
    if type_of(item) == "()" || !item.flags.is_corpse {
        send_client_message(connection_id, "There is no corpse here by that name.");
        return false;
    }

    if item.flags.corpse_is_player {
        send_client_message(connection_id, "You cannot raise the body of a fallen adventurer.");
        return false;
    }

    let source_vnum = item.flags.corpse_source_vnum;
    if source_vnum == "" {
        send_client_message(connection_id, "This corpse is too far gone to animate.");
        return false;
    }

    broadcast_to_room_except(room_id, connection_id, char.name + " traces dark sigils over " + item.name + ", and it shudders...");
    send_client_message(connection_id, "You trace dark sigils over " + item.name + ", and it shudders...");

    // Despawn the corpse (its contents may orphan; that mirrors corpse decay behavior).
    delete_item(item.id);

    // Spawn a fresh instance of the source mob.
    let undead = spawn_mobile_from_prototype(source_vnum);
    if type_of(undead) == "()" {
        send_client_message(connection_id, "The dark magic fizzles — the body refuses to rise.");
        award_per_spell_xp(connection_id, char.name, spell["id"], false);
        return true;
    }

    // Stamp it as undead: weakened HP, shambling description, placed at caster's feet.
    let undead_max_hp = undead.max_hp * 60 / 100;
    if undead_max_hp < 1 { undead_max_hp = 1; }
    set_mobile_hp(undead.id, undead_max_hp);
    set_mobile_short_desc(undead.id, "the zombie of " + undead.short_desc);
    move_mobile_to_room(undead.id, room_id);

    // Permanent charm bond — clears on caster quit/death via break_all_charms_by_player.
    apply_buff_to_mobile(undead.id, "charmed", 0, -1, char.name);
    forget_player(undead.id, char.name);
    if is_mobile_in_combat(undead.id) {
        exit_mobile_combat(undead.id);
    }

    let risen = get_mobile_data(undead.id);
    let risen_desc = if type_of(risen) != "()" { risen.short_desc } else { "the zombie" };
    send_client_message(connection_id, risen_desc + " rises jerkily, its eyes fixed on you.");
    broadcast_to_room_except(room_id, connection_id, risen_desc + " rises jerkily, its eyes fixed on " + char.name + ".");
    award_per_spell_xp(connection_id, char.name, spell["id"], true);
    return true;
}

fn cast_control_weather(connection_id, char, spell, target_arg) {
    let arg = target_arg.to_lower();
    if arg != "better" && arg != "worse" {
        send_client_message(connection_id, "You must specify 'better' or 'worse' (e.g. cast 'control weather' better).");
        return false;
    }

    let room_id = char.current_room_id;
    broadcast_to_room_except(room_id, connection_id, char.name + " gestures skyward, weaving an incantation into the air.");
    send_client_message(connection_id, "You gesture skyward and weave the spell into the air...");

    let result = shift_global_weather(arg);
    let landed = false;
    if result["changed"] {
        landed = true;
        if arg == "better" {
            send_client_message(connection_id, "The skies obey: the weather grows calmer.");
        } else {
            send_client_message(connection_id, "The skies obey: the weather turns fouler.");
        }
    } else {
        let endpoint = result["at_endpoint"];
        if type_of(endpoint) != "()" && endpoint == "clear" {
            send_client_message(connection_id, "The skies are already as clear as they can be.");
        } else if type_of(endpoint) != "()" && endpoint == "storm" {
            send_client_message(connection_id, "The skies are already as foul as they can be.");
        } else {
            send_client_message(connection_id, "The spell stirs the air, but nothing changes.");
        }
    }
    award_per_spell_xp(connection_id, char.name, spell["id"], landed);
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

    // Silence buff: caster cannot speak the words. god_mode bypasses.
    if !char.god_mode && has_buff(char.name, "silence") {
        send_client_message(connection_id, "Your throat seizes - you cannot speak the words of the spell.");
        return;
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

    // Thinblood tier-3 lock: tier-3 disciplines (skill_required >= 5)
    // can't be channeled until the kindred is acknowledged by a clan.
    // Tier-1 and tier-2 powers remain available; this only gates the
    // top-of-tree powers (Dominate proper, Time Stop, Mask of a Thousand
    // Faces, etc.).
    if is_thinblood(connection_id) && spell["skill_required"] >= 5 {
        send_client_message(connection_id, "Your blood is too thin to channel this. Find your clan to claim its full power.");
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
    if stype == "debuff" {
        success = cast_debuff(connection_id, char, spell, target_arg);
    }
    if stype == "heal" {
        success = cast_heal(connection_id, char, spell, target_arg);
    }
    if stype == "utility" {
        success = cast_utility(connection_id, char, spell, target_arg);
    }
    if stype == "summon" {
        success = cast_summon(connection_id, char, spell, target_arg);
    }
    if stype == "charm" {
        success = cast_charm(connection_id, char, spell, target_arg);
    }
    if stype == "fear" {
        success = cast_fear(connection_id, char, spell, target_arg);
    }
    if stype == "dominate" {
        success = cast_dominate(connection_id, char, spell, target_arg);
    }
    if stype == "animate_dead" {
        success = cast_animate_dead(connection_id, char, spell, target_arg);
    }
    if stype == "control_weather" {
        success = cast_control_weather(connection_id, char, spell, target_arg);
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
