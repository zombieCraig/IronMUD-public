// Admin command - powerful server administration functions
// Only accessible to admin users

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    if !char.is_admin {
        send_client_message(connection_id, "This command requires admin access.");
        return;
    }

    // Parse subcommand
    let parts = args.split(" ");
    let subcommand = if parts.len() > 0 && parts[0] != "" {
        parts[0].to_lower()
    } else {
        ""
    };

    if subcommand == "" || subcommand == "help" {
        show_help(connection_id);
        return;
    }

    if subcommand == "kick" {
        handle_kick(connection_id, args, char);
        return;
    }

    if subcommand == "settime" {
        handle_settime(connection_id, args);
        return;
    }

    if subcommand == "broadcast" {
        handle_broadcast(connection_id, args);
        return;
    }

    if subcommand == "shutdown" {
        handle_shutdown(connection_id, args, char);
        return;
    }

    if subcommand == "cancel" {
        handle_cancel(connection_id);
        return;
    }

    if subcommand == "god" {
        handle_god(connection_id, char);
        return;
    }

    if subcommand == "summon" {
        handle_summon(connection_id, args, char);
        return;
    }

    if subcommand == "heal" {
        handle_heal(connection_id, args, char);
        return;
    }

    if subcommand == "user" {
        handle_user(connection_id, args, char);
        return;
    }

    if subcommand == "api-key" {
        handle_api_key(connection_id, args);
        return;
    }

    if subcommand == "ban" {
        handle_ban(connection_id, args, char);
        return;
    }

    if subcommand == "unban" {
        handle_unban(connection_id, args, char);
        return;
    }

    if subcommand == "siteban" {
        handle_siteban(connection_id, args, char);
        return;
    }

    if subcommand == "sitebans" {
        handle_sitebans(connection_id);
        return;
    }

    if subcommand == "alts" {
        handle_alts(connection_id, args);
        return;
    }

    if subcommand == "preset" {
        handle_preset(connection_id, args, char);
        return;
    }

    if subcommand == "reload" {
        handle_reload_game_data(connection_id);
        return;
    }

    if subcommand == "email-stats" {
        handle_email_stats(connection_id);
        return;
    }

    if subcommand == "email-audit" {
        handle_email_audit(connection_id, args);
        return;
    }

    if subcommand == "email-test" {
        handle_email_test(connection_id, args, char);
        return;
    }

    if subcommand == "embrace" {
        handle_embrace(connection_id, args, char);
        return;
    }

    if subcommand == "embrace_revoke" || subcommand == "revoke_embrace" {
        handle_embrace_revoke(connection_id, args, char);
        return;
    }

    if subcommand == "masquerade_reset" {
        handle_masquerade_reset(connection_id, args, char);
        return;
    }

    if subcommand == "account" {
        handle_account(connection_id, args, char);
        return;
    }

    if subcommand == "world" {
        handle_world(connection_id, args, char);
        return;
    }

    if subcommand == "config" {
        handle_config(connection_id, args, char);
        return;
    }

    if subcommand == "ticks" {
        handle_ticks(connection_id);
        return;
    }

    if subcommand == "morality" {
        handle_morality(connection_id, args, char);
        return;
    }

    if subcommand == "loadout" {
        handle_loadout(connection_id, args, char);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand. Type 'admin help' for commands.");
}

// `admin loadout` — admin-only editor for class + race starting kits (gold +
// item vnums granted at character creation). Replaces the old builder-facing
// `cedit`; starting equipment is admin policy, not world-building. Class and
// race kits STACK at creation: a new character receives the union of both item
// lists and the sum of both gold values.
// Usage:
//   admin loadout list                              — all classes + races with summary
//   admin loadout <class|race> <id>                 — show one kit
//   admin loadout <class|race> <id> gold <amount>   — set starting gold (>= 0)
//   admin loadout <class|race> <id> items add <vnum>
//   admin loadout <class|race> <id> items remove <vnum>
//   admin loadout <class|race> <id> items clear
fn handle_loadout(connection_id, args, admin_char) {
    let parts = args.split(" ");
    let kind = if parts.len() > 1 && parts[1] != "" {
        parts[1].to_lower()
    } else {
        ""
    };

    if kind == "" || kind == "help" {
        show_loadout_usage(connection_id);
        return;
    }

    if kind == "list" {
        handle_loadout_list(connection_id);
        return;
    }

    if kind == "class" || kind == "race" {
        loadout_target(kind, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown loadout target: " + kind + " (expected class, race, or list)");
}

fn show_loadout_usage(connection_id) {
    send_client_message(connection_id, "Usage:");
    send_client_message(connection_id, "  admin loadout list                              List all class + race kits");
    send_client_message(connection_id, "  admin loadout <class|race> <id>                 Show one kit");
    send_client_message(connection_id, "  admin loadout <class|race> <id> gold <amount>   Set starting gold");
    send_client_message(connection_id, "  admin loadout <class|race> <id> items add <vnum>");
    send_client_message(connection_id, "  admin loadout <class|race> <id> items remove <vnum>");
    send_client_message(connection_id, "  admin loadout <class|race> <id> items clear");
    send_client_message(connection_id, "Class and race kits stack: a new character gets both item lists and the sum of both gold values.");
}

fn handle_loadout_list(connection_id) {
    send_client_message(connection_id, "=== Class Starting Kits ===");
    loadout_list_kind("class", connection_id);
    send_client_message(connection_id, "=== Race Starting Kits ===");
    loadout_list_kind("race", connection_id);
}

fn loadout_list_kind(kind, connection_id) {
    let ids = kit_ids(kind);
    if ids.len() == 0 {
        send_client_message(connection_id, "  (none loaded)");
        return;
    }
    for id in ids {
        let gold = kit_gold(kind, id);
        let items = kit_items(kind, id);
        let item_str = "(none)";
        if items.len() > 0 {
            item_str = join_loadout_items(items);
        }
        send_client_message(connection_id, "  " + id + " — gold=" + gold + ", items=" + item_str);
    }
}

fn join_loadout_items(items) {
    let out = "";
    let first = true;
    for v in items {
        if first {
            out = v;
            first = false;
        } else {
            out += ", " + v;
        }
    }
    return out;
}

fn loadout_target(kind, parts, connection_id) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin loadout " + kind + " <id> [show | gold <n> | items add|remove|clear <vnum>]");
        return;
    }
    let id = parts[2].to_lower();
    if !kit_exists(kind, id) {
        send_client_message(connection_id, "No such " + kind + ": " + id);
        send_client_message(connection_id, "Use 'admin loadout list' to see available entries.");
        return;
    }

    let subcommand = "";
    if parts.len() >= 4 {
        subcommand = parts[3].to_lower();
    }

    if subcommand == "" || subcommand == "show" {
        show_loadout_kit(kind, id, connection_id);
        return;
    }

    if subcommand == "gold" {
        loadout_handle_gold(kind, id, parts, connection_id);
        return;
    }

    if subcommand == "items" {
        loadout_handle_items(kind, id, parts, connection_id);
        return;
    }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
    send_client_message(connection_id, "Use 'admin loadout " + kind + " " + id + "' to see the kit.");
}

fn show_loadout_kit(kind, id, connection_id) {
    let gold = kit_gold(kind, id);
    let items = kit_items(kind, id);
    send_client_message(connection_id, "Starting kit for " + kind + " " + id + ":");
    send_client_message(connection_id, "  gold:  " + gold);
    if items.len() == 0 {
        send_client_message(connection_id, "  items: (none)");
    } else {
        send_client_message(connection_id, "  items:");
        for v in items {
            send_client_message(connection_id, "    - " + v);
        }
    }
}

fn loadout_handle_gold(kind, id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: admin loadout " + kind + " " + id + " gold <amount>");
        return;
    }
    let amount = try_parse_int(parts[4]);
    if amount == () {
        send_client_message(connection_id, "Amount must be a non-negative integer.");
        return;
    }
    if amount < 0 {
        send_client_message(connection_id, "Amount must be non-negative.");
        return;
    }
    if kit_set_gold(kind, id, amount) {
        send_client_message(connection_id, kind + " " + id + " starting gold set to " + amount + ".");
    } else {
        send_client_message(connection_id, "Failed to set starting gold.");
    }
}

fn loadout_handle_items(kind, id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: admin loadout " + kind + " " + id + " items <add|remove|clear> [vnum]");
        return;
    }
    let action = parts[4].to_lower();
    if action == "clear" {
        if kit_clear(kind, id) {
            send_client_message(connection_id, kind + " " + id + " starting items cleared.");
        } else {
            send_client_message(connection_id, "Failed to clear starting items.");
        }
        return;
    }
    if action != "add" && action != "remove" {
        send_client_message(connection_id, "Unknown items action: " + action + " (expected add, remove, or clear)");
        return;
    }
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: admin loadout " + kind + " " + id + " items " + action + " <vnum>");
        return;
    }
    let vnum = parts[5];
    if action == "add" {
        if !item_vnum_exists(vnum) {
            send_client_message(connection_id, "No item prototype with vnum: " + vnum);
            return;
        }
        if kit_add_item(kind, id, vnum) {
            send_client_message(connection_id, "Added " + vnum + " to " + kind + " " + id + " starting kit.");
        } else {
            send_client_message(connection_id, "Failed to add item (already in kit?).");
        }
        return;
    }
    // remove
    if kit_remove_item(kind, id, vnum) {
        send_client_message(connection_id, "Removed " + vnum + " from " + kind + " " + id + " starting kit.");
    } else {
        send_client_message(connection_id, vnum + " is not in " + kind + " " + id + " starting kit.");
    }
}

