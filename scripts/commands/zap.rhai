// zap <wand> [target] — fire a wand's bound spell at a target. Magic skill
// gated by the wand's `min_level`; consumes one charge. Wand stays in inventory
// when drained (a useless stick).

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if args == "" {
        send_client_message(connection_id, "Zap what?");
        return;
    }

    // Split "wand target" — first token is the wand keyword.
    let parts = args.split(" ");
    let wand_arg = parts[0];
    let target_arg = "";
    if parts.len() > 1 {
        target_arg = parts[1];
        for i in 2..parts.len() {
            target_arg += " " + parts[i];
        }
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(wand_arg, inv_items);
    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(wand_arg, eq_items);
    }
    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    if item.item_type != "wand" {
        send_client_message(connection_id, "That isn't a wand.");
        return;
    }

    let cou = item.cast_on_use;
    if type_of(cou) == "()" {
        send_client_message(connection_id, "The wand seems to be a dud.");
        return;
    }

    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < cou["min_level"] {
        send_client_message(connection_id, "Your magic skill is too weak to focus this wand. (Need magic level " + cou["min_level"] + ".)");
        return;
    }

    if cou["charges"] <= 0 {
        send_client_message(connection_id, "The wand is drained.");
        return;
    }

    let spell_id = cou["spell"];
    let spell = get_spell_info(spell_id);
    if type_of(spell) == "()" {
        send_client_message(connection_id, "The wand's magic fizzles.");
        decrement_item_charges(item.id);
        return;
    }

    let cd_remaining = get_spell_cooldown_remaining(char.name, spell_id);
    if cd_remaining > 0 {
        send_client_message(connection_id, spell["name"] + " is on cooldown for " + cd_remaining + " more seconds.");
        return;
    }

    // Consume the charge before firing — failed targeting still spends it.
    let remaining = decrement_item_charges(item.id);
    send_client_message(connection_id, "You aim " + item.short_desc + " and unleash " + spell["name"] + "!");
    broadcast_to_room_except(char.current_room_id, char.name + " unleashes " + spell["name"] + " from " + item.short_desc + "!", char.name);

    fire_wand_spell(connection_id, char, spell, target_arg);

    let effective_cd = resolve_item_spell_cooldown(cou, spell);
    if effective_cd > 0 {
        set_spell_cooldown(char.name, spell_id, effective_cd);
    }

    if remaining == 0 {
        send_client_message(connection_id, "The wand crumbles inertly — its last spark spent.");
    }
}

// Per-item cooldown override wins over the spell's own cooldown.
// `cou["cooldown_secs"]` is `()` when no override; positive int otherwise.
fn resolve_item_spell_cooldown(cou, spell) {
    let item_cd = cou["cooldown_secs"];
    if type_of(item_cd) != "()" && item_cd > 0 {
        return item_cd;
    }
    let spell_cd = spell["cooldown_secs"];
    if type_of(spell_cd) != "()" && spell_cd > 0 {
        return spell_cd;
    }
    return 0;
}

fn fire_wand_spell(connection_id, char, spell, target_arg) {
    let stype = spell["spell_type"];

    if stype == "damage" {
        let mob_id = find_mob_target_local(char, target_arg);
        if mob_id == "" {
            send_client_message(connection_id, "Your spell finds no target.");
            return;
        }
        let mob = get_mobile_data(mob_id);
        if type_of(mob) == "()" { return; }
        if mob.flags.no_attack {
            send_client_message(connection_id, "You cannot harm " + mob.name + ".");
            return;
        }
        let damage = spell["damage_base"];
        if damage < 1 { damage = 1; }
        mob.hp -= damage;
        save_mobile_data(mob);
        if !is_in_combat(char.name) {
            enter_combat(char.name, mob_id, "mobile");
        }
        enter_mobile_combat(mob_id, "player", char.name);
        send_client_message(connection_id, "Energy strikes " + mob.name + " for " + damage + " damage!");
        broadcast_to_room_except(char.current_room_id, char.name + "'s spell strikes " + mob.name + "!", char.name);
        return;
    }

    if stype == "debuff" {
        let mob_id = find_mob_target_local(char, target_arg);
        if mob_id == "" {
            send_client_message(connection_id, "Your spell finds no target.");
            return;
        }
        let mob = get_mobile_data(mob_id);
        if type_of(mob) == "()" { return; }
        let effect = spell["buff_effect"];
        let resisted = false;
        if effect == "sleep" && mob.flags.no_sleep { resisted = true; }
        if effect == "blind" && mob.flags.no_blind { resisted = true; }
        if !is_in_combat(char.name) {
            enter_combat(char.name, mob_id, "mobile");
        }
        enter_mobile_combat(mob_id, "player", char.name);
        if resisted {
            send_client_message(connection_id, mob.name + " resists the wand's magic!");
            return;
        }
        let mag = spell["buff_magnitude"];
        let dur = spell["buff_duration_secs"];
        apply_buff_to_mobile(mob_id, effect, mag, dur, spell["name"]);
        send_client_message(connection_id, "The magic settles over " + mob.name + ".");
        return;
    }

    if stype == "buff" || stype == "heal" {
        // Self-target wand. Reuse simple buff/heal model.
        let effect = spell["buff_effect"];
        if stype == "buff" && effect != "" && type_of(effect) != "()" {
            apply_buff(char.name, effect, spell["buff_magnitude"], spell["buff_duration_secs"], spell["name"]);
            send_client_message(connection_id, "The wand's magic settles over you.");
        }
        if stype == "heal" {
            let target = get_character_data(char.name);
            if type_of(target) == "()" { return; }
            let amt = spell["heal_base"];
            if amt < 1 { amt = 1; }
            target.hp += amt;
            if target.hp > target.max_hp { target.hp = target.max_hp; }
            save_character_data(target);
            let refreshed = get_character_data(char.name);
            if type_of(refreshed) != "()" {
                set_player_character(connection_id, refreshed);
            }
            send_client_message(connection_id, "The wand's magic mends you for " + amt + " HP.");
        }
        return;
    }

    send_client_message(connection_id, "The wand's spell has no usable effect.");
}

fn find_mob_target_local(char, target_arg) {
    let mobs = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
    if target_arg == "" {
        if is_in_combat(char.name) {
            let targets = get_combat_targets(char.name);
            if targets.len() > 0 { return targets[0]; }
        }
        return "";
    }
    let lower = target_arg.to_lower();
    for mob_id in mobs {
        let mob = get_mobile_data(mob_id);
        if type_of(mob) != "()" {
            if mob.name.to_lower().contains(lower) { return mob_id; }
            for kw in mob.keywords {
                if kw.to_lower().starts_with(lower) { return mob_id; }
            }
        }
    }
    return "";
}
