// scripts/commands/world.rhai
//
//   world             - the world's rating, what is holding it back, and why
//   world milestones  - the wall: what the world has crossed, and what is next
//
// `build audit world` answers "what is broken". This answers the other
// question: how far along is this? The rating is a weighted composite of five
// things — size, density, depth, quality, connectivity — reported as a named
// tier, because "Village" tells an operator where they are and `41` does not.
//
// Some absences hold the rating back regardless of everything else. A world
// with no quests is not a Town however many rooms it has, and the cap says so
// outright rather than letting a strong showing elsewhere average past it.
//
// Rating and facts come from a five-minute scan, not from this command.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let word = text::normalise(args).to_lower();
    if word == "milestones" || word == "wall" || word == "history" {
        show_wall(connection_id);
        return;
    }
    show_rating(connection_id);
}

fn tier_colour(score) {
    if score >= 80 { return "\x1b[1;35m"; }
    if score >= 65 { return "\x1b[1;36m"; }
    if score >= 50 { return "\x1b[1;32m"; }
    if score >= 22 { return "\x1b[1;33m"; }
    return "\x1b[0;37m";
}

fn bar(score) {
    return audit_grade_bar(score);
}

fn plural(n, word) {
    if n == 1 { return "" + n + " " + word; }
    return "" + n + " " + word + "s";
}

fn not_ready(connection_id) {
    send_client_message(
        connection_id,
        "The world has not been surveyed yet. The scan runs every few minutes — try again shortly."
    );
}

// ===== the rating =====

fn show_rating(connection_id) {
    let r = get_world_rating();
    if !r.ready {
        not_ready(connection_id);
        return;
    }

    let out = "\x1b[1;36mThe World\x1b[0m\n";
    out += "  " + tier_colour(r.score) + r.tier_label + "\x1b[0m  "
        + text::pad_left("" + r.score, 3) + "  " + bar(r.score) + "\n";
    out += "  \x1b[0;37m" + r.description + "\x1b[0m\n";

    if r.next_label != "" {
        out += "  \x1b[0;37mNext: " + r.next_label + " at " + r.next_at + ".\x1b[0m\n";
    }

    out += "\n\x1b[1;37mWhat it is made of\x1b[0m\n";
    for c in r.components {
        out += "  " + text::pad_right(c.label, 14)
            + text::pad_left("" + c.score, 4) + "  " + bar(c.score)
            + "  \x1b[90mweight " + c.weight + "\x1b[0m\n";
    }

    let f = r.facts;
    out += "\n  " + plural(f.areas, "area") + "   " + plural(f.rooms, "room") + "   "
        + plural(f.mobiles, "mobile") + "   " + plural(f.items, "item") + "\n";
    out += "  " + plural(f.quests, "quest") + "   " + plural(f.dialogue_trees, "dialogue tree")
        + "   " + plural(f.recipes, "recipe") + "   " + plural(f.transports, "transport")
        + "   " + plural(f.boards, "board") + "\n";

    out += "\n\x1b[1;37mNext step\x1b[0m\n";
    if r.capped {
        out += "  \x1b[1;33m" + r.next_step + "\x1b[0m\n";
        out += "  \x1b[0;37mUncapped the rating would be " + r.uncapped + ". Nothing else you build\n";
        out += "  will move it until that is fixed.\x1b[0m\n";
    } else {
        out += "  " + r.next_step + "\n";
    }

    out += "\nUse 'world milestones' for the wall, 'build audit world' for what is broken.";
    send_client_message(connection_id, out);
}

// ===== the wall =====

fn show_wall(connection_id) {
    let rows = get_world_milestones();
    if rows.len() == 0 {
        not_ready(connection_id);
        return;
    }

    let done = "";
    let todo = "";
    let done_count = 0;

    for row in rows {
        if row.unlocked {
            done_count += 1;
            done += "  \x1b[1;32m*\x1b[0m " + text::pad_right(row.name, 26);
            if row.contributors.len() > 0 {
                done += "\x1b[0;37m" + credit_names(row.contributors) + "\x1b[0m";
            } else {
                done += "\x1b[90m(nobody was credited)\x1b[0m";
            }
            done += "\n";
        } else {
            todo += "  \x1b[90m-\x1b[0m " + text::pad_right(row.name, 26)
                + "\x1b[0;37m" + row.progress + "\x1b[0m\n";
        }
    }

    let out = "\x1b[1;36mWorld Milestones\x1b[0m  \x1b[0;37m" + done_count + " of " + rows.len() + "\x1b[0m\n";
    if done_count > 0 {
        out += "\n\x1b[1;37mCrossed\x1b[0m\n" + done;
    }
    if done_count < rows.len() {
        out += "\n\x1b[1;37mAhead\x1b[0m\n" + todo;
    }
    out += "\n\x1b[0;37mCredit goes to every builder carrying a score at the moment it crossed.\x1b[0m";
    send_client_message(connection_id, out);
}

// Up to three names, then a count. A milestone on a busy server would
// otherwise print the whole builder roster on one line.
fn credit_names(names) {
    let shown = [];
    let i = 0;
    while i < names.len() && i < 3 {
        shown.push(names[i]);
        i += 1;
    }
    let out = "";
    let n = 0;
    for name in shown {
        if n > 0 { out += ", "; }
        out += name;
        n += 1;
    }
    if names.len() > 3 {
        out += " and " + (names.len() - 3) + " more";
    }
    return out;
}
