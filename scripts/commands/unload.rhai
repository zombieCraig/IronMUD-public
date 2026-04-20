// scripts/commands/unload.rhai
// Eject loaded ammunition from a magazine-based ranged weapon

fn run_command(args, connection_id) {
    let check = validate_unload(connection_id);
    if check.error != "" {
        send_client_message(connection_id, check.error);
        return;
    }
    execute_unload(check.char_name, check.room_id, check.weapon_id, check.weapon_name, connection_id);
}

fn validate_unload(connection_id) {
    let result = #{ "error": "", "char_name": "", "room_id": "", "weapon_id": "", "weapon_name": "" };
    let char = get_player_character(connection_id);
    if char == () { result.error = "You must be logged in to unload."; return result; }
    if char.position == "sleeping" { result.error = "You are asleep! Wake up first."; return result; }
    if is_resting(char) { result.error = "You must stand up first."; return result; }

    let weapon_info = find_wielded_ranged_weapon(char.name);
    if weapon_info.weapon_id == "" { result.error = "You need to wield a ranged weapon to unload."; return result; }

    let rtype = get_item_ranged_type(weapon_info.weapon_id);
    if is_bow_or_unset(rtype) { result.error = "Bows don't have magazines to unload."; return result; }

    let loaded = get_item_loaded_ammo(weapon_info.weapon_id);
    if loaded <= 0 { result.error = "Your " + weapon_info.name + " is not loaded."; return result; }

    result.char_name = char.name;
    result.room_id = char.current_room_id;
    result.weapon_id = weapon_info.weapon_id;
    result.weapon_name = weapon_info.name;
    return result;
}

fn is_resting(char) {
    if char.position == "resting" { return true; }
    if char.position == "sitting" { return true; }
    return false;
}

fn is_bow_or_unset(rtype) {
    if rtype == () { return true; }
    if rtype == "" { return true; }
    if rtype == "bow" { return true; }
    return false;
}

fn execute_unload(char_name, room_id, weapon_id, weapon_name, connection_id) {
    let result = perform_unload(weapon_id, char_name);
    if !result.success {
        send_client_message(connection_id, result.message);
        return;
    }

    send_client_message(connection_id, result.message);
    broadcast_to_room_awake(room_id, char_name + " unloads their " + weapon_name + ".", char_name);
}

fn find_wielded_ranged_weapon(char_name) {
    let result = #{ "weapon_id": "", "name": "" };
    let equipped = get_equipped_items(char_name);
    for item in equipped {
        if is_wielded_ranged(item) {
            result.weapon_id = item.id;
            result.name = item.name;
            return result;
        }
    }
    return result;
}

fn is_wielded_ranged(item) {
    let locs = item.wear_locations;
    for loc in locs {
        if loc.to_lower() == "wielded" {
            let wskill = get_item_weapon_skill(item.id);
            if wskill == "ranged" { return true; }
        }
    }
    return false;
}
