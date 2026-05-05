// scripts/commands/attack.rhai
// Attack command for initiating combat against targets

// === Damage-type verb helpers (use early returns to avoid Rhai complexity limits) ===

fn get_hit_verb(damage_type) {
    if damage_type == "slashing" { return "slash"; }
    if damage_type == "piercing" { return "stab"; }
    if damage_type == "bludgeoning" { return "crush"; }
    if damage_type == "fire" { return "burn"; }
    if damage_type == "cold" { return "freeze"; }
    if damage_type == "lightning" { return "shock"; }
    if damage_type == "poison" { return "poison"; }
    if damage_type == "acid" { return "corrode"; }
    if damage_type == "bite" { return "bite"; }
    if damage_type == "ballistic" { return "shoot"; }
    return "hit";
}

fn get_hit_verb_third(damage_type) {
    if damage_type == "slashing" { return "slashes"; }
    if damage_type == "piercing" { return "stabs"; }
    if damage_type == "bludgeoning" { return "crushes"; }
    if damage_type == "fire" { return "burns"; }
    if damage_type == "cold" { return "freezes"; }
    if damage_type == "lightning" { return "shocks"; }
    if damage_type == "poison" { return "poisons"; }
    if damage_type == "acid" { return "corrodes"; }
    if damage_type == "bite" { return "bites"; }
    if damage_type == "ballistic" { return "shoots"; }
    return "hits";
}

fn get_graze_verb(damage_type) {
    if damage_type == "slashing" { return "nick"; }
    if damage_type == "piercing" { return "scratch"; }
    if damage_type == "bludgeoning" { return "brush"; }
    if damage_type == "fire" { return "singe"; }
    if damage_type == "cold" { return "chill"; }
    if damage_type == "lightning" { return "spark"; }
    if damage_type == "poison" { return "irritate"; }
    if damage_type == "acid" { return "sting"; }
    if damage_type == "bite" { return "nip"; }
    if damage_type == "ballistic" { return "graze"; }
    return "graze";
}

fn get_graze_verb_third(damage_type) {
    if damage_type == "slashing" { return "nicks"; }
    if damage_type == "piercing" { return "scratches"; }
    if damage_type == "bludgeoning" { return "brushes"; }
    if damage_type == "fire" { return "singes"; }
    if damage_type == "cold" { return "chills"; }
    if damage_type == "lightning" { return "sparks"; }
    if damage_type == "poison" { return "irritates"; }
    if damage_type == "acid" { return "stings"; }
    if damage_type == "bite" { return "nips"; }
    if damage_type == "ballistic" { return "grazes"; }
    return "grazes";
}

fn get_block_verb(damage_type) {
    if damage_type == "slashing" { return "slash at"; }
    if damage_type == "piercing" { return "thrust at"; }
    if damage_type == "bludgeoning" { return "swing at"; }
    if damage_type == "fire" { return "hurl flames at"; }
    if damage_type == "cold" { return "send frost at"; }
    if damage_type == "lightning" { return "send lightning at"; }
    if damage_type == "poison" { return "strike at"; }
    if damage_type == "acid" { return "fling acid at"; }
    if damage_type == "bite" { return "snap at"; }
    if damage_type == "ballistic" { return "fire at"; }
    return "strike at";
}

fn get_block_verb_third(damage_type) {
    if damage_type == "slashing" { return "slashes at"; }
    if damage_type == "piercing" { return "thrusts at"; }
    if damage_type == "bludgeoning" { return "swings at"; }
    if damage_type == "fire" { return "hurls flames at"; }
    if damage_type == "cold" { return "sends frost at"; }
    if damage_type == "lightning" { return "sends lightning at"; }
    if damage_type == "poison" { return "strikes at"; }
    if damage_type == "acid" { return "flings acid at"; }
    if damage_type == "bite" { return "snaps at"; }
    if damage_type == "ballistic" { return "fires at"; }
    return "strikes at";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to attack.");
        return;
    }

    // Can't attack while sleeping
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }

    // Can't attack while resting
    if char.position == "resting" || char.position == "sitting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Attack whom?");
        return;
    }

    let room_id = char.current_room_id;
    let target_keyword = args.to_lower();

    // Attacking breaks stealth
    break_stealth(char.name);

    // Check combat zone
    let zone = get_combat_zone(room_id);
    let in_build = is_in_build_mode(char.name, room_id);
    if zone == "safe" && !char.god_mode && !in_build {
        send_client_message(connection_id, "This is a safe zone. Combat is not allowed here.");
        return;
    }

    // Try to find target mobile
    let target_info = find_attack_target(room_id, target_keyword, char.name, zone, char.god_mode || in_build);
    if target_info.target_type == "none" {
        send_client_message(connection_id, "You don't see '" + args + "' here.");
        return;
    }

    if target_info.error != "" {
        send_client_message(connection_id, target_info.error);
        return;
    }

    // Check stamina (configurable via settings)
    let stamina_cost_str = get_setting_or_default("stamina_cost_attack", "5");
    let stamina_cost = parse_int(stamina_cost_str);
    if stamina_cost == () { stamina_cost = 5; }
    if char.stamina < stamina_cost {
        send_client_message(connection_id, "You are too exhausted to attack. Rest to recover stamina.");
        return;
    }

    // Consume stamina
    if !consume_stamina_for_attack(char.name, stamina_cost) {
        send_client_message(connection_id, "Failed to consume stamina.");
        return;
    }

    // Execute the attack
    if target_info.target_type == "mobile" {
        execute_attack_vs_mobile(char, target_info, connection_id);
    } else if target_info.target_type == "player" {
        execute_attack_vs_player(char, target_info, connection_id);
    }
}

