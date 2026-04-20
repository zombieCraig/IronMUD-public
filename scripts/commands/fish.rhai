// scripts/commands/fish.rhai
// fish - Cast your line and try to catch something
// Requires: fishing rod held (via hold/wield command)
// Optional: bait in inventory for better chances

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check if already fishing
    if is_fishing(connection_id) {
        send_client_message(connection_id, "You already have a line in the water. Use 'reel' to pull it in.");
        return;
    }

    // Check if room has fishable water
    if !has_fishable_water(char.current_room_id) {
        send_client_message(connection_id, "There's no water here to fish in.");
        return;
    }

    // Check for equipped fishing rod (offhand slot)
    let equipped_items = get_equipped_items(char.name);
    let rod = ();
    for item in equipped_items {
        if item.is_fishing_rod {
            rod = item;
        }
    }

    if rod == () {
        send_client_message(connection_id, "You need to hold a fishing rod first. Try 'hold <rod>'.");
        return;
    }

    // Look for bait in inventory (optional)
    let inv_items = get_items_in_inventory(char.name);
    let bait = ();
    for item in inv_items {
        if item.is_bait {
            bait = item;
        }
    }

    // Calculate bite delay
    let base_delay = random_int(8, 12);
    let skill_level = get_skill_level(char.name, "fishing");
    let skill_modifier = -(skill_level * 3) / 10; // -0.3 per level, max -3

    let bait_modifier = 0;
    if bait != () {
        bait_modifier = -2;
    }

    // Water type modifier
    let water_type = get_room_water_type(char.current_room_id);
    let water_modifier = 0;
    if water_type == "saltwater" { water_modifier = 2; }
    if water_type == "magical" { water_modifier = -2; }

    let final_delay = base_delay + skill_modifier + bait_modifier + water_modifier;
    if final_delay < 3 { final_delay = 3; }
    if final_delay > 15 { final_delay = 15; }

    // Calculate bite time
    let now = get_current_time();
    let bite_time = now + final_delay;

    // Start fishing state
    let bait_id = "";
    if bait != () {
        bait_id = bait.id;
    }

    if start_fishing(connection_id, rod.id, bait_id, char.current_room_id, bite_time) {
        // Send cast message
        let cast_msg = "You cast your line into the water";
        if bait != () {
            cast_msg += ", baited with " + bait.name;
        }
        cast_msg += "...";
        send_client_message(connection_id, cast_msg);
        broadcast_to_room(char.current_room_id, char.name + " casts a fishing line into the water.", char.name);
    } else {
        send_client_message(connection_id, "Something went wrong. Please try again.");
    }
}
