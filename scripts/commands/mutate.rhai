// scripts/commands/mutate.rhai
// Mutant: Year Zero mutation powers.
//
//   mutate              - list your mutations and MP pool
//   mutate <power> [target] [mp]
//
// Actives spend Mutation Points (default 1; more MP = stronger effect).
// Every MP spent adds a misfire die: any 1 and the power turns on you —
// lost MP, self-trauma, a permanent deformity, or full Overload (permanent
// stat loss + a brand-new uncontrolled mutation). MP comes back ONLY from
// `push` (and the Rot-Eater mutation). The gamble is the game.
//
// The activation transaction (gates, spend, misfire table) lives Rust-side
// in activate_pc_mutation; this script resolves targeting, applies the
// per-power effect, and narrates.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_mutant(connection_id) {
        send_client_message(connection_id, "The Zone never marked you. You have no mutations to call on.");
        return;
    }

    let trimmed = args.to_lower();
    if trimmed == "" || trimmed == "list" {
        show_mutation_list(connection_id);
        return;
    }

    let parts = trimmed.split(' ');
    let power_arg = parts[0];

    // Optional trailing MP amount; optional middle target.
    let mp = 1;
    let target_arg = "";
    if parts.len() >= 2 {
        let last = try_parse_int(parts[parts.len() - 1]);
        if last != () {
            mp = last;
            if parts.len() >= 3 {
                target_arg = parts[1];
            }
        } else {
            target_arg = parts[1];
        }
    }
    if mp < 1 { mp = 1; }

    let mutation_id = resolve_owned_mutation(connection_id, power_arg);
    if mutation_id == "" {
        send_client_message(connection_id, "You don't carry that mutation. Type 'mutate' to see what the Zone gave you.");
        return;
    }

    let info = get_mutation_info(mutation_id);
    if info.len() == 0 {
        send_client_message(connection_id, "That mutation is not known to this world.");
        return;
    }
    if info["activation"] == "passive" {
        send_client_message(connection_id, info["name"] + " is always part of you — it needs no effort to use.");
        return;
    }

    // Damage powers need a living target before we spend anything.
    let effect = info["effect"];
    let mob_id = "";
    if effect == "damage" || effect == "drain" {
        mob_id = find_power_target(char, target_arg);
        if mob_id == "" {
            send_client_message(connection_id, "Use " + info["name"] + " on whom?");
            return;
        }
        let zone = get_combat_zone(char.current_room_id);
        if zone == "safe" && !char.god_mode {
            send_client_message(connection_id, "A peaceful aura smothers the violence in your blood.");
            return;
        }
    }
    if effect == "light" && target_arg != "" {
        mob_id = find_power_target(char, target_arg);
        if mob_id == "" {
            send_client_message(connection_id, "You don't see them here to dazzle.");
            return;
        }
        let zone = get_combat_zone(char.current_room_id);
        if zone == "safe" && !char.god_mode {
            send_client_message(connection_id, "A peaceful aura smothers the violence in your blood.");
            return;
        }
    }

    let result = activate_pc_mutation(connection_id, mutation_id, mp);
    if !result["success"] {
        narrate_activation_error(connection_id, result);
        return;
    }

    // Reload: the activation mutated MP/HP/stats on the session copy.
    char = get_player_character(connection_id);

    let fizzled = false;
    if result["misfire"] {
        fizzled = narrate_misfire(connection_id, char, result);
    }
    if !fizzled {
        apply_power_effect(connection_id, char, info, result, mob_id);
    }

    let max_mp = get_pc_max_mp(connection_id);
    send_client_message(connection_id, "\x1b[90m[MP " + result["mp_left"] + "/" + max_mp + "]\x1b[0m");
}

fn show_mutation_list(connection_id) {
    let mp = get_pc_mp(connection_id);
    let max_mp = get_pc_max_mp(connection_id);
    let owned = get_pc_mutations(connection_id);

    let output = "\x1b[1;32mYour mutations\x1b[0m  (MP: " + mp + "/" + max_mp + " — refill with 'push')\n";
    if owned.len() == 0 {
        output += "  The Zone has given you nothing yet.\n";
    }
    for id in owned {
        let info = get_mutation_info(id);
        if info.len() == 0 { continue; }
        let tag = if info["activation"] == "active" { "\x1b[33m[active]\x1b[0m " } else { "\x1b[36m[passive]\x1b[0m" };
        output += "  \x1b[1m" + info["name"] + "\x1b[0m " + tag + "\n";
        output += "    " + info["description"] + "\n";
    }
    output += "\nUsage: mutate <power> [target] [mp]   (more MP = stronger, but riskier)";

    let deformities = get_pc_deformities(connection_id);
    if deformities.len() > 0 {
        output += "\n\n\x1b[35mThe power has marked you:\x1b[0m\n";
        for d in deformities {
            output += "  - " + d + "\n";
        }
    }
    send_client_message(connection_id, output);
}

