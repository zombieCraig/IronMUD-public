// scripts/triggers/crystal_greet.rhai
// on_greet trigger for Crystal, the barista in the plaza cafe (plaza:crystal).
//
// Usage: medit <id> trigger add greet crystal_greet

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    let greetings = [
        "\"Hi there, " + char.name + "! What can I get started for you?\"",
        "\"Welcome in! The espresso's fresh, I promise.\"",
        "\"Oh, hey " + char.name + "! Your usual? Kidding, kidding - take a look at the menu.\"",
        "\"Come on in, grab a seat anywhere! Type 'list' to see what we're brewing.\""
    ];

    let idx = random_int(0, greetings.len() - 1);
    send_client_message(connection_id, mobile.name + " says: " + greetings[idx]);

    return "continue";
}
