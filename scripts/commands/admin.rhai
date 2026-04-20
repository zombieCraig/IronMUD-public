// Admin command - powerful server administration functions
// Only accessible to admin users

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if !char.is_admin {
        send_client_message(connection_id, "This command requires admin access.");
        return;
    }

    // Parse subcommand
    let parts = args.split(" ");
    let subcommand = if parts.len() > 0 && parts[0] != "" {
        parts[0].to_lower()
    } else {
        ""
    };

    if subcommand == "" || subcommand == "help" {
        show_help(connection_id);
        return;
    }

    if subcommand == "kick" {
        handle_kick(connection_id, args, char);
        return;
    }

    if subcommand == "settime" {
        handle_settime(connection_id, args);
        return;
    }

    if subcommand == "broadcast" {
        handle_broadcast(connection_id, args);
        return;
    }

    if subcommand == "shutdown" {
        handle_shutdown(connection_id, args, char);
        return;
    }

    if subcommand == "cancel" {
        handle_cancel(connection_id);
        return;
    }

    if subcommand == "god" {
        handle_god(connection_id, char);
        return;
    }

    if subcommand == "summon" {
        handle_summon(connection_id, args, char);
        return;
    }

    if subcommand == "heal" {
        handle_heal(connection_id, args, char);
        return;
    }

    if subcommand == "user" {
        handle_user(connection_id, args, char);
        return;
    }

    if subcommand == "api-key" {
        handle_api_key(connection_id, args);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand. Type 'admin help' for commands.");
}

fn show_help(connection_id) {
    let help = "Admin Commands:\n";
    help += "  admin kick <player> [reason]      - Forcibly disconnect a player\n";
    help += "  admin summon <player>             - Teleport a player to your location\n";
    help += "  admin heal <player>               - Fully heal a player (HP, wounds, conditions)\n";
    help += "  admin settime <hour> <day> <month> <year> - Set the game time\n";
    help += "  admin broadcast <message>         - Send announcement to all players\n";
    help += "  admin shutdown <seconds> [reason] - Shutdown server with countdown\n";
    help += "  admin cancel                      - Abort a pending shutdown\n";
    help += "  admin god                         - Toggle god mode (invulnerability)\n";
    help += "  admin user ...                    - User management (type 'admin user' for help)\n";
    help += "  admin api-key ...                 - API key management (type 'admin api-key' for help)\n";
    help += "  admin help                        - Show this help\n";
    send_client_message(connection_id, help);
}

fn handle_kick(connection_id, args, admin_char) {
    // Parse: "kick <player> [reason]"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin kick <player> [reason]");
        return;
    }

    let target_name = parts[1];

    // Build reason from remaining args
    let reason = "";
    if parts.len() > 2 {
        let reason_parts = [];
        for i in 2..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }

    if reason == "" {
        reason = "No reason given";
    }

    // Call the kick_player function
    let result = kick_player(target_name, reason, admin_char.name);
    send_client_message(connection_id, result);
}

