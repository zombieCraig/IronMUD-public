// scripts/commands/clan.rhai
// Clan administration + membership commands. Dispatcher style — keep each
// subcommand branch lean to stay under Rhai's expression complexity ceiling.

const YELLOW = "\x1b[1;33m";
const RESET = "\x1b[0m";

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () { return; }

    let parts = split_args(args);
    if parts.len() == 0 {
        show_self_info(connection_id, char);
        return;
    }

    let sub = parts[0].to_lower();
    if sub == "info"     { handle_info(connection_id, char, parts); return; }
    if sub == "list"     { handle_list(connection_id); return; }
    if sub == "who"      { handle_who(connection_id, char); return; }
    if sub == "motd"     { handle_motd(connection_id, char); return; }
    if sub == "leave"    { handle_leave(connection_id, char); return; }
    if sub == "promote"  { handle_promote(connection_id, char, parts); return; }
    if sub == "demote"   { handle_demote(connection_id, char, parts); return; }
    if sub == "kick"     { handle_kick(connection_id, char, parts); return; }

    // Admin-only subcommands.
    if sub == "create"    { handle_create(connection_id, char, parts); return; }
    if sub == "destroy"   { handle_destroy(connection_id, char, parts); return; }
    if sub == "addmember" { handle_addmember(connection_id, char, parts); return; }
    if sub == "color"     { handle_color(connection_id, char, parts); return; }
    if sub == "setmotd"   { handle_setmotd(connection_id, char, parts, args); return; }
    if sub == "setdesc"   { handle_setdesc(connection_id, char, parts, args); return; }

    if sub == "help" || sub == "?" { show_help(connection_id, char); return; }

    send_client_message(connection_id, "Unknown clan command. Try 'clan help'.");
}

// ---------- helpers ----------

fn split_args(args) {
    let raw = args.split(" ");
    let out = [];
    for p in raw { if p != "" { out.push(p); } }
    return out;
}

fn require_membership(connection_id, char) {
    let tag = get_character_clan_tag(char.name);
    if tag == "" {
        send_client_message(connection_id, "You are not in a clan.");
        return "";
    }
    return tag;
}

fn require_admin(connection_id, char) {
    if !char.is_admin {
        send_client_message(connection_id, "Only immortals may use that.");
        return false;
    }
    return true;
}

fn require_officer_or_leader(connection_id, char) {
    let tag = require_membership(connection_id, char);
    if tag == "" { return ""; }
    let rank = get_character_clan_rank(char.name);
    if rank != "leader" && rank != "officer" {
        send_client_message(connection_id, "You don't have authority to do that in your clan.");
        return "";
    }
    return tag;
}

fn rest_of_args(parts, drop_count) {
    let out = "";
    let i = drop_count;
    while i < parts.len() {
        if out != "" { out += " "; }
        out += parts[i];
        i += 1;
    }
    return out;
}

fn color_from_name(name) {
    let n = name.to_lower();
    if n == "yellow"  { return "\x1b[1;33m"; }
    if n == "red"     { return "\x1b[1;31m"; }
    if n == "green"   { return "\x1b[1;32m"; }
    if n == "blue"    { return "\x1b[1;34m"; }
    if n == "magenta" { return "\x1b[1;35m"; }
    if n == "cyan"    { return "\x1b[1;36m"; }
    if n == "white"   { return "\x1b[1;37m"; }
    return "";
}

// ---------- read-side handlers ----------

fn show_self_info(connection_id, char) {
    let tag = get_character_clan_tag(char.name);
    if tag == "" {
        send_client_message(connection_id, "You are not in a clan. Use 'clan list' to see existing clans, or 'clan help'.");
        return;
    }
    let info = get_clan_info(tag);
    if info == () {
        send_client_message(connection_id, "Your clan record could not be loaded.");
        return;
    }
    let rank = get_character_clan_rank(char.name);
    let color = info.color;
    let out = color + "[" + tag + "]\x1b[0m " + info.name + " (your rank: " + rank + ")\n";
    out += info.member_count + " members\n";
    if info.motd != "" { out += "\nMOTD: " + info.motd + "\n"; }
    send_client_message(connection_id, out);
}

