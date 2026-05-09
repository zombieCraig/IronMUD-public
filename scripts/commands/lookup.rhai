// scripts/commands/lookup.rhai
// Builder knowledge-base lookup. Browse spells/traits/effects/skills with
// olist-style list/detail/search subforms. Builder/admin only — surfaces
// internal numbers (damage_base, scaling, costs) not normally player-facing.
//
// Usage:
//   lookup spell                    — list all spell ids
//   lookup spell <name>             — detail card
//   lookup spell search <text>      — substring match name + description
//   lookup trait [<name>|search <text>]
//   lookup effect [<name>|search <text>]
//   lookup skill  [<name>]

fn c_header(text, conn_id) { color_or(ansi_green(text), text, conn_id) }
fn c_label(text, conn_id)  { color_or(ansi_cyan(text),  text, conn_id) }
fn c_value(text, conn_id)  { color_or(ansi_white("" + text), "" + text, conn_id) }
fn c_dim(text, conn_id)    { color_or(ansi_yellow(text), text, conn_id) }

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }
    if !char.is_builder && !char.is_admin {
        send_client_message(connection_id, "You must be a builder to use this command.");
        return;
    }

    let trimmed = args;
    if trimmed == "" {
        send_lookup_usage(connection_id);
        return;
    }

    let parts = trimmed.split(" ");
    let kind = parts[0].to_lower();
    let rest = "";
    if parts.len() > 1 {
        rest = parts[1];
        for i in 2..parts.len() {
            rest += " " + parts[i];
        }
    }

    if kind == "spell"  { dispatch_spell(rest, connection_id);  return; }
    if kind == "trait"  { dispatch_trait(rest, connection_id);  return; }
    if kind == "effect" { dispatch_effect(rest, connection_id); return; }
    if kind == "skill"  { dispatch_skill(rest, connection_id);  return; }

    send_client_message(connection_id, "Unknown lookup kind: " + kind);
    send_lookup_usage(connection_id);
}

fn send_lookup_usage(conn_id) {
    send_client_message(conn_id, "Usage: lookup <spell|trait|effect|skill> [<name> | search <text>]");
    send_client_message(conn_id, "  lookup spell                  — list all spells");
    send_client_message(conn_id, "  lookup spell heal             — detail card");
    send_client_message(conn_id, "  lookup spell search fire      — substring match");
}

// ===== spells =====

fn dispatch_spell(rest, conn_id) {
    if rest == "" {
        list_spells(conn_id);
        return;
    }
    let parts = rest.split(" ");
    if parts[0].to_lower() == "search" && parts.len() >= 2 {
        let q = parts[1];
        for i in 2..parts.len() { q += " " + parts[i]; }
        search_spells(q, conn_id);
        return;
    }
    show_spell_detail(rest, conn_id);
}

fn list_spells(conn_id) {
    let ids = get_spell_list();
    if ids.len() == 0 {
        send_client_message(conn_id, "No spells defined.");
        return;
    }
    let labels = [];
    for id in ids {
        let info = get_spell_info(id);
        let name = if info.len() > 0 { info["name"] } else { "" + id };
        labels.push("" + id + " (" + name + ")");
    }
    labels.sort();
    let term_width = get_terminal_width(conn_id);
    let output = c_header("=== Spells (" + labels.len() + ") ===\n", conn_id);
    output += format_columns(labels, term_width, 2);
    output += "\n" + c_dim("`lookup spell <id>` for details.", conn_id);
    send_client_message(conn_id, output);
}

fn search_spells(query, conn_id) {
    let q = query.to_lower();
    let hits = [];
    let ids = get_spell_list();
    for id in ids {
        let info = get_spell_info(id);
        if info.len() == 0 { continue; }
        let hay = ("" + id + " " + info["name"] + " " + info["description"]).to_lower();
        if hay.contains(q) {
            hits.push("" + id + " — " + info["name"]);
        }
    }
    if hits.len() == 0 {
        send_client_message(conn_id, "No spells match '" + query + "'.");
        return;
    }
    hits.sort();
    let output = c_header("=== Spell search '" + query + "' (" + hits.len() + ") ===\n", conn_id);
    for h in hits { output += "  " + h + "\n"; }
    send_client_message(conn_id, output);
}

