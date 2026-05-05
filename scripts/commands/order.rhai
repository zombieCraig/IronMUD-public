// scripts/commands/order.rhai
//
// Direct a charmed mobile. v2 syntax:
//   order <mob_keyword> attack <target_keyword>
//   order <mob_keyword> stay
//   order <mob_keyword> follow [<player>]
//   order <mob_keyword> drop
//   order group <attack <target>|stay|follow|drop>
//
// All same-room subjects only — `order group` dispatches to mobs charmed by
// the caster who happen to share the caster's current room.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if type_of(char) == "()" { return; }

    let parts = args.split(" ");
    let cleaned = [];
    for p in parts {
        if p != "" { cleaned.push(p); }
    }
    if cleaned.len() < 2 {
        send_client_message(connection_id, "Usage: order <mobile|group> <stay|follow|attack|drop> [target]");
        return;
    }

    let first = cleaned[0];
    if first.to_lower() == "group" {
        handle_group(connection_id, char, cleaned);
        return;
    }

    handle_single(connection_id, char, cleaned);
}

fn handle_single(connection_id, char, cleaned) {
    let mob_keyword = cleaned[0];
    let subcommand = cleaned[1].to_lower();

    let room_id = char.current_room_id;
    let mob_id = find_charmed_mob(char, mob_keyword, room_id);
    if mob_id == "" {
        send_client_message(connection_id, "No charmed creature here matches '" + mob_keyword + "'.");
        return;
    }

    let mob = get_mobile_data(mob_id);
    if type_of(mob) == "()" {
        send_client_message(connection_id, "That creature is no longer here.");
        return;
    }

    if mobile_charm_master(mob_id).to_lower() != char.name.to_lower() {
        send_client_message(connection_id, "You have no authority over " + mob.short_desc + ".");
        return;
    }

    let arg = if cleaned.len() >= 3 { cleaned[2] } else { "" };
    dispatch_subcommand(connection_id, char, mob, subcommand, arg);
}

fn handle_group(connection_id, char, cleaned) {
    if cleaned.len() < 2 {
        send_client_message(connection_id, "Usage: order group <stay|follow|attack <target>|drop>");
        return;
    }
    let subcommand = cleaned[1].to_lower();
    let arg = if cleaned.len() >= 3 { cleaned[2] } else { "" };

    let room_id = char.current_room_id;
    let charmed = list_charmed_mobs_in_room(char.name, room_id);
    if charmed.len() == 0 {
        send_client_message(connection_id, "You have no charmed creatures here.");
        return;
    }

    let count = 0;
    for mob in charmed {
        dispatch_subcommand(connection_id, char, mob, subcommand, arg);
        count += 1;
    }
    if count > 1 {
        send_client_message(connection_id, "(Order issued to " + count + " creatures.)");
    }
}

fn dispatch_subcommand(connection_id, char, mob, subcommand, arg) {
    if subcommand == "attack" || subcommand == "kill" {
        if arg == "" {
            send_client_message(connection_id, "Order " + mob.short_desc + " to attack whom?");
            return;
        }
        order_attack(connection_id, char, mob, arg);
        return;
    }
    if subcommand == "stay" {
        order_stay(connection_id, char, mob);
        return;
    }
    if subcommand == "follow" {
        order_follow(connection_id, char, mob, arg);
        return;
    }
    if subcommand == "drop" || subcommand == "release" || subcommand == "free" {
        order_drop(connection_id, char, mob);
        return;
    }
    send_client_message(connection_id, "Unknown order. Try: order <mob> attack|stay|follow|drop");
}

fn find_charmed_mob(char, keyword, room_id) {
    let mobs = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    let lower = keyword.to_lower();
    let master_lower = char.name.to_lower();
    for mob_id in mobs {
        let m = get_mobile_data(mob_id);
        if type_of(m) == "()" { continue; }
        if mobile_charm_master(mob_id).to_lower() != master_lower { continue; }
        if m.name.to_lower().contains(lower) { return mob_id; }
        for kw in m.keywords {
            if kw.to_lower().starts_with(lower) { return mob_id; }
        }
    }
    return "";
}

fn order_stay(connection_id, char, mob) {
    set_charm_stay(mob.id, true);
    send_client_message(connection_id, mob.short_desc + " plants its feet and waits.");
}

fn order_follow(connection_id, char, mob, target_keyword) {
    set_charm_stay(mob.id, false);

    if target_keyword == "" || target_keyword.to_lower() == char.name.to_lower() || target_keyword.to_lower() == "me" {
        // Resume following master (clear any override).
        set_charm_follow_player(mob.id, "");
        send_client_message(connection_id, mob.short_desc + " falls back into step beside you.");
        return;
    }

    // Resolve a player target — must be online so we have a canonical name.
    let target_conn = find_player_connection_by_name(target_keyword);
    if target_conn == "" {
        send_client_message(connection_id, "You see no one called '" + target_keyword + "' online.");
        return;
    }
    let target_char = get_player_character(target_conn);
    if type_of(target_char) == "()" {
        send_client_message(connection_id, "You see no one called '" + target_keyword + "' online.");
        return;
    }
    if target_char.name.to_lower() == char.name.to_lower() {
        set_charm_follow_player(mob.id, "");
        send_client_message(connection_id, mob.short_desc + " falls back into step beside you.");
        return;
    }

    set_charm_follow_player(mob.id, target_char.name);
    send_client_message(connection_id, mob.short_desc + " turns its attention to " + target_char.name + ".");
}

fn order_drop(connection_id, char, mob) {
    remove_buff_from_mobile(mob.id, "charmed");
    set_charm_stay(mob.id, false);
    set_charm_follow_player(mob.id, "");
    send_client_message(connection_id, mob.short_desc + "'s eyes clear as you release your hold.");
    broadcast_to_room_except(char.current_room_id, connection_id, mob.short_desc + "'s eyes clear, free again.");
}

fn order_attack(connection_id, char, mob, target_keyword) {
    let room_id = char.current_room_id;
    let lower = target_keyword.to_lower();

    // Refuse to turn the mob on its master.
    if char.name.to_lower().starts_with(lower) || lower == "self" || lower == "me" {
        send_client_message(connection_id, mob.short_desc + " looks confused and refuses.");
        return;
    }

    // Try mob target in room.
    let mobs = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    for tid in mobs {
        if tid == mob.id { continue; }
        let t = get_mobile_data(tid);
        if type_of(t) == "()" { continue; }
        let matched = t.name.to_lower().contains(lower);
        if !matched {
            for kw in t.keywords {
                if kw.to_lower().starts_with(lower) { matched = true; break; }
            }
        }
        if matched {
            enter_mobile_combat(mob.id, "mobile", tid);
            send_client_message(connection_id, mob.short_desc + " moves to attack " + t.short_desc + " at your command.");
            broadcast_to_room_except(room_id, connection_id, mob.short_desc + " moves to attack " + t.short_desc + ".");
            return;
        }
    }

    // Try player target in room (excluding the master).
    let players = get_characters_in_room(room_id);
    for p in players {
        if p.name.to_lower() == char.name.to_lower() { continue; }
        if p.name.to_lower().starts_with(lower) {
            enter_mobile_combat(mob.id, "player", p.name);
            send_client_message(connection_id, mob.short_desc + " moves to attack " + p.name + " at your command.");
            broadcast_to_room_except(room_id, connection_id, mob.short_desc + " moves to attack " + p.name + ".");
            return;
        }
    }

    send_client_message(connection_id, "No such target here.");
}
