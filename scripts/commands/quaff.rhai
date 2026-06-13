// quaff <potion> — drink a magical potion to fire its bound spell on yourself.
// Universal access (no skill gate); single-use (delete on use).

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
        send_client_message(connection_id, "Quaff what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    if item.item_type != "potion" {
        send_client_message(connection_id, "You can only quaff potions. Try `drink` for liquids.");
        return;
    }

    let cou = item.cast_on_use;
    if type_of(cou) == "()" {
        send_client_message(connection_id, "There's nothing to quaff in " + item.short_desc + ".");
        return;
    }

    let spell_id = cou["spell"];
    let spell = get_spell_info(spell_id);
    if type_of(spell) == "()" {
        send_client_message(connection_id, "The potion fizzles without effect.");
        delete_item(item.id);
        return;
    }

    let cd_remaining = get_spell_cooldown_remaining(char.name, spell_id);
    if cd_remaining > 0 {
        send_client_message(connection_id, spell["name"] + " is on cooldown for " + cd_remaining + " more seconds.");
        return;
    }

    send_client_message(connection_id, "You quaff " + item.short_desc + ".");
    broadcast_to_room_except(char.current_room_id, char.name + " quaffs " + item.short_desc + ".", char.name);

    fire_potion_spell(connection_id, char, spell);

    let effective_cd = resolve_item_spell_cooldown(cou, spell);
    if effective_cd > 0 {
        set_spell_cooldown(char.name, spell_id, effective_cd);
    }

    delete_item(item.id);
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

// Fire spell at self based on spell_type. Potions support heal/buff/utility/debuff
// (a poison potion is a self-debuff). Damage spells in potion form are oddities
// — we still allow them but warn the player.
fn fire_potion_spell(connection_id, char, spell) {
    let stype = spell["spell_type"];

    if stype == "heal" {
        let target = get_character_data(char.name);
        if type_of(target) == "()" { return; }
        let magic_level = get_skill_level(char.name, "magic");
        let effective_int = get_effective_stat(char.name, "int");
        let int_bonus = if effective_int > 10 { effective_int - 10 } else { 0 };
        let heal_amount = spell["heal_base"] + (spell["heal_per_skill"] * magic_level) + (spell["heal_int_scaling"] * int_bonus);
        if heal_amount < 1 { heal_amount = 1; }
        // Organic medicine barely takes on a polymer chassis: 25%, min 1.
        let is_synth = is_character_synth(char.name);
        if is_synth {
            heal_amount = scale_heal_for_synth(heal_amount);
        }
        let old_hp = target.hp;
        target.hp += heal_amount;
        if target.hp > target.max_hp { target.hp = target.max_hp; }
        let actual = target.hp - old_hp;
        save_character_data(target);
        let refreshed = get_character_data(char.name);
        if type_of(refreshed) != "()" {
            set_player_character(connection_id, refreshed);
        }
        if is_synth {
            send_client_message(connection_id, "The draught pools uselessly in synthetic plumbing — " + actual + " HP. Repairs would serve better.");
        } else {
            send_client_message(connection_id, "Warmth flows through you, healing " + actual + " HP.");
        }
        return;
    }

    if stype == "buff" || stype == "utility" {
        let effect = spell["buff_effect"];
        if effect == "" || type_of(effect) == "()" {
            // utility with no buff_effect (e.g. dispel, light) — fall back to noop
            send_client_message(connection_id, "The potion's magic disperses harmlessly.");
            return;
        }
        let mag = spell["buff_magnitude"];
        let dur = spell["buff_duration_secs"];
        apply_buff(char.name, effect, mag, dur, spell["name"]);
        send_client_message(connection_id, "The potion's magic settles over you (" + spell["name"] + ").");
        return;
    }

    if stype == "debuff" {
        let effect = spell["buff_effect"];
        let mag = spell["buff_magnitude"];
        let dur = spell["buff_duration_secs"];
        apply_buff(char.name, effect, mag, dur, spell["name"]);
        send_client_message(connection_id, "The potion's bitter draught afflicts you with " + spell["name"] + ".");
        return;
    }

    // Fear in potion form frightens the drinker. Routes through the Rust
    // chokepoint (try_apply_fear) so immunities (synth, courage, frenzy)
    // and status resistance hold.
    if stype == "fear" {
        let outcome = try_apply_fear(char.name, 100, spell["buff_magnitude"], spell["buff_duration_secs"], spell["name"]);
        if outcome == "applied" {
            send_client_message(connection_id, "\x1b[1;31mCold dread crawls up your spine!\x1b[0m");
            broadcast_to_room_except(char.current_room_id, connection_id, char.name + " blanches in sudden terror!");
        } else {
            send_client_message(connection_id, "A chill passes through you, but finds nothing to grip.");
        }
        return;
    }

    if stype == "damage" {
        let target = get_character_data(char.name);
        if type_of(target) == "()" { return; }
        let damage = spell["damage_base"];
        if damage < 1 { damage = 1; }
        target.hp -= damage;
        save_character_data(target);
        let refreshed = get_character_data(char.name);
        if type_of(refreshed) != "()" {
            set_player_character(connection_id, refreshed);
        }
        send_client_message(connection_id, "The potion sears you for " + damage + " damage!");
        return;
    }

    send_client_message(connection_id, "The potion's magic has no effect on you.");
}
