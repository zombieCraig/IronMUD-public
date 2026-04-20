// scripts/commands/lore.rhai
// lore <target> - Study a mobile to learn about its capabilities

fn get_health_desc(hp, max_hp) {
    let pct = (hp * 100) / max_hp;
    if pct >= 90 { return "looks healthy"; }
    if pct >= 60 { return "has some wounds"; }
    if pct >= 30 { return "looks wounded"; }
    if pct >= 10 { return "looks badly wounded"; }
    return "is near death";
}

fn get_danger_desc(mob_level, char_level) {
    let diff = mob_level - char_level;
    if diff <= -10 { return "completely harmless"; }
    if diff <= -5 { return "not very threatening"; }
    if diff <= 0 { return "somewhat threatening"; }
    if diff <= 5 { return "very dangerous"; }
    return "extremely dangerous";
}

fn get_lore_details(mob, tracking_skill) {
    let output = "";
    output += "  Armor: " + mob.armor_class + " AC\n";
    if mob.damage_dice != "" {
        output += "  Damage: " + mob.damage_dice + "\n";
    }
    if mob.flags.aggressive {
        output += "  Temperament: Aggressive - will attack on sight!\n";
    } else {
        output += "  Temperament: Passive\n";
    }
    if mob.flags.sentinel {
        output += "  Behavior: Stationary - does not wander\n";
    }
    if mob.flags.cowardly {
        output += "  Behavior: Cowardly - will flee when injured\n";
    }
    return output;
}

fn get_lore_advanced(mob) {
    let output = "";
    output += "  HP: " + mob.hp + "/" + mob.max_hp + "\n";
    if mob.flags.shopkeeper {
        output += "  Role: Shopkeeper\n";
    }
    if mob.flags.healer {
        output += "  Role: Healer\n";
    }
    if mob.flags.guard {
        output += "  Role: Guard - enhanced perception\n";
    }
    if mob.flags.scavenger {
        output += "  Behavior: Scavenger - picks up items\n";
    }
    if mob.perception > 0 {
        output += "  Perception: " + mob.perception + "/10\n";
    }
    return output;
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

    let tracking_skill = get_skill_level(char.name, "tracking");

    if args == "" {
        send_client_message(connection_id, "Study what? Usage: lore <target>");
        return;
    }

    if char.stamina < 3 && !char.god_mode {
        send_client_message(connection_id, "You are too exhausted. (Need 3 stamina)");
        return;
    }

    if !char.god_mode {
        char.stamina = char.stamina - 3;
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    let room_id = char.current_room_id;
    let target_keyword = args.to_lower();

    // Find target mobile in room
    let mobiles = get_mobiles_in_room(room_id);
    let target_id = "";
    let target_name = "";
    for mobile in mobiles {
        if mobile.name.to_lower().contains(target_keyword) {
            target_id = mobile.id;
            target_name = mobile.name;
        }
        for keyword in mobile.keywords {
            if keyword == target_keyword {
                target_id = mobile.id;
                target_name = mobile.name;
            }
        }
    }

    if target_id == "" {
        send_client_message(connection_id, "You don't see '" + args + "' here.");
        return;
    }

    let mob = get_mobile_data(target_id);
    if mob == () {
        send_client_message(connection_id, "You can't study that.");
        return;
    }

    let output = "You study " + target_name + " carefully...\n";

    // Level 0-1: danger assessment based on level comparison
    if tracking_skill < 2 {
        output += "  It seems " + get_danger_desc(mob.level, char.level) + ".\n";
    }

    // Level 1: add rough health
    if tracking_skill == 1 {
        output += "  Condition: " + get_health_desc(mob.hp, mob.max_hp) + "\n";
    }

    // Level 2+: exact level + health description
    if tracking_skill >= 2 {
        output += "  Level: " + mob.level + "\n";
        output += "  Condition: " + get_health_desc(mob.hp, mob.max_hp) + "\n";
    }

    // Level 4+: AC, damage, temperament
    if tracking_skill >= 4 {
        output += get_lore_details(mob, tracking_skill);
    }

    // Level 7+: exact HP, roles, perception
    if tracking_skill >= 7 {
        output += get_lore_advanced(mob);
    }

    send_client_message(connection_id, output);

    let leveled = add_skill_experience(char.name, "tracking", 5);
    if leveled {
        send_client_message(connection_id, "Your tracking skill has improved!");
    }
}
