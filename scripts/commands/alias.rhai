// scripts/commands/alias.rhai

fn format_alias(name, expansion) {
    let line = "  " + name;
    line += " => ";
    line += expansion;
    line
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to manage aliases.");
        return;
    }

    let user_aliases = get_aliases(connection_id);
    let default_aliases = get_default_aliases();

    // No arguments: list all aliases
    if args == "" {
        list_aliases(connection_id, user_aliases, default_aliases);
        return;
    }

    let parts = args.split(" ");

    // One argument: show that specific alias
    if parts.len() == 1 {
        show_alias(connection_id, parts[0], user_aliases, default_aliases);
        return;
    }

    // Two or more arguments: set alias
    let alias_name = parts[0];

    // Get everything after the first space as the expansion
    let first_space = 0;
    for i in 0..args.len() {
        if args.sub_string(i, 1) == " " {
            first_space = i;
            break;
        }
    }
    let expansion = args.sub_string(first_space + 1);

    // Prevent aliasing reserved commands
    if alias_name == "alias" {
        send_client_message(connection_id, "Cannot create alias for reserved command: alias");
        return;
    }
    if alias_name == "unalias" {
        send_client_message(connection_id, "Cannot create alias for reserved command: unalias");
        return;
    }

    set_alias(connection_id, alias_name, expansion);
    let msg = "Alias set: " + alias_name;
    msg += " => ";
    msg += expansion;
    send_client_message(connection_id, msg);
}

fn list_aliases(connection_id, user_aliases, default_aliases) {
    send_client_message(connection_id, "=== Your Aliases ===");

    if user_aliases.len() == 0 {
        send_client_message(connection_id, "(no custom aliases defined)");
    } else {
        for alias in user_aliases.keys() {
            let exp = user_aliases[alias];
            send_client_message(connection_id, format_alias(alias, exp));
        }
    }

    send_client_message(connection_id, "");
    send_client_message(connection_id, "=== Default Aliases ===");
    for alias in default_aliases.keys() {
        let exp = default_aliases[alias];
        let line = format_alias(alias, exp);
        if user_aliases.contains(alias) {
            line += " (overridden)";
        }
        send_client_message(connection_id, line);
    }
}

fn show_alias(connection_id, alias_name, user_aliases, default_aliases) {
    if user_aliases.contains(alias_name) {
        let exp = user_aliases[alias_name];
        let msg = alias_name + " => ";
        msg += exp;
        send_client_message(connection_id, msg);
    } else if default_aliases.contains(alias_name) {
        let exp = default_aliases[alias_name];
        let msg = alias_name + " => ";
        msg += exp;
        msg += " (default)";
        send_client_message(connection_id, msg);
    } else {
        let msg = "No alias '" + alias_name;
        msg += "' defined.";
        send_client_message(connection_id, msg);
    }
}
