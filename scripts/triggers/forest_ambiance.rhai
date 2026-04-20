// scripts/triggers/forest_ambiance.rhai
// Example periodic trigger that displays random atmospheric messages
//
// Usage: redit trigger add periodic forest_ambiance
//        redit trigger interval 0 30  (fire every 30 seconds)
//        redit trigger chance 0 50    (50% chance each interval)
//
// This trigger fires periodically for all players in the room.

fn run_trigger(room_id, connection_id, context) {
    let messages = [
        "An owl hoots in the distance.",
        "The wind whispers through the ancient branches.",
        "You hear rustling in the underbrush nearby.",
        "A cold mist swirls around your ankles.",
        "Leaves flutter down from above.",
        "You catch a glimpse of glowing eyes in the shadows.",
        "The forest seems to breathe around you."
    ];

    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);

    return "continue";
}
