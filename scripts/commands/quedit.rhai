// scripts/commands/quedit.rhai
// Builder editor for quest prototypes.
//
// Usage:
//   quedit create <vnum> <name...>
//   quedit <vnum>                          # show
//   quedit <vnum> show
//   quedit <vnum> name <text>
//   quedit <vnum> summary <text>
//   quedit <vnum> desc <text>
//   quedit <vnum> completion <text>
//   quedit <vnum> giver <mob_vnum|"">      # "" clears
//   quedit <vnum> keywords <kw1 kw2 ...>
//   quedit <vnum> repeatable on|off
//   quedit <vnum> objective add kill <mob_vnum> <count>
//   quedit <vnum> objective add bring <item_vnum> <qty> [<return_mob>]
//   quedit <vnum> objective add visit <room_vnum>
//   quedit <vnum> objective add flag <var> <value>
//   quedit <vnum> objective remove <idx>
//   quedit <vnum> reward add gold <amount>
//   quedit <vnum> reward add item <item_vnum> <qty>
//   quedit <vnum> reward add skill <skill_key> <amount>
//   quedit <vnum> reward add achievement <key>
//   quedit <vnum> reward add recipe <recipe_id>
//   quedit <vnum> reward add embrace_clan <brujah|toreador|ventrue|nosferatu|gangrel>
//   quedit <vnum> reward remove <idx>
//   quedit <vnum> delete

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

    if parts[0] == "create" {
        handle_create(parts, connection_id);
        return;
    }

    let vnum = parts[0];
    if parts.len() == 1 {
        show_quest(vnum, connection_id);
        return;
    }

    let sub = parts[1];

    if sub == "show" { show_quest(vnum, connection_id); return; }
    if sub == "delete" { handle_delete(vnum, connection_id); return; }
    if sub == "name" { handle_string_set(vnum, parts, 2, "set_quest_name", "name", connection_id); return; }
    if sub == "summary" { handle_string_set(vnum, parts, 2, "set_quest_summary", "summary", connection_id); return; }
    if sub == "desc" || sub == "description" {
        handle_string_set(vnum, parts, 2, "set_quest_description", "description", connection_id);
        return;
    }
    if sub == "completion" {
        handle_string_set(vnum, parts, 2, "set_quest_completion_text", "completion text", connection_id);
        return;
    }
    if sub == "giver" {
        let mob_vnum = if parts.len() < 3 { "" } else { parts[2] };
        let err = set_quest_giver(vnum, mob_vnum);
        report(err, "Quest giver " + (if mob_vnum == "" { "cleared" } else { "set to " + mob_vnum }) + ".", connection_id);
        return;
    }
    if sub == "keywords" {
        let rest = join_from(parts, 2);
        let err = set_quest_keywords(vnum, rest);
        report(err, "Keywords updated.", connection_id);
        return;
    }
    if sub == "repeatable" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: quedit <vnum> repeatable on|off");
            return;
        }
        let on = parts[2].to_lower() == "on" || parts[2].to_lower() == "true";
        let err = set_quest_repeatable(vnum, on);
        report(err, "Repeatable: " + (if on { "on" } else { "off" }) + ".", connection_id);
        return;
    }
    if sub == "prereq" {
        let prereq_vnum = if parts.len() < 3 { "" } else { parts[2] };
        let err = set_quest_prereq(vnum, prereq_vnum);
        let label = if prereq_vnum == "" || prereq_vnum.to_lower() == "clear" || prereq_vnum.to_lower() == "none" {
            "cleared"
        } else {
            "set to " + prereq_vnum
        };
        report(err, "Prereq quest " + label + ".", connection_id);
        return;
    }
    if sub == "min_skill" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: quedit <vnum> min_skill <n>  (0 clears)");
            return;
        }
        let n = parse_int(parts[2]);
        let err = set_quest_min_skill(vnum, n);
        let label = if n <= 0 { "cleared" } else { "set to " + n };
        report(err, "Min skill total " + label + ".", connection_id);
        return;
    }
    if sub == "duration" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: quedit <vnum> duration <secs>  (0 clears, no expiry)");
            return;
        }
        let secs = parse_int(parts[2]);
        let err = set_quest_duration(vnum, secs);
        let label = if secs <= 0 { "cleared (no expiry)" } else { "set to " + secs + " seconds" };
        report(err, "Duration " + label + ".", connection_id);
        return;
    }
    if sub == "objective" {
        handle_objective(vnum, parts, connection_id);
        return;
    }
    if sub == "reward" {
        handle_reward(vnum, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + sub);
    send_help(connection_id);
}

fn handle_create(parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: quedit create <vnum> <name...>");
        return;
    }
    let vnum = parts[1];
    let name = join_from(parts, 2);
    let err = create_quest(vnum, name);
    report(err, "Created quest " + vnum + " (" + name + ").", connection_id);
}

