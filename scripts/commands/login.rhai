// scripts/commands/login.rhai

fn run_command(args, connection_id) {
    let mode = get_olc_mode(connection_id);

    // If in migration prompt mode, handle that first
    if mode == "migration_prompt" {
        handle_migration_prompt(args, connection_id);
        return;
    }

    // If in race migration prompt mode, handle that
    if mode == "race_migration_prompt" {
        handle_race_migration_prompt(args, connection_id);
        return;
    }

    // If in race migration selection mode, handle that
    if mode == "race_migration_select" {
        handle_race_migration_select(args, connection_id);
        return;
    }

    // If in wizard mode (from migration option 2), handle wizard input
    if mode != "" && mode.index_of("chargen_") == 0 {
        handle_wizard_input(args, connection_id, mode);
        return;
    }

    let parts = args.split(" ");
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: login <character_name> <password>");
        return;
    }

    let name = parts[0];
    let password = parts[1];

    if name == "" {
        send_client_message(connection_id, "Character name cannot be empty.");
        return;
    }

    if password == "" {
        send_client_message(connection_id, "Password cannot be empty.");
        return;
    }

    // Retrieve character data
    let existing_char = get_character_data(name);

    if existing_char == () {
        send_client_message(connection_id, "Character '" + name + "' not found.");
        return;
    }

    // Check if account is locked out
    let lockout_remaining = check_lockout(name);
    if lockout_remaining > 0 {
        send_client_message(connection_id, "Too many failed attempts. Try again in " + format_lockout_time(lockout_remaining) + ".");
        return;
    }

    // Verify password
    let password_hash = existing_char.password_hash;
    let is_valid = verify_password(password, password_hash);

    if !is_valid {
        let result = record_failed_attempt(name);
        send_client_message(connection_id, result.message);
        return;
    }

    // Successful login - clear any lockout data
    clear_lockout_data(name);

    // Authentication successful - check if migration is needed
    // Migration needed if: creation_complete is false AND race is empty
    if !existing_char.creation_complete && existing_char.race == "" {
        // Store character name for migration flow
        set_wizard_data(connection_id, "char_name=" + name + ";migration=true");
        set_olc_mode(connection_id, "migration_prompt");
        show_migration_prompt(connection_id, name);
        return;
    }

    // Check if character's race is valid (race system update migration)
    if !is_valid_race(existing_char.race) {
        set_wizard_data(connection_id, "char_name=" + name + ";migration=race");
        set_olc_mode(connection_id, "race_migration_prompt");
        show_race_migration_prompt(connection_id, name, existing_char.race);
        return;
    }

    // No migration needed - complete login
    complete_login(connection_id, existing_char);
}

fn show_migration_prompt(connection_id, char_name) {
    let output = "\n=== Character Update Required ===\n\n";
    output += "The character creation system has been updated with new options:\n";
    output += "  - Race selection\n";
    output += "  - Character description\n";
    output += "  - Class/profession selection\n";
    output += "  - Personality traits\n";
    output += "\nWould you like to:\n";
    output += "  [1] Apply defaults and continue playing\n";
    output += "  [2] Customize your character now\n";
    output += "\nEnter 1 or 2: ";

    send_client_message(connection_id, output);
}

fn show_race_migration_prompt(connection_id, char_name, old_race) {
    let output = "\n=== Race System Update ===\n\n";
    output += "The race system has been updated with mechanical effects!\n";
    output += "Your current race '" + old_race + "' needs to be updated to one of the\n";
    output += "predefined races with stat bonuses, resistances, and abilities.\n";
    output += "\nWould you like to:\n";
    output += "  [1] Become Human and continue playing\n";
    output += "  [2] Choose a race from the full list\n";
    output += "\nEnter 1 or 2: ";
    send_client_message(connection_id, output);
}

fn handle_race_migration_prompt(input, connection_id) {
    let lower = input.to_lower();
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    if lower == "1" {
        char_data.race = "human";
        save_character_data(char_data);
        set_olc_mode(connection_id, "");
        clear_wizard_data(connection_id);
        send_client_message(connection_id, "\nRace set to Human. Continuing login...\n");
        complete_login(connection_id, char_data);
        return;
    }

    if lower == "2" {
        set_olc_mode(connection_id, "race_migration_select");
        show_race_migration_selection(connection_id);
        return;
    }

    send_client_message(connection_id, "Please enter 1 or 2: ");
}