fn handle_info(connection_id, char, parts) {
    let tag = "";
    if parts.len() >= 2 {
        tag = parts[1].to_upper();
    } else {
        tag = get_character_clan_tag(char.name);
        if tag == "" {
            send_client_message(connection_id, "Usage: clan info <tag>");
            return;
        }
    }
    let info = get_clan_info(tag);
    if info == () {
        send_client_message(connection_id, "No clan tagged [" + tag + "].");
        return;
    }
    let color = info.color;
    let out = color + "[" + tag + "]\x1b[0m " + info.name + "\n";
    out += "  Founder: " + info.founder + "\n";
    out += "  Members: " + info.member_count + "\n";
    if info.description != "" { out += "  " + info.description + "\n"; }
    if info.motd != "" { out += "  MOTD: " + info.motd + "\n"; }
    send_client_message(connection_id, out);
}

fn handle_list(connection_id) {
    let clans = list_clans();
    if clans.len() == 0 {
        send_client_message(connection_id, "No clans exist yet.");
        return;
    }
    let out = "Clans:\n";
    for c in clans {
        let color = c.color;
        out += "  " + color + "[" + c.tag + "]\x1b[0m " + c.name + " — " + c.member_count + " members\n";
    }
    send_client_message(connection_id, out);
}

fn handle_who(connection_id, char) {
    let tag = require_membership(connection_id, char);
    if tag == "" { return; }
    let info = get_clan_info(tag);
    if info == () { return; }
    let out = info.color + "[" + tag + "]\x1b[0m " + info.name + " — roster (" + info.member_count + "):\n";
    for m in info.members {
        let online = find_player_connection_by_name(m.name) != "";
        let status = if online { "online " } else { "offline" };
        out += "  " + status + "  " + m.rank.to_upper() + "  " + m.name + "\n";
    }
    send_client_message(connection_id, out);
}

fn handle_motd(connection_id, char) {
    let tag = require_membership(connection_id, char);
    if tag == "" { return; }
    let info = get_clan_info(tag);
    if info == () { return; }
    if info.motd == "" {
        send_client_message(connection_id, "Your clan has no MOTD set.");
    } else {
        send_client_message(connection_id, info.color + "[" + tag + "] MOTD\x1b[0m: " + info.motd);
    }
}

// ---------- member-action handlers ----------

