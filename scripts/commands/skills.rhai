// scripts/commands/skills.rhai
// skills - Display your skill levels and progress

// Format skill name for display: "short_blades" -> "Short Blades"
fn format_skill_name(name) {
    let result = "";
    let capitalize_next = true;
    for i in 0..name.len() {
        let ch = name.sub_string(i, 1);
        if ch == "_" {
            result += " ";
            capitalize_next = true;
        } else if capitalize_next {
            result += ch.to_upper();
            capitalize_next = false;
        } else {
            result += ch;
        }
    }
    return result;
}

// Build a 10-char progress bar: [###-------]
fn build_progress_bar(level) {
    let bar = "[";
    for i in 0..10 {
        if i < level {
            bar += "#";
        } else {
            bar += "-";
        }
    }
    bar += "]";
    return bar;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let all_skills = get_all_skills(char.name);

    let output = "=== Your Skills ===\n";

    // Category: Combat (alphabetically sorted)
    let combat_skills = ["long_blades", "long_blunt", "polearms", "ranged", "short_blades", "short_blunt", "unarmed"];
    output += render_category("Combat", combat_skills, all_skills);

    // Category: Crafting (alphabetically sorted)
    let crafting_skills = ["cooking", "crafting", "fishing", "foraging", "gardening"];
    output += render_category("Crafting", crafting_skills, all_skills);

    // Category: Magic
    let magic_skills = ["magic"];
    output += render_category("Magic", magic_skills, all_skills);

    // Category: Stealth (alphabetically sorted)
    let stealth_skills = ["stealth", "thievery", "tracking"];
    output += render_category("Stealth", stealth_skills, all_skills);

    // Category: Utility
    let utility_skills = ["medical", "swimming"];
    output += render_category("Utility", utility_skills, all_skills);

    send_client_message(connection_id, output);
}

fn render_category(category_name, skill_list, all_skills) {
    let out = "\n  -- " + category_name + " --\n";

    for skill_name in skill_list {
        out += render_skill(skill_name, all_skills);
    }

    return out;
}

fn render_skill(skill_name, all_skills) {
    let display_name = format_skill_name(skill_name);

    // Pad skill name to 15 chars with spaces
    let padded_name = display_name;
    while padded_name.len() < 15 {
        padded_name += " ";
    }

    let level = 0;
    let experience = 0;
    let xp_to_level = get_xp_for_level(0);

    if all_skills.contains(skill_name) {
        let skill_data = all_skills[skill_name];
        level = skill_data.level;
        experience = skill_data.experience;
        xp_to_level = skill_data.xp_to_level;
    }

    let bar = build_progress_bar(level);

    // Right-align level number in 2 chars
    let level_str = "" + level;
    if level < 10 {
        level_str = " " + level_str;
    }

    let xp_str = "";
    if level < 10 {
        xp_str = "(" + experience + "/" + xp_to_level + " XP)";
    } else {
        xp_str = "(MASTERED)";
    }

    return "  " + padded_name + bar + "  " + level_str + "   " + xp_str + "\n";
}