fn handle_settime(connection_id, args) {
    // Parse: "settime <hour> <day> <month> <year>"
    let parts = args.split(" ");
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: admin settime <hour> <day> <month> <year>");
        send_client_message(connection_id, "  hour:  0-23 (0=midnight, 12=noon)");
        send_client_message(connection_id, "  day:   1-30");
        send_client_message(connection_id, "  month: 1-12 (3-5=spring, 6-8=summer, 9-11=autumn, 12,1,2=winter)");
        send_client_message(connection_id, "  year:  1+");
        return;
    }

    // Parse numeric values
    let hour = parse_int(parts[1]);
    let day = parse_int(parts[2]);
    let month = parse_int(parts[3]);
    let year = parse_int(parts[4]);

    if hour == () || day == () || month == () || year == () {
        send_client_message(connection_id, "Error: All values must be valid integers.");
        return;
    }

    // Call admin_set_time which validates, sets time, and broadcasts messages
    let result = admin_set_time(hour, day, month, year);

    if !result.success {
        let error = result.get("error");
        if error == () {
            error = "Unknown error";
        }
        send_client_message(connection_id, "Error: " + error);
        return;
    }

    // Report what was set
    let gt = get_game_time();
    let msg = "Game time set to: ";
    msg += "Hour " + gt.hour + ", ";
    msg += "Day " + gt.day + ", ";
    msg += "Month " + gt.month + " (" + gt.season + "), ";
    msg += "Year " + gt.year;
    msg += " [" + gt.time_of_day + "]";
    send_client_message(connection_id, msg);

    // Fire triggers based on what changed
    let triggers_fired = 0;

    if result.time_changed {
        // Build time change context
        let ctx = #{
            old_time: result.old_time_of_day,
            new_time: result.new_time_of_day,
            is_dawn: (result.new_time_of_day == "Dawn").to_string(),
            is_dusk: (result.new_time_of_day == "Dusk").to_string(),
            is_night: (result.new_time_of_day == "Night").to_string(),
            is_day: (gt.is_daytime).to_string()
        };
        if fire_time_triggers("time_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Time of day changed: " + result.old_time_of_day + " -> " + result.new_time_of_day);
    }

    if result.season_changed {
        // Build season change context
        let ctx = #{
            old_season: result.old_season,
            new_season: result.new_season,
            is_spring: (result.new_season == "spring").to_string(),
            is_summer: (result.new_season == "summer").to_string(),
            is_autumn: (result.new_season == "autumn").to_string(),
            is_winter: (result.new_season == "winter").to_string()
        };
        if fire_time_triggers("season_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Season changed: " + result.old_season + " -> " + result.new_season);
    }

    if result.month_changed {
        // Build month change context
        let is_festival = (month == 4 || month == 8 || month == 12);
        let ctx = #{
            old_month: result.old_month.to_string(),
            new_month: result.new_month.to_string(),
            new_day: day.to_string(),
            new_year: year.to_string(),
            is_festival: is_festival.to_string()
        };
        if fire_time_triggers("month_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Month changed: " + result.old_month + " -> " + result.new_month);
    }

    if triggers_fired > 0 {
        send_client_message(connection_id, "Fired " + triggers_fired + " trigger type(s).");
    }
}

fn handle_broadcast(connection_id, args) {
    // Parse: "broadcast <message>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin broadcast <message>");
        return;
    }

    // Build message from all parts after "broadcast"
    let message_parts = [];
    for i in 1..parts.len() {
        message_parts.push(parts[i]);
    }
    let message = message_parts.join(" ");

    // Format the announcement
    let announcement = "\n*** SERVER ANNOUNCEMENT ***\n" + message + "\n***************************\n";

    // Broadcast to all players
    broadcast_to_all(announcement);

    send_client_message(connection_id, "Broadcast sent to all players.");
}

fn handle_shutdown(connection_id, args, admin_char) {
    // Parse: "shutdown <seconds> [reason]"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin shutdown <seconds> [reason]");
        send_client_message(connection_id, "  seconds: Countdown time before shutdown (0 for immediate)");
        send_client_message(connection_id, "  reason:  Optional reason for shutdown");
        return;
    }

    let delay = parse_int(parts[1]);
    if delay == () {
        send_client_message(connection_id, "Error: Delay must be a valid integer.");
        return;
    }

    if delay < 0 {
        send_client_message(connection_id, "Error: Delay cannot be negative.");
        return;
    }

    // Build reason from remaining args
    let reason = "Server maintenance";
    if parts.len() > 2 {
        let reason_parts = [];
        for i in 2..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }

    // Schedule the shutdown
    if schedule_shutdown(delay, reason, admin_char.name) {
        if delay == 0 {
            send_client_message(connection_id, "Shutdown initiated immediately.");
        } else {
            send_client_message(connection_id, "Shutdown scheduled in " + delay + " seconds.");
        }
    } else {
        send_client_message(connection_id, "Error: Failed to schedule shutdown.");
    }
}

