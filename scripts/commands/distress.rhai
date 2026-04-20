// scripts/commands/distress.rhai
// Emergency distress call for medical assistance
// @description Send a distress call on the helpline channel
// @category communication

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Get current room info
    let room = get_room_data(char.current_room_id);
    let room_name = "Unknown Location";
    let area_name = "";

    if room != () {
        room_name = room.title;
        let area = get_area_data(room.area_id);
        if area != () {
            area_name = area.name;
        }
    }

    // Build distress message
    let message = "";
    if args != "" {
        message = char.name + " calls for help: \"" + args + "\"";
    } else {
        message = char.name + " is calling for medical assistance!";
    }

    // Broadcast to helpline
    let recipients = broadcast_to_helpline(message, room_name, area_name);

    // Feedback to sender
    if recipients > 0 {
        send_client_message(connection_id, "You call for help on the medical helpline!");
        send_client_message(connection_id, "[HELPLINE] " + message);
        send_client_message(connection_id, "Your distress call was received by " + recipients + " responder(s).");
    } else {
        send_client_message(connection_id, "You call for help on the medical helpline!");
        send_client_message(connection_id, "Unfortunately, no one is currently monitoring the helpline.");
        send_client_message(connection_id, "Try asking for help in local chat or seek a healer NPC.");
    }

    // Also broadcast to room
    broadcast_to_room(char.current_room_id, char.name + " calls out for medical help!", char.name);
}
