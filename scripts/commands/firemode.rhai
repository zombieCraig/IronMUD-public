// scripts/commands/firemode.rhai
// Change fire mode on a firearm (single/burst/auto)

fn run_command(args, connection_id) {
    let check = validate_firemode(connection_id);
    if check.error != "" {
        send_client_message(connection_id, check.error);
        return;
    }
    execute_firemode(args, check.char_name, check.room_id, check.weapon_id, check.weapon_name, check.supported, connection_id);
}

fn validate_firemode(connection_id) {
    let result = #{ "error": "", "char_name": "", "room_id": "", "weapon_id": "", "weapon_name": "", "supported": [] };
    let char = get_player_character(connection_id);
    if char == () { result.error = "You must be logged in."; return result; }
    if char.position == "sleeping" { result.error = "You are asleep! Wake up first."; return result; }

    let weapon_info = find_wielded_firearm(char.name);
    if weapon_info.weapon_id == "" { result.error = "You need to wield a firearm to change fire modes."; return result; }

    let supported = get_item_supported_fire_modes(weapon_info.weapon_id);
    if supported.len() == 0 { result.error = "This weapon has no configurable fire modes."; return result; }

    result.char_name = char.name;
    result.room_id = char.current_room_id;
    result.weapon_id = weapon_info.weapon_id;
    result.weapon_name = weapon_info.name;
    result.supported = supported;
    return result;
}

fn execute_firemode(args, char_name, room_id, weapon_id, weapon_name, supported, connection_id) {
    let current_mode = get_item_fire_mode(weapon_id);

    if args == "" {
        let next_mode = cycle_fire_mode(current_mode, supported);
        set_item_fire_mode(weapon_id, next_mode);
        send_client_message(connection_id, "You switch " + weapon_name + " to " + next_mode + " fire mode.");
        broadcast_to_room_awake(room_id, char_name + " adjusts their " + weapon_name + ".", char_name);
        return;
    }

    handle_specific_mode(args, current_mode, weapon_id, weapon_name, char_name, room_id, supported, connection_id);
}

fn handle_specific_mode(args, current_mode, weapon_id, weapon_name, char_name, room_id, supported, connection_id) {
    let requested = args.to_lower();
    if !is_supported_mode(requested, supported) {
        let mode_list = join_modes(supported);
        send_client_message(connection_id, "Invalid fire mode. Supported modes: " + mode_list);
        return;
    }
    if requested == current_mode {
        send_client_message(connection_id, "Your " + weapon_name + " is already in " + current_mode + " mode.");
        return;
    }
    set_item_fire_mode(weapon_id, requested);
    send_client_message(connection_id, "You switch " + weapon_name + " to " + requested + " fire mode.");
    broadcast_to_room_awake(room_id, char_name + " adjusts their " + weapon_name + ".", char_name);
}

fn find_wielded_firearm(char_name) {
    let result = #{ "weapon_id": "", "name": "" };
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_wielded_firearm(item) {
            result.weapon_id = item.id;
            result.name = item.name;
            return result;
        }
    }
    return result;
}

fn is_wielded_firearm(item) {
    let locs = item.wear_locations;
    for loc in locs {
        if loc.to_lower() == "wielded" {
            let rtype = get_item_ranged_type(item.id);
            if rtype == "firearm" { return true; }
        }
    }
    return false;
}

// Cycle to next fire mode
fn cycle_fire_mode(current, supported) {
    let idx = -1;
    let i = 0;
    while i < supported.len() {
        if supported[i] == current {
            idx = i;
        }
        i += 1;
    }
    let next_idx = 0;
    if idx >= 0 {
        next_idx = (idx + 1) % supported.len();
    }
    return supported[next_idx];
}

// Check if mode is in supported list
fn is_supported_mode(mode, supported) {
    for s in supported {
        if s == mode { return true; }
    }
    return false;
}

// Join mode names into string
fn join_modes(modes) {
    let result = "";
    let i = 0;
    while i < modes.len() {
        if i > 0 { result += ", "; }
        result += modes[i];
        i += 1;
    }
    return result;
}
