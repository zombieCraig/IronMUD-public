// scripts/commands/describe.rhai
// Change character's short description after creation

fn run_command(args, connection_id) {
    // Must be logged in
    let char_data = get_player_character(connection_id);
    if char_data == () {
        send_client_message(connection_id, "You must be logged in to use this command.");
        return;
    }

    let new_desc = args;

    if new_desc == "" {
        send_client_message(connection_id, "Current description: " + char_data.short_description);
        send_client_message(connection_id, "\nUsage: describe <new description>");
        return;
    }

    // Update the description
    char_data.short_description = new_desc;
    save_character_data(char_data);

    send_client_message(connection_id, "Description updated to: " + new_desc);
}
