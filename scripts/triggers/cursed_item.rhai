// scripts/triggers/cursed_item.rhai
// Example on_get trigger for a cursed item that sometimes hurts the player
//
// Usage: oedit <id> trigger add get cursed_item
//
// This trigger fires when a player tries to pick up the item.
// It has a 30% chance to hurt the player and cancel the pickup.

fn run_trigger(item_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // 30% chance to trigger curse
    let roll = random_int(1, 100);
    if roll <= 30 {
        send_client_message(connection_id, "As you touch the item, dark energy courses through your hand!");
        send_client_message(connection_id, "You recoil in pain and drop it.");
        // TODO: Apply damage to player when combat system is ready
        return "cancel";
    }

    // Item feels unsettling even when picked up successfully
    send_client_message(connection_id, "A chill runs down your spine as you touch the item.");
    return "continue";
}
