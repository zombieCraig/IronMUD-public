// scripts/commands/read.rhai
// read <item> - Read an item, possibly learning a recipe from it

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Read what?");
        return;
    }

    // Check inventory first
    let inv_items = get_items_in_inventory(char.name);
    let item = find_item_by_keyword(args, inv_items);

    // Then equipped items
    if item == () {
        let eq_items = get_equipped_items(char.name);
        item = find_item_by_keyword(args, eq_items);
    }

    // Then room
    if item == () {
        let room_items = get_items_in_room(char.current_room_id);
        item = find_item_by_keyword(args, room_items);
    }

    if item == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    // Check if item is a transport status sign
    let transport_id = get_item_transport_link(item.id);
    if transport_id != "" {
        show_transport_sign(item, transport_id, connection_id);
        return;
    }

    // Readable note body takes precedence over teaches_*: if a builder authored
    // long-form content on this item, show that and stop — recipe/spell scrolls
    // should not carry a note body.
    let note_body = get_item_note_content(item.id);
    if note_body != "" {
        send_client_message(connection_id, "You read " + item.short_desc + ":");
        send_client_message(connection_id, note_body);
        return;
    }

    // Check if item teaches a recipe
    let recipe_id = get_item_teaches_recipe(item.id);

    // Check if item teaches a spell
    let spell_id = get_item_teaches_spell(item.id);
    if spell_id != () {
        learn_spell_from_scroll(char, item, spell_id, connection_id);
        return;
    }

    if recipe_id == () {
        // Just a regular readable item - show the description
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, item.long_desc);
        return;
    }

    // Item teaches a recipe
    let recipe = get_recipe(recipe_id);
    if recipe == () {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "The contents are incomprehensible.");
        return;
    }

    // Check if character already knows this recipe
    if knows_recipe(char.name, recipe_id) {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "You already know the recipe for " + recipe.name + ".");
        return;
    }

    // Check skill requirement (if not auto-learn, they need the skill level)
    let skill_level = get_skill_level(char.name, recipe.skill);
    if skill_level < recipe.skill_required {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "The techniques described are too advanced for you.");
        send_client_message(connection_id, "You need " + recipe.skill + " level " + recipe.skill_required + " to learn this recipe. (You have " + skill_level + ")");
        return;
    }

    // Learn the recipe
    let learned = learn_recipe(char.name, recipe_id);
    if !learned {
        send_client_message(connection_id, "Something went wrong learning the recipe.");
        return;
    }

    send_client_message(connection_id, "You carefully study " + item.short_desc + "...");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "*** You have learned: " + recipe.name + "! ***");
    send_client_message(connection_id, "");

    // Show recipe details
    let skill_display = recipe.skill;
    if skill_display.len() > 0 {
        skill_display = skill_display.sub_string(0, 1).to_upper() + skill_display.sub_string(1);
    }
    send_client_message(connection_id, "Skill: " + skill_display + " (requires level " + recipe.skill_required + ")");

    // Check if item should be consumed (scrolls vs books)
    // If item type is "scroll" or has "consumable" flag, consume it
    let should_consume = false;
    if item.item_type == "Scroll" { should_consume = true; }
    if get_item_flag(item.id, "consumable") { should_consume = true; }

    if should_consume {
        delete_item(item.id);
        send_client_message(connection_id, "The " + item.name + " crumbles to dust.");
    }
}

// Display transport status sign information
fn show_transport_sign(item, transport_id, connection_id) {
    let transport = get_transport_data(transport_id);
    if transport == () {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "The display is blank.");
        return;
    }

    send_client_message(connection_id, "You read " + item.short_desc + ":");

    let output = "";
    let stops = transport.stops;
    let current_index = transport.current_stop_index;

    if transport.state == "stopped" {
        let current_stop = stops[current_index];
        output += "  " + transport.name + " - Currently at: " + current_stop.name;
    } else {
        let next_stop = stops[current_index];
        output += "  " + transport.name + " - En route to: " + next_stop.name;
    }

    // For scheduled transports, show operating hours
    let schedule_type = get_transport_schedule_type(transport_id);
    if schedule_type == "gametime" {
        let schedule = get_transport_schedule_info(transport_id);
        output += "\n  Operating hours: " + format_hour(schedule.operating_start);
        output += " - " + format_hour(schedule.operating_end);

        // Check if currently operating
        let game_time = get_game_time();
        let within = is_transport_within_operating_hours(transport_id, game_time.hour);
        if !within {
            output += " (Currently CLOSED)";
        }
    }

    send_client_message(connection_id, output);
}

fn learn_spell_from_scroll(char, item, spell_id, connection_id) {
    // Check if already learned
    if has_learned_spell(char.name, spell_id) {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "You have already mastered this spell.");
        return;
    }

    // Check magic skill requirement
    let info = get_spell_info(spell_id);
    if info.len() == 0 {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "The arcane script is incomprehensible.");
        return;
    }

    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < info["skill_required"] {
        send_client_message(connection_id, "You read " + item.short_desc + ".");
        send_client_message(connection_id, "The arcane formulae are beyond your current understanding.");
        send_client_message(connection_id, "You need magic skill level " + info["skill_required"] + " to learn this spell. (You have " + magic_level + ")");
        return;
    }

    // Learn the spell
    let learned = learn_spell(char.name, spell_id);
    if !learned {
        send_client_message(connection_id, "Something went wrong learning the spell.");
        return;
    }

    send_client_message(connection_id, "You carefully study the arcane scroll...");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "\x1b[1;35m*** You have learned: " + info["name"] + "! ***\x1b[0m");
    send_client_message(connection_id, info["description"]);
    send_client_message(connection_id, "");

    // Consume the scroll
    delete_item(item.id);
    send_client_message(connection_id, "The scroll crumbles to dust as the knowledge flows into your mind.");
}

fn format_hour(hour) {
    if hour == 0 { return "12:00 AM"; }
    if hour < 12 { return hour + ":00 AM"; }
    if hour == 12 { return "12:00 PM"; }
    return (hour - 12) + ":00 PM";
}
