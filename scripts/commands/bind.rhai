// scripts/commands/bind.rhai
// bind - Bind your spawn point to the current room
// Restricted to rooms with spawn_point flag OR rooms in player's rental property

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let room_id = char.current_room_id;
    let room = get_room_data(room_id);
    if room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check if room has spawn_point flag
    let has_spawn_flag = get_room_flag(room_id, "spawn_point");

    // Check if room is in player's rental property
    let is_rental_property = false;
    let area_id = room.area_id;
    if area_id != "" {
        let lease = get_player_lease_in_area(char.name, area_id);
        if lease != () && !lease.is_evicted {
            // Check if this room is part of the property
            // We consider it a rental property room if player has a lease in this area
            // and the room has property_storage flag (typical for rental rooms)
            let is_property_room = get_room_flag(room_id, "property_storage");
            if is_property_room {
                is_rental_property = true;
            }
        }
    }

    // Must be in a spawn_point room or rental property
    if !has_spawn_flag && !is_rental_property {
        send_client_message(connection_id, "You can't bind your spawn point here.\nLook for an inn, tavern, or other safe location, or bind in your rental property.");
        return;
    }

    // Check if already bound here
    let current_spawn = char.spawn_room_id;
    if current_spawn == room_id {
        send_client_message(connection_id, "Your spawn point is already bound here.");
        return;
    }

    // Set the spawn point
    if set_character_spawn_room(char.name, room_id) {
        send_client_message(connection_id, "You bind your soul to " + room.title + ".\nThis is now your recall point.");
    } else {
        send_client_message(connection_id, "Error: Failed to bind spawn point.");
    }
}
