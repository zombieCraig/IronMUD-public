// scripts/commands/medit.rhai
// medit <mobile_id> [subcommand] or medit create
// Mobile/NPC Editor for builders

// Import shared OLC helpers
import "olc_helpers" as olc;
import "dg_olc" as dg;

// AI helper: handle short help command
fn handle_ai_short_help(mobile, uuid, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> short help <description prompt>");
        return true;
    }
    let prompt = build_string_from_parts(parts, 3);
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: mobile.name,
        theme: theme
    };
    let request_id = ai_help_write(connection_id, "mob_short", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "mobile", uuid, "short_desc");
    send_client_message(connection_id, "Generating short description... please wait.");
    return true;
}

// AI helper: handle short rephrase command
fn handle_ai_short_rephrase(mobile, uuid, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if mobile.short_desc == "" {
        send_client_message(connection_id, "Mobile has no short description to rephrase.");
        return true;
    }
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: mobile.name,
        theme: theme
    };
    let request_id = ai_rephrase(connection_id, "mob_short", mobile.short_desc, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "mobile", uuid, "short_desc");
    send_client_message(connection_id, "Rephrasing short description... please wait.");
    return true;
}

// AI helper: handle long help command
fn handle_ai_long_help(mobile, uuid, parts, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> long help <description prompt>");
        return true;
    }
    let prompt = build_string_from_parts(parts, 3);
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: mobile.name,
        theme: theme
    };
    let request_id = ai_help_write(connection_id, "mob_long", prompt, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "mobile", uuid, "long_desc");
    send_client_message(connection_id, "Generating long description... please wait.");
    return true;
}

// AI helper: handle long rephrase command
fn handle_ai_long_rephrase(mobile, uuid, connection_id) {
    if !ai_is_enabled() {
        send_client_message(connection_id, "AI assistance is not configured (CLAUDE_API_KEY not set).");
        return true;
    }
    if mobile.long_desc == "" {
        send_client_message(connection_id, "Mobile has no long description to rephrase.");
        return true;
    }
    let theme = olc::get_area_theme_for_player(connection_id);
    let context = #{
        entity_name: mobile.name,
        theme: theme
    };
    let request_id = ai_rephrase(connection_id, "mob_long", mobile.long_desc, context);
    if request_id == "" {
        send_client_message(connection_id, "Failed to send AI request.");
        return true;
    }
    ai_set_target(connection_id, "mobile", uuid, "long_desc");
    send_client_message(connection_id, "Rephrasing long description... please wait.");
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let parts = split_quoted_args(args);

    // Handle create first - medit create <name>
    if parts.len() >= 1 && parts[0].to_lower() == "create" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: medit create <name>\nExample: medit create Town Guard");
            return;
        }

        // Sandbox: new prototype inherits the area of the builder's current
        // room. Refuse if they don't have edit rights on that area. Orphan
        // creation (when standing in an area-less room) stays open.
        let current_room = get_room_data(char.current_room_id);
        let new_area_id = "";
        if current_room != () {
            new_area_id = current_room.area_id;
        }
        if new_area_id != "" {
            if !can_edit_area(new_area_id, char.name) {
                send_client_message(connection_id, "You don't have permission to author mobiles for this area. Move to a room you can edit, or use a builder area.");
                return;
            }
            let quota_err = area_quota_check(new_area_id, "mobiles");
            if quota_err != "" {
                send_client_message(connection_id, quota_err);
                return;
            }
        }

        // Join remaining parts as name
        let name = build_string_from_parts(parts, 1);
        let vnum = generate_unique_mobile_vnum(name);

        let mobile = new_mobile(name);
        save_mobile_data(mobile);  // Save first to persist
        set_mobile_vnum(mobile.id, vnum);
        set_mobile_prototype(mobile.id, true);
        set_mobile_short_desc(mobile.id, name + " is standing here.");
        if new_area_id != "" {
            set_mobile_area_id(mobile.id, new_area_id);
        }

        // Re-fetch to get updated data for display
        let mobile = get_mobile_data(mobile.id);
        send_client_message(connection_id, "Created mobile prototype with vnum: " + vnum + "\n");
        show_mobile_properties(mobile, mobile.id, connection_id);
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: medit <mobile_id|vnum> [subcommand] or medit create <name>");
        return;
    }

    let mobile_id = parts[0];
    let mobile = get_mobile_data(mobile_id);

    // If not found by ID, try by vnum
    if mobile == () {
        mobile = get_mobile_by_vnum(mobile_id);
    }

    // If still not found, try by keyword in current room
    if mobile == () {
        let room_mobiles = get_mobiles_in_room(char.current_room_id);
        mobile = find_mobile_by_keyword(mobile_id, room_mobiles);
    }

    if mobile == () {
        send_client_message(connection_id, "Mobile not found: " + mobile_id);
        return;
    }

    let subcommand = "";
    if parts.len() >= 2 {
        subcommand = parts[1].to_lower();
    }

    // Show properties if no subcommand
    if subcommand == "" || subcommand == "show" {
        show_mobile_properties(mobile, mobile_id, connection_id);
        return;
    }

    // Sandbox: stamped prototypes require can_edit_area on their owner.
    // Orphans (area_id == "") remain editable by any builder. Read-only
    // `show` above is intentionally not gated.
    let mob_area_id = mobile.area_id;
    if mob_area_id != "" {
        if !can_edit_area(mob_area_id, char.name) {
            send_client_message(connection_id, "You don't have permission to edit mobiles in that area.");
            return;
        }
    }

    // Handle subcommands
    handle_subcommand(mobile, mobile_id, subcommand, parts, char, connection_id);
}

// Dispatch group 1: Text properties (name, short, long, keywords)
fn dispatch_text_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) {
    if subcommand == "name" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: medit " + mobile_id + " name <text>");
            return true;
        }
        let name = build_string_from_parts(parts, 2);
        set_mobile_name(uuid, name);
        send_client_message(connection_id, "Name set to: " + name);
        return true;
    }
    if subcommand == "short" {
        if parts.len() >= 3 {
            let subcmd = parts[2].to_lower();
            if subcmd == "help" { handle_ai_short_help(mobile, uuid, parts, connection_id); return true; }
            if subcmd == "rephrase" { handle_ai_short_rephrase(mobile, uuid, connection_id); return true; }
        }
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: medit " + mobile_id + " short <text>\nAI: short help <prompt> | short rephrase");
            return true;
        }
        let desc = build_string_from_parts(parts, 2);
        set_mobile_short_desc(uuid, desc);
        send_client_message(connection_id, "Short description set to: " + desc);
        return true;
    }
    if subcommand == "long" {
        if parts.len() >= 3 {
            let subcmd = parts[2].to_lower();
            if subcmd == "help" { handle_ai_long_help(mobile, uuid, parts, connection_id); return true; }
            if subcmd == "rephrase" { handle_ai_long_rephrase(mobile, uuid, connection_id); return true; }
        }
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: medit " + mobile_id + " long <text>\nAI: long help <prompt> | long rephrase");
            return true;
        }
        let desc = build_string_from_parts(parts, 2);
        set_mobile_long_desc(uuid, desc);
        send_client_message(connection_id, "Long description set.");
        return true;
    }
    if subcommand == "keywords" {
        if parts.len() < 3 {
            send_client_message(connection_id, "Usage: medit " + mobile_id + " keywords <kw1> <kw2> ...");
            return true;
        }
        let kws = collect_parts(parts, 2);
        set_mobile_keywords(uuid, kws);
        send_client_message(connection_id, "Keywords set: " + olc::join_array(kws));
        return true;
    }
    return false;
}

