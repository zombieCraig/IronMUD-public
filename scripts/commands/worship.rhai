// scripts/commands/worship.rhai
//
//   worship            - show your god and standing
//   worship <god>      - swear the pact: the god must be in the room, or you
//                        must invoke them by name in a temple
//
// The pact is gated by the god's builder config: a pact artifact (consumed
// on the spot) or a completed quest. There is no command to renounce a god —
// the pact is for life (admins can clear it with `admin religion`).

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to worship.");
        return;
    }

    if args == "" {
        show_worship_status(char, connection_id);
        return;
    }

    attempt_pact(char, args, connection_id);
}

fn show_worship_status(char, connection_id) {
    let god_vnum = get_worship_god(char.name);
    if god_vnum == "" {
        send_client_message(connection_id, "You worship no god. Stand before one — or invoke one in a temple — with 'worship <god>'.");
        return;
    }
    let name = get_god_display_name(god_vnum);
    if name == "" {
        name = "a forgotten god";
    }
    let epithet = get_deity_epithet(god_vnum);
    let output = "\x1b[1;36mYou are sworn to " + name;
    if epithet != "" {
        output += ", " + epithet;
    }
    output += ".\x1b[0m\n";

    let lore = get_deity_lore(god_vnum);
    if lore != "" {
        output += lore + "\n";
    }

    let overdue = get_worship_overdue_days(char.name);
    if overdue > 0 {
        output += "\x1b[33mTribute is " + overdue + " day(s) overdue. Pray at a temple before the wrath finds you.\x1b[0m\n";
    } else {
        output += "Your tribute is current.\n";
    }
    output += "Favor: " + get_worship_favor(char.name);
    send_client_message(connection_id, output);
}

// Find a deity mob in the actor's room matching the keyword. Returns its
// vnum or "".
fn find_deity_in_room(char, keyword, room_id) {
    let lower = keyword.to_lower();
    let mobs = get_mobiles_in_room(room_id);
    for mob_id in mobs {
        let mob = get_mobile_data(mob_id);
        if type_of(mob) != "()" && mob.is_deity {
            if mob.name.to_lower().contains(lower) {
                return mob.vnum;
            }
            for kw in mob.keywords {
                if kw.to_lower().starts_with(lower) {
                    return mob.vnum;
                }
            }
        }
    }
    return "";
}

fn attempt_pact(char, args, connection_id) {
    let room_id = char.current_room_id;
    let god_vnum = find_deity_in_room(char, args, room_id);
    if god_vnum == "" {
        let room = get_room_data(room_id);
        if room != () && room.flags.temple {
            god_vnum = find_deity_by_keyword(args);
        }
    }
    if god_vnum == "" {
        send_client_message(connection_id, "No such god stands before you. Seek their presence, or invoke their name in a temple.");
        return;
    }
    if !is_worshipable_god(god_vnum) {
        let name = get_god_display_name(god_vnum);
        if name == "" {
            name = "That being";
        }
        send_client_message(connection_id, name + " is no true god. Only the highest accept worship.");
        return;
    }

    let current = get_worship_god(char.name);
    if current != "" {
        let current_name = get_god_display_name(current);
        if current_name == "" {
            current_name = "your god";
        }
        if current == god_vnum {
            send_client_message(connection_id, "You are already sworn to " + current_name + ".");
        } else {
            send_client_message(connection_id, "You are sworn to " + current_name + ". The gods do not release their own.");
        }
        return;
    }

    let result = create_worship_pact(char.name, god_vnum);
    handle_pact_result(char, god_vnum, result, connection_id);
}

fn handle_pact_result(char, god_vnum, result, connection_id) {
    if result == "gate_failed" {
        send_client_message(connection_id, "You feel yourself weighed... and found unworthy. An artifact of the god, or a deed done in their name, must open this path.");
        return;
    }
    if result != "ok" {
        send_client_message(connection_id, "The moment passes. Nothing answers.");
        return;
    }

    let name = get_god_display_name(god_vnum);
    if name == "" {
        name = "the god";
    }
    let epithet = get_deity_epithet(god_vnum);
    let label = name;
    if epithet != "" {
        label += ", " + epithet;
    }
    send_client_message(connection_id, "\x1b[1;36mYou swear yourself to " + label + ". The pact settles into your soul — there is no taking it back.\x1b[0m");
    broadcast_to_room(char.current_room_id, char.name + " kneels and swears an oath to " + name + ".", char.name);

    // Let the god's own script greet the new faithful.
    let god_id = find_god_mobile_id(god_vnum);
    if god_id != "" {
        let context = #{ god_vnum: god_vnum };
        fire_mobile_trigger(god_id, "on_worship_pact", connection_id, context);
    }

    let outcome = apply_worship_blessing(char.name);
    if outcome == "blessed" {
        send_client_message(connection_id, "\x1b[1;36mDivine warmth flows through you. You are blessed.\x1b[0m");
    }
    send_client_message(connection_id, "Tribute will be expected. Pray at a temple to keep the faith.");
}
