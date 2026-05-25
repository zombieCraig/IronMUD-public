// scripts/commands/clanchat.rhai
// Clan-internal chat channel. Usage: clanchat <message>  (alias: ct)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let tag = get_character_clan_tag(char.name);
    if tag == "" {
        send_client_message(connection_id, "You are not in a clan.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "What do you want to say to your clan?");
        return;
    }

    if !char.god_mode {
        let wait = try_throttle_command(char.name, "clanchat", 2);
        if wait > 0 {
            send_client_message(connection_id, "Slow down. (" + wait + "s)");
            return;
        }
    }

    let color = get_clan_color(tag);
    if color == "" { color = "\x1b[1;33m"; }
    let line = color + "[" + tag + "] " + char.name + ": " + args + "\x1b[0m";

    let members = get_online_clan_members(tag);
    let delivered = 0;
    for listener_name in members {
        if listener_name.to_lower() == char.name.to_lower() { continue; }
        if is_ignoring(listener_name, char.name) { continue; }
        let listener_conn = find_player_connection_by_name(listener_name);
        if listener_conn == "" { continue; }
        send_client_message(listener_conn, line);
        delivered += 1;
    }

    // Speaker always sees their own line — even if alone, so they know it went through.
    send_client_message(connection_id, color + "[" + tag + "] You clanchat: " + args + "\x1b[0m");
    if delivered == 0 {
        send_client_message(connection_id, "(No other clanmates are online to hear you.)");
    }
}
