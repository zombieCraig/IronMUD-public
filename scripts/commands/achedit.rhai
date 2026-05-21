// scripts/commands/achedit.rhai
// Builder editor for achievement definitions.
//
// Usage:
//   achedit list                           # List all achievements
//   achedit create <key> <name...>         # Create new achievement
//   achedit <key>                          # Show achievement details
//   achedit <key> delete                   # Delete achievement
//   achedit <key> name <text>              # Set name
//   achedit <key> desc <text>              # Set description
//   achedit <key> category <cat>           # skill|combat|crafting|exploration|social|wealth|builder
//   achedit <key> hidden on|off            # Set hidden status
//   achedit <key> reward title <text>      # Set rewarded title
//   achedit <key> reward gold <amount>     # Set gold reward (0 clears)
//   achedit <key> reward item <vnum>       # Set item reward (empty clears)
//   achedit <key> reward morality <delta>  # Shift player morality at unlock (+good/-evil, 0 clears)
//   achedit <key> criterion manual         # Set manual award criterion
//   achedit <key> criterion counter <k> <n> # Set counter criterion (key + threshold)
//   achedit <key> criterion skill <k> <n>  # Set skill criterion (skill + level)
//   achedit <key> criterion recipe <vnum>  # Set learned-recipe criterion
//   achedit <key> criterion lease [vnum]   # Set owned-lease criterion (vnum optional)
//   achedit <key> criterion gold <amount>  # Set gold-held high-water criterion

import "olc_helpers" as olc;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let parts = [];
    for tok in args.split(" ") {
        if tok != "" { parts.push(tok); }
    }

    if parts.len() == 0 {
        send_help(connection_id);
        return;
    }

    if parts[0] == "list" {
        handle_list(connection_id);
        return;
    }

    if parts[0] == "create" {
        handle_create(parts, char.name, connection_id);
        return;
    }

    let key = parts[0];
    if parts.len() == 1 {
        show_achievement(key, connection_id);
        return;
    }

    let sub = parts[1].to_lower();

    if sub == "show" { show_achievement(key, connection_id); return; }
    if sub == "delete" { handle_delete(key, connection_id); return; }
    if sub == "name" { handle_name(key, parts, connection_id); return; }
    if sub == "desc" || sub == "description" { handle_desc(key, parts, connection_id); return; }
    if sub == "category" { handle_category(key, parts, connection_id); return; }
    if sub == "hidden" { handle_hidden(key, parts, connection_id); return; }
    if sub == "reward" { handle_reward(key, parts, connection_id); return; }
    if sub == "criterion" { handle_criterion(key, parts, connection_id); return; }

    send_client_message(connection_id, "Unknown subcommand: " + sub);
    send_help(connection_id);
}

fn handle_list(connection_id) {
    let all = list_achievement_defs();
    let out = olc::c_header("Achievement Definitions:", connection_id) + "\n";
    if all.len() == 0 {
        out += "  (none)\n";
    } else {
        for ach in all {
            let src = ach["source"]["kind"];
            let src_tag = if src == "json" { "[JSON] " } else { "[DB]   " };
            out += "  " + src_tag + ach["key"] + " — " + ach["name"] + " (" + ach["category"] + ")\n";
        }
    }
    send_client_message(connection_id, out);
}

fn handle_create(parts, author, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: achedit create <key> <name...>");
        return;
    }
    let key = parts[1];
    let name = olc::build_string_from_parts(parts, 2);
    let err = create_achievement(key, name, author);
    report(err, "Created achievement '" + key + "'.", connection_id);
}

fn handle_delete(key, connection_id) {
    if delete_achievement(key) {
        send_client_message(connection_id, "Deleted achievement '" + key + "'.");
    } else {
        send_client_message(connection_id, "Failed to delete achievement '" + key + "' (maybe it's a JSON definition?)");
    }
}

fn handle_name(key, parts, connection_id) {
    let name = olc::build_string_from_parts(parts, 2);
    if name == "" {
        send_client_message(connection_id, "Usage: achedit <key> name <text>");
        return;
    }
    let err = set_achievement_name(key, name);
    report(err, "Name updated.", connection_id);
}

