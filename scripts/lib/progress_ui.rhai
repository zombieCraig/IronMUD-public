// scripts/lib/progress_ui.rhai
// Shared rendering helpers for progression displays.
//
// `skills` and `status` both draw skill bars and both need the same
// underscore-to-title-case skill naming. They had drifted once already; a
// second copy in `status` would have guaranteed it happened again.

// The core skill set, grouped as `skills` displays it. Lives here rather
// than in `skills.rhai` because `status` needs the flattened list to give
// its mastery count a denominator, and two copies would drift.
//
// Builder-defined custom skills are deliberately excluded: they are per-world
// and would make "N of M mastered" mean something different on every server.
fn core_skill_categories() {
    return [
        #{ name: "Combat", skills: ["long_blades", "long_blunt", "polearms", "ranged", "short_blades", "short_blunt", "unarmed"] },
        #{ name: "Crafting", skills: ["cooking", "crafting", "fishing", "foraging", "gardening"] },
        #{ name: "Magic", skills: ["magic"] },
        #{ name: "Stealth", skills: ["stealth", "thievery", "tracking"] },
        #{ name: "Utility", skills: ["medical", "swimming"] },
    ];
}

// There is deliberately no `core_skill_count()` here. `get_renown` returns
// `core_skill_count` from `progress::CORE_SKILLS`, which is the same set this
// file groups for display; a second count computed here would be a second
// source of truth for the denominator. The membership of the two is pinned by
// a guard test in tests/server.rs.

// "short_blades" -> "Short Blades"
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
    return build_bar_of_width(level, 10, 10);
}

// Generalised bar. `filled` of `max` rendered across `width` characters.
// A max of zero renders empty rather than dividing by it.
fn build_bar_of_width(filled, max, width) {
    let filled_cells = 0;
    if max > 0 {
        filled_cells = (filled * width) / max;
        if filled_cells > width { filled_cells = width; }
        if filled_cells < 0 { filled_cells = 0; }
    }
    let bar = "[";
    for i in 0..width {
        if i < filled_cells {
            bar += "#";
        } else {
            bar += "-";
        }
    }
    bar += "]";
    return bar;
}

// Seconds -> "4h 12m", "12m", or "less than a minute". Days are folded in
// once a character has enough time to make the hour count unreadable.
fn format_playtime(seconds) {
    if seconds < 60 { return "less than a minute"; }
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if days > 0 {
        return "" + days + "d " + (hours % 24) + "h";
    }
    if hours > 0 {
        return "" + hours + "h " + (minutes % 60) + "m";
    }
    return "" + minutes + "m";
}

// Right-pad to `width`. Left alone when already at or past it.
fn pad_right(s, width) {
    let out = s;
    while out.len() < width {
        out += " ";
    }
    return out;
}