fn show_race_migration_selection(connection_id) {
    let races = get_race_list();
    let output = "\n=== Select a Race ===\n\n";

    let idx = 1;
    for race_id in races {
        let info = get_race_info(race_id);
        output += format_race_entry_login(idx, info);
        idx += 1;
    }

    output += "\nEnter number to select:\n> ";
    send_client_message(connection_id, output);
}

fn format_race_entry_login(idx, info) {
    let line = "  [" + idx + "] " + info["name"];
    let stat_str = format_stat_mods_login(info["stat_modifiers"]);
    if stat_str != "" {
        line += " (" + stat_str + ")";
    }
    line += "\n      " + info["description"] + "\n";
    return line;
}

fn format_stat_mods_login(mods) {
    let keys = mods.keys();
    if keys.len() == 0 { return ""; }
    let parts = "";
    for k in keys {
        let v = mods[k];
        if parts != "" { parts += ", "; }
        let stat_upper = k.to_upper();
        if v > 0 {
            parts += "+" + v + " " + stat_upper;
        } else {
            parts += v + " " + stat_upper;
        }
    }
    return parts;
}

fn handle_race_migration_select(input, connection_id) {
    let lower = input.to_lower();
    let races = get_race_list();
    let idx = try_parse_int(lower);

    if idx == () || idx < 1 || idx > races.len() {
        send_client_message(connection_id, "Invalid selection. Enter a number:\n> ");
        return;
    }

    let race_id = races[idx - 1];
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);
    char_data.race = race_id;
    save_character_data(char_data);

    let info = get_race_info(race_id);
    set_olc_mode(connection_id, "");
    clear_wizard_data(connection_id);
    send_client_message(connection_id, "\nRace set to " + info["name"] + ". Logging in...\n");
    complete_login(connection_id, char_data);
}

fn handle_migration_prompt(input, connection_id) {
    let lower = input.to_lower();

    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    if lower == "1" {
        // Apply defaults
        char_data.race = "human";
        char_data.short_description = "A seasoned adventurer.";
        char_data.class_name = "unemployed";
        char_data.traits = [];
        char_data.trait_points = 10;
        char_data.creation_complete = true;
        save_character_data(char_data);

        // Clear migration state
        set_olc_mode(connection_id, "");
        clear_wizard_data(connection_id);

        send_client_message(connection_id, "\nDefaults applied! Continuing login...\n");
        complete_login(connection_id, char_data);
        return;
    }

    if lower == "2" {
        // Enter wizard mode
        // First apply random defaults so they have something to edit
        char_data.race = get_random_race();
        char_data.short_description = get_random_short_desc();
        char_data.class_name = "unemployed";
        if char_data.trait_points == 0 {
            char_data.trait_points = 10;  // Give points if they had none
        }
        save_character_data(char_data);

        set_olc_mode(connection_id, "chargen_menu");
        show_wizard_menu(connection_id, char_data);
        return;
    }

    send_client_message(connection_id, "Please enter 1 or 2: ");
}