// kit_* helpers dispatch each kit operation to the class or race Rhai binding
// based on `kind`. Keeps loadout_target/handlers kind-agnostic and each fn
// well under Rhai's branch-complexity limit.
fn kit_exists(kind, id) {
    if kind == "class" { return class_exists(id); }
    return race_exists(id);
}

fn kit_ids(kind) {
    if kind == "class" { return list_class_ids(); }
    return list_race_ids();
}

fn kit_gold(kind, id) {
    if kind == "class" { return get_class_starting_gold(id); }
    return get_race_starting_gold(id);
}

fn kit_items(kind, id) {
    if kind == "class" { return get_class_starting_items(id); }
    return get_race_starting_items(id);
}

fn kit_set_gold(kind, id, amount) {
    if kind == "class" { return set_class_starting_gold(id, amount); }
    return set_race_starting_gold(id, amount);
}

fn kit_add_item(kind, id, vnum) {
    if kind == "class" { return add_class_starting_item(id, vnum); }
    return add_race_starting_item(id, vnum);
}

fn kit_remove_item(kind, id, vnum) {
    if kind == "class" { return remove_class_starting_item(id, vnum); }
    return remove_race_starting_item(id, vnum);
}

fn kit_clear(kind, id) {
    if kind == "class" { return clear_class_starting_items(id); }
    return clear_race_starting_items(id);
}

// `admin morality <player> <delta-or-set>` — adjust a player's morality slider.
// Usage:
//   admin morality <player> +25      (delta)
//   admin morality <player> -50      (delta)
//   admin morality <player> set 100  (absolute)
//   admin morality <player>          (show current value + tier)
// Clamps to [-200, 200].
fn handle_morality(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin morality <player> [+/-<delta> | set <value>]");
        return;
    }
    let target_name = parts[1];
    let target_data = get_character_data(target_name);
    if target_data == () {
        send_client_message(connection_id, "No character named '" + target_name + "' found.");
        return;
    }

    if parts.len() < 3 || parts[2] == "" {
        let tier = get_morality_tier(target_data.name);
        send_client_message(connection_id, target_data.name + " morality: " + target_data.morality + " (" + tier + ")");
        return;
    }

    let action = parts[2].to_lower();
    if action == "set" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: admin morality " + target_name + " set <value>");
            return;
        }
        let v = try_parse_int(parts[3]);
        if v == () {
            send_client_message(connection_id, "Value must be an integer.");
            return;
        }
        set_morality(target_data.name, v);
        let tier = get_morality_tier(target_data.name);
        send_client_message(connection_id, "Set " + target_data.name + " morality to " + v + " (" + tier + ").");
        return;
    }

    // Treat third token as a signed delta.
    let delta = try_parse_int(action);
    if delta == () {
        send_client_message(connection_id, "Delta must be an integer (e.g. +25, -50) or 'set <value>'.");
        return;
    }
    let new_val = adjust_morality(target_data.name, delta);
    let tier = get_morality_tier(target_data.name);
    send_client_message(connection_id, "Adjusted " + target_data.name + " morality by " + delta + " -> " + new_val + " (" + tier + ").");
}

// `admin ticks` — show liveness for every background tick. Healthy
// (recently-beaten) ticks are listed in green; stale ones in red.
// Surfaces silent task deaths that wouldn't show up in logs at INFO.
fn handle_ticks(connection_id) {
    let entries = get_tick_heartbeats();
    if entries.len() == 0 {
        send_client_message(connection_id, "No ticks registered.");
        return;
    }
    let output = "Tick heartbeats:\n";
    let stale_count = 0;
    for e in entries {
        let status_tag = if e.stale {
            stale_count += 1;
            "\x1b[1;31mSTALE\x1b[0m"
        } else {
            "\x1b[1;32mOK   \x1b[0m"
        };
        output += "  " + status_tag + "  ";
        output += pad_right(e.name, 22);
        output += " " + pad_right(e.age_secs + "s ago", 12);
        output += " (interval " + e.expected_secs + "s)\n";
    }
    if stale_count > 0 {
        output += "\n\x1b[1;31m" + stale_count + " stale tick(s) — those tokio tasks may have died and need a server restart.\x1b[0m\n";
    }
    send_client_message(connection_id, output);
}

fn handle_embrace(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin embrace <player> [<clan>]");
        send_client_message(connection_id, "Clans: brujah, toreador, ventrue, nosferatu, gangrel (omit for thinblood/no clan).");
        return;
    }
    let target_name = parts[1];
    let clan = if parts.len() >= 3 { parts[2].to_lower() } else { "" };

    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    if is_pc_vampire(target_conn) {
        send_client_message(connection_id, target_name + " is already kindred.");
        return;
    }

    let sire = admin_char.name;
    if !embrace_pc(target_conn, sire, clan) {
        send_client_message(connection_id, "Failed to embrace " + target_name + ".");
        return;
    }

    let clan_label = if clan == "" { "(thinblood)" } else { "as a " + clan };
    send_client_message(connection_id, "You embrace " + target_name + " " + clan_label + ".");
    send_client_message(target_conn, "\x1b[1;31m" + sire + " sinks fangs into your throat. Cold floods your veins. You wake changed.\x1b[0m");
    if clan != "" {
        send_client_message(target_conn, "You feel the call of clan " + clan + ".");
    }
}

fn handle_embrace_revoke(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin embrace_revoke <player>");
        return;
    }
    let target_name = parts[1];

    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    if !is_pc_vampire(target_conn) {
        send_client_message(connection_id, target_name + " is not kindred.");
        return;
    }

    if !revoke_pc_vampirism(target_conn) {
        send_client_message(connection_id, "Failed to revoke vampirism on " + target_name + ".");
        return;
    }

    send_client_message(connection_id, "Restored mortality to " + target_name + ".");
    send_client_message(target_conn, "Mortality returns. The hunger fades.");
}

fn handle_masquerade_reset(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin masquerade_reset <player>");
        return;
    }
    let target_name = parts[1];

    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    if masquerade_reset_pc(target_conn) {
        send_client_message(connection_id, "Cleared masquerade_broken on " + target_name + ".");
        send_client_message(target_conn, "The whispers fade. The masquerade holds again.");
    } else {
        send_client_message(connection_id, target_name + " is not a vampire or their masquerade was already intact.");
    }
}

