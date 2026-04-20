// orefresh <id|vnum> - Refresh item(s) from prototype
// @access: builder

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: orefresh <uuid|vnum>");
        send_client_message(connection_id, "  uuid - Refresh a specific instance (or all instances if given a prototype)");
        send_client_message(connection_id, "  vnum - Refresh all instances of that vnum");
        return;
    }

    let target = args;

    // Check if UUID (36 chars with dashes)
    if target.len() == 36 && target.contains("-") {
        let item = get_item_data(target);
        if item == () {
            send_client_message(connection_id, "No item found: " + target);
            return;
        }
        let vnum = item.vnum;
        if vnum == () {
            send_client_message(connection_id, "Item has no vnum - cannot refresh.");
            return;
        }
        if item.is_prototype {
            // Given a prototype UUID - refresh all instances of its vnum
            let instances = get_item_instances_by_vnum(vnum);
            if instances.len() == 0 {
                send_client_message(connection_id, "No spawned instances found for vnum: " + vnum);
                return;
            }
            let count = 0;
            for instance in instances {
                if refresh_item_from_prototype(instance.id) != () {
                    count += 1;
                }
            }
            send_client_message(connection_id, "Refreshed " + count + " instance(s) of '" + vnum + "'.");
            return;
        }
        // Given an instance UUID - refresh just this one
        let refreshed = refresh_item_from_prototype(target);
        if refreshed == () {
            send_client_message(connection_id, "Failed - prototype '" + vnum + "' not found.");
            return;
        }
        send_client_message(connection_id, "Refreshed " + refreshed.name + " from prototype '" + vnum + "'.");
        return;
    }

    // Treat as vnum - refresh all instances
    let instances = get_item_instances_by_vnum(target);
    if instances.len() == 0 {
        send_client_message(connection_id, "No spawned instances found for vnum: " + target);
        return;
    }

    let count = 0;
    for instance in instances {
        if refresh_item_from_prototype(instance.id) != () {
            count += 1;
        }
    }
    send_client_message(connection_id, "Refreshed " + count + " instance(s) of '" + target + "'.");
}
