// scripts/commands/shoot.rhai
// Fire a ranged weapon at a target to initiate ranged combat

fn run_command(args, connection_id) {
    let check = validate_shoot(args, connection_id);
    if check.error != "" {
        send_client_message(connection_id, check.error);
        return;
    }

    // Consume stamina
    if !consume_stamina_for_attack(check.char.name, 5) {
        send_client_message(connection_id, "Failed to consume stamina.");
        return;
    }

    // Execute the shot
    if check.target_info.target_type == "mobile" {
        execute_shoot_vs_mobile(check.char, check.target_info, check.weapon_info, connection_id);
    }
}

fn validate_shoot(args, connection_id) {
    let result = #{ "error": "", "char": (), "target_info": (), "weapon_info": () };
    let char = get_player_character(connection_id);
    if char == () { result.error = "You must be logged in to shoot."; return result; }
    if char.position == "sleeping" { result.error = "You are asleep! Wake up first."; return result; }
    if is_resting(char) { result.error = "You must stand up first."; return result; }
    if args == "" { result.error = "Shoot whom?"; return result; }

    let weapon_info = get_attacker_ranged_info(char.name);
    if !weapon_info.is_ranged { result.error = "You need to wield a ranged weapon to shoot."; return result; }

    let room_id = char.current_room_id;
    let target_keyword = args.to_lower();

    let zone = get_combat_zone(room_id);
    if zone == "safe" && !char.god_mode { result.error = "This is a safe zone. Combat is not allowed here."; return result; }

    let target_info = find_shoot_target(room_id, target_keyword, char.name, zone, char.god_mode);
    if target_info.target_type == "none" { result.error = "You don't see '" + args + "' here."; return result; }
    if target_info.error != "" { result.error = target_info.error; return result; }

    let melee_err = check_melee_distance(char.name, target_info.target_id);
    if melee_err != "" { result.error = melee_err; return result; }

    if char.stamina < 5 { result.error = "You are too exhausted to shoot. Rest to recover stamina."; return result; }

    let ammo_err = check_shoot_ammo(weapon_info.weapon_id, char.name);
    if ammo_err != "" { result.error = ammo_err; return result; }

    result.char = char;
    result.target_info = target_info;
    result.weapon_info = weapon_info;
    return result;
}

fn is_resting(char) {
    if char.position == "resting" { return true; }
    if char.position == "sitting" { return true; }
    return false;
}

fn check_melee_distance(char_name, target_id) {
    if is_in_combat(char_name) {
        let distance = get_combat_distance(char_name, target_id);
        if distance == "melee" {
            return "You're too close to shoot! Use `attack` to fight with fists or `retreat` first.";
        }
    }
    return "";
}

fn check_shoot_ammo(weapon_id, char_name) {
    let rtype = get_item_ranged_type(weapon_id);
    if rtype == "crossbow" || rtype == "firearm" {
        let loaded = get_item_loaded_ammo(weapon_id);
        if loaded <= 0 { return "Your weapon is empty! Use `reload` to load ammunition."; }
        return "";
    }
    return check_bow_ammo(weapon_id, char_name);
}

fn check_bow_ammo(weapon_id, char_name) {
    let caliber = get_item_caliber(weapon_id);
    if caliber == () || caliber == "" { return ""; }
    let ammo = find_compatible_ammo(char_name, caliber);
    if ammo.source == "none" { return "You have no " + caliber + " ammunition."; }
    if ammo.source == "inventory" { return "Your ammunition is in your pack. Use `ready` to ready it first."; }
    return "";
}

// Find a valid shoot target in the room
fn find_shoot_target(room_id, keyword, attacker_name, zone, god_mode) {
    let result = #{
        "target_type": "none",
        "target_id": "",
        "target_name": "",
        "error": ""
    };

    // Check mobiles (filter invisible ones the shooter can't see)
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), attacker_name);
    let mobile = find_mobile_by_keyword(keyword, mobiles);
    if mobile != () {
        if mobile.flags.no_attack && !god_mode {
            result.error = mobile.name + " cannot be attacked.";
            return result;
        }
        result.target_type = "mobile";
        result.target_id = mobile.id;
        result.target_name = mobile.name;
        return result;
    }

    return result;
}