// Dispatch group 2: Combat stats (level, hp, damage, ac, damtype, stat, autostats)
fn dispatch_combat_subcommand(uuid, subcommand, parts, connection_id) {
    if subcommand == "level" { handle_level_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "hp" { handle_hp_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "damage" { handle_damage_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "ac" { handle_ac_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "damtype" { handle_damtype_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "stat" { handle_stat_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "autostats" { handle_autostats_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "gold" { handle_gold_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "perception" { handle_perception_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "world_max" || subcommand == "worldmax" || subcommand == "world_max_count" {
        handle_world_max_subcommand(uuid, parts, connection_id);
        return true;
    }
    if subcommand == "faction" {
        handle_faction_subcommand(uuid, parts, connection_id);
        return true;
    }
    if subcommand == "language" || subcommand == "lang" {
        handle_language_subcommand(uuid, parts, connection_id);
        return true;
    }
    if subcommand == "position" || subcommand == "stance" {
        handle_position_subcommand(uuid, parts, connection_id);
        return true;
    }
    if subcommand == "gender" || subcommand == "sex" {
        handle_gender_subcommand(uuid, parts, connection_id);
        return true;
    }
    return false;
}

// Dispatch group 3: Meta commands (flags, flag, prototype, vnum)
fn dispatch_meta_subcommand(mobile, uuid, subcommand, parts, connection_id) {
    if subcommand == "flags" { show_mobile_flags(mobile, uuid, connection_id); return true; }
    if subcommand == "flag" { handle_flag_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "prototype" { handle_prototype_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "vnum" { handle_vnum_subcommand(uuid, parts, connection_id); return true; }
    return false;
}

// Dispatch group 4: Dialogue commands (dialogue, dialogues, rmdialogue, tree)
fn dispatch_dialogue_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) {
    if subcommand == "dialogue" { handle_dialogue_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "dialogues" { show_mobile_dialogues(mobile, uuid, connection_id); return true; }
    if subcommand == "rmdialogue" { handle_rmdialogue_subcommand(uuid, parts, connection_id); return true; }
    if subcommand == "tree" { handle_tree_subcommand(uuid, mobile_id, parts, connection_id); return true; }
    return false;
}

// `mobile_id` is the UUID used by setter helpers; `display_id` is what
// the user typed (vnum or keyword) and is echoed back in help / usage
// lines so the examples stay re-typable.
fn handle_tree_subcommand(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 3 {
        tree_help(display_id, connection_id);
        return;
    }
    let action = parts[2].to_lower();
    if action == "show" { tree_show(mobile_id, parts, connection_id); return; }
    if action == "nodes" { tree_nodes(mobile_id, connection_id); return; }
    if action == "clear" {
        if clear_mobile_dialogue_tree(mobile_id) {
            send_client_message(connection_id, "Dialogue tree cleared.");
        } else {
            send_client_message(connection_id, "Error clearing dialogue tree.");
        }
        return;
    }
    if action == "set" { tree_set_json(mobile_id, display_id, parts, connection_id); return; }
    if action == "addnode" { tree_addnode(mobile_id, display_id, parts, connection_id); return; }
    if action == "delnode" { tree_delnode(mobile_id, display_id, parts, connection_id); return; }
    if action == "setroot" { tree_setroot(mobile_id, display_id, parts, connection_id); return; }
    if action == "text" { tree_text(mobile_id, display_id, parts, connection_id); return; }
    if action == "edittext" { tree_edittext(mobile_id, display_id, parts, connection_id); return; }
    if action == "addchoice" { tree_addchoice(mobile_id, display_id, parts, connection_id); return; }
    if action == "delchoice" { tree_delchoice(mobile_id, display_id, parts, connection_id); return; }
    if action == "editchoice" { tree_editchoice(mobile_id, display_id, parts, connection_id); return; }
    if action == "addcond" { tree_addcond(mobile_id, display_id, parts, connection_id); return; }
    if action == "delcond" { tree_delcond(mobile_id, display_id, parts, connection_id); return; }
    if action == "addfx" || action == "addeffect" { tree_addfx(mobile_id, display_id, parts, connection_id); return; }
    if action == "delfx" || action == "deleffect" { tree_delfx(mobile_id, display_id, parts, connection_id); return; }
    tree_help(display_id, connection_id);
}

fn tree_help(display_id, connection_id) {
    let m = "Dialogue tree commands:\n"
        + "  medit " + display_id + " tree show [<node>]      - Show JSON, or one node summary\n"
        + "  medit " + display_id + " tree nodes              - List all node names\n"
        + "  medit " + display_id + " tree addnode <name> <text>  - Add a node (first node becomes root)\n"
        + "  medit " + display_id + " tree delnode <name>     - Remove a node (must not be root or referenced)\n"
        + "  medit " + display_id + " tree setroot <name>     - Point root_node at an existing node\n"
        + "  medit " + display_id + " tree text <node> <new text>  - Set node text in one line\n"
        + "  medit " + display_id + " tree edittext <node>    - Open multi-line editor for node text\n"
        + "  medit " + display_id + " tree addchoice <node> <kw> | <label> | <goto|exit|repeat> [target]\n"
        + "  medit " + display_id + " tree delchoice <node> <index>\n"
        + "  medit " + display_id + " tree editchoice <node> <index> hint <text...>   (empty clears)\n"
        + "  medit " + display_id + " tree editchoice <node> <index> cooldown <secs>  (0 clears)\n"
        + "  medit " + display_id + " tree editchoice <node> <index> once <on|off>\n"
        + "  medit " + display_id + " tree addcond <node> <choice_idx> <kind> [args...]\n"
        + "    kinds: flag_set|flag_unset|has_item|skill_at_least|counter_at_least|\n"
        + "           quest_active|quest_complete|quest_completable|has_achievement\n"
        + "  medit " + display_id + " tree delcond <node> <choice_idx> <cond_idx>\n"
        + "  medit " + display_id + " tree addfx <node> <choice_idx> <kind> [args...]\n"
        + "    kinds: set_flag|clear_flag|give_item|take_item|award_skill_xp|\n"
        + "           set_counter|increment_counter|offer_quest|complete_quest|abandon_quest\n"
        + "  medit " + display_id + " tree delfx <node> <choice_idx> <fx_idx>\n"
        + "  medit " + display_id + " tree set <json>         - Replace whole tree from JSON\n"
        + "  medit " + display_id + " tree clear              - Remove the dialogue tree";
    send_client_message(connection_id, m);
}

fn tree_show(mobile_id, parts, connection_id) {
    if parts.len() >= 4 {
        // tree show <node>
        let out = describe_dialogue_node(mobile_id, parts[3]);
        send_client_message(connection_id, out);
        return;
    }
    let json = get_mobile_dialogue_tree_json(mobile_id);
    if json == "" {
        send_client_message(connection_id, "(no dialogue tree set)");
    } else {
        send_client_message(connection_id, json);
    }
}

fn tree_nodes(mobile_id, connection_id) {
    let names = list_dialogue_node_names(mobile_id);
    if names == "" {
        send_client_message(connection_id, "(no dialogue tree set)");
    } else {
        send_client_message(connection_id, "Nodes: " + names);
    }
}

fn tree_set_json(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree set <json>");
        return;
    }
    let payload = "";
    for i in 3..parts.len() {
        if i > 3 { payload += " "; }
        payload += parts[i];
    }
    let err = set_mobile_dialogue_tree_json(mobile_id, payload);
    if err == "" {
        send_client_message(connection_id, "Dialogue tree updated.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_addnode(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree addnode <name> <text...>");
        return;
    }
    let name = parts[3];
    let text = "";
    for i in 4..parts.len() {
        if i > 4 { text += " "; }
        text += parts[i];
    }
    let err = olc_add_dialogue_node(mobile_id, name, text);
    if err == "" {
        send_client_message(connection_id, "Node `" + name + "` added.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_delnode(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree delnode <name>");
        return;
    }
    let err = olc_remove_dialogue_node(mobile_id, parts[3]);
    if err == "" {
        send_client_message(connection_id, "Node `" + parts[3] + "` removed.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_setroot(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree setroot <name>");
        return;
    }
    let err = olc_set_dialogue_root(mobile_id, parts[3]);
    if err == "" {
        send_client_message(connection_id, "Root set to `" + parts[3] + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_text(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree text <node> <new text...>");
        return;
    }
    let name = parts[3];
    let text = "";
    for i in 4..parts.len() {
        if i > 4 { text += " "; }
        text += parts[i];
    }
    let err = olc_set_dialogue_node_text(mobile_id, name, text);
    if err == "" {
        send_client_message(connection_id, "Node `" + name + "` text updated.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_edittext(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree edittext <node>");
        return;
    }
    let name = parts[3];
    // Stage the editor: anchor mobile + node, prefill buffer with existing text,
    // flip mode to collecting_dialogue_node_text.
    set_olc_edit_mobile(connection_id, mobile_id);
    set_olc_dialogue_node(connection_id, name);
    let json = get_mobile_dialogue_tree_json(mobile_id);
    // Best-effort: if the tree JSON contains the node, the existing text would
    // be embedded. We don't parse it here; the engine clears the buffer to
    // empty if start fresh. Keep it simple — start with an empty buffer.
    set_olc_buffer_text(connection_id, "");
    set_olc_mode(connection_id, "collecting_dialogue_node_text");
    if try_enter_modern_editor(connection_id) {
        return;
    }
    send_client_message(connection_id,
        "Editing text for dialogue node `" + name + "`. Type lines, .h for help, .save to save, .abort to cancel.");
}

fn tree_addchoice(mobile_id, display_id, parts, connection_id) {
    // Form: tree addchoice <node> <kw> | <label> | <kind> [target]
    // We re-join from parts[3..] and split on `|`.
    if parts.len() < 5 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree addchoice <node> <keyword> | <label> | <goto|exit|repeat> [target_node]");
        return;
    }
    let node = parts[3];
    // Re-join everything after the node name.
    let rest = "";
    for i in 4..parts.len() {
        if i > 4 { rest += " "; }
        rest += parts[i];
    }
    // Split on `|`. Rhai's split returns an array.
    let segs = rest.split('|');
    if segs.len() < 3 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree addchoice <node> <keyword> | <label> | <goto|exit|repeat> [target_node]");
        return;
    }
    let keyword = segs[0];
    let label = segs[1];
    let target_part = segs[2];
    // target_part is "goto X" or "exit" or "repeat".
    let tparts = target_part.split(' ');
    let kind = if tparts.len() >= 1 { tparts[0].to_lower() } else { "" };
    let target_node = if tparts.len() >= 2 {
        let t = "";
        for i in 1..tparts.len() {
            if i > 1 { t += " "; }
            t += tparts[i];
        }
        t
    } else { "" };
    let err = olc_add_dialogue_choice(mobile_id, node, keyword, label, kind, target_node);
    if err == "" {
        send_client_message(connection_id, "Choice added to `" + node + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_delchoice(mobile_id, display_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit " + display_id + " tree delchoice <node> <index>");
        return;
    }
    let idx = try_parse_int(parts[4]);
    if idx == () {
        send_client_message(connection_id, "Index must be a non-negative number.");
        return;
    }
    let err = olc_remove_dialogue_choice(mobile_id, parts[3], idx);
    if err == "" {
        send_client_message(connection_id, "Choice " + idx + " removed from `" + parts[3] + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_editchoice(mobile_id, display_id, parts, connection_id) {
    // Form: tree editchoice <node> <index> <field> [value...]
    if parts.len() < 6 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree editchoice <node> <index> <hint|cooldown|once> [value]");
        return;
    }
    let node = parts[3];
    let idx = try_parse_int(parts[4]);
    if idx == () {
        send_client_message(connection_id, "Index must be a non-negative number.");
        return;
    }
    let field = parts[5].to_lower();
    // Re-join the rest as the value (preserving spaces in hint text).
    let value = "";
    for i in 6..parts.len() {
        if i > 6 { value += " "; }
        value += parts[i];
    }
    let err = olc_set_dialogue_choice_field(mobile_id, node, idx, field, value);
    if err == "" {
        send_client_message(connection_id,
            "Choice " + idx + " on `" + node + "` updated (" + field + ").");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_addcond(mobile_id, display_id, parts, connection_id) {
    // Form: tree addcond <node> <choice_idx> <kind> [args...]
    if parts.len() < 6 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree addcond <node> <choice_idx> <kind> [args...]");
        return;
    }
    let node = parts[3];
    let idx = try_parse_int(parts[4]);
    if idx == () {
        send_client_message(connection_id, "Choice index must be a non-negative number.");
        return;
    }
    let kind = parts[5].to_lower();
    let args = "";
    for i in 6..parts.len() {
        if i > 6 { args += " "; }
        args += parts[i];
    }
    let err = olc_add_choice_condition(mobile_id, node, idx, kind, args);
    if err == "" {
        send_client_message(connection_id,
            "Condition added to choice " + idx + " on `" + node + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_delcond(mobile_id, display_id, parts, connection_id) {
    // Form: tree delcond <node> <choice_idx> <cond_idx>
    if parts.len() < 6 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree delcond <node> <choice_idx> <cond_idx>");
        return;
    }
    let node = parts[3];
    let choice_idx = try_parse_int(parts[4]);
    let cond_idx = try_parse_int(parts[5]);
    if choice_idx == () || cond_idx == () {
        send_client_message(connection_id, "Indices must be non-negative numbers.");
        return;
    }
    let err = olc_remove_choice_condition(mobile_id, node, choice_idx, cond_idx);
    if err == "" {
        send_client_message(connection_id,
            "Condition " + cond_idx + " removed from choice " + choice_idx + " on `" + node + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_addfx(mobile_id, display_id, parts, connection_id) {
    // Form: tree addfx <node> <choice_idx> <kind> [args...]
    if parts.len() < 6 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree addfx <node> <choice_idx> <kind> [args...]");
        return;
    }
    let node = parts[3];
    let idx = try_parse_int(parts[4]);
    if idx == () {
        send_client_message(connection_id, "Choice index must be a non-negative number.");
        return;
    }
    let kind = parts[5].to_lower();
    let args = "";
    for i in 6..parts.len() {
        if i > 6 { args += " "; }
        args += parts[i];
    }
    let err = olc_add_choice_effect(mobile_id, node, idx, kind, args);
    if err == "" {
        send_client_message(connection_id,
            "Effect added to choice " + idx + " on `" + node + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn tree_delfx(mobile_id, display_id, parts, connection_id) {
    // Form: tree delfx <node> <choice_idx> <fx_idx>
    if parts.len() < 6 {
        send_client_message(connection_id,
            "Usage: medit " + display_id + " tree delfx <node> <choice_idx> <fx_idx>");
        return;
    }
    let node = parts[3];
    let choice_idx = try_parse_int(parts[4]);
    let fx_idx = try_parse_int(parts[5]);
    if choice_idx == () || fx_idx == () {
        send_client_message(connection_id, "Indices must be non-negative numbers.");
        return;
    }
    let err = olc_remove_choice_effect(mobile_id, node, choice_idx, fx_idx);
    if err == "" {
        send_client_message(connection_id,
            "Effect " + fx_idx + " removed from choice " + choice_idx + " on `" + node + "`.");
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

// Dispatch group 5: Special roles (spawn, shop, healer)
fn dispatch_role_subcommand(mobile, uuid, subcommand, parts, char, connection_id) {
    if subcommand == "spawn" { handle_spawn_subcommand(mobile, char, connection_id); return true; }
    if subcommand == "shop" { handle_shop_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "healer" { handle_healer_subcommand(mobile, uuid, parts, connection_id); return true; }
    return false;
}

// Dispatch group 6: Advanced features (trigger, transport, leasing, combat_spells)
fn dispatch_advanced_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) {
    if subcommand == "trigger" { handle_trigger_subcommand(mobile, uuid, mobile_id, parts, connection_id); return true; }
    if subcommand == "transport" { handle_transport_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "leasing" { handle_leasing_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "combat_spells" || subcommand == "spells" { handle_combat_spells_subcommand(mobile, uuid, parts, connection_id); return true; }
    return false;
}

// Dispatch group 7: Daily routine
fn dispatch_routine_subcommand(mobile, uuid, subcommand, parts, connection_id) {
    if subcommand == "routine" { handle_routine_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "simulation" || subcommand == "sim" { handle_simulation_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "social" { handle_social_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "pregnancy" || subcommand == "preg" { handle_pregnancy_subcommand(mobile, uuid, parts, connection_id); return true; }
    if subcommand == "on_hit" || subcommand == "onhit" { handle_mob_on_hit_subcommand(uuid, mobile_id, parts, connection_id); return true; }
    return false;
}

// Dispatch group 8: Generic mobile presets
fn dispatch_preset_subcommand(uuid, subcommand, parts, connection_id) {
    if subcommand == "preset" { handle_preset_apply(uuid, parts, connection_id); return true; }
    if subcommand == "presets" { handle_preset_list(parts, connection_id); return true; }
    return false;
}

// Dispatch group 9: Vampire subcommand (gated — only fires when this is a vampire mob).
// Off the vampire path, builders never see the surface; the friendly-fallback message
// hints at the preset/flag commands that turn it on.
fn dispatch_vampire_subcommand(mobile, uuid, subcommand, parts, connection_id) {
    if subcommand != "vampire" { return false; }
    if !mobile.flags.vampire && !is_mobile_vampire(uuid) {
        send_client_message(connection_id, "This mob isn't a vampire.\nApply 'medit " + uuid + " preset vampire_goon' (or similar) or 'medit " + uuid + " flag vampire on' to enable vampire options.");
        return true;
    }
    handle_vampire_subcommand(uuid, parts, connection_id);
    return true;
}

fn handle_vampire_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        show_vampire_state(mobile_id, connection_id);
        return;
    }
    let action = parts[2].to_lower();
    if action == "show" { show_vampire_state(mobile_id, connection_id); return; }
    if action == "blood" { handle_vampire_blood(mobile_id, parts, connection_id); return; }
    if action == "humanity" { handle_vampire_humanity(mobile_id, parts, connection_id); return; }
    if action == "embrace" { handle_vampire_embrace(mobile_id, parts, connection_id); return; }
    if action == "clear" || action == "revoke" { handle_vampire_revoke(mobile_id, connection_id); return; }
    send_client_message(connection_id, "Vampire subcommands: show, blood <n>, humanity <0-10>, embrace [<sire>], clear");
}

fn show_vampire_state(mobile_id, connection_id) {
    if !is_mobile_vampire(mobile_id) {
        send_client_message(connection_id, "(no vampire state)");
        return;
    }
    let blood = get_mobile_blood_pool(mobile_id);
    let humanity = get_mobile_humanity(mobile_id);
    let output = olc::c_header("=== Vampire State ===", connection_id) + "\n";
    let blood_line = olc::c_label("Blood pool: ", connection_id) + olc::c_value(blood, connection_id) + "\n";
    let humanity_line = olc::c_label("Humanity: ", connection_id) + olc::c_value(humanity, connection_id) + " / 10\n";
    send_client_message(connection_id, output + blood_line + humanity_line);
}

fn handle_vampire_blood(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> vampire blood <n>");
        return;
    }
    let n = try_parse_int(parts[3]);
    if n == () || n < 0 {
        send_client_message(connection_id, "Blood must be a non-negative integer.");
        return;
    }
    if set_mobile_blood_pool(mobile_id, n) {
        send_client_message(connection_id, "Blood pool set to " + n + ".");
    } else {
        send_client_message(connection_id, "Failed to set blood pool.");
    }
}

fn handle_vampire_humanity(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> vampire humanity <0-10>");
        return;
    }
    let n = try_parse_int(parts[3]);
    if n == () || n < 0 || n > 10 {
        send_client_message(connection_id, "Humanity must be between 0 and 10.");
        return;
    }
    if set_mobile_humanity(mobile_id, n) {
        send_client_message(connection_id, "Humanity set to " + n + ".");
    } else {
        send_client_message(connection_id, "Failed to set humanity.");
    }
}

fn handle_vampire_embrace(mobile_id, parts, connection_id) {
    let sire = "";
    if parts.len() >= 4 { sire = build_string_from_parts(parts, 3); }
    if embrace_mobile(mobile_id, sire, "") {
        let msg = if sire == "" { "" } else { " (sire: " + sire + ")" };
        send_client_message(connection_id, "Mobile embraced as a vampire" + msg + ".");
    } else {
        send_client_message(connection_id, "Failed to embrace mobile.");
    }
}

fn handle_vampire_revoke(mobile_id, connection_id) {
    if revoke_mobile_vampirism(mobile_id) {
        send_client_message(connection_id, "Vampire state cleared.");
    } else {
        send_client_message(connection_id, "Mobile is not a vampire.");
    }
}

fn handle_preset_list(parts, connection_id) {
    let tag_filter = "";
    if parts.len() >= 3 { tag_filter = parts[2]; }

    let presets = list_mobile_presets(tag_filter);
    if presets.len() == 0 {
        if tag_filter == "" {
            send_client_message(connection_id, "No mobile presets defined.");
        } else {
            send_client_message(connection_id, "No mobile presets matching tag '" + tag_filter + "'.");
        }
        return;
    }

    let header = "=== Mobile Presets";
    if tag_filter != "" { header += " (" + tag_filter + ")"; }
    header += " ===";
    let output = olc::c_header(header, connection_id) + "\n";
    for preset in presets {
        output += olc::c_label(preset["id"], connection_id);
        if preset["name"] != "" { output += " (" + preset["name"] + ")"; }
        output += "\n";
        if preset["description"] != "" {
            output += "  " + preset["description"] + "\n";
        }
        let tags = preset["tags"];
        if tags.len() > 0 {
            let tag_str = "";
            for t in tags {
                if tag_str != "" { tag_str += ", "; }
                tag_str += t;
            }
            output += "  tags: " + olc::c_value(tag_str, connection_id) + "\n";
        }
    }
    output += "\nUsage: medit <id> preset <preset_id>";
    send_client_message(connection_id, output);
}

fn handle_preset_apply(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit <id> preset <preset_id>");
        send_client_message(connection_id, "Run 'medit <id> presets' to see what's available.");
        return;
    }

    let preset_id = parts[2];
    if apply_mobile_preset(mobile_id, preset_id) {
        send_client_message(connection_id, "Applied preset '" + preset_id + "'.");
    } else {
        send_client_message(connection_id, "Failed to apply preset '" + preset_id + "'. Check that the preset id exists (medit <id> presets).");
    }
}

fn handle_subcommand(mobile, mobile_id, subcommand, parts, char, connection_id) {
    let uuid = mobile.id;

    // Dispatch to handler groups
    if dispatch_text_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) { return; }
    if dispatch_combat_subcommand(uuid, subcommand, parts, connection_id) { return; }
    if dispatch_meta_subcommand(mobile, uuid, subcommand, parts, connection_id) { return; }
    if dispatch_dialogue_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) { return; }
    if dispatch_role_subcommand(mobile, uuid, subcommand, parts, char, connection_id) { return; }
    if dispatch_advanced_subcommand(mobile, uuid, mobile_id, subcommand, parts, connection_id) { return; }
    if dispatch_routine_subcommand(mobile, uuid, subcommand, parts, connection_id) { return; }
    if dispatch_preset_subcommand(uuid, subcommand, parts, connection_id) { return; }
    if dispatch_vampire_subcommand(mobile, uuid, subcommand, parts, connection_id) { return; }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
}

fn build_string_from_parts(parts, start_idx) {
    let result = "";
    for i in start_idx..parts.len() {
        if result != "" { result += " "; }
        result += parts[i];
    }
    return result;
}

fn collect_parts(parts, start_idx) {
    let result = [];
    for i in start_idx..parts.len() {
        result.push(parts[i]);
    }
    return result;
}

fn handle_level_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " level <value>");
        return;
    }
    let level = try_parse_int(parts[2]);
    if level == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if level < 1 {
        send_client_message(connection_id, "Level must be at least 1.");
        return;
    }
    set_mobile_level(mobile_id, level);
    send_client_message(connection_id, "Level set to: " + level);
}

fn handle_hp_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " hp <value>");
        return;
    }
    let hp = try_parse_int(parts[2]);
    if hp == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if hp < 1 {
        send_client_message(connection_id, "HP must be at least 1.");
        return;
    }
    set_mobile_hp(mobile_id, hp);
    send_client_message(connection_id, "Max HP set to: " + hp);
}

fn handle_damage_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " damage <dice> (e.g., 2d6+3)");
        return;
    }
    let damage_dice = build_string_from_parts(parts, 2);
    set_mobile_damage(mobile_id, damage_dice);
    send_client_message(connection_id, "Damage set to: " + damage_dice);
}

fn handle_ac_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " ac <value>");
        return;
    }
    let ac = try_parse_int(parts[2]);
    if ac == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    set_mobile_ac(mobile_id, ac);
    send_client_message(connection_id, "Armor class set to: " + ac);
}

fn handle_gold_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " gold <amount>");
        return;
    }
    let amount = try_parse_int(parts[2]);
    if amount == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if amount < 0 {
        send_client_message(connection_id, "Gold amount must be 0 or greater.");
        return;
    }
    set_mobile_gold(mobile_id, amount);
    send_client_message(connection_id, "Gold set to: " + amount);
}

fn handle_perception_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " perception <0-10>");
        return;
    }
    let value = try_parse_int(parts[2]);
    if value == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if value < 0 || value > 10 {
        send_client_message(connection_id, "Perception must be between 0 and 10.");
        return;
    }
    set_mobile_perception(mobile_id, value);
    send_client_message(connection_id, "Perception set to: " + value);
}

fn handle_world_max_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " world_max <n>  (0 = unlimited)");
        return;
    }
    let n = try_parse_int(parts[2]);
    if n == () {
        send_client_message(connection_id, "Cap must be a whole number (0 to clear).");
        return;
    }
    set_mobile_world_max_count(mobile_id, n);
    if n <= 0 {
        send_client_message(connection_id, "World cap cleared (unlimited copies).");
    } else {
        send_client_message(connection_id, "World cap set to " + n + " live copies.");
    }
}

fn handle_gender_subcommand(mobile_id, parts, connection_id) {
    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        send_client_message(connection_id, "Mobile not found.");
        return;
    }
    if parts.len() < 3 {
        let current = mobile.gender;
        let display = if current == "" { "(unset - reads as neuter in DG)" } else { current };
        send_client_message(connection_id, "Gender: " + display);
        send_client_message(connection_id, "Usage: medit " + mobile_id + " gender <male|female|nonbinary|clear|<custom>>");
        return;
    }
    let value = parts[2];
    let lowered = value.to_lower();
    if lowered == "clear" || lowered == "none" || lowered == "unset" {
        set_mobile_gender(mobile_id, "");
        send_client_message(connection_id, "Gender cleared (DG resolves as neuter).");
        return;
    }
    let stored = set_mobile_gender(mobile_id, value);
    if stored == "" {
        send_client_message(connection_id, "Failed to set gender.");
        return;
    }
    let pronouns = resolve_gender_pronouns(stored);
    send_client_message(connection_id, "Gender set to: " + stored + " (pronouns: " + pronouns + ")");
}

fn handle_position_subcommand(mobile_id, parts, connection_id) {
    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        send_client_message(connection_id, "Mobile not found.");
        return;
    }
    if parts.len() < 3 {
        send_client_message(connection_id, "Position: " + mobile.position);
        send_client_message(connection_id, "Usage: medit " + mobile_id + " position <standing|sitting|sleeping>");
        return;
    }
    let value = parts[2].to_lower();
    if !set_mobile_position(mobile_id, value) {
        send_client_message(connection_id, "Invalid position. Choose: standing, sitting, or sleeping.");
        return;
    }
    send_client_message(connection_id, "Position set to: " + value);
}

fn handle_faction_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        let current = get_mobile_faction(mobile_id);
        let shown = if current == "" { "(none)" } else { current };
        send_client_message(connection_id, "Faction: " + shown + "\nUsage: medit " + mobile_id + " faction <tag|clear>");
        return;
    }
    let value = parts[2];
    if value.to_lower() == "clear" || value == "" {
        set_mobile_faction(mobile_id, "");
        send_client_message(connection_id, "Faction cleared.");
        return;
    }
    set_mobile_faction(mobile_id, value);
    send_client_message(connection_id, "Faction set to: " + value);
}

fn handle_language_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        let current = get_mobile_spoken_language(mobile_id);
        let shown = if current == "" { "(none — speaks Common)" } else { get_language_display_name(current) };
        send_client_message(connection_id,
            "Speaks: " + shown
            + "\nUsage: medit " + mobile_id + " language <key|clear>"
            + "\nKnown languages: " + olc::join_array(get_language_keys()));
        return;
    }
    let value = parts[2];
    if value.to_lower() == "clear" || value == "" {
        let err = set_mobile_spoken_language(mobile_id, "");
        if err == "" {
            send_client_message(connection_id, "Spoken language cleared (mob now speaks Common).");
        } else {
            send_client_message(connection_id, "Error: " + err);
        }
        return;
    }
    let err = set_mobile_spoken_language(mobile_id, value);
    if err == "" {
        send_client_message(connection_id, "Spoken language set to: " + get_language_display_name(value));
    } else {
        send_client_message(connection_id, "Error: " + err);
    }
}

fn handle_damtype_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        let all_types = get_all_damage_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Usage: medit <mobile> damtype <type>\nAvailable: " + types_str);
        return;
    }
    let type_str = parts[2].to_lower();
    if set_mobile_damage_type(mobile_id, type_str) {
        send_client_message(connection_id, "Damage type set to: " + type_str);
    } else {
        let all_types = get_all_damage_types();
        let types_str = olc::join_array(all_types);
        send_client_message(connection_id, "Invalid damage type. Available: " + types_str);
    }
}

fn handle_stat_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " stat <str|dex|con|int|wis|cha> <value>");
        return;
    }
    let stat_name = parts[2].to_lower();
    let value = try_parse_int(parts[3]);
    if value == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    let valid_stats = ["str", "dex", "con", "int", "wis", "cha"];
    let is_valid = false;
    for s in valid_stats {
        if stat_name == s { is_valid = true; }
    }
    if !is_valid {
        send_client_message(connection_id, "Invalid stat. Use: str, dex, con, int, wis, cha");
        return;
    }
    set_mobile_stat(mobile_id, stat_name, value);
    let full_name = get_stat_full_name(stat_name);
    send_client_message(connection_id, full_name + " set to: " + value);
}

