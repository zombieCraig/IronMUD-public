// scripts/commands/consider.rhai
// Consider command - gauge the relative difficulty of a mobile opponent

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Consider whom?");
        send_client_message(connection_id, "Usage: consider <target>");
        return;
    }

    let room_id = char.current_room_id;
    let keyword = args.to_lower();

    // Find target mobile in the room
    let mobiles = get_mobiles_in_room(room_id);
    let mobile = find_mobile_by_keyword(keyword, mobiles);

    if mobile == () {
        send_client_message(connection_id, "You don't see '" + args + "' here.");
        return;
    }

    let player_combat_level = get_effective_combat_level(char.name);
    let diff = mobile.level - player_combat_level;

    let adjective = get_diff_adjective(diff);
    let color = get_diff_color(diff);

    let output = color + "You consider " + mobile.name + "...\n";
    output += "Difficulty: " + adjective + "\x1b[0m";

    send_client_message(connection_id, output);
}

// Map diff to adjective — uses early returns to avoid if-else chain
fn get_diff_adjective(diff) {
    if diff <= -5 { return "Trivial"; }
    if diff == -4 { return "Effortless"; }
    if diff == -3 { return "Easy"; }
    if diff == -2 { return "Fairly Easy"; }
    if diff == -1 { return "Moderate"; }
    if diff == 0 { return "Even Match"; }
    if diff == 1 { return "Challenging"; }
    if diff == 2 { return "Tough"; }
    if diff == 3 { return "Dangerous"; }
    if diff == 4 { return "Very Dangerous"; }
    return "Deadly";
}

// Map diff to ANSI color — uses early returns to avoid if-else chain
fn get_diff_color(diff) {
    if diff <= -4 { return "\x1b[90m"; }
    if diff <= -1 { return "\x1b[32m"; }
    if diff == 0 { return "\x1b[33m"; }
    if diff <= 2 { return "\x1b[31m"; }
    return "\x1b[1;31m";
}