// Find a valid attack target in the room
fn find_attack_target(room_id, keyword, attacker_name, zone, god_mode) {
    let result = #{
        "target_type": "none",
        "target_id": "",
        "target_name": "",
        "error": ""
    };

    // First check mobiles (filter out invisible ones the attacker can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    let mobile = find_mobile_by_keyword(keyword, mobiles);
    if mobile != () {
        if mobile.flags.no_attack && !god_mode {
            result.error = mobile.name + " cannot be attacked.";
            return result;
        }
        // Child-safety gate. Default "0" is disabled; admin may set
        // `min_attackable_age` via `ironmud-admin settings set`. Applies only
        // when the mobile has a non-zero `age` on its Characteristics — generic
        // mobs with unset age are not gated.
        if !god_mode && mobile.has_characteristics && mobile.age > 0 {
            let min_age_str = get_setting("min_attackable_age");
            if min_age_str != "" {
                let min_age = parse_int(min_age_str);
                if min_age != () && min_age > 0 && mobile.age < min_age {
                    result.error = mobile.name + " is too young to harm.";
                    return result;
                }
            }
        }
        result.target_type = "mobile";
        result.target_id = mobile.id;
        result.target_name = mobile.name;
        return result;
    }

    // Then check players (only in PvP zones)
    if zone == "pvp" {
        let characters = get_characters_in_room(room_id);
        for char_name in characters {
            if char_name.to_lower() == attacker_name.to_lower() { continue; }
            if char_name.to_lower() == keyword || char_name.to_lower().starts_with(keyword) {
                result.target_type = "player";
                result.target_id = char_name;
                result.target_name = char_name;
                return result;
            }
        }
    }

    return result;
}

// Execute attack against a mobile
fn execute_attack_vs_mobile(attacker, target_info, connection_id) {
    let mobile = get_mobile_data(target_info.target_id);
    if mobile == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // God mode: instant divine smite
    if attacker.god_mode {
        god_smite_mobile(attacker, mobile, connection_id);
        return;
    }

    // Check if target is unconscious - instant kill
    if is_mobile_unconscious(target_info.target_id) {
        execute_coup_de_grace_mobile(attacker, mobile, connection_id);
        return;
    }

    // Enter combat with the mobile
    enter_combat(attacker.name, "mobile", target_info.target_id);
    // Using attack command initiates at melee distance
    set_combat_distance(attacker.name, target_info.target_id, "melee");

    // Mobile enters combat with the player (it fights back)
    enter_mobile_combat(target_info.target_id, "player", attacker.name);
    // Mobile also at melee distance
    set_mobile_combat_distance(target_info.target_id, attacker.name, "melee");

    // Fire on_attack trigger for the mobile
    fire_mobile_trigger(target_info.target_id, "on_attack", connection_id, #{
        "attacker": attacker.name,
        "attacker_type": "player"
    });

    // IMPORTANT: Reload mobile after entering combat so we have the updated combat state
    // This ensures we don't overwrite the combat state when saving HP changes later
    let mobile = get_mobile_data(target_info.target_id);
    if mobile == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // Get attacker's weapon and skill
    let weapon_info = get_attacker_weapon_info(attacker.name);

    // If wielding a ranged weapon, attack command uses unarmed (fists) at melee
    let ranged_override = false;
    if weapon_info.skill == "ranged" {
        ranged_override = true;
        weapon_info.name = "fists";
        weapon_info.skill = "unarmed";
        weapon_info.dice_count = 1;
        weapon_info.dice_sides = 2;
        weapon_info.damage_bonus = 0;
        weapon_info.damage_type = "bludgeoning";
        send_client_message(connection_id, "You can't use your " + get_wielded_weapon_name(attacker.name) + " at this range! You resort to fighting with your fists.");
    }

    let attacker_skill = get_character_skill_level(attacker.name, weapon_info.skill);
    let attacker_dex_mod = get_stat_modifier(get_effective_stat(attacker.name, "dexterity"));

    // Get target's defense
    let target_dex_mod = get_stat_modifier(mobile.stat_dex);
    let target_ac = mobile.armor_class;

    // Calculate hit chance and roll
    let hit_chance = calculate_hit_chance(attacker_skill, attacker_dex_mod, target_dex_mod, target_ac);

    // Apply distance modifier based on weapon type
    let distance = get_combat_distance(attacker.name, target_info.target_id);
    let distance_mod = get_distance_modifier(weapon_info.skill, distance);
    hit_chance = hit_chance + (distance_mod * 3); // Convert modifier to percentage (each point = ~3%)

    // Apply vision penalty from lighting/traits
    let room_id = attacker.current_room_id;
    let vision_penalty = get_vision_penalty(room_id, connection_id);
    if vision_penalty > 0 {
        hit_chance = hit_chance - vision_penalty;
        if hit_chance < 5 { hit_chance = 5; } // Minimum 5% hit chance
    }

    let attack_roll = roll_attack(hit_chance);

    if !attack_roll.hit {
        send_miss_messages(attacker, mobile.name, connection_id);
        return;
    }

    // Calculate and apply damage
    let combat_result = calculate_attack_damage(attacker_skill, weapon_info);
    let body_part = roll_random_body_part();
    let armor_result = check_target_armor(target_info.target_id, "mobile", body_part, combat_result.damage);

    // Apply damage to mobile
    if armor_result.damage_taken > 0 {
        mobile.hp = mobile.hp - armor_result.damage_taken;
        save_mobile_data(mobile);
    }

    // Apply critical effects if damage went through and it was a crit
    if combat_result.is_crit && !armor_result.blocked {
        apply_crit_effects_to_mobile(target_info.target_id, body_part, combat_result);
    }

    // Apply envenom poison if weapon has charges
    if armor_result.damage_taken > 0 {
        let env_charges = get_envenomed_charges(attacker.name);
        if env_charges > 0 {
            apply_mobile_ongoing_effect(target_info.target_id, "poison", 3, 3, body_part);
            set_envenomed_charges(attacker.name, env_charges - 1);
            send_client_message(connection_id, "\x1b[0;32mYour envenomed weapon poisons " + mobile.name + "!\x1b[0m");
            if env_charges - 1 <= 0 {
                send_client_message(connection_id, "The poison on your weapon has been used up.");
            }
        }
    }

    // Award XP for successful hit
    if !armor_result.blocked && armor_result.damage_taken > 0 {
        let leveled = add_skill_experience(attacker.name, weapon_info.skill, 10);
        if leveled {
            send_client_message(connection_id, "\x1b[1;33mYour " + weapon_info.skill + " skill has improved!\x1b[0m");
        }
    }

    // Send combat messages
    send_hit_messages(attacker, mobile.name, weapon_info.name, body_part,
                      armor_result.damage_taken, combat_result,
                      armor_result.blocked, armor_result.armor_name,
                      weapon_info.damage_type, connection_id);

    // Check if target died
    if mobile.hp <= 0 {
        // Fire on_death trigger before removing the mobile
        fire_mobile_trigger(target_info.target_id, "on_death", connection_id, #{
            "killer": attacker.name,
            "killer_type": "player"
        });

        send_death_message(attacker, mobile.name, connection_id);
        // Exit combat for both parties
        exit_mobile_combat(target_info.target_id);
        remove_combat_target(attacker.name, target_info.target_id);

        // Create corpse and transfer mobile items
        let room_id = attacker.current_room_id;
        let corpse_id = create_corpse(mobile.name, room_id, false);
        if corpse_id != "" {
            transfer_mobile_items_to_corpse(target_info.target_id, corpse_id);
            set_corpse_gold(corpse_id, mobile.gold);
        }

        // Remove the dead mobile from the game
        delete_mobile(target_info.target_id);

        // Check if attacker has no more targets
        let targets = get_combat_targets(attacker.name);
        if targets.len() == 0 {
            exit_combat(attacker.name);
        }
    }
}