// `admin config <key> [<value>]` — runtime-tunable server settings.
// Reading without a value shows the current setting; writing accepts
// on/off / true/false / a literal string. Set <value> to '' to delete.
fn handle_config(connection_id, args, admin_char) {
    let parts = args.split(" ");
    
    let allowed_keys = [
        "enable_vampire_creation", "enable_werewolf_creation", "builder_mode", "motd", "recall_enabled",
        "login_lockout_duration", "idle_timeout_secs", "idle_disconnect_enabled", "starting_room_id",
        "stamina_regen_standing", "stamina_regen_sitting", "stamina_regen_sleeping",
        "hp_regen_sitting", "hp_regen_sleeping",
        "mana_regen_standing", "mana_regen_sitting", "mana_regen_sleeping",
        "stamina_cost_move", "stamina_cost_move_difficult", "stamina_cost_attack",
        "stamina_cost_recall", "stamina_cost_flee",
        "thirst_base_rate", "hunger_base_rate",
        "player_corpse_decay_secs", "mobile_corpse_decay_secs",
        "rent_period_game_days", "escrow_expiry_real_days",
        "mail_stamp_price", "mail_max_messages", "mail_level_requirement",
        "wander_chance_percent", "min_attackable_age",
        "conception_chance_per_day", "adoption_chance_per_day",
        "email_verification_required", "email_daily_cap", "email_monthly_cap",
        "email_verification_code_ttl_secs",
        // SMTP delivery config. Listed here so it's settable live (the email
        // module re-reads these on every send — no restart needed). smtp_user
        // and smtp_pass are intentionally omitted: they stay offline-only via
        // `ironmud-admin settings set` so credentials aren't typed over the
        // wire or shown in `admin config list`. If a password somehow is set,
        // mask_secret() below keeps it out of the display/broadcast.
        "smtp_host", "smtp_port", "smtp_from_address", "smtp_from_name",
        "email_verification_subject", "password_reset_subject", "email_test_subject"
    ];

    if parts.len() < 2 || parts[1] == "" || parts[1] == "list" {
        let output = "Configurable settings:\n";
        let configured = list_all_settings();
        
        for k in allowed_keys {
            let val = "";
            let found = false;
            for entry in configured {
                if entry.key == k {
                    val = mask_secret(k, entry.value);
                    found = true;
                    break;
                }
            }
            if !found {
                let def = get_setting_default(k);
                val = if def == "" { "(not set)" } else { "(not set) [" + def + "]" };
            }
            output += "  " + pad_right(k, 35) + " " + val + "\n";
        }
        
        // Show any extra configured settings not in the allowed list
        let extra_shown = false;
        for entry in configured {
            let is_allowed = false;
            for k in allowed_keys { if k == entry.key { is_allowed = true; break; } }
            if !is_allowed {
                if !extra_shown {
                    output += "\nExtra configured settings:\n";
                    extra_shown = true;
                }
                output += "  " + pad_right(entry.key, 35) + " " + mask_secret(entry.key, entry.value) + "\n";
            }
        }
        
        send_client_message(connection_id, output);
        send_client_message(connection_id, "\nUsage: admin config <key> [<value>]   — Set <value> to '' to delete");
        return;
    }
    
    let key = parts[1].to_lower();

    if parts.len() < 3 {
        let current = get_setting(key);
        if current == "" {
            let def = get_setting_default(key);
            if def == "" {
                send_client_message(connection_id, key + " is not set.");
            } else {
                send_client_message(connection_id, key + " is not set (default: " + def + ").");
            }
        } else {
            send_client_message(connection_id, key + " is currently: " + mask_secret(key, current));
        }
        return;
    }

    // Build value from remaining parts
    let value_parts = [];
    for i in 2..parts.len() {
        value_parts.push(parts[i]);
    }
    let value_in = value_parts.join(" ");
    
    if value_in == "''" || value_in == "\"\"" {
        if delete_setting(key) {
            send_client_message(connection_id, "Deleted setting " + key + ".");
            broadcast_to_builders("[admin] " + admin_char.name + " deleted setting " + key);
        } else {
            send_client_message(connection_id, "Setting " + key + " was not set.");
        }
        return;
    }

    // Only coerce explicit boolean words. `0` / `1` pass through as-is so
    // numeric settings (mail_level_requirement, regen rates, caps, ...) can
    // be set to a literal integer without being clobbered to false/true.
    let value = if value_in.to_lower() == "on" || value_in.to_lower() == "true" || value_in.to_lower() == "enable" {
        "true"
    } else if value_in.to_lower() == "off" || value_in.to_lower() == "false" || value_in.to_lower() == "disable" {
        "false"
    } else {
        value_in
    };

    if set_setting(key, value) {
        let shown = mask_secret(key, value);
        send_client_message(connection_id, "Set " + key + " = " + shown + ".");
        broadcast_to_builders("[admin] " + admin_char.name + " set " + key + " = " + shown);
    } else {
        send_client_message(connection_id, "Failed to write setting.");
    }
}

// Mask secret setting values (e.g. smtp_pass) so they never appear in the
// `admin config` list, single-key display, or the builder broadcast. Empty
// values pass through so "(not set)" / deletion still read correctly.
fn mask_secret(key, value) {
    let lk = key.to_lower();
    if lk == "smtp_pass" || lk == "smtp_password" {
        if value == "" { return value; }
        return "********";
    }
    value
}

fn show_help(connection_id) {
    let help = "Admin Commands:\n";
    help += "  admin kick <player> [reason]      - Forcibly disconnect a player\n";
    help += "  admin summon <player>             - Teleport a player to your location\n";
    help += "  admin heal <player>               - Fully heal a player (HP, wounds, conditions)\n";
    help += "  admin settime <hour> <day> <month> <year> - Set the game time\n";
    help += "  admin broadcast <message>         - Send announcement to all players\n";
    help += "  admin shutdown <seconds> [reason] - Shutdown server with countdown\n";
    help += "  admin cancel                      - Abort a pending shutdown\n";
    help += "  admin god                         - Toggle god mode (invulnerability)\n";
    help += "  admin user ...                    - User management (type 'admin user' for help)\n";
    help += "  admin api-key ...                 - API key management (type 'admin api-key' for help)\n";
    help += "  admin ban <account> [duration] [reason]   - Suspend an account (perm/1d/7d/30d/<n>{h|d})\n";
    help += "  admin unban <account>             - Lift an account suspension\n";
    help += "  admin siteban <ip> [duration] [reason]    - Refuse connections from an IP\n";
    help += "  admin sitebans                    - List active site bans\n";
    help += "  admin alts <account>              - Show accounts sharing IP / normalized email\n";
    help += "  admin preset <name>               - Switch world preset (fantasy/modern/...) and hot-reload definitions\n";
    help += "  admin reload                      - Reload class/race/spell/language definitions from disk\n";
    help += "  admin email-stats                 - Show email send counters vs daily/monthly caps\n";
    help += "  admin email-audit [N]             - Show last N email send attempts (default 20)\n";
    help += "  admin email-test <address>        - Send a test email to confirm SMTP delivery works\n";
    help += "  admin embrace <player> [<clan>]   - Embrace a PC as kindred (clans: brujah/toreador/ventrue/nosferatu/gangrel)\n";
    help += "  admin embrace_revoke <player>     - Restore mortality to a vampire PC\n";
    help += "  admin masquerade_reset <player>   - Clear masquerade_broken on a vampire PC\n";
    help += "  admin config [<key>] [<value>]    - Read, write, or list runtime settings\n";
    help += "  admin ticks                       - Show liveness of every background tick task\n";
    help += "  admin morality <player> [<delta>|set <v>] - Show or adjust a player's morality slider (-200..+200)\n";
    help += "  admin account ...                 - Account management (type 'admin account' for help)\n";
    help += "  admin world ...                   - World statistics and management (type 'admin world' for help)\n";
    help += "  admin loadout ...                 - Edit class/race starting kits (type 'admin loadout' for help)\n";
    help += "  admin help                        - Show this help\n";
    send_client_message(connection_id, help);
}

