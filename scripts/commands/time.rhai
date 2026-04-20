// scripts/commands/time.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere.");
        return;
    }

    // Check if player can see outside
    if room.flags.indoors || room.flags.no_windows {
        send_client_message(connection_id, "You can't see the sky from here.");
        return;
    }

    // Get time and display sun/moon position
    let gt = get_game_time();
    let time_of_day = gt.time_of_day;

    let message = "";
    if time_of_day == "dawn" {
        message = "The sun is just rising over the eastern horizon.";
    } else if time_of_day == "morning" {
        message = "The sun climbs steadily in the eastern sky.";
    } else if time_of_day == "high noon" {
        message = "The sun blazes directly overhead.";
    } else if time_of_day == "afternoon" {
        message = "The sun hangs in the western sky.";
    } else if time_of_day == "dusk" {
        message = "The sun is setting on the western horizon.";
    } else if time_of_day == "evening" {
        message = "The moon rises as twilight fades.";
    } else if time_of_day == "night" {
        message = "The moon hangs in the dark sky above.";
    } else {
        message = "You glance at the sky.";
    }

    send_client_message(connection_id, message);
}
