// scripts/commands/properties.rhai
// properties - List available property templates at a leasing agent

fn format_availability(template, vnum) {
    if template.max_instances <= 0 {
        return "";
    }
    let active = count_template_instances(vnum);
    let available = template.max_instances - active;
    if available <= 0 {
        return "    (SOLD OUT)\n";
    }
    return "    Available: " + available + " of " + template.max_instances + "\n";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.");
        return;
    }

    // Get templates offered by this agent
    let template_vnums = get_agent_templates(agent.id);
    if template_vnums.len() == 0 {
        let output = "=== " + agent.name + " ===\n\n";
        output += "No properties are currently available for rent.\n";
        send_client_message(connection_id, output);
        return;
    }

    let output = "=== " + agent.name + " ===\n\n";
    output += "Available Properties:\n\n";

    let has_properties = false;
    for vnum in template_vnums {
        let template = get_property_template(vnum);
        if template == () {
            continue;
        }

        has_properties = true;
        output += "  [" + vnum + "] " + template.name + " - " + template.monthly_rent + " gold/month\n";

        if template.description != "" {
            output += "    " + template.description + "\n";
        }

        // Show level requirement if any
        if template.level_requirement > 0 {
            output += "    Level required: " + template.level_requirement + "\n";
        }

        // Show availability if max_instances is set
        output += format_availability(template, vnum);

        output += "\n";
    }

    if !has_properties {
        output += "  (no properties available)\n\n";
    }

    output += "Use 'tour <property>' to preview, 'rent <property>' to lease.\n";
    output += "Your gold: " + char.gold;

    send_client_message(connection_id, output);
}