fn handle_cancel(connection_id) {
    let result = cancel_shutdown();

    if result == "cancelled" {
        send_client_message(connection_id, "Shutdown cancelled.");
    } else {
        send_client_message(connection_id, "No shutdown is currently pending.");
    }
}

fn handle_god(connection_id, char) {
    let new_state = !char.god_mode;
    set_god_mode(connection_id, new_state);

    let room_id = char.current_room_id;

    if new_state {
        send_client_message(connection_id, "God mode ENABLED. You are now invulnerable.");
        broadcast_to_room_except(room_id, connection_id,
            char.name + " begins to emit a divine glow.");
    } else {
        send_client_message(connection_id, "God mode DISABLED. You are mortal once more.");
        broadcast_to_room_except(room_id, connection_id,
            "The glow around " + char.name + " fades away.");
    }
}

fn handle_summon(connection_id, args, admin_char) {
    // Parse: "summon <player>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin summon <player>");
        return;
    }

    let target_name = parts[1];

    // Can't summon yourself
    if target_name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot summon yourself.");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Could not find character data for '" + target_name + "'.");
        return;
    }

    // Check if already in same room
    if target_char.current_room_id == admin_char.current_room_id {
        send_client_message(connection_id, target_char.name + " is already here.");
        return;
    }

    let old_room_id = target_char.current_room_id;
    let new_room_id = admin_char.current_room_id;

    // Clear combat state if in combat
    if is_in_combat(target_char.name) {
        // Clear player's combat state
        exit_combat(target_char.name);

        // Clear any mobiles targeting this player in the old room
        let mobiles = get_mobiles_in_room(old_room_id);
        for mobile in mobiles {
            exit_mobile_combat(mobile.id);
        }

        send_client_message(target_conn, "Your combat has ended due to being summoned.");
    }

    // Announce departure from old room
    broadcast_to_room(old_room_id, target_char.name + " vanishes in a flash of light!", target_char.name);

    // Move the target character
    target_char.current_room_id = new_room_id;
    save_character_data(target_char);
    set_player_character(target_conn, target_char);

    // Announce arrival
    broadcast_to_room_except(new_room_id, target_conn,
        target_char.name + " appears in a flash of light!");

    // Notify target player
    send_client_message(target_conn, "You have been summoned by " + admin_char.name + "!");

    // Show target the new room
    display_room(new_room_id, target_conn, target_char.name);

    // Confirm to admin
    send_client_message(connection_id, "You have summoned " + target_char.name + " to your location.");
}

fn handle_heal(connection_id, args, admin_char) {
    // Parse: "heal <player>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin heal <player>");
        return;
    }

    let target_name = parts[1];

    // Allow "self" to heal yourself
    if target_name.to_lower() == "self" {
        target_name = admin_char.name;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Could not find character data for '" + target_name + "'.");
        return;
    }

    // Heal HP and stamina
    target_char.hp = target_char.max_hp;
    target_char.stamina = target_char.max_stamina;

    // Clear unconscious/bleedout state
    target_char.is_unconscious = false;
    target_char.bleedout_rounds_remaining = 0;

    // Clear all wounds and ongoing effects
    clear_character_wounds(target_char.name);
    clear_ongoing_effects(target_char.name);

    // Clear wetness and exposure
    target_char.is_wet = false;
    target_char.wet_level = 0;
    target_char.cold_exposure = 0;
    target_char.heat_exposure = 0;

    // Clear all conditions
    target_char.has_hypothermia = false;
    target_char.has_frostbite = [];
    target_char.has_heat_exhaustion = false;
    target_char.has_heat_stroke = false;
    target_char.has_illness = false;
    target_char.illness_progress = 0;

    // Save and update session
    save_character_data(target_char);
    set_player_character(target_conn, target_char);

    // Notify target
    send_client_message(target_conn, "You feel a warm divine energy wash over you, healing all ailments!");

    // Confirm to admin
    send_client_message(connection_id, "Fully healed " + target_char.name + ".");
}