// Get ranged weapon info from character's wielded weapon
fn get_attacker_ranged_info(char_name) {
    let result = #{
        "is_ranged": false,
        "weapon_id": "",
        "name": "",
        "skill": "ranged",
        "dice_count": 1,
        "dice_sides": 4,
        "damage_bonus": 0,
        "damage_type": "piercing"
    };

    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_item_wielded(item) {
            let wskill = get_item_weapon_skill(item.id);
            if wskill == "ranged" {
                result.is_ranged = true;
                result.weapon_id = item.id;
                result.name = item.name;
                result.skill = "ranged";
                result.dice_count = item.damage_dice_count;
                result.dice_sides = item.damage_dice_sides;
                result.damage_type = item.damage_type;
                return result;
            }
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

// Execute shoot against a mobile
fn execute_shoot_vs_mobile(attacker, target_info, weapon_info, connection_id) {
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

    // Enter combat at ranged distance
    enter_combat(attacker.name, "mobile", target_info.target_id);
    set_combat_distance(attacker.name, target_info.target_id, "ranged");

    // Mobile enters combat with the player
    enter_mobile_combat(target_info.target_id, "player", attacker.name);
    set_mobile_combat_distance(target_info.target_id, attacker.name, "ranged");

    // Fire on_attack trigger
    fire_mobile_trigger(target_info.target_id, "on_attack", connection_id, #{
        "attacker": attacker.name,
        "attacker_type": "player"
    });

    // Reload mobile after entering combat
    let mobile = get_mobile_data(target_info.target_id);
    if mobile == () {
        send_client_message(connection_id, "Target no longer exists.");
        return;
    }

    // Check and consume ammo - determine shots and penalty based on ranged_type
    let ammo_info = consume_shoot_ammo(weapon_info, attacker.name);

    // Get skill and calculate base hit chance (includes attachment bonuses)
    let base_hit_chance = calc_shoot_hit_chance(attacker, mobile, target_info.target_id, weapon_info, connection_id);

    // Multi-shot loop
    fire_shots(attacker, mobile, target_info, weapon_info, ammo_info, base_hit_chance, connection_id);

    // Broadcast gunshot noise to adjacent rooms
    let noise_bonuses = get_weapon_attachment_bonuses(weapon_info.weapon_id);
    let noise_level = get_shoot_noise_level(weapon_info.weapon_id, noise_bonuses.noise_reduction);
    if noise_level == "loud" {
        broadcast_gunshot_noise(attacker.current_room_id, attacker.name);
    }
}

fn consume_shoot_ammo(weapon_info, char_name) {
    let result = #{ "ammo_bonus": 0, "shots": 1, "accuracy_penalty": 0, "rtype": "", "ammo_id": "" };
    let rtype = get_item_ranged_type(weapon_info.weapon_id);
    result.rtype = rtype;

    if rtype == "crossbow" || rtype == "firearm" {
        let loaded = get_item_loaded_ammo(weapon_info.weapon_id);
        let fire_mode = get_item_fire_mode(weapon_info.weapon_id);
        result.shots = get_shots_for_mode(fire_mode, loaded);
        result.accuracy_penalty = get_penalty_for_mode(fire_mode);
        result.ammo_bonus = get_item_loaded_ammo_bonus(weapon_info.weapon_id);
        set_item_loaded_ammo(weapon_info.weapon_id, loaded - result.shots);
        return result;
    }

    // Bow or unset: consume from ready slot
    let caliber = get_item_caliber(weapon_info.weapon_id);
    if caliber != () && caliber != "" {
        let ammo = find_compatible_ammo(char_name, caliber);
        if ammo.source == "ready" {
            result.ammo_bonus = ammo.ammo_damage_bonus;
            result.ammo_id = ammo.item_id;
            consume_ammo(ammo.item_id, 1);
        }
    }
    return result;
}

fn calc_shoot_hit_chance(attacker, mobile, target_id, weapon_info, connection_id) {
    let attacker_skill = get_skill_level(attacker.name, "ranged");
    let attacker_dex_mod = get_stat_modifier(get_effective_stat(attacker.name, "dexterity"));
    let target_dex_mod = get_stat_modifier(mobile.stat_dex);
    let target_ac = mobile.armor_class;
    let base_hit_chance = calculate_hit_chance(attacker_skill, attacker_dex_mod, target_dex_mod, target_ac);

    // Apply distance modifier for ranged
    let distance = get_combat_distance(attacker.name, target_id);
    let distance_mod = get_distance_modifier("ranged", distance);
    base_hit_chance = base_hit_chance + (distance_mod * 3);

    // Apply weapon attachment accuracy bonus
    let bonuses = get_weapon_attachment_bonuses(weapon_info.weapon_id);
    base_hit_chance = base_hit_chance + bonuses.accuracy_bonus;

    // Apply vision penalty
    let room_id = attacker.current_room_id;
    let vision_penalty = get_vision_penalty(room_id, connection_id);
    if vision_penalty > 0 {
        base_hit_chance = base_hit_chance - vision_penalty;
        if base_hit_chance < 5 { base_hit_chance = 5; }
    }

    return base_hit_chance;
}

