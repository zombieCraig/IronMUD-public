// scripts/triggers/quest_giver.rhai
// Example on_greet trigger for a quest-giving NPC
//
// Usage: medit <id> trigger add greet quest_giver
//
// This trigger makes the NPC offer a quest when players enter.

fn run_trigger(mobile_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        return "continue";
    }

    // Chance to offer quest
    let roll = random_int(1, 100);
    if roll > 50 {
        return "continue";  // Don't always pester the player
    }

    send_client_message(connection_id, mobile.name + " waves you over urgently.");
    send_client_message(connection_id, mobile.name + " says: \"Psst! " + char.name + "! I have a task that needs doing...\"");
    send_client_message(connection_id, mobile.name + " says: \"Say 'quest' if you're interested.\"");

    return "continue";
}