fn complete_login(connection_id, char_data) {
    // Check for duplicate login - kick existing session if found
    let existing_conn = find_player_connection_by_name(char_data.name);
    if existing_conn != "" {
        if existing_conn == connection_id {
            // Already logged in on this connection - just refresh the view
            send_client_message(connection_id, "You are already logged in as " + char_data.name + ".");
            display_room(char_data.current_room_id, connection_id, char_data.name);
            return;
        }
        // Another connection is using this character - disconnect them
        disconnect_existing_session(existing_conn, char_data.name);
    }

    set_player_character(connection_id, char_data);

    // Safety net: verify room exists, relocate if not
    let room = get_room_data(char_data.current_room_id);
    if room == () {
        // Room doesn't exist, move to starting room
        char_data.current_room_id = "00000000-0000-0000-0000-000000000001";
        save_character_data(char_data);
        set_player_character(connection_id, char_data);

        // Check if this was likely a property eviction
        let escrows = get_player_escrow(char_data.name);
        if escrows.len() > 0 {
            send_client_message(connection_id, "\n[Property] Your property was evicted while you were away. Your belongings have been moved to storage.\nVisit a leasing office and use 'escrow' to view and retrieve your items.\n");
        }
    }

    // Migration: Initialize HP for existing characters that don't have it
    if char_data.max_hp == 0 {
        char_data.max_hp = 100;
        char_data.hp = 100;
        save_character_data(char_data);
        set_player_character(connection_id, char_data);
    }

    // Migration: Auto-enable mana for characters whose race has mana-costing abilities
    if char_data.race != "" && !char_data.mana_enabled {
        let race_info = get_race_info(char_data.race);
        let actives = race_info["active_abilities"];
        let needs_mana = false;
        for a in actives {
            if a["mana_cost"] > 0 { needs_mana = true; }
        }
        if needs_mana {
            char_data.mana_enabled = true;
            char_data.max_mana = 100;
            char_data.mana = 100;
            save_character_data(char_data);
            set_player_character(connection_id, char_data);
        }
    }

    // Auto-enable mana for mage class (magic skill users)
    if !char_data.mana_enabled {
        let magic_level = get_skill_level(char_data.name, "magic");
        if magic_level >= 1 {
            char_data.mana_enabled = true;
            let new_max = recalculate_max_mana(char_data.name);
            char_data = get_character_data(char_data.name);
            if char_data.mana == 0 {
                char_data.mana = char_data.max_mana;
                save_character_data(char_data);
            }
            set_player_character(connection_id, char_data);
        }
    }

    // Clear unconscious state from previous session
    set_unconscious(char_data.name, false);
    if char_data.hp <= 0 {
        char_data.hp = 1;
        save_character_data(char_data);
        set_player_character(connection_id, char_data);
    }

    send_client_message(connection_id, "Welcome, " + char_data.name + "!");

    // Check for must_change_password flag
    if char_data.must_change_password {
        send_client_message(connection_id, "\n*** WARNING: Your password must be changed. Use the 'password' command. ***\n");
    }

    // Check for unread mail
    let unread_mail = get_unread_mail_count(char_data.name);
    if unread_mail > 0 {
        let plural = "s";
        if unread_mail == 1 { plural = ""; }
        send_client_message(connection_id, "\n[MAIL] You have " + unread_mail + " unread message" + plural + ". Type 'mail list' to view.\n");
    }

    // Display MOTD if set
    let motd = get_setting("motd");
    if motd != "" {
        send_client_message(connection_id, "\n=== Message of the Day ===\n" + motd + "\n==========================\n");
    }

    // Announce spawn to others in room
    let char_name = char_data.name;
    let room_id = char_data.current_room_id;
    broadcast_to_room(room_id, char_name + " appears in a flash of light.", char_name);

    // Announce to Matrix room
    matrix_broadcast(char_name + " has entered the realm.");

    // Display the room
    display_room(room_id, connection_id, char_name);
}

fn disconnect_existing_session(old_conn_id, char_name) {
    let old_char = get_player_character(old_conn_id);

    if old_char != () {
        let room_id = old_char.current_room_id;

        // Save character data before disconnecting
        save_character_data(old_char);

        // Announce departure to room
        broadcast_to_room(room_id, char_name + " fades away into the ether.", char_name);

        // Announce to Matrix
        matrix_broadcast(char_name + " has been disconnected (duplicate login).");

        // Clear character from old session
        clear_player_character(old_conn_id);
    }

    // Send disconnect message to old connection
    send_client_message(old_conn_id, "\n*** Your session has been disconnected: Someone logged in from another location. ***\n");

    // Disconnect the old client
    disconnect_client(old_conn_id);
}

// === Wizard functions (for migration option 2) ===

fn show_wizard_menu(connection_id, char_data) {
    let output = "\n=== Character Update: " + char_data.name + " ===\n\n";

    let race_display = get_race_name(char_data.race);

    let desc_display = char_data.short_description;
    if desc_display == "" { desc_display = "(not set)"; }
    if desc_display.len() > 40 { desc_display = desc_display.sub_string(0, 37) + "..."; }

    let traits_display = format_traits(char_data.traits);

    output += "  [1] Race:        " + race_display + "\n";
    output += "  [2] Description: " + desc_display + "\n";
    output += "  [3] Class:       " + char_data.class_name + "\n";
    output += "  [4] Traits:      " + traits_display + " (Points: " + char_data.trait_points + ")\n";
    output += "\n";
    output += "  [R] Randomize all\n";
    output += "  [D] Done - Finish and login\n";
    output += "\nEnter number to edit, or R/D: ";

    send_client_message(connection_id, output);
}

