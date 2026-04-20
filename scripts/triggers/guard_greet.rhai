// scripts/triggers/guard_greet.rhai
// Example on_greet trigger for a guard NPC
//
// Usage: medit <id> trigger add greet guard_greet
//
// This trigger fires when a player enters the room with this NPC.
// Guards challenge newcomers based on random chance.

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    // Random greeting behavior
    let roll = random_int(1, 3);

    if roll == 1 {
        send_client_message(connection_id, mobile.name + " eyes you suspiciously.");
    } else if roll == 2 {
        send_client_message(connection_id, mobile.name + " nods curtly in your direction.");
    } else {
        send_client_message(connection_id, mobile.name + " says: \"State your business here, traveler.\"");
    }

    return "continue";
}
