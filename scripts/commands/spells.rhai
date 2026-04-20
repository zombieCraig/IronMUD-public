// spells.rhai - List available spells
// Usage: spells

fn get_type_header(stype) {
    if stype == "damage" { return "-- Damage Spells --"; }
    if stype == "buff" { return "-- Buff/Defense Spells --"; }
    if stype == "heal" { return "-- Healing Spells --"; }
    if stype == "utility" { return "-- Utility Spells --"; }
    return "-- Other --";
}

fn get_type_color(stype) {
    if stype == "damage" { return "\x1b[1;31m"; }
    if stype == "buff" { return "\x1b[1;34m"; }
    if stype == "heal" { return "\x1b[1;32m"; }
    if stype == "utility" { return "\x1b[1;33m"; }
    return "\x1b[0m";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if type_of(char) == "()" { return; }

    let magic_level = get_skill_level(char.name, "magic");
    if magic_level < 1 {
        send_client_message(connection_id, "You have no training in magic.");
        return;
    }

    let available = get_available_spells(char.name);
    let all_spells = get_spell_list();

    send_client_message(connection_id, "\x1b[1;35m=== Your Spellbook ===\x1b[0m");
    send_client_message(connection_id, "Magic Skill: " + magic_level + "  |  Mana: " + char.mana + "/" + char.max_mana);
    send_client_message(connection_id, "");

    let types = ["damage", "buff", "heal", "utility"];

    for stype in types {
        let type_spells = [];

        // Collect spells of this type
        for spell_id in all_spells {
            let info = get_spell_info(spell_id);
            if info.len() > 0 {
                if info["spell_type"] == stype {
                    type_spells.push(spell_id);
                }
            }
        }

        if type_spells.len() == 0 { continue; }

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
                    status = " \x1b[0;37m[Skill " + skill_req + " needed]\x1b[0m";
                }
            }

            let line = "  " + color + name + "\x1b[0m";
            line += "  Mana: " + mana + "  CD: " + cd + "s  Skill: " + skill_req;
            line += status;
            send_client_message(connection_id, line);

            // Show description on next line
            send_client_message(connection_id, "    " + info["description"]);
        }
        send_client_message(connection_id, "");
    }

    send_client_message(connection_id, "Use '\x1b[1mcast <spell> [target]\x1b[0m' to cast a spell.");
}
