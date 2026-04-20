// scripts/commands/recall.rhai
// recall - Teleport back to your spawn point (inn, rental property, or town center)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check admin toggle
    let recall_enabled = get_setting_or_default("recall_enabled", "true");
    if recall_enabled == "false" {
        send_client_message(connection_id, "The recall command has been disabled by the administrators.");
        return;
    }

    // Block if sleeping or unconscious
    let pos = char.position;
    if pos == "sleeping" {
        send_client_message(connection_id, "You can't recall while sleeping. Wake up first.");
        return;
    }
    if char.is_unconscious {
        send_client_message(connection_id, "You are unconscious and cannot recall.");
        return;
    }

    // Block if in combat
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't recall while in combat! Try 'flee' or 'retreat' first.");
        return;
    }

    // Block if overloaded
    let enc_level = get_encumbrance_level(char.name);
    if enc_level >= 4 {
        send_client_message(connection_id, "You are too overloaded to recall. Drop some items first.");
        return;
    }

    // Check stamina requirement (configurable via settings)
    let stamina_cost_str = get_setting_or_default("stamina_cost_recall", "50");
    let stamina_cost = parse_int(stamina_cost_str);
    if stamina_cost == () { stamina_cost = 50; }
    if char.stamina < stamina_cost {
        send_client_message(connection_id, "You don't have enough stamina to recall. (Need " + stamina_cost + ", have " + char.stamina + ")");
        return;
    }

    // Determine spawn room (priority: rental property > manual spawn > default)
    let spawn_room_id = get_spawn_point(char, connection_id);
    if spawn_room_id == "" {
        send_client_message(connection_id, "Error: Could not determine spawn point.");
        return;
    }

    // Verify spawn room exists
    let spawn_room = get_room_data(spawn_room_id);
    if spawn_room == () {
        // Fallback to default spawn if saved room is invalid
        spawn_room_id = get_character_spawn_room(char.name);
        spawn_room = get_room_data(spawn_room_id);
        if spawn_room == () {
            send_client_message(connection_id, "Error: Spawn room no longer exists. Contact an administrator.");
            return;
        }
    }

    // Check if already at spawn
    let current_room_id = char.current_room_id;
    if current_room_id == spawn_room_id {
        send_client_message(connection_id, "You are already at your spawn point.");
        return;
    }

    // Cancel fishing if active
    if is_fishing(connection_id) {
        cancel_fishing(connection_id);
        send_client_message(connection_id, "Your fishing line snaps as you prepare to recall!");
    }

    // Deduct stamina
    char.stamina -= stamina_cost;
    if char.stamina < 0 {
        char.stamina = 0;
    }

    // Broadcast departure to old room
    broadcast_to_room_awake(current_room_id, char.name + " shimmers briefly and vanishes.", char.name);

    // Move the character
    char.current_room_id = spawn_room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    // Clear AFK status on movement
    if is_afk(connection_id) {
        clear_afk(connection_id);
        send_client_message(connection_id, "You are no longer AFK.");
    }

    // Show recall message
    send_client_message(connection_id, "You close your eyes and focus on your spawn point...\nReality shifts around you.\n");

    // Broadcast arrival to new room
    broadcast_to_room_awake(spawn_room_id, char.name + " shimmers into existence.", char.name);

    // Display the room
    display_room(spawn_room_id, connection_id, char.name);

    // Fire on_greet triggers for all NPCs in the new room
    let mobiles = get_mobiles_in_room(spawn_room_id);
    for mobile in mobiles {
        fire_mobile_trigger(mobile.id, "on_greet", connection_id, #{
            "char_name": char.name,
            "direction": "recall",
            "source_room": current_room_id,
            "mobile_name": mobile.name
        });
    }
}

// Determine the best spawn point for the character
// Priority: rental property in current area > manual spawn room > default starting room
fn get_spawn_point(char, connection_id) {
    // First check for rental property in current area
    let current_room = get_room_data(char.current_room_id);
    if current_room != () {
        let area_id = current_room.area_id;
        if area_id != "" {
            let lease = get_player_lease_in_area(char.name, area_id);
            if lease != () && !lease.is_evicted {
                let entrance_id = get_property_entrance(lease.id);
                if entrance_id != "" {
                    return entrance_id;
                }
            }
        }
    }

    // Fall back to character's manual spawn room or default
    return get_character_spawn_room(char.name);
}