fn show_spell_detail(query, conn_id) {
    let lower = query.to_lower();
    // Try exact id first.
    let info = get_spell_info(query);
    if info.len() == 0 {
        info = get_spell_info(lower);
    }
    if info.len() == 0 {
        // Fall back to first substring match.
        let ids = get_spell_list();
        for id in ids {
            let candidate = get_spell_info(id);
            if candidate.len() == 0 { continue; }
            if id.to_lower().contains(lower) || candidate["name"].to_lower().contains(lower) {
                info = candidate;
                break;
            }
        }
    }
    if info.len() == 0 {
        send_client_message(conn_id, "No spell matches '" + query + "'. Try `lookup spell` to list.");
        return;
    }
    let stype = info["spell_type"];
    let out = c_header("=== Spell: " + info["name"] + " ===\n", conn_id);
    out += c_label("  id:              ", conn_id) + c_value(info["id"], conn_id) + "\n";
    out += c_label("  type:            ", conn_id) + c_value(stype, conn_id) + "\n";
    out += c_label("  target:          ", conn_id) + c_value(info["target_type"], conn_id) + "\n";
    out += c_label("  mana cost:       ", conn_id) + c_value(info["mana_cost"], conn_id) + "\n";
    out += c_label("  cooldown:        ", conn_id) + c_value("" + info["cooldown_secs"] + "s", conn_id) + "\n";
    out += c_label("  skill required:  ", conn_id) + c_value(info["skill_required"], conn_id);
    if info["scroll_only"] { out += c_dim(" (scroll-only)", conn_id); }
    out += "\n";
    out += c_label("  reagent:         ", conn_id) + c_value(info["reagent_vnum"], conn_id) + "\n";
    out += c_label("  xp award:        ", conn_id) + c_value(info["xp_award"], conn_id) + "\n";
    out += c_label("  requires combat: ", conn_id) + c_value(info["requires_combat"], conn_id) + "\n";

    if stype == "damage" {
        out += c_label("  damage:          ", conn_id);
        out += c_value("" + info["damage_base"] + " base + " + info["damage_per_skill"] + "/skill + " + info["damage_int_scaling"] + "·int", conn_id);
        out += c_label(" (", conn_id) + c_value(info["damage_type"], conn_id) + c_label(")", conn_id) + "\n";
    }
    if stype == "heal" {
        out += c_label("  heal:            ", conn_id);
        out += c_value("" + info["heal_base"] + " base + " + info["heal_per_skill"] + "/skill + " + info["heal_int_scaling"] + "·int", conn_id) + "\n";
    }
    if stype == "buff" || stype == "debuff" || stype == "utility" {
        out += c_label("  effect:          ", conn_id) + c_value(info["buff_effect"], conn_id);
        out += c_label(" mag=", conn_id) + c_value(info["buff_magnitude"], conn_id);
        out += c_label(" dur=", conn_id) + c_value("" + info["buff_duration_secs"] + "s", conn_id) + "\n";
    }
    out += "\n" + c_dim(info["description"], conn_id);
    send_client_message(conn_id, out);
}

// ===== traits =====

fn dispatch_trait(rest, conn_id) {
    if rest == "" {
        list_traits(conn_id);
        return;
    }
    let parts = rest.split(" ");
    if parts[0].to_lower() == "search" && parts.len() >= 2 {
        let q = parts[1];
        for i in 2..parts.len() { q += " " + parts[i]; }
        search_traits(q, conn_id);
        return;
    }
    show_trait_detail(rest, conn_id);
}

fn list_traits(conn_id) {
    let ids = get_trait_list();
    if ids.len() == 0 {
        send_client_message(conn_id, "No traits defined.");
        return;
    }
    let labels = [];
    for id in ids {
        let info = get_trait_info(id);
        if info.len() == 0 { continue; }
        let suffix = "" + info["cost"];
        if info["cost"] >= 0 { suffix = "+" + suffix; }
        labels.push("" + id + " [" + suffix + "]");
    }
    labels.sort();
    let term_width = get_terminal_width(conn_id);
    let output = c_header("=== Traits (" + labels.len() + ") ===\n", conn_id);
    output += format_columns(labels, term_width, 2);
    output += "\n" + c_dim("`lookup trait <id>` for details.", conn_id);
    send_client_message(conn_id, output);
}

fn search_traits(query, conn_id) {
    let q = query.to_lower();
    let hits = [];
    let ids = get_trait_list();
    for id in ids {
        let info = get_trait_info(id);
        if info.len() == 0 { continue; }
        let hay = ("" + id + " " + info["name"] + " " + info["description"]).to_lower();
        if hay.contains(q) {
            hits.push("" + id + " — " + info["name"]);
        }
    }
    if hits.len() == 0 {
        send_client_message(conn_id, "No traits match '" + query + "'.");
        return;
    }
    hits.sort();
    let output = c_header("=== Trait search '" + query + "' (" + hits.len() + ") ===\n", conn_id);
    for h in hits { output += "  " + h + "\n"; }
    send_client_message(conn_id, output);
}