fn fire_shots(attacker, mobile, target_info, weapon_info, ammo_info, base_hit_chance, connection_id) {
    let target_dead = false;
    let shot_num = 0;
    while shot_num < ammo_info.shots && !target_dead {
        let hit_chance = base_hit_chance + (ammo_info.accuracy_penalty * shot_num);
        if hit_chance < 5 { hit_chance = 5; }

        let attack_roll = roll_attack(hit_chance);

        if !attack_roll.hit {
            send_shoot_miss_message(attacker, mobile.name, ammo_info.rtype, connection_id);
            shot_num += 1;
            continue;
        }

        target_dead = process_shot_hit(attacker, mobile, target_info, weapon_info, ammo_info, connection_id);
        shot_num += 1;
    }
}

fn process_shot_hit(attacker, mobile, target_info, weapon_info, ammo_info, connection_id) {
    // Calculate damage (add ammo bonus)
    let base_damage = calculate_damage(weapon_info.dice_count, weapon_info.dice_sides, weapon_info.damage_bonus + ammo_info.ammo_bonus);
    let is_crit = check_critical_hit(get_skill_level(attacker.name, "ranged"));
    let crit_effect = "none";
    let crit_effect_value = 0;

    let damage = base_damage;
    if is_crit {
        let skill = get_skill_level(attacker.name, "ranged");
        damage = calculate_crit_damage(damage, skill);
        crit_effect = roll_critical_effect(weapon_info.damage_type);
        crit_effect_value = get_typed_crit_value(crit_effect, skill);
    }

    let combat_result = #{
        "damage": damage,
        "is_crit": is_crit,
        "crit_effect": crit_effect,
        "crit_effect_value": crit_effect_value,
        "damage_type": weapon_info.damage_type
    };

    let body_part = roll_random_body_part();
    let armor_result = check_target_armor(target_info.target_id, "mobile", body_part, damage);

    // Apply damage
    if armor_result.damage_taken > 0 {
        mobile.hp = mobile.hp - armor_result.damage_taken;
        save_mobile_data(mobile);
    }

    // Apply critical effects
    if is_crit && !armor_result.blocked {
        apply_crit_effects_to_mobile(target_info.target_id, body_part, combat_result);
    }

    // Award XP to ranged skill
    if !armor_result.blocked && armor_result.damage_taken > 0 {
        award_ranged_xp(attacker.name, connection_id);
    }

    // Build combat messages
    send_shoot_messages(attacker, mobile.name, weapon_info, body_part,
                        armor_result.damage_taken, combat_result,
                        armor_result.blocked, armor_result.armor_name, ammo_info.rtype, connection_id);

    // Apply ammo special effects on hit (not blocked)
    if !armor_result.blocked && armor_result.damage_taken > 0 {
        let effect_type = "";
        let effect_dur = 0;
        let effect_dmg = 0;
        if ammo_info.rtype == "bow" {
            if ammo_info.ammo_id != "" {
                effect_type = get_item_ammo_effect_type(ammo_info.ammo_id);
                effect_dur = get_item_ammo_effect_duration(ammo_info.ammo_id);
                effect_dmg = get_item_ammo_effect_damage(ammo_info.ammo_id);
            }
        } else {
            effect_type = get_item_loaded_ammo_effect_type(weapon_info.weapon_id);
            effect_dur = get_item_loaded_ammo_effect_duration(weapon_info.weapon_id);
            effect_dmg = get_item_loaded_ammo_effect_damage(weapon_info.weapon_id);
        }
        if effect_type != "" {
            apply_mobile_ongoing_effect(target_info.target_id, effect_type, effect_dur, effect_dmg, body_part);
            send_client_message(connection_id, "Your " + effect_type + " ammunition sears " + mobile.name + "!");
        }
    }

    // Embed projectile for arrow recovery (arrows/bolts only, not bullets)
    if !armor_result.blocked && armor_result.damage_taken > 0 {
        let ammo_vnum = get_ammo_vnum_for_embed(weapon_info, ammo_info);
        if ammo_vnum != "" {
            embed_projectile(target_info.target_id, ammo_vnum);
        }
    }

    // Check if target died
    if mobile.hp <= 0 {
        handle_target_death(attacker, mobile, target_info, connection_id);
        return true;
    }
    return false;
}

