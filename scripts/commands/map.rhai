// scripts/commands/map.rhai
// map [radius] - print an ASCII map of the surrounding area.
// Only rooms the player has already entered are drawn (plus a one-step
// glimpse of their immediate neighbors).

fn run_command(args, connection_id) {
    if !map_enabled() {
        send_client_message(connection_id, "Maps are disabled on this world.");
        return;
    }

    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let radius = 5;
    let parts = args.split(" ");
    if parts.len() > 0 && parts[0] != "" {
        let n = parse_int(parts[0]);
        if n != () {
            if n < 1 { radius = 1; }
            else if n > 8 { radius = 8; }
            else { radius = n; }
        }
    }

    let rendered = render_map_for_session(connection_id, radius);
    if rendered == "" {
        send_client_message(connection_id, "You have no map data here yet.");
        return;
    }
    send_client_message(connection_id, rendered);
}
