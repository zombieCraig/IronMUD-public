// scripts/commands/snipe.rhai
// Fire a ranged weapon at a target in an adjacent room (cross-room sniping)

fn run_command(args, connection_id) {
    let check = validate_snipe(args, connection_id);
    if check.error != "" {
        send_client_message(connection_id, check.error);
        return;
    }

    if !consume_stamina_for_attack(check.char.name, 10) {
        send_client_message(connection_id, "Failed to consume stamina.");
        return;
    }

    execute_snipe(check, connection_id);
}

fn validate_snipe(args, connection_id) {
    let result = #{ "error": "", "char": (), "weapon_info": (), "direction": "", "target_keyword": "", "target_room_id": "" };
    let char = get_player_character(connection_id);
    if char == () { result.error = "You must be logged in to snipe."; return result; }
    if char.position == "sleeping" { result.error = "You are asleep! Wake up first."; return result; }
    if char.position == "resting" || char.position == "sitting" { result.error = "You must stand up first."; return result; }
    if is_in_combat(char.name) { result.error = "You can't snipe while in combat! Use `shoot` instead."; return result; }
    if args == "" { result.error = "Snipe in which direction? Usage: snipe <direction> [target]"; return result; }

    let parts = args.split(" ");
    let dir_arg = parts[0].to_lower();
    let target_keyword = "";
    if parts.len() > 1 {
        target_keyword = rejoin_parts(parts, 1);
    }

    let direction = normalize_dir(dir_arg);
    if !is_std_dir(direction) { result.error = "Invalid direction. Use: north, south, east, west, up, down (or n/s/e/w/u/d)."; return result; }

    let weapon_info = get_ranged_info(char.name);
    if !weapon_info.is_ranged { result.error = "You need to wield a ranged weapon to snipe."; return result; }
    if char.stamina < 10 { result.error = "You are too exhausted to snipe. Rest to recover stamina."; return result; }

    let ammo_err = check_ammo(weapon_info.weapon_id, char.name);
    if ammo_err != "" { result.error = ammo_err; return result; }

    let room_id = char.current_room_id;
    let target_room_id = get_exit_target(room_id, direction);
    if target_room_id == "" { result.error = "There is no exit to the " + direction + "."; return result; }

    let door = get_door(room_id, direction);
    if door != () {
        if door.is_closed { result.error = "The " + door.name + " to the " + direction + " is closed."; return result; }
    }

    let zone = get_combat_zone(target_room_id);
    if zone == "safe" && !char.god_mode { result.error = "That room is a safe zone. You cannot attack targets there."; return result; }

    result.char = char;
    result.weapon_info = weapon_info;
    result.direction = direction;
    result.target_keyword = target_keyword;
    result.target_room_id = target_room_id;
    return result;
}

fn execute_snipe(check, connection_id) {
    let char = check.char;
    let weapon_info = check.weapon_info;
    let direction = check.direction;
    let target_room_id = check.target_room_id;
    let opposite_dir = opposite(direction);

    let target_info = find_snipe_target(target_room_id, check.target_keyword, char.god_mode);
    if target_info.target_type == "none" {
        if check.target_keyword != "" {
            send_client_message(connection_id, "You don't see '" + check.target_keyword + "' to the " + direction + ".");
        } else {
            send_client_message(connection_id, "There is nobody to snipe to the " + direction + ".");
        }
        return;
    }
    if target_info.error != "" { send_client_message(connection_id, target_info.error); return; }

    let mobile = get_mobile_data(target_info.target_id);
    if mobile == () { send_client_message(connection_id, "Target no longer exists."); return; }

    if char.god_mode {
        god_smite_snipe(char, mobile, target_room_id, direction, opposite_dir, connection_id);
        return;
    }

    let ammo_info = consume_ammo_for_shot(weapon_info, char.name);
    let att_bonuses = get_weapon_attachment_bonuses(weapon_info.weapon_id);
    let base_hit = calc_snipe_hit(char, mobile, target_room_id, att_bonuses.accuracy_bonus, connection_id);
    let noise_level = get_effective_noise_level(weapon_info.weapon_id, att_bonuses.noise_reduction);

    let any_hit = false;
    let target_dead = false;
    let shot_num = 0;

    while shot_num < ammo_info.shots && !target_dead {
        let hit_chance = base_hit + (ammo_info.accuracy_penalty * shot_num);
        if hit_chance < 5 { hit_chance = 5; }
        let attack_roll = roll_attack(hit_chance);

        if !attack_roll.hit {
            send_client_message(connection_id, "You fire to the " + direction + " but miss!");
            broadcast_to_room_awake(char.current_room_id, char.name + " fires to the " + direction + ".", char.name);
            broadcast_to_room_awake(target_room_id, "A " + projectile_name(ammo_info.rtype) + " flies in from the " + opposite_dir + " and misses!", "");
            shot_num += 1;
            continue;
        }

        any_hit = true;
        let hit_result = do_snipe_hit(char, mobile, target_info, weapon_info, ammo_info, direction, opposite_dir, target_room_id, connection_id);
        target_dead = hit_result;
        shot_num += 1;
    }

    // Post-shot resolution
    if !target_dead {
        resolve_aftermath(char, mobile, target_info, noise_level, opposite_dir, target_room_id, any_hit);
    }
    do_noise_broadcast(noise_level, target_room_id, char.name);
}

