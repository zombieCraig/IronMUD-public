// scripts/commands/circle.rhai
// circle - Attempt to circle behind your opponent in combat for a bonus attack

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
    if stealth_skill < 4 {
        send_client_message(connection_id, "You need stealth level 4 to circle.");
        return;
    }

    if !is_in_combat(char.name) {
        send_client_message(connection_id, "You must be in combat to circle.");
        return;
    }

    if char.stamina < 15 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 15 stamina)");
        return;
    }

    // Check cooldown (2-round = ~10 seconds)
    let cooldown = get_circle_cooldown(char.name);
    if cooldown > 0 {
        send_client_message(connection_id, "You must wait " + cooldown + " seconds before circling again.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 15;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Set cooldown (10 seconds = ~2 combat rounds)
    set_circle_cooldown(char.name, 10);

    let room_id = char.current_room_id;

    // Get current combat targets
    let targets = get_combat_targets(char.name);
    if targets.len() == 0 {
        send_client_message(connection_id, "You have no combat targets.");
        return;
    }

    // Stealth check vs first target
    let target = targets[0];
    let target_type = target["target_type"];
    let target_id = target["target_id"];

    let success = false;
    let target_name = "";

    if target_type == "mobile" {
        success = stealth_check_vs_mobile(char.name, target_id, room_id);
        let mob = get_mobile_data(target_id);
        if mob != () { target_name = mob.name; }
    } else {
        success = stealth_check_vs_player(char.name, target_id, room_id);
        target_name = target_id;
    }

    if target_name == "" {
        send_client_message(connection_id, "Your target is no longer available.");
        return;
    }

    if !success {
        send_client_message(connection_id, "You try to circle behind " + target_name + " but they track your movement.");
        broadcast_to_room_awake(room_id, char.name + " tries to circle behind " + target_name + " but fails.", char.name);
        return;
    }

    // Success! Get weapon info and deliver bonus attack
    let weapon_info = get_circle_weapon(char.name);
    let attacker_skill = get_skill_level(char.name, weapon_info.skill);

    // Guaranteed hit with 1.5x damage
    let base_damage = calculate_damage(weapon_info.dice_count, weapon_info.dice_sides, weapon_info.damage_bonus);
    let damage = (base_damage * 15) / 10;
    if damage < 1 { damage = 1; }

    send_client_message(connection_id, "You circle behind " + target_name + " and strike for " + damage + " damage!");
    broadcast_to_room_awake(room_id, char.name + " circles behind " + target_name + " and strikes!", char.name);

    if target_type == "mobile" {
        let mobile = get_mobile_data(target_id);
        if mobile != () {
            mobile.hp = mobile.hp - damage;
            save_mobile_data(mobile);

            if mobile.hp <= 0 {
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
                exit_mobile_combat(target_id);
                remove_combat_target(char.name, target_id);
                delete_mobile(target_id);
                let remaining = get_combat_targets(char.name);
                if remaining.len() == 0 {
                    exit_combat(char.name);
                }
            }
        }
    } else {
        let target_char = get_character_data(target_id);
        if target_char != () {
            target_char.hp = target_char.hp - damage;
            save_character_data(target_char);

            let target_conn = find_player_connection_by_name(target_id);
            if target_conn != "" {
                send_client_message(target_conn, char.name + " circles behind you and strikes for " + damage + " damage!");
            }

            if target_char.hp <= 0 {
                send_client_message(connection_id, target_name + " collapses to the ground, dead!");
                broadcast_to_room_awake(room_id, target_name + " collapses to the ground, dead!", char.name);
                exit_combat(target_id);
                remove_combat_target(char.name, target_id);
                let remaining = get_combat_targets(char.name);
                if remaining.len() == 0 {
                    exit_combat(char.name);
                }
            }
        }
    }

    let leveled = add_skill_experience(char.name, "stealth", 15);
    if leveled {
        send_client_message(connection_id, "Your stealth skill has improved!");
    }
}

fn get_circle_weapon(char_name) {
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
