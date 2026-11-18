// spells.rhai - List available spells
// Usage: spells
//        spells codex [god]   - spells of the gods: canon, faded, rumoured
//        spells info <spell>  - where a spell comes from

fn get_type_header(stype) {
    if stype == "damage" { return "-- Damage Spells --"; }
    if stype == "buff" { return "-- Buff/Defense Spells --"; }
    if stype == "heal" { return "-- Healing Spells --"; }
    if stype == "debuff" { return "-- Debuff Spells --"; }
    if stype == "utility" { return "-- Utility Spells --"; }
    return "-- Other Spells --";
}

fn get_type_color(stype) {
    if stype == "damage" { return "\x1b[1;31m"; }
    if stype == "buff" { return "\x1b[1;34m"; }
    if stype == "heal" { return "\x1b[1;32m"; }
    if stype == "debuff" { return "\x1b[1;35m"; }
    if stype == "utility" { return "\x1b[1;33m"; }
    return "\x1b[1;36m";
}

// 10-char mastery bar: [###-------]
fn build_spell_bar(level) {
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
    if type_of(char) == "()" { return; }

    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < 1 {
        send_client_message(connection_id, "You have no training in magic.");
        return;
    }

    let words = args.split(" ");
    let sub = if words.len() > 0 { words[0].to_lower() } else { "" };
    if sub == "codex" {
        show_codex(connection_id, char, join_words_from(words, 1));
        return;
    }
    if sub == "info" {
        show_spell_info(connection_id, char, join_words_from(words, 1));
        return;
    }

    let available = get_available_spells(char.name);
    // Only spells this character could ever cast; other races' and kinds'
    // disciplines and gifts stay out of the listing.
    let all_spells = get_eligible_spells(char.name);
    let spell_progress = get_all_spell_progress(char.name);

    send_client_message(connection_id, "\x1b[1;35m=== Your Spellbook ===\x1b[0m");
    send_client_message(connection_id, "Magic Skill: " + magic_level + "  |  Mana: " + char.mana + "/" + char.max_mana);
    send_client_message(connection_id, "");

    let known_types = ["damage", "debuff", "buff", "heal", "utility"];

    for stype in ["damage", "debuff", "buff", "heal", "utility", "other"] {
        let type_spells = [];

        // Collect spells of this type. "other" gathers every type without a
        // section of its own (summon, charm, fear, dominate, ...).
        for spell_id in all_spells {
            let info = get_spell_info(spell_id);
            if info.len() > 0 {
                let t = info["spell_type"];
                let matches = if stype == "other" { !known_types.contains(t) } else { t == stype };
                if matches {
                    type_spells.push(spell_id);
                }
            }
        }

        if type_spells.len() == 0 { continue; }
        type_spells.sort();

        let color = get_type_color(stype);
        send_client_message(connection_id, color + get_type_header(stype) + "\x1b[0m");

        for spell_id in type_spells {
            let info = get_spell_info(spell_id);
            let is_available = false;
            for a in available {
                if a == spell_id {
                    is_available = true;
                }
            }

            let name = info["name"];
            let mana = info["mana_cost"];
            let skill_req = info["skill_required"];
            let cd = info["cooldown_secs"];

            let status = "";
            if is_available {
                let cd_remaining = get_spell_cooldown_remaining(char.name, spell_id);
                if cd_remaining > 0 {
                    status = " \x1b[1;31m[CD: " + cd_remaining + "s]\x1b[0m";
                } else {
                    status = " \x1b[1;32m[Ready]\x1b[0m";
                }
            } else {
                if info["scroll_only"] {
                    status = " \x1b[0;37m[Scroll Required]\x1b[0m";
                } else {
                    let req_skill = info["requires_skill"];
                    let label = if req_skill == "magic" { "Skill" } else { req_skill };
                    status = " \x1b[0;37m[" + label + " " + skill_req + " needed]\x1b[0m";
                }
            }

            let line = "  " + color + name + "\x1b[0m";
            line += "  Mana: " + mana + "  CD: " + cd + "s  Skill: " + skill_req;
            line += status;
            send_client_message(connection_id, line);

            // Show description on next line
            send_client_message(connection_id, "    " + info["description"]);

            // Per-spell mastery + evolution hint (only when learned/available).
            if is_available && spell_progress.contains(spell_id) {
                let prog = spell_progress[spell_id];
                let lvl = prog.level;
                let xp_line = if lvl < 10 {
                    "(" + prog.experience + "/" + prog.xp_to_level + " XP)"
                } else {
                    "(MASTERED)"
                };
                let lvl_str = "" + lvl;
                if lvl < 10 { lvl_str = " " + lvl_str; }
                send_client_message(connection_id, "    \x1b[0;36mMastery " + build_spell_bar(lvl) + " " + lvl_str + "  " + xp_line + "\x1b[0m");

                let evolves_id = info["evolves_to_id"];
                if evolves_id != "" {
                    let ev_info = get_spell_info(evolves_id);
                    let ev_name = if ev_info.len() > 0 { ev_info["name"] } else { evolves_id };
                    send_client_message(connection_id, "    \x1b[0;35m-> Evolves into " + ev_name + " at level " + info["evolves_at_level"] + "\x1b[0m");
                }
            }
        }
        send_client_message(connection_id, "");
    }

    send_client_message(connection_id, "Use '\x1b[1mcast <spell> [target]\x1b[0m' to cast a spell.");
    send_client_message(connection_id, "'\x1b[1mspells info <spell>\x1b[0m' tells where a spell comes from; '\x1b[1mspells codex\x1b[0m' lists the spells of the gods.");
}