// Execute attack against another player (PvP)
fn execute_attack_vs_player(attacker, target_info, connection_id) {
    let target = get_character_data(target_info.target_name);
    if target == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // God mode: instant divine smite
    if attacker.god_mode {
        god_smite_player(attacker, target, target_info, connection_id);
        return;
    }

    // Cannot attack a god
    if target.god_mode {
        send_client_message(connection_id, "You think better of it. You are not that foolish.");
        return;
    }

    // Check if target is unconscious - instant kill
    if is_character_unconscious(target_info.target_name) {
        execute_coup_de_grace_player(attacker, target, connection_id);
        return;
    }

    // Check if target is sleeping - wake them up and give automatic hit
    let was_sleeping = target.position == "sleeping";
    if was_sleeping {
        target.position = "standing";
        save_character_data(target);
        // Sync to target's session
        let target_conn = find_player_connection_by_name(target.name);
        if target_conn != "" {
            set_player_character(target_conn, target);
            send_client_message(target_conn, "You are jolted awake by an attack!");
        }
        broadcast_to_room_awake(attacker.current_room_id, target.name + " is jolted awake!", target.name);
    }

    // Enter combat with the player (PvP) - use character name as target_id
    // Note: For PvP, we store the character name, not UUID
    // Distance tracking for PvP will be added when PvP UUID handling is improved
    enter_combat(attacker.name, "player", target_info.target_name);

    // Target enters combat with the attacker (they fight back)
    enter_combat(target_info.target_name, "player", attacker.name);

    // Get attacker's weapon and skill
    let weapon_info = get_attacker_weapon_info(attacker.name);
    let attacker_skill = get_character_skill_level(attacker.name, weapon_info.skill);
    let attacker_dex_mod = get_stat_modifier(get_effective_stat(attacker.name, "dexterity"));

    // Get target's defense
    let target_dex_mod = get_stat_modifier(get_effective_stat(target.name, "dexterity"));
    let target_ac = get_character_total_ac(target.name);

    // Calculate hit chance and roll (automatic hit if target was sleeping)
    let hit_chance = calculate_hit_chance(attacker_skill, attacker_dex_mod, target_dex_mod, target_ac);

    // Apply distance modifier (PvP assumed melee for now)
    let distance_mod = get_distance_modifier(weapon_info.skill, "melee");
    hit_chance = hit_chance + (distance_mod * 3);

    // Apply vision penalty from lighting/traits
    let room_id = attacker.current_room_id;
    let vision_penalty = get_vision_penalty(room_id, connection_id);
    if vision_penalty > 0 {
        hit_chance = hit_chance - vision_penalty;
        if hit_chance < 5 { hit_chance = 5; } // Minimum 5% hit chance
    }

    let attack_roll = roll_attack(hit_chance);

    if !was_sleeping && !attack_roll.hit {
        send_miss_messages(attacker, target.name, connection_id);
        return;
    }

    // Calculate and apply damage
    let combat_result = calculate_attack_damage(attacker_skill, weapon_info);
    let body_part = roll_random_body_part();
    let armor_result = check_target_armor(target.name, "player", body_part, combat_result.damage);

    // Apply damage to target
    if armor_result.damage_taken > 0 {
        target.hp = target.hp - armor_result.damage_taken;
        save_character_data(target);
    }

    // Apply critical effects if damage went through and it was a crit
    if combat_result.is_crit && !armor_result.blocked {
        apply_crit_effects_to_player(target.name, body_part, combat_result);
    }

    // Apply envenom poison if weapon has charges
    if armor_result.damage_taken > 0 {
        let env_charges = get_envenomed_charges(attacker.name);
        if env_charges > 0 {
            apply_ongoing_effect(target.name, "poison", 3, 3, body_part);
            set_envenomed_charges(attacker.name, env_charges - 1);
            send_client_message(connection_id, "\x1b[0;32mYour envenomed weapon poisons " + target.name + "!\x1b[0m");
            if env_charges - 1 <= 0 {
                send_client_message(connection_id, "The poison on your weapon has been used up.");
            }
        }
    }

    // Award XP for successful hit
    if !armor_result.blocked && armor_result.damage_taken > 0 {
        let leveled = add_skill_experience(attacker.name, weapon_info.skill, 10);
        if leveled {
            send_client_message(connection_id, "\x1b[1;33mYour " + weapon_info.skill + " skill has improved!\x1b[0m");
        }
    }

    // Send combat messages
    send_hit_messages(attacker, target.name, weapon_info.name, body_part,
                      armor_result.damage_taken, combat_result,
                      armor_result.blocked, armor_result.armor_name,
                      weapon_info.damage_type, connection_id);

    // Check if target died
    if target.hp <= 0 {
        send_death_message(attacker, target.name, connection_id);
        // Exit combat for both parties
        exit_combat(target_info.target_name);
        remove_combat_target(attacker.name, target_info.target_name);
        // Check if attacker has no more targets
        let targets = get_combat_targets(attacker.name);
        if targets.len() == 0 {
            exit_combat(attacker.name);
        }
    }
}

