// scripts/triggers/trapped_room.rhai
// Example on_enter trigger that blocks entry without a key
//
// Usage: redit trigger add enter trapped_room
//
// This trigger fires when a player tries to enter the room.
// If the player doesn't have the "magic_key" item, entry is blocked.

fn run_trigger(room_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // Check if player has the magic key (by item keyword)
    // Note: This requires has_item_in_inventory to be registered
    // For now, we'll just demonstrate the cancellation mechanism

    // Example: Block entry 30% of the time to simulate a magical barrier
    let roll = random_int(1, 100);
    if roll <= 30 {
        send_client_message(connection_id, "A magical barrier flickers and repels you!");
        broadcast_to_room(room_id, char.name + " is repelled by a magical barrier!", char.name);
        return "cancel";  // Prevent entry
    }

    // Allow entry with a warning message
    send_client_message(connection_id, "You feel a tingling sensation as you pass through...");
    return "continue";
}
