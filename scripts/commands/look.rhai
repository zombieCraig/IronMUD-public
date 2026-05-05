// scripts/commands/look.rhai

fn normalize_direction(target) {
    if target == "n" { return "north"; }
    if target == "s" { return "south"; }
    if target == "e" { return "east"; }
    if target == "w" { return "west"; }
    if target == "u" { return "up"; }
    if target == "d" { return "down"; }
    return target;
}

fn get_exit_for_direction(exits, direction) {
    if direction == "north" { return exits.north; }
    if direction == "east" { return exits.east; }
    if direction == "south" { return exits.south; }
    if direction == "west" { return exits.west; }
    if direction == "up" { return exits.up; }
    if direction == "down" { return exits.down; }
    if direction == "out" { return exits.out; }
    // Check custom exits (e.g., "elevator", "train")
    let dir_lower = direction.to_lower();
    if exits.custom.contains(dir_lower) {
        return exits.custom[dir_lower];
    }
    return "";
}

fn find_door_by_keyword(room_id, keyword) {
    let directions = ["north", "south", "east", "west", "up", "down"];
    for direction in directions {
        let door = get_door(room_id, direction);
        if door == () { continue; }
        if door.name.to_lower() == keyword { return direction; }
        for kw in door.keywords {
            if kw.to_lower() == keyword { return direction; }
        }
    }
    return "";
}

// Helper to check if vision is blocked (returns true if blocked)
fn check_vision_blocked(should_check, room_id, connection_id, char_name) {
    if !should_check { return false; }
    if !can_see(connection_id) {
        send_client_message(connection_id, "You cannot see.");
        return true;
    }
    if is_room_dark(room_id, connection_id) {
        // Build mode bypasses darkness in editable areas
        if is_in_build_mode(char_name, room_id) {
            return false;
        }
        send_client_message(connection_id, "It is too dark to see.");
        return true;
    }
    return false;
}

fn look_at_door(door, direction, term_width, connection_id) {
    let desc = door.description;
    if desc == "" {
        desc = "You see a " + door.name + " to the " + direction + ".";
    }
    if door.is_locked {
        desc += " It is closed and locked.";
    } else if door.is_closed {
        desc += " It is closed.";
    } else {
        desc += " It is open.";
    }
    send_client_message(connection_id, wrap_text(desc, term_width));
}

fn look_in_direction(room, direction, term_width, connection_id) {
    let target_room_id = get_exit_for_direction(room.exits, direction);
    if target_room_id == "" { return false; }

    let target_room = get_room_data(target_room_id);
    if target_room == () { return false; }

    let door = get_door(room.id, direction);
    let msg = "";
    if door != () {
        if door.is_closed {
            msg = "Through the closed " + door.name + " to the " + direction + ", you can't see much.";
            send_client_message(connection_id, msg);
            return true;
        }
        msg = "Through the open " + door.name + " to the " + direction + ": ";
    } else {
        msg = "To the " + direction + ": ";
    }
    msg += target_room.title;
    send_client_message(connection_id, msg);
    return true;
}