fn handle_delete(vnum, connection_id) {
    if delete_quest(vnum) {
        send_client_message(connection_id, "Deleted quest " + vnum + ".");
    } else {
        send_client_message(connection_id, "Failed to delete quest " + vnum + ".");
    }
}

fn handle_string_set(vnum, parts, start_idx, fn_name, label, connection_id) {
    let value = join_from(parts, start_idx);
    if value == "" {
        send_client_message(connection_id, "Usage: quedit <vnum> " + label + " <text>");
        return;
    }
    let err = "";
    if fn_name == "set_quest_name" { err = set_quest_name(vnum, value); }
    else if fn_name == "set_quest_summary" { err = set_quest_summary(vnum, value); }
    else if fn_name == "set_quest_description" { err = set_quest_description(vnum, value); }
    else if fn_name == "set_quest_completion_text" { err = set_quest_completion_text(vnum, value); }
    report(err, capitalize(label) + " updated.", connection_id);
}

fn handle_objective(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: quedit <vnum> objective add|remove ...");
        return;
    }
    let action = parts[2];
    if action == "add" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: quedit <vnum> objective add kill|bring|visit|flag ...");
            return;
        }
        let kind = parts[3];
        if kind == "kill" {
            if parts.len() < 6 {
                send_client_message(connection_id, "Usage: quedit <vnum> objective add kill <mob_vnum> <count>");
                return;
            }
            let count = parse_int(parts[5]);
            let err = add_quest_objective_kill(vnum, parts[4], count);
            report(err, "Added kill objective.", connection_id);
            return;
        }
        if kind == "bring" {
            if parts.len() < 6 {
                send_client_message(connection_id, "Usage: quedit <vnum> objective add bring <item_vnum> <qty> [<return_mob>]");
                return;
            }
            let qty = parse_int(parts[5]);
            let return_mob = if parts.len() >= 7 { parts[6] } else { "" };
            let err = add_quest_objective_bring(vnum, parts[4], qty, return_mob);
            report(err, "Added bring objective.", connection_id);
            return;
        }
        if kind == "visit" {
            if parts.len() < 5 {
                send_client_message(connection_id, "Usage: quedit <vnum> objective add visit <room_vnum>");
                return;
            }
            let err = add_quest_objective_visit(vnum, parts[4]);
            report(err, "Added visit objective.", connection_id);
            return;
        }
        if kind == "flag" {
            if parts.len() < 6 {
                send_client_message(connection_id, "Usage: quedit <vnum> objective add flag <var> <value>");
                return;
            }
            let err = add_quest_objective_flag(vnum, parts[4], parts[5]);
            report(err, "Added flag objective.", connection_id);
            return;
        }
        send_client_message(connection_id, "Unknown objective kind: " + kind);
        return;
    }
    if action == "remove" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: quedit <vnum> objective remove <idx>");
            return;
        }
        let idx = parse_int(parts[3]);
        let err = remove_quest_objective(vnum, idx);
        report(err, "Removed objective " + idx + ".", connection_id);
        return;
    }
    send_client_message(connection_id, "Unknown objective action: " + action);
}

fn handle_reward(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: quedit <vnum> reward add|remove ...");
        return;
    }
    let action = parts[2];
    if action == "add" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: quedit <vnum> reward add gold|item|skill|achievement|recipe|embrace_clan ...");
            return;
        }
        let kind = parts[3];
        if kind == "gold" {
            if parts.len() < 5 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add gold <amount>");
                return;
            }
            let amt = parse_int(parts[4]);
            let err = add_quest_reward_gold(vnum, amt);
            report(err, "Added gold reward.", connection_id);
            return;
        }
        if kind == "item" {
            if parts.len() < 6 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add item <item_vnum> <qty>");
                return;
            }
            let qty = parse_int(parts[5]);
            let err = add_quest_reward_item(vnum, parts[4], qty);
            report(err, "Added item reward.", connection_id);
            return;
        }
        if kind == "skill" {
            if parts.len() < 6 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add skill <skill_key> <amount>");
                return;
            }
            let amt = parse_int(parts[5]);
            let err = add_quest_reward_skill(vnum, parts[4], amt);
            report(err, "Added skill xp reward.", connection_id);
            return;
        }
        if kind == "achievement" {
            if parts.len() < 5 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add achievement <key>");
                return;
            }
            let err = add_quest_reward_achievement(vnum, parts[4]);
            report(err, "Added achievement reward.", connection_id);
            return;
        }
        if kind == "recipe" {
            if parts.len() < 5 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add recipe <recipe_id>");
                return;
            }
            let err = add_quest_reward_recipe(vnum, parts[4]);
            report(err, "Added recipe reward.", connection_id);
            return;
        }
        if kind == "embrace_clan" || kind == "clan" {
            if parts.len() < 5 {
                send_client_message(connection_id, "Usage: quedit <vnum> reward add embrace_clan <brujah|toreador|ventrue|nosferatu|gangrel>");
                return;
            }
            let err = add_quest_reward_embrace_clan(vnum, parts[4]);
            report(err, "Added embrace-clan reward.", connection_id);
            return;
        }
        send_client_message(connection_id, "Unknown reward kind: " + kind);
        return;
    }
    if action == "remove" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: quedit <vnum> reward remove <idx>");
            return;
        }
        let idx = parse_int(parts[3]);
        let err = remove_quest_reward(vnum, idx);
        report(err, "Removed reward " + idx + ".", connection_id);
        return;
    }
    send_client_message(connection_id, "Unknown reward action: " + action);
}