// Prefix-match an owned mutation by id or display name.
fn resolve_owned_mutation(connection_id, power_arg) {
    let owned = get_pc_mutations(connection_id);
    for id in owned {
        if id.starts_with(power_arg) { return id; }
        let info = get_mutation_info(id);
        if info.len() > 0 {
            let name = info["name"];
            if name.to_lower().starts_with(power_arg) { return id; }
        }
        // Match second word too ("spit" for acid_spit, "legs" for frog_legs).
        let words = id.split('_');
        for w in words {
            if w.starts_with(power_arg) { return id; }
        }
    }
    return "";
}

fn find_power_target(char, target_arg) {
    let room_id = char.current_room_id;
    let mobs = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);

    if target_arg == "" {
        if is_in_combat(char.name) {
            let targets = get_combat_targets(char.name);
            if targets.len() > 0 { return targets[0]; }
        }
        return "";
    }

    for mob_id in mobs {
        let mob = get_mobile_data(mob_id);
        if type_of(mob) != "()" {
            if mob.flags.no_attack { continue; }
            if mob.name.to_lower().contains(target_arg) { return mob_id; }
            for kw in mob.keywords {
                if kw.to_lower().starts_with(target_arg) { return mob_id; }
            }
        }
    }
    return "";
}

fn narrate_activation_error(connection_id, result) {
    let error = result["error"];
    if error == "not enough mp" {
        send_client_message(connection_id, "The mutation stirs but finds nothing to feed on. (MP " + result["mp"] + " — push yourself to earn more.)");
        return;
    }
    if error == "passive" {
        send_client_message(connection_id, "That mutation works on its own.");
        return;
    }
    if error == "not owned" {
        send_client_message(connection_id, "The Zone never gave you that gift.");
        return;
    }
    send_client_message(connection_id, "The power slips away from you. (" + error + ")");
}

// Returns true when the power fizzled entirely (power_loss misfire).
fn narrate_misfire(connection_id, char, result) {
    let kind = result["misfire_kind"];
    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;31mThe power twists in your grip — MISFIRE!\x1b[0m");

    if kind == "power_loss" {
        send_client_message(connection_id, "\x1b[31mThe mutation gutters out. Every drop of power drains into nothing.\x1b[0m");
        broadcast_to_room(room_id, char.name + " convulses, and whatever was building inside them collapses.", char.name);
        return true;
    }
    if kind == "self_trauma" {
        send_client_message(connection_id, "\x1b[31mThe power tears through your own flesh on its way out. [-" + result["misfire_damage"] + " hp]\x1b[0m");
        broadcast_to_room(room_id, char.name + " screams as their own mutation savages them.", char.name);
        return false;
    }
    if kind == "deformity" {
        send_client_message(connection_id, "\x1b[35mSomething shifts under your skin and doesn't shift back: you now bear " + result["deformity"] + ".\x1b[0m");
        broadcast_to_room(room_id, char.name + "'s body warps visibly — something new and wrong settles into their flesh.", char.name);
        return false;
    }
    // Overload: permanent loss, new power.
    send_client_message(connection_id, "\x1b[1;35mOVERLOAD! The Zone pours through you like floodwater through a fence.\x1b[0m");
    if result["stat_lost"] != "" {
        send_client_message(connection_id, "\x1b[31mSomething in you burns out forever. (-1 " + result["stat_lost"].to_upper() + ")\x1b[0m");
    }
    if result["new_mutation"] != "" {
        send_client_message(connection_id, "\x1b[1;32mAnd something new wakes up: you have gained " + result["new_mutation_name"] + "!\x1b[0m");
    }
    if result["max_mp_increased"] {
        send_client_message(connection_id, "\x1b[1;32mThe Zone finds nothing new to give — so it deepens what is already there. (+1 max MP)\x1b[0m");
    }
    broadcast_to_room(room_id, char.name + " howls as raw mutation boils over, reshaping them before your eyes.", char.name);
    return false;
}

fn apply_power_effect(connection_id, char, info, result, mob_id) {
    let effect = info["effect"];
    if effect == "damage" {
        apply_damage_power(connection_id, char, info, result, mob_id, false);
        return;
    }
    if effect == "drain" {
        apply_damage_power(connection_id, char, info, result, mob_id, true);
        return;
    }
    if effect == "self_buff" {
        apply_self_buff_power(connection_id, char, info, result);
        return;
    }
    if effect == "heal" {
        apply_heal_power(connection_id, char, info, result);
        return;
    }
    if effect == "light" {
        apply_light_power(connection_id, char, info, result, mob_id);
        return;
    }
    send_client_message(connection_id, "The power surges and fades, formless.");
}

fn apply_damage_power(connection_id, char, info, result, mob_id, drain) {
    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "Your target is no longer here.");
        return;
    }
    let room_id = char.current_room_id;
    let damage = result["power"];
    if damage < 1 { damage = 1; }

    mob.hp -= damage;
    save_mobile_data(mob);
    record_mob_damaged_by(mob_id, char.name, damage);

    if !is_in_combat(char.name) {
        enter_combat(char.name, mob_id, "mobile");
    }
    enter_mobile_combat(mob_id, "player", char.name);

    let self_msg = info["msg_self"];
    self_msg.replace("{target}", mob.name);
    let room_msg = info["msg_room"];
    room_msg.replace("{target}", mob.name);
    room_msg.replace("{name}", char.name);
    send_client_message(connection_id, "\x1b[33m" + self_msg + "\x1b[0m \x1b[31m[" + damage + " " + info["damage_type"] + " damage]\x1b[0m");
    broadcast_to_room_except(room_id, room_msg, char.name);

    if drain {
        let healed = damage / 2;
        if healed > 0 {
            char.hp = char.hp + healed;
            if char.hp > char.max_hp { char.hp = char.max_hp; }
            save_character_data(char);
            send_client_message(connection_id, "\x1b[32mStolen life floods into you. [+" + healed + " hp]\x1b[0m");
        }
    }
}

