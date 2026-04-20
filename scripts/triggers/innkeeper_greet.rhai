// scripts/triggers/innkeeper_greet.rhai
// Example on_greet trigger for a friendly innkeeper
//
// Usage: medit <id> trigger add greet innkeeper_greet
//
// This trigger fires when a player enters the inn.
// The innkeeper warmly welcomes visitors.

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    // Personalized greeting
    let greetings = [
        "\"Welcome to my humble inn, " + char.name + "!\"",
        "\"Ah, a weary traveler! Come in, come in!\"",
        "\"Need a room for the night? We have the softest beds in town!\"",
        "\"" + char.name + "! So good to see you! What can I get for you?\""
    ];

    let idx = random_int(0, greetings.len() - 1);
    send_client_message(connection_id, mobile.name + " says: " + greetings[idx]);

    return "continue";
}
