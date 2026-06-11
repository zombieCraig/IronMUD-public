// scripts/commands/frenzy.rhai
//
// Vampire-only voluntary frenzy. Apply the Frenzy + Rage buff pair to self:
// a brief surge of strength, a combat lock that prevents fleeing, and the
// rage effect — the beast attacks whoever it sees (any mobile outside safe
// zones, players in PvP zones) until the frenzy passes. The involuntary
// version (hunger frenzy at empty blood pool, humanity check + clan banes)
// is stamped by the blood tick (src/vampire/mod.rs::maybe_hunger_frenzy);
// this command is the player-side opt-in to the same beast.

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
    apply_buff(char.name, "rage", 1, FRENZY_DURATION_SECS, char.name);

    send_client_message(connection_id, "\x1b[1;31mYou loose the beast. Hunger sharpens to violence — you will strike at whatever you see.\x1b[0m");
    let room_id = char.current_room_id;
    broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes blacken — the beast surfaces.");
}
