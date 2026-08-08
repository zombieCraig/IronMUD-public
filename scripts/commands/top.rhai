// scripts/commands/top.rhai
//
//   top             - the renown board, and a pointer at the rest
//   top boards      - every board that exists, grouped
//   top me          - every board you rank on, best placing first
//   top <board>     - one board
//
// Boards are rebuilt by a background scan every few minutes, not on the
// command — ranking needs every character in the world, and that is not a
// thing to do while someone is waiting. The "Updated N ago" line is there so
// a player who just did the thing knows why the number has not moved yet.
//
// Every board is discovered from the data: any achievement counter anyone has
// bumped, any skill anyone has trained, any faction anyone has a standing
// with is already a board. See `src/leaderboard.rs`.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to see the boards.");
        return;
    }

    if get_leaderboard_generated_at() == 0 {
        send_client_message(
            connection_id,
            "The boards have not been drawn up yet. Try again in a few minutes."
        );
        return;
    }

    let word = text::normalise(args);

    if word == "" {
        show_board(char, "renown", connection_id);
        return;
    }
    if word == "boards" || word == "list" {
        show_index(char, connection_id);
        return;
    }
    if word == "me" || word == "mine" || word == "self" {
        show_placings(char, connection_id);
        return;
    }

    show_board(char, word, connection_id);
}

// ===== one board =====

fn show_board(char, key, connection_id) {
    let board = get_leaderboard(key);
    if board.found && board.group == "Building" && !can_see_building(char) {
        send_client_message(connection_id, "There is no board by that name.");
        return;
    }
    if !board.found {
        send_client_message(
            connection_id,
            "No board by that name. Use 'top boards' to see what is ranked."
        );
        return;
    }

    let output = "\x1b[1;36m" + board.label + "\x1b[0m";
    if board.ranked > board.entries.len() {
        output += "  \x1b[90m(top " + board.entries.len() + " of " + board.ranked + ")\x1b[0m";
    }
    output += "\n";

    for row in board.entries {
        let mine = row.name.to_lower() == char.name.to_lower();
        output += render_row(row, mine);
    }

    // A board shows ten rows; most players are not in them. Telling someone
    // they are 34th of 214 is the difference between a board that includes
    // them and a board that is about other people.
    let placing = get_leaderboard_placing(char.name, board.key);
    if placing > 0 && !shown(board.entries, char.name) {
        output += "\x1b[90m  ...\x1b[0m\n";
        output += "\x1b[1;33m" + text::pad_left("" + placing, 4) + "  " + text::pad_right(char.name, 22) + "\x1b[0m\n";
    }
    if placing == 0 {
        output += "\x1b[90mYou do not rank here yet.\x1b[0m\n";
    }

    output += freshness_line();
    send_client_message(connection_id, output);
}

fn render_row(row, mine) {
    let colour = "";
    if mine { colour = "\x1b[1;33m"; }
    if !mine && row.rank == 1 { colour = "\x1b[1;37m"; }

    let line = colour + text::pad_left("" + row.rank, 4) + "  " + text::pad_right(row.name, 22) + row.display;
    if colour != "" { line += "\x1b[0m"; }
    return line + "\n";
}

fn shown(entries, name) {
    let want = name.to_lower();
    for row in entries {
        if row.name.to_lower() == want { return true; }
    }
    return false;
}

// ===== the index =====

// Builder boards rank a different field on different rules, and a player has
// no way to compete on them. They are listed and resolvable only for the
// people they are about.
fn can_see_building(char) {
    return char.is_builder || char.is_admin;
}

fn show_index(char, connection_id) {
    let index = get_leaderboard_index();
    if index.len() == 0 {
        send_client_message(connection_id, "Nothing is ranked yet.");
        return;
    }

    let output = "\x1b[1;36mBoards\x1b[0m\n";
    let group = "";
    let names = [];

    for board in index {
        if board.group == "Building" && !can_see_building(char) { continue; }
        if board.group != group {
            output += flush_group(group, names);
            group = board.group;
            names = [];
        }
        names.push(board.key);
    }
    output += flush_group(group, names);

    output += "\nUse 'top <board>' for one of them, or 'top me' for where you stand.";
    output += freshness_line();
    send_client_message(connection_id, output);
}

// One group of board names, wrapped four to a line. A group that has grown
// past a screenful — the per-mobile kill tallies do, one key per prototype —
// is summarised instead of listed, so the index stays an index.
fn flush_group(group, names) {
    if group == "" || names.len() == 0 { return ""; }

    let out = "\x1b[1;33m" + group + "\x1b[0m\n";
    let listed = names;
    let extra = 0;
    if names.len() > 12 {
        listed = [];
        let i = 0;
        while i < 8 {
            listed.push(names[i]);
            i += 1;
        }
        extra = names.len() - 8;
    }

    let line = "  ";
    let n = 0;
    for name in listed {
        line += text::pad_right(name, 20);
        n += 1;
        if n % 4 == 0 {
            out += line + "\n";
            line = "  ";
        }
    }
    if n % 4 != 0 { out += line + "\n"; }
    if extra > 0 {
        out += "  \x1b[90m...and " + extra + " more; 'top <name>' finds any of them.\x1b[0m\n";
    }
    return out;
}

// ===== where one player stands =====

fn show_placings(char, connection_id) {
    // Same gate as show_board and show_index. Without it a demoted builder
    // reads "Rooms Built - 2nd of 4" here and then gets "There is no board by
    // that name" from `top build.rooms`.
    let mine = [];
    for board in get_leaderboard_placings(char.name) {
        if board.group == "Building" && !can_see_building(char) { continue; }
        mine.push(board);
    }
    if mine.len() == 0 {
        send_client_message(
            connection_id,
            "You do not rank on any board yet. Everything you do counts toward one."
        );
        return;
    }

    let output = "\x1b[1;36mWhere you stand\x1b[0m  \x1b[90m(of "
        + get_leaderboard_population() + " who have played)\x1b[0m\n";
    for board in mine {
        let colour = "";
        if board.rank <= 3 { colour = "\x1b[1;33m"; }
        output += colour + text::pad_left(ordinal(board.rank), 6) + "\x1b[0m  "
            + text::pad_right(board.label, 26)
            + "\x1b[90mof " + board.ranked + "\x1b[0m\n";
    }
    output += freshness_line();
    send_client_message(connection_id, output);
}

// ===== shared =====

fn freshness_line() {
    let age = now_unix_secs() - get_leaderboard_generated_at();
    if age < 90 { return "\x1b[90mUpdated just now.\x1b[0m"; }
    let minutes = age / 60;
    if minutes < 60 { return "\x1b[90mUpdated " + minutes + " minutes ago.\x1b[0m"; }
    let hours = minutes / 60;
    if hours < 24 { return "\x1b[90mUpdated " + hours + " hours ago.\x1b[0m"; }
    return "\x1b[90mUpdated " + (hours / 24) + " days ago.\x1b[0m";
}

fn ordinal(n) {
    let tail = n % 10;
    let teens = n % 100;
    if teens >= 11 && teens <= 13 { return "" + n + "th"; }
    if tail == 1 { return "" + n + "st"; }
    if tail == 2 { return "" + n + "nd"; }
    if tail == 3 { return "" + n + "rd"; }
    return "" + n + "th";
}