// ========== User Management Subcommand ==========

fn handle_user(connection_id, args, admin_char) {
    // Parse: "user <action> [name] [...]"
    let parts = args.split(" ");

    // parts[0] = "user", parts[1] = action, parts[2] = name, etc.
    let action = if parts.len() > 1 && parts[1] != "" {
        parts[1].to_lower()
    } else {
        ""
    };

    if action == "" || action == "help" {
        show_user_help(connection_id);
        return;
    }

    if action == "list" {
        handle_user_list(connection_id);
        return;
    }

    if action == "info" {
        handle_user_info(connection_id, parts);
        return;
    }

    if action == "grant-admin" {
        handle_user_grant_admin(connection_id, parts, admin_char);
        return;
    }

    if action == "revoke-admin" {
        handle_user_revoke_admin(connection_id, parts, admin_char);
        return;
    }

    if action == "grant-builder" {
        handle_user_grant_builder(connection_id, parts);
        return;
    }

    if action == "revoke-builder" {
        handle_user_revoke_builder(connection_id, parts);
        return;
    }

    if action == "password" {
        handle_user_password(connection_id, parts);
        return;
    }

    if action == "delete" {
        handle_user_delete(connection_id, parts, admin_char);
        return;
    }

    send_client_message(connection_id, "Unknown user action. Type 'admin user' for help.");
}

fn show_user_help(connection_id) {
    let help = "User Management Commands:\n";
    help += "  admin user list                   - List all characters with status\n";
    help += "  admin user info <name>            - Show detailed character info\n";
    help += "  admin user grant-admin <name>     - Grant admin privileges\n";
    help += "  admin user revoke-admin <name>    - Revoke admin privileges\n";
    help += "  admin user grant-builder <name>   - Grant builder privileges\n";
    help += "  admin user revoke-builder <name>  - Revoke builder privileges\n";
    help += "  admin user password <name>        - Require password change on next login\n";
    help += "  admin user delete <name>          - Delete a character (requires confirmation)\n";
    send_client_message(connection_id, help);
}

fn handle_user_list(connection_id) {
    let characters = list_all_characters();

    if characters.len() == 0 {
        send_client_message(connection_id, "No characters found.");
        return;
    }

    // Build header
    let output = "Name                 Admin      Builder    PwdChg     Online\n";
    output += "--------------------------------------------------------------\n";

    // Build each row
    for char in characters {
        // Pad name to 20 chars
        let name = char.name;
        while name.len() < 20 {
            name += " ";
        }
        if name.len() > 20 {
            name = name.sub_string(0, 20);
        }

        // Admin column (10 chars)
        let admin_str = if char.is_admin { "Yes" } else { "No" };
        while admin_str.len() < 10 {
            admin_str += " ";
        }

        // Builder column (10 chars)
        let builder_str = if char.is_builder { "Yes" } else { "No" };
        while builder_str.len() < 10 {
            builder_str += " ";
        }

        // Password change column (10 chars)
        let pwd_str = if char.must_change_password { "Yes" } else { "No" };
        while pwd_str.len() < 10 {
            pwd_str += " ";
        }

        // Online column
        let online_conn = find_player_connection_by_name(char.name);
        let online_str = if online_conn != "" { "Yes" } else { "No" };

        output += name + " " + admin_str + " " + builder_str + " " + pwd_str + " " + online_str + "\n";
    }

    output += "Total: " + characters.len() + " character(s)";
    send_client_message(connection_id, output);
}

