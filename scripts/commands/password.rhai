// scripts/commands/password.rhai
// Allow logged-in users to change their password

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to change your password.");
        return;
    }

    let new_password = args;

    if new_password == "" {
        send_client_message(connection_id, "Usage: password <new_password>");
        send_client_message(connection_id, "Your password will be updated immediately.");
        return;
    }

    // Validate password length (minimum 4 characters)
    if new_password.len() < 4 {
        send_client_message(connection_id, "Password must be at least 4 characters long.");
        return;
    }

    // Password cannot be the same as character name
    if new_password.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "Password cannot be the same as your character name.");
        return;
    }

    // Securely change password (hashes and saves internally)
    if change_password(connection_id, new_password) {
        send_client_message(connection_id, "Your password has been changed successfully.");
    } else {
        send_client_message(connection_id, "Failed to change password. Please try again.");
    }
}
