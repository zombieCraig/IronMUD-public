// scripts/commands/mfind.rhai
// mfind <keyword> - Search mobiles by keyword (builder command)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: mfind <keyword>");
        return;
    }

    let mobiles = search_mobiles(args);

    if mobiles.len() == 0 {
        send_client_message(connection_id, "No mobiles found matching: " + args);
        return;
    }

    let output = "=== Mobile Search Results for '" + args + "' ===\n\n";

    for mobile in mobiles {
        let display_name = mobile.name;
        if mobile.is_prototype {
            display_name = "[PROTO] " + display_name;
            if mobile.vnum != "" {
                display_name += " (" + mobile.vnum + ")";
            }
        }
        let name_link = mxp_or(
            mxp_menu(["medit " + mobile.id, "mspawn " + mobile.vnum], ["Edit Mobile", "Spawn Mobile"], display_name),
            display_name,
            connection_id
        );
        output += name_link + "\n";
        output += "  ID: " + mobile.id + "\n";
        output += "  Level: " + mobile.level + "  HP: " + mobile.max_hp + "\n\n";
    }

    send_mxp_message(connection_id, output);
}