fn get_stat_full_name(stat) {
    if stat == "str" { return "Strength"; }
    if stat == "dex" { return "Dexterity"; }
    if stat == "con" { return "Constitution"; }
    if stat == "int" { return "Intelligence"; }
    if stat == "wis" { return "Wisdom"; }
    if stat == "cha" { return "Charisma"; }
    return stat;
}

fn handle_autostats_subcommand(mobile_id, parts, connection_id) {
    // Get current mobile to check its level
    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        send_client_message(connection_id, "Mobile not found.");
        return;
    }

    let level = mobile.level;
    if level < 1 { level = 1; }
    if level > 10 { level = 10; }

    // Apply level-appropriate stats
    if apply_level_stats(mobile_id, level) {
        let stats = get_level_stats(level);
        let output = "Applied level " + level + " stats:\n";
        output += "  HP: " + stats.hp + "\n";
        output += "  AC: " + stats.ac + "\n";
        output += "  Damage: " + stats.damage + "\n";
        output += "  Hit Modifier: " + stats.hit_modifier + "\n";
        output += "  All Stats: " + stats.stats + "\n";
        output += "\nTip: Set level first with 'medit <id> level <1-10>', then use 'autostats'.";
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, "Failed to apply level stats.");
    }
}

fn handle_flag_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " flag <name> [<name>...] [on|off]");
        return;
    }
    let mobile = get_mobile_data(mobile_id);
    if mobile == () {
        send_client_message(connection_id, "Mobile not found.");
        return;
    }

    // Split into flag names and optional trailing on/off value.
    let flag_names = [];
    for i in 2..parts.len() { flag_names.push(parts[i].to_lower()); }
    let explicit_val = ();
    if flag_names.len() >= 2 {
        let last = flag_names[flag_names.len() - 1];
        if olc::is_on_value(last) {
            explicit_val = true;
            flag_names.pop();
        } else if olc::is_off_value(last) {
            explicit_val = false;
            flag_names.pop();
        }
    }

    // Validate all names before changing anything.
    for name in flag_names {
        if get_mobile_flag_value(mobile, name) == () {
            send_client_message(connection_id, "Unknown flag: " + name + "\nAvailable: aggressive, sentinel, scavenger, shopkeeper, no_attack, healer, helper, leasing_agent, cowardly, can_open_doors, guard, thief, cant_swim, poisonous, fiery, chilling, corrosive, shocking, unique, stay_zone, aware, memory, no_sleep, no_blind, no_bash, no_summon, no_charm, hostile_on_steal, tameable, undead, vampire, holy_vulnerable");
            return;
        }
    }

    for name in flag_names {
        let new_val = if explicit_val == () {
            !get_mobile_flag_value(mobile, name)
        } else {
            explicit_val
        };
        if set_mobile_flag(mobile_id, name, new_val) {
            let status = if new_val { "ON" } else { "OFF" };
            send_client_message(connection_id, "Flag '" + name + "' is now " + status + ".");
        } else {
            send_client_message(connection_id, "Failed to set flag '" + name + "'.");
        }
    }
}

fn get_mobile_flag_value(mobile, flag_name) {
    let flags = mobile.flags;
    if flag_name == "aggressive" { return flags.aggressive; }
    if flag_name == "sentinel" { return flags.sentinel; }
    if flag_name == "scavenger" { return flags.scavenger; }
    if flag_name == "shopkeeper" { return flags.shopkeeper; }
    if flag_name == "no_attack" || flag_name == "noattack" { return flags.no_attack; }
    if flag_name == "healer" { return flags.healer; }
    if flag_name == "leasing_agent" || flag_name == "leasingagent" { return flags.leasing_agent; }
    if flag_name == "cowardly" { return flags.cowardly; }
    if flag_name == "can_open_doors" || flag_name == "canopendoors" { return flags.can_open_doors; }
    if flag_name == "guard" { return flags.guard; }
    if flag_name == "helper" { return flags.helper; }
    if flag_name == "thief" { return flags.thief; }
    if flag_name == "cant_swim" || flag_name == "cantswim" { return flags.cant_swim; }
    if flag_name == "poisonous" { return flags.poisonous; }
    if flag_name == "fiery" { return flags.fiery; }
    if flag_name == "chilling" { return flags.chilling; }
    if flag_name == "corrosive" { return flags.corrosive; }
    if flag_name == "shocking" { return flags.shocking; }
    if flag_name == "unique" { return flags.unique; }
    if flag_name == "stay_zone" || flag_name == "stayzone" { return flags.stay_zone; }
    if flag_name == "aware" { return flags.aware; }
    if flag_name == "memory" { return flags.memory; }
    if flag_name == "no_sleep" || flag_name == "nosleep" { return flags.no_sleep; }
    if flag_name == "no_blind" || flag_name == "noblind" { return flags.no_blind; }
    if flag_name == "no_bash" || flag_name == "nobash" { return flags.no_bash; }
    if flag_name == "no_summon" || flag_name == "nosummon" { return flags.no_summon; }
    if flag_name == "no_charm" || flag_name == "nocharm" { return flags.no_charm; }
    if flag_name == "hostile_on_steal" || flag_name == "hostileonsteal" { return flags.hostile_on_steal; }
    if flag_name == "tameable" { return flags.tameable; }
    if flag_name == "undead" { return flags.undead; }
    if flag_name == "vampire" { return flags.vampire; }
    if flag_name == "holy_vulnerable" || flag_name == "holyvulnerable" { return flags.holy_vulnerable; }
    return ();
}

fn handle_prototype_subcommand(mobile_id, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    set_mobile_prototype(mobile_id, new_val);
    let status = "a prototype template";
    if !new_val { status = "a spawned instance"; }
    send_client_message(connection_id, "Mobile is now: " + status);
}

fn handle_vnum_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " vnum <name> (or 'none' to clear)");
        return;
    }
    let vnum = parts[2];
    if vnum.to_lower() == "none" || vnum == "" {
        set_mobile_vnum(mobile_id, "");
        send_client_message(connection_id, "VNUM cleared.");
    } else {
        // Check if vnum is already in use
        let existing = get_mobile_by_vnum(vnum);
        if existing != () && existing.id != mobile_id {
            send_client_message(connection_id, "VNUM '" + vnum + "' is already in use by: " + existing.name);
            return;
        }
        set_mobile_vnum(mobile_id, vnum);
        send_client_message(connection_id, "VNUM set to: " + vnum);
    }
}

fn handle_dialogue_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " dialogue <keyword> <response text>");
        return;
    }
    let keyword = parts[2].to_lower();
    let response = build_string_from_parts(parts, 3);
    set_mobile_dialogue(mobile_id, keyword, response);
    send_client_message(connection_id, "Dialogue set for '" + keyword + "'.");
}

fn handle_rmdialogue_subcommand(mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " rmdialogue <keyword>");
        return;
    }
    let keyword = parts[2].to_lower();
    if remove_mobile_dialogue(mobile_id, keyword) {
        send_client_message(connection_id, "Dialogue removed for '" + keyword + "'.");
    } else {
        send_client_message(connection_id, "No dialogue found for '" + keyword + "'.");
    }
}

fn handle_spawn_subcommand(mobile, char, connection_id) {
    if mobile.vnum == "" {
        send_client_message(connection_id, "This prototype has no VNUM. Set one with 'medit " + mobile.id + " vnum <name>'.");
        return;
    }
    let spawned = spawn_mobile_from_prototype(mobile.vnum);
    if spawned == () {
        send_client_message(connection_id, "Failed to spawn mobile.");
        return;
    }
    move_mobile_to_room(spawned.id, char.current_room_id);
    send_client_message(connection_id, "Spawned " + spawned.name + " in room.");
    broadcast_to_room(char.current_room_id, spawned.short_desc, char.name);
}

fn show_mobile_properties(mobile, mobile_id, connection_id) {
    let output = olc::c_header("=== Mobile Editor ===", connection_id) + "\n";
    output += olc::c_label("Name: ", connection_id) + olc::c_value(mobile.name, connection_id) + "\n";
    output += olc::c_label("ID: ", connection_id) + olc::c_value(mobile.id, connection_id) + "\n";
    output += olc::c_label("Short: ", connection_id) + olc::c_value(mobile.short_desc, connection_id) + "\n";

    let kws = mobile.keywords;
    output += olc::c_label("Keywords: ", connection_id);
    if kws.len() == 0 {
        output += "(none)\n";
    } else {
        output += olc::c_value(olc::join_array(kws), connection_id) + "\n";
    }

    output += "\n" + olc::c_label("--- Combat Stats ---", connection_id) + "\n";
    output += olc::c_label("Level: ", connection_id) + olc::c_value("" + mobile.level, connection_id) + "\n";
    output += olc::c_label("HP: ", connection_id) + olc::c_value(mobile.current_hp + "/" + mobile.max_hp, connection_id) + "\n";
    output += olc::c_label("Damage: ", connection_id) + olc::c_value(mobile.damage_dice, connection_id) + " (" + mobile.damage_type + ")\n";
    output += olc::c_label("AC: ", connection_id) + olc::c_value("" + mobile.armor_class, connection_id) + "\n";
    output += olc::c_label("Hit Modifier: ", connection_id) + olc::c_value("" + mobile.hit_modifier, connection_id) + "\n";
    output += olc::c_label("Perception: ", connection_id) + olc::c_value("" + mobile.perception, connection_id) + "\n";
    output += olc::c_label("Gold: ", connection_id) + olc::c_value("" + mobile.gold, connection_id) + "\n";
    if mobile.has_world_max_count {
        output += olc::c_label("World Max: ", connection_id) + olc::c_value("" + mobile.world_max_count + " live copies", connection_id) + "\n";
    }
    if mobile.faction != "" {
        output += olc::c_label("Faction: ", connection_id) + olc::c_value(mobile.faction, connection_id) + "\n";
    }
    if mobile.has_spoken_language {
        let lang_label = get_language_display_name(mobile.spoken_language);
        output += olc::c_label("Speaks: ", connection_id) + olc::c_value(lang_label, connection_id) + "\n";
    }
    let mob_on_hit = get_mobile_on_hit_effects(mobile_id);
    if mob_on_hit.len() > 0 {
        output += olc::c_label("On-hit Effects: ", connection_id) + olc::c_value("" + mob_on_hit.len(), connection_id) + "\n";
        let i = 0;
        while i < mob_on_hit.len() {
            output += "  [" + (i + 1) + "] " + format_mob_on_hit_entry(mob_on_hit[i]) + "\n";
            i += 1;
        }
    }

    output += "\n" + olc::c_label("--- Attributes ---", connection_id) + "\n";
    output += olc::c_label("STR: ", connection_id) + olc::c_value("" + mobile.stat_str, connection_id) + "  " + olc::c_label("DEX: ", connection_id) + olc::c_value("" + mobile.stat_dex, connection_id) + "  " + olc::c_label("CON: ", connection_id) + olc::c_value("" + mobile.stat_con, connection_id) + "\n";
    output += olc::c_label("INT: ", connection_id) + olc::c_value("" + mobile.stat_int, connection_id) + "  " + olc::c_label("WIS: ", connection_id) + olc::c_value("" + mobile.stat_wis, connection_id) + "  " + olc::c_label("CHA: ", connection_id) + olc::c_value("" + mobile.stat_cha, connection_id) + "\n";

    output += build_flags_display(mobile);

    // Triggers
    output += build_triggers_display(mobile_id);

    // Prototype info
    if mobile.is_prototype {
        output += "\n" + olc::c_warn("*** PROTOTYPE TEMPLATE ***", connection_id) + "\n";
        output += olc::c_label("VNUM: ", connection_id) + olc::c_value(if mobile.vnum != "" { mobile.vnum } else { "(not set)" }, connection_id) + "\n";
    } else {
        output += "\n" + olc::c_label("Status: ", connection_id) + olc::c_value("Spawned Instance", connection_id) + "\n";
        let loc = mobile.current_room_id;
        if loc != "" {
            output += olc::c_label("Location: ", connection_id) + olc::c_value(loc, connection_id) + "\n";
        }
    }

    // Dialogue count
    let dialogues = get_all_mobile_dialogues(mobile_id);
    let dialogue_count = 0;
    for k in dialogues.keys() {
        dialogue_count += 1;
    }
    output += olc::c_label("Dialogues: ", connection_id) + olc::c_value("" + dialogue_count, connection_id) + " (use 'medit " + mobile_id + " dialogues' to view)\n";

    // Simulation info
    if is_npc_simulated(mobile_id) {
        let sim_config = get_npc_simulation_config(mobile_id);
        let sim_needs = get_npc_needs(mobile_id);
        output += olc::c_label("Simulation: ", connection_id) + olc::c_value("ENABLED", connection_id) + "\n";
        if sim_config != () {
            output += "  Home: " + sim_config["home_room_vnum"] + " | Work: " + sim_config["work_room_vnum"] + " | Shop: " + sim_config["shop_room_vnum"] + "\n";
            output += "  Pay: " + sim_config["work_pay"] + "g | Hours: " + sim_config["work_start_hour"] + "-" + sim_config["work_end_hour"] + "\n";
        }
        if sim_needs != () {
            output += "  Hunger: " + sim_needs["hunger"] + " | Energy: " + sim_needs["energy"] + " | Comfort: " + sim_needs["comfort"] + " | Goal: " + sim_needs["goal"] + "\n";
        }
    }

    // Routine info
    let routine_count = get_mobile_routine_entry_count(mobile_id);
    let activity = get_mobile_activity(mobile_id);
    output += olc::c_label("Activity: ", connection_id) + olc::c_value(activity, connection_id) + "\n";
    output += olc::c_label("Routine: ", connection_id) + olc::c_value("" + routine_count + " entries", connection_id) + " (use 'medit " + mobile_id + " routine' to view)\n";

    output += "\n" + olc::c_label("Long Description:", connection_id) + "\n";
    if mobile.long_desc != "" {
        output += mobile.long_desc + "\n";
    } else {
        output += "(none)\n";
    }

    output += build_commands_help(mobile_id, mobile);

    send_client_message(connection_id, output);
}

fn build_flags_display(mobile) {
    let flags = mobile.flags;
    let output = "\nFlags: ";
    let flag_str = "";
    if flags.aggressive { flag_str += "aggressive "; }
    if flags.sentinel { flag_str += "sentinel "; }
    if flags.scavenger { flag_str += "scavenger "; }
    if flags.shopkeeper { flag_str += "shopkeeper "; }
    if flags.no_attack { flag_str += "no_attack "; }
    if flags.healer { flag_str += "healer "; }
    if flags.leasing_agent { flag_str += "leasing_agent "; }
    if flags.cowardly { flag_str += "cowardly "; }
    if flags.can_open_doors { flag_str += "can_open_doors "; }
    if flags.guard { flag_str += "guard "; }
    if flags.helper { flag_str += "helper "; }
    if flags.thief { flag_str += "thief "; }
    if flags.cant_swim { flag_str += "cant_swim "; }
    if flags.poisonous { flag_str += "poisonous "; }
    if flags.fiery { flag_str += "fiery "; }
    if flags.chilling { flag_str += "chilling "; }
    if flags.corrosive { flag_str += "corrosive "; }
    if flags.shocking { flag_str += "shocking "; }
    if flags.unique { flag_str += "unique "; }
    if flags.stay_zone { flag_str += "stay_zone "; }
    if flags.aware { flag_str += "aware "; }
    if flags.memory { flag_str += "memory "; }
    if flags.no_sleep { flag_str += "no_sleep "; }
    if flags.no_blind { flag_str += "no_blind "; }
    if flags.no_bash { flag_str += "no_bash "; }
    if flags.no_summon { flag_str += "no_summon "; }
    if flags.no_charm { flag_str += "no_charm "; }
    if flags.hostile_on_steal { flag_str += "hostile_on_steal "; }
    if flags.tameable { flag_str += "tameable "; }
    if flags.undead { flag_str += "undead "; }
    if flags.vampire { flag_str += "vampire "; }
    if flags.holy_vulnerable { flag_str += "holy_vulnerable "; }
    if flag_str == "" { flag_str = "(none)"; }
    output += flag_str + "\n";
    return output;
}

