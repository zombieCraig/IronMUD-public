//! Readline-like input handling for the telnet input loop.
//!
//! Each function mutates the per-connection `PlayerSession` input state
//! (buffer, cursor, history) and writes the corresponding ANSI sequence
//! through the raw output channel. Called from the input dispatch loop
//! in `lib.rs::handle_connection` after `telnet::parse_key_event` decodes
//! a control byte.

use tokio::sync::mpsc;

use crate::telnet;
use crate::{ConnectionId, SharedConnections};

pub fn handle_readline_backspace(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            let char_count = session.input_buffer.chars().count();

            if session.cursor_pos >= char_count {
                // Cursor at end - simple case, but handle variable width chars
                if let Some(ch) = session.input_buffer.pop() {
                    let char_width = telnet::display_width(&ch.to_string());
                    session.cursor_pos -= 1;
                    // Move left, overwrite with spaces, move left again
                    let mut output = Vec::new();
                    output.extend(telnet::ansi::cursor_left(char_width));
                    output.extend(vec![b' '; char_width]);
                    output.extend(telnet::ansi::cursor_left(char_width));
                    let _ = tx_raw.send(output);
                }
            } else {
                // Cursor in middle - need to redraw
                let chars: Vec<char> = session.input_buffer.chars().collect();
                let remove_idx = session.cursor_pos - 1;
                let mut new_buffer = String::new();
                for (i, c) in chars.iter().enumerate() {
                    if i != remove_idx {
                        new_buffer.push(*c);
                    }
                }
                session.input_buffer = new_buffer;
                session.cursor_pos -= 1;

                // Redraw from cursor position
                let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
                let _ = tx_raw.send(output);
            }
            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

pub fn handle_readline_delete(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Remove character at cursor position
            let chars: Vec<char> = session.input_buffer.chars().collect();
            let mut new_buffer = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i != session.cursor_pos {
                    new_buffer.push(*c);
                }
            }
            session.input_buffer = new_buffer;

            // Redraw from cursor position
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

pub fn handle_readline_cursor_left(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            // Get the character we're moving over and its display width
            if let Some(ch) = session.input_buffer.chars().nth(session.cursor_pos - 1) {
                let char_width = telnet::display_width(&ch.to_string());
                session.cursor_pos -= 1;
                let _ = tx_raw.send(telnet::ansi::cursor_left(char_width));
            }
        }
    }
}

pub fn handle_readline_cursor_right(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Get the character we're moving over and its display width
            if let Some(ch) = session.input_buffer.chars().nth(session.cursor_pos) {
                let char_width = telnet::display_width(&ch.to_string());
                session.cursor_pos += 1;
                let _ = tx_raw.send(telnet::ansi::cursor_right(char_width));
            }
        }
    }
}

pub fn handle_readline_cursor_home(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            // Calculate display width of characters from start to cursor
            let cols_to_move = telnet::display_width_up_to(&session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(telnet::ansi::cursor_left(cols_to_move));
            session.cursor_pos = 0;
        }
    }
}

pub fn handle_readline_cursor_end(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            // Calculate display width of characters from cursor to end
            let chars_after: String = session.input_buffer.chars().skip(session.cursor_pos).collect();
            let cols_to_move = telnet::display_width(&chars_after);
            let _ = tx_raw.send(telnet::ansi::cursor_right(cols_to_move));
            session.cursor_pos = char_count;
        }
    }
}

pub fn handle_readline_history_up(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.command_history.is_empty() {
            return;
        }

        let new_index = match session.history_index {
            None => {
                // Save current input before navigating
                session.saved_input = session.input_buffer.clone();
                Some(0)
            }
            Some(idx) if idx + 1 < session.command_history.len() => Some(idx + 1),
            Some(_) => return, // Already at oldest
        };

        if let Some(idx) = new_index {
            session.history_index = Some(idx);
            let history_line = session.command_history[idx].clone();

            // Clear current line and display history entry
            let output = telnet::redraw_input_line("> ", &history_line, history_line.chars().count());
            session.input_buffer = history_line;
            session.cursor_pos = session.input_buffer.chars().count();
            let _ = tx_raw.send(output);
        }
    }
}

pub fn handle_readline_history_down(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        match session.history_index {
            None => return, // Already at newest
            Some(0) => {
                // Return to saved input
                session.history_index = None;
                let restored = session.saved_input.clone();
                let output = telnet::redraw_input_line("> ", &restored, restored.chars().count());
                session.input_buffer = restored;
                session.cursor_pos = session.input_buffer.chars().count();
                let _ = tx_raw.send(output);
            }
            Some(idx) => {
                session.history_index = Some(idx - 1);
                let history_line = session.command_history[idx - 1].clone();
                let output = telnet::redraw_input_line("> ", &history_line, history_line.chars().count());
                session.input_buffer = history_line;
                session.cursor_pos = session.input_buffer.chars().count();
                let _ = tx_raw.send(output);
            }
        }
    }
}

