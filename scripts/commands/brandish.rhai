// brandish <staff> — invoke a staff's bound spell. Staves are area-style:
// damage/debuff spells hit every hostile in the room; buff/heal spells fire
// once on the wielder. Magic skill gated by `min_level`; consumes one charge.

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
        send_client_message(connection_id, "Brandish what?");
        return;
    }

    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);
    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(args, eq_items);
    }
    if item == () {
        send_client_message(connection_id, "You don't have that.");
        return;
    }

    if item.item_type != "staff" {
        send_client_message(connection_id, "That isn't a staff.");
        return;
    }

    let cou = item.cast_on_use;
    if type_of(cou) == "()" {
        send_client_message(connection_id, "The staff seems to be a dud.");
        return;
    }

    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < cou["min_level"] {
        send_client_message(connection_id, "Your magic skill is too weak to channel this staff. (Need magic level " + cou["min_level"] + ".)");
        return;
    }

    if cou["charges"] <= 0 {
        send_client_message(connection_id, "The staff is drained.");
        return;
    }

    let spell_id = cou["spell"];
    let spell = get_spell_info(spell_id);
    if type_of(spell) == "()" {
        send_client_message(connection_id, "The staff's magic fizzles.");
        decrement_item_charges(item.id);
        return;
    }

    let remaining = decrement_item_charges(item.id);
    send_client_message(connection_id, "You raise " + item.short_desc + " and channel " + spell["name"] + "!");
    broadcast_to_room_except(char.current_room_id, char.name + " channels " + spell["name"] + " through " + item.short_desc + "!", char.name);

    fire_staff_spell(connection_id, char, spell);

    if remaining == 0 {
        send_client_message(connection_id, "The staff crumbles inertly — its final charge spent.");
    }
}

fn fire_staff_spell(connection_id, char, spell) {
    let stype = spell["spell_type"];

    if stype == "damage" || stype == "debuff" {
        let mobs = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
        let hit_count = 0;
        for mob_id in mobs {
            let mob = get_mobile_data(mob_id);
            if type_of(mob) == "()" { continue; }
            if mob.flags.no_attack { continue; }
            if stype == "damage" {
                let damage = spell["damage_base"];
                if damage < 1 { damage = 1; }
                mob.hp -= damage;
                save_mobile_data(mob);
                if !is_in_combat(char.name) {
                    enter_combat(char.name, mob_id, "mobile");
                }
                enter_mobile_combat(mob_id, "player", char.name);
                send_client_message(connection_id, "Energy strikes " + mob.name + " for " + damage + " damage!");
                hit_count += 1;
            } else {
                let effect = spell["buff_effect"];
                let resisted = false;
                if effect == "sleep" && mob.flags.no_sleep { resisted = true; }
                if effect == "blind" && mob.flags.no_blind { resisted = true; }
                if !is_in_combat(char.name) {
                    enter_combat(char.name, mob_id, "mobile");
                }
                enter_mobile_combat(mob_id, "player", char.name);
                if resisted {
                    send_client_message(connection_id, mob.name + " resists the staff's magic!");
                } else {
                    apply_buff_to_mobile(mob_id, effect, spell["buff_magnitude"], spell["buff_duration_secs"], spell["name"]);
                    send_client_message(connection_id, "The magic settles over " + mob.name + ".");
                    hit_count += 1;
                }
            }
        }
        if hit_count == 0 && stype == "damage" {
            send_client_message(connection_id, "The staff's magic finds no target.");
        }
        return;
    }

    if stype == "buff" || stype == "utility" {
        let effect = spell["buff_effect"];
        if effect == "" || type_of(effect) == "()" {
            send_client_message(connection_id, "The staff's magic disperses harmlessly.");
            return;
        }
        apply_buff(char.name, effect, spell["buff_magnitude"], spell["buff_duration_secs"], spell["name"]);
        send_client_message(connection_id, "The staff's magic settles over you (" + spell["name"] + ").");
        return;
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
        send_client_message(connection_id, "The staff's magic mends you for " + amt + " HP.");
        return;
    }

    send_client_message(connection_id, "The staff's spell has no usable effect.");
}