fn handle_user_info(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user info <name>");
        return;
    }

    let target_name = parts[2];
    let char = get_character_data(target_name);

    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Check if online
    let online_conn = find_player_connection_by_name(char.name);
    let is_online = online_conn != "";

    // Get effective level
    let eff_level = get_effective_level(char.name);

    let output = "Character Details: " + char.name + "\n";
    output += "----------------------------------------\n";
    output += "Name: " + char.name + "\n";
    output += "Admin: " + (if char.is_admin { "Yes" } else { "No" }) + "\n";
    output += "Builder: " + (if char.is_builder { "Yes" } else { "No" }) + "\n";
    output += "Effective Level: " + eff_level + "\n";
    output += "Gold: " + char.gold + " (Bank: " + char.bank_gold + ")\n";
    output += "Location: " + char.current_room_id + "\n";
    output += "Online: " + (if is_online { "Yes" } else { "No" }) + "\n";
    output += "Password Change Required: " + (if char.must_change_password { "Yes" } else { "No" });

    send_client_message(connection_id, output);
}

fn handle_user_grant_admin(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user grant-admin <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-modification
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot modify your own admin status.");
        return;
    }

    if char.is_admin {
        send_client_message(connection_id, char.name + " is already an admin.");
        return;
    }

    if set_admin_flag(connection_id, char.name, true) {
        send_client_message(connection_id, "Granted admin privileges to '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "You have been granted admin privileges by " + admin_char.name + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to grant admin privileges.");
    }
}

fn handle_user_revoke_admin(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user revoke-admin <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-modification
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot modify your own admin status.");
        return;
    }

    if !char.is_admin {
        send_client_message(connection_id, char.name + " is not an admin.");
        return;
    }

    if set_admin_flag(connection_id, char.name, false) {
        send_client_message(connection_id, "Revoked admin privileges from '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "Your admin privileges have been revoked by " + admin_char.name + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to revoke admin privileges.");
    }
}

fn handle_user_grant_builder(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user grant-builder <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if char.is_builder {
        send_client_message(connection_id, char.name + " is already a builder.");
        return;
    }

    if set_builder_flag(connection_id, char.name, true) {
        send_client_message(connection_id, "Granted builder privileges to '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "You have been granted builder privileges.");
        }
    } else {
        send_client_message(connection_id, "Failed to grant builder privileges.");
    }
}

fn handle_user_revoke_builder(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user revoke-builder <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if !char.is_builder {
        send_client_message(connection_id, char.name + " is not a builder.");
        return;
    }

    if set_builder_flag(connection_id, char.name, false) {
        send_client_message(connection_id, "Revoked builder privileges from '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "Your builder privileges have been revoked.");
        }
    } else {
        send_client_message(connection_id, "Failed to revoke builder privileges.");
    }
}

fn handle_user_password(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user password <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if char.must_change_password {
        send_client_message(connection_id, char.name + " already has a password change pending.");
        return;
    }

    // Set the must_change_password flag
    char.must_change_password = true;
    if save_character_data(char) {
        send_client_message(connection_id, "Set password change requirement for '" + char.name + "'.");
        send_client_message(connection_id, "They will be prompted to change their password on next login.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "An administrator has flagged your account for a password change. Please change your password soon.");
        }
    } else {
        send_client_message(connection_id, "Failed to set password change requirement.");
    }
}

fn handle_user_delete(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user delete <name>");
        send_client_message(connection_id, "Note: To confirm deletion, type: admin user delete <name> confirm <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-deletion
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot delete your own character.");
        return;
    }

    // Check for confirmation
    let has_confirm = parts.len() >= 5 && parts[3].to_lower() == "confirm";
    let confirm_name = if parts.len() >= 5 { parts[4] } else { "" };

    if !has_confirm || confirm_name.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, "WARNING: You are about to delete character '" + char.name + "'.");
        send_client_message(connection_id, "This action is PERMANENT and cannot be undone.");

        // Check if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(connection_id, "NOTE: " + char.name + " is currently ONLINE. They will be kicked first.");
        }

        send_client_message(connection_id, "To confirm, type: admin user delete " + char.name + " confirm " + char.name);
        return;
    }

    // Check if online and kick if necessary
    let target_conn = find_player_connection_by_name(char.name);
    if target_conn != "" {
        kick_player(char.name, "Your character has been deleted by an administrator.", admin_char.name);
        send_client_message(connection_id, "Kicked " + char.name + " from the server.");
    }

    // Delete the character
    if delete_character(char.name) {
        send_client_message(connection_id, "Character '" + char.name + "' has been deleted.");
    } else {
        send_client_message(connection_id, "Failed to delete character '" + char.name + "'.");
    }
}

