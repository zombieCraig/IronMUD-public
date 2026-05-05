// scripts/commands/ask.rhai
// Player-visible relationship query: `ask <mobile> about <other>`.
// Reports the subject NPC's feelings toward a named acquaintance based on
// the stored affinity value. Only subjects with a SocialState (simulated
// migrant mobiles) can answer.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Ask whom about what?");
        send_client_message(connection_id, "Usage: ask <mobile> about <other>");
        return;
    }

    let parts = split_ask_args(args);
    if parts.len() != 2 {
        send_client_message(connection_id, "Usage: ask <mobile> about <other>");
        return;
    }
    let subject_keyword = parts[0].to_lower();
    let target_keyword = parts[1].to_lower();

    let room_id = char.current_room_id;
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
    let subject = find_mobile_by_keyword(subject_keyword, mobiles);

    if subject == () {
        send_client_message(connection_id, "You don't see '" + parts[0] + "' here.");
        return;
    }

    // Subject must be simulated (social present) to have opinions.
    let happiness = get_mobile_happiness(subject.id);
    if happiness < 0 {
        send_client_message(
            connection_id,
            subject.name + " gives you a blank look and says nothing."
        );
        return;
    }

    // Broadcast the question so others in the room can overhear it.
    let char_name = char.name;
    send_client_message(
        connection_id,
        "You ask " + subject.name + " about '" + target_keyword + "'."
    );
    broadcast_to_room(
        room_id,
        char_name + " asks " + subject.name + " about '" + target_keyword + "'.",
        char_name
    );

    let rel = describe_mobile_relationship(subject.id, target_keyword);
    if rel == () {
        let line = subject.name + " says: \"I don't know anyone by that name.\"";
        send_client_message(connection_id, line);
        broadcast_to_room(room_id, line, char_name);
        return;
    }

    let reply = feelings_reply(rel.name, rel.affinity, rel.kind);
    let line = subject.name + " says: \"" + reply + "\"";
    send_client_message(connection_id, line);
    broadcast_to_room(room_id, line, char_name);
}

// Split the raw args on the literal word " about ". Returns an empty array
// if the pattern isn't present. Case-insensitive match so "ABOUT" works too.
fn split_ask_args(raw) {
    let lower = raw.to_lower();
    let idx = lower.index_of(" about ");
    if idx < 0 { return []; }
    let subj = raw.sub_string(0, idx);
    let rest_start = idx + 7; // length of " about "
    let targ = raw.sub_string(rest_start, raw.len() - rest_start);
    if subj == "" || targ == "" { return []; }
    return [subj, targ];
}

// Map affinity to a natural-language reply. Cohabitant kind adds a housing
// flourish on top of the affinity-based sentiment.
fn feelings_reply(target_name, affinity, kind) {
    let sentiment = feelings_sentiment(target_name, affinity);
    if kind == "cohabitant" {
        return sentiment + " We share a home.";
    }
    return sentiment;
}

fn feelings_sentiment(target_name, affinity) {
    if affinity >= 80 { return target_name + " means the world to me."; }
    if affinity >= 50 { return target_name + " is a dear friend of mine."; }
    if affinity >= 20 { return "I like " + target_name + " well enough."; }
    if affinity > -20 { return "I know " + target_name + ". That's about all I can say."; }
    if affinity > -50 { return target_name + " and I don't really get along."; }
    if affinity > -80 { return "I can't stand " + target_name + "."; }
    return "I hate " + target_name + ". I wish they'd leave.";
}
