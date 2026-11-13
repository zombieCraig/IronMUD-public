// scripts/commands/mspawn.rhai
// mspawn <vnum> - Spawn a mobile from a prototype and place in room

import "olc_helpers" as olc;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Spawning changes this room's area: same rule as building in it, and
    // as the API's spawn endpoints.
    let here = get_room_data(char.current_room_id);
    let here_area = if here == () { "" } else { here.area_id };
    if !olc::require_area_edit(connection_id, here_area, char.name, "You don't have permission to spawn things in this area.") {
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: mspawn <vnum>");
        return;
    }

    let vnum = args.split(" ")[0];

    // Find the prototype
    let prototype = get_mobile_by_vnum(vnum);
    if prototype == () {
        send_client_message(connection_id, "No prototype found with vnum: " + vnum);
        return;
    }

    // Spawn the mobile
    let mobile = spawn_mobile_from_prototype(vnum);
    if mobile == () {
        send_client_message(connection_id, "Failed to spawn mobile from prototype: " + vnum);
        return;
    }

    // Place in room
    move_mobile_to_room(mobile.id, char.current_room_id);
    send_client_message(connection_id, "Spawned " + mobile.name + " in the room.");
    broadcast_to_room(char.current_room_id, mobile.short_desc, char.name);
}
