// scripts/commands/unalias.rhai

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to manage aliases.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: unalias <alias_name>");
        return;
    }

    let parts = args.split(" ");
    let alias_name = parts[0];

    let default_aliases = get_default_aliases();
    let user_aliases = get_aliases(connection_id);

    // Check if it's a default alias
    if default_aliases.contains(alias_name) {
        // Check if user has an override
        if user_aliases.contains(alias_name) {
            remove_alias(connection_id, alias_name);
            send_client_message(connection_id, "Removed override for '" + alias_name + "'. Default alias restored.");
        } else {
            send_client_message(connection_id, "Cannot remove default alias '" + alias_name + "'. Use 'alias " + alias_name + " <new_expansion>' to override it.");
        }
        return;
    }

    // Try to remove user alias
    if remove_alias(connection_id, alias_name) {
        send_client_message(connection_id, "Alias '" + alias_name + "' removed.");
    } else {
        send_client_message(connection_id, "No alias '" + alias_name + "' defined.");
    }
}
