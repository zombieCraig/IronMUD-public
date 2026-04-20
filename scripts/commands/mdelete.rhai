// scripts/commands/mdelete.rhai
// mdelete <mobile_id|vnum> [confirm] - Delete/despawn a mobile (builder command)
// Instances are despawned immediately; prototypes require 'confirm' argument

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: mdelete <mobile_id|vnum> [confirm]");
        return;
    }

    let parts = args.split(" ");
    let target = parts[0];
    let confirm = parts.len() > 1 && parts[1].to_lower() == "confirm";

    // Try to find mobile by ID first, then by vnum
    let mobile = get_mobile_data(target);
    if mobile == () {
        mobile = get_mobile_by_vnum(target);
    }

    if mobile == () {
        send_client_message(connection_id, "Mobile not found: " + target);
        return;
    }

    if mobile.is_prototype {
        // Prototype requires confirmation
        if !confirm {
            let msg = "WARNING: '" + mobile.name + "' is a PROTOTYPE.\n";
            msg += "Deleting will permanently remove it from disk.\n";
            msg += "To confirm, type: mdelete " + target + " confirm";
            send_client_message(connection_id, msg);
            return;
        }
        // Confirmed - delete prototype
        let name = mobile.name;
        if delete_mobile(mobile.id) {
            send_client_message(connection_id, "Permanently deleted prototype: " + name);
        } else {
            send_client_message(connection_id, "Failed to delete prototype.");
        }
    } else {
        // Instance - delete without confirmation (despawn)
        let name = mobile.name;
        if delete_mobile(mobile.id) {
            send_client_message(connection_id, "Despawned: " + name + " (will respawn at next spawn tick)");
        } else {
            send_client_message(connection_id, "Failed to despawn mobile.");
        }
    }
}