fn handle_wizard_input(input, connection_id, mode) {
    let trimmed = input.to_lower();

    if mode == "chargen_menu" {
        handle_menu_input(trimmed, connection_id);
        return;
    }
    if mode == "chargen_race" {
        handle_race_input(input, connection_id);
        return;
    }
    if mode == "chargen_desc" {
        handle_desc_input(input, connection_id);
        return;
    }
    if mode == "chargen_class" {
        handle_class_input(trimmed, connection_id);
        return;
    }
    if mode == "chargen_traits" {
        handle_trait_input(input, connection_id);
        return;
    }
}

fn handle_menu_input(trimmed, connection_id) {
    if trimmed == "1" {
        show_race_selection_login(connection_id);
        set_olc_mode(connection_id, "chargen_race");
        return;
    }
    if trimmed == "2" {
        set_olc_mode(connection_id, "chargen_desc");
        send_client_message(connection_id, "\nEnter a short description of your character (or 'random', 'back'):\n> ");
        return;
    }
    if trimmed == "3" {
        show_class_selection(connection_id);
        set_olc_mode(connection_id, "chargen_class");
        return;
    }
    if trimmed == "4" {
        show_trait_selection(connection_id);
        set_olc_mode(connection_id, "chargen_traits");
        return;
    }
    if trimmed == "r" {
        randomize_all(connection_id);
        return;
    }
    if trimmed == "d" {
        finalize_migration(connection_id);
        return;
    }
    send_client_message(connection_id, "Invalid option. Enter 1-4, R, or D: ");
}

fn show_race_selection_login(connection_id) {
    let races = get_race_list();
    let output = "\n=== Select a Race ===\n\n";

    let idx = 1;
    for race_id in races {
        let info = get_race_info(race_id);
        output += "  [" + idx + "] " + info["name"];
        let mods = info["stat_modifiers"];
        let stat_str = format_stat_mods_login(mods);
        if stat_str != "" {
            output += " (" + stat_str + ")";
        }
        output += "\n      " + info["description"] + "\n";
        idx += 1;
    }

    if races.len() == 0 {
        output += "  (No races available)\n";
    }

    output += "\nEnter number to select, 'random', or 'back' to return:\n> ";
    send_client_message(connection_id, output);
}

fn handle_race_input(input, connection_id) {
    let lower = input.to_lower();

    if lower == "back" {
        return_to_menu(connection_id);
        return;
    }

    if lower == "random" {
        let race = get_random_race();
        update_char_field(connection_id, "race", race);
        let name = get_race_name(race);
        send_client_message(connection_id, "Race set to: " + name);
        return_to_menu(connection_id);
        return;
    }

    let races = get_race_list();
    let idx = try_parse_int(lower);

    if idx == () || idx < 1 || idx > races.len() {
        send_client_message(connection_id, "Invalid selection. Enter a number or 'back':\n> ");
        return;
    }

    let race_id = races[idx - 1];
    update_char_field(connection_id, "race", race_id);
    let info = get_race_info(race_id);
    send_client_message(connection_id, "Race set to: " + info["name"]);
    return_to_menu(connection_id);
}

fn handle_desc_input(input, connection_id) {
    let lower = input.to_lower();

    if lower == "back" {
        return_to_menu(connection_id);
        return;
    }

    if lower == "random" {
        let desc = get_random_short_desc();
        update_char_field(connection_id, "short_description", desc);
        send_client_message(connection_id, "Description set.");
        return_to_menu(connection_id);
        return;
    }

    if input == "" {
        send_client_message(connection_id, "Description cannot be empty. Enter a description or 'back':\n> ");
        return;
    }

    update_char_field(connection_id, "short_description", input);
    send_client_message(connection_id, "Description set.");
    return_to_menu(connection_id);
}

