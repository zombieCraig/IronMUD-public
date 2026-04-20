// scripts/commands/endlease.rhai
// endlease - Voluntarily end your property lease
// endlease confirm - Confirm and end the lease

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Find leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.\nYou must be at a leasing office to end your lease.");
        return;
    }

    // Get area_id from agent (uses fallback to prototype for spawned instances)
    let area_id = get_agent_leasing_area(agent.id);
    if area_id == "" {
        send_client_message(connection_id, "This leasing agent is not properly configured.");
        return;
    }

    // Get player's lease in this area
    let lease = get_player_lease_in_area(char.name, area_id);
    if lease == () {
        send_client_message(connection_id, "You don't have a property in this area.");
        return;
    }

    // Check if already evicted
    if lease.is_evicted {
        send_client_message(connection_id, "Your property was already evicted.");
        return;
    }

    // Get template for name
    let template = get_property_template(lease.template_vnum);
    let property_name = "your property";
    if template != () {
        property_name = template.name;
    }

    // Count items
    let item_count = count_items_in_property(lease.id);

    // Check for confirmation
    let is_confirm = args.to_lower() == "confirm";

    if !is_confirm {
        let output = "=== End Lease ===\n\n";
        output += "Property: " + property_name + "\n";
        output += "Items inside: " + item_count + "\n\n";

        if item_count > 0 {
            output += "WARNING: Your items will be moved to escrow.\n";
            output += "You will have 30 days to retrieve them for a small fee.\n\n";
        } else {
            output += "No items to store.\n\n";
        }

        output += "Type 'endlease confirm' to proceed.";
        send_client_message(connection_id, output);
        return;
    }

    // End the lease with escrow if there are items
    let to_escrow = item_count > 0;
    if end_lease(lease.id, to_escrow) {
        let output = "Your lease has been terminated.\n";
        if to_escrow {
            output += item_count + " items have been placed in escrow.\n";
            output += "Use 'escrow' to view and retrieve your items.";
        } else {
            output += "Thank you for renting with us!";
        }
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, "Failed to end your lease. Please contact an administrator.");
    }
}