fn run_command(args, connection_id) {
    // Get terminal width for text wrapping (used for extra descriptions)
    let term_width = get_terminal_width(connection_id);

    // Get the current character
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to look around.");
        return;
    }

    // Check if sleeping - can't look while asleep
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep and cannot see anything. Use 'status' to check your condition or 'wake' to wake up.");
        return;
    }

    // Get the character's current room
    let room_id = char.current_room_id;
    let room = get_room_data(room_id);

    if room == () {
        send_client_message(connection_id, "You are in a void. Something is wrong.");
        return;
    }

    // Check if looking at something specific
    if args != "" {
        let target = args.to_lower();

        // Check for "look in <container>" syntax
        if target.starts_with("in ") {
            let container_keyword = target.sub_string(3);
            // Allow looking in inventory containers even if dark/blind
            let inv_container = find_item_in_inventory(char.name, container_keyword);
            let blocked = check_vision_blocked(inv_container == (), room_id, connection_id, char.name);
            if blocked { return; }
            look_in_container(char, container_keyword, connection_id);
            return;
        }

        // Check if target is in inventory (can always examine inventory items)
        let inv_item = find_item_in_inventory(char.name, target);
        let in_inventory = inv_item != ();

        // Check darkness/blindness for non-inventory targets
        let blocked = check_vision_blocked(!in_inventory, room_id, connection_id, char.name);
        if blocked { return; }

        // First, check for extra descriptions
        let extra_desc = get_room_extra_desc(room_id, target);
        if extra_desc != "" {
            send_client_message(connection_id, wrap_text(extra_desc, term_width));
            return;
        }

        // Then, check for item-level extra descriptions (sub-keyword lore on
        // items in inventory, equipment, or this room).
        let item_extra_sources = [];
        for it in get_items_in_inventory(char.name) { item_extra_sources.push(it); }
        for it in get_equipped_items(char.name) { item_extra_sources.push(it); }
        for it in get_items_in_room(room_id) { item_extra_sources.push(it); }
        for it in item_extra_sources {
            let it_extra = get_item_extra_desc(it.id, target);
            if it_extra != "" {
                send_client_message(connection_id, wrap_text(it_extra, term_width));
                return;
            }
        }

        // Check for mobiles in the room (filter invisible ones the looker can't see)
        let mobiles = filter_visible_mobiles(get_mobiles_in_room(room_id), char.name);
        let mobile = find_mobile_by_keyword(target, mobiles);
        if mobile != () {
            show_mobile_description(mobile, term_width, connection_id);
            return;
        }

        // Check for other players in the room
        let characters_in_room = get_characters_in_room(room_id);
        let viewer_sees_invis = has_buff(char.name, "detect_invisible") || char.is_admin;
        for char_name in characters_in_room {
            let name_lower = char_name.to_lower();
            if name_lower == char.name.to_lower() { continue; }
            if name_lower != target && !name_lower.starts_with(target) { continue; }
            // Skip invisible players unless viewer can see them
            if !viewer_sees_invis && has_buff(char_name, "invisibility") { continue; }
            let other_char = get_character_data(char_name);
            if other_char == () { continue; }
            show_player_description(other_char, char_name, term_width, connection_id);
            return;
        }

        // Check for items in inventory or room
        let item = find_item_in_inventory(char.name, target);
        if item == () {
            item = find_item_in_room(room_id, target);
        }
        if item != () {
            // Handle corpses specially - show contents
            if item.flags.is_corpse {
                look_at_corpse(item, connection_id);
                return;
            }
            let desc = item.long_desc;
            if desc == "" {
                desc = "You see nothing special about " + item.name + ".";
            }
            send_client_message(connection_id, wrap_text(desc, term_width));
            return;
        }

        // Check for doors by name or keyword
        let found_door_dir = find_door_by_keyword(room_id, target);
        if found_door_dir != "" {
            let door = get_door(room_id, found_door_dir);
            look_at_door(door, found_door_dir, term_width, connection_id);
            return;
        }

        // Check if it's a direction
        let direction = normalize_direction(target);
        if look_in_direction(room, direction, term_width, connection_id) {
            return;
        }

        // Not a direction or extra description
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    // Display the room
    display_room(room_id, connection_id, char.name);

    // Surface buried-treasure hint if the player is carrying a detector
    let detector = find_detector(char.name);
    if detector != () {
        let buried = get_buried_items_in_room(room_id);
        if buried.len() > 0 {
            send_client_message(connection_id, "Your " + detector.short_desc + " hums - something is buried here.");
        }
    }

    // Check for vision penalty and warn player
    let vision_penalty = get_vision_penalty(room_id, connection_id);
    if vision_penalty > 0 && vision_penalty < 100 {
        send_client_message(connection_id, "\x1b[33m[Vision impaired: -" + vision_penalty + "% to actions]\x1b[0m");
    }

    // Check if this is a transport interior and show floor/stop info
    display_transport_info(room_id, connection_id);

    // Fire on_look trigger (doesn't cancel look, just adds effects)
    fire_room_trigger(room_id, "on_look", connection_id, #{});
    return;
}

// Display transport floor/stop information if in a transport vehicle interior
fn display_transport_info(room_id, connection_id) {
    let transport = get_transport_by_interior_room(room_id);
    if transport == () { return; }

    let output = "";
    let stops = transport.stops;
    let current_index = transport.current_stop_index;

    // Different display for elevator vs scheduled transport
    if is_transport_ondemand(transport.id) {
        // Elevator - show all floors with press numbers (MXP clickable)
        output += "\nFloors:";
        for i in 0..stops.len() {
            let stop = stops[i];
            let marker = "";
            if i == current_index && transport.state == "stopped" {
                marker = " (current)";
            }
            let floor_num = i + 1;
            let floor_link = mxp_send("press " + floor_num, "[" + floor_num + "]");
            let floor_display = mxp_or(floor_link, "[" + floor_num + "]", connection_id);
            output += "\n  " + floor_display + " " + stop.name + marker;
        }
    } else {
        // Scheduled transport - show next stops
        if transport.state == "moving" {
            output += "\nNext stop: " + stops[current_index].name;
        } else {
            output += "\nCurrently at: " + stops[current_index].name;
        }
        // Show a few upcoming stops
        let upcoming = "";
        let count = 0;
        let idx = current_index;
        let direction = transport.direction;
        while count < 3 {
            // Calculate next index based on ping-pong direction
            let next_idx = idx + direction;
            if next_idx < 0 || next_idx >= stops.len() {
                // Would reverse direction
                break;
            }
            upcoming += "\n  - " + stops[next_idx].name;
            idx = next_idx;
            count += 1;
        }
        if upcoming != "" {
            output += "\nUpcoming stops:" + upcoming;
        }
    }

    if output != "" {
        send_client_message(connection_id, output);
    }
}

fn look_in_container(char, keyword, connection_id) {
    // Find container in inventory or room
    let container = find_item_in_inventory(char.name, keyword);
    if container == () {
        container = find_item_in_room(char.current_room_id, keyword);
    }

    if container == () {
        send_client_message(connection_id, "You don't see that here.");
        return;
    }

    if !container.is_container {
        send_client_message(connection_id, "That's not a container.");
        return;
    }

    if container.container_closed {
        send_client_message(connection_id, "It's closed.");
        return;
    }

    let contents = get_items_in_container(container.id);
    if contents.len() == 0 {
        send_client_message(connection_id, container.name + " is empty.");
        return;
    }

    let output = container.name + " contains:";
    for item in contents {
        output += "\n  " + item.name;
    }
    send_client_message(connection_id, output);
}

// Get health status text based on HP percentage
fn get_health_status(current_hp, max_hp, name) {
    if max_hp <= 0 { return ""; }
    let percent = (current_hp * 100) / max_hp;

    if percent >= 100 { return name + " is in perfect health."; }
    if percent >= 75 { return name + " has a few scratches."; }
    if percent >= 50 { return name + " has some wounds."; }
    if percent >= 25 { return name + " is badly wounded."; }
    if percent > 0 { return name + " is barely holding on."; }
    return name + " appears to be dead.";
}

// Format visible wounds for display
fn format_visible_wounds(mobile_id, name) {
    let wounds = get_mobile_wounds(mobile_id);
    if wounds.len() == 0 { return ""; }

    let texts = [];
    for wound in wounds {
        // Only show moderate+ wounds visibly
        if wound.level == "bruise" || wound.level == "light" { continue; }
        let text = "a " + wound.level + " " + wound.wound_type + " on the " + wound.body_part;
        if wound.bleeding_severity > 0 { text += " (bleeding)"; }
        texts.push(text);
    }
    if texts.len() == 0 { return ""; }

    let result = name + " has " + texts[0];
    for i in 1..texts.len() {
        if i == texts.len() - 1 { result += " and "; }
        else { result += ", "; }
        result += texts[i];
    }
    return result + ".";
}

// Format visible scars for display
fn format_visible_scars(scars_map, name) {
    let texts = [];
    let keys = scars_map.keys();
    if keys.len() == 0 { return ""; }
    for key in keys {
        let count = scars_map[key];
        if count <= 0 { continue; }
        let desc = get_scar_description(count);
        texts.push(desc + " on their " + key);
    }
    if texts.len() == 0 { return ""; }

    let result = name + " has " + texts[0];
    for i in 1..texts.len() {
        if i == texts.len() - 1 { result += " and "; }
        else { result += ", "; }
        result += texts[i];
    }
    return result + ".";
}

fn get_scar_description(count) {
    if count == 1 { return "a scar"; }
    if count == 2 { return "a couple of scars"; }
    if count == 3 { return "a few scars"; }
    if count <= 5 { return "several scars"; }
    return "many scars";
}

// Show a mobile's description with their visible equipment
fn show_mobile_description(mobile, term_width, connection_id) {
    let output = "";

    // Long description
    let desc = mobile.long_desc;
    if desc == "" {
        desc = "You see nothing special about " + mobile.name + ".";
    }
    output += wrap_text(desc, term_width);

    // Show health status
    let health = get_health_status(mobile.hp, mobile.max_hp, mobile.name);
    if health != "" {
        output += "\n\n" + health;
    }

    // Show visible wounds
    let wounds = format_visible_wounds(mobile.id, mobile.name);
    if wounds != "" {
        output += "\n" + wounds;
    }

    // Show scars
    let scars_map = get_mobile_scars(mobile.id);
    let scars_text = format_visible_scars(scars_map, mobile.name);
    if scars_text != "" {
        output += "\n" + scars_text;
    }

    // Show visible equipped items
    let equip_text = format_mobile_equipment(mobile);
    if equip_text != "" {
        output += "\n\n" + equip_text;
    }

    send_client_message(connection_id, output);
}

// Format the equipment text for a mobile
fn format_mobile_equipment(mobile) {
    let all_items = get_items_equipped_on_mobile(mobile.id);
    let visible_items = filter_visible_items(all_items);

    // Filter out death_only items (hidden until mobile dies)
    let final_items = [];
    for item in visible_items {
        if !get_item_flag(item.id, "death_only") {
            final_items.push(item);
        }
    }

    if final_items.len() == 0 {
        return "";
    }

    let item_descs = [];
    for item in final_items {
        let desc = format_visible_item(item);
        item_descs.push(desc);
    }

    return mobile.name + " is " + join_item_list(item_descs) + ".";
}

// Filter items to only visible slots (shared by player and mobile equipment)
fn filter_visible_items(all_items) {
    let visible = [];
    // Updated for L/R body part variants
    let visible_slots = [
        "wielded", "off-hand", "head", "neck", "torso", "back", "shoulders",
        "left arm", "right arm", "left hand", "right hand",
        "waist", "left leg", "right leg", "left foot", "right foot",
        // Legacy slots for backward compatibility
        "arms", "hands", "legs", "feet"
    ];

    for item in all_items {
        if is_item_visible(item, visible_slots) {
            visible.push(item);
        }
    }
    return visible;
}

fn is_item_visible(item, visible_slots) {
    for loc in item.wear_locations {
        let l = loc.to_lower();
        for vs in visible_slots {
            if l == vs { return true; }
        }
    }
    return false;
}

// Show a player's description with their visible equipment
fn show_player_description(other_char, char_name, term_width, connection_id) {
    let output = "";

    // God mode divine glow description
    if other_char.god_mode {
        output += other_char.name + " is surrounded by a divine glow.\n\n";
    }

    // Short description
    let desc = other_char.short_description;
    if desc == "" {
        desc = "You see nothing special about " + other_char.name + ".";
    }
    output += wrap_text(desc, term_width);

    // Show scars
    let scars_map = get_character_scars(other_char.name);
    let scars_text = format_visible_scars(scars_map, other_char.name);
    if scars_text != "" {
        output += "\n" + scars_text;
    }

    // Show visible equipped items
    let equip_text = format_player_equipment(char_name, other_char.name);
    if equip_text != "" {
        output += "\n\n" + equip_text;
    }

    send_client_message(connection_id, output);
}

// Format the equipment text for a player (e.g., "Thorin is wielding a sword.")
fn format_player_equipment(char_name, display_name) {
    let visible_items = get_visible_equipment(char_name);
    if visible_items.len() == 0 {
        return "";
    }

    let item_descs = [];
    for item in visible_items {
        let desc = format_visible_item(item);
        item_descs.push(desc);
    }

    return display_name + " is " + join_item_list(item_descs) + ".";
}

// Get only visibly equipped items (weapons in hand, visible armor)
fn get_visible_equipment(char_name) {
    let all_items = get_equipped_items(char_name);
    let visible = [];

    // Visible slots (updated for L/R body part variants)
    let visible_slots = [
        "wielded", "off-hand", "head", "neck", "torso", "back", "shoulders",
        "left arm", "right arm", "left hand", "right hand",
        "waist", "left leg", "right leg", "left foot", "right foot",
        // Legacy slots for backward compatibility
        "arms", "hands", "legs", "feet"
    ];

    for item in all_items {
        let is_visible = false;
        for loc in item.wear_locations {
            let l = loc.to_lower();
            for vs in visible_slots {
                if l == vs { is_visible = true; }
            }
        }
        if is_visible { visible.push(item); }
    }

    return visible;
}

// Format a visible item with its wear location context
fn format_visible_item(item) {
    let locs = item.wear_locations;
    let primary_loc = "";
    if locs.len() > 0 {
        primary_loc = locs[0].to_lower();
    }

    // Customize description based on where it's worn
    if primary_loc == "wielded" { return "wielding " + item.name; }
    if primary_loc == "off-hand" { return "holding " + item.name; }
    if primary_loc == "head" { return "wearing " + item.name + " on their head"; }
    if primary_loc == "torso" { return "wearing " + item.name; }
    if primary_loc == "back" { return "wearing " + item.name + " on their back"; }
    if primary_loc == "shoulders" { return "wearing " + item.name + " on their shoulders"; }
    if primary_loc == "legs" { return "wearing " + item.name + " on their legs"; }
    if primary_loc == "feet" { return "wearing " + item.name + " on their feet"; }
    if primary_loc == "hands" { return "wearing " + item.name + " on their hands"; }
    if primary_loc == "waist" { return "wearing " + item.name + " around their waist"; }
    if primary_loc == "arms" { return "wearing " + item.name + " on their arms"; }

    return "wearing " + item.name;
}

// Join list of items with proper grammar
// ["a", "b", "c"] -> "a, b, and c"
fn join_item_list(items) {
    if items.len() == 0 { return ""; }
    if items.len() == 1 { return items[0]; }
    if items.len() == 2 { return items[0] + " and " + items[1]; }

    let result = "";
    let i = 0;
    for item in items {
        if i == items.len() - 1 {
            result += "and " + item;
        } else {
            result += item + ", ";
        }
        i += 1;
    }
    return result;
}

// Display a corpse's description and contents
fn look_at_corpse(corpse, connection_id) {
    let output = corpse.long_desc;
    if output == "" {
        output = "The lifeless body lies here.";
    }

    // Show gold if any
    let gold = get_corpse_gold(corpse.id);
    if gold > 0 {
        output += "\n\nYou see " + gold + " gold coins.";
    }

    // Show contents
    let contents = get_items_in_container(corpse.id);
    if contents.len() > 0 {
        output += "\n\n" + corpse.name + " contains:";
        for item in contents {
            output += "\n  " + item.name;
        }
    } else if gold == 0 {
        output += "\n\nThe corpse has nothing of value.";
    }

    send_client_message(connection_id, output);
}
