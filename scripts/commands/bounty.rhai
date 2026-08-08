// scripts/commands/bounty.rhai
//
//   bounty                           the board
//   bounty mine                      what you have claimed
//   bounty show <n>                  one bounty in full
//   bounty post <points> <title>     ask for something
//   bounty claim <n>                 take it
//   bounty drop <n>                  give it back
//   bounty submit <n> [vnums...]     say it is done
//   bounty accept <n>                pay it (requester or admin)
//   bounty reject <n> <reason>       send it back
//
// The board pays in **builder points, not gold**. Gold is meaningless to a
// builder; points feed the same score, boards and achievements as everything
// else, which makes this a way of routing effort rather than a second economy.
//
// Half the board writes itself. The content auditor turns its own blockers and
// warnings into bounties, so there is work here before anybody has typed
// anything — and those close themselves when the problem stops occurring, so
// fixing a room pays whether or not you ever read this.
//
// Claims expire after three days. A board that can be squatted is a board with
// nothing available on it.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Two splits of the same line. `parts` is lowercased, for matching verbs
    // and reading ticket numbers. `raw` keeps the player's capitals, because a
    // bounty title and a rejection reason are prose somebody else reads —
    // matching on a normalised copy and storing the normalised copy is how
    // "A blacksmith for Oakvale" became "a blacksmith for oakvale".
    let line = text::normalise(args);
    let parts = line.split(" ");
    let raw = text::trim_spaces(args).split(" ");
    let verb = "";
    if parts.len() > 0 { verb = parts[0].to_lower(); }

    if verb == "" || verb == "list" {
        show_board(list_bounties(""), "Bounty Board", connection_id);
        return;
    }
    if verb == "mine" {
        show_board(list_bounties("mine:" + char.name), "Your Claims", connection_id);
        return;
    }
    // Not `done` — that is the submit verb, and matching it here shadowed it.
    if verb == "closed" {
        show_board(list_bounties("accepted"), "Closed", connection_id);
        return;
    }
    if verb == "help" {
        send_client_message(connection_id, usage());
        return;
    }

    handle_action(char, verb, parts, raw, connection_id);
}

// Split from run_command so neither grows an if-else chain long enough to trip
// Rhai's expression-complexity limit.
fn handle_action(char, verb, parts, raw, connection_id) {
    if verb == "show" || verb == "read" {
        show_one(ticket_arg(parts, 1), connection_id);
        return;
    }
    if verb == "post" || verb == "ask" {
        do_post(char, parts, raw, connection_id);
        return;
    }
    if verb == "claim" || verb == "take" {
        report(claim_bounty(ticket_arg(parts, 1), char.name), connection_id);
        return;
    }
    if verb == "drop" || verb == "release" {
        report(drop_bounty(ticket_arg(parts, 1), char.name, char.is_admin), connection_id);
        return;
    }
    if verb == "submit" || verb == "done" {
        report(submit_bounty(ticket_arg(parts, 1), char.name, rest_from(raw, 2)), connection_id);
        return;
    }
    if verb == "accept" || verb == "pay" {
        report(accept_bounty(ticket_arg(parts, 1), char.name, char.is_admin), connection_id);
        return;
    }
    if verb == "reject" {
        report(
            reject_bounty(ticket_arg(parts, 1), char.name, char.is_admin, rest_from(raw, 2)),
            connection_id
        );
        return;
    }
    send_client_message(connection_id, usage());
}

fn usage() {
    let out = "\x1b[1;36mbounty\x1b[0m — the builder help-wanted board\n";
    out += "  bounty                         everything open\n";
    out += "  bounty mine                    what you have claimed\n";
    out += "  bounty show <n>                one bounty in full\n";
    out += "  bounty post <points> <title>   ask for something to be built\n";
    out += "  bounty claim <n>               take it (claims expire after 3 days)\n";
    out += "  bounty drop <n>                give it back\n";
    out += "  bounty submit <n> [vnums]      say it is done\n";
    out += "  bounty accept <n>              pay it — requester or admin\n";
    out += "  bounty reject <n> <reason>     send it back to the board\n";
    out += "\n\x1b[0;37mPaid in builder points. SYSTEM bounties come from the content auditor\n";
    out += "and close themselves when the problem stops occurring.\x1b[0m";
    return out;
}

// `try_parse_int`, not the built-in: Rhai's `parse_int` raises on non-numeric
// input, and a raised error escapes run_command instead of printing usage. See
// src/script/utilities.rs.
fn ticket_arg(parts, idx) {
    if parts.len() <= idx { return 0; }
    let n = try_parse_int(parts[idx]);
    if n == () { return 0; }
    return n;
}

fn rest_from(parts, idx) {
    let out = "";
    let i = idx;
    while i < parts.len() {
        if i > idx { out += " "; }
        out += parts[i];
        i += 1;
    }
    return out;
}