fn handle_account(connection_id, args, admin_char) {
    let parts = args.split(" ");
    let action = if parts.len() > 1 && parts[1] != "" {
        parts[1].to_lower()
    } else {
        ""
    };

    if action == "" || action == "help" {
        let help = "Account Management Commands:\n";
        help += "  admin account list                    - List all accounts\n";
        help += "  admin account info <name>             - Show detailed account info\n";
        help += "  admin account ban <name> [dur] [msg]  - Suspend an account\n";
        help += "  admin account unban <name>            - Lift account suspension\n";
        help += "  admin account alts <name>             - Show possible alts\n";
        help += "  admin account verify <name>           - Mark email as verified\n";
        help += "  admin account unverify <name>         - Mark email as unverified\n";
        help += "  admin account set-email <name> <addr> - Set account email\n";
        help += "  admin account send-code <name>        - Send fresh verification code\n";
        help += "  admin account adjust-bank <name> <n>  - Adjust shared bank gold\n";
        help += "  admin account delete <name>           - Delete account and characters\n";
        send_client_message(connection_id, help);
        return;
    }

    if action == "list" {
        let accounts = list_all_accounts();
        if accounts.len() == 0 {
            send_client_message(connection_id, "No accounts found.");
            return;
        }
        let output = "Name                     Chars    Banned   Characters\n";
        output += "----------------------------------------------------------------------\n";
        for a in accounts {
            let name = pad_right(a.name, 24);
            let chars_count = pad_right(a.character_names.len().to_string(), 8);
            let banned = pad_right(if a.is_banned { "Yes" } else { "No" }, 8);
            let char_list = a.character_names.join(", ");
            output += name + " " + chars_count + " " + banned + " " + char_list + "\n";
        }
        output += "\nTotal: " + accounts.len() + " account(s)";
        send_client_message(connection_id, output);
        return;
    }

    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin account " + action + " <account_name> [...]");
        return;
    }

    let target_name = parts[2];

    if action == "ban" {
        handle_ban(connection_id, "ban " + args.sub_string(args.find("ban") + 4), admin_char);
        return;
    }

    if action == "unban" {
        handle_unban(connection_id, "unban " + args.sub_string(args.find("unban") + 6), admin_char);
        return;
    }

    if action == "alts" {
        handle_alts(connection_id, "alts " + args.sub_string(args.find("alts") + 5));
        return;
    }

    let account = get_account_by_name(target_name);
    if account == () {
        send_client_message(connection_id, "Account '" + target_name + "' not found.");
        return;
    }

    if action == "info" {
        let output = "Account Details: " + account.name + "\n";
        output += "----------------------------------------\n";
        output += "ID:             " + account.id + "\n";
        output += "Banned:         " + account.is_banned + "\n";
        if account.contains("ban_record") {
            let br = account.ban_record;
            output += "  Reason:       " + br.reason + "\n";
            output += "  By:           " + br.banned_by + "\n";
            output += "  At:           " + format_unix_time(br.banned_at) + "\n";
            if br.expires_at != () {
                output += "  Expires at:   " + format_unix_time(br.expires_at) + "\n";
            } else {
                output += "  Expires at:   permanent\n";
            }
        }
        output += "Email:          " + (if account.email == "" { "(none)" } else { account.email }) + "\n";
        output += "Normalized:     " + (if account.normalized_email == "" { "(none)" } else { account.normalized_email }) + "\n";
        output += "Email Verified: " + account.email_verified + "\n";
        output += "Pending Code:   " + account.pending_code + "\n";
        output += "Last Login IP:  " + (if account.last_login_ip == "" { "(none)" } else { account.last_login_ip }) + "\n";
        output += "Creation IP:    " + (if account.creation_ip == "" { "(none)" } else { account.creation_ip }) + "\n";
        output += "Created At:     " + format_unix_time(account.created_at) + "\n";
        output += "Last Login At:  " + (if account.last_login_at == 0 { "Never" } else { format_unix_time(account.last_login_at) }) + "\n";
        output += "Shared Bank:    " + account.shared_bank_gold + " gold\n";
        output += "Characters:     " + account.character_names.join(", ") + "\n";

        // Per-character play time. Skips silently on characters that fail to
        // load (renamed, mid-creation, etc.) so the rest of the block still
        // renders.
        let any_play_time = false;
        let play_lines = "";
        for cname in account.character_names {
            let c = get_character_data(cname);
            if c != () {
                let secs = c.total_seconds_played;
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                let pretty = hours + "h " + mins + "m";
                play_lines += "  " + pad_right(c.name, 20) + " " + pretty + "\n";
                any_play_time = true;
            }
        }
        if any_play_time {
            output += "Play Time:\n" + play_lines;
        }

        let d = account.character_defaults;
        if d.is_set {
            output += "Character Defaults:\n";
            output += "  prompt=" + (if d.prompt_mode == "" { "default" } else { d.prompt_mode }) + " colors=" + d.colors_enabled + " mxp=" + d.mxp_enabled + "\n";
            output += "  abbrev=" + d.abbrev_enabled + " helpline=" + d.helpline_enabled + " summonable=" + d.summonable + "\n";
            output += "  automap=" + d.automap_enabled + " radius=" + d.automap_radius + " ascii=" + d.ascii_map + "\n";
        }
        
        send_client_message(connection_id, output);
        return;
    }

    if action == "verify" {
        if verify_account(account.id) {
            send_client_message(connection_id, "Account '" + account.name + "' marked as email-verified.");
            broadcast_to_builders("[admin] " + admin_char.name + " verified account " + account.name);
        } else {
            send_client_message(connection_id, "Failed to verify account.");
        }
        return;
    }

    if action == "unverify" {
        if unverify_account(account.id) {
            send_client_message(connection_id, "Account '" + account.name + "' marked as unverified.");
            broadcast_to_builders("[admin] " + admin_char.name + " unverified account " + account.name);
        } else {
            send_client_message(connection_id, "Failed to unverify account.");
        }
        return;
    }

    if action == "set-email" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: admin account set-email <name> <email_address>");
            return;
        }
        let email = parts[3];
        if set_account_email(account.id, email) {
            send_client_message(connection_id, "Account '" + account.name + "' email updated to: " + email);
            broadcast_to_builders("[admin] " + admin_char.name + " set email for " + account.name + " to " + email);
        } else {
            send_client_message(connection_id, "Failed to update email.");
        }
        return;
    }

    if action == "send-code" {
        if account.email == "" {
            send_client_message(connection_id, "Account has no email on file. Use 'admin account set-email' first.");
            return;
        }
        if send_verification_code(account.id, account.email) {
            send_client_message(connection_id, "Verification code sent to " + account.email + " for account '" + account.name + "'.");
        } else {
            send_client_message(connection_id, "Failed to send verification code (check SMTP config).");
        }
        return;
    }

    if action == "adjust-bank" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: admin account adjust-bank <name> <amount>");
            return;
        }
        let amount = try_parse_int(parts[3]);
        if amount == () {
            send_client_message(connection_id, "Invalid amount.");
            return;
        }
        if add_shared_bank_gold(account.id, amount) {
            let updated = get_account_by_id(account.id);
            send_client_message(connection_id, "Adjusted shared bank for '" + account.name + "' by " + amount + ". New balance: " + updated.shared_bank_gold + " gold.");
            broadcast_to_builders("[admin] " + admin_char.name + " adjusted shared bank for " + account.name + " by " + amount);
        } else {
            send_client_message(connection_id, "Failed to adjust bank (insufficient funds for debit?).");
        }
        return;
    }

    if action == "delete" {
        if parts.len() < 5 || parts[3].to_lower() != "confirm" || parts[4].to_lower() != account.name.to_lower() {
            send_client_message(connection_id, "WARNING: You are about to delete account '" + account.name + "' AND all of its characters:");
            for n in account.character_names {
                send_client_message(connection_id, "  - " + n);
            }
            send_client_message(connection_id, "This action is PERMANENT and cannot be undone.");
            send_client_message(connection_id, "To confirm, type: admin account delete " + account.name + " confirm " + account.name);
            return;
        }

        // Kick all characters first
        for n in account.character_names {
            kick_player(n, "Your account has been deleted by an administrator.", admin_char.name);
            delete_character(n);
        }
        
        if delete_account(account.name) {
            send_client_message(connection_id, "Account '" + account.name + "' and all owned characters have been deleted.");
            broadcast_to_builders("[admin] " + admin_char.name + " DELETED account " + account.name);
        } else {
            send_client_message(connection_id, "Failed to delete account.");
        }
        return;
    }

    send_client_message(connection_id, "Unknown account action. Type 'admin account help' for help.");
}

fn handle_world(connection_id, args, admin_char) {
    let parts = args.split(" ");
    let action = if parts.len() > 1 && parts[1] != "" {
        parts[1].to_lower()
    } else {
        ""
    };

    if action == "info" || action == "" {
        let stats = world_stats();
        let output = "World Statistics\n";
        output += "------------------------------\n";
        output += "  Areas:              " + pad_right(stats.areas.to_string(), 6) + "\n";
        output += "  Rooms:              " + pad_right(stats.rooms.to_string(), 6) + "\n";
        output += "  Items:              " + pad_right(stats.items.to_string(), 6) + "\n";
        output += "  Mobiles:            " + pad_right(stats.mobiles.to_string(), 6) + "\n";
        output += "  Spawn Points:       " + pad_right(stats.spawn_points.to_string(), 6) + "\n";
        output += "  Recipes:            " + pad_right(stats.recipes.to_string(), 6) + "\n";
        output += "  Transports:         " + pad_right(stats.transports.to_string(), 6) + "\n";
        output += "  Property Templates: " + pad_right(stats.property_templates.to_string(), 6) + "\n";
        output += "  Leases:             " + pad_right(stats.leases.to_string(), 6) + "\n";
        output += "  Plant Prototypes:   " + pad_right(stats.plant_prototypes.to_string(), 6) + "\n";
        output += "  Plants:             " + pad_right(stats.plants.to_string(), 6) + "\n";
        output += "  Characters:         " + pad_right(stats.characters.to_string(), 6) + "\n";
        send_client_message(connection_id, output);
        return;
    }

    if action == "clear" {
        if parts.len() < 3 || parts[2] != "CONFIRM" {
            send_client_message(connection_id, "WARNING: This will permanently delete ALL world data.");
            send_client_message(connection_id, "Characters, settings, and API keys will be preserved.");
            send_client_message(connection_id, "To confirm, type: admin world clear CONFIRM");
            return;
        }

        if clear_world_data() {
            send_client_message(connection_id, "World data cleared. Restart the server to re-seed the demo world.");
            broadcast_to_builders("[admin] " + admin_char.name + " CLEARED ALL WORLD DATA");
        } else {
            send_client_message(connection_id, "Failed to clear world data.");
        }
        return;
    }

    send_client_message(connection_id, "Unknown world action. Use 'admin world info' or 'admin world clear'.");
}

fn handle_kick(connection_id, args, admin_char) {
    // Parse: "kick <player> [reason]"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin kick <player> [reason]");
        return;
    }

    let target_name = parts[1];

    // Build reason from remaining args
    let reason = "";
    if parts.len() > 2 {
        let reason_parts = [];
        for i in 2..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }

    if reason == "" {
        reason = "No reason given";
    }

    // Call the kick_player function
    let result = kick_player(target_name, reason, admin_char.name);
    send_client_message(connection_id, result);
}