fn do_snipe_hit(char, mobile, target_info, weapon_info, ammo_info, direction, opposite_dir, target_room_id, connection_id) {
    let base_damage = calculate_damage(weapon_info.dice_count, weapon_info.dice_sides, weapon_info.damage_bonus + ammo_info.ammo_bonus);
    let is_crit = check_critical_hit(get_skill_level(char.name, "ranged"));
    let damage = base_damage;
    let crit_text = "";
    if is_crit {
        damage = calculate_crit_damage(damage, get_skill_level(char.name, "ranged"));
        crit_text = " \x1b[1;33m[CRITICAL]\x1b[0m";
    }

    let body_part = roll_random_body_part();

    // Inline armor check using Rust functions
    let blocked = false;
    let armor_name = "";
    let armor_list = get_mobile_armor_for_body_part(target_info.target_id, body_part);
    for armor in armor_list {
        let save = roll_armor_save(armor.id, damage);
        if save.blocked {
            blocked = true;
            armor_name = armor.name;
            add_armor_hole(armor.id);
            damage = 0;
            break;
        }
    }

    if !blocked && damage > 0 {
        mobile.hp = mobile.hp - damage;
        save_mobile_data(mobile);
    }

    // Apply crit effects via Rust functions
    if is_crit && !blocked {
        apply_snipe_crit(target_info.target_id, body_part, weapon_info.damage_type, char.name);
    }

    if !blocked && damage > 0 {
        let leveled = add_skill_experience(char.name, "ranged", 10);
        if leveled { send_client_message(connection_id, "\x1b[1;33mYour ranged skill has improved!\x1b[0m"); }
    }

    // Messages — body-part contextual
    broadcast_to_room_awake(char.current_room_id, char.name + " fires to the " + direction + ".", char.name);
    let projectile = projectile_name(ammo_info.rtype);
    if blocked {
        send_snipe_blocked_messages(connection_id, target_room_id, projectile, mobile.name, body_part, direction, opposite_dir, armor_name, crit_text);
    } else if damage > 0 {
        let severity = snipe_get_severity(damage, weapon_info);
        let verb = snipe_hit_verb(ammo_info.rtype, severity);
        send_snipe_hit_messages(connection_id, target_room_id, projectile, verb, mobile.name, body_part, direction, opposite_dir, damage, crit_text);
    }

    // Apply ammo special effects on hit (not blocked)
    if !blocked && damage > 0 {
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

    // Embed projectile for arrow recovery (arrows/bolts only)
    if !blocked && damage > 0 {
        let ammo_vnum = snipe_get_ammo_vnum(weapon_info, ammo_info);
        if ammo_vnum != "" {
            embed_projectile(target_info.target_id, ammo_vnum);
        }
    }

    if mobile.hp <= 0 {
        fire_mobile_trigger(target_info.target_id, "on_death", connection_id, #{ "killer": char.name, "killer_type": "player" });
        send_client_message(connection_id, "\x1b[1;31m" + mobile.name + " is dead!\x1b[0m");
        broadcast_to_room_awake(target_room_id, "\x1b[1;31m" + mobile.name + " collapses, struck down by a shot from the " + opposite_dir + "!\x1b[0m", "");
        let corpse_id = create_corpse(mobile.name, target_room_id, false);
        if corpse_id != "" {
            transfer_mobile_items_to_corpse(target_info.target_id, corpse_id);
            set_corpse_gold(corpse_id, mobile.gold);
            // Arrow recovery
            process_arrow_recovery(target_info.target_id, corpse_id);
        }
        delete_mobile(target_info.target_id);
        return true;
    }
    return false;
}

fn snipe_get_bow_ammo_vnum(ammo_info) {
    if ammo_info.ammo_id == "" { return ""; }
    let ammo_item = get_item_data(ammo_info.ammo_id);
    if ammo_item == () { return ""; }
    let v = ammo_item.vnum;
    if v == () || v == "" { return ""; }
    return v;
}

fn snipe_get_ammo_vnum(weapon_info, ammo_info) {
    let caliber = get_item_caliber(weapon_info.weapon_id);
    if caliber == () || caliber == "" { return ""; }
    let cal_lower = caliber.to_lower();
    if cal_lower != "arrow" && cal_lower != "bolt" { return ""; }
    if ammo_info.rtype == "bow" {
        return snipe_get_bow_ammo_vnum(ammo_info);
    }
    let v = get_item_loaded_ammo_vnum(weapon_info.weapon_id);
    if v != () && v != "" { return v; }
    return "";
}

fn apply_snipe_crit(target_id, body_part, damage_type, attacker_name) {
    let effect = roll_critical_effect(damage_type);
    let skill = get_skill_level(attacker_name, "ranged");
    apply_crit_by_type(target_id, body_part, effect, skill);
}

fn apply_crit_by_type(target_id, body_part, effect, skill) {
    // Stun effects
    if is_stun_type(effect) {
        apply_mobile_stun(target_id, get_crit_stun_rounds(skill));
        return;
    }
    // Bleed effects
    if is_bleed_type(effect) {
        add_mobile_wound_bleeding(target_id, body_part, get_crit_bleeding_severity(skill));
        return;
    }
    // Disable effects
    if is_disable_type(effect) {
        escalate_mobile_wound_to_severe(target_id, body_part);
        return;
    }
    // Combined stun+bleed
    if effect == "impaled" || effect == "bullet_lodged" {
        apply_mobile_stun(target_id, get_crit_stun_rounds(skill));
        add_mobile_wound_bleeding(target_id, body_part, 2);
        return;
    }
    // Ongoing damage effects
    apply_crit_ongoing(target_id, body_part, effect, skill);
}

fn is_stun_type(e) {
    if e == "concussion" { return true; }
    if e == "hypothermic_shock" { return true; }
    if e == "electrocuted" { return true; }
    if e == "toxic_shock" { return true; }
    if e == "nerve_strike" { return true; }
    if e == "stun" { return true; }
    if e == "lockjaw" { return true; }
    return false;
}

fn is_bleed_type(e) {
    if e == "deep_laceration" { return true; }
    if e == "arterial_cut" { return true; }
    if e == "punctured_organ" { return true; }
    if e == "dissolved_flesh" { return true; }
    if e == "bleeding" { return true; }
    if e == "mauled" { return true; }
    if e == "through_and_through" { return true; }
    return false;
}

fn is_disable_type(e) {
    if e == "severed_tendon" { return true; }
    if e == "frozen_limb" { return true; }
    if e == "nerve_damage" { return true; }
    if e == "paralysis" { return true; }
    if e == "charred" { return true; }
    if e == "corroded_armor" { return true; }
    if e == "broken_bone" { return true; }
    if e == "disable" { return true; }
    if e == "severed_chunk" { return true; }
    if e == "shrapnel" { return true; }
    return false;
}

fn apply_crit_ongoing(target_id, body_part, effect, skill) {
    let element = "fire";
    if effect == "frostbitten" { element = "cold"; }
    if effect == "venom_surge" { element = "poison"; }
    if effect == "acid_burn" { element = "acid"; }

    if effect == "ignited" {
        apply_mobile_stun(target_id, get_crit_stun_rounds(skill));
        apply_mobile_ongoing_effect(target_id, element, 3, 3, body_part);
        return;
    }
    if effect == "severe_burn" || effect == "frostbitten" || effect == "venom_surge" || effect == "acid_burn" {
        let dmg = 3;
        if effect == "frostbitten" { dmg = 2; }
        apply_mobile_ongoing_effect(target_id, element, 3, dmg, body_part);
    }
}

fn resolve_aftermath(char, mobile, target_info, noise_level, opposite_dir, target_room_id, any_hit) {
    let mob = get_mobile_data(target_info.target_id);
    if mob == () { return; }

    // Cowardly mob flees
    if mob.flags.cowardly {
        do_cowardly_flee(mob, target_info.target_id, opposite_dir, target_room_id);
        return;
    }

    // Determine pursuit
    let pursue = calc_pursuit(noise_level, any_hit);
    if pursue.will_pursue {
        start_mob_pursuit(target_info.target_id, char.name, char.current_room_id, opposite_dir, pursue.certain);
    }
}

fn do_cowardly_flee(mob, mob_id, shot_from_dir, target_room_id) {
    let flee_dir = opposite(shot_from_dir);
    let flee_target = try_flee_exit(target_room_id, flee_dir);

    if flee_target == "" {
        flee_target = find_any_flee_exit(target_room_id, shot_from_dir);
        if flee_target.room == "" { return; }
        flee_dir = flee_target.dir;
        flee_target = flee_target.room;
    }

    move_mobile_to_room(mob_id, flee_target);
    broadcast_to_room_awake(target_room_id, mob.name + " panics and flees to the " + flee_dir + "!", "");
    broadcast_to_room_awake(flee_target, mob.name + " arrives from the " + opposite(flee_dir) + ", looking terrified!", "");
}

fn try_flee_exit(room_id, direction) {
    let target = get_exit_target(room_id, direction);
    if target == "" { return ""; }
    let door = get_door(room_id, direction);
    if door != () && door.is_closed { return ""; }
    return target;
}

fn find_any_flee_exit(room_id, avoid_dir) {
    let result = #{ "dir": "", "room": "" };
    let dirs = ["north", "south", "east", "west", "up", "down"];
    for d in dirs {
        if d != avoid_dir {
            let t = try_flee_exit(room_id, d);
            if t != "" {
                result.dir = d;
                result.room = t;
                return result;
            }
        }
    }
    return result;
}

fn calc_pursuit(noise, did_hit) {
    let result = #{ "will_pursue": false, "certain": false };
    let roll = random_int(1, 100);
    if did_hit {
        return calc_pursuit_hit(noise, roll);
    }
    return calc_pursuit_miss(noise, roll);
}

fn calc_pursuit_hit(noise, roll) {
    let result = #{ "will_pursue": false, "certain": false };
    if noise == "silent" { result.will_pursue = roll <= 50; return result; }
    if noise == "quiet" { result.will_pursue = roll <= 75; result.certain = random_int(1, 100) <= 50; return result; }
    result.will_pursue = true;
    result.certain = true;
    return result;
}

fn calc_pursuit_miss(noise, roll) {
    let result = #{ "will_pursue": false, "certain": false };
    if noise == "silent" { result.will_pursue = roll <= 25; return result; }
    if noise == "quiet" { result.will_pursue = roll <= 37; result.certain = random_int(1, 100) <= 50; return result; }
    if noise == "normal" { result.will_pursue = roll <= 50; result.certain = true; return result; }
    result.will_pursue = true;
    result.certain = true;
    return result;
}

fn do_noise_broadcast(noise_level, target_room_id, exclude_name) {
    if noise_level != "loud" { return; }
    let dirs = ["north", "south", "east", "west", "up", "down"];
    for d in dirs {
        let adj = get_exit_target(target_room_id, d);
        if adj != "" {
            broadcast_to_room_awake(adj, "You hear a gunshot from the " + opposite(d) + "!", exclude_name);
        }
    }
}

fn find_snipe_target(room_id, keyword, god_mode) {
    let result = #{ "target_type": "none", "target_id": "", "target_name": "", "error": "" };
    let mobiles = get_mobiles_in_room(room_id);
    if keyword != "" {
        let mobile = find_mobile_by_keyword(keyword, mobiles);
        if mobile != () {
            if mobile.flags.no_attack && !god_mode { result.error = mobile.name + " cannot be attacked."; return result; }
            result.target_type = "mobile";
            result.target_id = mobile.id;
            result.target_name = mobile.name;
            return result;
        }
    } else {
        for m in mobiles {
            if m.hp > 0 && !m.flags.no_attack {
                result.target_type = "mobile";
                result.target_id = m.id;
                result.target_name = m.name;
                return result;
            }
        }
    }
    return result;
}

fn god_smite_snipe(char, mobile, target_room_id, direction, opposite_dir, connection_id) {
    fire_mobile_trigger(mobile.id, "on_death", connection_id, #{ "killer": char.name, "killer_type": "player" });
    send_client_message(connection_id, "\x1b[1;33mYou point to the " + direction + " and call down divine wrath!\x1b[0m");
    broadcast_to_room_awake(char.current_room_id, "\x1b[1;33m" + char.name + " points to the " + direction + ". A distant flash of light!\x1b[0m", char.name);
    broadcast_to_room_awake(target_room_id, "\x1b[1;33mA column of blinding light obliterates " + mobile.name + "!\x1b[0m", "");
    let corpse_id = create_corpse(mobile.name, target_room_id, false);
    if corpse_id != "" {
        transfer_mobile_items_to_corpse(mobile.id, corpse_id);
        set_corpse_gold(corpse_id, mobile.gold);
    }
    delete_mobile(mobile.id);
}

// ========== Weapon/ammo helpers ==========

fn is_wielded_ranged(item) {
    let locs = item.wear_locations;
    for loc in locs {
        if loc.to_lower() == "wielded" {
            let wskill = get_item_weapon_skill(item.id);
            if wskill == "ranged" { return true; }
        }
    }
    return false;
}

fn get_ranged_info(char_name) {
    let result = #{ "is_ranged": false, "weapon_id": "", "name": "", "skill": "ranged", "dice_count": 1, "dice_sides": 4, "damage_bonus": 0, "damage_type": "piercing" };
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_wielded_ranged(item) {
            result.is_ranged = true;
            result.weapon_id = item.id;
            result.name = item.name;
            result.dice_count = item.damage_dice_count;
            result.dice_sides = item.damage_dice_sides;
            result.damage_type = item.damage_type;
            return result;
        }
    }
    return result;
}

fn check_ammo(weapon_id, char_name) {
    let rtype = get_item_ranged_type(weapon_id);
    if rtype == "crossbow" || rtype == "firearm" {
        if get_item_loaded_ammo(weapon_id) <= 0 { return "Your weapon is empty! Use `reload` to load ammunition."; }
        return "";
    }
    let caliber = get_item_caliber(weapon_id);
    if caliber == () || caliber == "" { return ""; }
    let ammo = find_compatible_ammo(char_name, caliber);
    if ammo.source == "none" { return "You have no " + caliber + " ammunition."; }
    if ammo.source == "inventory" { return "Your ammunition is in your pack. Use `ready` to ready it first."; }
    return "";
}

fn consume_ammo_for_shot(weapon_info, char_name) {
    let result = #{ "ammo_bonus": 0, "shots": 1, "accuracy_penalty": 0, "rtype": "", "ammo_id": "" };
    let rtype = get_item_ranged_type(weapon_info.weapon_id);
    result.rtype = rtype;
    if rtype == "crossbow" || rtype == "firearm" {
        let loaded = get_item_loaded_ammo(weapon_info.weapon_id);
        let fire_mode = get_item_fire_mode(weapon_info.weapon_id);
        result.shots = shots_for_mode(fire_mode, loaded);
        result.accuracy_penalty = penalty_for_mode(fire_mode);
        result.ammo_bonus = get_item_loaded_ammo_bonus(weapon_info.weapon_id);
        set_item_loaded_ammo(weapon_info.weapon_id, loaded - result.shots);
        return result;
    }
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

fn shots_for_mode(mode, loaded) {
    if mode == "burst" { if loaded < 3 { return loaded; } return 3; }
    if mode == "auto" { return loaded; }
    return 1;
}

fn penalty_for_mode(mode) {
    if mode == "burst" { return -1; }
    if mode == "auto" { return -3; }
    return 0;
}

fn calc_snipe_hit(char, mobile, target_room_id, accuracy_bonus, connection_id) {
    let skill = get_skill_level(char.name, "ranged");
    let hit = calculate_hit_chance(skill, get_stat_modifier(get_effective_stat(char.name, "dexterity")), get_stat_modifier(mobile.stat_dex), mobile.armor_class);
    hit = hit - 10; // snipe penalty
    hit = hit + accuracy_bonus; // attachment bonus
    let vp = get_vision_penalty(target_room_id, connection_id);
    if vp > 0 { hit = hit - vp; }
    if hit < 5 { hit = 5; }
    return hit;
}

fn get_effective_noise_level(weapon_id, noise_reduction) {
    let base = get_item_noise_level(weapon_id);
    if noise_reduction <= 0 { return base; }
    // Step down noise levels: loud -> normal -> quiet -> silent
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

fn projectile_name(rtype) {
    if rtype == "crossbow" { return "bolt"; }
    if rtype == "bow" { return "arrow"; }
    if rtype == "firearm" { return "bullet"; }
    return "projectile";
}

fn snipe_get_severity(damage, weapon_info) {
    let max_damage = weapon_info.dice_count * weapon_info.dice_sides;
    if max_damage <= 0 { return "medium"; }
    if damage <= max_damage / 4 { return "low"; }
    if damage > (max_damage * 3) / 4 { return "high"; }
    return "medium";
}

fn snipe_hit_verb(rtype, severity) {
    if severity == "low" {
        if rtype == "crossbow" { return "nicks"; }
        return "grazes";
    }
    if severity == "medium" {
        if rtype == "bow" { return "lodges in"; }
        if rtype == "crossbow" { return "punches into"; }
        return "rips into";
    }
    if rtype == "bow" { return "punches through"; }
    return "tears through";
}

fn send_snipe_blocked_messages(connection_id, target_room_id, projectile, mob_name, body_part, direction, opposite_dir, armor_name, crit_text) {
    let msg = "Your " + projectile + " strikes " + mob_name + "'s " + body_part;
    msg += " to the " + direction + " but " + armor_name + " blocks it!" + crit_text;
    send_client_message(connection_id, msg);
    let room_msg = mob_name + "'s " + body_part + " is struck by a " + projectile;
    room_msg += " from the " + opposite_dir + " but armor blocks it!";
    broadcast_to_room_awake(target_room_id, room_msg, "");
}

fn send_snipe_hit_messages(connection_id, target_room_id, projectile, verb, mob_name, body_part, direction, opposite_dir, damage, crit_text) {
    let msg = "Your " + projectile + " " + verb + " " + mob_name + "'s " + body_part;
    msg += " to the " + direction + " for " + damage + " damage!" + crit_text;
    send_client_message(connection_id, msg);
    let room_msg = "A " + projectile + " from the " + opposite_dir;
    room_msg += " " + verb + " " + mob_name + "'s " + body_part + " for " + damage + " damage!";
    broadcast_to_room_awake(target_room_id, room_msg, "");
}

// ========== Direction helpers ==========

fn normalize_dir(d) {
    if d == "n" { return "north"; }
    if d == "s" { return "south"; }
    if d == "e" { return "east"; }
    if d == "w" { return "west"; }
    if d == "u" { return "up"; }
    if d == "d" { return "down"; }
    return d;
}

fn is_std_dir(d) {
    if d == "north" { return true; }
    if d == "south" { return true; }
    if d == "east" { return true; }
    if d == "west" { return true; }
    if d == "up" { return true; }
    if d == "down" { return true; }
    return false;
}

fn opposite(d) {
    if d == "north" { return "south"; }
    if d == "south" { return "north"; }
    if d == "east" { return "west"; }
    if d == "west" { return "east"; }
    if d == "up" { return "below"; }
    if d == "down" { return "above"; }
    return "somewhere";
}

fn rejoin_parts(parts, start) {
    let s = "";
    let i = start;
    while i < parts.len() {
        if s != "" { s += " "; }
        s += parts[i];
        i += 1;
    }
    return s;
}