fn show_class_selection(connection_id) {
    let classes = get_class_list();
    let output = "\n=== Select a Class ===\n\n";

    let idx = 1;
    for class_id in classes {
        let info = get_class_info(class_id);
        output += "  [" + idx + "] " + info["name"] + " - " + info["description"] + "\n";
        idx += 1;
    }

    if classes.len() == 0 {
        output += "  (No classes available)\n";
    }

    output += "\nEnter number to select, 'back' to return:\n> ";
    send_client_message(connection_id, output);
}

fn handle_class_input(input, connection_id) {
    if input == "back" {
        return_to_menu(connection_id);
        return;
    }

    let classes = get_class_list();
    let idx = try_parse_int(input);

    if idx == () || idx < 1 || idx > classes.len() {
        send_client_message(connection_id, "Invalid selection. Enter a number or 'back':\n> ");
        return;
    }

    let class_id = classes[idx - 1];
    update_char_field(connection_id, "class_name", class_id);
    let info = get_class_info(class_id);
    send_client_message(connection_id, "Class set to: " + info["name"]);
    return_to_menu(connection_id);
}

fn show_trait_selection(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    let output = "\n=== Trait Selection (Points: " + char_data.trait_points + ") ===\n\n";

    output += "Your traits: " + format_traits(char_data.traits) + "\n\n";

    let traits = get_trait_list();
    if traits.len() == 0 {
        output += "(No traits available yet)\n";
    } else {
        output += "Available Traits:\n";
        let idx = 1;
        for trait_id in traits {
            let info = get_trait_info(trait_id);
            let cost_str = "";
            let cost_val = info["cost"];
            if cost_val > 0 {
                cost_str = "-" + cost_val + " pts";
            } else if cost_val < 0 {
                let gain = -cost_val;
                cost_str = "+" + gain + " pts";
            } else {
                cost_str = "0 pts";
            }
            let line = "  [" + idx + "] ";
            line += info["name"];
            line += " (" + cost_str + ") - ";
            line += info["description"] + "\n";
            output += line;
            idx += 1;
        }
    }

    output += "\nWARNING: Traits are PERMANENT once accepted!\n";
    output += "\nCommands:\n";
    output += "  <number> - Preview trait details\n";
    output += "  accept <number> - PERMANENTLY add trait\n";
    output += "  back - Return to menu\n";
    output += "\n> ";

    send_client_message(connection_id, output);
}

fn handle_trait_input(input, connection_id) {
    let lower = input.to_lower();

    if lower == "back" {
        return_to_menu(connection_id);
        return;
    }

    if lower.index_of("accept ") == 0 {
        let num_str = lower.sub_string(7, lower.len() - 7);
        let idx = try_parse_int(num_str);
        if idx != () {
            accept_trait(connection_id, idx);
        } else {
            send_client_message(connection_id, "Usage: accept <number>\n> ");
        }
        return;
    }

    let idx = try_parse_int(lower);
    if idx != () {
        preview_trait(connection_id, idx);
        return;
    }

    send_client_message(connection_id, "Commands: <number> to preview, 'accept <number>' to add trait, 'back' to return\n> ");
}

fn preview_trait(connection_id, idx) {
    let traits = get_trait_list();
    if idx < 1 || idx > traits.len() {
        send_client_message(connection_id, "Invalid trait number.\n> ");
        return;
    }

    let trait_id = traits[idx - 1];
    let info = get_trait_info(trait_id);

    let output = "\n--- " + info["name"] + " ---\n";
    output += info["description"] + "\n";
    output += "Category: " + info["category"] + "\n";
    if info["cost"] > 0 {
        output += "Cost: " + info["cost"] + " points\n";
    } else if info["cost"] < 0 {
        output += "Grants: " + (-info["cost"]) + " points\n";
    }
    output += "\nTo accept this trait permanently, type: accept " + idx + "\n> ";

    send_client_message(connection_id, output);
}

