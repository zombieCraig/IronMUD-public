// scripts/commands/schedule.rhai
// schedule <npc> - View an NPC's daily routine schedule

fn get_time_name(hour) {
    if hour >= 5 && hour < 8 { return "Dawn"; }
    if hour >= 8 && hour < 12 { return "Morning"; }
    if hour >= 12 && hour < 14 { return "Midday"; }
    if hour >= 14 && hour < 18 { return "Afternoon"; }
    if hour >= 18 && hour < 21 { return "Evening"; }
    if hour >= 21 && hour < 24 { return "Night"; }
    return "Late Night";
}

fn format_hour(hour) {
    if hour == 0 { return "12am"; }
    if hour < 12 { return "" + hour + "am"; }
    if hour == 12 { return "12pm"; }
    return "" + (hour - 12) + "pm";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Schedule who? Usage: schedule <npc>");
        return;
    }

    let keyword = args.to_lower();

    // Find mobile in the same room
    let mobiles = get_mobiles_in_room(char.current_room_id);
    let mobile = find_mobile_by_keyword(keyword, mobiles);

    if mobile == () {
        send_client_message(connection_id, "You don't see anyone like that here.");
        return;
    }

    // Check if schedule is visible
    let visible = get_mobile_schedule_visible(mobile.id);
    if !visible && !char.god_mode {
        send_client_message(connection_id, mobile.name + " doesn't share their schedule.");
        return;
    }

    // Get routine entries
    let entries = get_mobile_routine_entries(mobile.id);

    if entries.len() == 0 {
        send_client_message(connection_id, mobile.name + " doesn't seem to have a set routine.");
        return;
    }

    let activity = get_mobile_activity(mobile.id);
    let output = "=== Schedule for " + mobile.name + " ===\n";
    output += "Currently: " + activity + "\n\n";

    for entry in entries {
        let hour = entry["start_hour"];
        let act = entry["activity"];
        output += "  " + format_hour(hour) + " (" + get_time_name(hour) + "): " + act + "\n";
    }

    send_client_message(connection_id, output);
}