// ========== API Key Management Subcommand ==========

fn handle_api_key(connection_id, args) {
    let parts = args.split(" ");
    let action = get_api_key_action(parts);

    if action == "" || action == "help" {
        show_api_key_help(connection_id);
        return;
    }

    if action == "list" {
        handle_api_key_list(connection_id);
        return;
    }

    if action == "create" {
        handle_api_key_create(connection_id, parts);
        return;
    }

    if action == "show" {
        handle_api_key_show(connection_id, parts);
        return;
    }

    if action == "revoke" {
        handle_api_key_revoke(connection_id, parts);
        return;
    }

    if action == "enable" {
        handle_api_key_enable(connection_id, parts);
        return;
    }

    if action == "delete" {
        handle_api_key_delete(connection_id, parts);
        return;
    }

    send_client_message(connection_id, "Unknown api-key action. Type 'admin api-key' for help.");
}

fn get_api_key_action(parts) {
    // parts[0] = "api-key", parts[1] = action
    if parts.len() > 1 && parts[1] != "" {
        return parts[1].to_lower();
    }
    return "";
}

fn show_api_key_help(connection_id) {
    let help = "API Key Management Commands:\n";
    help += "  admin api-key list                                       - List all API keys\n";
    help += "  admin api-key create --name <n> --character <c> [flags]  - Create a new key\n";
    help += "    Flags: --read  --write  --admin\n";
    help += "  admin api-key show <uuid>                                - Show key details\n";
    help += "  admin api-key revoke <uuid>                              - Disable a key\n";
    help += "  admin api-key enable <uuid>                              - Re-enable a key\n";
    help += "  admin api-key delete <uuid> confirm                     - Permanently delete a key\n";
    send_client_message(connection_id, help);
}

fn handle_api_key_list(connection_id) {
    let keys = list_api_keys();

    if keys.len() == 0 {
        send_client_message(connection_id, "No API keys found.");
        return;
    }

    let output = "ID                                    Name                 Character        Enabled  Perms\n";
    output += "-----------------------------------------------------------------------------------------\n";

    for key in keys {
        let id_str = key.get("id");
        let name_str = pad_right(key.get("name"), 20);
        let char_str = pad_right(key.get("owner_character"), 15);
        let enabled_str = pad_right(if key.get("enabled") { "Yes" } else { "No" }, 8);

        let perms = "";
        if key.get("read") { perms += "r"; } else { perms += "-"; }
        if key.get("write") { perms += "w"; } else { perms += "-"; }
        if key.get("admin") { perms += "a"; } else { perms += "-"; }

        output += id_str + "  " + name_str + " " + char_str + " " + enabled_str + " " + perms + "\n";
    }

    output += "Total: " + keys.len() + " key(s)";
    send_client_message(connection_id, output);
}