fn handle_desc(key, parts, connection_id) {
    let desc = olc::build_string_from_parts(parts, 2);
    if desc == "" {
        send_client_message(connection_id, "Usage: achedit <key> desc <text>");
        return;
    }
    let err = set_achievement_description(key, desc);
    report(err, "Description updated.", connection_id);
}

fn handle_category(key, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: achedit <key> category <skill|combat|crafting|exploration|social|wealth|builder>");
        return;
    }
    let err = set_achievement_category(key, parts[2]);
    report(err, "Category updated.", connection_id);
}

fn handle_hidden(key, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: achedit <key> hidden on|off");
        return;
    }
    let on = olc::is_on_value(parts[2]);
    let err = set_achievement_hidden(key, on);
    report(err, "Hidden: " + (if on { "on" } else { "off" }) + ".", connection_id);
}

fn handle_reward(key, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: achedit <key> reward title|gold|item ...");
        return;
    }
    let kind = parts[2].to_lower();
    if kind == "title" {
        let title = olc::build_string_from_parts(parts, 3);
        let err = set_achievement_reward_title(key, title);
        report(err, "Reward title updated.", connection_id);
    } else if kind == "gold" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: achedit <key> reward gold <amount>");
            return;
        }
        let amt = try_parse_int(parts[3]);
        let err = set_achievement_reward_gold(key, amt);
        report(err, "Reward gold updated.", connection_id);
    } else if kind == "item" {
        let vnum = if parts.len() < 4 { "" } else { parts[3] };
        let err = set_achievement_reward_item(key, vnum);
        report(err, "Reward item updated.", connection_id);
    } else if kind == "morality" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: achedit <key> reward morality <delta>");
            return;
        }
        let delta = try_parse_int(parts[3]);
        let err = set_achievement_reward_morality(key, delta);
        report(err, "Reward morality delta updated.", connection_id);
    } else {
        send_client_message(connection_id, "Unknown reward kind: " + kind);
    }
}

fn handle_criterion(key, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: achedit <key> criterion manual|counter|skill|recipe|lease|gold ...");
        return;
    }
    let kind = parts[2].to_lower();
    if kind == "manual" {
        let err = set_achievement_criterion_manual(key);
        report(err, "Criterion set to manual.", connection_id);
    } else if kind == "counter" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: achedit <key> criterion counter <counter_key> <threshold>");
            return;
        }
        let threshold = try_parse_int(parts[4]);
        let err = set_achievement_criterion_counter(key, parts[3], threshold);
        report(err, "Criterion set to counter.", connection_id);
    } else if kind == "skill" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: achedit <key> criterion skill <skill_key> <level>");
            return;
        }
        let level = try_parse_int(parts[4]);
        let err = set_achievement_criterion_skill(key, parts[3], level);
        report(err, "Criterion set to skill reached.", connection_id);
    } else if kind == "recipe" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: achedit <key> criterion recipe <vnum>");
            return;
        }
        let err = set_achievement_criterion_recipe(key, parts[3]);
        report(err, "Criterion set to recipe learned.", connection_id);
    } else if kind == "lease" {
        let vnum = if parts.len() < 4 { "" } else { parts[3] };
        let err = set_achievement_criterion_lease(key, vnum);
        report(err, "Criterion set to owned lease.", connection_id);
    } else if kind == "gold" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: achedit <key> criterion gold <amount>");
            return;
        }
        let amt = try_parse_int(parts[3]);
        let err = set_achievement_criterion_gold(key, amt);
        report(err, "Criterion set to gold held.", connection_id);
    } else {
        send_client_message(connection_id, "Unknown criterion kind: " + kind);
    }
}