fn build_commands_help(mobile_id, mobile) {
    // Use vnum if available for cleaner command examples
    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let output = "\nCommands:\n";
    output += "  medit " + display_id + " name <text>\n";
    output += "  medit " + display_id + " short <text>          (AI: help <prompt> | rephrase)\n";
    output += "  medit " + display_id + " long <text>           (AI: help <prompt> | rephrase)\n";
    output += "  medit " + display_id + " keywords <kw1> <kw2> ...\n";
    output += "  medit " + display_id + " level <value>\n";
    output += "  medit " + display_id + " autostats              (set HP/AC/damage/stats from level)\n";
    output += "  medit " + display_id + " hp <value>\n";
    output += "  medit " + display_id + " damage <dice> (e.g., 2d6+3)\n";
    output += "  medit " + display_id + " damtype <type>\n";
    output += "  medit " + display_id + " ac <value>\n";
    output += "  medit " + display_id + " gold <amount>            (gold dropped on death)\n";
    output += "  medit " + display_id + " perception <0-10>       (stealth detection ability)\n";
    output += "  medit " + display_id + " world_max <n>           (cap live copies world-wide; 0 = unlimited)\n";
    output += "  medit " + display_id + " faction <tag|clear>     (helper system: matching tags ally)\n";
    output += "  medit " + display_id + " language <key|clear>    (spoken language)\n";
    output += "  medit " + display_id + " position <standing|sitting|sleeping>\n";
    output += "  medit " + display_id + " gender <male|female|nonbinary|clear>\n";
    output += "  medit " + display_id + " stat <str|dex|con|int|wis|cha> <value>\n";
    output += "  medit " + display_id + " flags\n";
    output += "  medit " + display_id + " flag <name> [on|off]\n";
    output += "  medit " + display_id + " presets               (list available presets)\n";
    output += "  medit " + display_id + " preset <id>           (apply a preset)\n";

    output += "\nDialogue Commands:\n";
    output += "  medit " + display_id + " dialogue <keyword> <response>\n";
    output += "  medit " + display_id + " dialogues\n";
    output += "  medit " + display_id + " rmdialogue <keyword>\n";

    output += "  medit " + display_id + " tree                  (dialogue tree editor; run for subcommands)\n";

    output += "\nPrototype Commands:\n";
    output += "  medit " + display_id + " prototype [on|off]\n";
    output += "  medit " + display_id + " vnum <name|none>\n";

    let flags = mobile.flags;

    if flags.shopkeeper {
        output += "\nShop Commands:\n";
        output += "  medit " + display_id + " shop              - Show shop configuration\n";
    }

    if flags.healer {
        output += "\nHealer Commands:\n";
        output += "  medit " + display_id + " healer            - Show healer configuration\n";
        output += "  medit " + display_id + " healer type <medic|herbalist|cleric>\n";
        output += "  medit " + display_id + " healer free on|off\n";
        output += "  medit " + display_id + " healer cost <percent>\n";
    }

    output += "\n  medit " + display_id + " transport         - Configure transport route\n";

    if flags.leasing_agent {
        output += "\nLeasing Agent Commands:\n";
        output += "  medit " + display_id + " leasing           - Show leasing configuration\n";
        output += "  medit " + display_id + " leasing add <template_vnum>\n";
        output += "  medit " + display_id + " leasing remove <template_vnum>\n";
    }

    if is_npc_simulated(mobile_id) {
        output += "\nSimulation Commands:\n";
        output += "  medit " + display_id + " simulation                                 - Show simulation status\n";
        output += "  medit " + display_id + " simulation setup <home> <work> <shop>       - Reconfigure simulation\n";
        output += "  medit " + display_id + " simulation remove                           - Disable simulation\n";
        output += "  medit " + display_id + " simulation pay <amount>                     - Set work shift pay\n";
        output += "  medit " + display_id + " simulation hours <start> <end>              - Set work hours\n";
        output += "  medit " + display_id + " simulation food <vnum>                      - Set preferred food\n";
        output += "  medit " + display_id + " simulation decay <need> <rate>              - Set decay rate (100=normal)\n";
    } else {
        output += "\n  medit " + display_id + " simulation setup <home> <work> <shop>  - Enable needs simulation\n";
    }

    let routine_count = get_mobile_routine_entry_count(mobile.id);
    if routine_count > 0 {
        output += "\nRoutine Commands:\n";
        output += "  medit " + display_id + " routine                              - Show current routine\n";
        output += "  medit " + display_id + " routine add <hour> <activity> [vnum]  - Add entry\n";
        output += "  medit " + display_id + " routine remove <hour>                 - Remove entry\n";
        output += "  medit " + display_id + " routine clear                         - Clear all entries\n";
        output += "  medit " + display_id + " routine preset list                   - List presets\n";
        output += "  medit " + display_id + " routine preset <name> <key=vnum ...>  - Apply preset\n";
        output += "  medit " + display_id + " routine msg <hour> <message>          - Set transition msg\n";
        output += "  medit " + display_id + " routine wander <hour> on|off          - Toggle wander suppression\n";
        output += "  medit " + display_id + " routine visible on|off                - Toggle schedule visibility\n";
        output += "  medit " + display_id + " routine dialogue <hour> <kw> <resp>   - Set dialogue override\n";
    } else {
        output += "\n  medit " + display_id + " routine              - Manage daily routine\n";
    }

    let triggers = get_mobile_triggers(mobile.id);
    if triggers.len() > 0 {
        output += "\nTrigger Commands:\n";
        output += "  medit " + display_id + " trigger list\n";
        output += "  medit " + display_id + " trigger add <greet|attack|death|say|idle|always> <script>\n";
        output += "  medit " + display_id + " trigger remove <index>\n";
        output += "  medit " + display_id + " trigger enable/disable <index>\n";
        output += "  medit " + display_id + " trigger chance <index> <1-100>\n";
        output += "  medit " + display_id + " trigger interval <index> <secs>\n";
    } else {
        output += "  medit " + display_id + " trigger list|add      - Manage triggers\n";
    }

    if mobile.is_prototype && mobile.vnum != "" {
        output += "\n  medit " + display_id + " spawn";
    }
    return output;
}

fn show_mobile_flags(mobile, mobile_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let flags = mobile.flags;
    let output = olc::c_header("=== Mobile Flags for " + mobile.name + " ===", connection_id) + "\n\n";

    output += build_flag_line("aggressive", flags.aggressive, "Attacks players on sight", display_id, connection_id);
    output += build_flag_line("sentinel", flags.sentinel, "Never wanders from spawn room", display_id, connection_id);
    output += build_flag_line("scavenger", flags.scavenger, "Picks up items from ground", display_id, connection_id);
    output += build_flag_line("shopkeeper", flags.shopkeeper, "Can buy/sell items (configure via 'shop' subcommand)", display_id, connection_id);
    output += build_flag_line("no_attack", flags.no_attack, "Cannot be attacked", display_id, connection_id);
    output += build_flag_line("healer", flags.healer, "NPC healer (configure via 'healer' subcommand)", display_id, connection_id);
    output += build_flag_line("leasing_agent", flags.leasing_agent, "Property rental agent (configure via 'leasing' subcommand)", display_id, connection_id);
    output += build_flag_line("cowardly", flags.cowardly, "Flees when sniped or HP below 25%", display_id, connection_id);
    output += build_flag_line("can_open_doors", flags.can_open_doors, "Can open/unlock doors during routine pathfinding", display_id, connection_id);
    output += build_flag_line("guard", flags.guard, "Enhanced perception, responds to nearby theft", display_id, connection_id);
    output += build_flag_line("helper", flags.helper, "Joins combat to defend faction allies attacked by a player", display_id, connection_id);
    output += build_flag_line("thief", flags.thief, "Steals gold from players", display_id, connection_id);
    output += build_flag_line("cant_swim", flags.cant_swim, "Cannot enter water rooms, takes damage in water", display_id, connection_id);
    output += build_flag_line("poisonous", flags.poisonous, "Melee hits apply a poison DoT", display_id, connection_id);
    output += build_flag_line("fiery", flags.fiery, "Melee hits apply a fire DoT", display_id, connection_id);
    output += build_flag_line("chilling", flags.chilling, "Melee hits apply a cold DoT", display_id, connection_id);
    output += build_flag_line("corrosive", flags.corrosive, "Melee hits apply an acid DoT", display_id, connection_id);
    output += build_flag_line("shocking", flags.shocking, "Melee hits apply a lightning DoT", display_id, connection_id);
    output += build_flag_line("unique", flags.unique, "Only one live (non-prototype) instance allowed in the world", display_id, connection_id);
    output += build_flag_line("stay_zone", flags.stay_zone, "Wandering and pursuit stay inside the home area", display_id, connection_id);
    output += build_flag_line("aware", flags.aware, "Sees through hidden / sneaking / invisibility", display_id, connection_id);
    output += build_flag_line("memory", flags.memory, "Remembers PC attackers and attacks on sight", display_id, connection_id);
    output += build_flag_line("no_sleep", flags.no_sleep, "Immune to the sleep spell", display_id, connection_id);
    output += build_flag_line("no_blind", flags.no_blind, "Immune to the blind spell", display_id, connection_id);
    output += build_flag_line("no_bash", flags.no_bash, "Immune to the bash skill", display_id, connection_id);
    output += build_flag_line("no_summon", flags.no_summon, "Immune to the summon spell", display_id, connection_id);
    output += build_flag_line("no_charm", flags.no_charm, "Immune to the charm spell", display_id, connection_id);
    output += build_flag_line("hostile_on_steal", flags.hostile_on_steal, "Attacks the thief when a steal attempt is caught", display_id, connection_id);
    output += build_flag_line("tameable", flags.tameable, "Charm spell installs a permanent pet bond instead of temporary control", display_id, connection_id);
    output += build_flag_line("undead", flags.undead, "Generic undead marker (zombies, skeletons, vampires, ghouls)", display_id, connection_id);
    output += build_flag_line("vampire", flags.vampire, "Sun-burn tick eligibility, on-hit blood drain, gates vampire subcommand", display_id, connection_id);
    output += build_flag_line("holy_vulnerable", flags.holy_vulnerable, "Doubled incoming Holy damage", display_id, connection_id);

    send_mxp_message(connection_id, output);
}

fn build_flag_line(name, value, desc, mobile_id, connection_id) {
    let status = "[OFF]";
    if value { status = "[ON] "; }

    let toggle_link = mxp_or(
        mxp_send("medit " + mobile_id + " flag " + name, status),
        status,
        connection_id
    );
    return toggle_link + " " + name + " - " + desc + "\n";
}

fn show_mobile_dialogues(mobile, mobile_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let dialogues = get_all_mobile_dialogues(mobile_id);
    let output = olc::c_header("=== Dialogues for " + mobile.name + " ===", connection_id) + "\n\n";

    let count = 0;
    for keyword in dialogues.keys() {
        let response = dialogues[keyword];
        output += "  [" + keyword + "]: " + response + "\n";
        count += 1;
    }

    if count == 0 {
        output += "(no dialogues defined)\n";
    }

    output += "\nUse 'medit " + display_id + " dialogue <keyword> <response>' to add.\n";
    output += "Use 'medit " + display_id + " rmdialogue <keyword>' to remove.";

    send_client_message(connection_id, output);
}

// Helper functions for setting mobile properties
fn set_mobile_name(mobile_id, name) {
    let mobile = get_mobile_data(mobile_id);
    if mobile != () {
        mobile.name = name;
        save_mobile_data(mobile);
    }
}

fn set_mobile_short_desc(mobile_id, desc) {
    let mobile = get_mobile_data(mobile_id);
    if mobile != () {
        mobile.short_desc = desc;
        save_mobile_data(mobile);
    }
}

fn set_mobile_long_desc(mobile_id, desc) {
    let mobile = get_mobile_data(mobile_id);
    if mobile != () {
        mobile.long_desc = desc;
        save_mobile_data(mobile);
    }
}

