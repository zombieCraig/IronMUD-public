// scripts/commands/create.rhai
// Character Creation Wizard

fn run_command(args, connection_id) {
    let mode = get_olc_mode(connection_id);

    // If already in wizard mode, handle input based on current state
    if mode != "" && mode.index_of("chargen_") == 0 {
        handle_wizard_input(args, connection_id, mode);
        return;
    }

    // Initial create command: create <name> <password>
    let parts = args.split(" ");
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: create <character_name> <password>");
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

    // Password cannot be the same as character name
    if password.to_lower() == name.to_lower() {
        send_client_message(connection_id, "Password cannot be the same as your character name.");
        return;
    }

    // Check if character already exists
    let existing = get_character_data(name);
    if existing != () {
        send_client_message(connection_id, "Character '" + name + "' already exists.");
        return;
    }

    // Check if this is the first character (will become admin)
    let is_first_character = count_characters() == 0;

    // Hash the password
    let password_hash = hash_password(password);

    // Create a new CharacterData with the starting room ID
    let new_char = new_character(name, password_hash, "00000000-0000-0000-0000-000000000001");

    // Apply defaults/random values
    new_char.race = get_random_race();
    new_char.short_description = get_random_short_desc();
    new_char.class_name = "unemployed";
    new_char.traits = [];
    new_char.trait_points = 10;
    new_char.creation_complete = false;

    // Save character (preliminary - can be deleted if cancelled)
    save_character_data(new_char);

    // First character becomes admin automatically (secured server-side)
    if is_first_character {
        set_first_character_privileges(name);
    }

    // Store wizard state as JSON-like string
    let wizard_state = "char_name=" + name + ";is_first=" + is_first_character;
    set_wizard_data(connection_id, wizard_state);

    // Enter wizard menu
    set_olc_mode(connection_id, "chargen_menu");

    // Reload character data and show menu
    let char_data = get_character_data(name);
    show_wizard_menu(connection_id, char_data);
}

fn show_wizard_menu(connection_id, char_data) {
    let output = "\n=== Character Creation: " + char_data.name + " ===\n\n";

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
    output += "  [D] Done - Finish character creation\n";
    output += "  [Q] Quit - Cancel and delete character\n";
    output += "\nEnter number to edit, or R/D/Q: ";

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
        show_race_selection(connection_id);
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
        finalize_character(connection_id);
        return;
    }
    if trimmed == "q" {
        cancel_creation(connection_id);
        return;
    }
    send_client_message(connection_id, "Invalid option. Enter 1-4, R, D, or Q: ");
}

// === Race Selection ===

fn show_race_selection(connection_id) {
    let races = get_race_list();
    let output = "\n=== Select a Race ===\n\n";

    let idx = 1;
    for race_id in races {
        let info = get_race_info(race_id);
        output += format_race_entry(idx, info);
        idx += 1;
    }

    if races.len() == 0 {
        output += "  (No races available)\n";
    }

    output += "\nEnter number to select, 'back' to return:\n> ";
    send_client_message(connection_id, output);
}

fn format_race_entry(idx, info) {
    let line = "  [" + idx + "] " + info["name"];
    let stat_str = format_stat_mods(info["stat_modifiers"]);
    if stat_str != "" {
        line += " (" + stat_str + ")";
    }
    line += "\n      " + info["description"] + "\n";
    line += format_race_details(info);
    return line;
}

fn format_race_details(info) {
    let details = "";
    let resist_str = format_resistances(info["resistances"]);
    if resist_str != "" {
        details += "      Resistances: " + resist_str + "\n";
    }
    let trait_str = format_granted_traits(info["granted_traits"]);
    if trait_str != "" {
        details += "      Racial Traits: " + trait_str + "\n";
    }
    let passives = info["passive_abilities"];
    if passives.len() > 0 {
        let pnames = "";
        for p in passives {
            if pnames != "" { pnames += ", "; }
            pnames += p["name"];
        }
        details += "      Passives: " + pnames + "\n";
    }
    let actives = info["active_abilities"];
    if actives.len() > 0 {
        let anames = "";
        for a in actives {
            if anames != "" { anames += ", "; }
            anames += a["name"];
        }
        details += "      Abilities: " + anames + "\n";
    }
    return details;
}

