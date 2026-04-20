// scripts/commands/reload.rhai
// Load ammunition into a magazine-based ranged weapon (crossbow/firearm)

fn run_command(args, connection_id) {
    let check = validate_reload(connection_id);
    if check.error != "" {
        send_client_message(connection_id, check.error);
        return;
    }
    execute_reload(check.char_name, check.room_id, check.weapon_id, check.weapon_name, connection_id);
}

fn validate_reload(connection_id) {
    let result = #{ "error": "", "char_name": "", "room_id": "", "weapon_id": "", "weapon_name": "" };
    let char = get_player_character(connection_id);
    if char == () { result.error = "You must be logged in to reload."; return result; }
    if char.position == "sleeping" { result.error = "You are asleep! Wake up first."; return result; }
    if is_resting(char) { result.error = "You must stand up first."; return result; }

    let weapon_info = find_wielded_ranged_weapon(char.name);
    if weapon_info.weapon_id == "" { result.error = "You need to wield a ranged weapon to reload."; return result; }

    let rtype = get_item_ranged_type(weapon_info.weapon_id);
    if is_bow_or_unset(rtype) { result.error = "Bows don't need reloading. Use `ready` to ready ammunition."; return result; }

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

fn execute_reload(char_name, room_id, weapon_id, weapon_name, connection_id) {
    let base_mag_size = get_item_magazine_size(weapon_id);
    let bonuses = get_weapon_attachment_bonuses(weapon_id);
    let mag_size = base_mag_size + (base_mag_size * bonuses.magazine_bonus) / 100;
    if mag_size < base_mag_size { mag_size = base_mag_size; }

    // Temporarily set magazine_size to effective size for reload
    set_item_magazine_size(weapon_id, mag_size);

    let loaded = get_item_loaded_ammo(weapon_id);
    if loaded >= mag_size {
        // Restore original magazine_size
        set_item_magazine_size(weapon_id, base_mag_size);
        send_client_message(connection_id, "Your " + weapon_name + " is already fully loaded (" + loaded + "/" + mag_size + ").");
        return;
    }

    let result = perform_reload(weapon_id, char_name);

    // Restore original magazine_size after reload
    set_item_magazine_size(weapon_id, base_mag_size);
    if !result.success {
        send_client_message(connection_id, result.message);
        return;
    }

    send_client_message(connection_id, result.message);
    broadcast_to_room_awake(room_id, char_name + " reloads their " + weapon_name + ".", char_name);

    if is_in_combat(char_name) {
        set_reloading(char_name, true);
        send_client_message(connection_id, "You begin reloading in combat. You'll skip your next attack.");
    }
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