fn handle_settime(connection_id, args) {
    // Parse: "settime <hour> <day> <month> <year>"
    let parts = args.split(" ");
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: admin settime <hour> <day> <month> <year>");
        send_client_message(connection_id, "  hour:  0-23 (0=midnight, 12=noon)");
        send_client_message(connection_id, "  day:   1-30");
        send_client_message(connection_id, "  month: 1-12 (3-5=spring, 6-8=summer, 9-11=autumn, 12,1,2=winter)");
        send_client_message(connection_id, "  year:  1+");
        return;
    }

    // Parse numeric values
    let hour = try_parse_int(parts[1]);
    let day = try_parse_int(parts[2]);
    let month = try_parse_int(parts[3]);
    let year = try_parse_int(parts[4]);

    if hour == () || day == () || month == () || year == () {
        send_client_message(connection_id, "Error: All values must be valid integers.");
        return;
    }

    // Call admin_set_time which validates, sets time, and broadcasts messages
    let result = admin_set_time(hour, day, month, year);

    if !result.success {
        let error = result.get("error");
        if error == () {
            error = "Unknown error";
        }
        send_client_message(connection_id, "Error: " + error);
        return;
    }

    // Report what was set
    let gt = get_game_time();
    let msg = "Game time set to: ";
    msg += "Hour " + gt.hour + ", ";
    msg += "Day " + gt.day + ", ";
    msg += "Month " + gt.month + " (" + gt.season + "), ";
    msg += "Year " + gt.year;
    msg += " [" + gt.time_of_day + "]";
    send_client_message(connection_id, msg);

    // Fire triggers based on what changed
    let triggers_fired = 0;

    if result.time_changed {
        // Build time change context
        let ctx = #{
            old_time: result.old_time_of_day,
            new_time: result.new_time_of_day,
            is_dawn: (result.new_time_of_day == "Dawn").to_string(),
            is_dusk: (result.new_time_of_day == "Dusk").to_string(),
            is_night: (result.new_time_of_day == "Night").to_string(),
            is_day: (gt.is_daytime).to_string()
        };
        if fire_time_triggers("time_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Time of day changed: " + result.old_time_of_day + " -> " + result.new_time_of_day);
    }

    if result.season_changed {
        // Build season change context
        let ctx = #{
            old_season: result.old_season,
            new_season: result.new_season,
            is_spring: (result.new_season == "spring").to_string(),
            is_summer: (result.new_season == "summer").to_string(),
            is_autumn: (result.new_season == "autumn").to_string(),
            is_winter: (result.new_season == "winter").to_string()
        };
        if fire_time_triggers("season_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Season changed: " + result.old_season + " -> " + result.new_season);
    }

    if result.month_changed {
        // Build month change context
        let is_festival = (month == 4 || month == 8 || month == 12);
        let ctx = #{
            old_month: result.old_month.to_string(),
            new_month: result.new_month.to_string(),
            new_day: day.to_string(),
            new_year: year.to_string(),
            is_festival: is_festival.to_string()
        };
        if fire_time_triggers("month_change", ctx) {
            triggers_fired += 1;
        }
        send_client_message(connection_id, "Month changed: " + result.old_month + " -> " + result.new_month);
    }

    if triggers_fired > 0 {
        send_client_message(connection_id, "Fired " + triggers_fired + " trigger type(s).");
    }
}

fn handle_broadcast(connection_id, args) {
    // Parse: "broadcast <message>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin broadcast <message>");
        return;
    }

    // Build message from all parts after "broadcast"
    let message_parts = [];
    for i in 1..parts.len() {
        message_parts.push(parts[i]);
    }
    let message = message_parts.join(" ");

    // Format the announcement
    let announcement = "\n*** SERVER ANNOUNCEMENT ***\n" + message + "\n***************************\n";

    // Broadcast to all players
    broadcast_to_all_players(announcement);

    send_client_message(connection_id, "Broadcast sent to all players.");
}

fn handle_shutdown(connection_id, args, admin_char) {
    // Parse: "shutdown <seconds> [reason]"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin shutdown <seconds> [reason]");
        send_client_message(connection_id, "  seconds: Countdown time before shutdown (0 for immediate)");
        send_client_message(connection_id, "  reason:  Optional reason for shutdown");
        return;
    }

    let delay = try_parse_int(parts[1]);
    if delay == () {
        send_client_message(connection_id, "Error: Delay must be a valid integer.");
        return;
    }

    if delay < 0 {
        send_client_message(connection_id, "Error: Delay cannot be negative.");
        return;
    }

    // Build reason from remaining args
    let reason = "Server maintenance";
    if parts.len() > 2 {
        let reason_parts = [];
        for i in 2..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }

    // Schedule the shutdown
    if schedule_shutdown(delay, reason, admin_char.name) {
        if delay == 0 {
            send_client_message(connection_id, "Shutdown initiated immediately.");
        } else {
            send_client_message(connection_id, "Shutdown scheduled in " + delay + " seconds.");
        }
    } else {
        send_client_message(connection_id, "Error: Failed to schedule shutdown.");
    }
}

fn handle_cancel(connection_id) {
    let result = cancel_shutdown();

    if result == "cancelled" {
        send_client_message(connection_id, "Shutdown cancelled.");
    } else {
        send_client_message(connection_id, "No shutdown is currently pending.");
    }
}

fn handle_god(connection_id, char) {
    let new_state = !char.god_mode;
    set_god_mode(connection_id, new_state);

    let room_id = char.current_room_id;

    if new_state {
        send_client_message(connection_id, "God mode ENABLED. You are now invulnerable.");
        broadcast_to_room_except(room_id, connection_id,
            char.name + " begins to emit a divine glow.");
    } else {
        send_client_message(connection_id, "God mode DISABLED. You are mortal once more.");
        broadcast_to_room_except(room_id, connection_id,
            "The glow around " + char.name + " fades away.");
    }
}

fn handle_summon(connection_id, args, admin_char) {
    // Parse: "summon <player>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin summon <player>");
        return;
    }

    let target_name = parts[1];

    // Can't summon yourself
    if target_name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot summon yourself.");
        return;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Could not find character data for '" + target_name + "'.");
        return;
    }

    // Check if already in same room
    if target_char.current_room_id == admin_char.current_room_id {
        send_client_message(connection_id, target_char.name + " is already here.");
        return;
    }

    let old_room_id = target_char.current_room_id;
    let new_room_id = admin_char.current_room_id;

    // Clear combat state if in combat
    if is_in_combat(target_char.name) {
        // Clear player's combat state
        exit_combat(target_char.name);

        // Clear any mobiles targeting this player in the old room
        let mobiles = get_mobiles_in_room(old_room_id);
        for mobile in mobiles {
            exit_mobile_combat(mobile.id);
        }

        send_client_message(target_conn, "Your combat has ended due to being summoned.");
    }

    // Announce departure from old room
    broadcast_to_room(old_room_id, target_char.name + " vanishes in a flash of light!", target_char.name);

    // Move the target character
    target_char.current_room_id = new_room_id;
    save_character_data(target_char);
    set_player_character(target_conn, target_char);

    // Announce arrival
    broadcast_to_room_except(new_room_id, target_conn,
        target_char.name + " appears in a flash of light!");

    // Notify target player
    send_client_message(target_conn, "You have been summoned by " + admin_char.name + "!");

    // Show target the new room
    display_room(new_room_id, target_conn, target_char.name);

    // Confirm to admin
    send_client_message(connection_id, "You have summoned " + target_char.name + " to your location.");
}

fn handle_heal(connection_id, args, admin_char) {
    // Parse: "heal <player>"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin heal <player>");
        return;
    }

    let target_name = parts[1];

    // Allow "self" to heal yourself
    if target_name.to_lower() == "self" {
        target_name = admin_char.name;
    }

    // Find target player's connection
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + target_name + "' is not online.");
        return;
    }

    // Get target's character data
    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Could not find character data for '" + target_name + "'.");
        return;
    }

    // Heal HP and stamina (use effective max to honor APPLY_MAXHIT bonuses).
    target_char.hp = get_effective_max_hp(target_char.name);
    target_char.stamina = target_char.max_stamina;

    // Clear unconscious/bleedout state
    target_char.is_unconscious = false;
    target_char.bleedout_rounds_remaining = 0;

    // Clear all wounds and ongoing effects
    clear_character_wounds(target_char.name);
    clear_ongoing_effects(target_char.name);

    // Clear wetness and exposure
    target_char.is_wet = false;
    target_char.wet_level = 0;
    target_char.cold_exposure = 0;
    target_char.heat_exposure = 0;

    // Clear all conditions
    target_char.has_hypothermia = false;
    target_char.has_frostbite = [];
    target_char.has_heat_exhaustion = false;
    target_char.has_heat_stroke = false;
    target_char.has_illness = false;
    target_char.illness_progress = 0;

    // Save and update session
    save_character_data(target_char);
    set_player_character(target_conn, target_char);

    // Notify target
    send_client_message(target_conn, "You feel a warm divine energy wash over you, healing all ailments!");

    // Confirm to admin
    send_client_message(connection_id, "Fully healed " + target_char.name + ".");
}

// ========== User Management Subcommand ==========

fn handle_user(connection_id, args, admin_char) {
    // Parse: "user <action> [name] [...]"
    let parts = args.split(" ");

    // parts[0] = "user", parts[1] = action, parts[2] = name, etc.
    let action = if parts.len() > 1 && parts[1] != "" {
        parts[1].to_lower()
    } else {
        ""
    };

    if action == "" || action == "help" {
        show_user_help(connection_id);
        return;
    }

    if action == "list" {
        handle_user_list(connection_id);
        return;
    }

    if action == "info" {
        handle_user_info(connection_id, parts);
        return;
    }

    if action == "grant-admin" {
        handle_user_grant_admin(connection_id, parts, admin_char);
        return;
    }

    if action == "revoke-admin" {
        handle_user_revoke_admin(connection_id, parts, admin_char);
        return;
    }

    if action == "grant-builder" {
        handle_user_grant_builder(connection_id, parts);
        return;
    }

    if action == "revoke-builder" {
        handle_user_revoke_builder(connection_id, parts);
        return;
    }

    if action == "password" {
        handle_user_password(connection_id, parts);
        return;
    }

    if action == "delete" {
        handle_user_delete(connection_id, parts, admin_char);
        return;
    }

    send_client_message(connection_id, "Unknown user action. Type 'admin user' for help.");
}

