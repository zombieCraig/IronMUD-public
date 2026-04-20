// scripts/commands/racial.rhai
// Racial active ability command

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to use racial abilities.");
        return;
    }

    let race_info = get_race_info(char.race);
    if race_info.len() == 0 {
        send_client_message(connection_id, "Your race has no special abilities.");
        return;
    }

    let actives = race_info["active_abilities"];
    if actives.len() == 0 {
        send_client_message(connection_id, "Your race has no active abilities.");
        return;
    }

    let lower = args.to_lower();

    // No args or "list" - show abilities with cooldown status
    if lower == "" || lower == "list" {
        show_ability_list(connection_id, char, actives);
        return;
    }

    // Try to match ability by name
    let matched = find_ability(lower, actives);
    if matched == () {
        send_client_message(connection_id, "Unknown ability. Use 'racial' to see your abilities.");
        return;
    }

    use_ability(connection_id, char, matched);
}

fn show_ability_list(connection_id, char, actives) {
    let output = "=== Racial Abilities ===\n\n";
    let now = get_current_time();
    let cooldowns = char.racial_cooldowns;

    for a in actives {
        output += "  " + a["name"] + "\n";
        output += "    " + a["description"] + "\n";
        output += format_ability_costs(a);
        output += "    Cooldown: " + a["cooldown_secs"] + "s\n";

        // Check cooldown
        let cd_end = get_cooldown_end(cooldowns, a["id"]);
        if cd_end > now {
            let remaining = cd_end - now;
            output += "    Status: Ready in " + remaining + "s\n";
        } else {
            output += "    Status: READY\n";
        }
        output += "\n";
    }

    output += "Usage: racial <ability_name>\n";
    send_client_message(connection_id, output);
}

fn format_ability_costs(a) {
    let cost_parts = "";
    if a["stamina_cost"] > 0 { cost_parts += a["stamina_cost"] + " stamina"; }
    if a["mana_cost"] > 0 {
        if cost_parts != "" { cost_parts += ", "; }
        cost_parts += a["mana_cost"] + " mana";
    }
    if cost_parts != "" {
        return "    Cost: " + cost_parts + "\n";
    }
    return "";
}

fn get_cooldown_end(cooldowns, aid) {
    if cooldowns.contains_key(aid) {
        return cooldowns[aid];
    }
    return 0;
}

fn find_ability(input, actives) {
    // Try exact match on ID first
    for a in actives {
        if a["id"] == input { return a; }
    }
    // Try match on lowered name
    for a in actives {
        let lower_name = a["name"].to_lower();
        if lower_name == input { return a; }
    }
    // Try partial match on name or ID
    for a in actives {
        let lower_name = a["name"].to_lower();
        if lower_name.index_of(input) == 0 { return a; }
    }
    return ();
}

fn use_ability(connection_id, char, ability) {
    let now = get_current_time();
    let aid = ability["id"];

    // Check cooldown
    let cooldowns = char.racial_cooldowns;
    let cd_end = get_cooldown_end(cooldowns, aid);
    if cd_end > now {
        let remaining = cd_end - now;
        send_client_message(connection_id, ability["name"] + " is on cooldown for " + remaining + " more seconds.");
        return;
    }

    // Check stamina cost
    if ability["stamina_cost"] > 0 {
        if char.stamina < ability["stamina_cost"] {
            send_client_message(connection_id, "Not enough stamina! " + ability["name"] + " requires " + ability["stamina_cost"] + " stamina.");
            return;
        }
    }

    // Check mana cost
    if ability["mana_cost"] > 0 {
        if char.mana < ability["mana_cost"] {
            send_client_message(connection_id, "Not enough mana! " + ability["name"] + " requires " + ability["mana_cost"] + " mana.");
            return;
        }
    }

    // Dispatch to specific ability handler
    if aid == "breath_weapon" {
        use_breath_weapon(connection_id, char, ability);
        return;
    }
    if aid == "adrenaline_surge" {
        use_adrenaline_surge(connection_id, char, ability);
        return;
    }
    if aid == "neural_spike" {
        use_neural_spike(connection_id, char, ability);
        return;
    }
    if aid == "regenerate" {
        use_regenerate(connection_id, char, ability);
        return;
    }

    send_client_message(connection_id, "That ability is not yet implemented.");
}

