// scripts/commands/frenzy.rhai
//
// Vampire-only voluntary frenzy. Apply the Frenzy buff to self for a brief
// surge of strength — and a combat lock that prevents fleeing. The
// involuntary version (triggered by empty blood pool / fire / sunlight) gets
// stamped from the blood tick / damage handlers; this command is the
// player-side opt-in.

const FRENZY_DURATION_SECS = 30;
const FRENZY_DAMAGE_BONUS = 4;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to frenzy.");
        return;
    }

    if !is_pc_vampire(connection_id) {
        send_client_message(connection_id, "You feel no beast within. Mortals don't frenzy.");
        return;
    }

    if has_buff(char.name, "frenzy") {
        send_client_message(connection_id, "You're already frenzying.");
        return;
    }

    apply_buff(char.name, "frenzy", FRENZY_DAMAGE_BONUS, FRENZY_DURATION_SECS, char.name);

    send_client_message(connection_id, "\x1b[1;31mYou loose the beast. Hunger sharpens to violence.\x1b[0m");
    let room_id = char.current_room_id;
    broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes blacken — the beast surfaces.");
}
