// scripts/commands/who.rhai

// Helper function to format a single player line
fn format_player_line(player) {
    let room = get_room_data(player.room_id);
    let room_name = "???";
    if room != () {
        room_name = room.title;
    }
    let afk_tag = "";
    if is_player_afk(player.name) {
        afk_tag = " [AFK]";
    }
    let idle_tag = "";
    if is_player_idle(player.name) {
        idle_tag = " [Idle]";
    }
    // Check for god mode glow
    let glow_tag = "";
    let char = get_character_data(player.name);
    if char != () && char.god_mode {
        glow_tag = " (glowing)";
    }
    // Get effective level for display
    let eff_level = get_effective_level(player.name);
    let level_tag = " [Eff:" + eff_level + "]";
    return "  " + player.name + level_tag + glow_tag + afk_tag + idle_tag + " - " + room_name + " [" + player.addr + "]\n";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    let viewer_sees_invis = false;
    if char != () {
        viewer_sees_invis = has_buff(char.name, "detect_invisible") || char.is_admin;
    }

    let online = get_online_players();

    if online.len() == 0 {
        send_client_message(connection_id, "No players online.");
        return;
    }

    let visible = [];
    for player in online {
        // Skip invisible players unless viewer can see them
        if !viewer_sees_invis && has_buff(player.name, "invisibility") {
            continue;
        }
        visible.push(player);
    }

    let output = "Players online (" + visible.len() + "):\n";
    for player in visible {
        output += format_player_line(player);
    }
    send_client_message(connection_id, output);
}