fn handle_api_key_create(connection_id, parts) {
    // Parse flags: --name <n> --character <c> --read --write --admin
    let name = "";
    let character = "";
    let read = false;
    let write = false;
    let admin = false;

    let i = 2; // Start after "api-key create"
    while i < parts.len() {
        let flag = parts[i];
        if flag == "--name" {
            i += 1;
            if i < parts.len() { name = parts[i]; }
        }
        if flag == "--character" {
            i += 1;
            if i < parts.len() { character = parts[i]; }
        }
        if flag == "--read" { read = true; }
        if flag == "--write" { write = true; }
        if flag == "--admin" { admin = true; }
        i += 1;
    }

    if name == "" {
        send_client_message(connection_id, "Error: --name is required.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> [--read] [--write] [--admin]");
        return;
    }

    if character == "" {
        send_client_message(connection_id, "Error: --character is required.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> [--read] [--write] [--admin]");
        return;
    }

    if !read && !write && !admin {
        send_client_message(connection_id, "Error: At least one permission flag is required (--read, --write, or --admin).");
        return;
    }

    if admin && !read && !write {
        send_client_message(connection_id, "Error: --admin alone has no effect. It only bypasses area ownership checks for --read or --write operations.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> --read [--write] [--admin]");
        return;
    }

    let result = create_api_key(name, character, read, write, admin);

    if !result.get("success") {
        let error = result.get("error");
        if error == () { error = "Unknown error"; }
        send_client_message(connection_id, "Error creating API key: " + error);
        return;
    }

    let output = "API key created successfully!\n\n";
    output += "Key ID: " + result.get("key_id") + "\n";
    output += "Name: " + name + "\n";
    output += "Character: " + character + "\n";
    output += "Permissions: read=" + read + ", write=" + write + ", admin=" + admin + "\n\n";
    output += "=== IMPORTANT: Save this key now! It cannot be recovered! ===\n\n";
    output += "API Key: " + result.get("raw_key") + "\n";
    send_client_message(connection_id, output);
}

fn handle_api_key_show(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key show <uuid>");
        return;
    }

    let key_id = parts[2];
    let key = get_api_key_info(key_id);

    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    let output = "API Key Details\n";
    output += "----------------------------------------\n";
    output += "ID: " + key.get("id") + "\n";
    output += "Name: " + key.get("name") + "\n";
    output += "Character: " + key.get("owner_character") + "\n";
    output += "Enabled: " + (if key.get("enabled") { "Yes" } else { "No" }) + "\n";
    output += "Permissions:\n";
    output += "  Read: " + (if key.get("read") { "Yes" } else { "No" }) + "\n";
    output += "  Write: " + (if key.get("write") { "Yes" } else { "No" }) + "\n";
    output += "  Admin: " + (if key.get("admin") { "Yes" } else { "No" }) + "\n";
    output += "Created: " + key.get("created_at") + "\n";
    let last_used = key.get("last_used_at");
    if last_used == () {
        output += "Last Used: Never";
    } else {
        output += "Last Used: " + last_used;
    }
    send_client_message(connection_id, output);
}

fn handle_api_key_revoke(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key revoke <uuid>");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    if !key.get("enabled") {
        send_client_message(connection_id, "API key '" + key.get("name") + "' is already revoked.");
        return;
    }

    if revoke_api_key(key_id) {
        send_client_message(connection_id, "Revoked API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to revoke API key.");
    }
}

fn handle_api_key_enable(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key enable <uuid>");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    if key.get("enabled") {
        send_client_message(connection_id, "API key '" + key.get("name") + "' is already enabled.");
        return;
    }

    if enable_api_key(key_id) {
        send_client_message(connection_id, "Enabled API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to enable API key.");
    }
}

fn handle_api_key_delete(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key delete <uuid> confirm");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    // Require "confirm" keyword
    let has_confirm = parts.len() >= 4 && parts[3].to_lower() == "confirm";
    if !has_confirm {
        send_client_message(connection_id, "WARNING: This will permanently delete API key '" + key.get("name") + "'.");
        send_client_message(connection_id, "To confirm, type: admin api-key delete " + key_id + " confirm");
        return;
    }

    if delete_api_key_by_id(key_id) {
        send_client_message(connection_id, "Deleted API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to delete API key.");
    }
}

fn pad_right(text, width) {
    let s = "" + text;
    while s.len() < width {
        s += " ";
    }
    if s.len() > width {
        s = s.sub_string(0, width);
    }
    return s;
}