fn format_stat_mods(mods) {
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

fn format_resistances(resists) {
    let keys = resists.keys();
    if keys.len() == 0 { return ""; }
    let parts = "";
    for k in keys {
        let v = resists[k];
        if parts != "" { parts += ", "; }
        if v > 0 {
            parts += k + " +" + v + "%";
        } else {
            parts += k + " " + v + "%";
        }
    }
    return parts;
}

fn format_granted_traits(traits) {
    if traits.len() == 0 { return ""; }
    let parts = "";
    for t in traits {
        if parts != "" { parts += ", "; }
        let info = get_trait_info(t);
        if info["name"] != () && info["name"] != "" {
            parts += info["name"];
        } else {
            parts += t;
        }
    }
    return parts;
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
    let trimmed = input;
    let lower = trimmed.to_lower();

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

    if trimmed == "" {
        send_client_message(connection_id, "Description cannot be empty. Enter a description or 'back':\n> ");
        return;
    }

    update_char_field(connection_id, "short_description", trimmed);
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

    // Show current traits
    output += "Your traits: " + format_traits(char_data.traits) + "\n\n";

    // Show available traits
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

    output += "\nNote: Traits can be changed until you select [D] Done.\n";
    output += "\nCommands:\n";
    output += "  <number> - Preview trait details\n";
    output += "  accept <number> - Add trait to your character\n";
    output += "  remove <number> - Remove one of your traits\n";
    output += "  random - Randomize your trait selection\n";
    output += "  clear - Clear all traits and restore points\n";
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

    // Handle "random" command
    if lower == "random" {
        let count = randomize_traits(connection_id);
        send_client_message(connection_id, "\nRandomized! " + count + " traits selected.");
        show_trait_selection(connection_id);
        return;
    }

    // Handle "clear" command
    if lower == "clear" {
        clear_traits(connection_id);
        return;
    }

    // Check for "accept N" command
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

    // Check for "remove N" command
    if lower.index_of("remove ") == 0 {
        let num_str = lower.sub_string(7, lower.len() - 7);
        let idx = try_parse_int(num_str);
        if idx != () {
            remove_trait_chargen(connection_id, idx);
        } else {
            send_client_message(connection_id, "Usage: remove <number>\n> ");
        }
        return;
    }

    // Try to preview a trait by number
    let idx = try_parse_int(lower);
    if idx != () {
        preview_trait(connection_id, idx);
        return;
    }

    send_client_message(connection_id, "Commands: <number> to preview, 'accept/remove <number>', 'random', 'clear', 'back'\n> ");
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
    output += "\nTo accept this trait, type: accept " + idx + "\n> ";

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

    // Check if already has this trait
    for t in char_data.traits {
        if t == trait_id {
            send_client_message(connection_id, "You already have the " + info["name"] + " trait.\n> ");
            return;
        }
    }

    // Check if race already grants this trait
    let race_info = get_race_info(char_data.race);
    let granted = race_info["granted_traits"];
    if granted != () {
        for g in granted {
            if g == trait_id {
                send_client_message(connection_id, "Your race already grants " + info["name"] + " for free!\n> ");
                return;
            }
        }
    }

    // Check if can afford
    let new_points = char_data.trait_points - info["cost"];
    if new_points < 0 {
        send_client_message(connection_id, "Not enough trait points! You need " + info["cost"] + " but only have " + char_data.trait_points + ".\n> ");
        return;
    }

    // Check conflicts
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

    // Add the trait
    let new_traits = char_data.traits;
    new_traits.push(trait_id);
    char_data.traits = new_traits;
    char_data.trait_points = new_points;
    save_character_data(char_data);

    send_client_message(connection_id, "\nTrait '" + info["name"] + "' added! Points remaining: " + new_points);

    // Refresh the trait selection screen
    show_trait_selection(connection_id);
}

fn return_to_menu(connection_id) {
    set_olc_mode(connection_id, "chargen_menu");
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);
    show_wizard_menu(connection_id, char_data);
}