// Calculate damage including critical hit check and effects
fn calculate_attack_damage(attacker_skill, weapon_info) {
    let damage = calculate_damage(weapon_info.dice_count, weapon_info.dice_sides, weapon_info.damage_bonus);
    let is_crit = check_critical_hit(attacker_skill);
    let crit_effect = "none";
    let crit_effect_value = 0;
    let damage_type = weapon_info.damage_type;

    if is_crit {
        // Scale damage based on skill level
        damage = calculate_crit_damage(damage, attacker_skill);
        // Roll for damage-type-specific critical effect
        crit_effect = roll_critical_effect(damage_type);
        // Get effect value based on effect type
        crit_effect_value = get_typed_crit_value(crit_effect, attacker_skill);
    }
    return #{
        "damage": damage,
        "is_crit": is_crit,
        "crit_effect": crit_effect,
        "crit_effect_value": crit_effect_value,
        "damage_type": damage_type
    };
}

// Get the mechanical value for a typed crit effect
fn get_typed_crit_value(effect, skill_level) {
    // Stun effects
    if effect == "concussion" { return get_crit_stun_rounds(skill_level); }
    if effect == "hypothermic_shock" { return get_crit_stun_rounds(skill_level); }
    if effect == "electrocuted" { return get_crit_stun_rounds(skill_level); }
    if effect == "toxic_shock" { return get_crit_stun_rounds(skill_level); }
    if effect == "nerve_strike" { return get_crit_stun_rounds(skill_level); }
    if effect == "lockjaw" { return get_crit_stun_rounds(skill_level); }
    // Bleeding effects
    if effect == "deep_laceration" { return get_crit_bleeding_severity(skill_level); }
    if effect == "arterial_cut" { return get_crit_bleeding_severity(skill_level) + 1; }
    if effect == "punctured_organ" { return get_crit_bleeding_severity(skill_level); }
    if effect == "dissolved_flesh" { return get_crit_bleeding_severity(skill_level); }
    if effect == "mauled" { return get_crit_bleeding_severity(skill_level); }
    // Stun+bleed combo
    if effect == "impaled" { return get_crit_stun_rounds(skill_level); }
    if effect == "ignited" { return get_crit_stun_rounds(skill_level); }
    // Extra damage effects
    if effect == "crushed" { return 0; }
    if effect == "cardiac_shock" { return 0; }
    // Ongoing effects (value = damage per round)
    if effect == "severe_burn" { return 3; }
    if effect == "frostbitten" { return 2; }
    if effect == "venom_surge" { return 3; }
    if effect == "acid_burn" { return 3; }
    // Default
    return 0;
}

// Get attacker's weapon information
fn get_attacker_weapon_info(char_name) {
    let result = #{
        "name": "fists",
        "skill": "unarmed",
        "dice_count": 1,
        "dice_sides": 2,
        "damage_bonus": 0,
        "damage_type": "bludgeoning"
    };

    // Check for wielded weapon
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_item_wielded(item) {
            return get_weapon_stats(item, result);
        }
    }

    return result;
}

// Check if item is wielded
fn is_item_wielded(item) {
    let locs = item.wear_locations;
    for loc in locs {
        if loc.to_lower() == "wielded" { return true; }
    }
    return false;
}

