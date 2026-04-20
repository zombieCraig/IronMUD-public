// scripts/commands/property.rhai
// property - View and manage your property settings
// property access <none|visit|full> - Set party access level
// property trust <name> - Add a trusted visitor
// property untrust <name> - Remove a trusted visitor

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Get all player's leases
    let leases = get_all_player_leases(char.name);

    // Filter out evicted leases
    let active_leases = [];
    for lease in leases {
        if !lease.is_evicted {
            active_leases.push(lease);
        }
    }

    if active_leases.len() == 0 {
        send_client_message(connection_id, "You don't have any properties.\nUse 'properties' at a leasing office to see available rentals.");
        return;
    }

    // For now, just use the first active lease
    // In the future, could support multiple properties
    let lease = active_leases[0];

    // Parse subcommand
    if args == "" {
        show_property_info(lease, connection_id);
        return;
    }

    let parts = args.split(" ");
    let subcommand = parts[0].to_lower();

    if subcommand == "access" {
        handle_access(lease, parts, connection_id);
        return;
    }

    if subcommand == "trust" {
        handle_trust(lease, parts, connection_id);
        return;
    }

    if subcommand == "untrust" {
        handle_untrust(lease, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand + "\nUse 'property' to see available commands.");
}

fn show_property_info(lease, connection_id) {
    let template = get_property_template(lease.template_vnum);
    let template_name = lease.template_vnum;
    if template != () {
        template_name = template.name;
    }

    let output = "=== Your Property ===\n\n";
    output += "Name: " + lease.owner_name + "'s " + template_name + "\n";
    output += "Rent: " + lease.monthly_rent + " gold/month\n";

    // Show rent status
    let days_remaining = get_lease_days_remaining(lease.id);
    if days_remaining > 0 {
        output += "Paid until: " + days_remaining + " game days remaining\n";
    } else {
        output += "Rent: OVERDUE\n";
    }

    // Show party access
    output += "\nParty Access: " + lease.party_access + "\n";
    output += "  none - No one can enter\n";
    output += "  visit - Grouped players can enter and look around\n";
    output += "  full - Grouped players can use amenities and take items\n";

    // Show trusted visitors
    output += "\nTrusted Visitors: ";
    let visitors = lease.trusted_visitors;
    if visitors.len() == 0 {
        output += "(none)\n";
    } else {
        let visitor_str = "";
        for v in visitors {
            if visitor_str != "" { visitor_str += ", "; }
            visitor_str += v;
        }
        output += visitor_str + "\n";
    }

    output += "\nCommands:\n";
    output += "  property access <none|visit|full>\n";
    output += "  property trust <player name>\n";
    output += "  property untrust <player name>";

    send_client_message(connection_id, output);
}

fn handle_access(lease, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: property access <none|visit|full>\n\n  none - No party access\n  visit - Grouped players can enter and look\n  full - Grouped players have full access");
        return;
    }

    let level = parts[1].to_lower();
    if level != "none" && level != "visit" && level != "full" {
        send_client_message(connection_id, "Invalid access level. Use: none, visit, or full");
        return;
    }

    if set_property_access(lease.id, level) {
        let desc = "No party access";
        if level == "visit" { desc = "Grouped players can enter and look around"; }
        if level == "full" { desc = "Grouped players have full access"; }
        send_client_message(connection_id, "Party access set to '" + level + "'.\n" + desc);
    } else {
        send_client_message(connection_id, "Failed to update party access.");
    }
}

fn handle_trust(lease, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: property trust <player name>");
        return;
    }

    // Join remaining parts as the player name
    let name = "";
    for i in 1..parts.len() {
        if name != "" { name += " "; }
        name += parts[i];
    }

    if add_trusted_visitor(lease.id, name) {
        send_client_message(connection_id, name + " added to trusted visitors.\nThey now have full access to your property.");
    } else {
        send_client_message(connection_id, "Failed to add trusted visitor.");
    }
}

fn handle_untrust(lease, parts, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: property untrust <player name>");
        return;
    }

    // Join remaining parts as the player name
    let name = "";
    for i in 1..parts.len() {
        if name != "" { name += " "; }
        name += parts[i];
    }

    if remove_trusted_visitor(lease.id, name) {
        send_client_message(connection_id, name + " removed from trusted visitors.");
    } else {
        send_client_message(connection_id, "Failed to remove trusted visitor.");
    }
}