fn use_breath_weapon(connection_id, char, ability) {
    // Must be in combat
    if !is_in_combat(char.name) {
        send_client_message(connection_id, "You must be in combat to use Breath Weapon.");
        return;
    }

    // Get combat target info
    let target_info = get_primary_target_info(char.name);
    if target_info == () {
        send_client_message(connection_id, "You have no combat target!");
        return;
    }

    let now = get_current_time();

    // Deduct stamina
    char.stamina = char.stamina - ability["stamina_cost"];

    // Set cooldown
    let cooldowns = char.racial_cooldowns;
    cooldowns[ability["id"]] = now + ability["cooldown_secs"];
    char.racial_cooldowns = cooldowns;
    save_character_data(char);

    // Calculate damage: base 10 + level * 3
    let damage = 10 + char.level * 3;

    let target_name = target_info["target_name"];
    let target_id = target_info["target_id"];
    let target_type = target_info["target_type"];

    // Apply damage based on target type
    if target_type == "mobile" {
        let mobile = get_mobile_data(target_id);
        if mobile != () {
            mobile.hp = mobile.hp - damage;
            save_mobile_data(mobile);
        }
    }

    // Messages
    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;33mYou unleash a torrent of dragonfire at " + target_name + " for " + damage + " fire damage!\x1b[0m");
    broadcast_to_room(room_id, char.name + " breathes a cone of fire at " + target_name + "!", char.name);
}

fn use_adrenaline_surge(connection_id, char, ability) {
    let now = get_current_time();

    // Deduct stamina
    char.stamina = char.stamina - ability["stamina_cost"];

    // Set cooldown
    let cooldowns = char.racial_cooldowns;
    cooldowns[ability["id"]] = now + ability["cooldown_secs"];
    char.racial_cooldowns = cooldowns;
    save_character_data(char);

    // Apply haste buff
    apply_buff(char.name, "haste", 1, 60, "Adrenaline Surge");

    // Messages
    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;33mYour cybernetic implants flood your system with adrenaline — everything slows to a crawl around you!\x1b[0m");
    broadcast_to_room(room_id, char.name + "'s eyes dilate as their reflex boosters engage in a burst of inhuman speed.", char.name);
}

fn use_regenerate(connection_id, char, ability) {
    let now = get_current_time();

    // Deduct stamina
    char.stamina = char.stamina - ability["stamina_cost"];

    // Set cooldown
    let cooldowns = char.racial_cooldowns;
    cooldowns[ability["id"]] = now + ability["cooldown_secs"];
    char.racial_cooldowns = cooldowns;
    save_character_data(char);

    // Apply regeneration buff
    apply_buff(char.name, "regeneration", 1, 60, "Regenerate");

    // Messages
    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;32mYour mutated cells surge into overdrive, knitting wounds closed with unnatural speed!\x1b[0m");
    broadcast_to_room(room_id, char.name + "'s wounds begin closing visibly as their body regenerates at an alarming rate.", char.name);
}

fn use_neural_spike(connection_id, char, ability) {
    // Must be in combat
    if !is_in_combat(char.name) {
        send_client_message(connection_id, "You must be in combat to use Neural Spike.");
        return;
    }

    // Get combat target info
    let target_info = get_primary_target_info(char.name);
    if target_info == () {
        send_client_message(connection_id, "You have no combat target!");
        return;
    }

    let now = get_current_time();

    // Deduct mana
    char.mana = char.mana - ability["mana_cost"];

    // Set cooldown
    let cooldowns = char.racial_cooldowns;
    cooldowns[ability["id"]] = now + ability["cooldown_secs"];
    char.racial_cooldowns = cooldowns;
    save_character_data(char);

    // Calculate damage: base 8 + level * 3
    let damage = 8 + char.level * 3;

    let target_name = target_info["target_name"];
    let target_id = target_info["target_id"];
    let target_type = target_info["target_type"];

    // Apply damage based on target type
    if target_type == "mobile" {
        let mobile = get_mobile_data(target_id);
        if mobile != () {
            mobile.hp = mobile.hp - damage;
            save_mobile_data(mobile);
        }
    }

    // Messages
    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;36mYou project a searing psionic spike into " + target_name + "'s mind for " + damage + " psychic damage!\x1b[0m");
    broadcast_to_room(room_id, char.name + " fixes " + target_name + " with an intense stare — " + target_name + " staggers from an unseen force.", char.name);
}