fn apply_self_buff_power(connection_id, char, info, result) {
    let power = result["power"];
    let duration = result["duration_secs"];
    let room_id = char.current_room_id;

    if info["id"] == "manbeast" {
        apply_buff(char.name, "damage_bonus", power, duration, info["name"]);
        apply_buff(char.name, "hit_bonus", power, duration, info["name"]);
    } else {
        // extreme_reflexes and future speed-shaped powers.
        apply_buff(char.name, "haste", 1, duration, info["name"]);
    }

    send_client_message(connection_id, "\x1b[33m" + info["msg_self"] + "\x1b[0m \x1b[36m(" + duration + "s)\x1b[0m");
    let room_msg = info["msg_room"];
    room_msg.replace("{name}", char.name);
    broadcast_to_room(room_id, room_msg, char.name);
}

fn apply_heal_power(connection_id, char, info, result) {
    let healed = result["power"];
    char.hp = char.hp + healed;
    if char.hp > char.max_hp { char.hp = char.max_hp; }
    save_character_data(char);
    apply_buff(char.name, "regeneration", 1, 60, info["name"]);

    send_client_message(connection_id, "\x1b[32m" + info["msg_self"] + " [+" + healed + " hp]\x1b[0m");
    let room_msg = info["msg_room"];
    room_msg.replace("{name}", char.name);
    broadcast_to_room(char.current_room_id, room_msg, char.name);
}

fn apply_light_power(connection_id, char, info, result, mob_id) {
    let room_id = char.current_room_id;

    if mob_id == "" {
        apply_buff(char.name, "magic_light", 1, result["duration_secs"], info["name"]);
        send_client_message(connection_id, "\x1b[33m" + info["msg_self"] + "\x1b[0m \x1b[36m(" + result["duration_secs"] + "s)\x1b[0m");
        let room_msg = info["msg_room"];
        room_msg.replace("{name}", char.name);
        broadcast_to_room(room_id, room_msg, char.name);
        return;
    }

    // Targeted flare: try to blind.
    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "Your target is no longer here.");
        return;
    }
    let chance = result["power"];
    if roll_status_application(mob_id, "blind", chance) {
        apply_buff_to_mobile(mob_id, "blind", 0, 20, info["name"]);
        send_client_message(connection_id, "\x1b[1;33mYou flare blinding-bright! " + mob.name + " reels, sightless.\x1b[0m");
        broadcast_to_room_except(room_id, char.name + " flares like a cold green sun. " + mob.name + " staggers, blinded!", char.name);
    } else {
        send_client_message(connection_id, "\x1b[33mYou flare bright, but " + mob.name + " shields its eyes in time.\x1b[0m");
        broadcast_to_room_except(room_id, char.name + " flares with cold green light.", char.name);
    }
    enter_mobile_combat(mob_id, "player", char.name);
}
