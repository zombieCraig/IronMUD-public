// scripts/commands/adelete.rhai
// Usage: adelete <area_id>
// Deletes an area (rooms are unassigned but not deleted)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        let msg = "Usage: adelete <area_id>\n";
        msg += "Use 'alist' to see available areas.\n";
        msg += "WARNING: This will unassign all rooms from the area.";
        send_client_message(connection_id, msg);
        return;
    }

    let area_id = args;
    let area = get_area_data(area_id);
    if area == () {
        send_client_message(connection_id, "Area not found: " + area_id);
        return;
    }

    // Permission check
    if !can_edit_area(area.id, char.name) {
        send_client_message(connection_id, "You don't have permission to delete this area.");
        return;
    }

    let area_name = area.name;
    let rooms = get_rooms_in_area(area_id);
    let room_count = rooms.len();

    if delete_area(area_id) {
        let msg = "Deleted area: " + area_name + "\n";
        msg += room_count + " room(s) have been unassigned from this area.";
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Error: Failed to delete area.");
    }
}