fn show_quest(vnum, connection_id) {
    let quest = get_quest_data(vnum);
    if quest == () {
        send_client_message(connection_id, "No such quest: " + vnum);
        return;
    }
    let out = "Quest \x1b[1;36m" + quest.vnum + "\x1b[0m: " + quest.name + "\n";
    out += "Summary: " + (if quest.summary == "" { "(none)" } else { quest.summary }) + "\n";
    out += "Description:\n";
    out += "  " + (if quest.description == "" { "(none)" } else { quest.description }) + "\n";
    out += "Completion:\n";
    out += "  " + (if quest.completion_text == "" { "(none)" } else { quest.completion_text }) + "\n";
    out += "Giver: " + (if quest.giver_mob_vnum == "" { "(none)" } else { quest.giver_mob_vnum }) + "\n";
    out += "Repeatable: " + (if quest.repeatable { "yes" } else { "no" }) + "\n";
    out += "Prereq: " + (if quest.prereq_quest_vnum == "" { "(none)" } else { quest.prereq_quest_vnum }) + "\n";
    out += "Min skill total: " + (if quest.min_player_skill_total == 0 { "(none)" } else { "" + quest.min_player_skill_total }) + "\n";
    out += "Duration: " + (if quest.duration_secs == 0 { "(none)" } else { "" + quest.duration_secs + " seconds" }) + "\n";
    let kws = quest.keywords;
    let kw_str = "";
    for k in kws {
        if kw_str == "" { kw_str = k; }
        else { kw_str = kw_str + " " + k; }
    }
    out += "Keywords: " + (if kw_str == "" { "(none)" } else { kw_str }) + "\n";

    out += "Objectives:\n";
    let objs = describe_quest_objectives(vnum);
    if objs.len() == 0 {
        out += "  (none)\n";
    } else {
        let i = 0;
        while i < objs.len() {
            out += "  [" + i + "] " + objs[i] + "\n";
            i += 1;
        }
    }

    out += "Rewards:\n";
    let rewards = describe_quest_rewards(vnum);
    if rewards.len() == 0 {
        out += "  (none)\n";
    } else {
        let i = 0;
        while i < rewards.len() {
            out += "  [" + i + "] " + rewards[i] + "\n";
            i += 1;
        }
    }
    send_client_message(connection_id, out);
}

fn send_help(connection_id) {
    let lines = [
        "Quest editor:",
        "  quedit create <vnum> <name...>",
        "  quedit <vnum> [show]",
        "  quedit <vnum> name|summary|desc|completion <text>",
        "  quedit <vnum> giver <mob_vnum>",
        "  quedit <vnum> keywords <kw1 kw2 ...>",
        "  quedit <vnum> repeatable on|off",
        "  quedit <vnum> prereq <quest_vnum|clear>",
        "  quedit <vnum> min_skill <n>     (sum of skill levels; 0 clears)",
        "  quedit <vnum> duration <secs>   (0 clears, no expiry)",
        "  quedit <vnum> objective add kill <mob_vnum> <count>",
        "  quedit <vnum> objective add bring <item_vnum> <qty> [<return_mob>]",
        "  quedit <vnum> objective add visit <room_vnum>",
        "  quedit <vnum> objective add flag <var> <value>",
        "  quedit <vnum> objective remove <idx>",
        "  quedit <vnum> reward add gold|item|skill|achievement|recipe|embrace_clan ...",
        "  quedit <vnum> reward remove <idx>",
        "  quedit <vnum> delete",
    ];
    let s = "";
    for l in lines { s += l + "\n"; }
    send_client_message(connection_id, s);
}

fn join_from(parts, start) {
    let s = "";
    let i = start;
    while i < parts.len() {
        if s == "" { s = parts[i]; }
        else { s = s + " " + parts[i]; }
        i += 1;
    }
    return s;
}

fn report(err, success_msg, connection_id) {
    if err != "" {
        send_client_message(connection_id, "Error: " + err);
    } else {
        send_client_message(connection_id, success_msg);
    }
}

fn capitalize(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1);
    return first.to_upper() + rest;
}