fn show_trait_detail(query, conn_id) {
    let info = get_trait_info(query);
    if info.len() == 0 {
        info = get_trait_info(query.to_lower());
    }
    if info.len() == 0 {
        let ids = get_trait_list();
        let lower = query.to_lower();
        for id in ids {
            let candidate = get_trait_info(id);
            if candidate.len() == 0 { continue; }
            if id.to_lower().contains(lower) || candidate["name"].to_lower().contains(lower) {
                info = candidate;
                break;
            }
        }
    }
    if info.len() == 0 {
        send_client_message(conn_id, "No trait matches '" + query + "'. Try `lookup trait`.");
        return;
    }
    let out = c_header("=== Trait: " + info["name"] + " ===\n", conn_id);
    out += c_label("  id:           ", conn_id) + c_value(info["id"], conn_id) + "\n";
    out += c_label("  category:     ", conn_id) + c_value(info["category"], conn_id) + "\n";
    out += c_label("  cost:         ", conn_id) + c_value(info["cost"], conn_id) + "\n";
    out += c_label("  available:    ", conn_id) + c_value(info["available"], conn_id) + "\n";
    let conflicts = info["conflicts_with"];
    if conflicts.len() > 0 {
        out += c_label("  conflicts:    ", conn_id) + c_value(conflicts.join(", "), conn_id) + "\n";
    }
    let effects = info["effects"];
    if effects.len() > 0 {
        out += c_label("  effects:\n", conn_id);
        for k in effects.keys() {
            out += "    " + c_label(k + ": ", conn_id) + c_value(effects[k], conn_id) + "\n";
        }
    }
    out += "\n" + c_dim(info["description"], conn_id);
    send_client_message(conn_id, out);
}

// ===== effects =====

fn dispatch_effect(rest, conn_id) {
    if rest == "" {
        list_effects(conn_id);
        return;
    }
    let parts = rest.split(" ");
    if parts[0].to_lower() == "search" && parts.len() >= 2 {
        let q = parts[1];
        for i in 2..parts.len() { q += " " + parts[i]; }
        search_effects(q, conn_id);
        return;
    }
    show_effect_detail(rest, conn_id);
}

fn list_effects(conn_id) {
    let names = get_effect_list();
    if names.len() == 0 {
        send_client_message(conn_id, "No effects defined.");
        return;
    }
    names.sort();
    let term_width = get_terminal_width(conn_id);
    let output = c_header("=== EffectType variants (" + names.len() + ") ===\n", conn_id);
    output += format_columns(names, term_width, 2);
    output += "\n" + c_dim("`lookup effect <name>` for cross-references.", conn_id);
    send_client_message(conn_id, output);
}

fn search_effects(query, conn_id) {
    let q = query.to_lower();
    let names = get_effect_list();
    let hits = [];
    for n in names {
        if n.to_lower().contains(q) { hits.push(n); }
    }
    if hits.len() == 0 {
        send_client_message(conn_id, "No effects match '" + query + "'.");
        return;
    }
    hits.sort();
    let out = c_header("=== Effect search '" + query + "' (" + hits.len() + ") ===\n", conn_id);
    for h in hits { out += "  " + h + "\n"; }
    send_client_message(conn_id, out);
}

fn show_effect_detail(query, conn_id) {
    let info = get_effect_info(query);
    if info.len() == 0 {
        send_client_message(conn_id, "Unknown effect '" + query + "'. Try `lookup effect` to list variants.");
        return;
    }
    let out = c_header("=== Effect: " + info["display"] + " ===\n", conn_id);
    out += c_label("  name:    ", conn_id) + c_value(info["name"], conn_id) + "\n";
    out += c_label("  display: ", conn_id) + c_value(info["display"], conn_id) + "\n";
    let used = info["used_by_spells"];
    if used.len() == 0 {
        out += c_dim("\nNo spells reference this effect (yet).", conn_id);
    } else {
        out += c_label("\nUsed by spells (" + used.len() + "):\n", conn_id);
        for entry in used {
            out += "  " + entry["id"] + " — " + entry["name"] + "\n";
        }
    }
    send_client_message(conn_id, out);
}

// ===== skills =====

fn dispatch_skill(rest, conn_id) {
    if rest == "" {
        list_skills(conn_id);
        return;
    }
    show_skill_detail(rest, conn_id);
}

fn list_skills(conn_id) {
    let names = get_skill_list();
    if names.len() == 0 {
        send_client_message(conn_id, "No skills defined.");
        return;
    }
    names.sort();
    let term_width = get_terminal_width(conn_id);
    let output = c_header("=== Skills (" + names.len() + ") ===\n", conn_id);
    output += format_columns(names, term_width, 2);
    output += "\n" + c_dim("`lookup skill <name>` for cross-references. No catalog yet — names only.", conn_id);
    send_client_message(conn_id, output);
}

fn show_skill_detail(query, conn_id) {
    let lower = query.to_lower();
    let info = get_skill_references(lower);
    let spells = info["spells"];
    let recipes = info["recipes"];
    if spells.len() == 0 && recipes.len() == 0 {
        send_client_message(conn_id, "No spells or recipes reference skill '" + lower + "'.");
        return;
    }
    let out = c_header("=== Skill: " + lower + " ===\n", conn_id);
    if spells.len() > 0 {
        out += c_label("Spells gating on this skill (" + spells.len() + "):\n", conn_id);
        for entry in spells {
            out += "  " + entry["id"] + " — " + entry["name"] + " (req level " + entry["skill_required"] + ")\n";
        }
    }
    if recipes.len() > 0 {
        out += c_label("\nRecipes gating on this skill (" + recipes.len() + "):\n", conn_id);
        for entry in recipes {
            out += "  " + entry["id"] + " — " + entry["name"] + " (req level " + entry["skill_required"] + ")\n";
        }
    }
    send_client_message(conn_id, out);
}
