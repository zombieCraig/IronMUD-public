// scripts/commands/board.rhai
//
// Bulletin board interaction (CircleMUD gen_board parity). Operates on
// ItemType::Board prototypes in the player's room, keyed by their vnum.
//
//   board                       -- list posts on the (single) board here
//   board list [<keyword>]      -- same, with explicit board pick when >1
//   board read <n> [<keyword>]  -- show post by 1-based index
//   board write <subject>       -- start a multi-line post (.save / .cancel)
//   board remove <n> [<keyword>] -- delete (own posts always; others=admin)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }

    let parts = args.split(" ");
    let cleaned = [];
    for p in parts {
        if p != "" { cleaned.push(p); }
    }

    let sub = if cleaned.len() == 0 { "list" } else { cleaned[0].to_lower() };

    if sub == "list" {
        let kw = if cleaned.len() >= 2 { cleaned[1] } else { "" };
        handle_list(char, connection_id, kw);
        return;
    }

    if sub == "read" {
        if cleaned.len() < 2 {
            send_client_message(connection_id, "Read which post? Try `board read <n>`.");
            return;
        }
        let n = try_parse_int(cleaned[1]);
        if n == () {
            send_client_message(connection_id, "Read which post? Try `board read <n>`.");
            return;
        }
        let kw = if cleaned.len() >= 3 { cleaned[2] } else { "" };
        handle_read(char, connection_id, n, kw);
        return;
    }

    if sub == "write" || sub == "post" {
        if cleaned.len() < 2 {
            send_client_message(connection_id, "Write what? Try `board write <subject>`.");
            return;
        }
        let subject = "";
        for i in 1..cleaned.len() {
            if i > 1 { subject += " "; }
            subject += cleaned[i];
        }
        handle_write(char, connection_id, subject);
        return;
    }

    if sub == "remove" || sub == "delete" {
        if cleaned.len() < 2 {
            send_client_message(connection_id, "Remove which post? Try `board remove <n>`.");
            return;
        }
        let n = try_parse_int(cleaned[1]);
        if n == () {
            send_client_message(connection_id, "Remove which post? Try `board remove <n>`.");
            return;
        }
        let kw = if cleaned.len() >= 3 { cleaned[2] } else { "" };
        handle_remove(char, connection_id, n, kw);
        return;
    }

    send_client_message(connection_id, "Usage: board [list|read <n>|write <subject>|remove <n>] [<keyword>]");
}

fn find_board_in_room(char, keyword) {
    let items = get_items_in_room(char.current_room_id);
    let boards = [];
    for it in items {
        if it.item_type == "board" { boards.push(it); }
    }
    if boards.len() == 0 { return (); }
    if keyword == "" {
        if boards.len() == 1 { return boards[0]; }
        return "ambiguous";
    }
    let lower = keyword.to_lower();
    for b in boards {
        if b.name.to_lower().contains(lower) { return b; }
        for kw in b.keywords {
            if kw.to_lower().starts_with(lower) { return b; }
        }
    }
    return ();
}

fn resolve_board(char, connection_id, keyword) {
    let board = find_board_in_room(char, keyword);
    if board == () {
        send_client_message(connection_id, "There is no bulletin board here.");
        return ();
    }
    if type_of(board) == "string" && board == "ambiguous" {
        send_client_message(connection_id, "Several boards here — which one? `board <list|read|write|remove> ... <keyword>`.");
        return ();
    }
    return board;
}

fn handle_list(char, connection_id, keyword) {
    let board = resolve_board(char, connection_id, keyword);
    if board == () { return; }

    if board.board_read_admin_only && !char.is_admin {
        send_client_message(connection_id, "The text on " + board.short_desc + " is unreadable to you.");
        return;
    }

    let posts = list_board_posts(board.vnum);
    if posts.len() == 0 {
        send_client_message(connection_id, board.short_desc + " is empty.");
        return;
    }

    let output = board.short_desc + " — " + posts.len() + " post(s):\n";
    let i = 1;
    for p in posts {
        output += "  " + i + ". " + p.subject + "  (" + p.author + ")\n";
        i += 1;
    }
    send_client_message(connection_id, output);
}

fn handle_read(char, connection_id, index, keyword) {
    let board = resolve_board(char, connection_id, keyword);
    if board == () { return; }

    if board.board_read_admin_only && !char.is_admin {
        send_client_message(connection_id, "The text on " + board.short_desc + " is unreadable to you.");
        return;
    }

    let post = get_board_post_by_index(board.vnum, index);
    if post == () {
        send_client_message(connection_id, "No such post.");
        return;
    }
    let header = "[#" + index + "] " + post.subject + " — by " + post.author;
    send_client_message(connection_id, header);
    send_client_message(connection_id, post.body);
}

fn handle_write(char, connection_id, subject) {
    let board = resolve_board(char, connection_id, "");
    if board == () { return; }

    if board.board_write_admin_only && !char.is_admin {
        send_client_message(connection_id, "You are not allowed to post on " + board.short_desc + ".");
        return;
    }

    if start_board_post(connection_id, board.vnum, subject) {
        if try_enter_modern_editor(connection_id) {
            return;
        }
        send_client_message(connection_id, "Posting to " + board.short_desc + " — subject: " + subject);
        send_client_message(connection_id, "Enter your post; .save to commit, .cancel to abort, .h for editor help.");
    } else {
        send_client_message(connection_id, "Could not start the editor.");
    }
}

fn handle_remove(char, connection_id, index, keyword) {
    let board = resolve_board(char, connection_id, keyword);
    if board == () { return; }

    let author = get_board_post_author_at(board.vnum, index);
    if author == "" {
        send_client_message(connection_id, "No such post.");
        return;
    }
    let is_author = author.to_lower() == char.name.to_lower();
    if !is_author && !char.is_admin {
        send_client_message(connection_id, "You may only remove your own posts.");
        return;
    }

    if delete_board_post_by_index(board.vnum, index) {
        send_client_message(connection_id, "Post removed.");
    } else {
        send_client_message(connection_id, "Could not remove that post.");
    }
}