fn finalize_character(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    // Mark creation as complete
    char_data.creation_complete = true;
    save_character_data(char_data);

    // Apply class bonuses
    let class_info = get_class_info(char_data.class_name);
    // Apply stat bonuses to char_data directly
    apply_stat_bonus(char_data, class_info["stat_bonuses"]);
    save_character_data(char_data);
    // Apply starting skills
    let starting_skills = class_info["starting_skills"];
    let skill_keys = starting_skills.keys();
    for skill in skill_keys {
        let level = starting_skills[skill];
        set_skill_level(char_name, skill, level);
    }

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

    let is_first = is_first_from_wizard(connection_id);

    let output = "\nCharacter '" + char_name + "' created successfully!\n";
    if is_first {
        output += "You are the first user - you have been granted administrator status.\n";
    }
    send_client_message(connection_id, output);

    // Automatically log in the player
    set_player_character(connection_id, char_data);

    // Safety net: verify room exists, relocate if not
    let room = get_room_data(char_data.current_room_id);
    if room == () {
        char_data.current_room_id = "00000000-0000-0000-0000-000000000001";
        save_character_data(char_data);
        set_player_character(connection_id, char_data);
    }

    send_client_message(connection_id, "Welcome, " + char_data.name + "!\n");

    // Announce spawn to others in room
    let room_id = char_data.current_room_id;
    broadcast_to_room(room_id, char_name + " appears in a flash of light.", char_name);

    // Display the room
    display_room(room_id, connection_id, char_name);
}

fn cancel_creation(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);

    // Delete the character
    delete_character(char_name);

    // Clear wizard state
    set_olc_mode(connection_id, "");
    clear_wizard_data(connection_id);

    send_client_message(connection_id, "\nCharacter creation cancelled. Character deleted.\n");
}

// Helper functions

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
    // Parse char_name from "char_name=Name;is_first=true/false"
    let parts = wizard_data.split(";");
    for part in parts {
        if part.index_of("char_name=") == 0 {
            return part.sub_string(10, part.len() - 10);
        }
    }
    return "";
}

fn is_first_from_wizard(connection_id) {
    let wizard_data = get_wizard_data(connection_id);
    return wizard_data.index_of("is_first=true") >= 0;
}

// Shuffle an array using Fisher-Yates algorithm
fn shuffle_array(arr) {
    let result = [];
    for item in arr {
        result.push(item);
    }
    let len = result.len();
    if len <= 1 { return result; }

    let i = len - 1;
    while i > 0 {
        let j = random_int(0, i);
        // Swap elements
        let temp = result[i];
        result[i] = result[j];
        result[j] = temp;
        i -= 1;
    }
    return result;
}

// Check if a trait can be added without conflicts
fn can_add_trait(trait_id, selected_traits) {
    let info = get_trait_info(trait_id);
    let conflicts = info["conflicts_with"];

    for selected in selected_traits {
        // Check if new trait conflicts with existing
        for c in conflicts {
            if c == selected { return false; }
        }
        // Check if existing trait conflicts with new
        let selected_info = get_trait_info(selected);
        for c in selected_info["conflicts_with"] {
            if c == trait_id { return false; }
        }
    }
    return true;
}

