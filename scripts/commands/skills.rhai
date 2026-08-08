// scripts/commands/skills.rhai
// skills - Display your skill levels and progress

// Skill-name formatting and bar drawing are shared with `status`, which
// renders the same skill data in summary form.
import "progress_ui" as ui;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let all_skills = get_all_skills(char.name);

    let output = "=== Your Skills ===\n";

    for category in ui::core_skill_categories() {
        output += render_category(category.name, category.skills, all_skills);
    }

    // Category: Custom (builder-defined) — surfaces only the keys this
    // character has a non-zero base value for or is buffed in. Empty for
    // worlds that don't use the custom-skill registry.
    let custom_rows = collect_custom_skill_rows(char.name);
    if custom_rows.len() > 0 {
        output += render_custom_category(custom_rows);
    }

    send_client_message(connection_id, output);
}

fn collect_custom_skill_rows(char_name) {
    let rows = [];
    for key in list_character_custom_skills(char_name) {
        let base = get_custom_skill(char_name, key);
        let eff = get_effective_custom_skill(char_name, key);
        rows.push(#{ key: key, base: base, eff: eff });
    }
    return rows;
}

fn render_custom_category(rows) {
    let out = "\n  -- Custom (Builder-Defined) --\n";
    for row in rows {
        let display = ui::format_skill_name(row.key);
        let padded = display;
        while padded.len() < 15 { padded += " "; }
        let diff = row.eff - row.base;
        let value = "" + row.base;
        if diff > 0 { value += " (+" + diff + ")"; }
        else if diff < 0 { value += " (" + diff + ")"; }
        out += "  " + padded + ": " + value + "\n";
    }
    return out;
}

fn render_category(category_name, skill_list, all_skills) {
    let out = "\n  -- " + category_name + " --\n";

    for skill_name in skill_list {
        out += render_skill(skill_name, all_skills);
    }

    return out;
}

fn render_skill(skill_name, all_skills) {
    let display_name = ui::format_skill_name(skill_name);

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

    let bar = ui::build_progress_bar(level);

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
