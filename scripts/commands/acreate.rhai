// scripts/commands/acreate.rhai
// Usage: acreate <prefix> <name>
// Creates a new area with the given prefix and name

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        let msg = "Usage: acreate <prefix> <name>\n";
        msg += "Example: acreate forest The Dark Forest\n";
        msg += "The prefix is used for room vnums (e.g., forest:1, forest:entrance)";
        send_client_message(connection_id, msg);
        return;
    }

    let parts = args.split(" ");
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: acreate <prefix> <name>");
        return;
    }

    let prefix = parts[0].to_lower();

    // Join remaining parts as name
    let name_parts = [];
    for i in 1..parts.len() {
        name_parts.push(parts[i]);
    }
    let name = name_parts.reduce(|a, b| a + " " + b);

    // Create area with the creator as owner
    let area = create_area_with_owner(name, prefix, char.name);

    let output = "Area created!\n";
    output += "Name: " + area.name + "\n";
    output += "Prefix: " + area.prefix + "\n";
    output += "ID: " + area.id + "\n";
    output += "Owner: " + char.name + "\n";
    output += "\nUse 'aedit' to modify area properties.";
    output += "\nUse 'redit area " + area.id + "' to assign rooms to this area.";

    send_client_message(connection_id, output);
}
