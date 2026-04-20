// scripts/triggers/safe_zone_exit.rhai
// Example on_exit trigger that warns players leaving a safe area
//
// Usage: redit trigger add exit safe_zone_exit
//
// This trigger fires when a player tries to leave the room.
// It just sends a warning message but doesn't block movement.

fn run_trigger(room_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // Get direction from context if available
    let direction = context.get("direction");
    if direction == () {
        direction = "that way";
    }

    send_client_message(connection_id, "Warning: You are leaving the safe zone. Danger lies " + direction + "!");

    // Don't cancel - just warn
    return "continue";
}