fn show_user_help(connection_id) {
    let help = "User Management Commands:\n";
    help += "  admin user list                   - List all characters with status\n";
    help += "  admin user info <name>            - Show detailed character info\n";
    help += "  admin user grant-admin <name>     - Grant admin privileges\n";
    help += "  admin user revoke-admin <name>    - Revoke admin privileges\n";
    help += "  admin user grant-builder <name>   - Grant builder privileges\n";
    help += "  admin user revoke-builder <name>  - Revoke builder privileges\n";
    help += "  admin user password <name>        - Require password change on next login\n";
    help += "  admin user delete <name>          - Delete a character (requires confirmation)\n";
    send_client_message(connection_id, help);
}

fn handle_user_list(connection_id) {
    let characters = list_all_characters();

    if characters.len() == 0 {
        send_client_message(connection_id, "No characters found.");
        return;
    }

    // Build header
    let output = "Name                 Admin      Builder    PwdChg     Online\n";
    output += "--------------------------------------------------------------\n";

    // Build each row
    for char in characters {
        // Pad name to 20 chars
        let name = char.name;
        while name.len() < 20 {
            name += " ";
        }
        if name.len() > 20 {
            name = name.sub_string(0, 20);
        }

        // Admin column (10 chars)
        let admin_str = if char.is_admin { "Yes" } else { "No" };
        while admin_str.len() < 10 {
            admin_str += " ";
        }

        // Builder column (10 chars)
        let builder_str = if char.is_builder { "Yes" } else { "No" };
        while builder_str.len() < 10 {
            builder_str += " ";
        }

        // Password change column (10 chars)
        let pwd_str = if char.must_change_password { "Yes" } else { "No" };
        while pwd_str.len() < 10 {
            pwd_str += " ";
        }

        // Online column
        let online_conn = find_player_connection_by_name(char.name);
        let online_str = if online_conn != "" { "Yes" } else { "No" };

        output += name + " " + admin_str + " " + builder_str + " " + pwd_str + " " + online_str + "\n";
    }

    output += "Total: " + characters.len() + " character(s)";
    send_client_message(connection_id, output);
}

fn handle_user_info(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user info <name>");
        return;
    }

    let target_name = parts[2];
    let char = get_character_data(target_name);

    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Check if online
    let online_conn = find_player_connection_by_name(char.name);
    let is_online = online_conn != "";

    // Get effective level
    let eff_level = get_effective_level(char.name);

    let output = "Character Details: " + char.name + "\n";
    output += "----------------------------------------\n";
    output += "Name: " + char.name + "\n";
    output += "Admin: " + (if char.is_admin { "Yes" } else { "No" }) + "\n";
    output += "Builder: " + (if char.is_builder { "Yes" } else { "No" }) + "\n";
    output += "Effective Level: " + eff_level + "\n";
    output += "Gold: " + char.gold + " (Bank: " + char.bank_gold + ")\n";
    output += "Location: " + char.current_room_id + "\n";
    output += "Online: " + (if is_online { "Yes" } else { "No" }) + "\n";
    output += "Password Change Required: " + (if char.must_change_password { "Yes" } else { "No" });

    send_client_message(connection_id, output);
}

fn handle_user_grant_admin(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user grant-admin <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-modification
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot modify your own admin status.");
        return;
    }

    if char.is_admin {
        send_client_message(connection_id, char.name + " is already an admin.");
        return;
    }

    if set_admin_flag(connection_id, char.name, true) {
        send_client_message(connection_id, "Granted admin privileges to '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "You have been granted admin privileges by " + admin_char.name + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to grant admin privileges.");
    }
}

fn handle_user_revoke_admin(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user revoke-admin <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-modification
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot modify your own admin status.");
        return;
    }

    if !char.is_admin {
        send_client_message(connection_id, char.name + " is not an admin.");
        return;
    }

    if set_admin_flag(connection_id, char.name, false) {
        send_client_message(connection_id, "Revoked admin privileges from '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "Your admin privileges have been revoked by " + admin_char.name + ".");
        }
    } else {
        send_client_message(connection_id, "Failed to revoke admin privileges.");
    }
}

fn handle_user_grant_builder(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user grant-builder <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if char.is_builder {
        send_client_message(connection_id, char.name + " is already a builder.");
        return;
    }

    if set_builder_flag(connection_id, char.name, true) {
        send_client_message(connection_id, "Granted builder privileges to '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "You have been granted builder privileges.");
        }
    } else {
        send_client_message(connection_id, "Failed to grant builder privileges.");
    }
}

fn handle_user_revoke_builder(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user revoke-builder <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if !char.is_builder {
        send_client_message(connection_id, char.name + " is not a builder.");
        return;
    }

    if set_builder_flag(connection_id, char.name, false) {
        send_client_message(connection_id, "Revoked builder privileges from '" + char.name + "'.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "Your builder privileges have been revoked.");
        }
    } else {
        send_client_message(connection_id, "Failed to revoke builder privileges.");
    }
}

fn handle_user_password(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user password <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    if char.must_change_password {
        send_client_message(connection_id, char.name + " already has a password change pending.");
        return;
    }

    // Set the must_change_password flag
    char.must_change_password = true;
    if save_character_data(char) {
        send_client_message(connection_id, "Set password change requirement for '" + char.name + "'.");
        send_client_message(connection_id, "They will be prompted to change their password on next login.");

        // Notify target if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(target_conn, "An administrator has flagged your account for a password change. Please change your password soon.");
        }
    } else {
        send_client_message(connection_id, "Failed to set password change requirement.");
    }
}

fn handle_user_delete(connection_id, parts, admin_char) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin user delete <name>");
        send_client_message(connection_id, "Note: To confirm deletion, type: admin user delete <name> confirm <name>");
        return;
    }

    let target_name = parts[2];

    // Check if character exists
    let char = get_character_data(target_name);
    if char == () {
        send_client_message(connection_id, "Character '" + target_name + "' not found.");
        return;
    }

    // Prevent self-deletion
    if char.name.to_lower() == admin_char.name.to_lower() {
        send_client_message(connection_id, "You cannot delete your own character.");
        return;
    }

    // Check for confirmation
    let has_confirm = parts.len() >= 5 && parts[3].to_lower() == "confirm";
    let confirm_name = if parts.len() >= 5 { parts[4] } else { "" };

    if !has_confirm || confirm_name.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, "WARNING: You are about to delete character '" + char.name + "'.");
        send_client_message(connection_id, "This action is PERMANENT and cannot be undone.");

        // Check if online
        let target_conn = find_player_connection_by_name(char.name);
        if target_conn != "" {
            send_client_message(connection_id, "NOTE: " + char.name + " is currently ONLINE. They will be kicked first.");
        }

        send_client_message(connection_id, "To confirm, type: admin user delete " + char.name + " confirm " + char.name);
        return;
    }

    // Check if online and kick if necessary
    let target_conn = find_player_connection_by_name(char.name);
    if target_conn != "" {
        kick_player(char.name, "Your character has been deleted by an administrator.", admin_char.name);
        send_client_message(connection_id, "Kicked " + char.name + " from the server.");
    }

    // Delete the character
    if delete_character(char.name) {
        send_client_message(connection_id, "Character '" + char.name + "' has been deleted.");
    } else {
        send_client_message(connection_id, "Failed to delete character '" + char.name + "'.");
    }
}

// ========== API Key Management Subcommand ==========

fn handle_api_key(connection_id, args) {
    let parts = args.split(" ");
    let action = get_api_key_action(parts);

    if action == "" || action == "help" {
        show_api_key_help(connection_id);
        return;
    }

    if action == "list" {
        handle_api_key_list(connection_id);
        return;
    }

    if action == "create" {
        handle_api_key_create(connection_id, parts);
        return;
    }

    if action == "show" {
        handle_api_key_show(connection_id, parts);
        return;
    }

    if action == "revoke" {
        handle_api_key_revoke(connection_id, parts);
        return;
    }

    if action == "enable" {
        handle_api_key_enable(connection_id, parts);
        return;
    }

    if action == "delete" {
        handle_api_key_delete(connection_id, parts);
        return;
    }

    send_client_message(connection_id, "Unknown api-key action. Type 'admin api-key' for help.");
}

fn get_api_key_action(parts) {
    // parts[0] = "api-key", parts[1] = action
    if parts.len() > 1 && parts[1] != "" {
        return parts[1].to_lower();
    }
    return "";
}

fn show_api_key_help(connection_id) {
    let help = "API Key Management Commands:\n";
    help += "  admin api-key list                                       - List all API keys\n";
    help += "  admin api-key create --name <n> --character <c> [flags]  - Create a new key\n";
    help += "    Flags: --read  --write  --admin\n";
    help += "  admin api-key show <uuid>                                - Show key details\n";
    help += "  admin api-key revoke <uuid>                              - Disable a key\n";
    help += "  admin api-key enable <uuid>                              - Re-enable a key\n";
    help += "  admin api-key delete <uuid> confirm                     - Permanently delete a key\n";
    send_client_message(connection_id, help);
}