pub fn handle_readline_transpose(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let chars: Vec<char> = session.input_buffer.chars().collect();
        let len = chars.len();

        // Need at least 2 characters, and cursor must not be at position 0
        if len < 2 || session.cursor_pos == 0 {
            let _ = tx_raw.send(telnet::ansi::bell());
            return;
        }

        let (a, b) = if session.cursor_pos >= len {
            // At end of line: swap last two characters
            (len - 2, len - 1)
        } else {
            // Mid-line: swap char before cursor with char at cursor, advance cursor
            (session.cursor_pos - 1, session.cursor_pos)
        };

        let mut new_chars = chars;
        new_chars.swap(a, b);
        session.input_buffer = new_chars.into_iter().collect();

        // Advance cursor (unless already at end)
        if session.cursor_pos < len {
            session.cursor_pos += 1;
        }

        let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
        let _ = tx_raw.send(output);

        // Reset history navigation on edit
        session.history_index = None;
    }
}

pub fn handle_readline_delete_word(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos == 0 {
            return;
        }

        // Find word boundary (skip trailing spaces, then skip word chars)
        let chars: Vec<char> = session.input_buffer.chars().collect();
        let mut new_pos = session.cursor_pos;

        // Skip spaces backward
        while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }
        // Skip word backward
        while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
            new_pos -= 1;
        }

        if new_pos < session.cursor_pos {
            // Remove characters from new_pos to cursor_pos
            let mut new_buffer = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i < new_pos || i >= session.cursor_pos {
                    new_buffer.push(*c);
                }
            }
            session.input_buffer = new_buffer;
            session.cursor_pos = new_pos;

            // Redraw line
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

pub fn handle_readline_delete_to_start(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        if session.cursor_pos > 0 {
            let chars: Vec<char> = session.input_buffer.chars().collect();
            session.input_buffer = chars[session.cursor_pos..].iter().collect();
            session.cursor_pos = 0;

            let output = telnet::redraw_input_line("> ", &session.input_buffer, 0);
            let _ = tx_raw.send(output);

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

pub fn handle_readline_delete_to_end(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        let char_count = session.input_buffer.chars().count();
        if session.cursor_pos < char_count {
            let chars: Vec<char> = session.input_buffer.chars().collect();
            session.input_buffer = chars[..session.cursor_pos].iter().collect();

            // Clear to end of line
            let _ = tx_raw.send(telnet::ansi::clear_to_eol());

            // Reset history navigation on edit
            session.history_index = None;
        }
    }
}

pub fn handle_readline_cancel_line(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        // Display ^C and start new line
        let _ = tx_raw.send(b"^C\r\n> ".to_vec());
        session.input_buffer.clear();
        session.cursor_pos = 0;
        session.history_index = None;
        session.saved_input.clear();
    }
}

pub fn handle_readline_redraw_screen(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let conns = connections.lock().unwrap();
    if let Some(session) = conns.get(&connection_id) {
        // Clear screen and redraw
        let mut output = b"\x1b[2J\x1b[H".to_vec(); // Clear screen, home cursor
        output.extend(telnet::redraw_input_line(
            "> ",
            &session.input_buffer,
            session.cursor_pos,
        ));
        let _ = tx_raw.send(output);
    }
}

pub fn handle_readline_insert_char(
    connections: &SharedConnections,
    connection_id: ConnectionId,
    c: char,
    tx_raw: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut conns = connections.lock().unwrap();
    if let Some(session) = conns.get_mut(&connection_id) {
        // Refuse to grow the line past the global cap. Outer read loop
        // disconnects on the same condition; this is belt-and-braces for
        // any non-keystroke caller (tab completion, paste handling).
        if session.input_buffer.len() + c.len_utf8() > crate::MAX_INPUT_LINE {
            return;
        }
        let char_count = session.input_buffer.chars().count();

        if session.cursor_pos >= char_count {
            // Append at end (simple case)
            session.input_buffer.push(c);
            session.cursor_pos += 1;
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf);
            let _ = tx_raw.send(bytes.as_bytes().to_vec());
        } else {
            // Insert in middle
            let chars: Vec<char> = session.input_buffer.chars().collect();
            let mut new_buffer = String::new();
            for (i, ch) in chars.iter().enumerate() {
                if i == session.cursor_pos {
                    new_buffer.push(c);
                }
                new_buffer.push(*ch);
            }
            session.input_buffer = new_buffer;
            session.cursor_pos += 1;

            // Redraw from insertion point
            let output = telnet::redraw_input_line("> ", &session.input_buffer, session.cursor_pos);
            let _ = tx_raw.send(output);
        }

        // Reset history navigation on edit
        session.history_index = None;
    }
}