fn get_bow_ammo_vnum(ammo_info) {
    if ammo_info.ammo_id == "" { return ""; }
    let ammo_item = get_item_data(ammo_info.ammo_id);
    if ammo_item == () { return ""; }
    let v = ammo_item.vnum;
    if v == () || v == "" { return ""; }
    return v;
}

fn get_ammo_vnum_for_embed(weapon_info, ammo_info) {
    // Only embed arrows and bolts, not bullets
    let caliber = get_item_caliber(weapon_info.weapon_id);
    if caliber == () || caliber == "" { return ""; }
    let cal_lower = caliber.to_lower();
    if cal_lower != "arrow" && cal_lower != "bolt" { return ""; }
    if ammo_info.rtype == "bow" {
        return get_bow_ammo_vnum(ammo_info);
    }
    // For magazine weapons (crossbow), get loaded_ammo_vnum
    let v = get_item_loaded_ammo_vnum(weapon_info.weapon_id);
    if v != () && v != "" { return v; }
    return "";
}

fn award_ranged_xp(char_name, connection_id) {
    let leveled = add_skill_experience(char_name, "ranged", 10);
    if leveled {
        send_client_message(connection_id, "\x1b[1;33mYour ranged skill has improved!\x1b[0m");
    }
}

fn handle_target_death(attacker, mobile, target_info, connection_id) {
    fire_mobile_trigger(target_info.target_id, "on_death", connection_id, #{
        "killer": attacker.name,
        "killer_type": "player"
    });

    send_client_message(connection_id, "\x1b[1;31m" + mobile.name + " is dead!\x1b[0m");
    broadcast_to_room_awake(attacker.current_room_id, "\x1b[1;31m" + mobile.name + " is dead!\x1b[0m", attacker.name);
    exit_mobile_combat(target_info.target_id);
    remove_combat_target(attacker.name, target_info.target_id);

    let room_id = attacker.current_room_id;
    let corpse_id = create_corpse(mobile.name, room_id, false);
    if corpse_id != "" {
        transfer_mobile_items_to_corpse(target_info.target_id, corpse_id);
        set_corpse_gold(corpse_id, mobile.gold);
        // Arrow recovery
        process_arrow_recovery(target_info.target_id, corpse_id);
    }

    delete_mobile(target_info.target_id);

    let targets = get_combat_targets(attacker.name);
    if targets.len() == 0 {
        exit_combat(attacker.name);
    }
}

// Helper: determine shots to fire based on fire mode and loaded ammo
fn get_shots_for_mode(fire_mode, loaded) {
    if fire_mode == "burst" {
        if loaded < 3 { return loaded; }
        return 3;
    }
    if fire_mode == "auto" { return loaded; }
    return 1; // single
}

// Helper: get accuracy penalty per shot for fire mode
fn get_penalty_for_mode(fire_mode) {
    if fire_mode == "burst" { return -1; }
    if fire_mode == "auto" { return -3; }
    return 0;
}

fn get_shoot_noise_level(weapon_id, noise_reduction) {
    let base = get_item_noise_level(weapon_id);
    if noise_reduction <= 0 { return base; }
    let levels = ["silent", "quiet", "normal", "loud"];
    let idx = 0;
    let i = 0;
    while i < levels.len() {
        if levels[i] == base { idx = i; }
        i += 1;
    }
    idx = idx - noise_reduction;
    if idx < 0 { idx = 0; }
    return levels[idx];
}

fn broadcast_gunshot_noise(room_id, exclude_name) {
    let dirs = ["north", "south", "east", "west", "up", "down"];
    for d in dirs {
        let adj = get_exit_target(room_id, d);
        if adj != "" {
            let from_dir = shoot_opposite_dir(d);
            broadcast_to_room_awake(adj, "You hear gunfire from the " + from_dir + "!", exclude_name);
        }
    }
}

fn shoot_opposite_dir(d) {
    if d == "north" { return "south"; }
    if d == "south" { return "north"; }
    if d == "east" { return "west"; }
    if d == "west" { return "east"; }
    if d == "up" { return "below"; }
    if d == "down" { return "above"; }
    return "somewhere";
}

