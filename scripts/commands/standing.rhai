// scripts/commands/standing.rhai
//
//   standing            - how every faction you have dealt with regards you
//   standing <faction>  - the detail for one of them, including its rivals
//
// Standing moves when you kill a faction's members (down), kill the members
// of a faction it opposes (up), or finish its quests and conversations. A
// faction you have never dealt with is Neutral and does not appear in the
// list — the list is a record of what you have done, not a directory.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to check your standing.");
        return;
    }

    let word = text::normalise(args);
    if word == "" {
        show_all_standings(char, connection_id);
        return;
    }

    show_one_standing(char, word, connection_id);
}

fn show_all_standings(char, connection_id) {
    let factions = get_reputation_factions(char.name);
    if factions.len() == 0 {
        send_client_message(
            connection_id,
            "No faction has an opinion of you yet. Fight for one, or against one, and that will change."
        );
        return;
    }

    let output = "\x1b[1;36mWhere you stand:\x1b[0m\n";
    for key in factions {
        let name = get_faction_name(key);
        let value = get_reputation(char.name, key);
        let label = get_reputation_label(char.name, key);
        output += "  " + text::pad_right(name, 24) + standing_color(char, key) + text::pad_right(label, 14)
            + "\x1b[0m" + render_standing_bar(value) + " " + value + "\n";
    }
    output += "\nUse 'standing <faction>' for what a standing means and who it costs you.";
    send_client_message(connection_id, output);
}

fn show_one_standing(char, keyword, connection_id) {
    // Match on the faction key. Players see display names, so accept a
    // prefix of either — "iron" should find iron_guard whether the world
    // calls it that or "The Iron Guard".
    let key = resolve_faction(char, keyword);
    if key == "" {
        send_client_message(connection_id, "You have had no dealings with any faction by that name.");
        return;
    }

    let name = get_faction_name(key);
    let value = get_reputation(char.name, key);
    let output = "\x1b[1;36m" + name + "\x1b[0m\n";

    let lore = get_faction_description(key);
    if lore != "" {
        output += lore + "\n";
    }

    output += "Standing: " + standing_color(char, key) + get_reputation_label(char.name, key)
        + "\x1b[0m (" + value + ")\n";
    output += get_reputation_description(char.name, key) + "\n";

    let foes = get_faction_opposed(key);
    if foes.len() > 0 {
        let line = "";
        for f in foes {
            if line != "" { line += ", "; }
            line += get_faction_name(f);
        }
        output += "\x1b[33mAt odds with: " + line + "\x1b[0m\n";
        output += "Earning their goodwill costs you standing here, and the reverse.";
    }
    send_client_message(connection_id, output);
}

// Resolve a player-typed word to a faction key the character actually has a
// standing with. Exact key first, then a prefix of the key, then a prefix of
// the display name.
fn resolve_faction(char, keyword) {
    let want = keyword.to_lower();
    let factions = get_reputation_factions(char.name);
    for key in factions {
        if key == want { return key; }
    }
    for key in factions {
        if key.starts_with(want) { return key; }
    }
    for key in factions {
        if get_faction_name(key).to_lower().starts_with(want) { return key; }
    }
    return "";
}

// Ten cells spanning the whole ladder, so half full is Neutral and which side
// of nothing you are on reads at a glance. The band name and the raw number
// beside it carry the precision; this is only the shape.
fn render_standing_bar(value) {
    let filled = (value + 1000) / 200;
    if filled < 0 { filled = 0; }
    if filled > 10 { filled = 10; }
    let bar = "[";
    let i = 0;
    while i < 10 {
        if i < filled { bar += "#"; } else { bar += "-"; }
        i += 1;
    }
    return bar + "]";
}

// Colour by band key, not by re-typed thresholds. `src/tiers.rs` owns where
// the bands fall; a second copy of ±200 here is exactly the drift the shared
// ladder was extracted to prevent, and it would go quietly wrong the first
// time someone retunes the ladder.
fn standing_color(char, key) {
    let tier = get_reputation_tier(char.name, key);
    if tier == "hated" || tier == "hostile" { return "\x1b[1;31m"; }
    if tier == "disliked" { return "\x1b[31m"; }
    if tier == "honored" || tier == "revered" { return "\x1b[1;32m"; }
    if tier == "accepted" { return "\x1b[32m"; }
    return "";
}
