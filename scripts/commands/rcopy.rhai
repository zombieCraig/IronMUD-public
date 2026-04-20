// scripts/commands/rcopy.rhai
// Copy room attributes from a source room to the current room
// Usage: rcopy <source_vnum> [category]
// Categories: all, flags, desc, seasonal, triggers, water, doors, extra

fn copy_room_flags(source, target_id) {
    let flags = source.flags;
    set_room_flag(target_id, "dark", flags.dark);
    set_room_flag(target_id, "no_mob", flags.no_mob);
    set_room_flag(target_id, "indoors", flags.indoors);
    set_room_flag(target_id, "underwater", flags.underwater);
    set_room_flag(target_id, "city", flags.city);
    set_room_flag(target_id, "no_windows", flags.no_windows);
    set_room_flag(target_id, "climate_controlled", flags.climate_controlled);
    set_room_flag(target_id, "always_hot", flags.always_hot);
    set_room_flag(target_id, "always_cold", flags.always_cold);
    set_room_flag(target_id, "difficult_terrain", flags.difficult_terrain);
    set_room_flag(target_id, "dirt_floor", flags.dirt_floor);
    set_room_flag(target_id, "shallow_water", flags.shallow_water);
    set_room_flag(target_id, "deep_water", flags.deep_water);

    // Copy combat zone
    let source_zone = get_room_combat_zone_raw(source.id);
    if source_zone != "" {
        set_room_combat_zone(target_id, source_zone);
    } else {
        clear_room_combat_zone(target_id);
    }
}

fn copy_room_desc(source, target_id) {
    set_room_description(target_id, source.description);
}

fn copy_room_seasonal(source, target_id) {
    set_room_spring_desc(target_id, source.spring_desc);
    set_room_summer_desc(target_id, source.summer_desc);
    set_room_autumn_desc(target_id, source.autumn_desc);
    set_room_winter_desc(target_id, source.winter_desc);
}

fn copy_room_triggers(source_id, target_id) {
    let triggers = get_room_triggers(source_id);
    let count = 0;
    for trigger in triggers {
        let ttype = get_trigger_type(trigger);
        // Add trigger with args if present
        if trigger.args.len() > 0 {
            add_room_trigger_with_args(target_id, ttype, trigger.script_name, trigger.args);
        } else {
            add_room_trigger(target_id, ttype, trigger.script_name);
        }
        // Get the index of the newly added trigger
        let target_triggers = get_room_triggers(target_id);
        let idx = target_triggers.len() - 1;
        // Copy interval and chance settings
        if trigger.interval_secs != 60 {
            set_trigger_interval(target_id, idx, trigger.interval_secs);
        }
        if trigger.chance < 100 {
            set_trigger_chance(target_id, idx, trigger.chance);
        }
        if !trigger.enabled {
            set_trigger_enabled(target_id, idx, false);
        }
        count += 1;
    }
    return count;
}

fn copy_room_water(source_id, target_id) {
    // Copy water type
    let water_type = get_room_water_type(source_id);
    set_room_water_type(target_id, water_type);

    // Clear existing catch table
    let target_catches = get_catch_table(target_id);
    for entry in target_catches {
        remove_catch_entry(target_id, entry.vnum);
    }

    // Add source catch entries
    let source_catches = get_catch_table(source_id);
    for entry in source_catches {
        add_catch_entry(target_id, entry.vnum, entry.weight, entry.min_skill, entry.rarity);
    }
    return source_catches.len();
}

fn copy_room_catch(source_id, target_id) {
    // Clear existing catch table
    let target_catches = get_catch_table(target_id);
    for entry in target_catches {
        remove_catch_entry(target_id, entry.vnum);
    }

    // Add source catch entries
    let source_catches = get_catch_table(source_id);
    for entry in source_catches {
        add_catch_entry(target_id, entry.vnum, entry.weight, entry.min_skill, entry.rarity);
    }
    return source_catches.len();
}

fn copy_single_door(source_id, target_id, direction) {
    let door = get_door(source_id, direction);
    if door == () { return false; }

    // Only copy if target has an exit in this direction
    let target_exit = get_exit_target(target_id, direction);
    if target_exit == "" { return false; }

    // Remove existing door if any
    if has_door(target_id, direction) {
        remove_door(target_id, direction);
    }

    // Add new door with same properties
    add_door(target_id, direction, door.name);

    if door.description != "" {
        set_door_description(target_id, direction, door.description);
    }
    if door.keywords.len() > 0 {
        set_door_keywords(target_id, direction, door.keywords);
    }
    if door.key_id != "" {
        set_door_key(target_id, direction, door.key_id);
    }

    set_door_closed(target_id, direction, door.is_closed);
    set_door_locked(target_id, direction, door.is_locked);
    return true;
}

fn copy_room_doors(source_id, target_id) {
    let directions = ["north", "south", "east", "west", "up", "down"];
    let count = 0;
    for direction in directions {
        if copy_single_door(source_id, target_id, direction) {
            count += 1;
        }
    }
    return count;
}

fn copy_room_extras(source, target_id) {
    let extras = source.extra_descs;
    let count = 0;
    for extra in extras {
        // Join keywords with spaces for the add function
        let kw_str = "";
        for kw in extra.keywords {
            if kw_str != "" { kw_str += " "; }
            kw_str += kw;
        }
        add_room_extra_desc(target_id, kw_str, extra.description);
        count += 1;
    }
    return count;
}

