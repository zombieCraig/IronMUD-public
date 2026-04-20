// scripts/commands/backstab.rhai
// backstab <target> - Deliver a devastating attack from stealth

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    let stealth_skill = get_skill_level(char.name, "stealth");
    if stealth_skill < 3 {
        send_client_message(connection_id, "You need stealth level 3 to backstab.");
        return;
    }

    // Must be hidden, sneaking, or camouflaged
    if !is_hidden(char.name) && !is_sneaking(char.name) && !is_camouflaged(char.name) {
        send_client_message(connection_id, "You must be hidden or sneaking to backstab.");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't backstab while already in combat! Use 'circle' instead.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Backstab whom? Usage: backstab <target>");
        return;
    }

    if char.stamina < 15 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 15 stamina)");
        return;
    }

    // Must be wielding a short_blades weapon
    let weapon_info = get_backstab_weapon(char.name);
    if weapon_info.skill != "short_blades" {
        send_client_message(connection_id, "You need to wield a short blade to backstab.");
        return;
    }

    let room_id = char.current_room_id;
    let zone = get_combat_zone(room_id);
    if zone == "safe" && !char.god_mode {
        send_client_message(connection_id, "This is a safe zone. Combat is not allowed here.");
        return;
    }

    let target_keyword = args.to_lower();

    // Find target mobile
    let mobiles = get_mobiles_in_room(room_id);
    let target_id = "";
    let target_name = "";
    let target_type = "mobile";
    for mobile in mobiles {
        if mobile.name.to_lower().contains(target_keyword) {
            target_id = mobile.id;
            target_name = mobile.name;
        }
        for keyword in mobile.keywords {
            if keyword == target_keyword {
                target_id = mobile.id;
                target_name = mobile.name;
            }
        }
    }

    // Check player target in PvP zones
    if target_id == "" && zone == "pvp" {
        let players = get_characters_in_room(room_id);
        for pname in players {
            if pname == char.name { continue; }
            if pname.to_lower() == target_keyword || pname.to_lower().starts_with(target_keyword) {
                target_id = pname;
                target_name = pname;
                target_type = "player";
            }
        }
    }

    if target_id == "" {
        send_client_message(connection_id, "You don't see '" + args + "' here.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 15;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Break stealth
    break_stealth(char.name);

    // Calculate attack with bonuses
    let attacker_skill = get_skill_level(char.name, weapon_info.skill);
    let attacker_dex_mod = get_stat_modifier(get_effective_stat(char.name, "dexterity"));

    // +30% hit bonus from stealth
    let hit_chance = 50 + (attacker_skill * 5) + (attacker_dex_mod * 3) + 30;
    if hit_chance > 98 { hit_chance = 98; }
    if hit_chance < 20 { hit_chance = 20; }

    let roll = random_int(1, 100);

    if roll > hit_chance {
        // Miss
        send_client_message(connection_id, "You lunge from the shadows at " + target_name + " but miss!");
        broadcast_to_room_awake(room_id, char.name + " lunges from the shadows at " + target_name + " but misses!", char.name);
        // Enter combat anyway
        if target_type == "mobile" {
            enter_combat(char.name, "mobile", target_id);
            set_combat_distance(char.name, target_id, "melee");
            enter_mobile_combat(target_id, "player", char.name);
            set_mobile_combat_distance(target_id, char.name, "melee");
        } else {
            enter_combat(char.name, "player", target_id);
            enter_combat(target_id, "player", char.name);
        }
        return;
    }

    // Calculate damage with multiplier: 1.5 + (stealth_skill * 0.2) using integer math
    // e.g., at skill 3: 1.5 + 0.6 = 2.1x, at skill 10: 1.5 + 2.0 = 3.5x
    let base_damage = calculate_damage(weapon_info.dice_count, weapon_info.dice_sides, weapon_info.damage_bonus);
    let multiplier_x10 = 15 + (stealth_skill * 2); // x10 to avoid floats
    let damage = (base_damage * multiplier_x10) / 10;
    if damage < 1 { damage = 1; }

    // Doubled critical hit chance
    let is_crit = check_critical_hit(attacker_skill * 2);

    if is_crit {
        damage = calculate_crit_damage(damage, attacker_skill);
    }

    // Apply damage
    let crit_text = "";
    if is_crit { crit_text = " \x1b[1;33m[CRITICAL BACKSTAB!]\x1b[0m"; }

    send_client_message(connection_id, "You plunge your " + weapon_info.name + " into " + target_name + "'s back for " + damage + " damage!" + crit_text);
    broadcast_to_room_awake(room_id, char.name + " emerges from the shadows and plunges a blade into " + target_name + "'s back!", char.name);

    if target_type == "mobile" {
        let mobile = get_mobile_data(target_id);
        if mobile != () {
            mobile.hp = mobile.hp - damage;
            save_mobile_data(mobile);

            if mobile.hp <= 0 {
                // Target died
                fire_mobile_trigger(target_id, "on_death", connection_id, #{
                    "killer": char.name,
                    "killer_type": "player"
                });
                send_client_message(connection_id, target_name + " collapses to the ground, dead!");
                broadcast_to_room_awake(room_id, target_name + " collapses to the ground, dead!", char.name);
                let corpse_id = create_corpse(mobile.name, room_id, false);
                if corpse_id != "" {
                    transfer_mobile_items_to_corpse(target_id, corpse_id);
                    set_corpse_gold(corpse_id, mobile.gold);
                }
                delete_mobile(target_id);
            } else {
                // Enter combat
                enter_combat(char.name, "mobile", target_id);
                set_combat_distance(char.name, target_id, "melee");
                enter_mobile_combat(target_id, "player", char.name);
                set_mobile_combat_distance(target_id, char.name, "melee");
            }
        }
    } else {
        // Player target
        let target = get_character_data(target_id);
        if target != () {
            target.hp = target.hp - damage;
            save_character_data(target);

            if target.hp <= 0 {
                send_client_message(connection_id, target_name + " collapses to the ground, dead!");
                broadcast_to_room_awake(room_id, target_name + " collapses to the ground, dead!", char.name);
                let corpse_id = create_corpse(target_name, room_id, true);
                transfer_inventory_to_corpse(target_name, corpse_id);
                transfer_equipment_to_corpse(target_name, corpse_id);
                transfer_gold_to_corpse(target_name, corpse_id);
                respawn_character(target_name);
                let target_conn = find_player_connection_by_name(target_name);
                if target_conn != "" {
                    send_client_message(target_conn, "You have been slain by " + char.name + "'s backstab!");
                    send_client_message(target_conn, "You awaken at your spawn point...");
                }
            } else {
                enter_combat(char.name, "player", target_id);
                enter_combat(target_id, "player", char.name);
                let target_conn = find_player_connection_by_name(target_name);
                if target_conn != "" {
                    send_client_message(target_conn, char.name + " backstabs you from the shadows for " + damage + " damage!" + crit_text);
                }
            }
        }
    }

    // Award XP
    let leveled = add_skill_experience(char.name, "stealth", 25);
    if leveled {
        send_client_message(connection_id, "Your stealth skill has improved!");
    }
}

fn get_backstab_weapon(char_name) {
    let result = #{
        "name": "fists",
        "skill": "unarmed",
        "dice_count": 1,
        "dice_sides": 2,
        "damage_bonus": 0,
        "damage_type": "bludgeoning"
    };

    let equipped = get_equipped_items(char_name);
    for item in equipped {
        let locs = item.wear_locations;
        for loc in locs {
            if loc.to_lower() == "wielded" {
                let wskill = get_item_weapon_skill(item.id);
                if wskill != () && wskill != "" {
                    result.name = item.name;
                    result.skill = wskill;
                    result.dice_count = item.damage_dice_count;
                    result.dice_sides = item.damage_dice_sides;
                    result.damage_type = item.damage_type;
                }
                return result;
            }
        }
    }

    return result;
}