// Get weapon stats from item
fn get_weapon_stats(item, default_result) {
    let result = default_result;

    // Check if item has weapon_skill (is actually a weapon)
    let wskill = get_item_weapon_skill(item.id);
    let is_weapon = wskill != () && wskill != "";

    if is_weapon {
        // Use weapon's stats
        result.name = item.name;
        result.skill = wskill;
        result.dice_count = item.damage_dice_count;
        result.dice_sides = item.damage_dice_sides;
        result.damage_type = item.damage_type;
    }
    // If not a weapon, keep unarmed defaults (fists, 1d2, "unarmed" skill)

    return result;
}

// Get the name of the currently wielded weapon (for messages)
fn get_wielded_weapon_name(char_name) {
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_item_wielded(item) { return item.name; }
    }
    return "weapon";
}

// Check target's armor for the body part
fn check_target_armor(target_id, target_type, body_part, damage) {
    let result = #{
        "blocked": false,
        "damage_taken": damage,
        "armor_name": "",
        "armor_destroyed": false
    };

    // Get armor protecting this body part
    let armor_list = get_target_armor_list(target_id, target_type, body_part);
    if armor_list.len() == 0 {
        return result;
    }

    // Try each piece of armor
    for armor in armor_list {
        let save_result = roll_armor_save(armor.id, damage);
        if save_result.blocked {
            return process_armor_block(result, armor);
        }
    }

    return result;
}

// Get armor list for target
fn get_target_armor_list(target_id, target_type, body_part) {
    if target_type == "mobile" {
        return get_mobile_armor_for_body_part(target_id, body_part);
    }
    return get_armor_for_body_part(target_id, body_part);
}

// Process successful armor block
fn process_armor_block(result, armor) {
    result.blocked = true;
    result.damage_taken = 0;
    result.armor_name = armor.name;
    let hole_result = add_armor_hole(armor.id);
    result.armor_destroyed = hole_result.destroyed;
    return result;
}

// Send miss messages (sleeping bystanders don't see combat)
fn send_miss_messages(attacker, target_name, connection_id) {
    send_client_message(connection_id, "You swing at " + target_name + " but miss!");
    broadcast_to_room_awake(attacker.current_room_id, attacker.name + " swings at " + target_name + " but misses!", attacker.name);
}

// Send hit messages
fn send_hit_messages(attacker, target_name, weapon_name, body_part, damage, combat_result, blocked, armor_name, damage_type, connection_id) {
    let crit_text = get_crit_text(combat_result.is_crit, combat_result.crit_effect, body_part, blocked, damage_type);

    if blocked {
        send_blocked_messages(attacker, target_name, weapon_name, body_part, armor_name, crit_text, damage_type, connection_id);
        return;
    }

    if damage > 0 {
        send_damage_messages(attacker, target_name, weapon_name, body_part, damage, crit_text, damage_type, connection_id);
        return;
    }

    send_graze_messages(attacker, target_name, weapon_name, body_part, damage_type, connection_id);
}

fn get_crit_text(is_crit, crit_effect, body_part, blocked, damage_type) {
    if !is_crit { return ""; }
    if blocked { return " \x1b[1;33m[CRITICAL - Blocked!]\x1b[0m"; }
    return get_typed_crit_text(crit_effect, body_part, damage_type);
}

fn get_crit_color(damage_type) {
    if damage_type == "fire" { return "\x1b[1;31m"; }
    if damage_type == "cold" { return "\x1b[1;36m"; }
    if damage_type == "poison" { return "\x1b[1;32m"; }
    if damage_type == "acid" { return "\x1b[1;32m"; }
    if damage_type == "lightning" { return "\x1b[1;34m"; }
    if damage_type == "bite" { return "\x1b[1;31m"; }
    if damage_type == "ballistic" { return "\x1b[1;37m"; }
    return "\x1b[1;33m";
}

