// scripts/commands/pray.rhai
//
// Outside a temple (or for the godless) `pray` keeps its original meaning:
// a message relayed to the game's administrators via the chat bridge.
//
// In a room flagged `temple`, a worshiper's prayer reaches their god:
//   pray            - devotion; blessing if tribute is current, a nudge if not
//   pray tribute    - pay the god's tribute (default: gold percent) + blessing
//   pray atone      - massive payment that lifts wrath (incl. permanent smites)
//   pray petition   - ask to shape a spell (divine spellcraft); shows the
//                     requirements checklist until the worshipper is eligible
//   pray ritual     - once the petition's trial is done: begin the sealing rite
//   pray push|seal  - at a pause in the rite: deepen the gift, or inscribe it
//   pray aid <name> - lend mana to (and steady) someone's rite in this room
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

    // Drafting a spell: every line belongs to the petition.
    if get_olc_mode(connection_id) == "petitioning" {
        handle_petition_input(char, args, connection_id);
        return;
    }

    let room = get_room_data(char.current_room_id);
    let god_vnum = get_worship_god(char.name);
    let in_temple = room != () && room.flags.temple;

    // Divine spellcraft. While the system is off, "petition" is just words
    // of an ordinary prayer.
    if args.to_lower() == "petition" && spellcraft_enabled() {
        if god_vnum == "" {
            send_client_message(connection_id, "You must first be bound to a god.");
            return;
        }
        if !in_temple {
            send_client_message(connection_id, "Petitions are heard only in a temple.");
            return;
        }
        handle_petition(char, connection_id);
        return;
    }

    if spellcraft_enabled() && handle_ritual_words(char, args, connection_id) {
        return;
    }

    if in_temple && god_vnum != "" {
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

    // Relay to the chat bridge, at most once a minute per player so a
    // spammer can't crowd out other announcements. Over the limit the prayer
    // still shows locally; it just isn't relayed.
    chat_broadcast_limited("pray:" + char_name, 60, char_name + " prays to the gods: " + args);
}

// The sealing rite: 'pray ritual', 'pray push', 'pray seal', 'pray aid <name>'.
// Returns true when the words were a rite command.
fn handle_ritual_words(char, args, connection_id) {
    let words = args.split(" ");
    let sub = words[0].to_lower();
    if sub == "ritual" {
        send_client_message(connection_id, ritual_begin(connection_id));
        return true;
    }
    if sub == "push" {
        send_client_message(connection_id, ritual_push(connection_id));
        return true;
    }
    if sub == "seal" {
        send_client_message(connection_id, ritual_seal(connection_id));
        return true;
    }
    if sub == "aid" {
        if words.len() < 2 || words[1] == "" {
            send_client_message(connection_id, "Aid whose rite?");
            return true;
        }
        send_client_message(connection_id, ritual_aid(connection_id, words[1]));
        return true;
    }
    if sub == "restore" {
        if words.len() < 2 || words[1] == "" {
            send_client_message(connection_id, "Restore which faded spell?");
            return true;
        }
        let name = "";
        let i = 1;
        while i < words.len() {
            if name == "" { name = words[i]; } else { name = name + " " + words[i]; }
            i += 1;
        }
        send_client_message(connection_id, pray_restore(connection_id, name));
        return true;
    }
    return false;
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

// `pray petition`: the requirements checklist, from the same Rust function
// that decides eligibility, so the list and the verdict cannot disagree.
fn handle_petition(char, connection_id) {
    let status = petition_checklist(char.name);
    if !status.worships {
        send_client_message(connection_id, "You must first be bound to a god.");
        return;
    }
    let god = status.god_name;
    if !status.god_grants {
        send_client_message(connection_id, god + " grants no spells to mortals.");
        return;
    }
    let pending = petition_pending(char.name);
    if pending != "" {
        send_client_message(connection_id, pending);
        return;
    }
    if status.can_petition {
        let opened = petition_begin(connection_id);
        if opened != "" {
            send_client_message(connection_id, opened);
            return;
        }
    }
    let out = god + " regards your petition. To shape a gift, you must stand where few have stood:\n";
    for r in status.requirements {
        let mark = if r.met { "\x1b[1;32m[x]\x1b[0m" } else { "\x1b[0;31m[ ]\x1b[0m" };
        let label = r.label;
        while label.len() < 38 { label += " "; }
        out += "  " + mark + " " + label + " " + r.detail + "\n";
    }
    send_client_message(connection_id, out);
}

// A line typed while drafting. `offer` puts the draft to the god's OnPray
// triggers (action "petition") first; a DG body returning 0 refuses it.
fn handle_petition_input(char, line, connection_id) {
    let r = petition_input(connection_id, line);
    if r.text != "" {
        send_client_message(connection_id, r.text);
    }
    if !r.offer {
        return;
    }
    let god_vnum = get_worship_god(char.name);
    let god_id = find_god_mobile_id(god_vnum);
    if god_id != "" {
        let context = #{
            action: "petition",
            overdue_days: get_worship_overdue_days(char.name),
            god_vnum: god_vnum,
            spell_name: r.name,
            spell_type: r.spell_type,
            power: r.power,
            tier: r.tier
        };
        if fire_mobile_trigger(god_id, "on_pray", connection_id, context) == "cancel" {
            send_client_message(connection_id, god_label(god_vnum) + " turns away from your petition. Change it, or 'abort'.");
            return;
        }
    }
    broadcast_to_room(char.current_room_id, char.name + " kneels and lays a petition before the altar.", char.name);
    send_client_message(connection_id, petition_offer(connection_id));
}
