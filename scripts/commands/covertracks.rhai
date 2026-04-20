// scripts/commands/covertracks.rhai
// covertracks - Remove your departure records from the current room

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

    let tracking_skill = get_skill_level(char.name, "tracking");
    if tracking_skill < 2 {
        send_client_message(connection_id, "You need tracking level 2 to cover tracks. Practice with track and lore.");
        return;
    }

    if char.stamina < 5 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 5 stamina)");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 5;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    clear_departures_for(room_id, char.name);

    send_client_message(connection_id, "You carefully obscure your tracks.");

    let leveled = add_skill_experience(char.name, "tracking", 5);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
