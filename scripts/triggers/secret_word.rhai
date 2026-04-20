// scripts/triggers/secret_word.rhai
// Example on_say trigger that responds to a secret word
//
// Usage: medit <id> trigger add say secret_word
//
// This trigger fires when a player says something in the room.
// If they say the secret word, something special happens.

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let message = context.get("message");
    if message == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    // Check for secret word (case insensitive)
    let msg_lower = message.to_lower();

    if msg_lower.contains("xyzzy") {
        send_client_message(connection_id, mobile.name + " gasps and whispers: \"You know the ancient word!\"");
        send_client_message(connection_id, mobile.name + " reveals a hidden passage behind them!");
        // In a real implementation, this might open a door or teleport the player
        return "continue";
    }

    if msg_lower.contains("plugh") {
        send_client_message(connection_id, mobile.name + " chuckles knowingly.");
        return "continue";
    }

    return "continue";
}
