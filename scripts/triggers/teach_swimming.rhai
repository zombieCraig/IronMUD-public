// scripts/triggers/teach_swimming.rhai
// Say trigger for swim trainer NPCs - teaches swimming skill for gold
//
// Usage: medit <id> trigger add say teach_swimming
//
// When a player says "swim", "lesson", or "teach" near this NPC,
// the trainer offers a swimming lesson for 50 gold, granting 100 swimming XP.
// Lessons stop once the player reaches swimming level 3 (enough for deep water).

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

    let msg_lower = message.to_lower();

    // Check for swimming-related keywords
    if !msg_lower.contains("swim") && !msg_lower.contains("lesson") && !msg_lower.contains("teach") {
        return "continue";
    }

    let swim_level = get_skill_level(char.name, "swimming");

    // Already skilled enough - no more lessons needed
    if swim_level >= 3 {
        send_client_message(connection_id, mobile.name + " says, \"You're already a capable swimmer. Practice in deeper waters to improve further.\"");
        return "continue";
    }

    let cost = 50;

    // Check if player can afford it
    if char.gold < cost {
        send_client_message(connection_id, mobile.name + " says, \"A swimming lesson costs " + cost + " gold. Come back when you can afford it.\"");
        return "continue";
    }

    // Deduct gold
    char.gold -= cost;
    set_player_character(connection_id, char);
    save_character_data(char);

    // Grant swimming XP
    let leveled = add_skill_experience(char.name, "swimming", 100);

    // Flavor text
    send_client_message(connection_id, mobile.name + " takes your " + cost + " gold and leads you through a swimming drill.");
    send_client_message(connection_id, "You practice breathing techniques, treading water, and basic strokes.");
    send_client_message(connection_id, "\x1b[1;36mYou gained 100 swimming experience!\x1b[0m");

    if leveled {
        send_client_message(connection_id, "\x1b[1;33mYour swimming skill has improved!\x1b[0m");
    }

    // Check if they just reached level 3
    let new_level = get_skill_level(char.name, "swimming");
    if new_level >= 3 {
        send_client_message(connection_id, mobile.name + " says, \"You're ready for deep water now. Be careful out there!\"");
    }

    return "continue";
}
