// scripts/triggers/guardian_look.rhai
// Example on_look trigger that adds flavor when looking at a guarded room
//
// Usage: redit trigger add look guardian_look
//
// This trigger fires after the player looks at the room.
// It adds additional atmospheric description.

fn run_trigger(room_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // Add atmospheric description based on random choice
    let roll = random_int(1, 3);

    if roll == 1 {
        send_client_message(connection_id, "\nThe guardian statue's eyes seem to follow your movement.");
    } else if roll == 2 {
        send_client_message(connection_id, "\nYou feel an ancient presence watching over this place.");
    } else {
        send_client_message(connection_id, "\nDust motes drift lazily in beams of pale light.");
    }

    return "continue";
}
