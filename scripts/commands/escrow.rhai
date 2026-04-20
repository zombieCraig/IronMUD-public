// scripts/commands/escrow.rhai
// escrow - View your escrow storage
// escrow retrieve <number> - Retrieve items from escrow (requires leasing office)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let escrows = get_player_escrow(char.name);

    if args == "" {
        show_escrow_list(escrows, char, connection_id);
        return;
    }

    let parts = args.split(" ");
    let subcommand = parts[0].to_lower();

    if subcommand == "retrieve" {
        handle_retrieve(escrows, parts, char, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand + "\nUse 'escrow' to see your stored items.");
}

fn show_escrow_list(escrows, char, connection_id) {
    if escrows.len() == 0 {
        send_client_message(connection_id, "You have no items in escrow storage.");
        return;
    }

    let output = "=== Your Escrow Storage ===\n\n";

    let index = 1;
    for escrow in escrows {
        let items = get_escrow_items(escrow.id);
        let days_remaining = calculate_days_remaining(escrow.expires_at);

        output += "Escrow #" + index + ":\n";
        output += "  Items: " + items.len() + " items stored\n";
        let local_fee = escrow.retrieval_fee / 10;
        if local_fee < 1 { local_fee = 1; }
        output += "  Retrieval Fee: " + escrow.retrieval_fee + " gold (" + local_fee + " gold if re-rented locally)\n";
        output += "  Expires: " + days_remaining + " days remaining\n";

        // Show item list
        if items.len() > 0 {
            output += "  Contents: ";
            let item_str = "";
            let count = 0;
            for item_name in items {
                if count > 0 { item_str += ", "; }
                if count >= 5 {
                    item_str += "...";
                    break;
                }
                item_str += item_name;
                count += 1;
            }
            output += item_str + "\n";
        }
        output += "\n";
        index += 1;
    }

    output += "Use 'escrow retrieve <number>' at a leasing office to retrieve items.\n";
    output += "Your gold: " + char.gold;

    send_client_message(connection_id, output);
}

fn handle_retrieve(escrows, parts, char, connection_id) {
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: escrow retrieve <number>\nExample: escrow retrieve 1");
        return;
    }

    let index_str = parts[1];
    let index = parse_int(index_str);
    if index == () || index < 1 || index > escrows.len() {
        send_client_message(connection_id, "Invalid escrow number. Use 'escrow' to see available entries.");
        return;
    }

    let escrow = escrows[index - 1];

    // Check for leasing agent in the room
    let agent = find_leasing_agent_in_room(char.current_room_id);
    if agent == () {
        send_client_message(connection_id, "You must be at a leasing office to retrieve escrow items.");
        return;
    }

    // Get area_id from agent (uses fallback to prototype for spawned instances)
    let area_id = get_agent_leasing_area(agent.id);
    if area_id == "" {
        send_client_message(connection_id, "This leasing agent is not properly configured.");
        return;
    }

    // Determine destination: same-area property > any property > current room
    let dest_id = "";
    let dest_msg = "";
    let same_area = false;

    // First try: property in the same area as this leasing agent
    let lease = get_player_lease_in_area(char.name, area_id);
    if lease != () && !lease.is_evicted {
        dest_id = get_property_entrance(lease.id);
        dest_msg = "Your belongings have been delivered to your property entrance.";
        // Check if the escrow originated from this same area (local retrieval)
        let source_lease = get_lease(escrow.source_lease_id);
        if source_lease != () && source_lease.area_id == area_id {
            same_area = true;
        }
    }

    // Second try: any active property the player owns
    if dest_id == "" {
        let all_leases = get_all_player_leases(char.name);
        for l in all_leases {
            if !l.is_evicted {
                let eid = get_property_entrance(l.id);
                if eid != "" {
                    dest_id = eid;
                    dest_msg = "Your belongings have been shipped to your property.";
                    break;
                }
            }
        }
    }

    // Last resort: drop at current room (the leasing office)
    if dest_id == "" {
        dest_id = char.current_room_id;
        dest_msg = "Your belongings have been placed on the ground here.";
    }

    // Discounted fee for local retrieval (re-rented in same area)
    let full_fee = escrow.retrieval_fee;
    let fee = full_fee;
    if same_area {
        fee = full_fee / 10;
        if fee < 1 { fee = 1; }
    }

    // Check gold
    if char.gold < fee {
        let fee_label = "" + fee + " gold";
        if same_area {
            fee_label += " (local retrieval discount)";
        }
        send_client_message(connection_id, "You cannot afford the retrieval fee.\nRequired: " + fee_label + "\nYou have: " + char.gold + " gold");
        return;
    }

    // Deduct gold
    add_character_gold(char.name, -fee);

    // Retrieve items
    if retrieve_escrow_items(escrow.id, dest_id) {
        send_client_message(connection_id, "You pay " + fee + " gold and retrieve your items.\n" + dest_msg);
    } else {
        // Refund on failure
        add_character_gold(char.name, fee);
        send_client_message(connection_id, "Failed to retrieve items. Fee refunded.");
    }
}

fn calculate_days_remaining(expires_at) {
    let now = get_current_time();
    let remaining_secs = expires_at - now;
    if remaining_secs <= 0 {
        return 0;
    }
    // Convert seconds to days (real days, not game days)
    return remaining_secs / (24 * 60 * 60);
}
