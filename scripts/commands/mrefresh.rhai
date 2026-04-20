// mrefresh <id|vnum> - Refresh mobile(s) from prototype
// @access: builder

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: mrefresh <uuid|vnum>");
        send_client_message(connection_id, "  uuid - Refresh a specific instance");
        send_client_message(connection_id, "  vnum - Refresh all instances of that vnum");
        return;
    }

    let target = args;

    // Check if UUID (36 chars with dashes)
    if target.len() == 36 && target.contains("-") {
        let mobile = get_mobile_data(target);
        if mobile == () {
            send_client_message(connection_id, "No mobile found: " + target);
            return;
        }
        if mobile.is_prototype {
            send_client_message(connection_id, "Cannot refresh a prototype.");
            return;
        }
        let refreshed = refresh_mobile_from_prototype(target);
        if refreshed == () {
            send_client_message(connection_id, "Failed - prototype '" + mobile.vnum + "' not found.");
            return;
        }
        send_client_message(connection_id, "Refreshed " + refreshed.name + " from prototype '" + refreshed.vnum + "'.");
        return;
    }

    // Treat as vnum
    let prototype = get_mobile_by_vnum(target);
    if prototype == () {
        send_client_message(connection_id, "No prototype found with vnum: " + target);
        return;
    }
    if !prototype.is_prototype {
        send_client_message(connection_id, "'" + target + "' is not a prototype.");
        return;
    }

    let instances = get_mobile_instances_by_vnum(target);
    if instances.len() == 0 {
        send_client_message(connection_id, "No spawned instances found for vnum: " + target);
        return;
    }

    let count = 0;
    for instance in instances {
        if refresh_mobile_from_prototype(instance.id) != () {
            count += 1;
        }
    }
    send_client_message(connection_id, "Refreshed " + count + " instance(s) of '" + target + "'.");
}
