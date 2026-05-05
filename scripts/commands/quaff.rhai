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

    send_client_message(connection_id, "You quaff " + item.short_desc + ".");
    broadcast_to_room_except(char.current_room_id, char.name + " quaffs " + item.short_desc + ".", char.name);

    fire_potion_spell(connection_id, char, spell);

    delete_item(item.id);
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
        let old_hp = target.hp;
        target.hp += heal_amount;
        if target.hp > target.max_hp { target.hp = target.max_hp; }
        let actual = target.hp - old_hp;
        save_character_data(target);
        let refreshed = get_character_data(char.name);
        if type_of(refreshed) != "()" {
            set_player_character(connection_id, refreshed);
        }
        send_client_message(connection_id, "Warmth flows through you, healing " + actual + " HP.");
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