fn report(outcome, connection_id) {
    if outcome.ok {
        send_client_message(connection_id, "\x1b[1;32m" + outcome.message + "\x1b[0m");
    } else {
        send_client_message(connection_id, "\x1b[1;31m" + outcome.message + "\x1b[0m");
    }
}

// ===== posting =====

fn do_post(char, parts, raw, connection_id) {
    if parts.len() < 3 {
        send_client_message(
            connection_id,
            "Usage: bounty post <points> <title>\nExample: bounty post 60 A blacksmith for Oakvale, with a dialogue tree"
        );
        return;
    }
    let points = try_parse_int(parts[1]);
    if points == () {
        send_client_message(connection_id, "The points have to be a number: bounty post 60 <title>");
        return;
    }
    if points < 1 {
        send_client_message(connection_id, "A bounty has to be worth at least one point.");
        return;
    }
    // From `raw`, so the title keeps the capitals the poster typed.
    let title = rest_from(raw, 2);

    // The area you are standing in, so a request lands where the work is
    // without anybody having to name it.
    let area_label = "";
    let room = get_room_data(char.current_room_id);
    if room != () {
        let area = get_area_data(room.area_id);
        if area != () { area_label = area.prefix; }
    }

    let result = post_bounty(char.name, "any", area_label, title, "", points);
    if result.ok {
        send_client_message(connection_id, "\x1b[1;32mPosted as bounty #" + result.ticket + ".\x1b[0m");
    } else {
        send_client_message(connection_id, "\x1b[1;31m" + result.message + "\x1b[0m");
    }
}

// ===== rendering =====

fn status_colour(status) {
    if status == "open" { return "\x1b[1;32m"; }
    if status == "claimed" { return "\x1b[1;33m"; }
    if status == "submitted" { return "\x1b[1;36m"; }
    // No "rejected" arm: rejecting returns work to Open. See RequestStatus.
    return "\x1b[90m";
}

fn column(value, width) {
    let s = "" + value;
    if s.len() > width - 1 {
        s = s.sub_string(0, width - 2) + "…";
    }
    return text::pad_right(s, width);
}

fn show_board(rows, heading, connection_id) {
    if rows.len() == 0 {
        send_client_message(
            connection_id,
            "Nothing on the board. `bounty post <points> <title>` asks for something."
        );
        return;
    }

    let out = "\x1b[1;36m" + heading + "\x1b[0m  \x1b[0;37m" + rows.len() + " item(s)\x1b[0m\n";
    for r in rows {
        // Who asked matters as much as what: a SYSTEM row is the auditor
        // pointing at a real problem, and it closes itself when fixed. A named
        // row is a person waiting on you.
        let who = column(r.requester, 9);
        if r.system { who = "\x1b[0;36m" + column("SYSTEM", 9) + "\x1b[0m"; }

        out += "  \x1b[1;37m#" + text::pad_right("" + r.ticket, 4) + "\x1b[0m"
            + status_colour(r.status) + text::pad_right(r.status, 11) + "\x1b[0m"
            + text::pad_left("" + r.points, 4) + " pts  "
            + who
            + column(r.area, 10)
            + column(r.title, 40);
        if r.claimed_by != "" {
            out += "  \x1b[0;37m<- " + r.claimed_by + "\x1b[0m";
        }
        out += "\n";
    }
    out += "\n\x1b[0;37m'bounty show <n>' for one, 'bounty claim <n>' to take it.\x1b[0m";
    send_client_message(connection_id, out);
}

fn show_one(ticket, connection_id) {
    if ticket < 1 {
        send_client_message(connection_id, "Which bounty? `bounty show <number>`");
        return;
    }
    let r = get_bounty(ticket);
    if !r.found {
        send_client_message(connection_id, "No bounty numbered " + ticket + ".");
        return;
    }

    let out = "\x1b[1;36m#" + r.ticket + "  " + r.title + "\x1b[0m\n";
    out += "  " + status_colour(r.status) + r.status + "\x1b[0m"
        + "   \x1b[1;33m" + r.points + " points\x1b[0m"
        + "   asked by " + r.requester;
    if r.area != "" { out += "   in " + r.area; }
    out += "\n";
    if r.kind != "any" { out += "  Wants: " + r.kind + "\n"; }
    if r.detail != "" { out += "\n  " + r.detail + "\n"; }
    if r.claimed_by != "" { out += "\n  Claimed by " + r.claimed_by + "\n"; }
    if r.fulfilled_by != "" { out += "  Submitted by " + r.fulfilled_by + "\n"; }
    if r.linked.len() > 0 {
        out += "  Delivered: ";
        let n = 0;
        for v in r.linked {
            if n > 0 { out += ", "; }
            out += v;
            n += 1;
        }
        out += "\n";
    }
    if r.notes.len() > 0 {
        out += "\n\x1b[1;37mNotes\x1b[0m\n";
        for note in r.notes {
            out += "  \x1b[0;37m" + note.author + ":\x1b[0m " + note.message + "\n";
        }
    }
    send_client_message(connection_id, out);
}