// Shop handling functions
fn handle_shop_subcommand(mobile, mobile_id, parts, connection_id) {
    if parts.len() < 3 {
        show_shop_config(mobile, mobile_id, connection_id);
        return;
    }

    let shop_cmd = parts[2].to_lower();

    if shop_cmd == "stock" {
        handle_shop_stock(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "buyrate" {
        handle_shop_buyrate(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "sellrate" {
        handle_shop_sellrate(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "buys" {
        handle_shop_buys(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "categories" {
        handle_shop_categories(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "minvalue" {
        handle_shop_minvalue(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "maxvalue" {
        handle_shop_maxvalue(mobile_id, parts, connection_id);
        return;
    }
    if shop_cmd == "preset" {
        handle_shop_preset(mobile_id, parts, connection_id);
        return;
    }

    show_shop_config(mobile, mobile_id, connection_id);
}

fn show_shop_config(mobile, mobile_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let output = olc::c_header("=== Shop Configuration for " + mobile.name + " ===", connection_id) + "\n\n";

    // Check if shopkeeper flag is enabled
    let flags = mobile.flags;
    if !flags.shopkeeper {
        output += olc::c_warn("*** WARNING: 'shopkeeper' flag is OFF ***", connection_id) + "\n";
        output += "Enable with: medit " + display_id + " flag shopkeeper on\n\n";
    }

    output += "Buy Rate: " + mobile.shop_buy_rate + "% (paid to players selling items)\n";
    output += "Sell Rate: " + mobile.shop_sell_rate + "% (charged to players buying items)\n";

    // Show what types this shop buys
    let buys_types = get_shop_buys_types(mobile_id);
    output += "Buys Types: ";
    output += format_type_list(buys_types);
    output += "\n";

    // Show buy categories
    let buys_cats = get_shop_buys_categories(mobile_id);
    output += "Buys Categories: ";
    if buys_cats.len() == 0 {
        output += "(none)";
    } else {
        output += buys_cats.join(", ");
    }
    output += "\n";

    // Value range
    let min_val = get_shop_min_value(mobile_id);
    let max_val = get_shop_max_value(mobile_id);
    output += "Value Range: ";
    if min_val > 0 && max_val > 0 {
        output += min_val + " - " + max_val + " gold";
    } else if min_val > 0 {
        output += min_val + "+ gold";
    } else if max_val > 0 {
        output += "up to " + max_val + " gold";
    } else {
        output += "(no restriction)";
    }
    output += "\n";

    // Preset info
    let preset_vnum = get_shop_preset_vnum(mobile_id);
    output += "Preset: ";
    if preset_vnum != "" {
        let preset = get_shop_preset(preset_vnum);
        if preset != () {
            output += preset_vnum + " (" + preset.name + ")";
        } else {
            output += preset_vnum + " (NOT FOUND!)";
        }
    } else {
        output += "(none)";
    }
    output += "\n";

    // Show preset overrides if any
    let extra_types = get_shop_extra_types(mobile_id);
    let extra_cats = get_shop_extra_categories(mobile_id);
    let deny_types = get_shop_deny_types(mobile_id);
    let deny_cats = get_shop_deny_categories(mobile_id);
    if extra_types.len() > 0 || extra_cats.len() > 0 || deny_types.len() > 0 || deny_cats.len() > 0 {
        output += "\nPreset Overrides:\n";
        if extra_types.len() > 0 {
            output += "  Extra Types: " + extra_types.join(", ") + "\n";
        }
        if extra_cats.len() > 0 {
            output += "  Extra Categories: " + extra_cats.join(", ") + "\n";
        }
        if deny_types.len() > 0 {
            output += "  Deny Types: " + deny_types.join(", ") + "\n";
        }
        if deny_cats.len() > 0 {
            output += "  Deny Categories: " + deny_cats.join(", ") + "\n";
        }
    }

    output += "\nBase Stock (infinite):\n";
    let stock = mobile.shop_stock;
    let sell_rate = mobile.shop_sell_rate;
    if stock.len() == 0 {
        output += "  (none)\n";
    } else {
        for vnum in stock {
            output += format_shop_stock_line(vnum, sell_rate);
        }
    }

    output += "\nBought Inventory (finite):\n";
    let inventory = get_shop_inventory(mobile_id);
    if inventory.len() == 0 {
        output += "  (none)\n";
    } else {
        for item in inventory {
            output += format_shop_inv_line(item, sell_rate);
        }
    }

    output += "\nCommands:\n";
    output += "  medit " + display_id + " shop stock add|remove <vnum>\n";
    output += "  medit " + display_id + " shop buyrate <0-100>\n";
    output += "  medit " + display_id + " shop sellrate <50+>\n";
    output += "  medit " + display_id + " shop buys all|none|add|remove <type>\n";
    output += "  medit " + display_id + " shop categories add|remove|clear <cat>\n";
    output += "  medit " + display_id + " shop minvalue|maxvalue <amount|clear>\n";
    output += "  medit " + display_id + " shop preset set|clear <vnum>\n";
    output += "  medit " + display_id + " shop preset extra|deny type|category add|remove <val>\n";

    send_client_message(connection_id, output);
}

fn format_type_list(types) {
    if types.len() == 0 { return "(nothing - sell only)"; }
    let has_all = false;
    for t in types {
        if t.to_lower() == "all" { has_all = true; }
    }
    if has_all { return "all item types"; }
    return types.join(", ");
}

fn handle_shop_stock(mobile_id, parts, connection_id) {
    // medit <id> shop stock add <vnum>
    // medit <id> shop stock remove <vnum>

    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " shop stock add|remove <vnum>");
        return;
    }

    let action = parts[3].to_lower();
    let vnum = parts[4];

    if action == "add" {
        // Verify prototype exists
        let proto = get_item_by_vnum(vnum);
        if proto == () {
            send_client_message(connection_id, "Item prototype not found: " + vnum);
            return;
        }
        if !proto.is_prototype {
            send_client_message(connection_id, "Item '" + vnum + "' is not marked as a prototype.");
            return;
        }

        if add_shop_stock(mobile_id, vnum) {
            send_client_message(connection_id, "Added '" + proto.name + "' to shop stock.");
        } else {
            send_client_message(connection_id, "Failed to add item to stock.");
        }
        return;
    }

    if action == "remove" {
        if remove_shop_stock(mobile_id, vnum) {
            send_client_message(connection_id, "Removed '" + vnum + "' from shop stock.");
        } else {
            send_client_message(connection_id, "Failed to remove item from stock.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: medit " + mobile_id + " shop stock add|remove <vnum>");
}

fn handle_shop_buyrate(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " shop buyrate <0-100>");
        return;
    }

    let rate = try_parse_int(parts[3]);
    if rate == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }

    if rate < 0 || rate > 100 {
        send_client_message(connection_id, "Buy rate must be between 0 and 100.");
        return;
    }

    if set_shop_buy_rate(mobile_id, rate) {
        send_client_message(connection_id, "Buy rate set to " + rate + "%.");
    } else {
        send_client_message(connection_id, "Failed to set buy rate.");
    }
}

fn handle_shop_sellrate(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit " + mobile_id + " shop sellrate <50+>");
        return;
    }

    let rate = try_parse_int(parts[3]);
    if rate == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }

    if rate < 50 {
        send_client_message(connection_id, "Sell rate must be at least 50 (use higher for profit margin).");
        return;
    }

    if set_shop_sell_rate(mobile_id, rate) {
        send_client_message(connection_id, "Sell rate set to " + rate + "%.");
    } else {
        send_client_message(connection_id, "Failed to set sell rate.");
    }
}

fn handle_shop_buys(mobile_id, parts, connection_id) {
    // medit <id> shop buys               - Show what types shop buys
    // medit <id> shop buys all           - Buy any item type
    // medit <id> shop buys none          - Don't buy anything
    // medit <id> shop buys add <type>    - Add type to buy list
    // medit <id> shop buys remove <type> - Remove type from buy list

    let valid_types = ["misc", "armor", "weapon", "container", "liquid_container", "food", "key", "gold"];

    // Show current config if no action specified
    if parts.len() < 4 {
        let current = get_shop_buys_types(mobile_id);
        let msg = "Shop buys: ";
        if current.len() == 0 {
            msg += "(nothing - sell only)";
        } else {
            let has_all = false;
            for t in current {
                if t.to_lower() == "all" {
                    has_all = true;
                }
            }
            if has_all {
                msg += "all item types";
            } else {
                msg += current.join(", ");
            }
        }
        msg += "\n\nValid types: " + valid_types.join(", ");
        msg += "\n\nCommands:";
        msg += "\n  shop buys all           - Buy any item type";
        msg += "\n  shop buys none          - Don't buy anything (sell only)";
        msg += "\n  shop buys add <type>    - Add type to buy list";
        msg += "\n  shop buys remove <type> - Remove type from buy list";
        send_client_message(connection_id, msg);
        return;
    }

    let action = parts[3].to_lower();

    if action == "all" {
        if set_shop_buys_types(mobile_id, ["all"]) {
            send_client_message(connection_id, "Shop will now buy all item types.");
        } else {
            send_client_message(connection_id, "Failed to update shop buy types.");
        }
        return;
    }

    if action == "none" {
        if set_shop_buys_types(mobile_id, []) {
            send_client_message(connection_id, "Shop will no longer buy any items (sell only).");
        } else {
            send_client_message(connection_id, "Failed to update shop buy types.");
        }
        return;
    }

    if action == "add" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: shop buys add <type>");
            send_client_message(connection_id, "Valid types: " + valid_types.join(", "));
            return;
        }
        let item_type = parts[4].to_lower();

        // Validate type
        let type_valid = false;
        for vt in valid_types {
            if vt == item_type {
                type_valid = true;
            }
        }
        if !type_valid {
            send_client_message(connection_id, "Invalid item type: " + item_type);
            send_client_message(connection_id, "Valid types: " + valid_types.join(", "));
            return;
        }

        // Get current types and add new one
        let current = get_shop_buys_types(mobile_id);
        // Remove "all" if present since we're specifying specific types
        let new_types = [];
        for t in current {
            if t.to_lower() != "all" && t.to_lower() != item_type {
                new_types.push(t);
            }
        }
        new_types.push(item_type);

        if set_shop_buys_types(mobile_id, new_types) {
            send_client_message(connection_id, "Shop will now buy '" + item_type + "' items.");
        } else {
            send_client_message(connection_id, "Failed to update shop buy types.");
        }
        return;
    }

    if action == "remove" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: shop buys remove <type>");
            return;
        }
        let item_type = parts[4].to_lower();

        // Get current types and remove specified one
        let current = get_shop_buys_types(mobile_id);
        let new_types = [];
        let found = false;
        for t in current {
            if t.to_lower() == item_type {
                found = true;
            } else {
                new_types.push(t);
            }
        }

        if !found {
            send_client_message(connection_id, "Shop doesn't currently buy '" + item_type + "'.");
            return;
        }

        if set_shop_buys_types(mobile_id, new_types) {
            send_client_message(connection_id, "Shop will no longer buy '" + item_type + "' items.");
        } else {
            send_client_message(connection_id, "Failed to update shop buy types.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: shop buys all|none|add|remove <type>");
}

fn handle_shop_categories(mobile_id, parts, connection_id) {
    // medit <id> shop categories            - Show categories
    // medit <id> shop categories add <cat>  - Add category
    // medit <id> shop categories remove <cat> - Remove category
    // medit <id> shop categories clear      - Clear all

    if parts.len() < 4 {
        let cats = get_shop_buys_categories(mobile_id);
        let msg = "Shop buy categories: ";
        if cats.len() == 0 {
            msg += "(none)";
        } else {
            msg += cats.join(", ");
        }
        msg += "\n\nCommands:";
        msg += "\n  shop categories add <category>";
        msg += "\n  shop categories remove <category>";
        msg += "\n  shop categories clear";
        send_client_message(connection_id, msg);
        return;
    }

    let action = parts[3].to_lower();

    if action == "clear" {
        if set_shop_buys_categories(mobile_id, []) {
            send_client_message(connection_id, "Shop buy categories cleared.");
        } else {
            send_client_message(connection_id, "Failed to clear categories.");
        }
        return;
    }

    if action == "add" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: shop categories add <category>");
            return;
        }
        let cat = parts[4].to_lower();
        let current = get_shop_buys_categories(mobile_id);
        let new_cats = [];
        for c in current {
            if c.to_lower() != cat {
                new_cats.push(c);
            }
        }
        new_cats.push(cat);
        if set_shop_buys_categories(mobile_id, new_cats) {
            send_client_message(connection_id, "Added category '" + cat + "'.");
        } else {
            send_client_message(connection_id, "Failed to add category.");
        }
        return;
    }

    if action == "remove" {
        if parts.len() < 5 {
            send_client_message(connection_id, "Usage: shop categories remove <category>");
            return;
        }
        let cat = parts[4].to_lower();
        let current = get_shop_buys_categories(mobile_id);
        let new_cats = [];
        let found = false;
        for c in current {
            if c.to_lower() == cat {
                found = true;
            } else {
                new_cats.push(c);
            }
        }
        if !found {
            send_client_message(connection_id, "Category '" + cat + "' not found.");
            return;
        }
        if set_shop_buys_categories(mobile_id, new_cats) {
            send_client_message(connection_id, "Removed category '" + cat + "'.");
        } else {
            send_client_message(connection_id, "Failed to remove category.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: shop categories add|remove|clear <category>");
}

fn handle_shop_minvalue(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        let val = get_shop_min_value(mobile_id);
        if val > 0 {
            send_client_message(connection_id, "Min buy value: " + val + " gold");
        } else {
            send_client_message(connection_id, "Min buy value: (none)");
        }
        send_client_message(connection_id, "Usage: shop minvalue <amount|clear>");
        return;
    }

    if parts[3].to_lower() == "clear" {
        if set_shop_min_value(mobile_id, 0) {
            send_client_message(connection_id, "Min value restriction cleared.");
        } else {
            send_client_message(connection_id, "Failed to clear min value.");
        }
        return;
    }

    let val = try_parse_int(parts[3]);
    if val == () || val < 0 {
        send_client_message(connection_id, "Invalid value. Use a positive number or 'clear'.");
        return;
    }
    if set_shop_min_value(mobile_id, val) {
        send_client_message(connection_id, "Min buy value set to " + val + " gold.");
    } else {
        send_client_message(connection_id, "Failed to set min value.");
    }
}

fn handle_shop_maxvalue(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        let val = get_shop_max_value(mobile_id);
        if val > 0 {
            send_client_message(connection_id, "Max buy value: " + val + " gold");
        } else {
            send_client_message(connection_id, "Max buy value: (none)");
        }
        send_client_message(connection_id, "Usage: shop maxvalue <amount|clear>");
        return;
    }

    if parts[3].to_lower() == "clear" {
        if set_shop_max_value(mobile_id, 0) {
            send_client_message(connection_id, "Max value restriction cleared.");
        } else {
            send_client_message(connection_id, "Failed to clear max value.");
        }
        return;
    }

    let val = try_parse_int(parts[3]);
    if val == () || val < 0 {
        send_client_message(connection_id, "Invalid value. Use a positive number or 'clear'.");
        return;
    }
    if set_shop_max_value(mobile_id, val) {
        send_client_message(connection_id, "Max buy value set to " + val + " gold.");
    } else {
        send_client_message(connection_id, "Failed to set max value.");
    }
}

fn handle_shop_preset(mobile_id, parts, connection_id) {
    // medit <id> shop preset                                  - Show preset info
    // medit <id> shop preset set <vnum>                       - Apply preset
    // medit <id> shop preset clear                            - Remove preset
    // medit <id> shop preset extra type add|remove <type>
    // medit <id> shop preset extra category add|remove <cat>
    // medit <id> shop preset deny type add|remove <type>
    // medit <id> shop preset deny category add|remove <cat>

    if parts.len() < 4 {
        show_preset_info(mobile_id, connection_id);
        return;
    }

    dispatch_preset_cmd(mobile_id, parts, connection_id);
}

fn show_preset_info(mobile_id, connection_id) {
    let pvnum = get_shop_preset_vnum(mobile_id);
    if pvnum == "" {
        send_client_message(connection_id, "No preset assigned.");
        return;
    }
    let preset = get_shop_preset(pvnum);
    if preset == () {
        send_client_message(connection_id, "Preset: " + pvnum + " (NOT FOUND!)");
        return;
    }
    let msg = "Preset: " + pvnum + " (" + preset.name + ")\n";
    msg += "  Types: " + format_type_list(preset.buy_types) + "\n";
    if preset.buy_categories.len() > 0 {
        msg += "  Categories: " + preset.buy_categories.join(", ") + "\n";
    }
    if preset.min_value > 0 {
        msg += "  Min Value: " + preset.min_value + "\n";
    }
    if preset.max_value > 0 {
        msg += "  Max Value: " + preset.max_value + "\n";
    }
    send_client_message(connection_id, msg);
}

fn dispatch_preset_cmd(mobile_id, parts, connection_id) {
    let action = parts[3].to_lower();

    if action == "set" {
        handle_preset_set(mobile_id, parts, connection_id);
        return;
    }
    if action == "clear" {
        handle_preset_clear(mobile_id, connection_id);
        return;
    }
    if action == "extra" {
        handle_shop_preset_override(mobile_id, parts, connection_id, "extra");
        return;
    }
    if action == "deny" {
        handle_shop_preset_override(mobile_id, parts, connection_id, "deny");
        return;
    }
    send_client_message(connection_id, "Usage: shop preset set|clear|extra|deny ...");
}

fn handle_preset_set(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: shop preset set <vnum>");
        return;
    }
    let vnum = parts[4].to_lower();
    let preset = get_shop_preset(vnum);
    if preset == () {
        send_client_message(connection_id, "Buy preset '" + vnum + "' not found. Use 'bpredit list' to see available presets.");
        return;
    }
    if set_shop_preset_vnum(mobile_id, vnum) {
        send_client_message(connection_id, "Preset set to '" + vnum + "' (" + preset.name + ").");
    } else {
        send_client_message(connection_id, "Failed to set preset.");
    }
}

fn handle_preset_clear(mobile_id, connection_id) {
    if set_shop_preset_vnum(mobile_id, "") {
        send_client_message(connection_id, "Preset cleared.");
    } else {
        send_client_message(connection_id, "Failed to clear preset.");
    }
}

fn get_override_list(mobile_id, mode, kind) {
    if mode == "extra" && kind == "type" {
        return get_shop_extra_types(mobile_id);
    }
    if mode == "extra" && kind == "category" {
        return get_shop_extra_categories(mobile_id);
    }
    if mode == "deny" && kind == "type" {
        return get_shop_deny_types(mobile_id);
    }
    if mode == "deny" && kind == "category" {
        return get_shop_deny_categories(mobile_id);
    }
    return [];
}

fn do_override_add(mobile_id, connection_id, mode, kind, value) {
    let current = get_override_list(mobile_id, mode, kind);
    let new_list = [];
    for c in current {
        if c.to_lower() != value { new_list.push(c); }
    }
    new_list.push(value);
    let result = save_preset_override_list(mobile_id, mode, kind, new_list);
    if result {
        send_client_message(connection_id, "Added " + mode + " " + kind + " '" + value + "'.");
    } else {
        send_client_message(connection_id, "Failed to update.");
    }
}

fn do_override_remove(mobile_id, connection_id, mode, kind, value) {
    let current = get_override_list(mobile_id, mode, kind);
    let new_list = [];
    let found = false;
    for c in current {
        if c.to_lower() == value {
            found = true;
        } else {
            new_list.push(c);
        }
    }
    if !found {
        send_client_message(connection_id, "'" + value + "' not found in " + mode + " " + kind + " list.");
        return;
    }
    let result = save_preset_override_list(mobile_id, mode, kind, new_list);
    if result {
        send_client_message(connection_id, "Removed " + mode + " " + kind + " '" + value + "'.");
    } else {
        send_client_message(connection_id, "Failed to update.");
    }
}

fn handle_shop_preset_override(mobile_id, parts, connection_id, mode) {
    // parts: [<id>, "shop", "preset", "extra"|"deny", "type"|"category", "add"|"remove", <value>]
    if parts.len() < 7 {
        send_client_message(connection_id, "Usage: shop preset " + mode + " type|category add|remove <value>");
        return;
    }

    let kind = parts[4].to_lower();
    let action = parts[5].to_lower();
    let value = parts[6].to_lower();

    if kind != "type" && kind != "category" {
        send_client_message(connection_id, "Must specify 'type' or 'category'.");
        return;
    }

    if action == "add" { do_override_add(mobile_id, connection_id, mode, kind, value); return; }
    if action == "remove" { do_override_remove(mobile_id, connection_id, mode, kind, value); return; }
    send_client_message(connection_id, "Must specify 'add' or 'remove'.");
}

fn save_preset_override_list(mobile_id, mode, kind, list) {
    if mode == "extra" && kind == "type" {
        return set_shop_extra_types(mobile_id, list);
    }
    if mode == "extra" && kind == "category" {
        return set_shop_extra_categories(mobile_id, list);
    }
    if mode == "deny" && kind == "type" {
        return set_shop_deny_types(mobile_id, list);
    }
    if mode == "deny" && kind == "category" {
        return set_shop_deny_categories(mobile_id, list);
    }
    return false;
}

fn format_shop_stock_line(vnum, sell_rate) {
    let proto = get_item_by_vnum(vnum);
    if proto != () {
        let price = calculate_sell_price(proto.value, sell_rate);
        return "  " + vnum + " - " + proto.name + " (" + price + " gold)\n";
    } else {
        return "  " + vnum + " - (prototype not found!)\n";
    }
}

fn format_shop_inv_line(item, sell_rate) {
    let price = calculate_sell_price(item.value, sell_rate);
    return "  " + item.name + " (" + price + " gold)\n";
}

// === Healer Subcommands ===

fn handle_healer_subcommand(mobile, mobile_id, parts, connection_id) {
    // medit <id> healer                  - Show healer config
    // medit <id> healer type <type>      - Set healer type (medic/herbalist/cleric)
    // medit <id> healer free on|off      - Toggle free healing
    // medit <id> healer cost <percent>   - Set cost multiplier (100 = normal)

    if parts.len() < 3 {
        show_healer_config(mobile, mobile_id, connection_id);
        return;
    }

    let healer_cmd = parts[2].to_lower();

    if healer_cmd == "type" {
        handle_healer_type(mobile_id, parts, connection_id);
        return;
    }

    if healer_cmd == "free" {
        handle_healer_free(mobile_id, parts, connection_id);
        return;
    }

    if healer_cmd == "cost" {
        handle_healer_cost(mobile_id, parts, connection_id);
        return;
    }

    show_healer_config(mobile, mobile_id, connection_id);
}

fn show_healer_config(mobile, mobile_id, connection_id) {
    // Use vnum if available for cleaner command examples
    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let output = olc::c_header("=== Healer Configuration for " + mobile.name + " ===", connection_id) + "\n\n";

    // Check if healer flag is enabled
    let flags = mobile.flags;
    if !flags.healer {
        output += olc::c_warn("*** WARNING: 'healer' flag is OFF ***", connection_id) + "\n";
        output += "Enable with: medit " + display_id + " flag healer on\n\n";
    }

    // Healer type
    let healer_type = mobile.healer_type;
    if healer_type == "" {
        healer_type = "(not set)";
    }
    output += "Healer Type: " + healer_type + "\n";

    // Type descriptions
    output += "  - medic: Treats physical wounds (cuts, punctures, bruises, fractures) and bleeding\n";
    output += "  - herbalist: Treats elemental wounds (burns, frostbite), cures poison and illness\n";
    output += "  - cleric: Restores HP, revives unconscious players, full healing\n\n";

    // Free healing
    let free_status = "No (charges gold)";
    if mobile.healing_free {
        free_status = "Yes (free healing)";
    }
    output += "Free Healing: " + free_status + "\n";

    // Cost multiplier
    output += "Cost Multiplier: " + mobile.healing_cost_multiplier + "%";
    if mobile.healing_cost_multiplier == 100 {
        output += " (normal prices)";
    } else if mobile.healing_cost_multiplier < 100 {
        output += " (discounted)";
    } else {
        output += " (premium)";
    }
    output += "\n\n";

    // Service pricing based on type
    if healer_type != "(not set)" {
        output += "--- Services & Pricing ---\n";
        let mult = mobile.healing_cost_multiplier;
        if mobile.healing_free {
            mult = 0;
        }

        if healer_type.to_lower() == "medic" {
            output += format_service_line("Minor wound treatment", 25, mult);
            output += format_service_line("Moderate wound treatment", 75, mult);
            output += format_service_line("Severe wound treatment", 200, mult);
            output += format_service_line("Stop bleeding", 50, mult);
        }
        if healer_type.to_lower() == "herbalist" {
            output += format_service_line("Minor wound treatment", 25, mult);
            output += format_service_line("Moderate wound treatment", 75, mult);
            output += format_service_line("Severe wound treatment", 200, mult);
            output += format_service_line("Cure poison", 100, mult);
            output += format_service_line("Cure illness", 150, mult);
            output += format_service_line("Treat burn/frostbite", 75, mult);
        }
        if healer_type.to_lower() == "cleric" {
            output += format_service_line("Restore HP (per 10 HP)", 20, mult);
            output += format_service_line("Full heal", 300, mult);
            output += format_service_line("Revive unconscious", 500, mult);
        }
        output += "\n";
    }

    output += "--- Commands ---\n";
    output += "medit " + display_id + " healer type <medic|herbalist|cleric>\n";
    output += "medit " + display_id + " healer free on|off\n";
    output += "medit " + display_id + " healer cost <percent>\n";

    send_client_message(connection_id, output);
}

fn format_service_line(service, base_cost, multiplier) {
    if multiplier == 0 {
        return "  " + service + ": FREE\n";
    }
    let price = (base_cost * multiplier) / 100;
    return "  " + service + ": " + price + " gold\n";
}

fn handle_healer_type(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> healer type <medic|herbalist|cleric>");
        return;
    }

    let healer_type = parts[3].to_lower();
    if healer_type != "medic" && healer_type != "herbalist" && healer_type != "cleric" {
        send_client_message(connection_id, "Invalid healer type. Use: medic, herbalist, or cleric");
        return;
    }

    if set_healer_type(mobile_id, healer_type) {
        send_client_message(connection_id, "Healer type set to: " + healer_type);
    } else {
        send_client_message(connection_id, "Failed to set healer type.");
    }
}

fn handle_healer_free(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> healer free on|off");
        return;
    }

    let setting = parts[3].to_lower();
    let free = false;
    if setting == "on" || setting == "true" || setting == "yes" || setting == "1" {
        free = true;
    }

    if set_healing_free(mobile_id, free) {
        if free {
            send_client_message(connection_id, "Healer now provides FREE healing.");
        } else {
            send_client_message(connection_id, "Healer now charges for healing.");
        }
    } else {
        send_client_message(connection_id, "Failed to set healing free status.");
    }
}

fn handle_healer_cost(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> healer cost <percent>");
        send_client_message(connection_id, "Example: 100 = normal prices, 50 = half price, 200 = double");
        return;
    }

    let cost = try_parse_int(parts[3]);
    if cost == () || cost < 0 {
        send_client_message(connection_id, "Cost must be a positive number (percent of base price).");
        return;
    }

    if set_healing_cost_multiplier(mobile_id, cost) {
        send_client_message(connection_id, "Healing cost multiplier set to " + cost + "%.");
    } else {
        send_client_message(connection_id, "Failed to set healing cost multiplier.");
    }
}

// === Trigger Subcommands ===

