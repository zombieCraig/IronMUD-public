// scripts/triggers/magic_potion.rhai
// Example on_use trigger that adds extra effects when drinking a magical potion
//
// Usage: oedit <id> trigger add use magic_potion
//
// This trigger fires when a player drinks from the item.
// It adds atmospheric effects but doesn't cancel the action.

fn run_trigger(item_id, connection_id, context) {
    let char = get_player_character(connection_id);
    if char == () {
        return "continue";
    }

    // Check if this is a drink action
    let use_type = context.get("use_type");
    if use_type != "drink" {
        return "continue";
    }

    // Roll for random effect message
    let roll = random_int(1, 4);

    if roll == 1 {
        send_client_message(connection_id, "The liquid sparkles with magical energy as you drink.");
    } else if roll == 2 {
        send_client_message(connection_id, "You feel a warm glow spread through your body.");
    } else if roll == 3 {
        send_client_message(connection_id, "Tiny motes of light dance around you momentarily.");
    } else {
        send_client_message(connection_id, "The potion tastes of starlight and honey.");
    }

    return "continue";
}
