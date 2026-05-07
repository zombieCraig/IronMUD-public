// scripts/commands/achievements.rhai
//
// Player-facing achievement command. Three subcommands:
//   achievements              -> list all visible achievements (locked + unlocked)
//   achievements list [cat]   -> same; optional category filter
//   achievements show <key>   -> details for one achievement
//   achievements title <key|none> -> set or clear active title
//
// Hidden achievements are omitted from listings until unlocked.

fn category_letter(category) {
    if category == "skill" { return "S"; }
    if category == "combat" { return "C"; }
    if category == "crafting" { return "R"; }
    if category == "exploration" { return "E"; }
    if category == "social" { return "O"; }
    if category == "wealth" { return "$"; }
    if category == "builder" { return "B"; }
    return "?";
}

fn render_list(items, filter_category, connection_id) {
    if items.len() == 0 {
        send_client_message(connection_id, "No achievements available on this world.");
        return;
    }

    let unlocked_count = 0;
    for it in items {
        if it.unlocked { unlocked_count += 1; }
    }

    let header = "Achievements (" + unlocked_count + "/" + items.len() + " unlocked)";
    send_client_message(connection_id, color_or(ansi_cyan(header), header, connection_id));
    send_client_message(connection_id, "  [S]kill  [C]ombat  c[R]afting  [E]xploration  s[O]cial  [$]wealth  [B]uilder");

    let any_shown = false;
    for it in items {
        if filter_category != "" && it.category != filter_category {
            continue;
        }
        let mark = "[ ]";
        if it.unlocked {
            mark = color_or(ansi_green("[X]"), "[X]", connection_id);
        }
        let active_marker = "";
        if it.active {
            active_marker = " " + color_or(ansi_yellow("(active title)"), "(active title)", connection_id);
        }
        let cat_letter = "[" + category_letter(it.category) + "]";
        let line = "  " + mark + " " + cat_letter + " " + it.name + active_marker;
        send_client_message(connection_id, line);
        if it.unlocked {
            send_client_message(connection_id, "      " + it.description);
        }
        any_shown = true;
    }
    if !any_shown {
        send_client_message(connection_id, "  (no achievements in that category)");
    }
}

fn render_show(items, key, connection_id) {
    let key_lc = key.to_lower();
    for it in items {
        if it.key == key_lc {
            let title_line = "  Title: " + it.title;
            let header = it.name + (if it.unlocked { " [unlocked]" } else { " [locked]" });
            send_client_message(connection_id, color_or(ansi_cyan(header), header, connection_id));
            send_client_message(connection_id, "  " + it.description);
            send_client_message(connection_id, title_line);
            if it.unlocked {
                send_client_message(connection_id, "  Unlocked at: " + it.unlocked_at);
            }
            return;
        }
    }
    send_client_message(connection_id, "No achievement with key '" + key + "'. Hidden achievements stay hidden until unlocked.");
}

fn run_command(args, connection_id) {
    if !achievements_enabled() {
        send_client_message(connection_id, "Achievements are disabled on this world.");
        return;
    }

    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let parts = args.split(' ');
    let sub = "list";
    if parts.len() > 0 && parts[0] != "" {
        sub = parts[0].to_lower();
    }

    if sub == "list" {
        let category = "";
        if parts.len() > 1 {
            category = parts[1].to_lower();
        }
        let items = list_achievements(char.name);
        render_list(items, category, connection_id);
        return;
    }

    if sub == "show" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: achievements show <key>");
            return;
        }
        let items = list_achievements(char.name);
        render_show(items, parts[1], connection_id);
        return;
    }

    if sub == "title" {
        if parts.len() < 2 {
            let cur = get_active_title(char.name);
            if cur == "" {
                send_client_message(connection_id, "You currently display no title. Use 'achievements title <key>' to set one.");
            } else {
                send_client_message(connection_id, "Active title: " + cur);
            }
            return;
        }
        let target = parts[1].to_lower();
        if target == "none" || target == "clear" || target == "off" {
            if set_active_title(char.name, "") {
                send_client_message(connection_id, "Active title cleared.");
            } else {
                send_client_message(connection_id, "Failed to clear title.");
            }
            return;
        }
        if set_active_title(char.name, target) {
            let cur = get_active_title(char.name);
            send_client_message(connection_id, "Active title: " + cur);
        } else {
            send_client_message(connection_id, "You haven't unlocked an achievement with that key.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: achievements [list [category] | show <key> | title <key|none>]");
}