fn get_typed_crit_text(effect, body_part, damage_type) {
    let color = get_crit_color(damage_type);
    let reset = "\x1b[0m";
    // Slashing
    if effect == "deep_laceration" { return " " + color + "[CRITICAL - Deep Laceration! Heavy bleeding!]" + reset; }
    if effect == "severed_tendon" { return " " + color + "[CRITICAL - Severed Tendon! " + body_part + " disabled!]" + reset; }
    if effect == "arterial_cut" { return " " + color + "[CRITICAL - Arterial Cut! Severe bleeding!]" + reset; }
    // Piercing
    if effect == "punctured_organ" { return " " + color + "[CRITICAL - Punctured Organ! Internal bleeding!]" + reset; }
    if effect == "impaled" { return " " + color + "[CRITICAL - Impaled! Stunned and bleeding!]" + reset; }
    if effect == "nerve_strike" { return " " + color + "[CRITICAL - Nerve Strike! Stunned!]" + reset; }
    // Bludgeoning
    if effect == "broken_bone" { return " " + color + "[CRITICAL - Broken Bone! " + body_part + " disabled!]" + reset; }
    if effect == "concussion" { return " " + color + "[CRITICAL - Concussion! Stunned!]" + reset; }
    if effect == "crushed" { return " " + color + "[CRITICAL - Crushed! Devastating blow!]" + reset; }
    // Fire
    if effect == "severe_burn" { return " " + color + "[CRITICAL - Severe Burn! Ongoing fire damage!]" + reset; }
    if effect == "ignited" { return " " + color + "[CRITICAL - Ignited! Burning and stunned!]" + reset; }
    if effect == "charred" { return " " + color + "[CRITICAL - Charred! " + body_part + " disabled!]" + reset; }
    // Cold
    if effect == "frozen_limb" { return " " + color + "[CRITICAL - Frozen Limb! " + body_part + " disabled!]" + reset; }
    if effect == "hypothermic_shock" { return " " + color + "[CRITICAL - Hypothermic Shock! Stunned!]" + reset; }
    if effect == "frostbitten" { return " " + color + "[CRITICAL - Frostbitten! Ongoing cold damage!]" + reset; }
    // Lightning
    if effect == "electrocuted" { return " " + color + "[CRITICAL - Electrocuted! Stunned!]" + reset; }
    if effect == "nerve_damage" { return " " + color + "[CRITICAL - Nerve Damage! " + body_part + " disabled!]" + reset; }
    if effect == "cardiac_shock" { return " " + color + "[CRITICAL - Cardiac Shock! Devastating jolt!]" + reset; }
    // Poison
    if effect == "venom_surge" { return " " + color + "[CRITICAL - Venom Surge! Ongoing poison damage!]" + reset; }
    if effect == "toxic_shock" { return " " + color + "[CRITICAL - Toxic Shock! Stunned!]" + reset; }
    if effect == "paralysis" { return " " + color + "[CRITICAL - Paralysis! " + body_part + " disabled!]" + reset; }
    // Acid
    if effect == "acid_burn" { return " " + color + "[CRITICAL - Acid Burn! Ongoing acid damage!]" + reset; }
    if effect == "corroded_armor" { return " " + color + "[CRITICAL - Corroded! " + body_part + " disabled!]" + reset; }
    if effect == "dissolved_flesh" { return " " + color + "[CRITICAL - Dissolved Flesh! Heavy bleeding!]" + reset; }
    // Bite
    if effect == "mauled" { return " " + color + "[CRITICAL - Mauled! Heavy bleeding!]" + reset; }
    if effect == "lockjaw" { return " " + color + "[CRITICAL - Lockjaw! Clamped and stunned!]" + reset; }
    if effect == "severed_chunk" { return " " + color + "[CRITICAL - Chunk Torn Away! " + body_part + " disabled!]" + reset; }
    // Ballistic
    if effect == "through_and_through" { return " " + color + "[CRITICAL - Through and Through! Heavy bleeding!]" + reset; }
    if effect == "shrapnel" { return " " + color + "[CRITICAL - Shrapnel! " + body_part + " disabled!]" + reset; }
    if effect == "bullet_lodged" { return " " + color + "[CRITICAL - Bullet Lodged! Stunned and bleeding!]" + reset; }
    // Legacy/default
    if effect == "bleeding" { return " \x1b[1;33m[CRITICAL - Bleeding!]\x1b[0m"; }
    if effect == "stun" { return " \x1b[1;33m[CRITICAL - Stun!]\x1b[0m"; }
    if effect == "disable" { return " \x1b[1;33m[CRITICAL - " + body_part + " Disabled!]\x1b[0m"; }
    return " \x1b[1;33m[CRITICAL]\x1b[0m";
}

fn send_blocked_messages(attacker, target_name, weapon_name, body_part, armor_name, crit_text, damage_type, connection_id) {
    let verb = get_block_verb(damage_type);
    let verb_third = get_block_verb_third(damage_type);
    let attacker_msg = "You " + verb + " " + target_name + "'s " + body_part + " with your " + weapon_name;
    attacker_msg += ", but their " + armor_name + " absorbs the blow!" + crit_text;
    send_client_message(connection_id, attacker_msg);

    let room_msg = attacker.name + " " + verb_third + " " + target_name + "'s " + body_part;
    room_msg += " with their " + weapon_name + ", but armor absorbs the blow!";
    broadcast_to_room_awake(attacker.current_room_id, room_msg, attacker.name);
}

fn send_damage_messages(attacker, target_name, weapon_name, body_part, damage, crit_text, damage_type, connection_id) {
    let verb = get_hit_verb(damage_type);
    let verb_third = get_hit_verb_third(damage_type);
    let attacker_msg = "You " + verb + " " + target_name + "'s " + body_part + " with your " + weapon_name;
    attacker_msg += " for " + damage + " damage!" + crit_text;
    send_client_message(connection_id, attacker_msg);

    let room_msg = attacker.name + " " + verb_third + " " + target_name + "'s " + body_part;
    room_msg += " with their " + weapon_name + "!";
    broadcast_to_room_awake(attacker.current_room_id, room_msg, attacker.name);
}

fn send_graze_messages(attacker, target_name, weapon_name, body_part, damage_type, connection_id) {
    let verb = get_graze_verb(damage_type);
    let verb_third = get_graze_verb_third(damage_type);
    let attacker_msg = "You " + verb + " " + target_name + "'s " + body_part + " with your " + weapon_name;
    attacker_msg += ", but deal no damage.";
    send_client_message(connection_id, attacker_msg);

    let room_msg = attacker.name + " " + verb_third + " " + target_name + " with their " + weapon_name + ".";
    broadcast_to_room_awake(attacker.current_room_id, room_msg, attacker.name);
}

fn send_death_message(attacker, target_name, connection_id) {
    send_client_message(connection_id, target_name + " collapses to the ground, dead!");
    broadcast_to_room_awake(attacker.current_room_id, target_name + " collapses to the ground, dead!", attacker.name);
}

