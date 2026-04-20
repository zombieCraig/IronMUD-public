// scripts/triggers/sticky_trap.rhai
// Example on_get trigger that has a chance to trap the player's hands
//
// Usage: oedit <id> trigger add get sticky_trap
//        oedit <id> trigger chance 0 50  (50% chance)
//
// This trigger simulates a sticky or trapped item.

fn run_trigger(item_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // The item sticks to the player's hands!
    send_client_message(connection_id, "Your hands stick to the item! It's covered in some kind of adhesive!");
    send_client_message(connection_id, "You manage to pull free but leave the item behind.");

    return "cancel";
}