// Helper: send miss message with appropriate verb
fn send_shoot_miss_message(attacker, target_name, rtype, connection_id) {
    if rtype == "crossbow" {
        send_client_message(connection_id, "You fire a bolt at " + target_name + " but miss!");
        broadcast_to_room_awake(attacker.current_room_id, attacker.name + " fires a bolt at " + target_name + " but misses!", attacker.name);
    } else {
        send_client_message(connection_id, "You fire at " + target_name + " but miss!");
        broadcast_to_room_awake(attacker.current_room_id, attacker.name + " fires at " + target_name + " but misses!", attacker.name);
    }
}

// Get the mechanical value for a typed crit effect
fn get_typed_crit_value(effect, skill_level) {
    if is_stun_crit(effect) { return get_crit_stun_rounds(skill_level); }
    if is_bleed_crit(effect) { return get_crit_bleeding_severity(skill_level); }
    if effect == "arterial_cut" { return get_crit_bleeding_severity(skill_level) + 1; }
    if effect == "impaled" { return get_crit_stun_rounds(skill_level); }
    if effect == "ignited" { return get_crit_stun_rounds(skill_level); }
    if effect == "crushed" { return 0; }
    if effect == "cardiac_shock" { return 0; }
    if effect == "severe_burn" { return 3; }
    if effect == "frostbitten" { return 2; }
    if effect == "venom_surge" { return 3; }
    if effect == "acid_burn" { return 3; }
    return 0;
}

fn is_stun_crit(effect) {
    if effect == "concussion" { return true; }
    if effect == "hypothermic_shock" { return true; }
    if effect == "electrocuted" { return true; }
    if effect == "toxic_shock" { return true; }
    if effect == "nerve_strike" { return true; }
    if effect == "lockjaw" { return true; }
    return false;
}

fn is_bleed_crit(effect) {
    if effect == "deep_laceration" { return true; }
    if effect == "punctured_organ" { return true; }
    if effect == "dissolved_flesh" { return true; }
    if effect == "mauled" { return true; }
    return false;
}

// Check target's armor for the body part
fn check_target_armor(target_id, target_type, body_part, damage) {
    let result = #{
        "blocked": false,
        "damage_taken": damage,
        "armor_name": "",
        "armor_destroyed": false
    };

    let armor_list = get_target_armor_list(target_id, target_type, body_part);
    if armor_list.len() == 0 {
        return result;
    }

    for armor in armor_list {
        let save_result = roll_armor_save(armor.id, damage);
        if save_result.blocked {
            return process_armor_block(result, armor);
        }
    }

    return result;
}

fn get_target_armor_list(target_id, target_type, body_part) {
    if target_type == "mobile" {
        return get_mobile_armor_for_body_part(target_id, body_part);
    }
    return get_armor_for_body_part(target_id, body_part);
}

fn process_armor_block(result, armor) {
    result.blocked = true;
    result.damage_taken = 0;
    result.armor_name = armor.name;
    let hole_result = add_armor_hole(armor.id);
    result.armor_destroyed = hole_result.destroyed;
    return result;
}

fn get_ballistic_crit_text(effect, body_part) {
    let color = "\x1b[1;37m";
    let reset = "\x1b[0m";
    if effect == "through_and_through" { return " " + color + "[CRITICAL - Through and Through! Heavy bleeding!]" + reset; }
    if effect == "shrapnel" { return " " + color + "[CRITICAL - Shrapnel! " + body_part + " disabled!]" + reset; }
    if effect == "bullet_lodged" { return " " + color + "[CRITICAL - Bullet Lodged! Stunned and bleeding!]" + reset; }
    if effect == "none" { return " \x1b[1;33m[CRITICAL]\x1b[0m"; }
    return " \x1b[1;33m[CRITICAL - " + effect + "!]\x1b[0m";
}

