// scripts/triggers/soulbound_drop.rhai
// Example on_drop trigger that prevents dropping a soulbound item
//
// Usage: oedit <id> trigger add drop soulbound_drop
//
// This trigger fires when a player tries to drop the item.
// It always prevents the drop.

fn run_trigger(item_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    send_client_message(connection_id, "This item is soulbound to you and cannot be dropped!");
    return "cancel";
}
