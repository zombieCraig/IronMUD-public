// scripts/commands/bash.rhai
// bash <target> - Charge a target, dealing modest damage and stunning them on hit.

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

    if char.stamina < 15 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 15 stamina)");
        return;
    }

    let room_id = char.current_room_id;
    let zone = get_combat_zone(room_id);
    if zone == "safe" && !char.god_mode {
        send_client_message(connection_id, "This is a safe zone. Combat is not allowed here.");
        return;
    }

    // Resolve target: explicit arg first, otherwise current melee combat target.
    let target_keyword = args.to_lower();
    let target_id = "";
    let target_name = "";

    if target_keyword == "" {
        if is_in_combat(char.name) {
            let targets = get_combat_targets(char.name);
            if targets.len() > 0 {
                target_id = targets[0];
                let mob = get_mobile_data(target_id);
                if mob != () {
                    target_name = mob.name;
                }
            }
        }
        if target_id == "" {
            send_client_message(connection_id, "Bash whom? Usage: bash <target>");
            return;
        }
    } else {
        let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
        for mobile in mobiles {
            if target_id != "" { break; }
            if mobile.name.to_lower().contains(target_keyword) {
                target_id = mobile.id;
                target_name = mobile.name;
            } else {
                for keyword in mobile.keywords {
                    if keyword == target_keyword {
                        target_id = mobile.id;
                        target_name = mobile.name;
                        break;
                    }
                }
            }
        }
        if target_id == "" {
            send_client_message(connection_id, "You don't see '" + args + "' here.");
            return;
        }
    }

    let mob = get_mobile_data(target_id);
    if mob == () {
        send_client_message(connection_id, "That target is no longer here.");
        return;
    }

    if mob.flags.no_attack {
        send_client_message(connection_id, "You cannot attack " + mob.name + ".");
        return;
    }

    // Spend stamina up front (matches CircleMUD: a wasted bash still costs you).
    if !char.god_mode {
        char.stamina = char.stamina - 15;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Hard immunity: MOB_NOBASH parity. No stun, no damage, no combat shift.
    if mob.flags.no_bash {
        send_client_message(connection_id, mob.name + " braces and shrugs off your charge!");
        broadcast_to_room_awake(room_id, char.name + " charges " + mob.name + ", but " + mob.name + " barely budges!", char.name);
        return;
    }

    let bash_skill = get_skill_level(char.name, "bash");
    let attacker_str_mod = get_stat_modifier(get_effective_stat(char.name, "strength"));

    // To-hit: 50 base + 5/skill_level + 2/strength_mod, clamped 20..95.
    let hit_chance = 50 + (bash_skill * 5) + (attacker_str_mod * 2);
    if hit_chance > 95 { hit_chance = 95; }
    if hit_chance < 20 { hit_chance = 20; }

    let roll = random_int(1, 100);

    if roll > hit_chance {
        send_client_message(connection_id, "You charge at " + target_name + " but stumble, missing wildly!");
        broadcast_to_room_awake(room_id, char.name + " charges at " + target_name + " but stumbles, missing wildly!", char.name);
        // Initiate combat on miss.
        if !is_in_combat(char.name) {
            enter_combat(char.name, "mobile", target_id);
            set_combat_distance(char.name, target_id, "melee");
        }
        enter_mobile_combat(target_id, "player", char.name);
        set_mobile_combat_distance(target_id, char.name, "melee");
        add_skill_experience(char.name, "bash", 1);
        return;
    }

    // Modest damage: 1d4 + strength_mod + skill/2.
    let damage = random_int(1, 4) + attacker_str_mod + (bash_skill / 2);
    if damage < 1 { damage = 1; }

    send_client_message(connection_id, "You slam into " + target_name + " for " + damage + " damage, knocking them senseless!");
    broadcast_to_room_awake(room_id, char.name + " slams into " + target_name + ", leaving them stunned!", char.name);

    // Apply damage + stun.
    let mobile = get_mobile_data(target_id);
    if mobile != () {
        mobile.hp = mobile.hp - damage;
        save_mobile_data(mobile);
        apply_mobile_stun(target_id, 2);

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
            delete_mobile(target_id);
            add_skill_experience(char.name, "bash", 5);
            return;
        }
    }

    // Initiate combat on hit.
    if !is_in_combat(char.name) {
        enter_combat(char.name, "mobile", target_id);
        set_combat_distance(char.name, target_id, "melee");
    }
    enter_mobile_combat(target_id, "player", char.name);
    set_mobile_combat_distance(target_id, char.name, "melee");

    let leveled = add_skill_experience(char.name, "bash", 4);
    if leveled {
        send_client_message(connection_id, "Your bash skill has improved!");
    }
}