// Send shoot hit/miss messages with ranged flavor
fn send_shoot_messages(attacker, target_name, weapon_info, body_part, damage, combat_result, blocked, armor_name, rtype, connection_id) {
    let crit_text = "";
    if combat_result.is_crit {
        crit_text = get_ballistic_crit_text(combat_result.crit_effect, body_part);
    }

    if blocked {
        let projectile = get_projectile_word(rtype);
        let msg = "Your " + projectile + " strikes " + target_name + "'s " + body_part;
        msg += " but " + armor_name + " blocks it!" + crit_text;
        send_client_message(connection_id, msg);
        let room_msg = attacker.name + "'s " + projectile + " strikes " + target_name + "'s " + body_part;
        room_msg += " but " + armor_name + " blocks it!";
        broadcast_to_room_awake(attacker.current_room_id, room_msg, attacker.name);
        return;
    }

    if damage > 0 {
        let max_damage = weapon_info.dice_count * weapon_info.dice_sides;
        let msg = ranged_hit_message(rtype, target_name, body_part, damage, max_damage);
        send_client_message(connection_id, msg + crit_text);
        let room_msg = ranged_hit_room_message(rtype, attacker.name, target_name, body_part, damage, max_damage);
        broadcast_to_room_awake(attacker.current_room_id, room_msg, attacker.name);
    }
}

fn get_projectile_word(rtype) {
    if rtype == "bow" { return "arrow"; }
    if rtype == "crossbow" { return "bolt"; }
    return "shot";
}

fn get_hit_verb(rtype, severity) {
    if severity == "low" {
        if rtype == "crossbow" { return "nicks"; }
        return "grazes";
    }
    if severity == "medium" {
        if rtype == "bow" { return "lodges in"; }
        if rtype == "crossbow" { return "punches into"; }
        return "rips into";
    }
    // high
    if rtype == "bow" { return "punches through"; }
    return "tears through";
}

fn get_damage_severity(damage, max_damage) {
    if max_damage <= 0 { return "medium"; }
    if damage <= max_damage / 4 { return "low"; }
    if damage > (max_damage * 3) / 4 { return "high"; }
    return "medium";
}

fn ranged_hit_message(rtype, target_name, body_part, damage, max_damage) {
    let severity = get_damage_severity(damage, max_damage);
    let projectile = get_projectile_word(rtype);
    let verb = get_hit_verb(rtype, severity);
    return "Your " + projectile + " " + verb + " " + target_name + "'s " + body_part + " for " + damage + " damage!";
}

fn ranged_hit_room_message(rtype, attacker_name, target_name, body_part, damage, max_damage) {
    let severity = get_damage_severity(damage, max_damage);
    let projectile = get_projectile_word(rtype);
    let verb = get_hit_verb(rtype, severity);
    return attacker_name + "'s " + projectile + " " + verb + " " + target_name + "'s " + body_part + " for " + damage + " damage!";
}

// God mode: instant divine smite against a mobile
fn god_smite_mobile(attacker, mobile, connection_id) {
    let room_id = attacker.current_room_id;

    fire_mobile_trigger(mobile.id, "on_death", connection_id, #{
        "killer": attacker.name,
        "killer_type": "player"
    });

    send_client_message(connection_id, "\x1b[1;33mYou raise your hand and call down divine wrath upon " + mobile.name + "!\x1b[0m");
    send_client_message(connection_id, "\x1b[1;37mA column of blinding light obliterates them instantly!\x1b[0m");
    broadcast_to_room_awake(room_id, "\x1b[1;33m" + attacker.name + " raises a hand toward " + mobile.name + ". A column of blinding light obliterates them!\x1b[0m", attacker.name);

    let corpse_id = create_corpse(mobile.name, room_id, false);
    if corpse_id != "" {
        transfer_mobile_items_to_corpse(mobile.id, corpse_id);
        set_corpse_gold(corpse_id, mobile.gold);
    }

    delete_mobile(mobile.id);
}

// Apply typed critical effects to a mobile target
fn apply_crit_effects_to_mobile(target_id, body_part, combat_result) {
    let effect = combat_result.crit_effect;
    let value = combat_result.crit_effect_value;
    apply_typed_crit_mobile(target_id, body_part, effect, value);
}

fn apply_typed_crit_mobile(target_id, body_part, effect, value) {
    if is_stun_effect(effect) { apply_mobile_stun(target_id, value); return; }
    if is_bleed_effect(effect) { add_mobile_wound_bleeding(target_id, body_part, value); return; }
    if is_disable_effect(effect) { escalate_mobile_wound_to_severe(target_id, body_part); return; }
    if is_stun_bleed_effect(effect) { apply_mobile_stun(target_id, value); add_mobile_wound_bleeding(target_id, body_part, 2); return; }
    if is_ongoing_effect(effect) { apply_mobile_ongoing_from_crit(target_id, body_part, effect, value); return; }
    if is_ongoing_stun_effect(effect) { apply_mobile_stun(target_id, value); apply_mobile_ongoing_from_crit(target_id, body_part, effect, 3); return; }
}

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
