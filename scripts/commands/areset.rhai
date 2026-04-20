// scripts/commands/areset.rhai
// Manual area reset - triggers all spawn points in an area
// Usage: areset <area_id>

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: areset <area_id>\nUse 'alist' to see available areas.");
        return;
    }

    let area = get_area_data(args);
    if area == () {
        area = get_area_by_prefix(args);
    }

    if area == () {
        send_client_message(connection_id, "Area not found: " + area_id);
        return;
    }

    let count = trigger_area_reset(area.id);

    if count > 0 {
        send_client_message(connection_id, "Area '" + area.name + "' reset. Spawned " + count + " entities.");
    } else {
        send_client_message(connection_id, "Area '" + area.name + "' reset. No new entities needed (all spawn points at max or disabled).");
    }
}