fn handle_trigger_subcommand(mobile, mobile_id, display_id, parts, connection_id) {
    // medit <id> trigger <action> [args...]
    // actions: list, add, remove, enable, disable, chance
    // `mobile_id` is the mobile UUID used by setter helpers; `display_id`
    // is what the user typed (vnum or keyword) — echoed back in help.

    if parts.len() < 3 {
        show_trigger_help(display_id, connection_id);
        return;
    }

    let action = parts[2].to_lower();

    if action == "list" {
        list_mobile_triggers(mobile, mobile_id, connection_id);
        return;
    }

    if action == "add" {
        add_trigger(mobile_id, parts, connection_id);
        return;
    }

    if action == "remove" {
        remove_trigger(mobile_id, parts, connection_id);
        return;
    }

    if action == "enable" {
        set_trigger_state(mobile_id, parts, true, connection_id);
        return;
    }

    if action == "disable" {
        set_trigger_state(mobile_id, parts, false, connection_id);
        return;
    }

    if action == "chance" {
        set_trigger_chance_cmd(mobile_id, parts, connection_id);
        return;
    }

    if action == "interval" {
        set_trigger_interval_cmd(mobile_id, parts, connection_id);
        return;
    }

    if action == "test" {
        test_trigger(mobile_id, parts, connection_id);
        return;
    }

    if action == "view" {
        view_trigger_script(parts, connection_id);
        return;
    }

    if action == "dg" {
        let dg_action = if parts.len() > 3 { parts[3] } else { "" };
        let dg_rest = [];
        let i = 4;
        while i < parts.len() { dg_rest.push(parts[i]); i += 1; }
        dg::handle("mobile", mobile_id, display_id, dg_action, dg_rest, connection_id);
        return;
    }

    show_trigger_help(display_id, connection_id);
}

fn show_trigger_help(mobile_id, connection_id) {
    let output = "NPC Trigger Commands:\n";
    output += "  medit " + mobile_id + " trigger list\n";
    output += "  medit " + mobile_id + " trigger add <type> <script>\n";
    output += "  medit " + mobile_id + " trigger remove <index>\n";
    output += "  medit " + mobile_id + " trigger enable <index>\n";
    output += "  medit " + mobile_id + " trigger disable <index>\n";
    output += "  medit " + mobile_id + " trigger chance <index> <1-100>\n";
    output += "  medit " + mobile_id + " trigger interval <index> <secs>\n";
    output += "  medit " + mobile_id + " trigger test <index>\n";
    output += "  medit " + mobile_id + " trigger view <script_name>\n";
    output += "\nTrigger types: greet, attack, death, say, idle, always, flee\n";
    output += "\nScript triggers: scripts/triggers/<script>.rhai\n";
    output += "  Available: " + list_available_trigger_scripts().join(", ") + "\n";
    output += "\nTemplate triggers (no script needed):\n";
    output += "  @say_greeting <message>      - NPC says message\n";
    output += "  @say_random <msg1|msg2|...>  - NPC says random message\n";
    output += "  @emote <action>              - NPC performs action\n";
    output += "  @shout <message>             - NPC shouts message to room + adjacent\n";
    output += "\nIdle triggers fire periodically when players are present.\n";
    output += "Always triggers fire periodically regardless of player presence.\n";
    output += "Use 'trigger interval' to set how often (default 60s).\n";
    output += "\nNote: attack triggers fire when combat begins; death triggers fire before the mobile is removed.\n";
    output += "Flee triggers fire when the mobile flees from combat.\n";
    output += "\nDG Scripts (imported tbamud/circlemud triggers):\n";
    output += "  medit " + mobile_id + " trigger dg list\n";
    output += "  medit " + mobile_id + " trigger dg view <index>\n";
    output += "  medit " + mobile_id + " trigger dg add <type> [name]\n";
    output += "  medit " + mobile_id + " trigger dg edit <index>\n";
    output += "  medit " + mobile_id + " trigger dg attach <vnum>\n";
    output += "  medit " + mobile_id + " trigger dg elevate <index> [on|off]  (admin)\n";
    output += "  medit " + mobile_id + " trigger dg protos\n";
    send_client_message(connection_id, output);
}

fn test_trigger(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> trigger test <index>");
        return;
    }
    let idx = try_parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }

    send_client_message(connection_id, "Testing trigger " + idx + "...");

    let result = test_mobile_trigger(mobile_id, idx, connection_id);

    if result.success {
        send_client_message(connection_id, olc::c_header("Trigger executed successfully!", connection_id));
        send_client_message(connection_id, "Result: " + result.result);
    } else {
        send_client_message(connection_id, olc::c_warn("Trigger test failed: ", connection_id) + result.error);
    }
}

fn view_trigger_script(parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> trigger view <script_name>\nAvailable: " + list_available_trigger_scripts().join(", "));
        return;
    }
    let script_name = parts[3];
    let result = get_trigger_script_content(script_name);
    if result.success {
        let output = olc::c_header("=== Script: " + script_name + ".rhai ===", connection_id) + "\n";
        output += result.content;
        send_client_message(connection_id, output);
    } else {
        send_client_message(connection_id, olc::c_warn("Error: ", connection_id) + result.error);
        send_client_message(connection_id, "Available scripts: " + list_available_trigger_scripts().join(", "));
    }
}

fn format_template_args(targs) {
    if targs.len() == 1 {
        return " \"" + targs[0] + "\"";
    }
    if targs.len() > 1 {
        let joined = "";
        let i = 0;
        for arg in targs {
            if i > 0 { joined += "|"; }
            joined += arg;
            i += 1;
        }
        return " [" + joined + "]";
    }
    return "";
}

fn format_trigger_line(trigger, idx, connection_id) {
    let enabled_str = olc::c_warn("[ON] ", connection_id);
    if !trigger.enabled { enabled_str = "[OFF]"; }
    let ttype = get_mobile_trigger_type(trigger);
    let line = idx + ": " + enabled_str + " " + ttype + " -> " + trigger.script_name;
    if trigger.script_name.starts_with("@") {
        line += format_template_args(trigger.args);
    }
    if trigger.chance < 100 {
        line += " (" + trigger.chance + "% chance)";
    }
    // Show interval for idle/always triggers
    if ttype == "on_idle" || ttype == "on_always" {
        line += " [every " + trigger.interval_secs + "s]";
    }
    return line;
}

fn list_mobile_triggers(mobile, mobile_id, connection_id) {
    let triggers = get_mobile_triggers(mobile_id);
    if triggers.len() == 0 {
        send_client_message(connection_id, "No triggers on this mobile.");
        return;
    }

    let output = olc::c_header("=== Triggers on " + mobile.name + " ===", connection_id) + "\n";
    let idx = 0;
    for trigger in triggers {
        output += format_trigger_line(trigger, idx, connection_id) + "\n";
        idx += 1;
    }
    send_client_message(connection_id, output);
}

fn show_add_trigger_usage(connection_id) {
    let msg = "Usage: medit <id> trigger add <type> <script|@template> [args...]\n";
    msg += "Types: greet, attack, death, say, idle, always, flee\n";
    msg += "Templates: @say_greeting, @say_random, @emote, @shout";
    send_client_message(connection_id, msg);
}

fn is_valid_mobile_trigger_type(trigger_type) {
    trigger_type == "greet" || trigger_type == "attack" || trigger_type == "death" || trigger_type == "say" || trigger_type == "idle" || trigger_type == "always" || trigger_type == "flee"
}

fn add_template_trigger(mobile_id, trigger_type, script_name, parts, connection_id) {
    let template_name = script_name.sub_string(1);  // Remove @ prefix

    // Collect remaining parts as args
    let args = [];
    if parts.len() > 5 {
        let i = 5;
        while i < parts.len() {
            args.push(parts[i]);
            i += 1;
        }
    }

    // For @say_random, parse pipe-separated values from first arg
    if template_name == "say_random" && args.len() > 0 {
        let first_arg = args[0];
        if first_arg.contains("|") {
            args = first_arg.split("|");
        }
    }

    // Combine remaining args for templates that take a message
    if template_name == "say_greeting" || template_name == "emote" {
        if args.len() > 0 {
            let combined = "";
            let i = 0;
            for arg in args {
                if i > 0 { combined += " "; }
                combined += arg;
                i += 1;
            }
            args = [combined];
        }
    }

    if args.len() == 0 {
        send_client_message(connection_id, "Template " + script_name + " requires arguments.");
        return;
    }

    if add_mobile_trigger_with_args(mobile_id, trigger_type, script_name, args) {
        send_client_message(connection_id, "Added " + trigger_type + " template: " + script_name);
    } else {
        send_client_message(connection_id, "Failed to add template trigger.");
    }
}

fn add_trigger(mobile_id, parts, connection_id) {
    // medit <id> trigger add <type> <script|@template> [args...]
    if parts.len() < 5 {
        show_add_trigger_usage(connection_id);
        return;
    }
    let trigger_type = parts[3].to_lower();
    let script_name = parts[4];

    if !is_valid_mobile_trigger_type(trigger_type) {
        send_client_message(connection_id, "Invalid trigger type. Use: greet, attack, death, say, idle, always, flee");
        return;
    }

    // Check if template (starts with @)
    if script_name.starts_with("@") {
        add_template_trigger(mobile_id, trigger_type, script_name, parts, connection_id);
        return;
    }

    // Regular script - validate before adding
    let validation = validate_trigger_script(script_name);
    if !validation.valid {
        send_client_message(connection_id, olc::c_warn("Script validation failed: ", connection_id) + validation.error);
        let scripts = list_available_trigger_scripts();
        send_client_message(connection_id, "Available scripts: " + scripts.join(", "));
        return;
    }

    if add_mobile_trigger(mobile_id, trigger_type, script_name) {
        send_client_message(connection_id, "Added " + trigger_type + " trigger: " + script_name);
    } else {
        send_client_message(connection_id, "Failed to add trigger.");
    }
}

fn remove_trigger(mobile_id, parts, connection_id) {
    // medit <id> trigger remove <index>
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> trigger remove <index>");
        return;
    }
    let idx = try_parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }
    if remove_mobile_trigger(mobile_id, idx) {
        send_client_message(connection_id, "Removed trigger at index " + idx + ".");
    } else {
        send_client_message(connection_id, "Failed to remove trigger. Check the index.");
    }
}

fn set_trigger_state(mobile_id, parts, enabled, connection_id) {
    // medit <id> trigger enable/disable <index>
    if parts.len() < 4 {
        let action = "enable";
        if !enabled { action = "disable"; }
        send_client_message(connection_id, "Usage: medit <id> trigger " + action + " <index>");
        return;
    }
    let idx = try_parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }
    if set_mobile_trigger_enabled(mobile_id, idx, enabled) {
        let state = "enabled";
        if !enabled { state = "disabled"; }
        send_client_message(connection_id, "Trigger at index " + idx + " " + state + ".");
    } else {
        send_client_message(connection_id, "Failed to update trigger. Check the index.");
    }
}

fn set_trigger_chance_cmd(mobile_id, parts, connection_id) {
    // medit <id> trigger chance <index> <1-100>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> trigger chance <index> <1-100>");
        return;
    }
    let idx = try_parse_int(parts[3]);
    let chance = try_parse_int(parts[4]);
    if idx == () || chance == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if chance < 1 || chance > 100 {
        send_client_message(connection_id, "Chance must be between 1 and 100.");
        return;
    }
    if set_mobile_trigger_chance(mobile_id, idx, chance) {
        send_client_message(connection_id, "Trigger at index " + idx + " now has " + chance + "% chance to fire.");
    } else {
        send_client_message(connection_id, "Failed to update trigger. Check the index.");
    }
}

fn set_trigger_interval_cmd(mobile_id, parts, connection_id) {
    // medit <id> trigger interval <index> <seconds>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> trigger interval <index> <seconds>");
        return;
    }
    let idx = try_parse_int(parts[3]);
    let secs = try_parse_int(parts[4]);
    if idx == () || secs == () {
        send_client_message(connection_id, "Invalid numbers.");
        return;
    }
    if secs < 1 {
        send_client_message(connection_id, "Interval must be at least 1 second.");
        return;
    }
    if set_mobile_trigger_interval(mobile_id, idx, secs) {
        send_client_message(connection_id, "Trigger at index " + idx + " now has " + secs + " second interval.");
    } else {
        send_client_message(connection_id, "Failed to update trigger. Check the index.");
    }
}

fn build_triggers_display(mobile_id) {
    let triggers = get_mobile_triggers(mobile_id);
    if triggers.len() == 0 {
        return "Triggers: (none)\n";
    }
    let output = "Triggers:\n";
    let idx = 0;
    for trigger in triggers {
        let enabled_str = "[ON] ";
        if !trigger.enabled { enabled_str = "[OFF]"; }
        let ttype = get_mobile_trigger_type(trigger);
        output += "  " + idx + ": " + enabled_str + " " + ttype + " -> " + trigger.script_name;
        if trigger.chance < 100 {
            output += " (" + trigger.chance + "%)";
        }
        // Show interval for idle/always triggers
        if ttype == "on_idle" || ttype == "on_always" {
            output += " [every " + trigger.interval_secs + "s]";
        }
        output += "\n";
        idx += 1;
    }
    return output;
}

// === Transport Route Functions ===

fn handle_transport_subcommand(mobile, mobile_id, parts, connection_id) {
    // medit <id> transport                                  - Show transport config
    // medit <id> transport set <vnum> <home> <dest>         - Set route (default fixed 8-17)
    // medit <id> transport fixed <depart> <return>          - Set fixed hours schedule
    // medit <id> transport random <chance>                  - Set random schedule
    // medit <id> transport permanent                        - Set permanent (conductor)
    // medit <id> transport clear                            - Remove route

    if parts.len() < 3 {
        show_transport_route(mobile, mobile_id, connection_id);
        return;
    }

    let transport_cmd = parts[2].to_lower();

    if transport_cmd == "set" {
        handle_transport_set(mobile_id, parts, connection_id);
        return;
    }
    if transport_cmd == "fixed" {
        handle_transport_fixed(mobile_id, parts, connection_id);
        return;
    }
    if transport_cmd == "random" {
        handle_transport_random(mobile_id, parts, connection_id);
        return;
    }
    if transport_cmd == "permanent" {
        handle_transport_permanent(mobile_id, connection_id);
        return;
    }
    if transport_cmd == "clear" {
        handle_transport_clear(mobile_id, connection_id);
        return;
    }

    show_transport_route(mobile, mobile_id, connection_id);
}

fn show_transport_route(mobile, mobile_id, connection_id) {
    let display_id = get_display_id(mobile, mobile_id);

    let output = olc::c_header("=== Transport Route for " + mobile.name + " ===", connection_id) + "\n\n";

    let route = get_mobile_transport_route(mobile.id);
    if route == () {
        output += "No transport route configured.\n\n";
        output += "Commands:\n";
        output += "  medit " + display_id + " transport set <transport_vnum> <home_stop> <dest_stop>\n";
        output += "    Sets up a route with default fixed hours (8-17)\n";
        send_client_message(connection_id, output);
        return;
    }

    // Get transport info
    let transport = get_transport_data(route.transport_id);
    let transport_name = "UNKNOWN";
    let transport_vnum = route.transport_id;
    if transport != () {
        transport_name = transport.name;
        if transport.vnum != () && transport.vnum != "" {
            transport_vnum = transport.vnum;
        }
    }

    output += olc::c_label("Transport: ", connection_id) + olc::c_value(transport_name + " (" + transport_vnum + ")", connection_id) + "\n";
    output += olc::c_label("Home Stop: ", connection_id) + olc::c_value("" + route.home_stop, connection_id);
    output += show_stop_name(transport, route.home_stop) + "\n";
    output += olc::c_label("Destination: ", connection_id) + olc::c_value("" + route.dest_stop, connection_id);
    output += show_stop_name(transport, route.dest_stop) + "\n";

    output += olc::c_label("Schedule: ", connection_id);
    output += format_schedule(route) + "\n";

    output += olc::c_label("Current State: ", connection_id);
    output += format_travel_state(route) + "\n";

    output += "\nCommands:\n";
    output += "  medit " + display_id + " transport fixed <depart_hour> <return_hour>\n";
    output += "  medit " + display_id + " transport random <chance_per_hour>\n";
    output += "  medit " + display_id + " transport permanent\n";
    output += "  medit " + display_id + " transport clear\n";

    send_client_message(connection_id, output);
}

fn show_stop_name(transport, stop_index) {
    if transport == () { return ""; }
    if stop_index < 0 || stop_index >= transport.stops.len() { return ""; }
    return " (" + transport.stops[stop_index].name + ")";
}

fn format_schedule(route) {
    if route.schedule_type == "fixed" {
        return "Fixed Hours (depart: " + route.depart_hour + ":00, return: " + route.return_hour + ":00)";
    }
    if route.schedule_type == "random" {
        return "Random (" + route.chance + "% chance per hour)";
    }
    if route.schedule_type == "permanent" {
        return "Permanent (always on transport)";
    }
    return "Unknown";
}

fn format_travel_state(route) {
    if route.is_on_transport {
        return "Currently riding transport";
    }
    if route.is_at_destination {
        return "At destination";
    }
    return "At home";
}

fn get_display_id(mobile, mobile_id) {
    if mobile.vnum != () && mobile.vnum != "" {
        return mobile.vnum;
    }
    return mobile_id;
}

fn handle_transport_set(mobile_id, parts, connection_id) {
    // medit <id> transport set <transport_vnum> <home_stop> <dest_stop>
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> transport set <transport_vnum> <home_stop> <dest_stop>");
        return;
    }

    let transport_ref = parts[3];
    let home_stop = try_parse_int(parts[4]);
    let dest_stop = try_parse_int(parts[5]);

    if home_stop == () || dest_stop == () {
        send_client_message(connection_id, "Stop indices must be numbers.");
        return;
    }

    // Find transport by vnum or ID
    let transport = get_transport_by_vnum(transport_ref);
    if transport == () {
        transport = get_transport_data(transport_ref);
    }
    if transport == () {
        send_client_message(connection_id, "Transport '" + transport_ref + "' not found. Use 'tlist' to see available transports.");
        return;
    }

    // Validate stop indices
    if home_stop < 0 || home_stop >= transport.stops.len() {
        send_client_message(connection_id, "Invalid home stop index. Transport has " + transport.stops.len() + " stops (0-" + (transport.stops.len() - 1) + ").");
        return;
    }
    if dest_stop < 0 || dest_stop >= transport.stops.len() {
        send_client_message(connection_id, "Invalid destination stop index. Transport has " + transport.stops.len() + " stops (0-" + (transport.stops.len() - 1) + ").");
        return;
    }
    if home_stop == dest_stop {
        send_client_message(connection_id, "Home and destination stops must be different.");
        return;
    }

    // Set route with default fixed schedule (8-17)
    let result = set_mobile_transport_route(mobile_id, transport.id, home_stop, dest_stop, "fixed", 8, 17);
    if result {
        send_client_message(connection_id, "Transport route set: " + transport.name);
        send_client_message(connection_id, "  Home: " + transport.stops[home_stop].name + " (stop " + home_stop + ")");
        send_client_message(connection_id, "  Dest: " + transport.stops[dest_stop].name + " (stop " + dest_stop + ")");
        send_client_message(connection_id, "  Schedule: Fixed hours 8:00-17:00 (use 'transport fixed/random/permanent' to change)");
    } else {
        send_client_message(connection_id, "Failed to set transport route.");
    }
}

