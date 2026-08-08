// scripts/commands/locate.rhai
//
//   locate corpse  - ask your god where your body fell
//
// A divination, not a map. It names the room and the area and stops there:
// a route would turn the corpse run into a formality, and the run is the
// whole stake of dying.
//
// Gated on worship favor. A god you have never tithed to does not take your
// calls, which makes the worship system something you notice at exactly the
// moment it would have helped.

import "text" as text;
import "progress_ui" as ui;

// The favor tier a player must reach for their god to answer. Compared through
// `worship_favor_at_least`, which owns the ladder's ordering — this script
// names a rung, it does not know where the rung sits.
const REQUIRED_TIER = "noticed";

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let what = text::normalise(args);
    if what == "" {
        send_client_message(connection_id, "Locate what? Usage: locate corpse");
        return;
    }
    if what != "corpse" && what != "body" && what != "remains" {
        send_client_message(connection_id, "You can only locate your own corpse. Usage: locate corpse");
        return;
    }

    let god = get_worship_god(char.name);
    if god == "" {
        send_client_message(connection_id, "You close your eyes and reach out. Nothing reaches back.\nOnly a god answers this kind of question, and you serve none.");
        return;
    }

    if !worship_favor_at_least(char.name, REQUIRED_TIER) {
        send_client_message(connection_id, "You close your eyes and ask. The silence that answers is not empty — it is pointed.\nYour god has not yet decided you are worth an answer. (" + get_worship_standing(char.name) + ")");
        return;
    }

    let corpses = find_own_corpses(char.name);
    if corpses.len() == 0 {
        send_client_message(connection_id, "You reach out and find nothing of yourself left lying anywhere. Whatever you have lost, it is not your body.");
        return;
    }

    let output = "You go still, and something older than you looks out through your eyes.\n";
    for corpse in corpses {
        output += "\n" + describe_corpse_location(corpse);
    }
    send_client_message(connection_id, output);
}

// One located corpse. Split out because the where-clause and the time-clause
// are each conditional, and Rhai's expression complexity limit dislikes them
// inline.
fn describe_corpse_location(corpse) {
    let where_it_lies = corpse.room;
    if corpse.area != "" {
        where_it_lies += ", in " + corpse.area;
    }

    let line = "  Your body lies at \x1b[1;37m" + where_it_lies + "\x1b[0m.";
    if corpse.remaining_secs > 0 {
        line += "\n    It has " + ui::format_playtime(corpse.remaining_secs) + " before it is gone.";
    } else {
        line += "\n    \x1b[1;31mIt is going now.\x1b[0m";
    }
    return line;
}