// Helper: Get character's total AC from all equipped armor
fn get_character_total_ac(char_name) {
    let total_ac = 0;
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if item.armor_class != () {
            let effective = calculate_effective_ac(item.id);
            total_ac = total_ac + effective;
        }
    }
    return total_ac;
}

// Helper: Get character's skill level
fn get_character_skill_level(char_name, skill_name) {
    return get_skill_level(char_name, skill_name);
}

// Apply typed critical effects to a mobile target
fn apply_crit_effects_to_mobile(target_id, body_part, combat_result) {
    let effect = combat_result.crit_effect;
    let value = combat_result.crit_effect_value;
    apply_typed_crit_mobile(target_id, body_part, effect, value);
}

fn apply_typed_crit_mobile(target_id, body_part, effect, value) {
    // Stun effects
    if is_stun_effect(effect) { apply_mobile_stun(target_id, value); return; }
    // Bleed effects
    if is_bleed_effect(effect) { add_mobile_wound_bleeding(target_id, body_part, value); return; }
    // Disable effects
    if is_disable_effect(effect) { escalate_mobile_wound_to_severe(target_id, body_part); return; }
    // Stun+bleed combo effects
    if is_stun_bleed_effect(effect) { apply_mobile_stun(target_id, value); add_mobile_wound_bleeding(target_id, body_part, 2); return; }
    // Ongoing damage effects
    if is_ongoing_effect(effect) { apply_mobile_ongoing_from_crit(target_id, body_part, effect, value); return; }
    // Ongoing+stun combo
    if is_ongoing_stun_effect(effect) { apply_mobile_stun(target_id, value); apply_mobile_ongoing_from_crit(target_id, body_part, effect, 3); return; }
    // "clean" or "crushed"/"cardiac_shock" - just bonus damage, no secondary
}

// Apply typed critical effects to a player target
fn apply_crit_effects_to_player(target_name, body_part, combat_result) {
    let effect = combat_result.crit_effect;
    let value = combat_result.crit_effect_value;
    apply_typed_crit_player(target_name, body_part, effect, value);
}

fn apply_typed_crit_player(target_name, body_part, effect, value) {
    if is_stun_effect(effect) { apply_stun(target_name, value); return; }
    if is_bleed_effect(effect) { add_wound_bleeding(target_name, body_part, value); return; }
    if is_disable_effect(effect) { escalate_wound_to_severe(target_name, body_part); return; }
    if is_stun_bleed_effect(effect) { apply_stun(target_name, value); add_wound_bleeding(target_name, body_part, 2); return; }
    if is_ongoing_effect(effect) { apply_player_ongoing_from_crit(target_name, body_part, effect, value); return; }
    if is_ongoing_stun_effect(effect) { apply_stun(target_name, value); apply_player_ongoing_from_crit(target_name, body_part, effect, 3); return; }
}

// Effect classification helpers (early-return pattern to avoid complexity limit)
fn is_stun_effect(effect) {
    if effect == "concussion" { return true; }
    if effect == "hypothermic_shock" { return true; }
    if effect == "electrocuted" { return true; }
    if effect == "toxic_shock" { return true; }
    if effect == "nerve_strike" { return true; }
    if effect == "stun" { return true; }
    if effect == "lockjaw" { return true; }
    return false;
}

fn is_bleed_effect(effect) {
    if effect == "deep_laceration" { return true; }
    if effect == "arterial_cut" { return true; }
    if effect == "punctured_organ" { return true; }
    if effect == "dissolved_flesh" { return true; }
    if effect == "bleeding" { return true; }
    if effect == "mauled" { return true; }
    if effect == "through_and_through" { return true; }
    return false;
}

fn is_disable_effect(effect) {
    if effect == "severed_tendon" { return true; }
    if effect == "frozen_limb" { return true; }
    if effect == "nerve_damage" { return true; }
    if effect == "paralysis" { return true; }
    if effect == "charred" { return true; }
    if effect == "corroded_armor" { return true; }
    if effect == "broken_bone" { return true; }
    if effect == "disable" { return true; }
    if effect == "severed_chunk" { return true; }
    if effect == "shrapnel" { return true; }
    return false;
}

fn is_stun_bleed_effect(effect) {
    if effect == "impaled" { return true; }
    if effect == "bullet_lodged" { return true; }
    return false;
}

fn is_ongoing_effect(effect) {
    if effect == "severe_burn" { return true; }
    if effect == "frostbitten" { return true; }
    if effect == "venom_surge" { return true; }
    if effect == "acid_burn" { return true; }
    return false;
}

fn is_ongoing_stun_effect(effect) {
    if effect == "ignited" { return true; }
    return false;
}

fn get_ongoing_element(effect) {
    if effect == "severe_burn" { return "fire"; }
    if effect == "ignited" { return "fire"; }
    if effect == "frostbitten" { return "cold"; }
    if effect == "venom_surge" { return "poison"; }
    if effect == "acid_burn" { return "acid"; }
    return "fire";
}

fn apply_mobile_ongoing_from_crit(target_id, body_part, effect, damage) {
    let element = get_ongoing_element(effect);
    apply_mobile_ongoing_effect(target_id, element, 3, damage, body_part);
}

fn apply_player_ongoing_from_crit(target_name, body_part, effect, damage) {
    let element = get_ongoing_element(effect);
    apply_ongoing_effect(target_name, element, 3, damage, body_part);
}

