// scripts/triggers/hostile_greet.rhai
// Example on_greet trigger for an aggressive monster
//
// Usage: medit <id> trigger add greet hostile_greet
//
// This trigger fires when a player enters the room.
// The creature threatens the player (future: could initiate combat).

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    // Threatening messages
    let threats = [
        mobile.name + " snarls menacingly at you!",
        mobile.name + " bares its fangs and growls!",
        mobile.name + " turns toward you with murderous intent!",
        mobile.name + " lets out a terrifying roar!"
    ];

    let idx = random_int(0, threats.len() - 1);
    send_client_message(connection_id, threats[idx]);

    // TODO: When combat is implemented, could auto-attack here
    // return "attack";

    return "continue";
}