fn count_set_flags(flags) {
    let count = 0;
    if flags.dark { count += 1; }
    if flags.no_mob { count += 1; }
    if flags.indoors { count += 1; }
    if flags.underwater { count += 1; }
    if flags.city { count += 1; }
    if flags.no_windows { count += 1; }
    if flags.climate_controlled { count += 1; }
    if flags.always_hot { count += 1; }
    if flags.always_cold { count += 1; }
    if flags.difficult_terrain { count += 1; }
    if flags.dirt_floor { count += 1; }
    if flags.shallow_water { count += 1; }
    if flags.deep_water { count += 1; }
    return count;
}

fn count_seasonal(source) {
    let count = 0;
    if source.spring_desc != "" { count += 1; }
    if source.summer_desc != "" { count += 1; }
    if source.autumn_desc != "" { count += 1; }
    if source.winter_desc != "" { count += 1; }
    return count;
}

fn show_copy_preview(source, connection_id) {
    let output = "=== Copy Preview from: " + source.title + " ===\n\n";

    // Flags
    let flag_count = count_set_flags(source.flags);
    output += "Flags: " + flag_count + " set\n";

    // Description
    let desc_len = source.description.len();
    output += "Description: " + desc_len + " characters\n";

    // Seasonal
    let seasonal_count = count_seasonal(source);
    output += "Seasonal descriptions: " + seasonal_count + " set\n";

    // Triggers
    let triggers = get_room_triggers(source.id);
    output += "Triggers: " + triggers.len() + "\n";

    // Water/fishing
    let water_type = get_room_water_type(source.id);
    let catch_table = get_catch_table(source.id);
    output += "Water type: " + water_type + "\n";
    output += "Catch table: " + catch_table.len() + " catchable items\n";

    // Doors
    let door_count = 0;
    let directions = ["north", "south", "east", "west", "up", "down"];
    for direction in directions {
        let door = get_door(source.id, direction);
        if door != () { door_count += 1; }
    }
    output += "Doors: " + door_count + "\n";

    // Extra descriptions
    output += "Extra descriptions: " + source.extra_descs.len() + "\n";

    output += "\nUsage: rcopy " + source.vnum + " <category>\n";
    output += "Categories: all, flags, desc, seasonal, triggers, water, catch, doors, extra";

    send_client_message(connection_id, output);
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to copy room attributes.");
        return;
    }

    // Get current room
    let current_room = get_room_data(char.current_room_id);
    if current_room == () {
        send_client_message(connection_id, "Error: You're not in a valid room.");
        return;
    }

    // Check area permissions for current room
    let room_area_id = current_room.area_id;
    if room_area_id != "" {
        if !can_edit_area(room_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to edit rooms in this area.");
            return;
        }
    }

    // Parse arguments
    let parts = args.split(" ");
    if parts.len() < 1 || parts[0] == "" {
        send_client_message(connection_id, "Usage: rcopy <source_vnum> [category]\nCategories: all, flags, desc, seasonal, triggers, water, catch, doors, extra");
        return;
    }

    let source_arg = parts[0];
    let category = "preview";
    if parts.len() >= 2 && parts[1] != "" {
        category = parts[1].to_lower();
    }

    // Resolve source room (vnum or UUID)
    let source_room_id = resolve_room_id(source_arg);
    if source_room_id == "" {
        send_client_message(connection_id, "Room not found: " + source_arg);
        return;
    }

    let source_room = get_room_data(source_room_id);
    if source_room == () {
        send_client_message(connection_id, "Room not found: " + source_arg);
        return;
    }

    // Prevent copying to self
    if source_room.id == current_room.id {
        send_client_message(connection_id, "Cannot copy a room to itself.");
        return;
    }

    // Handle categories
    let valid_categories = ["all", "flags", "desc", "seasonal", "triggers", "water", "catch", "doors", "extra", "preview"];
    let is_valid = false;
    for vc in valid_categories {
        if vc == category { is_valid = true; }
    }
    if !is_valid {
        send_client_message(connection_id, "Invalid category: " + category + "\nValid categories: all, flags, desc, seasonal, triggers, water, catch, doors, extra");
        return;
    }

    // Preview mode (no category specified)
    if category == "preview" {
        show_copy_preview(source_room, connection_id);
        return;
    }

    // Execute copy based on category
    let output = "Copying from " + source_room.title + ":\n";

    if category == "all" || category == "flags" {
        copy_room_flags(source_room, current_room.id);
        let flag_count = count_set_flags(source_room.flags);
        output += "- Flags: copied " + flag_count + " flag settings\n";
    }

    if category == "all" || category == "desc" {
        copy_room_desc(source_room, current_room.id);
        output += "- Description: copied\n";
    }

    if category == "all" || category == "seasonal" {
        copy_room_seasonal(source_room, current_room.id);
        let seasonal_count = count_seasonal(source_room);
        output += "- Seasonal descriptions: copied " + seasonal_count + "\n";
    }

    if category == "all" || category == "triggers" {
        let trigger_count = copy_room_triggers(source_room.id, current_room.id);
        output += "- Triggers: copied " + trigger_count + "\n";
    }

    if category == "all" || category == "water" {
        let catch_count = copy_room_water(source_room.id, current_room.id);
        let water_type = get_room_water_type(source_room.id);
        output += "- Water: " + water_type + " with " + catch_count + " catch entries\n";
    }

    if category == "catch" {
        let catch_count = copy_room_catch(source_room.id, current_room.id);
        output += "- Catch table: copied " + catch_count + " entries\n";
    }

    if category == "all" || category == "doors" {
        let door_count = copy_room_doors(source_room.id, current_room.id);
        output += "- Doors: copied " + door_count + " (where exits exist)\n";
    }

    if category == "all" || category == "extra" {
        let extra_count = copy_room_extras(source_room, current_room.id);
        output += "- Extra descriptions: copied " + extra_count + "\n";
    }

    output += "\nDone.";
    send_client_message(connection_id, output);
}
