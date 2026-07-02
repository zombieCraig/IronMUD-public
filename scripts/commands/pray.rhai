// scripts/commands/pray.rhai
//
// Outside a temple (or for the godless) `pray` keeps its original meaning:
// a message relayed to the game's administrators via the chat bridge.
//
// In a room flagged `temple`, a worshiper's prayer reaches their god:
//   pray            - devotion; blessing if tribute is current, a nudge if not
//   pray tribute    - pay the god's tribute (default: gold percent) + blessing
//   pray atone      - massive payment that lifts wrath (incl. permanent smites)
//
// The god's OnPray DG trigger (fired at a live instance when spawned, else
// the prototype) sees context vars `action` and `overdue_days`; returning 0
// from the trigger cancels the default handling so builders can script
// exotic tribute rules (blood, sacrifices, tasks) via worship_* DG commands.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to pray.");
        return;
    }

    let room = get_room_data(char.current_room_id);
    let god_vnum = get_worship_god(char.name);
    if room != () && room.flags.temple && god_vnum != "" {
        handle_temple_pray(char, god_vnum, args, connection_id);
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Pray what?");
        return;
    }

    let char_name = char.name;
    let room_id = char.current_room_id;

    // Show to actor
    send_client_message(connection_id, "You pray to the gods: " + args);

    // Broadcast to others in room
    broadcast_to_room(room_id, char_name + " prays to the gods.", char_name);

    // Send to Matrix
    matrix_broadcast(char_name + " prays to the gods: " + args);
}

fn handle_temple_pray(char, god_vnum, args, connection_id) {
    let sub = args.to_lower();
    if sub == "tribute" {
        handle_tribute(char, god_vnum, connection_id);
        return;
    }
    if sub == "atone" {
        handle_atone(char, god_vnum, connection_id);
        return;
    }
    handle_devotion(char, god_vnum, connection_id);
}

// Fire the god's OnPray triggers. Returns "cancel" when a DG body returned 0.
fn fire_god_pray_trigger(char, god_vnum, action, connection_id) {
    let god_id = find_god_mobile_id(god_vnum);
    if god_id == "" {
        return "continue";
    }
    let context = #{
        action: action,
        overdue_days: get_worship_overdue_days(char.name),
        god_vnum: god_vnum
    };
    return fire_mobile_trigger(god_id, "on_pray", connection_id, context);
}

fn god_label(god_vnum) {
    let name = get_god_display_name(god_vnum);
    if name == "" {
        return "your god";
    }
    let epithet = get_deity_epithet(god_vnum);
    if epithet == "" {
        return name;
    }
    return name + ", " + epithet;
}

fn handle_devotion(char, god_vnum, connection_id) {
    let label = god_label(god_vnum);
    send_client_message(connection_id, "You kneel and offer your devotion to " + label + ".");
    broadcast_to_room(char.current_room_id, char.name + " kneels in prayer.", char.name);

    if fire_god_pray_trigger(char, god_vnum, "pray", connection_id) == "cancel" {
        return;
    }

    let overdue = get_worship_overdue_days(char.name);
    if overdue > 0 {
        send_client_message(connection_id, "\x1b[33mYour prayer meets cold silence. Tribute is " + overdue + " day(s) overdue. Offer it with 'pray tribute'.\x1b[0m");
        return;
    }

    let outcome = apply_worship_blessing(char.name);
    if outcome == "blessed" {
        send_client_message(connection_id, "\x1b[1;36mDivine warmth flows through you. You are blessed.\x1b[0m");
    } else {
        send_client_message(connection_id, "You feel a quiet acknowledgment. Nothing more.");
    }
}

fn handle_tribute(char, god_vnum, connection_id) {
    broadcast_to_room(char.current_room_id, char.name + " makes an offering at the altar.", char.name);

    if fire_god_pray_trigger(char, god_vnum, "tribute", connection_id) == "cancel" {
        return;
    }

    let amount = take_tribute_gold_percent(char.name);
    if amount < 0 {
        send_client_message(connection_id, "Your offering finds no ear.");
        return;
    }
    let label = god_label(god_vnum);
    if amount > 0 {
        send_client_message(connection_id, "You give " + amount + " gold in tribute to " + label + ".");
    } else {
        send_client_message(connection_id, "You have little, and " + label + " accepts the gesture all the same.");
    }
    let outcome = apply_worship_blessing(char.name);
    if outcome == "blessed" {
        send_client_message(connection_id, "\x1b[1;36mDivine warmth flows through you. You are blessed.\x1b[0m");
    } else {
        send_client_message(connection_id, "Your standing with " + label + " is restored.");
    }
}

fn handle_atone(char, god_vnum, connection_id) {
    let label = god_label(god_vnum);
    let paid = atone_worship(char.name);
    if paid == -2 {
        send_client_message(connection_id, "You have too little for " + label + " to take your contrition seriously.");
        return;
    }
    if paid < 0 {
        send_client_message(connection_id, "Your plea finds no ear.");
        return;
    }
    broadcast_to_room(char.current_room_id, char.name + " prostrates themselves before the altar.", char.name);
    send_client_message(connection_id, "\x1b[1;36mYou surrender " + paid + " gold in atonement. The weight of divine wrath lifts from you.\x1b[0m");
    apply_worship_blessing(char.name);
}