// God mode: instant divine smite against a mobile
fn god_smite_mobile(attacker, mobile, connection_id) {
    let room_id = attacker.current_room_id;

    // Fire on_death trigger before removing the mobile
    fire_mobile_trigger(mobile.id, "on_death", connection_id, #{
        "killer": attacker.name,
        "killer_type": "player"
    });

    // Dramatic smite messages
    send_client_message(connection_id, "\x1b[1;33mYou raise your hand and call down divine wrath upon " + mobile.name + "!\x1b[0m");
    send_client_message(connection_id, "\x1b[1;37mA column of blinding light obliterates them instantly!\x1b[0m");
    broadcast_to_room_awake(room_id, "\x1b[1;33m" + attacker.name + " raises a hand toward " + mobile.name + ". A column of blinding light obliterates them!\x1b[0m", attacker.name);

    // Create corpse and transfer mobile items
    let corpse_id = create_corpse(mobile.name, room_id, false);
    if corpse_id != "" {
        transfer_mobile_items_to_corpse(mobile.id, corpse_id);
        set_corpse_gold(corpse_id, mobile.gold);
    }

    // Remove the dead mobile from the game
    delete_mobile(mobile.id);

    // Exit combat for attacker if in combat
    exit_mobile_combat(mobile.id);
    remove_combat_target(attacker.name, mobile.id);
    let targets = get_combat_targets(attacker.name);
    if targets.len() == 0 {
        exit_combat(attacker.name);
    }

    send_client_message(connection_id, mobile.name + " has been smitten.");
}

// God mode: instant divine smite against a player
fn god_smite_player(attacker, target, target_info, connection_id) {
    let room_id = attacker.current_room_id;
    let target_name = target.name;

    // Dramatic smite messages
    send_client_message(connection_id, "\x1b[1;33mYou raise your hand and call down divine wrath upon " + target_name + "!\x1b[0m");
    send_client_message(connection_id, "\x1b[1;37mA column of blinding light obliterates them instantly!\x1b[0m");
    broadcast_to_room_awake(room_id, "\x1b[1;33m" + attacker.name + " raises a hand toward " + target_name + ". A column of blinding light obliterates them!\x1b[0m", attacker.name);

    // Notify the victim
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn != "" {
        send_client_message(target_conn, "\x1b[1;33mA blinding column of divine light strikes you down!\x1b[0m");
        send_client_message(target_conn, "You have been smitten by " + attacker.name + "!");
    }

    // Create corpse and transfer items
    let corpse_id = create_corpse(target_name, room_id, true);
    transfer_inventory_to_corpse(target_name, corpse_id);
    transfer_equipment_to_corpse(target_name, corpse_id);
    transfer_gold_to_corpse(target_name, corpse_id);

    // Respawn the player
    respawn_character(target_name);

    // Notify victim of respawn
    if target_conn != "" {
        send_client_message(target_conn, "You awaken at your spawn point...");
    }

    // Exit combat for attacker if in combat
    exit_combat(target_info.target_name);
    remove_combat_target(attacker.name, target_info.target_name);
    let targets = get_combat_targets(attacker.name);
    if targets.len() == 0 {
        exit_combat(attacker.name);
    }

    send_client_message(connection_id, target_name + " has been smitten.");
}

// Execute a coup de grace against an unconscious mobile (instant kill)
fn execute_coup_de_grace_mobile(attacker, mobile, connection_id) {
    let room_id = attacker.current_room_id;

    // Fire on_death trigger before removing the mobile
    fire_mobile_trigger(mobile.id, "on_death", connection_id, #{
        "killer": attacker.name,
        "killer_type": "player"
    });

    // Message - sleeping bystanders don't see combat
    send_client_message(connection_id, "You deliver a killing blow to the unconscious " + mobile.name + "!");
    broadcast_to_room_awake(room_id, attacker.name + " delivers a killing blow to the unconscious " + mobile.name + "!", attacker.name);

    // Create corpse and transfer mobile items
    let corpse_id = create_corpse(mobile.name, room_id, false);
    if corpse_id != "" {
        transfer_mobile_items_to_corpse(mobile.id, corpse_id);
        set_corpse_gold(corpse_id, mobile.gold);
    }

    // Kill and remove the mobile
    delete_mobile(mobile.id);

    // Exit attacker's combat
    exit_combat(attacker.name);

    send_client_message(connection_id, mobile.name + " is dead.");
}

// Execute a coup de grace against an unconscious player (instant kill)
fn execute_coup_de_grace_player(attacker, target, connection_id) {
    let room_id = attacker.current_room_id;
    let target_name = target.name;

    // Message - sleeping bystanders don't see combat
    send_client_message(connection_id, "You deliver a killing blow to the unconscious " + target_name + "!");
    broadcast_to_room_awake(room_id, attacker.name + " delivers a killing blow to the unconscious " + target_name + "!", attacker.name);

    // Create corpse and respawn (this handles everything)
    let corpse_id = create_corpse(target_name, room_id, true);

    // Transfer items to corpse
    transfer_inventory_to_corpse(target_name, corpse_id);
    transfer_equipment_to_corpse(target_name, corpse_id);
    transfer_gold_to_corpse(target_name, corpse_id);

    // Respawn the player
    respawn_character(target_name);

    // Send death message to target
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn != "" {
        send_client_message(target_conn, "You have been slain by " + attacker.name + "!");
        send_client_message(target_conn, "You awaken at your spawn point...");
    }

    // Exit attacker's combat
    exit_combat(attacker.name);

    send_client_message(connection_id, target_name + " is dead.");
}
