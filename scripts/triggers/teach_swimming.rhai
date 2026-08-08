// scripts/triggers/teach_swimming.rhai
// Say trigger for swim trainer NPCs - teaches swimming skill for gold
//
// Usage: medit <id> trigger add say teach_swimming
//
// NOTE: this predates the `teach_skill` dialogue effect, which does the same
// job declaratively and is the preferred route for new trainers:
//
//   medit <id> tree addfx <node> <choice> teach_skill swimming 100 3 50
//
// This trigger is kept because it reacts to free-form speech rather than a
// dialogue choice, which a tree cannot do. Use it when the trainer should
// answer someone who just says "teach me"; use the effect otherwise.
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
    add_skill_experience(char.name, "swimming", 100, "teach");

    // Flavor text
    send_client_message(connection_id, mobile.name + " takes your " + cost + " gold and leads you through a swimming drill.");
    send_client_message(connection_id, "You practice breathing techniques, treading water, and basic strokes.");
    // No XP line here: add_skill_experience reports the award through the
    // player's xpfeed, and the pre-trait number this used to print contradicted
    // the post-trait one the feed shows.

    // Check if they just reached level 3
    let new_level = get_skill_level(char.name, "swimming");
    if new_level >= 3 {
        send_client_message(connection_id, mobile.name + " says, \"You're ready for deep water now. Be careful out there!\"");
    }

    return "continue";
}