// Randomize traits, spending ~80% of available points
fn randomize_traits(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    // Reset to initial state
    char_data.traits = [];
    char_data.trait_points = 10;

    // Get all available traits and separate by category
    let all_traits = get_trait_list();
    let positive_traits = [];
    let negative_traits = [];

    for trait_id in all_traits {
        let info = get_trait_info(trait_id);
        if info["cost"] > 0 {
            positive_traits.push(trait_id);
        } else if info["cost"] < 0 {
            negative_traits.push(trait_id);
        }
    }

    // Shuffle both lists
    positive_traits = shuffle_array(positive_traits);
    negative_traits = shuffle_array(negative_traits);

    let selected_traits = [];
    let current_points = 10;

    // Phase 1: Maybe add negative traits (40% chance each, max 2)
    let negative_count = 0;
    for trait_id in negative_traits {
        if negative_count >= 2 { break; }
        if random_int(1, 100) > 40 { continue; }

        if can_add_trait(trait_id, selected_traits) {
            selected_traits.push(trait_id);
            let info = get_trait_info(trait_id);
            current_points -= info["cost"]; // Subtracting negative = adding points
            negative_count += 1;
        }
    }

    // Phase 2: Add positive traits until ~80% of total budget spent
    // Target remaining is ~20% of current pool
    let target_remaining = current_points / 5;
    if target_remaining < 1 { target_remaining = 1; }

    for trait_id in positive_traits {
        let info = get_trait_info(trait_id);
        let cost = info["cost"];

        // Can we afford it and stay above minimum?
        if current_points - cost < 0 { continue; }

        // Check conflicts
        if can_add_trait(trait_id, selected_traits) {
            selected_traits.push(trait_id);
            current_points -= cost;

            // Stop if we're at or below target
            if current_points <= target_remaining { break; }
        }
    }

    // Apply selected traits
    char_data.traits = selected_traits;
    char_data.trait_points = current_points;
    save_character_data(char_data);

    return selected_traits.len();
}

// Remove a trait during character creation
fn remove_trait_chargen(connection_id, display_idx) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    if display_idx < 1 || display_idx > char_data.traits.len() {
        send_client_message(connection_id, "Invalid trait number. You have " + char_data.traits.len() + " traits.\n> ");
        return;
    }

    let trait_id = char_data.traits[display_idx - 1];
    let info = get_trait_info(trait_id);

    // Remove trait and refund points
    let new_traits = [];
    for t in char_data.traits {
        if t != trait_id {
            new_traits.push(t);
        }
    }
    char_data.traits = new_traits;
    char_data.trait_points += info["cost"]; // Refund (positive = gain back, negative = lose back)

    save_character_data(char_data);
    send_client_message(connection_id, "Removed '" + info["name"] + "'. Points: " + char_data.trait_points);
    show_trait_selection(connection_id);
}

// Clear all traits during character creation
fn clear_traits(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    char_data.traits = [];
    char_data.trait_points = 10;
    save_character_data(char_data);

    send_client_message(connection_id, "All traits cleared. Points restored to 10.");
    show_trait_selection(connection_id);
}

fn randomize_all(connection_id) {
    let char_name = get_char_name_from_wizard(connection_id);
    let char_data = get_character_data(char_name);

    char_data.race = get_random_race();
    char_data.short_description = get_random_short_desc();
    char_data.class_name = get_random_class();
    save_character_data(char_data);

    // Also randomize traits
    let trait_count = randomize_traits(connection_id);

    // Reload to get updated trait info
    char_data = get_character_data(char_name);

    send_client_message(connection_id, "\nCharacter randomized! (" + trait_count + " traits selected)");
    show_wizard_menu(connection_id, char_data);
}

// Apply stat bonuses from a class to a character data object
fn apply_stat_bonus(char_data, bonuses) {
    let keys = bonuses.keys();
    for stat in keys {
        let bonus = bonuses[stat];
        if stat == "str" { char_data.stat_str += bonus; }
        if stat == "dex" { char_data.stat_dex += bonus; }
        if stat == "con" { char_data.stat_con += bonus; }
        if stat == "int" { char_data.stat_int += bonus; }
        if stat == "wis" { char_data.stat_wis += bonus; }
        if stat == "cha" { char_data.stat_cha += bonus; }
    }
}