fn handle_transport_fixed(mobile_id, parts, connection_id) {
    // medit <id> transport fixed <depart_hour> <return_hour>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> transport fixed <depart_hour> <return_hour>");
        send_client_message(connection_id, "Example: medit goblin transport fixed 8 17  (depart at 8, return at 17)");
        return;
    }

    let route = get_mobile_transport_route(mobile_id);
    if route == () {
        send_client_message(connection_id, "No transport route configured. Use 'transport set' first.");
        return;
    }

    let depart = try_parse_int(parts[3]);
    let return_hour = try_parse_int(parts[4]);

    if depart == () || return_hour == () {
        send_client_message(connection_id, "Hours must be numbers (0-23).");
        return;
    }
    if depart < 0 || depart > 23 || return_hour < 0 || return_hour > 23 {
        send_client_message(connection_id, "Hours must be 0-23.");
        return;
    }

    let result = set_mobile_transport_route(mobile_id, route.transport_id, route.home_stop, route.dest_stop, "fixed", depart, return_hour);
    if result {
        send_client_message(connection_id, "Schedule set to Fixed Hours: depart at " + depart + ":00, return at " + return_hour + ":00");
    } else {
        send_client_message(connection_id, "Failed to update schedule.");
    }
}

fn handle_transport_random(mobile_id, parts, connection_id) {
    // medit <id> transport random <chance>
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> transport random <chance_per_hour>");
        send_client_message(connection_id, "Example: medit goblin transport random 25  (25% chance to travel each hour)");
        return;
    }

    let route = get_mobile_transport_route(mobile_id);
    if route == () {
        send_client_message(connection_id, "No transport route configured. Use 'transport set' first.");
        return;
    }

    let chance = try_parse_int(parts[3]);
    if chance == () || chance < 1 || chance > 100 {
        send_client_message(connection_id, "Chance must be 1-100.");
        return;
    }

    let result = set_mobile_transport_route(mobile_id, route.transport_id, route.home_stop, route.dest_stop, "random", chance, 0);
    if result {
        send_client_message(connection_id, "Schedule set to Random: " + chance + "% chance to travel each hour");
    } else {
        send_client_message(connection_id, "Failed to update schedule.");
    }
}

fn handle_transport_permanent(mobile_id, connection_id) {
    let route = get_mobile_transport_route(mobile_id);
    if route == () {
        send_client_message(connection_id, "No transport route configured. Use 'transport set' first.");
        return;
    }

    let result = set_mobile_transport_route(mobile_id, route.transport_id, route.home_stop, route.dest_stop, "permanent", 0, 0);
    if result {
        send_client_message(connection_id, "Schedule set to Permanent (NPC will always stay on transport, like a conductor)");
    } else {
        send_client_message(connection_id, "Failed to update schedule.");
    }
}

fn handle_transport_clear(mobile_id, connection_id) {
    let result = clear_mobile_transport_route(mobile_id);
    if result {
        send_client_message(connection_id, "Transport route cleared.");
    } else {
        send_client_message(connection_id, "No transport route to clear (or failed to clear).");
    }
}

// === Leasing Agent Functions ===

fn handle_leasing_subcommand(mobile, mobile_id, parts, connection_id) {
    // medit <id> leasing                      - Show leasing config
    // medit <id> leasing add <template_vnum>  - Add template
    // medit <id> leasing remove <template_vnum> - Remove template

    if parts.len() < 3 {
        show_leasing_config(mobile, mobile_id, connection_id);
        return;
    }

    let leasing_cmd = parts[2].to_lower();

    if leasing_cmd == "add" {
        handle_leasing_add(mobile_id, parts, connection_id);
        return;
    }

    if leasing_cmd == "remove" {
        handle_leasing_remove(mobile_id, parts, connection_id);
        return;
    }

    show_leasing_config(mobile, mobile_id, connection_id);
}

fn format_template_line(vnum) {
    let template = get_property_template(vnum);
    if template == () { return "  - " + vnum + " (template not found!)\n"; }
    return "  - " + vnum + " (" + template.name + ") - " + template.monthly_rent + " gold/month\n";
}

fn show_leasing_config(mobile, mobile_id, connection_id) {
    let display_id = get_display_id(mobile, mobile_id);
    let output = olc::c_header("=== Leasing Agent Configuration for " + mobile.name + " ===", connection_id) + "\n\n";

    let flags = mobile.flags;
    if !flags.leasing_agent {
        output += olc::c_warn("*** WARNING: 'leasing_agent' flag is OFF ***", connection_id) + "\n";
        output += "Enable with: medit " + display_id + " flag leasing_agent on\n\n";
    }

    output += olc::c_label("Available Property Templates:", connection_id) + "\n";

    let templates = mobile.property_templates;
    if templates.len() == 0 {
        output += "  (none)\n";
    } else {
        for vnum in templates { output += format_template_line(vnum); }
    }

    output += "\n" + olc::c_label("Commands:", connection_id) + "\n";
    output += "  medit " + display_id + " leasing add <template_vnum>  - Add template\n";
    output += "  medit " + display_id + " leasing remove <template_vnum> - Remove template\n";
    send_client_message(connection_id, output);
}

fn handle_leasing_add(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> leasing add <template_vnum>");
        return;
    }

    let vnum = parts[3].to_lower();

    // Verify template exists
    let template = get_property_template(vnum);
    if template == () {
        send_client_message(connection_id, "Property template not found: " + vnum);
        send_client_message(connection_id, "Use 'plist' to see available templates.");
        return;
    }

    if add_agent_property_template(mobile_id, vnum) {
        send_client_message(connection_id, "Added template '" + template.name + "' (" + vnum + ") to available properties.");
    } else {
        send_client_message(connection_id, "Failed to add template.");
    }
}

fn handle_leasing_remove(mobile_id, parts, connection_id) {
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: medit <id> leasing remove <template_vnum>");
        return;
    }

    let vnum = parts[3].to_lower();

    if remove_agent_property_template(mobile_id, vnum) {
        send_client_message(connection_id, "Removed template '" + vnum + "' from available properties.");
    } else {
        send_client_message(connection_id, "Failed to remove template (may not be in list).");
    }
}

// === Daily Routine Commands ===

fn get_time_name(hour) {
    if hour >= 5 && hour < 8 { return "Dawn"; }
    if hour >= 8 && hour < 12 { return "Morning"; }
    if hour >= 12 && hour < 14 { return "Midday"; }
    if hour >= 14 && hour < 18 { return "Afternoon"; }
    if hour >= 18 && hour < 21 { return "Evening"; }
    if hour >= 21 && hour < 24 { return "Night"; }
    return "Late Night";
}

fn format_hour(hour) {
    if hour == 0 { return "12am"; }
    if hour < 12 { return "" + hour + "am"; }
    if hour == 12 { return "12pm"; }
    return "" + (hour - 12) + "pm";
}

fn show_routine(mobile, mobile_id, connection_id) {
    let entries = get_mobile_routine_entries(mobile_id);
    let visible = get_mobile_schedule_visible(mobile_id);
    let activity = get_mobile_activity(mobile_id);

    let display_id = mobile_id;
    if mobile.vnum != () && mobile.vnum != "" {
        display_id = mobile.vnum;
    }

    let output = olc::c_header("=== Daily Routine for " + mobile.name + " ===", connection_id) + "\n";
    output += olc::c_label("Current Activity: ", connection_id) + olc::c_value(activity, connection_id) + "\n";
    output += olc::c_label("Schedule Visible: ", connection_id) + olc::c_value(if visible { "yes" } else { "no" }, connection_id) + "\n\n";

    if entries.len() == 0 {
        output += "(no routine entries)\n";
    } else {
        for entry in entries {
            let hour = entry["start_hour"];
            let act = entry["activity"];
            let dest = entry["destination_vnum"];
            let msg = entry["transition_message"];
            let suppress = entry["suppress_wander"];
            let dialogues = entry["dialogue_overrides"];

            output += olc::c_label(format_hour(hour), connection_id) + " (" + get_time_name(hour) + "): ";
            output += olc::c_value(act, connection_id);
            if dest != "" && dest != () {
                output += " -> " + dest;
            }
            if suppress {
                output += " [no wander]";
            }
            output += "\n";
            if msg != "" && msg != () {
                output += "    Msg: " + msg + "\n";
            }
            let dlg_count = 0;
            for k in dialogues.keys() {
                dlg_count += 1;
            }
            if dlg_count > 0 {
                output += "    Dialogue overrides: " + dlg_count + "\n";
                for kw in dialogues.keys() {
                    output += "      [" + kw + "]: " + dialogues[kw] + "\n";
                }
            }
        }
    }

    output += "\nUse 'medit " + display_id + " routine add <hour> <activity> [dest_vnum]' to add entries.";
    send_client_message(connection_id, output);
}

fn handle_routine_add(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> routine add <hour> <activity> [dest_vnum]");
        send_client_message(connection_id, "Activities: working, sleeping, patrolling, off_duty, socializing, eating, or custom text");
        return;
    }

    let hour = try_parse_int(parts[4]);
    if hour == () || hour < 0 || hour > 23 {
        send_client_message(connection_id, "Hour must be 0-23.");
        return;
    }

    let activity = parts[5];
    let dest = "";
    if parts.len() > 6 {
        dest = parts[6];
    }

    if add_mobile_routine_entry(mobile_id, hour, activity, dest, "", true) {
        let msg = "Added routine entry at " + format_hour(hour) + ": " + activity;
        if dest != "" {
            msg += " -> " + dest;
        }
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Failed to add routine entry. An entry at hour " + hour + " may already exist.");
    }
}

fn handle_routine_remove(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> routine remove <hour>");
        return;
    }

    let hour = try_parse_int(parts[4]);
    if hour == () || hour < 0 || hour > 23 {
        send_client_message(connection_id, "Hour must be 0-23.");
        return;
    }

    if remove_mobile_routine_entry(mobile_id, hour) {
        send_client_message(connection_id, "Removed routine entry at " + format_hour(hour) + ".");
    } else {
        send_client_message(connection_id, "No routine entry found at hour " + hour + ".");
    }
}

fn handle_routine_clear(mobile_id, connection_id) {
    if clear_mobile_routine(mobile_id) {
        send_client_message(connection_id, "Cleared all routine entries.");
    } else {
        send_client_message(connection_id, "Failed to clear routine.");
    }
}

fn handle_routine_preset_list(connection_id) {
    let presets = list_routine_presets();
    if presets.len() == 0 {
        send_client_message(connection_id, "No routine presets available.");
        return;
    }
    let output = olc::c_header("=== Routine Presets ===", connection_id) + "\n";
    for preset in presets {
        output += olc::c_label(preset["name"], connection_id) + " - " + preset["description"] + "\n";
        output += "  Keys: ";
        let keys = [];
        for entry in preset["entries"] {
            let dk = entry["destination_key"];
            if dk != "" && dk != () {
                let found = false;
                for k in keys {
                    if k == dk { found = true; }
                }
                if !found { keys.push(dk); }
            }
        }
        output += olc::c_value(olc::join_array(keys), connection_id) + "\n";
    }
    send_client_message(connection_id, output);
}

fn handle_routine_preset_apply(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> routine preset <name> <key=vnum> [key=vnum ...]");
        send_client_message(connection_id, "Example: medit mob_shopkeeper routine preset merchant_8to20 shop=midgaard_weapon_shop home=midgaard_house1");
        return;
    }

    let preset_name = parts[4];
    let mappings = [];
    for i in 5..parts.len() {
        mappings.push(parts[i]);
    }

    if apply_routine_preset(mobile_id, preset_name, mappings) {
        send_client_message(connection_id, "Applied routine preset '" + preset_name + "'.");
    } else {
        send_client_message(connection_id, "Failed to apply preset. Check that the preset name exists and all required key=vnum mappings are provided.");
    }
}

fn handle_routine_msg(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> routine msg <hour> <message>");
        send_client_message(connection_id, "Use {name} in message for the mobile's name.");
        return;
    }

    let hour = try_parse_int(parts[4]);
    if hour == () || hour < 0 || hour > 23 {
        send_client_message(connection_id, "Hour must be 0-23.");
        return;
    }

    let msg = build_string_from_parts(parts, 5);
    if set_routine_entry_message(mobile_id, hour, msg) {
        send_client_message(connection_id, "Set transition message at " + format_hour(hour) + ": " + msg);
    } else {
        send_client_message(connection_id, "No routine entry found at hour " + hour + ".");
    }
}

fn handle_routine_wander(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> routine wander <hour> on|off");
        return;
    }

    let hour = try_parse_int(parts[4]);
    if hour == () || hour < 0 || hour > 23 {
        send_client_message(connection_id, "Hour must be 0-23.");
        return;
    }

    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> routine wander <hour> on|off");
        return;
    }

    let val = parts[5].to_lower();
    let suppress = true;
    if val == "on" { suppress = false; }
    if val == "off" { suppress = true; }

    if set_routine_entry_wander(mobile_id, hour, suppress) {
        let state = if suppress { "suppressed" } else { "enabled" };
        send_client_message(connection_id, "Wandering " + state + " at " + format_hour(hour) + ".");
    } else {
        send_client_message(connection_id, "No routine entry found at hour " + hour + ".");
    }
}

fn handle_routine_visible(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> routine visible on|off");
        return;
    }

    let val = parts[4].to_lower();
    let visible = false;
    if val == "on" || val == "yes" || val == "true" { visible = true; }

    if set_mobile_schedule_visible(mobile_id, visible) {
        let state = if visible { "visible" } else { "hidden" };
        send_client_message(connection_id, "Schedule is now " + state + " to players.");
    } else {
        send_client_message(connection_id, "Failed to set schedule visibility.");
    }
}

fn handle_routine_dialogue(mobile_id, parts, connection_id) {
    if parts.len() < 7 {
        send_client_message(connection_id, "Usage: medit <id> routine dialogue <hour> <keyword> <response>");
        return;
    }

    let hour = try_parse_int(parts[4]);
    if hour == () || hour < 0 || hour > 23 {
        send_client_message(connection_id, "Hour must be 0-23.");
        return;
    }

    let keyword = parts[5].to_lower();
    let response = build_string_from_parts(parts, 6);

    if set_routine_entry_dialogue(mobile_id, hour, keyword, response) {
        send_client_message(connection_id, "Set dialogue override at " + format_hour(hour) + " [" + keyword + "]: " + response);
    } else {
        send_client_message(connection_id, "No routine entry found at hour " + hour + ".");
    }
}

fn get_routine_action(parts) {
    if parts.len() < 4 { return "show"; }
    return parts[3].to_lower();
}

fn handle_routine_subcommand(mobile, mobile_id, parts, connection_id) {
    let action = get_routine_action(parts);

    if action == "show" || action == "routine" { show_routine(mobile, mobile_id, connection_id); return; }
    if action == "add" { handle_routine_add(mobile_id, parts, connection_id); return; }
    if action == "remove" { handle_routine_remove(mobile_id, parts, connection_id); return; }
    if action == "clear" { handle_routine_clear(mobile_id, connection_id); return; }
    if action == "msg" || action == "message" { handle_routine_msg(mobile_id, parts, connection_id); return; }
    if action == "wander" { handle_routine_wander(mobile_id, parts, connection_id); return; }
    if action == "visible" { handle_routine_visible(mobile_id, parts, connection_id); return; }
    if action == "dialogue" { handle_routine_dialogue(mobile_id, parts, connection_id); return; }
    if action == "preset" {
        if parts.len() < 5 || parts[4].to_lower() == "list" {
            handle_routine_preset_list(connection_id);
        } else {
            handle_routine_preset_apply(mobile_id, parts, connection_id);
        }
        return;
    }

    send_client_message(connection_id, "Unknown routine action: " + action);
    send_client_message(connection_id, "Valid: add, remove, clear, preset, msg, wander, visible, dialogue");
}

// ========== Simulation Subcommand ==========

fn get_sim_action(parts) {
    if parts.len() < 4 { return "show"; }
    return parts[3].to_lower();
}

fn handle_simulation_subcommand(mobile, mobile_id, parts, connection_id) {
    let action = get_sim_action(parts);

    if action == "show" || action == "simulation" || action == "sim" {
        show_simulation_status(mobile, mobile_id, connection_id);
        return;
    }
    if action == "setup" { handle_sim_setup(mobile_id, parts, connection_id); return; }
    if action == "remove" { handle_sim_remove(mobile_id, connection_id); return; }
    if action == "pay" { handle_sim_pay(mobile_id, parts, connection_id); return; }
    if action == "hours" { handle_sim_hours(mobile_id, parts, connection_id); return; }
    if action == "food" { handle_sim_food(mobile_id, parts, connection_id); return; }
    if action == "decay" { handle_sim_decay(mobile_id, parts, connection_id); return; }
    if action == "lowgold" { handle_sim_lowgold(mobile_id, parts, connection_id); return; }

    send_client_message(connection_id, "Unknown simulation action: " + action);
    send_client_message(connection_id, "Valid: setup, remove, pay, hours, food, decay, lowgold");
}

fn show_simulation_status(mobile, mobile_id, connection_id) {
    if !is_npc_simulated(mobile_id) {
        send_client_message(connection_id, "This mobile is not simulated. Use 'medit " + mobile_id + " simulation setup <home> <work> <shop>' to enable.");
        return;
    }

    let config = get_npc_simulation_config(mobile_id);
    let needs = get_npc_needs(mobile_id);

    let output = "\n=== Simulation Config ===\n";
    output += "  Home Room:  " + config["home_room_vnum"] + "\n";
    output += "  Work Room:  " + config["work_room_vnum"] + "\n";
    output += "  Shop Room:  " + config["shop_room_vnum"] + "\n";
    output += "  Food Pref:  " + config["preferred_food_vnum"] + "\n";
    output += "  Work Pay:   " + config["work_pay"] + " gold\n";
    output += "  Work Hours: " + config["work_start_hour"] + ":00 - " + config["work_end_hour"] + ":00\n";
    output += "  Decay Rates: hunger=" + config["hunger_decay_rate"] + " energy=" + config["energy_decay_rate"] + " comfort=" + config["comfort_decay_rate"] + " (0=default, 100=normal)\n";
    output += "  Low Gold:   " + config["low_gold_threshold"] + " (NPC seeks work when gold <= this)\n";

    if needs != () {
        output += "\n=== Current Needs ===\n";
        output += "  Hunger:  " + needs["hunger"] + "/100\n";
        output += "  Energy:  " + needs["energy"] + "/100\n";
        output += "  Comfort: " + needs["comfort"] + "/100\n";
        output += "  Goal:    " + needs["goal"] + "\n";
        output += "  Paid:    " + needs["paid"] + "\n";
    }

    output += "\n  Gold: " + mobile.gold + "\n";
    send_client_message(connection_id, output);
}

