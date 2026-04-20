// scripts/triggers/ancient_tome.rhai
// Example on_examine trigger that reveals hidden knowledge
//
// Usage: oedit <id> trigger add examine ancient_tome
//
// This trigger fires when a player examines the item.
// It displays additional lore based on random selection.

fn run_trigger(item_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // Different passages revealed each time
    let roll = random_int(1, 5);

    send_client_message(connection_id, "\n--- Hidden Knowledge ---");

    if roll == 1 {
        send_client_message(connection_id, "The pages whisper of an ancient evil sealed beneath the mountains.");
    } else if roll == 2 {
        send_client_message(connection_id, "You glimpse a map to a hidden treasure before the ink fades.");
    } else if roll == 3 {
        send_client_message(connection_id, "Strange symbols glow briefly, teaching you a word of power.");
    } else if roll == 4 {
        send_client_message(connection_id, "The text describes rituals best left forgotten.");
    } else {
        send_client_message(connection_id, "For a moment, you understand the language of the stars.");
    }

    return "continue";
}