fn accept_trait(connection_id, idx) {
    let traits = get_trait_list();
    if idx < 1 || idx > traits.len() {
        send_client_message(connection_id, "Invalid trait number.\n> ");
        return;
    }

    let trait_id = traits[idx - 1];
    let info = get_trait_info(trait_id);

    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    for t in char_data.traits {
        if t == trait_id {
            send_client_message(connection_id, "You already have the " + info["name"] + " trait.\n> ");
            return;
        }
    }

    let new_points = char_data.trait_points - info["cost"];
    if new_points < 0 {
        send_client_message(connection_id, "Not enough trait points! You need " + info["cost"] + " but only have " + char_data.trait_points + ".\n> ");
        return;
    }

    let conflicts = info["conflicts_with"];
    let has_conflict = false;
    let conflict_msg = "";
    for c in conflicts {
        for t in char_data.traits {
            if t == c {
                let conflict_info = get_trait_info(c);
                conflict_msg = info["name"] + " conflicts with your ";
                conflict_msg += conflict_info["name"] + " trait.\n> ";
                has_conflict = true;
            }
        }
    }
    if has_conflict {
        send_client_message(connection_id, conflict_msg);
        return;
    }

    let new_traits = char_data.traits;
    new_traits.push(trait_id);
    char_data.traits = new_traits;
    char_data.trait_points = new_points;
    save_character_data(char_data);

    send_client_message(connection_id, "\nTrait '" + info["name"] + "' PERMANENTLY added! Points remaining: " + new_points);
    show_trait_selection(connection_id);
}

fn return_to_menu(connection_id) {
    set_olc_mode(connection_id, "chargen_menu");
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);
    show_wizard_menu(connection_id, char_data);
}

fn finalize_migration(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    // Mark creation as complete
    char_data.creation_complete = true;
    save_character_data(char_data);

    // Auto-enable mana if race has mana-costing abilities
    if char_data.race != "" {
        let race_info = get_race_info(char_data.race);
        let actives = race_info["active_abilities"];
        let needs_mana = false;
        for a in actives {
            if a["mana_cost"] > 0 { needs_mana = true; }
        }
        if needs_mana && !char_data.mana_enabled {
            char_data.mana_enabled = true;
            char_data.max_mana = 100;
            char_data.mana = 100;
            save_character_data(char_data);
        }
    }

    // Auto-enable mana for mage class
    if !char_data.mana_enabled {
        let magic_level = get_skill_level(char_data.name, "magic");
        if magic_level >= 1 {
            char_data.mana_enabled = true;
            let new_max = recalculate_max_mana(char_data.name);
            char_data = get_character_data(char_data.name);
            if char_data.mana == 0 {
                char_data.mana = char_data.max_mana;
                save_character_data(char_data);
            }
        }
    }

    // Clear wizard state
    set_olc_mode(connection_id, "");
    clear_wizard_data(connection_id);

    send_client_message(connection_id, "\nCharacter updated! Logging in...\n");
    complete_login(connection_id, char_data);
}

fn randomize_all(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    char_data.race = get_random_race();
    char_data.short_description = get_random_short_desc();
    char_data.class_name = "unemployed";
    // Don't randomize traits - they're permanent

    save_character_data(char_data);

    send_client_message(connection_id, "\nCharacter randomized! (Traits not changed - they are permanent)");
    show_wizard_menu(connection_id, char_data);
}

fn update_char_field(connection_id, field, value) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    if field == "race" { char_data.race = value; }
    else if field == "short_description" { char_data.short_description = value; }
    else if field == "class_name" { char_data.class_name = value; }

    save_character_data(char_data);
}

fn format_traits(traits) {
    if traits.len() == 0 { return "(none)"; }
    let result = "";
    for t in traits {
        if result != "" { result += ", "; }
        let info = get_trait_info(t);
        if info["name"] != () && info["name"] != "" {
            result += info["name"];
        } else {
            result += t;
        }
    }
    return result;
}

fn get_char_name_from_wizard(connection_id) {
    let wizard_data = get_wizard_data(connection_id);
    let parts = wizard_data.split(";");
    for part in parts {
        if part.index_of("char_name=") == 0 {
            return part.sub_string(10, part.len() - 10);
        }
    }
    return "";
}

// === Login Lockout Functions ===

