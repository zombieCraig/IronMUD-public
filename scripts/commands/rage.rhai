// scripts/commands/rage.rhai
// Werewolf deliberate Rage build: feed the anger mid-fight.
// @description Werewolf only — stoke your Rage in combat (+2, 60s cooldown; overflowing the pool risks frenzy)
// @category skills
//
// The transaction (cooldown, gain, overflow frenzy roll vs tribe banes)
// lives in the Rust binding `werewolf_build_rage`; this script gates on
// combat context and renders messages. With no argument and out of combat,
// shows the current pool instead.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_werewolf(connection_id) {
        send_client_message(connection_id, "You have anger, but not Rage. Only the Garou carry that fire.");
        return;
    }

    if !is_in_combat(char.name) {
        let rage = get_pc_rage(connection_id);
        let max = get_pc_max_rage(connection_id);
        send_client_message(connection_id, "Rage: " + rage + "/" + max + ". It only builds in battle — fight, bleed, or kill.");
        return;
    }

    let result = werewolf_build_rage(connection_id);
    if !result["success"] {
        if result["error"] == "cooldown" {
            send_client_message(connection_id, "The fire needs a moment to catch again. (" + result["cooldown_remaining"] + "s)");
        } else {
            send_client_message(connection_id, "The Rage won't answer: " + result["error"]);
        }
        return;
    }

    send_client_message(connection_id, "\x1b[31mYou feed the fight to the fire. Rage: " + result["rage"] + "/" + get_pc_max_rage(connection_id) + "\x1b[0m");
    if result["frenzied"] {
        let room_id = char.current_room_id;
        send_client_message(connection_id, "\x1b[1;31mToo much. The cage opens — the wolf is driving now!\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes flood amber. Something with too many teeth is wearing their face.");
    }
}
