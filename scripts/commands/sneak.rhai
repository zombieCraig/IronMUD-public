// scripts/commands/sneak.rhai
// sneak - Toggle sneaking mode (move silently)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    // Can't sneak in combat
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't start sneaking while in combat!");
        return;
    }

    // Toggling off is always allowed (a noisy item can't strand you mid-sneak
    // without recourse — equipping it already cleared the flag, but the
    // command should still be a clean toggle).
    if is_sneaking(char.name) {
        set_sneaking(char.name, false);
        send_client_message(connection_id, "You stop sneaking and resume moving normally.");
        return;
    }

    // Refuse to start sneaking if you're wearing something noisy (bells,
    // glowing runes, etc.). The first equipped Loud affect wins; we don't
    // try to name the offending item here.
    if is_loud(char.name) {
        send_client_message(connection_id, "Something you're wearing makes too much noise to sneak.");
        return;
    }

    set_sneaking(char.name, true);
    send_client_message(connection_id, "You begin moving silently, watching your every step.");
}
