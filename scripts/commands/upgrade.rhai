// scripts/commands/upgrade.rhai
// upgrade <property> - Transfer to a new property in the same area
// upgrade <property> confirm - Confirm and process the upgrade

fn calculate_transfer_fee(item_count) {
    // Base fee of 25 gold + 1 gold per item
    return 25 + item_count;
}

fn parse_args(args) {
    let parts = args.split(" ");
    let property_name = parts[0];
    let is_confirm = false;

    if parts.len() < 2 {
        return #{ "name": property_name, "confirm": false };
    }

    let last_part = parts[parts.len() - 1].to_lower();
    if last_part != "confirm" {
        return #{ "name": args, "confirm": false };
    }

    // Rebuild property name without "confirm"
    property_name = "";
    for i in 0..(parts.len() - 1) {
        if property_name != "" { property_name += " "; }
        property_name += parts[i];
    }
    return #{ "name": property_name, "confirm": true };
}

fn find_template_by_keyword(agent_id, search_term) {
    let template_vnums = get_agent_templates(agent_id);
    let lower_search = search_term.to_lower();

    for vnum in template_vnums {
        let template = get_property_template(vnum);
        if template == () { continue; }
        if vnum.to_lower() == lower_search { return template; }
        if template.name.to_lower().contains(lower_search) { return template; }
    }
    return ();
}

fn check_can_upgrade(target_template, char, connection_id) {
    // Check level requirement
    if target_template.level_requirement > 0 {
        if char.level < target_template.level_requirement {
            send_client_message(connection_id, "You must be level " + target_template.level_requirement + " to rent " + target_template.name + ".");
            return false;
        }
    }

    // Check max instances
    if target_template.max_instances > 0 {
        let active = count_template_instances(target_template.vnum);
        if active >= target_template.max_instances {
            send_client_message(connection_id, target_template.name + " is currently sold out.");
            return false;
        }
    }

    return true;
}

fn show_upgrade_preview(current_lease, target_template, char, connection_id) {
    let current_template = get_property_template(current_lease.template_vnum);
    let current_name = "your current property";
    let current_rent = current_lease.monthly_rent;
    if current_template != () {
        current_name = current_template.name;
    }

    let item_count = count_items_in_property(current_lease.id);
    let transfer_fee = calculate_transfer_fee(item_count);
    let first_month = target_template.monthly_rent;
    let total_cost = transfer_fee + first_month;

    let output = "=== Property Upgrade ===\n\n";
    output += "Current: " + current_name + " (" + current_rent + " gold/month)\n";
    output += "New: " + target_template.name + " (" + first_month + " gold/month)\n\n";
    output += "Items to transfer: " + item_count + "\n";
    output += "Transfer fee: " + transfer_fee + " gold\n";
    output += "First month rent: " + first_month + " gold\n";
    output += "Total cost: " + total_cost + " gold\n\n";
    output += "Your gold: " + char.gold + "\n\n";
    output += "Type 'upgrade " + target_template.vnum + " confirm' to proceed.";
    send_client_message(connection_id, output);
}

fn process_upgrade(current_lease, target_template, char, agent, connection_id) {
    let item_count = count_items_in_property(current_lease.id);
    let transfer_fee = calculate_transfer_fee(item_count);
    let first_month = target_template.monthly_rent;
    let total_cost = transfer_fee + first_month;

    // Check gold
    if char.gold < total_cost {
        send_client_message(connection_id, "You cannot afford the upgrade. Total: " + total_cost + " gold");
        return;
    }

    // Deduct gold
    add_character_gold(char.name, -total_cost);

    // Create new lease
    let new_lease = create_lease(target_template.vnum, char.name, agent.id, char.current_room_id);
    if new_lease == () {
        add_character_gold(char.name, total_cost);
        send_client_message(connection_id, "Failed to create your new property. Gold refunded.");
        return;
    }

    // Transfer items and delete old property
    let transferred = transfer_property_items(current_lease.id, new_lease.id);
    delete_lease_rooms(current_lease.id);

    let output = "Upgrade complete!\n\n";
    output += "You now rent: " + target_template.name + "\n";
    output += transferred + " items transferred.\n";
    output += total_cost + " gold deducted.\n\n";
    output += "Use 'enter' to access your new home.";
    send_client_message(connection_id, output);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: upgrade <property name>\n       upgrade <property name> confirm");
        return;
    }

    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "There is no leasing agent here.");
        return;
    }

    // Get area_id from agent (uses fallback to prototype for spawned instances)
    let area_id = get_agent_leasing_area(agent.id);
    if area_id == "" {
        send_client_message(connection_id, "This leasing agent is not properly configured.");
        return;
    }

    let current_lease = get_player_lease_in_area(char.name, area_id);
    if current_lease == () || current_lease.is_evicted {
        send_client_message(connection_id, "You don't have a property in this area to upgrade from.");
        return;
    }

    let parsed = parse_args(args);
    let target_template = find_template_by_keyword(agent.id, parsed.name);

    if target_template == () {
        send_client_message(connection_id, "No property matching '" + parsed.name + "' is available.");
        return;
    }

    if target_template.vnum == current_lease.template_vnum {
        send_client_message(connection_id, "You already have that property type.");
        return;
    }

    if !check_can_upgrade(target_template, char, connection_id) {
        return;
    }

    if !parsed.confirm {
        show_upgrade_preview(current_lease, target_template, char, connection_id);
        return;
    }

    process_upgrade(current_lease, target_template, char, agent, connection_id);
}