fn handle_leave(connection_id, char) {
    let tag = require_membership(connection_id, char);
    if tag == "" { return; }
    let result = clan_remove_member(tag, char.name);
    if result == "" {
        send_client_message(connection_id, "You have left clan [" + tag + "].");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_promote(connection_id, char, parts) {
    let tag = require_officer_or_leader(connection_id, char);
    if tag == "" { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan promote <player>");
        return;
    }
    let target = parts[1];
    let current = get_character_clan_rank(target);
    let new_rank = "";
    if current == "member"  { new_rank = "officer"; }
    else if current == "officer" {
        // Only a sitting leader may promote to leader.
        if get_character_clan_rank(char.name) != "leader" {
            send_client_message(connection_id, "Only a leader may promote an officer to leader.");
            return;
        }
        new_rank = "leader";
    }
    else if current == "" {
        send_client_message(connection_id, target + " is not in your clan.");
        return;
    }
    else {
        send_client_message(connection_id, target + " is already a leader.");
        return;
    }
    let result = clan_set_rank(tag, target, new_rank);
    if result == "" {
        send_client_message(connection_id, target + " has been promoted to " + new_rank + ".");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_demote(connection_id, char, parts) {
    let tag = require_officer_or_leader(connection_id, char);
    if tag == "" { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan demote <player>");
        return;
    }
    let target = parts[1];
    let current = get_character_clan_rank(target);
    let new_rank = "";
    if current == "leader"  {
        if get_character_clan_rank(char.name) != "leader" {
            send_client_message(connection_id, "Only a leader may demote another leader.");
            return;
        }
        new_rank = "officer";
    }
    else if current == "officer" { new_rank = "member"; }
    else if current == "member"  {
        send_client_message(connection_id, target + " is already at the lowest rank.");
        return;
    }
    else {
        send_client_message(connection_id, target + " is not in your clan.");
        return;
    }
    let result = clan_set_rank(tag, target, new_rank);
    if result == "" {
        send_client_message(connection_id, target + " has been demoted to " + new_rank + ".");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_kick(connection_id, char, parts) {
    let tag = require_officer_or_leader(connection_id, char);
    if tag == "" { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan kick <player>");
        return;
    }
    let target = parts[1];
    if target.to_lower() == char.name.to_lower() {
        send_client_message(connection_id, "Use 'clan leave' to depart your own clan.");
        return;
    }
    let target_rank = get_character_clan_rank(target);
    if target_rank == "leader" && get_character_clan_rank(char.name) != "leader" {
        send_client_message(connection_id, "Only a leader may kick another leader.");
        return;
    }
    let result = clan_remove_member(tag, target);
    if result == "" {
        send_client_message(connection_id, target + " has been removed from the clan.");
    } else {
        send_client_message(connection_id, result);
    }
}

// ---------- admin handlers ----------

fn handle_create(connection_id, char, parts) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: clan create <TAG> <name...>");
        return;
    }
    let tag = parts[1];
    let name = rest_of_args(parts, 2);
    let result = create_clan(tag, name);
    if result == "" {
        send_client_message(connection_id, "Created clan [" + tag.to_upper() + "] " + name + ".");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_destroy(connection_id, char, parts) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan destroy <TAG> confirm");
        return;
    }
    if parts.len() < 3 || parts[2].to_lower() != "confirm" {
        send_client_message(connection_id, "Add 'confirm' to actually destroy: clan destroy " + parts[1].to_upper() + " confirm");
        return;
    }
    let result = delete_clan_full(parts[1]);
    if result == "" {
        send_client_message(connection_id, "Clan [" + parts[1].to_upper() + "] destroyed.");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_addmember(connection_id, char, parts) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: clan addmember <TAG> <player> [leader|officer|member]");
        return;
    }
    let rank = if parts.len() >= 4 { parts[3].to_lower() } else { "member" };
    let result = clan_add_member(parts[1], parts[2], rank);
    if result == "" {
        send_client_message(connection_id, parts[2] + " added to [" + parts[1].to_upper() + "] as " + rank + ".");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_color(connection_id, char, parts) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: clan color <TAG> <yellow|red|green|blue|magenta|cyan|white|reset>");
        return;
    }
    let tag = parts[1];
    let color_arg = parts[2].to_lower();
    let ansi = "";
    if color_arg != "reset" {
        ansi = color_from_name(color_arg);
        if ansi == "" {
            send_client_message(connection_id, "Unknown color. Pick yellow/red/green/blue/magenta/cyan/white/reset.");
            return;
        }
    }
    let result = clan_set_color(tag, ansi);
    if result == "" {
        if ansi == "" {
            send_client_message(connection_id, "Clan [" + tag.to_upper() + "] color reset to default.");
        } else {
            send_client_message(connection_id, ansi + "[" + tag.to_upper() + "]\x1b[0m color updated.");
        }
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_setmotd(connection_id, char, parts, raw_args) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan setmotd <TAG> <text...>");
        return;
    }
    let body = rest_of_args(parts, 2);
    let result = clan_set_motd(parts[1], body);
    if result == "" {
        send_client_message(connection_id, "MOTD updated for [" + parts[1].to_upper() + "].");
    } else {
        send_client_message(connection_id, result);
    }
}

fn handle_setdesc(connection_id, char, parts, raw_args) {
    if !require_admin(connection_id, char) { return; }
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: clan setdesc <TAG> <text...>");
        return;
    }
    let body = rest_of_args(parts, 2);
    let result = clan_set_description(parts[1], body);
    if result == "" {
        send_client_message(connection_id, "Description updated for [" + parts[1].to_upper() + "].");
    } else {
        send_client_message(connection_id, result);
    }
}

// ---------- help ----------

fn show_help(connection_id, char) {
    let out = "Clan commands:\n";
    out += "  clan                       Show your clan summary\n";
    out += "  clan info [<tag>]          Show a clan's public card\n";
    out += "  clan list                  List all clans\n";
    out += "  clan who                   Roster of your clan (online/offline)\n";
    out += "  clan motd                  Re-show your clan's MOTD\n";
    out += "  clan promote <player>      Promote a clanmate (leader/officer only)\n";
    out += "  clan demote <player>       Demote a clanmate (leader/officer only)\n";
    out += "  clan kick <player>         Remove a clanmate (leader/officer only)\n";
    out += "  clan leave                 Leave your clan\n";
    out += "  clanchat <msg> (alias ct)  Talk to all online clanmates\n";
    if char.is_admin {
        out += "Immortal:\n";
        out += "  clan create <TAG> <name>          Create a new clan\n";
        out += "  clan destroy <TAG> confirm        Disband a clan\n";
        out += "  clan addmember <TAG> <player> [rank]  Add a member (default member)\n";
        out += "  clan color <TAG> <color|reset>    Set the [TAG] color\n";
        out += "  clan setmotd <TAG> <text...>      Set MOTD\n";
        out += "  clan setdesc <TAG> <text...>      Set description\n";
    }
    send_client_message(connection_id, out);
}
