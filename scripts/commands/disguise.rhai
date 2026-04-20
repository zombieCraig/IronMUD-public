// scripts/commands/disguise.rhai
// disguise <alias> - Disguise yourself under a false name
// undisguise - Remove your disguise (handled here with empty args or "remove")

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Wake up first.");
        return;
    }
    if char.position == "sitting" || char.position == "resting" {
        send_client_message(connection_id, "You must stand up first.");
        return;
    }

    let stealth_skill = get_skill_level(char.name, "stealth");
    if stealth_skill < 5 {
        send_client_message(connection_id, "You need stealth level 5 to disguise yourself.");
        return;
    }

    // Check if already disguised - allow removal
    if has_buff(char.name, "disguise") {
        remove_buff(char.name, "disguise");
        send_client_message(connection_id, "You remove your disguise and reveal your true identity.");
        broadcast_to_room_awake(char.current_room_id, char.name + " removes a disguise!", char.name);
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: disguise <alias name>");
        return;
    }

    if char.stamina < 20 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 20 stamina)");
        return;
    }

    // Check for disguise kit in inventory
    let inv_items = get_items_in_inventory(char.name);
    let kit_id = "";
    for item in inv_items {
        for keyword in item.keywords {
            if keyword == "disguise_kit" || keyword == "disguise" {
                kit_id = item.id;
            }
        }
    }

    if kit_id == "" {
        send_client_message(connection_id, "You need a disguise kit to disguise yourself.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 20;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Apply disguise buff - duration 1800 seconds (30 minutes), source stores the alias
    let alias = args;
    apply_buff(char.name, "disguise", 1, 1800, alias);

    send_client_message(connection_id, "You don a disguise and assume the identity of '" + alias + "'.");
    broadcast_to_room_awake(char.current_room_id, char.name + " adjusts their appearance.", char.name);

    let leveled = add_skill_experience(char.name, "stealth", 10);
    if leveled {
        send_client_message(connection_id, "Your stealth skill has improved!");
    }
}