// Get lockout data for a character
// Returns map with fail_count and lockout_until
fn get_lockout_data(char_name) {
    let key = "login_lockout:" + char_name.to_lower();
    let data = get_setting(key);

    let result = #{
        fail_count: 0,
        lockout_until: 0
    };

    if data == "" {
        return result;
    }

    // Parse "fail_count:N;lockout_until:TIMESTAMP"
    let parts = data.split(";");
    for part in parts {
        if part.index_of("fail_count:") == 0 {
            let val = part.sub_string(11, part.len() - 11);
            let parsed = try_parse_int(val);
            if parsed != () {
                result.fail_count = parsed;
            }
        }
        if part.index_of("lockout_until:") == 0 {
            let val = part.sub_string(14, part.len() - 14);
            let parsed = try_parse_int(val);
            if parsed != () {
                result.lockout_until = parsed;
            }
        }
    }

    return result;
}

// Save lockout data for a character
fn save_lockout_data(char_name, fail_count, lockout_until) {
    let key = "login_lockout:" + char_name.to_lower();
    let value = "fail_count:" + fail_count + ";lockout_until:" + lockout_until;
    set_setting(key, value);
}

// Clear lockout data for a character (on successful login)
fn clear_lockout_data(char_name) {
    let key = "login_lockout:" + char_name.to_lower();
    set_setting(key, "");
}

// Get current Unix timestamp
fn get_current_timestamp() {
    // Use game time's last_time_tick as a proxy for current time
    // Or we can use a simpler approach - get_game_time returns current info
    let gt = get_game_time();
    // We need real wall clock time for lockouts
    // The setting system timestamps could work, but let's use a dedicated approach
    // For now, use seconds since epoch from get_setting_or_default trick
    // Actually, we'll store/compare using a timestamp from the server
    // Let's use the forage timestamp approach - get current time
    let key = "_timestamp_probe";
    set_forage_timestamp("_timestamp_", key);
    let char_data = get_character_data("_timestamp_");
    if char_data == () {
        // Fallback: just return 0 and let it work anyway
        return 0;
    }
    let ts = 0;
    // This is getting complex - let's use a simpler approach
    // We'll rely on the existing timestamp infrastructure
    return get_current_time();
}

// Format seconds into human-readable time
fn format_lockout_time(seconds) {
    if seconds <= 0 {
        return "0 seconds";
    }
    if seconds >= 120 {
        let minutes = seconds / 60;
        return minutes + " minutes";
    }
    if seconds >= 60 {
        return "1 minute";
    }
    if seconds == 1 {
        return "1 second";
    }
    return seconds + " seconds";
}

// Check if account is currently locked out
// Returns 0 if not locked, or seconds remaining if locked
fn check_lockout(char_name) {
    let data = get_lockout_data(char_name);

    if data.lockout_until == 0 {
        return 0;
    }

    let now = get_current_time();
    if now >= data.lockout_until {
        return 0;
    }

    return data.lockout_until - now;
}

// Record a failed login attempt
// Returns map with is_locked and message
fn record_failed_attempt(char_name) {
    let data = get_lockout_data(char_name);
    let now = get_current_time();

    // If currently locked, don't increment (shouldn't happen but be safe)
    if data.lockout_until > 0 && now < data.lockout_until {
        let remaining = data.lockout_until - now;
        return #{
            is_locked: true,
            message: "Too many failed attempts. Try again in " + format_lockout_time(remaining) + "."
        };
    }

    // Clear old lockout if expired
    if data.lockout_until > 0 && now >= data.lockout_until {
        data.fail_count = 0;
        data.lockout_until = 0;
    }

    // Increment fail count
    data.fail_count += 1;

    // Get lockout duration from settings (default 600 seconds = 10 minutes)
    let duration_str = get_setting_or_default("login_lockout_duration", "600");
    let duration = try_parse_int(duration_str);
    if duration == () {
        duration = 600;
    }

    // Check if we've reached the limit (10 attempts)
    if data.fail_count >= 10 {
        data.lockout_until = now + duration;
        save_lockout_data(char_name, data.fail_count, data.lockout_until);

        // Log the lockout
        log_security_event("Account '" + char_name + "' locked after 10 failed login attempts");

        return #{
            is_locked: true,
            message: "Incorrect password. Account locked for " + format_lockout_time(duration) + "."
        };
    }

    // Not locked yet - save and return remaining attempts
    save_lockout_data(char_name, data.fail_count, 0);
    let remaining = 10 - data.fail_count;

    return #{
        is_locked: false,
        message: "Incorrect password. " + remaining + " attempt(s) remaining before lockout."
    };
}