fn join_words_from(words, start) {
    let out = "";
    for i in start..words.len() {
        if words[i] == "" { continue; }
        if out != "" { out += " "; }
        out += words[i];
    }
    return out;
}

fn knows_spell(char_name, spell_id) {
    for a in get_available_spells(char_name) {
        if a == spell_id { return true; }
    }
    return false;
}

fn status_label(status) {
    if status == "canon" { return "\x1b[1;33mCanon\x1b[0m"; }
    if status == "faded" { return "\x1b[0;37mFaded\x1b[0m"; }
    return "\x1b[0;31mLost\x1b[0m";
}

fn author_label(m) {
    if m.author == "" { return "unknown hands"; }
    if m.author_disgraced { return m.author + " the Faithless"; }
    return m.author;
}

// Codex spells of a god this world doesn't have are godless.
fn god_label(god_vnum) {
    let name = get_god_display_name(god_vnum);
    if name == "" { return "a forgotten god"; }
    return name;
}

// The god filter matches a vnum exactly or the start of the god's name.
fn god_matches(god_vnum, filter) {
    if filter == "" { return true; }
    let f = filter.to_lower();
    if god_vnum.to_lower() == f { return true; }
    return get_god_display_name(god_vnum).to_lower().starts_with(f);
}

// Each god's influence: the devotion its canon holds, most venerated first.
fn influence_lines(filter) {
    let out = "";
    for c in canon_influences() {
        if !god_matches(c.god_vnum, filter) || c.held == 0 { continue; }
        out += "  " + god_label(c.god_vnum) + ": influence " + c.influence + " (" + c.held + " in canon)\n";
    }
    if out == "" { return ""; }
    return "\x1b[1;35mInfluence\x1b[0m\n" + out;
}

fn show_codex(connection_id, char, filter) {
    let shown = 0;
    let rumoured = 0;
    let out = "\x1b[1;35m=== The Codex ===\x1b[0m\n";
    for m in codex_list(false) {
        if !god_matches(m.god_vnum, filter) { continue; }
        // Lost spells are only rumours: counted, never named.
        if m.status == "lost" {
            rumoured += 1;
            continue;
        }
        shown += 1;
        let known = if knows_spell(char.name, m.id) { "  \x1b[1;32m(known)\x1b[0m" } else { "" };
        out += "  \x1b[1;36m" + m.name + "\x1b[0m  [" + status_label(m.status) + ", tier " + m.tier + "]" + known + "\n";
        out += "    of " + god_label(m.god_vnum) + ", by " + author_label(m) + "\n";
    }
    if shown == 0 && rumoured == 0 {
        if filter == "" {
            send_client_message(connection_id, "No spell of the gods is known to the world.");
        } else {
            send_client_message(connection_id, "No spell of the gods matches '" + filter + "'.");
        }
        return;
    }
    if rumoured > 0 {
        out += "  \x1b[0;37mRumoured, but lost: " + rumoured + "\x1b[0m\n";
    }
    out += influence_lines(filter);
    out += "Use 'spells info <spell>' for a spell's history.";
    send_client_message(connection_id, out);
}

fn show_spell_info(connection_id, char, query) {
    if query == "" {
        send_client_message(connection_id, "Usage: spells info <spell>");
        return;
    }
    let spell_id = find_spell_id(query);
    let m = if spell_id == "" { #{} } else { get_codex_meta(spell_id) };
    // Lost and vetoed codex spells stay hidden unless the reader knows them.
    let known = spell_id != "" && knows_spell(char.name, spell_id);
    let hidden = m.len() > 0 && (m.vetoed || m.status == "lost") && !known;
    let info = if spell_id == "" { #{} } else { get_spell_info(spell_id) };
    if info.len() == 0 || hidden {
        send_client_message(connection_id, "You know of no spell called '" + query + "'.");
        return;
    }

    let tier = spell_power_tier(spell_id);
    let out = "\x1b[1;36m" + info["name"] + "\x1b[0m  (" + info["spell_type"] + (if tier > 0 { ", tier " + tier } else { "" }) + ")\n";
    out += "  " + info["description"] + "\n";
    if m.len() > 0 {
        out += "  A spell of " + god_label(m.god_vnum) + ", " + status_label(m.status) + ".\n";
        if m.origin == "ancient" {
            out += "  An ancient working, set down by " + author_label(m) + ".\n";
        } else {
            // After a restoration the author is the restorer; the first
            // sealer is the first lineage entry.
            let first = if m.lineage.len() > 0 && m.lineage[0].kind == "authored" { m.lineage[0].who } else { "" };
            if first != "" && first != m.author {
                out += "  Sealed by " + first + ".\n";
            } else {
                out += "  Sealed by " + author_label(m) + ".\n";
            }
        }
        for l in m.lineage {
            if l.kind == "restored" {
                out += "  Restored by " + l.who + ".\n";
            }
        }
    } else {
        out += "  A spell of common lore.\n";
    }
    // Exact numbers are for those who know the spell.
    if known {
        out += "  Mana: " + info["mana_cost"] + "  CD: " + info["cooldown_secs"] + "s  Skill: " + info["skill_required"];
    } else {
        out += "  You do not know this spell.";
    }
    send_client_message(connection_id, out);
}