fn handle_sim_setup(mobile_id, parts, connection_id) {
    if parts.len() < 7 {
        send_client_message(connection_id, "Usage: medit <id> simulation setup <home_vnum> <work_vnum> <shop_vnum>");
        return;
    }
    let home = parts[4];
    let work = parts[5];
    let shop = parts[6];

    if set_npc_simulation(mobile_id, home, work, shop) {
        send_client_message(connection_id, "Simulation enabled. Home: " + home + ", Work: " + work + ", Shop: " + shop);
    } else {
        send_client_message(connection_id, "Failed to enable simulation.");
    }
}

fn handle_sim_remove(mobile_id, connection_id) {
    if remove_npc_simulation(mobile_id) {
        send_client_message(connection_id, "Simulation disabled.");
    } else {
        send_client_message(connection_id, "Failed to disable simulation.");
    }
}

fn handle_sim_pay(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> simulation pay <amount>");
        return;
    }
    let amount = try_parse_int(parts[4]);
    if amount == () {
        send_client_message(connection_id, "Invalid amount.");
        return;
    }
    if set_npc_work_pay(mobile_id, amount) {
        send_client_message(connection_id, "Work pay set to " + amount + " gold.");
    } else {
        send_client_message(connection_id, "Failed. Is simulation enabled?");
    }
}

fn handle_sim_hours(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> simulation hours <start> <end>");
        return;
    }
    let start = try_parse_int(parts[4]);
    let end = try_parse_int(parts[5]);
    if start == () || end == () {
        send_client_message(connection_id, "Invalid hour values.");
        return;
    }
    if set_npc_work_hours(mobile_id, start, end) {
        send_client_message(connection_id, "Work hours set to " + start + ":00 - " + end + ":00.");
    } else {
        send_client_message(connection_id, "Failed. Is simulation enabled?");
    }
}

fn handle_sim_food(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> simulation food <vnum>");
        return;
    }
    if set_npc_preferred_food(mobile_id, parts[4]) {
        send_client_message(connection_id, "Preferred food set to: " + parts[4]);
    } else {
        send_client_message(connection_id, "Failed. Is simulation enabled?");
    }
}

fn handle_sim_decay(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> simulation decay <hunger|energy|comfort> <rate>");
        send_client_message(connection_id, "Rate: 100 = normal, 200 = 2x faster, 50 = half speed, 0 = use default");
        return;
    }
    let need = parts[4].to_lower();
    let rate = try_parse_int(parts[5]);
    if rate == () {
        send_client_message(connection_id, "Invalid rate value.");
        return;
    }
    if set_npc_decay_rate(mobile_id, need, rate) {
        send_client_message(connection_id, need + " decay rate set to " + rate + ".");
    } else {
        send_client_message(connection_id, "Failed. Valid needs: hunger, energy, comfort");
    }
}

fn handle_sim_lowgold(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> simulation lowgold <threshold>");
        send_client_message(connection_id, "When the NPC's gold drops to this value or below, they prioritize work over food.");
        return;
    }
    let threshold = try_parse_int(parts[4]);
    if threshold == () {
        send_client_message(connection_id, "Invalid threshold value.");
        return;
    }
    if set_npc_low_gold_threshold(mobile_id, threshold) {
        send_client_message(connection_id, "Low gold threshold set to " + threshold + ".");
    } else {
        send_client_message(connection_id, "Failed. Is simulation enabled?");
    }
}

// ========== Social Subcommand ==========

fn get_social_action(parts) {
    if parts.len() < 4 { return "show"; }
    return parts[3].to_lower();
}

fn handle_social_subcommand(mobile, mobile_id, parts, connection_id) {
    let action = get_social_action(parts);

    if action == "show" || action == "social" {
        show_social_status(mobile_id, connection_id);
        return;
    }
    if action == "happiness" { handle_social_happiness(mobile_id, parts, connection_id); return; }
    if action == "affinity" { handle_social_affinity(mobile_id, parts, connection_id); return; }
    if action == "family" { handle_social_family(mobile_id, parts, connection_id); return; }

    send_client_message(connection_id, "Unknown social action: " + action);
    send_client_message(connection_id, "Valid: show, happiness <0-100>, affinity <other_id> <value>, family ...");
}

fn show_social_status(mobile_id, connection_id) {
    let happiness = get_mobile_happiness(mobile_id);
    if happiness < 0 {
        send_client_message(connection_id, "This mobile has no social state (not a simulated migrant).");
        return;
    }

    let mood = get_mobile_mood(mobile_id);
    let likes = get_mobile_likes(mobile_id);
    let dislikes = get_mobile_dislikes(mobile_id);

    let output = "\n=== Social State ===\n";
    output += "  Happiness: " + happiness + "/100 (" + mood + ")\n";
    output += "  Likes:     " + format_topic_list(likes) + "\n";
    output += "  Dislikes:  " + format_topic_list(dislikes) + "\n";
    send_client_message(connection_id, output);
}

fn format_topic_list(list) {
    if list.len() == 0 { return "(none)"; }
    let out = "";
    for topic in list {
        if out != "" { out += ", "; }
        out += topic;
    }
    return out;
}

fn handle_social_happiness(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> social happiness <0-100>");
        return;
    }
    let value = try_parse_int(parts[4]);
    if value == () {
        send_client_message(connection_id, "Invalid happiness value.");
        return;
    }
    if get_mobile_happiness(mobile_id) < 0 {
        send_client_message(connection_id, "This mobile has no social state (not a simulated migrant).");
        return;
    }
    if set_mobile_happiness(mobile_id, value) {
        send_client_message(connection_id, "Happiness set to " + value + ".");
    } else {
        send_client_message(connection_id, "Failed to set happiness.");
    }
}

fn handle_social_affinity(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> social affinity <other_mobile_id> <value>");
        send_client_message(connection_id, "Value is clamped to -100..=100.");
        return;
    }
    let other_id = parts[4];
    let value = try_parse_int(parts[5]);
    if value == () {
        send_client_message(connection_id, "Invalid affinity value.");
        return;
    }
    if get_mobile_happiness(mobile_id) < 0 {
        send_client_message(connection_id, "This mobile has no social state (not a simulated migrant).");
        return;
    }
    if set_mobile_affinity(mobile_id, other_id, value) {
        send_client_message(connection_id, "Affinity toward " + other_id + " set to " + value + ".");
    } else {
        send_client_message(connection_id, "Failed to set affinity.");
    }
}

fn handle_social_family(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        show_family(mobile_id, connection_id);
        return;
    }
    let sub = parts[4].to_lower();
    if sub == "list" || sub == "show" {
        show_family(mobile_id, connection_id);
        return;
    }
    if sub == "set" {
        if parts.len() < 7 {
            send_client_message(connection_id, "Usage: medit <id> social family set <other_id> <parent|child|sibling|partner>");
            return;
        }
        let other_id = parts[5];
        let kind = parts[6].to_lower();
        if set_family_relationship(mobile_id, other_id, kind) {
            send_client_message(connection_id, "Linked " + mobile_id + " as " + kind + " of " + other_id + ".");
        } else {
            send_client_message(connection_id, "Failed. Check ids exist; Partner requires both sides to be partner-free.");
        }
        return;
    }
    if sub == "unset" {
        if parts.len() < 6 {
            send_client_message(connection_id, "Usage: medit <id> social family unset <other_id>");
            return;
        }
        let other_id = parts[5];
        if unset_family_relationship(mobile_id, other_id) {
            send_client_message(connection_id, "Relationship between " + mobile_id + " and " + other_id + " removed.");
        } else {
            send_client_message(connection_id, "Failed.");
        }
        return;
    }
    if sub == "household" {
        handle_family_household(mobile_id, parts, connection_id);
        return;
    }
    send_client_message(connection_id, "Unknown family action: " + sub);
    send_client_message(connection_id, "Valid: list, set <other_id> <kind>, unset <other_id>, household {new|link <other_id>|clear}");
}

fn show_family(mobile_id, connection_id) {
    let fam = get_mobile_family(mobile_id);
    let output = "\n=== Family ===\n";
    if fam.len() == 0 {
        output += "  (no family links)\n";
    } else {
        for entry in fam {
            output += "  " + entry.kind + ": " + entry.name + " (" + entry.other_id + ") [affinity " + entry.affinity + "]\n";
        }
    }
    send_client_message(connection_id, output);
}

fn handle_family_household(mobile_id, parts, connection_id) {
    if parts.len() < 6 {
        send_client_message(connection_id, "Usage: medit <id> social family household {new|link <other_id>|clear}");
        return;
    }
    let op = parts[5].to_lower();
    if op == "new" {
        let hid = new_household(mobile_id);
        if hid != "" {
            send_client_message(connection_id, "New household " + hid + " assigned.");
        } else {
            send_client_message(connection_id, "Failed to mint household.");
        }
        return;
    }
    if op == "link" {
        if parts.len() < 7 {
            send_client_message(connection_id, "Usage: medit <id> social family household link <other_id>");
            return;
        }
        let other_id = parts[6];
        if link_household(mobile_id, other_id) {
            send_client_message(connection_id, "Household linked to " + other_id + ".");
        } else {
            send_client_message(connection_id, "Failed. Check ids.");
        }
        return;
    }
    if op == "clear" {
        if clear_household(mobile_id) {
            send_client_message(connection_id, "Household cleared.");
        } else {
            send_client_message(connection_id, "Failed.");
        }
        return;
    }
    send_client_message(connection_id, "Unknown household op: " + op);
}

// ========== Pregnancy Subcommand ==========
//
// medit <id> pregnancy                      → show status
// medit <id> pregnancy preg <father_id> [days] → force-set pregnancy (builder-only)
// medit <id> pregnancy birth                → fire spawn_child now
// medit <id> pregnancy clear                → drop pregnancy state

fn handle_pregnancy_subcommand(mobile, mobile_id, parts, connection_id) {
    let action = if parts.len() < 4 { "show" } else { parts[3].to_lower() };
    if action == "show" || action == "status" {
        show_pregnancy(mobile_id, connection_id);
        return;
    }
    if action == "preg" || action == "set" {
        handle_pregnancy_set(mobile_id, parts, connection_id);
        return;
    }
    if action == "birth" {
        if force_birth(mobile_id) {
            send_client_message(connection_id, "Birth triggered.");
        } else {
            send_client_message(connection_id, "Failed. Mobile must be pregnant with a resolvable area.");
        }
        return;
    }
    if action == "clear" {
        if clear_pregnancy(mobile_id) {
            send_client_message(connection_id, "Pregnancy cleared.");
        } else {
            send_client_message(connection_id, "Failed.");
        }
        return;
    }
    send_client_message(connection_id, "Unknown pregnancy action: " + action);
    send_client_message(connection_id, "Valid: show, preg <father_id> [days], birth, clear");
}

fn show_pregnancy(mobile_id, connection_id) {
    let status = get_pregnancy_status(mobile_id);
    if status == () {
        send_client_message(connection_id, "Not pregnant.");
        return;
    }
    let out = "\n=== Pregnancy ===\n" +
        "  Due (absolute day): " + status.pregnant_until_day + "\n" +
        "  Days remaining:     " + status.days_remaining + "\n";
    if status.contains("pregnant_by") {
        send_client_message(connection_id, out + "  Father id:          " + status.pregnant_by);
    } else {
        send_client_message(connection_id, out + "  Father id:          (unknown)");
    }
}

fn handle_pregnancy_set(mobile_id, parts, connection_id) {
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: medit <id> pregnancy preg <father_id> [gestation_days]");
        return;
    }
    let father_id = parts[4];
    let days = if parts.len() >= 6 { try_parse_int(parts[5]) } else { 0 };
    if days == () { days = 0; }
    if force_pregnancy(mobile_id, father_id, days) {
        send_client_message(connection_id, "Pregnancy set (father " + father_id + ", " +
            (if days > 0 { days } else { 60 }) + " game days).");
    } else {
        send_client_message(connection_id, "Failed. Mobile must have social state.");
    }
}

// === Combat spells (CircleMUD `magic_user` analog) ===

fn show_combat_spells(mobile, connection_id) {
    let spells = mobile.combat_spells;
    let chance = mobile.combat_spell_chance;
    if spells.len() == 0 {
        send_client_message(connection_id, "Combat spells: (none) - mob fights melee-only.");
    } else {
        let line = "Combat spells (" + chance + "% per round): ";
        let first = true;
        for s in spells {
            if first { line += s; first = false; }
            else { line += ", " + s; }
        }
        send_client_message(connection_id, line);
    }
    send_client_message(connection_id, "Usage: medit <id> combat_spells [add|remove|clear|chance] [<spell_id>|<0-100>]");
}

fn handle_mob_on_hit_subcommand(uuid, mobile_id, parts, connection_id) {
    let mode = if parts.len() >= 3 { parts[2].to_lower() } else { "list" };

    if mode == "list" || mode == "show" {
        let effects = get_mobile_on_hit_effects(uuid);
        send_client_message(connection_id, build_mob_on_hit_list(effects, mobile_id));
        return;
    }

    if mode == "clear" {
        set_mobile_on_hit_effects(uuid, []);
        send_client_message(connection_id, "On-hit effects cleared.");
        return;
    }

    if mode == "remove" || mode == "rm" || mode == "delete" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: medit " + mobile_id + " on_hit remove <index>");
            return;
        }
        let one_based = try_parse_int(parts[3]);
        if one_based == () || one_based < 1 {
            send_client_message(connection_id, "Index must be a positive number (see 'medit " + mobile_id + " on_hit list').");
            return;
        }
        let effects = get_mobile_on_hit_effects(uuid);
        if one_based > effects.len() {
            send_client_message(connection_id, "No effect at index " + one_based + " (have " + effects.len() + ").");
            return;
        }
        let removed = effects[one_based - 1];
        effects.remove(one_based - 1);
        set_mobile_on_hit_effects(uuid, effects);
        send_client_message(connection_id, "Removed: " + format_mob_on_hit_entry(removed));
        return;
    }

    if mode == "add" {
        if parts.len() < 7 {
            send_client_message(
                connection_id,
                "Usage: medit " + mobile_id + " on_hit add <effect> <chance> <magnitude> <duration>\n" +
                "  effect:    bleeding | fire | poison | cold | acid | lightning | sleep | blind | slow | curse | ...\n" +
                "  chance:    1-100 (per-hit roll)\n" +
                "  magnitude: bleeding=severity; elemental=damage/round; buff=effect-specific\n" +
                "  duration:  bleeding=ignored; elemental=rounds; buff=seconds\n" +
                "Note: composes with the legacy poisonous/fiery/chilling/corrosive/shocking flags."
            );
            return;
        }
        let effect_str = parts[3].to_lower();
        let chance = try_parse_int(parts[4]);
        let magnitude = try_parse_int(parts[5]);
        let duration = try_parse_int(parts[6]);
        if chance == () || magnitude == () || duration == () {
            send_client_message(connection_id, "chance, magnitude, and duration must all be whole numbers.");
            return;
        }
        if chance < 1 || chance > 100 {
            send_client_message(connection_id, "chance must be 1-100.");
            return;
        }
        let effects = get_mobile_on_hit_effects(uuid);
        effects.push(#{
            effect: effect_str,
            chance: chance,
            magnitude: magnitude,
            duration: duration
        });
        set_mobile_on_hit_effects(uuid, effects);
        send_client_message(connection_id, "Added: " + format_mob_on_hit_entry(effects[effects.len() - 1]));
        return;
    }

    send_client_message(
        connection_id,
        "Usage: medit " + mobile_id + " on_hit [list|add|remove|clear]\n" +
        "  list                                — show current effects\n" +
        "  add <effect> <chance> <mag> <dur>   — append a new effect\n" +
        "  remove <index>                      — remove by 1-based index\n" +
        "  clear                               — remove all"
    );
}

fn format_mob_on_hit_entry(effect) {
    let suffix = if effect.effect == "bleeding" {
        "severity " + effect.magnitude
    } else if effect.effect == "fire" || effect.effect == "poison" || effect.effect == "cold" || effect.effect == "acid" || effect.effect == "lightning" {
        effect.magnitude + " dmg/round for " + effect.duration + " rounds"
    } else {
        "magnitude " + effect.magnitude + ", " + effect.duration + "s"
    };
    return effect.effect + " (" + effect.chance + "% chance, " + suffix + ")";
}

fn build_mob_on_hit_list(effects, mobile_id) {
    if effects.len() == 0 {
        return "On-hit effects: (none)\nUse 'medit " + mobile_id + " on_hit add <effect> <chance> <mag> <dur>' to add one.";
    }
    let output = "On-hit effects (" + effects.len() + "):\n";
    let i = 0;
    while i < effects.len() {
        output += "  [" + (i + 1) + "] " + format_mob_on_hit_entry(effects[i]) + "\n";
        i += 1;
    }
    return output;
}

fn handle_combat_spells_subcommand(mobile, mobile_id, parts, connection_id) {
    // medit <id> combat_spells                       - show
    // medit <id> combat_spells add <spell_id>
    // medit <id> combat_spells remove <spell_id>
    // medit <id> combat_spells clear
    // medit <id> combat_spells chance <0-100>
    if parts.len() < 3 {
        show_combat_spells(mobile, connection_id);
        return;
    }
    let action = parts[2].to_lower();

    if action == "add" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: medit <id> combat_spells add <spell_id>");
            return;
        }
        let spell_id = parts[3].to_lower();
        let info = get_spell_info(spell_id);
        if info.len() == 0 {
            send_client_message(connection_id, "Unknown spell '" + spell_id + "'.");
            return;
        }
        if add_mobile_combat_spell(mobile_id, spell_id) {
            send_client_message(connection_id, "Added " + spell_id + " to combat spell list.");
        } else {
            send_client_message(connection_id, "Already on the list (or save failed).");
        }
        return;
    }

    if action == "remove" || action == "rm" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: medit <id> combat_spells remove <spell_id>");
            return;
        }
        let spell_id = parts[3].to_lower();
        if remove_mobile_combat_spell(mobile_id, spell_id) {
            send_client_message(connection_id, "Removed " + spell_id + " from combat spell list.");
        } else {
            send_client_message(connection_id, spell_id + " was not on the list.");
        }
        return;
    }

    if action == "clear" {
        if clear_mobile_combat_spells(mobile_id) {
            send_client_message(connection_id, "Combat spell list cleared.");
        } else {
            send_client_message(connection_id, "Save failed.");
        }
        return;
    }

    if action == "chance" {
        if parts.len() < 4 {
            send_client_message(connection_id, "Usage: medit <id> combat_spells chance <0-100>");
            return;
        }
        let chance = try_parse_int(parts[3]);
        if chance == () {
            send_client_message(connection_id, "Chance must be a number 0-100.");
            return;
        }
        if set_mobile_combat_spell_chance(mobile_id, chance) {
            let clamped = if chance < 0 { 0 } else if chance > 100 { 100 } else { chance };
            send_client_message(connection_id, "Combat spell chance set to " + clamped + "%.");
        } else {
            send_client_message(connection_id, "Save failed.");
        }
        return;
    }

    send_client_message(connection_id, "Usage: medit <id> combat_spells [add|remove|clear|chance] [<spell_id>|<0-100>]");
}

