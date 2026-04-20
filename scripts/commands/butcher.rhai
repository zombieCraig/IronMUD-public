// scripts/commands/butcher.rhai
// butcher <corpse> - Butcher a corpse for materials

fn get_material_type(keywords) {
    // Check if animal corpse based on keywords
    for kw in keywords {
        if kw == "animal" { return "animal"; }
        if kw == "beast" { return "animal"; }
        if kw == "wolf" { return "animal"; }
        if kw == "bear" { return "animal"; }
        if kw == "deer" { return "animal"; }
        if kw == "boar" { return "animal"; }
        if kw == "rat" { return "animal"; }
        if kw == "spider" { return "animal"; }
        if kw == "snake" { return "animal"; }
        if kw == "fish" { return "animal"; }
        if kw == "bird" { return "animal"; }
        if kw == "cat" { return "animal"; }
        if kw == "dog" { return "animal"; }
        if kw == "lion" { return "animal"; }
        if kw == "tiger" { return "animal"; }
    }
    return "humanoid";
}

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
        send_client_message(connection_id, "You need tracking level 2 to butcher. Practice with track and lore.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Butcher what? Usage: butcher <corpse>");
        return;
    }

    if char.stamina < 10 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 10 stamina)");
        return;
    }

    let room_id = char.current_room_id;
    let target_keyword = args.to_lower();

    // Find a corpse in the room
    let items = get_items_in_room(room_id);
    let corpse_id = "";
    let corpse_name = "";
    let corpse_keywords = [];
    for item in items {
        if !item.flags.is_corpse { continue; }
        if item.name.to_lower().contains(target_keyword) {
            corpse_id = item.id;
            corpse_name = item.name;
            corpse_keywords = item.keywords;
        }
        for kw in item.keywords {
            if kw == target_keyword {
                corpse_id = item.id;
                corpse_name = item.name;
                corpse_keywords = item.keywords;
            }
        }
    }

    if corpse_id == "" {
        send_client_message(connection_id, "You don't see a corpse matching '" + args + "' here.");
        return;
    }

    // Check if already skinned
    if get_item_flag(corpse_id, "is_skinned") {
        send_client_message(connection_id, "This corpse has already been butchered.");
        return;
    }

    // Consume stamina
    if !char.god_mode {
        char.stamina = char.stamina - 10;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Set corpse as skinned
    set_item_flag(corpse_id, "is_skinned", true);

    let material = get_material_type(corpse_keywords);
    let yield_count = 1 + (tracking_skill / 3);
    if yield_count > 4 { yield_count = 4; }

    send_client_message(connection_id, "You carefully butcher " + corpse_name + "...");
    broadcast_to_room_awake(room_id, char.name + " butchers " + corpse_name + ".", char.name);

    if material == "animal" {
        // Animal corpses yield meat, pelts, and bones
        let found = 0;
        if yield_count >= 1 {
            // Try to spawn raw meat
            let meat = spawn_item_to_inventory("materials:raw_meat", char.name);
            if meat != () {
                send_client_message(connection_id, "You carve out some raw meat.");
                found += 1;
            } else {
                send_client_message(connection_id, "You carve out some usable meat. (Item prototype 'materials:raw_meat' not found - would spawn here)");
                found += 1;
            }
        }
        if yield_count >= 2 {
            let pelt = spawn_item_to_inventory("materials:animal_pelt", char.name);
            if pelt != () {
                send_client_message(connection_id, "You carefully skin an animal pelt.");
                found += 1;
            } else {
                send_client_message(connection_id, "You skin a usable pelt. (Item prototype 'materials:animal_pelt' not found - would spawn here)");
                found += 1;
            }
        }
        if yield_count >= 3 {
            let bones = spawn_item_to_inventory("materials:animal_bones", char.name);
            if bones != () {
                send_client_message(connection_id, "You collect some bones.");
                found += 1;
            } else {
                send_client_message(connection_id, "You collect some bones. (Item prototype 'materials:animal_bones' not found - would spawn here)");
                found += 1;
            }
        }
        if yield_count >= 4 {
            // Extra meat at high skill
            let meat2 = spawn_item_to_inventory("materials:raw_meat", char.name);
            if meat2 != () {
                send_client_message(connection_id, "You carve out an extra portion of meat.");
                found += 1;
            }
        }
        send_client_message(connection_id, "You obtained " + found + " materials from the corpse.");
    } else {
        // Humanoid corpses yield little
        send_client_message(connection_id, "The humanoid corpse yields little of value.");
        let bones = spawn_item_to_inventory("materials:animal_bones", char.name);
        if bones != () {
            send_client_message(connection_id, "You manage to collect some bones.");
        }
    }

    let leveled = add_skill_experience(char.name, "tracking", 8);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
