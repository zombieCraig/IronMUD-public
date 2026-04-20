// scripts/commands/rent.rhai
// rent <property> - Rent a property from a leasing agent
// rent <property> confirm - Confirm and finalize the rental

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: rent <property name>\n       rent <property name> confirm");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.");
        return;
    }

    // Parse arguments
    let parts = args.split(" ");
    let property_name = parts[0];
    let is_confirm = false;
    if parts.len() >= 2 && parts[parts.len() - 1].to_lower() == "confirm" {
        is_confirm = true;
        // Rebuild property name without "confirm"
        property_name = "";
        for i in 0..(parts.len() - 1) {
            if property_name != "" { property_name += " "; }
            property_name += parts[i];
        }
    }

    // Find the template by keyword
    let template_vnums = get_agent_templates(agent.id);
    let target_template = ();
    let search_term = property_name.to_lower();

    for vnum in template_vnums {
        let template = get_property_template(vnum);
        if template == () {
            continue;
        }

        // Match by vnum or name
        if vnum.to_lower() == search_term || template.name.to_lower().contains(search_term) {
            target_template = template;
            break;
        }
    }

    if target_template == () {
        send_client_message(connection_id, "No property matching '" + property_name + "' is available here.\nUse 'properties' to see what's available.");
        return;
    }

    // Get area_id from template directly
    let area_id = target_template.area_id;
    if area_id == () || area_id == "" {
        send_client_message(connection_id, "Template '" + target_template.vnum + "' has no area configured. Use 'pedit " + target_template.vnum + " area <area>'.");
        return;
    }

    // Check if player already has a lease in this area
    let existing_lease = get_player_lease_in_area(char.name, area_id);
    if existing_lease != () && !existing_lease.is_evicted {
        send_client_message(connection_id, "You already have a property in this area (" + existing_lease.template_vnum + ").\nYou must end your current lease first with 'endlease', or use 'upgrade' to transfer.");
        return;
    }

    // Check level requirement
    if target_template.level_requirement > 0 && char.level < target_template.level_requirement {
        send_client_message(connection_id, "You must be level " + target_template.level_requirement + " to rent " + target_template.name + ".\nYour level: " + char.level);
        return;
    }

    // Check max instances
    if target_template.max_instances > 0 {
        let active = count_template_instances(target_template.vnum);
        if active >= target_template.max_instances {
            send_client_message(connection_id, target_template.name + " is currently sold out. Check back later.");
            return;
        }
    }

    // Check gold
    let cost = target_template.monthly_rent;
    if char.gold < cost {
        send_client_message(connection_id, "You cannot afford " + target_template.name + ".\nRequired: " + cost + " gold\nYou have: " + char.gold + " gold");
        return;
    }

    // If not confirming, show the agreement
    if !is_confirm {
        let rent_days = get_setting_or_default("rent_period_game_days", "30");
        let output = "=== Rental Agreement ===\n\n";
        output += "Property: " + target_template.name + "\n";
        output += "Monthly Rent: " + cost + " gold\n";
        output += "Required Now: " + cost + " gold (" + rent_days + " game days upfront)\n";
        output += "Your Gold: " + char.gold + "\n\n";
        output += "Type 'rent " + target_template.vnum + " confirm' to proceed.";
        send_client_message(connection_id, output);
        return;
    }

    // Actually create the lease
    // Deduct gold first
    add_character_gold(char.name, -cost);

    // Create the lease (this instances the rooms)
    let lease = create_lease(target_template.vnum, char.name, agent.id, char.current_room_id);
    if lease == () {
        // Refund on failure
        add_character_gold(char.name, cost);
        send_client_message(connection_id, "Failed to create your property. Gold has been refunded.");
        return;
    }

    // Success!
    let rent_days = get_setting_or_default("rent_period_game_days", "30");
    let output = "Congratulations! You have rented '" + target_template.name + "'.\n";
    output += cost + " gold has been deducted.\n";
    output += "Rent is paid for " + rent_days + " game days.\n\n";
    output += "Use 'enter' to access your new home.";
    send_client_message(connection_id, output);
}
