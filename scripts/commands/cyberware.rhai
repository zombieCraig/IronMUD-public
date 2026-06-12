// scripts/commands/cyberware.rhai
// Show installed chrome and the humanity ledger. Alias: chrome.
//
// Production install/uninstall/therapy flows are builder-wired (ripperdoc
// NPC dialogue/DG triggers calling install_cyberware / uninstall_cyberware /
// cyberware_therapy). The admin-gated subcommands here exist so admins can
// exercise the machinery without a ripperdoc in the world:
//   cyberware install <item>     - install cyberware from your inventory
//   cyberware grant <vnum>       - spawn + install free (no humanity cost)
//   cyberware uninstall <item>   - remove an installed piece to inventory
//   cyberware therapy <points>   - restore humanity (no gold charge)

fn humanity_band_label(pct) {
    if pct >= 70 { return "\x1b[32mintegrated\x1b[0m"; }
    if pct >= 30 { return "\x1b[33mstrained\x1b[0m"; }
    if pct >= 15 { return "\x1b[33mfraying\x1b[0m"; }
    if pct >= 1 { return "\x1b[31mdissociating\x1b[0m"; }
    return "\x1b[1;31mGONE\x1b[0m";
}

fn format_piece_line(piece) {
    let line = "  " + piece["short_desc"];
    if piece["foundation"] {
        line += " [" + piece["slots_used"] + "/" + piece["slots_total"] + " slots]";
    } else if piece["category"] == "fashionware" || piece["category"] == "internal_body" || piece["category"] == "external_body" || piece["category"] == "borgware" {
        // standalone — nothing extra
    } else {
        line += " [option]";
    }
    if piece["exclusive_tag"] != "" {
        line += " (" + piece["exclusive_tag"] + ")";
    }
    return line;
}

fn show_chrome(connection_id) {
    let state = get_cyberware_state(connection_id);
    if !state["success"] {
        send_client_message(connection_id, "You are pure meat — no chrome, no ledger.");
        return;
    }

    let output = "\x1b[1;36mChrome:\x1b[0m\n";
    output += "  Humanity (chrome): " + state["humanity"] + "/" + state["max_humanity"];
    output += " (" + state["pct"] + "%) — " + humanity_band_label(state["pct"]) + "\n";
    if state["cha_penalty"] > 0 {
        output += "  \x1b[31mThe metal is showing: -" + state["cha_penalty"] + " effective CHA.\x1b[0m\n";
    }
    if state["episode_kind"] != "" {
        output += "  \x1b[1;31mEPISODE IN PROGRESS: " + state["episode_kind"] + "\x1b[0m\n";
    }

    let installed = state["installed"];
    if installed.len() == 0 {
        output += "  No cyberware installed.\n";
        send_client_message(connection_id, output);
        return;
    }

    output += "\x1b[1;36mInstalled cyberware:\x1b[0m\n";
    output += format_category_block(installed, "neuralware");
    output += format_category_block(installed, "cyberoptic");
    output += format_category_block(installed, "cyberaudio");
    output += format_category_block(installed, "cyberarm");
    output += format_category_block(installed, "cyberleg");
    output += format_category_block(installed, "internal_body");
    output += format_category_block(installed, "external_body");
    output += format_category_block(installed, "fashionware");
    output += format_category_block(installed, "borgware");
    send_client_message(connection_id, output);
}

fn format_category_block(installed, category) {
    let block = "";
    for piece in installed {
        if piece["category"] == category {
            if block == "" {
                block = " \x1b[36m" + category + "\x1b[0m\n";
            }
            block += format_piece_line(piece) + "\n";
        }
    }
    return block;
}

fn handle_admin_subcommand(verb, rest, connection_id) {
    if verb == "install" {
        let result = install_cyberware(connection_id, rest);
        if !result["success"] {
            send_client_message(connection_id, "Install failed: " + result["error"]);
            return;
        }
        send_client_message(connection_id, "Installed " + result["name"] + " (humanity -" + result["humanity_paid"] + ", now " + result["humanity"] + "/" + result["max_humanity"] + ").");
        return;
    }
    if verb == "grant" {
        let result = install_cyberware_free(connection_id, rest);
        if !result["success"] {
            send_client_message(connection_id, "Grant failed: " + result["error"]);
            return;
        }
        send_client_message(connection_id, "Granted " + result["name"] + " (free; humanity " + result["humanity"] + "/" + result["max_humanity"] + ").");
        return;
    }
    if verb == "uninstall" {
        let result = uninstall_cyberware(connection_id, rest);
        if !result["success"] {
            send_client_message(connection_id, "Uninstall failed: " + result["error"]);
            return;
        }
        send_client_message(connection_id, "Removed " + result["name"] + " to your inventory (humanity " + result["humanity"] + "/" + result["max_humanity"] + ").");
        return;
    }
    if verb == "therapy" {
        let points = try_parse_int(rest);
        if points == () || points <= 0 {
            send_client_message(connection_id, "Usage: cyberware therapy <points>");
            return;
        }
        let result = cyberware_therapy(connection_id, points);
        if !result["success"] {
            send_client_message(connection_id, "Therapy failed: " + result["error"]);
            return;
        }
        send_client_message(connection_id, "Therapy restored " + result["restored"] + " humanity (now " + result["humanity"] + "/" + result["max_humanity"] + ").");
        return;
    }
    send_client_message(connection_id, "Usage: cyberware [install|grant|uninstall|therapy] ...");
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        show_chrome(connection_id);
        return;
    }

    // Subcommands are admin tooling — production installs go through
    // ripperdoc NPCs that builders wire to the same capabilities.
    if !char.is_admin {
        send_client_message(connection_id, "Chrome goes in at a ripperdoc's table, not by wishing.");
        return;
    }

    let parts = args.split(" ");
    let verb = parts[0];
    let rest = "";
    if parts.len() > 1 {
        rest = args.sub_string(verb.len() + 1);
    }
    handle_admin_subcommand(verb, rest, connection_id);
}