fn handle_api_key_list(connection_id) {
    let keys = list_api_keys();

    if keys.len() == 0 {
        send_client_message(connection_id, "No API keys found.");
        return;
    }

    let output = "ID                                    Name                 Character        Enabled  Perms\n";
    output += "-----------------------------------------------------------------------------------------\n";

    for key in keys {
        let id_str = key.get("id");
        let name_str = pad_right(key.get("name"), 20);
        let char_str = pad_right(key.get("owner_character"), 15);
        let enabled_str = pad_right(if key.get("enabled") { "Yes" } else { "No" }, 8);

        let perms = "";
        if key.get("read") { perms += "r"; } else { perms += "-"; }
        if key.get("write") { perms += "w"; } else { perms += "-"; }
        if key.get("admin") { perms += "a"; } else { perms += "-"; }

        output += id_str + "  " + name_str + " " + char_str + " " + enabled_str + " " + perms + "\n";
    }

    output += "Total: " + keys.len() + " key(s)";
    send_client_message(connection_id, output);
}

fn handle_api_key_create(connection_id, parts) {
    // Parse flags: --name <n> --character <c> --read --write --admin
    let name = "";
    let character = "";
    let read = false;
    let write = false;
    let admin = false;

    let i = 2; // Start after "api-key create"
    while i < parts.len() {
        let flag = parts[i];
        if flag == "--name" {
            i += 1;
            if i < parts.len() { name = parts[i]; }
        }
        if flag == "--character" {
            i += 1;
            if i < parts.len() { character = parts[i]; }
        }
        if flag == "--read" { read = true; }
        if flag == "--write" { write = true; }
        if flag == "--admin" { admin = true; }
        i += 1;
    }

    if name == "" {
        send_client_message(connection_id, "Error: --name is required.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> [--read] [--write] [--admin]");
        return;
    }

    if character == "" {
        send_client_message(connection_id, "Error: --character is required.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> [--read] [--write] [--admin]");
        return;
    }

    if !read && !write && !admin {
        send_client_message(connection_id, "Error: At least one permission flag is required (--read, --write, or --admin).");
        return;
    }

    if admin && !read && !write {
        send_client_message(connection_id, "Error: --admin alone has no effect. It only bypasses area ownership checks for --read or --write operations.");
        send_client_message(connection_id, "Usage: admin api-key create --name <name> --character <char> --read [--write] [--admin]");
        return;
    }

    let result = create_api_key(name, character, read, write, admin);

    if !result.get("success") {
        let error = result.get("error");
        if error == () { error = "Unknown error"; }
        send_client_message(connection_id, "Error creating API key: " + error);
        return;
    }

    let output = "API key created successfully!\n\n";
    output += "Key ID: " + result.get("key_id") + "\n";
    output += "Name: " + name + "\n";
    output += "Character: " + character + "\n";
    output += "Permissions: read=" + read + ", write=" + write + ", admin=" + admin + "\n\n";
    output += "=== IMPORTANT: Save this key now! It cannot be recovered! ===\n\n";
    output += "API Key: " + result.get("raw_key") + "\n";
    send_client_message(connection_id, output);
}

fn handle_api_key_show(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key show <uuid>");
        return;
    }

    let key_id = parts[2];
    let key = get_api_key_info(key_id);

    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    let output = "API Key Details\n";
    output += "----------------------------------------\n";
    output += "ID: " + key.get("id") + "\n";
    output += "Name: " + key.get("name") + "\n";
    output += "Character: " + key.get("owner_character") + "\n";
    output += "Enabled: " + (if key.get("enabled") { "Yes" } else { "No" }) + "\n";
    output += "Permissions:\n";
    output += "  Read: " + (if key.get("read") { "Yes" } else { "No" }) + "\n";
    output += "  Write: " + (if key.get("write") { "Yes" } else { "No" }) + "\n";
    output += "  Admin: " + (if key.get("admin") { "Yes" } else { "No" }) + "\n";
    output += "Created: " + key.get("created_at") + "\n";
    let last_used = key.get("last_used_at");
    if last_used == () {
        output += "Last Used: Never";
    } else {
        output += "Last Used: " + last_used;
    }
    send_client_message(connection_id, output);
}

fn handle_api_key_revoke(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key revoke <uuid>");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    if !key.get("enabled") {
        send_client_message(connection_id, "API key '" + key.get("name") + "' is already revoked.");
        return;
    }

    if revoke_api_key(key_id) {
        send_client_message(connection_id, "Revoked API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to revoke API key.");
    }
}

fn handle_api_key_enable(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key enable <uuid>");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    if key.get("enabled") {
        send_client_message(connection_id, "API key '" + key.get("name") + "' is already enabled.");
        return;
    }

    if enable_api_key(key_id) {
        send_client_message(connection_id, "Enabled API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to enable API key.");
    }
}

fn handle_api_key_delete(connection_id, parts) {
    if parts.len() < 3 || parts[2] == "" {
        send_client_message(connection_id, "Usage: admin api-key delete <uuid> confirm");
        return;
    }

    let key_id = parts[2];

    // Check key exists first
    let key = get_api_key_info(key_id);
    if key == () {
        send_client_message(connection_id, "API key '" + key_id + "' not found.");
        return;
    }

    // Require "confirm" keyword
    let has_confirm = parts.len() >= 4 && parts[3].to_lower() == "confirm";
    if !has_confirm {
        send_client_message(connection_id, "WARNING: This will permanently delete API key '" + key.get("name") + "'.");
        send_client_message(connection_id, "To confirm, type: admin api-key delete " + key_id + " confirm");
        return;
    }

    if delete_api_key_by_id(key_id) {
        send_client_message(connection_id, "Deleted API key '" + key.get("name") + "'.");
    } else {
        send_client_message(connection_id, "Failed to delete API key.");
    }
}

fn pad_right(text, width) {
    let s = "" + text;
    while s.len() < width {
        s += " ";
    }
    if s.len() > width {
        s = s.sub_string(0, width);
    }
    return s;
}

// ===========================================================================
// Ban / unban / siteban / alts (ban-tooling slice)
// ===========================================================================

// Parse a duration token into absolute unix-secs.
//   "perm" or empty   → 0  (permanent — caller treats 0 as None expiry)
//   "1d", "7d", "30d" → secs offset from now()
//   "<n>h" / "<n>d"   → free-form hours / days
// Returns -1 on parse failure so callers can show a usage message.
fn parse_duration_to_expires_at(tok) {
    let s = tok.to_lower();
    if s == "" || s == "perm" || s == "permanent" {
        return 0;
    }
    if s.len() < 2 {
        return -1;
    }
    let suffix = s.sub_string(s.len() - 1, 1);
    let num_str = s.sub_string(0, s.len() - 1);
    let mult = if suffix == "h" {
        3600
    } else if suffix == "d" {
        86400
    } else {
        return -1;
    };
    let n = try_parse_int(num_str);
    if n == () || n <= 0 {
        return -1;
    }
    return now_unix_secs() + (n * mult);
}

fn handle_ban(connection_id, args, admin_char) {
    // Parse: "ban <account> [duration] [reason]"
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id,
            "Usage: admin ban <account> [duration] [reason]\n" +
            "  Duration: perm (default), 1d, 7d, 30d, or <n>{h|d}");
        return;
    }
    let target_name = parts[1];
    let account = get_account_by_name(target_name);
    if account == () {
        send_client_message(connection_id, "No account named '" + target_name + "'.");
        return;
    }

    // Optional duration token at parts[2]; if it doesn't parse as a duration,
    // treat it as the first reason word and default to permanent.
    let expires_at = 0;
    let reason_start_idx = 2;
    if parts.len() > 2 {
        let parsed = parse_duration_to_expires_at(parts[2]);
        if parsed >= 0 {
            expires_at = parsed;
            reason_start_idx = 3;
        }
    }

    let reason = "";
    if parts.len() > reason_start_idx {
        let reason_parts = [];
        for i in reason_start_idx..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }
    if reason == "" {
        reason = "No reason given";
    }

    if !ban_account(account.id, reason, admin_char.name, expires_at) {
        send_client_message(connection_id, "Failed to ban account.");
        return;
    }

    // Kick any character on this account that's currently online. We try
    // every character_name on the roster — kick_player is a no-op when
    // nobody by that name is connected.
    let names = account.character_names;
    for n in names {
        kick_player(n, "Banned: " + reason, admin_char.name);
    }

    let when = if expires_at == 0 { "permanent" } else { "until " + format_unix_time(expires_at) };
    send_client_message(connection_id,
        "Banned account '" + account.name + "' (" + when + "). Reason: " + reason);
}

fn handle_unban(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin unban <account>");
        return;
    }
    let target_name = parts[1];
    let account = get_account_by_name(target_name);
    if account == () {
        send_client_message(connection_id, "No account named '" + target_name + "'.");
        return;
    }
    if !unban_account(account.id) {
        send_client_message(connection_id, "Failed to unban account.");
        return;
    }
    send_client_message(connection_id, "Account '" + account.name + "' is no longer suspended.");
}

