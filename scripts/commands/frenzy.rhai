// scripts/commands/frenzy.rhai
//
// Vampire/werewolf voluntary frenzy. Apply the Frenzy + Rage buff pair to
// self: a brief surge of strength, a combat lock that prevents fleeing, and
// the rage effect — the beast attacks whoever it sees (any mobile outside
// safe zones, players in PvP zones) until the frenzy passes. The
// involuntary versions (vampire hunger frenzy at empty blood pool, werewolf
// rage frenzy when Rage runs hot) are stamped by the blood/rage ticks;
// this command is the player-side opt-in to the same beast.

const FRENZY_DURATION_SECS = 30;
const FRENZY_DAMAGE_BONUS = 4;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to frenzy.");
        return;
    }

    let is_vampire = is_pc_vampire(connection_id);
    let is_werewolf = is_pc_werewolf(connection_id);
    if !is_vampire && !is_werewolf {
        send_client_message(connection_id, "You feel no beast within. Mortals don't frenzy.");
        return;
    }

    if has_buff(char.name, "frenzy") {
        send_client_message(connection_id, "You're already frenzying.");
        return;
    }

    apply_buff(char.name, "frenzy", FRENZY_DAMAGE_BONUS, FRENZY_DURATION_SECS, char.name);
    apply_buff(char.name, "rage", 1, FRENZY_DURATION_SECS, char.name);

    let room_id = char.current_room_id;
    if is_werewolf {
        send_client_message(connection_id, "\x1b[1;31mYou stop holding the wolf back. The Rage pours through — you will strike at whatever you see.\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes flood amber — the wolf surfaces.");
    } else {
        send_client_message(connection_id, "\x1b[1;31mYou loose the beast. Hunger sharpens to violence — you will strike at whatever you see.\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes blacken — the beast surfaces.");
    }
}