fn show_achievement(key, connection_id) {
    let ach = get_achievement_def(key);
    if ach == () {
        send_client_message(connection_id, "No such achievement: " + key);
        return;
    }

    let out = olc::c_header("Achievement: ", connection_id) + olc::c_value(ach["key"], connection_id) + "\n";
    out += olc::c_label("Name:        ", connection_id) + ach["name"] + "\n";
    out += olc::c_label("Description: ", connection_id) + ach["description"] + "\n";
    out += olc::c_label("Category:    ", connection_id) + ach["category"] + "\n";
    out += olc::c_label("Hidden:      ", connection_id) + (if ach["hidden"] { "yes" } else { "no" }) + "\n";

    let src = ach["source"];
    let src_str = if src["kind"] == "json" { "JSON (" + src["file"] + ")" } else { "Database (Author: " + src["author"] + ")" };
    out += olc::c_label("Source:      ", connection_id) + src_str + "\n";

    out += "\n" + olc::c_header("Criterion:", connection_id) + "\n";
    let crit = ach["criterion"];
    let ck = crit["kind"];
    if ck == "manual" {
        out += "  Manual (Awarded by script/admin only)\n";
    } else if ck == "counter" {
        out += "  Counter: " + crit["counter"] + " >= " + crit["threshold"] + "\n";
    } else if ck == "skill_reached" {
        out += "  Skill:   " + crit["skill"] + " >= " + crit["level"] + "\n";
    } else if ck == "recipe_learned" {
        out += "  Recipe:  " + crit["recipe_key"] + "\n";
    } else if ck == "lease_owned" {
        let v = crit["area_vnum"];
        out += "  Lease:   " + (if v == "" { "any area" } else { "area " + v }) + "\n";
    } else if ck == "gold_held" {
        out += "  Gold:    " + crit["amount"] + " held (high-water)\n";
    }

    out += "\n" + olc::c_header("Reward:", connection_id) + "\n";
    let rew = ach["reward"];
    out += olc::c_label("  Title:     ", connection_id) + rew["title"] + "\n";
    if rew["gold"] > 0 {
        out += olc::c_label("  Gold:      ", connection_id) + rew["gold"] + "\n";
    }
    if rew["item_vnum"] != "" {
        out += olc::c_label("  Item:      ", connection_id) + rew["item_vnum"] + "\n";
    }
    if rew["morality_delta"] != 0 {
        out += olc::c_label("  Morality:  ", connection_id) + rew["morality_delta"] + "\n";
    }

    send_client_message(connection_id, out);
}

fn send_help(connection_id) {
    let lines = [
        olc::c_header("Achievement editor:", connection_id),
        "  achedit list                           # List all achievements",
        "  achedit create <key> <name...>         # Create new achievement",
        "  achedit <key> [show]                   # Show achievement details",
        "  achedit <key> name <text>              # Set name",
        "  achedit <key> desc <text>              # Set description",
        "  achedit <key> category <cat>           # Set category",
        "  achedit <key> hidden on|off            # Set hidden status",
        "  achedit <key> reward title <text>      # Set rewarded title",
        "  achedit <key> reward gold <amount>     # Set gold reward",
        "  achedit <key> reward item <vnum>       # Set item reward",
        "  achedit <key> reward morality <delta>  # Shift morality at unlock (+good/-evil, 0 clears)",
        "  achedit <key> criterion manual         # Set manual award criterion",
        "  achedit <key> criterion counter <k> <n> # Set counter criterion",
        "  achedit <key> criterion skill <k> <n>  # Set skill criterion",
        "  achedit <key> criterion recipe <vnum>  # Set learned-recipe criterion",
        "  achedit <key> criterion lease [vnum]   # Set owned-lease criterion",
        "  achedit <key> criterion gold <amount>  # Set gold-held high-water criterion",
        "  achedit <key> delete                   # Delete achievement",
    ];
    let s = "";
    for l in lines { s += l + "\n"; }
    send_client_message(connection_id, s);
}

fn report(err, success_msg, connection_id) {
    if err != "" {
        send_client_message(connection_id, olc::c_warn("Error: ", connection_id) + err);
    } else {
        send_client_message(connection_id, success_msg);
    }
}