fn handle_siteban(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id,
            "Usage: admin siteban <ip|remove <ip>> [duration] [reason]");
        return;
    }
    if parts[1].to_lower() == "remove" {
        if parts.len() < 3 || parts[2] == "" {
            send_client_message(connection_id, "Usage: admin siteban remove <ip>");
            return;
        }
        if remove_site_ban(parts[2]) {
            send_client_message(connection_id, "Site ban on " + parts[2] + " removed.");
        } else {
            send_client_message(connection_id, "No site ban found for " + parts[2] + ".");
        }
        return;
    }
    let ip = parts[1];

    let expires_at = 0;
    let reason_start_idx = 2;
    if parts.len() > 2 {
        let parsed = parse_duration_to_expires_at(parts[2]);
        if parsed >= 0 {
            expires_at = parsed;
            reason_start_idx = 3;
        }
    }
    let reason = "";
    if parts.len() > reason_start_idx {
        let reason_parts = [];
        for i in reason_start_idx..parts.len() {
            reason_parts.push(parts[i]);
        }
        reason = reason_parts.join(" ");
    }
    if reason == "" {
        reason = "No reason given";
    }

    if !add_site_ban(ip, reason, admin_char.name, expires_at) {
        send_client_message(connection_id, "Failed to add site ban (bad IP?).");
        return;
    }
    let kicked = siteban_kick_active(ip, reason, admin_char.name);
    let when = if expires_at == 0 { "permanent" } else { "until " + format_unix_time(expires_at) };
    let msg = "Site ban added for " + ip + " (" + when + "). Reason: " + reason + ".";
    if kicked > 0 {
        msg += " Kicked " + kicked + " active session(s) from that IP.";
    }
    send_client_message(connection_id, msg);
}

fn handle_sitebans(connection_id) {
    let bans = list_site_bans();
    if bans.len() == 0 {
        send_client_message(connection_id, "No active site bans.");
        return;
    }
    let out = "Active site bans (" + bans.len() + "):\n";
    for b in bans {
        let exp = if b.expires_at == () {
            "permanent"
        } else {
            "until " + format_unix_time(b.expires_at)
        };
        out += "  " + b.ip + "  [" + exp + "]  by " + b.banned_by + ": " + b.reason + "\n";
    }
    send_client_message(connection_id, out);
}

fn handle_alts(connection_id, args) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        send_client_message(connection_id, "Usage: admin alts <account>");
        return;
    }
    let target_name = parts[1];
    let account = get_account_by_name(target_name);
    if account == () {
        send_client_message(connection_id, "No account named '" + target_name + "'.");
        return;
    }
    let alts = find_alts_by_account(account.id);
    if alts.len() == 0 {
        send_client_message(connection_id,
            "No alts found for '" + account.name + "' (no shared IP / email).");
        return;
    }
    let out = "Possible alts for '" + account.name + "':\n";
    for a in alts {
        let flag = if a.banned { "[BANNED] " } else { "         " };
        out += "  " + flag + a.name + "  via " + a.match_type + "=" + a.match_value + "\n";
    }
    send_client_message(connection_id, out);
}

fn handle_preset(connection_id, args, admin_char) {
    let parts = args.split(" ");
    if parts.len() < 2 || parts[1] == "" {
        let current = get_setting_or_default("class_preset", "fantasy");
        send_client_message(connection_id,
            "Current world preset: " + current + "\n" +
            "Usage: admin preset <name>   (e.g. fantasy, modern)\n" +
            "Sets class_preset, race_preset, spell_preset, and language_preset to the same value\n" +
            "and hot-reloads the matching JSON files from scripts/data/.");
        return;
    }
    let preset = parts[1].to_lower();
    let result = apply_world_preset(preset);
    if !result.ok {
        let err = result.get("error");
        if err == () { err = "unknown error"; }
        send_client_message(connection_id, "Preset switch failed: " + err);
        return;
    }
    let msg = "World preset switched to '" + result.preset + "'.\n";
    msg += "  classes:   " + result.classes_count + "\n";
    msg += "  races:     " + result.races_count + "\n";
    msg += "  spells:    " + result.spells_count + "\n";
    msg += "  languages: " + result.languages_count + "\n";
    let missing = result.missing;
    if missing.len() > 0 {
        msg += "Missing files (kept previous values for these):\n";
        for m in missing {
            msg += "  - " + m + "\n";
        }
    }
    msg += "Note: existing players keep their current class/race even if it no longer exists in the new preset.";
    send_client_message(connection_id, msg);

    // Audit log via broadcast to builders so other staff see the swap.
    broadcast_to_builders("[admin] " + admin_char.name + " switched world preset to '" + result.preset + "'");
}

fn handle_reload_game_data(connection_id) {
    if reload_game_data() {
        send_client_message(connection_id, "Game data reloaded from current presets.");
    } else {
        send_client_message(connection_id, "Reload failed; check server logs.");
    }
}

fn handle_email_stats(connection_id) {
    let stats = get_email_send_stats();
    let day_count = stats.day_count;
    let day_cap = stats.day_cap;
    let month_count = stats.month_count;
    let month_cap = stats.month_cap;

    let day_pct = if day_cap > 0 { (day_count * 100) / day_cap } else { 0 };
    let month_pct = if month_cap > 0 { (month_count * 100) / month_cap } else { 0 };

    // Config status first — if any of the required pieces are missing, the
    // banner / forgot / verify-on-create flows are all silently disabled.
    let verification_required = stats.verification_required;
    let smtp_host = stats.smtp_host;
    let smtp_from_address = stats.smtp_from_address;
    let smtp_port = stats.smtp_port;
    let smtp_user_set = stats.smtp_user_set;
    let smtp_from_name = stats.smtp_from_name;

    let host_set = smtp_host != "";
    let from_set = smtp_from_address != "";
    let fully_ready = verification_required && host_set && from_set;

    let out = "Email configuration:\n";
    if verification_required {
        out += "  email_verification_required: true  [OK]\n";
    } else {
        out += "  email_verification_required: <unset>  [!] Email features OFF (banner hides create/forgot lines, create.rhai skips verify, forgot.rhai refuses)\n";
    }
    if host_set {
        let port_display = if smtp_port == "" { "587 (default)" } else { smtp_port };
        out += "  smtp_host:                   " + smtp_host + "  (port " + port_display + ")  [OK]\n";
    } else {
        out += "  smtp_host:                   <unset>  [!] Required — sends will fail with config_missing\n";
    }
    if from_set {
        let name_display = if smtp_from_name == "" { "IronMUD (default)" } else { smtp_from_name };
        out += "  smtp_from_address:           " + smtp_from_address + "  (name: " + name_display + ")  [OK]\n";
    } else {
        out += "  smtp_from_address:           <unset>  [!] Required — sends will fail with config_missing\n";
    }
    if smtp_user_set {
        out += "  smtp_user / smtp_pass:       set  [OK]\n";
    } else {
        out += "  smtp_user / smtp_pass:       <unset>  (OK only if relay accepts unauthenticated)\n";
    }
    if fully_ready {
        out += "  Status: READY\n";
    } else {
        out += "  Status: NOT CONFIGURED — fix the [!] rows above\n";
    }
    out += "\n";

    out += "Email send budget:\n";
    out += "  Today:      " + day_count + " / " + day_cap + " (" + day_pct + "%)\n";
    out += "  This month: " + month_count + " / " + month_cap + " (" + month_pct + "%)\n";
    out += "\nOverride caps via settings 'email_daily_cap' and 'email_monthly_cap'.";
    send_client_message(connection_id, out);
}

fn handle_email_audit(connection_id, args) {
    let parts = args.split(" ");
    let limit = 20;
    if parts.len() >= 2 && parts[1] != "" {
        let n = try_parse_int(parts[1]);
        if n != () && n > 0 {
            limit = n;
        }
    }

    let entries = list_email_audit_entries(limit);
    if entries.len() == 0 {
        send_client_message(connection_id, "No email audit entries recorded.");
        return;
    }

    let out = "Recent email send attempts (newest first, " + entries.len() + " entries):\n";
    for entry in entries {
        let acct = if entry.account_name == "" { "-" } else { entry.account_name };
        out += "  " + entry.timestamp + "  " + entry.kind + "  " + acct + "  " + entry.outcome + "\n";
    }
    send_client_message(connection_id, out);
}

// `admin email-test <address>` — send a fixed diagnostic message to <address>
// to confirm SMTP is actually delivering. Counts against the daily/monthly
// budget and records an audit row (kind "test") tagged with the admin's name.
// Reports the real delivery outcome so a misconfiguration is visible.
fn handle_email_test(connection_id, args, admin_char) {
    let parts = args.split(" ");
    let address = if parts.len() >= 2 && parts[1] != "" { parts[1] } else { "" };
    if address == "" {
        send_client_message(connection_id, "Usage: admin email-test <address>");
        return;
    }
    if !is_valid_email_format(address) {
        send_client_message(connection_id, "That doesn't look like an email address: " + address);
        return;
    }
    let outcome = send_test_email(address, admin_char.name);
    if outcome == "sent" {
        send_client_message(connection_id, "Test email to " + address + ": sent. Check the inbox (and spam) to confirm delivery.");
    } else {
        send_client_message(connection_id, "Test email to " + address + " FAILED: " + outcome);
    }
}
