// scripts/commands/group.rhai
// Manage party/group membership, and show the party status panel.
//
// The panel reads `get_group_panel`, which builds every row from the live
// session copies — a status display a player can spam must not cost a
// character load per member. Health bands and their colours come from that
// binding too, sourced from `src/combat_text.rs`; this script never decides
// what "Bloodied" means or what colour it is.

import "progress_ui" as ui;
import "text" as text;

fn try_add_follower_to_group(follower_name, leader_name) {
    let follower_conn = find_player_connection_by_name(follower_name);
    if follower_conn == "" { return false; }

    let follower = get_player_character(follower_conn);
    if follower == () { return false; }
    if follower.is_grouped { return false; }

    set_grouped(follower_name, true);
    send_client_message(follower_conn, leader_name + " has added you to the group.");
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // If following someone, we can only show status
    if char.following != "" {
        if args != "" {
            send_client_message(connection_id, "You must be the group leader to add members.");
            return;
        }
        show_group_status(char, connection_id);
        return;
    }

    if args == "" {
        // Show group status
        show_group_status(char, connection_id);
        return;
    }

    let target_name = args.to_lower();

    // "group all" - add all followers
    if target_name == "all" {
        let followers = get_followers(char.name);
        if followers.len() == 0 {
            send_client_message(connection_id, "You have no followers.");
            return;
        }

        let added = 0;
        for follower_name in followers {
            if try_add_follower_to_group(follower_name, char.name) {
                added += 1;
            }
        }

        if added > 0 {
            send_client_message(connection_id, "You add " + added + " follower(s) to your group.");
        } else {
            send_client_message(connection_id, "All your followers are already in the group.");
        }
        return;
    }

    // Group specific player
    let target_conn = find_player_connection_by_name(target_name);
    if target_conn == "" {
        send_client_message(connection_id, "Player '" + args + "' is not online.");
        return;
    }

    let target_char = get_player_character(target_conn);
    if target_char == () {
        send_client_message(connection_id, "Player '" + args + "' is not online.");
        return;
    }

    // Must be following us
    if target_char.following == "" || target_char.following.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, target_char.name + " is not following you.");
        return;
    }

    // Toggle group status
    if target_char.is_grouped {
        set_grouped(target_char.name, false);
        send_client_message(connection_id, target_char.name + " has been removed from your group.");
        send_client_message(target_conn, char.name + " has removed you from the group.");
    } else {
        set_grouped(target_char.name, true);
        send_client_message(connection_id, target_char.name + " is now in your group.");
        send_client_message(target_conn, char.name + " has added you to the group.");
    }
}

// One row of the panel. Split out because the row is four conditional
// fragments and Rhai's expression complexity limit dislikes them inline.
fn render_member_row(row) {
    let label = text::pad_right(row.name, 14);

    // A member in another room has no vitals worth printing — they are not in
    // the fight, and showing their HP invites the party to react to a number
    // that has nothing to do with what is in front of them.
    if !row.here {
        return "  " + label + "(elsewhere)";
    }

    let bar = ui::build_bar_of_width(row.hp, row.max_hp, 10);
    let band = row.condition_color + text::pad_right(row.condition, 10) + "\x1b[0m";
    let stam = text::pad_left("" + row.stamina, 3) + "/" + text::pad_right("" + row.max_stamina, 3);
    return "  " + label + bar + " " + band + " ST " + stam + " " + row.position;
}

fn render_member_tags(row) {
    let tag = "";
    if row.role == "leader" { tag += " (Leader)"; }
    if row.role == "following" { tag += " (Following)"; }
    if row.is_you { tag += " <-- You"; }
    return tag;
}

fn show_group_status(char, connection_id) {
    let rows = get_group_panel(char.name);

    // A panel of one is not a group. `get_group_panel` always returns at least
    // the asker, so the emptiness test is on the size, not on the array.
    if rows.len() < 2 {
        send_client_message(connection_id, "You are not in a group.");
        return;
    }

    let leader_name = get_group_leader(char.name);
    if leader_name == "" { leader_name = char.name; }

    let output = "Group led by " + leader_name + ":\n";
    for row in rows {
        output += render_member_row(row) + render_member_tags(row) + "\n";
        // Role legibility: what each member is actually swinging at. This is
        // how a party sees it has split across three mobs instead of focusing
        // one, which no other display shows.
        if row.fighting.len() > 0 {
            output += "                fighting " + row.fighting.join(", ") + "\n";
        }
    }

    send_client_message(connection_id, output);
}
