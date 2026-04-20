// src/script/utilities.rs
// MXP, ANSI colors, AFK, terminal size, and text formatting functions

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::SharedConnections;

/// Helper function to escape special characters for MXP
pub fn escape_mxp(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

/// Helper function to strip MXP tags from text
pub fn strip_mxp_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }

    result
}

/// Register utility functions (MXP, ANSI colors, AFK, terminal, text formatting)
pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections) {
    // ========== MXP (MUD eXtension Protocol) Functions ==========

    // is_mxp_enabled(connection_id) -> bool - Check if MXP is enabled for connection
    let conns = connections.clone();
    engine.register_fn("is_mxp_enabled", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.mxp_enabled).unwrap_or(false)
        } else {
            false
        }
    });

    // set_mxp_enabled(connection_id, enabled) -> bool - Toggle MXP for connection
    let conns = connections.clone();
    engine.register_fn("set_mxp_enabled", move |connection_id: String, enabled: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.mxp_enabled = enabled;
                return true;
            }
        }
        false
    });

    // mxp_send(command, display) -> string - Generate MXP send tag
    // Example: mxp_send("look", "Look") -> <send href="look">Look</send>
    engine.register_fn("mxp_send", |command: String, display: String| {
        format!("<send href=\"{}\">{}</send>", escape_mxp(&command), display)
    });

    // mxp_menu(commands, hints, display) -> string - Generate MXP popup menu
    // Example: mxp_menu(["cmd1", "cmd2"], ["Option 1", "Option 2"], "Click")
    // -> <send "cmd1|cmd2" hint="Menu|Option 1|Option 2">Click</send>
    engine.register_fn("mxp_menu", |commands: rhai::Array, hints: rhai::Array, display: String| {
        let cmds: Vec<String> = commands.into_iter()
            .filter_map(|d| d.try_cast::<String>())
            .collect();
        let hint_strs: Vec<String> = hints.into_iter()
            .filter_map(|d| d.try_cast::<String>())
            .collect();

        let cmd_str = cmds.join("|");
        let hint_str = format!("Menu|{}", hint_strs.join("|"));

        format!("<send \"{}\" hint=\"{}\">{}</send>",
                escape_mxp(&cmd_str), escape_mxp(&hint_str), display)
    });

    // mxp_or(mxp_text, plain_text, connection_id) -> string
    // Returns MXP version if MXP enabled, plain text otherwise
    let conns = connections.clone();
    engine.register_fn("mxp_or", move |mxp_text: String, plain_text: String, connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if conns.get(&uuid).map(|s| s.mxp_enabled).unwrap_or(false) {
                return mxp_text;
            }
        }
        plain_text
    });

    // send_mxp_message(connection_id, message) - Send message with MXP secure line prefix
    // If MXP disabled, strips tags and sends plain text
    let conns = connections.clone();
    engine.register_fn("send_mxp_message", move |connection_id: String, message: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                let output = if session.mxp_enabled {
                    // Send with MXP secure line prefix (ESC [ 1 z)
                    format!("\x1b[1z{}\n", message)
                } else {
                    // Strip MXP tags for plain text clients
                    format!("{}\n", strip_mxp_tags(&message))
                };
                let _ = session.sender.send(output);
            }
        }
    });

    // strip_mxp_tags(text) -> string - Remove MXP tags from text
    engine.register_fn("strip_mxp_tags", |text: String| {
        strip_mxp_tags(&text)
    });

    // ========== ANSI Color Functions ==========

    // ANSI color code constants
    const ANSI_RESET: &str = "\x1b[0m";
    const ANSI_BLACK: &str = "\x1b[30m";
    const ANSI_RED: &str = "\x1b[31m";
    const ANSI_GREEN: &str = "\x1b[32m";
    const ANSI_YELLOW: &str = "\x1b[33m";
    const ANSI_BLUE: &str = "\x1b[34m";
    const ANSI_MAGENTA: &str = "\x1b[35m";
    const ANSI_CYAN: &str = "\x1b[36m";
    const ANSI_WHITE: &str = "\x1b[37m";
    // Bright variants
    const ANSI_BRIGHT_BLACK: &str = "\x1b[90m";
    const ANSI_BRIGHT_RED: &str = "\x1b[91m";
    const ANSI_BRIGHT_GREEN: &str = "\x1b[92m";
    const ANSI_BRIGHT_YELLOW: &str = "\x1b[93m";
    const ANSI_BRIGHT_BLUE: &str = "\x1b[94m";
    const ANSI_BRIGHT_MAGENTA: &str = "\x1b[95m";
    const ANSI_BRIGHT_CYAN: &str = "\x1b[96m";
    const ANSI_BRIGHT_WHITE: &str = "\x1b[97m";

    // is_colors_enabled(connection_id) -> bool
    let conns = connections.clone();
    engine.register_fn("is_colors_enabled", move |connection_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.colors_enabled).unwrap_or(false)
        } else {
            false
        }
    });

    // set_colors_enabled(connection_id, enabled) -> bool
    let conns = connections.clone();
    engine.register_fn("set_colors_enabled", move |connection_id: String, enabled: bool| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.colors_enabled = enabled;
                return true;
            }
        }
        false
    });

    // is_room_flags_enabled(connection_id) -> bool - Check if room flags display is enabled
    let conns = connections.clone();
    engine.register_fn("is_room_flags_enabled", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.show_room_flags).unwrap_or(false)
        } else {
            false
        }
    });

    // set_room_flags_enabled(connection_id, enabled) -> bool
    // Also persists the setting to CharacterData
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("set_room_flags_enabled", move |connection_id: String, enabled: bool| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.show_room_flags = enabled;
                // Also persist to character data
                if let Some(ref mut character) = session.character {
                    character.show_room_flags = enabled;
                    // Save to database
                    if let Err(e) = cloned_db.save_character_data(character.clone()) {
                        tracing::error!("Failed to save show_room_flags setting: {}", e);
                    }
                }
                return true;
            }
        }
        false
    });

    // ========== AFK (Away From Keyboard) Functions ==========

    // is_afk(connection_id) -> bool - Check if connection is AFK
    let conns = connections.clone();
    engine.register_fn("is_afk", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.afk).unwrap_or(false)
        } else {
            false
        }
    });

    // set_afk(connection_id, is_afk) -> bool - Set AFK status for connection
    let conns = connections.clone();
    engine.register_fn("set_afk", move |connection_id: String, is_afk: bool| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.afk = is_afk;
                return true;
            }
        }
        false
    });

    // clear_afk(connection_id) - Clear AFK status (convenience function)
    let conns = connections.clone();
    engine.register_fn("clear_afk", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                session.afk = false;
            }
        }
    });

    // is_player_afk(char_name) -> bool - Check AFK status by character name
    let conns = connections.clone();
    engine.register_fn("is_player_afk", move |char_name: String| {
        let conns = conns.lock().unwrap();
        for session in conns.values() {
            if let Some(ref character) = session.character {
                if character.name.eq_ignore_ascii_case(&char_name) {
                    return session.afk;
                }
            }
        }
        false
    });

    // ========== Command Abbreviation Functions ==========

    // is_abbrev_enabled(connection_id) -> bool - Check if command abbreviations are enabled
    let conns = connections.clone();
    engine.register_fn("is_abbrev_enabled", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid).map(|s| s.abbrev_enabled).unwrap_or(true)
        } else {
            true
        }
    });

    // set_abbrev_enabled(connection_id, enabled) -> bool - Set abbreviation mode for connection
    let conns = connections.clone();
    engine.register_fn(
        "set_abbrev_enabled",
        move |connection_id: String, enabled: bool| {
            if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
                let mut conns = conns.lock().unwrap();
                if let Some(session) = conns.get_mut(&uuid) {
                    session.abbrev_enabled = enabled;
                    return true;
                }
            }
            false
        },
    );

    // ========== Idle Detection Functions ==========
    // Idle is automatic (unlike AFK) - computed from last_activity_time
    // Default idle threshold is 300 seconds (5 minutes), configurable via settings

    // is_idle(connection_id) -> bool - Check if connection is idle
    let conns = connections.clone();
    let cloned_db_idle = db.clone();
    engine.register_fn("is_idle", move |connection_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                let idle_threshold: i64 = cloned_db_idle
                    .get_setting_or_default("idle_timeout_secs", "300")
                    .unwrap_or_else(|_| "300".to_string())
                    .parse::<i64>()
                    .unwrap_or(300)
                    .max(30);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                return (now - session.last_activity_time) > idle_threshold;
            }
        }
        false
    });

    // get_idle_duration(connection_id) -> i64 - Get seconds since last activity
    let conns = connections.clone();
    engine.register_fn("get_idle_duration", move |connection_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                return now - session.last_activity_time;
            }
        }
        0
    });

    // is_player_idle(char_name) -> bool - Check idle status by character name
    let conns = connections.clone();
    let cloned_db_idle2 = db.clone();
    engine.register_fn("is_player_idle", move |char_name: String| {
        let conns = conns.lock().unwrap();
        for session in conns.values() {
            if let Some(ref character) = session.character {
                if character.name.eq_ignore_ascii_case(&char_name) {
                    let idle_threshold: i64 = cloned_db_idle2
                        .get_setting_or_default("idle_timeout_secs", "300")
                        .unwrap_or_else(|_| "300".to_string())
                        .parse::<i64>()
                        .unwrap_or(300)
                        .max(30);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    return (now - session.last_activity_time) > idle_threshold;
                }
            }
        }
        false
    });

    // ========== Builder Debug Channel Functions ==========

    // broadcast_to_builders(message) - Send message to builders with debug enabled
    let conns = connections.clone();
    engine.register_fn("broadcast_to_builders", move |message: String| {
        crate::broadcast_to_builders(&conns, &message);
    });

    // is_builder_debug_enabled(connection_id) -> bool - Check if builder debug is enabled
    let conns = connections.clone();
    engine.register_fn("is_builder_debug_enabled", move |connection_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if let Some(session) = conns.get(&uuid) {
                if let Some(ref character) = session.character {
                    return character.builder_debug_enabled;
                }
            }
        }
        false
    });

    // set_builder_debug_enabled(connection_id, enabled) -> bool - Set builder debug flag
    let conns = connections.clone();
    let cloned_db = db.clone();
    engine.register_fn("set_builder_debug_enabled", move |connection_id: String, enabled: bool| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let mut conns = conns.lock().unwrap();
            if let Some(session) = conns.get_mut(&uuid) {
                if let Some(ref mut character) = session.character {
                    character.builder_debug_enabled = enabled;
                    // Persist to database
                    let _ = cloned_db.save_character_data(character.clone());
                    return true;
                }
            }
        }
        false
    });

    // Color wrapper functions - wrap text in color codes with auto-reset
    engine.register_fn("ansi_black", |text: String| format!("{}{}{}", ANSI_BLACK, text, ANSI_RESET));
    engine.register_fn("ansi_red", |text: String| format!("{}{}{}", ANSI_RED, text, ANSI_RESET));
    engine.register_fn("ansi_green", |text: String| format!("{}{}{}", ANSI_GREEN, text, ANSI_RESET));
    engine.register_fn("ansi_yellow", |text: String| format!("{}{}{}", ANSI_YELLOW, text, ANSI_RESET));
    engine.register_fn("ansi_blue", |text: String| format!("{}{}{}", ANSI_BLUE, text, ANSI_RESET));
    engine.register_fn("ansi_magenta", |text: String| format!("{}{}{}", ANSI_MAGENTA, text, ANSI_RESET));
    engine.register_fn("ansi_cyan", |text: String| format!("{}{}{}", ANSI_CYAN, text, ANSI_RESET));
    engine.register_fn("ansi_white", |text: String| format!("{}{}{}", ANSI_WHITE, text, ANSI_RESET));

    // Bright color variants
    engine.register_fn("ansi_bright_black", |text: String| format!("{}{}{}", ANSI_BRIGHT_BLACK, text, ANSI_RESET));
    engine.register_fn("ansi_bright_red", |text: String| format!("{}{}{}", ANSI_BRIGHT_RED, text, ANSI_RESET));
    engine.register_fn("ansi_bright_green", |text: String| format!("{}{}{}", ANSI_BRIGHT_GREEN, text, ANSI_RESET));
    engine.register_fn("ansi_bright_yellow", |text: String| format!("{}{}{}", ANSI_BRIGHT_YELLOW, text, ANSI_RESET));
    engine.register_fn("ansi_bright_blue", |text: String| format!("{}{}{}", ANSI_BRIGHT_BLUE, text, ANSI_RESET));
    engine.register_fn("ansi_bright_magenta", |text: String| format!("{}{}{}", ANSI_BRIGHT_MAGENTA, text, ANSI_RESET));
    engine.register_fn("ansi_bright_cyan", |text: String| format!("{}{}{}", ANSI_BRIGHT_CYAN, text, ANSI_RESET));
    engine.register_fn("ansi_bright_white", |text: String| format!("{}{}{}", ANSI_BRIGHT_WHITE, text, ANSI_RESET));

    // color_or(colored_text, plain_text, connection_id) -> string
    // Returns colored_text if colors enabled, plain_text otherwise
    let conns = connections.clone();
    engine.register_fn("color_or", move |colored_text: String, plain_text: String, connection_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            if conns.get(&uuid).map(|s| s.colors_enabled).unwrap_or(false) {
                return colored_text;
            }
        }
        plain_text
    });

    // strip_ansi(text) -> string - removes all ANSI escape sequences
    engine.register_fn("strip_ansi", |text: String| -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm'
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    });

    // ========== Terminal Size Functions (NAWS) ==========

    // get_terminal_width(connection_id) -> int - Get client's terminal width (default 80)
    let conns = connections.clone();
    engine.register_fn("get_terminal_width", move |connection_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid)
                .map(|s| s.telnet_state.window_width as i64)
                .unwrap_or(80)
        } else {
            80
        }
    });

    // get_terminal_height(connection_id) -> int - Get client's terminal height (default 24)
    let conns = connections.clone();
    engine.register_fn("get_terminal_height", move |connection_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&connection_id) {
            let conns = conns.lock().unwrap();
            conns.get(&uuid)
                .map(|s| s.telnet_state.window_height as i64)
                .unwrap_or(24)
        } else {
            24
        }
    });

    // wrap_text(text, width) -> string - Word-wrap text to specified width
    engine.register_fn("wrap_text", |text: String, width: i64| -> String {
        let width = width.max(10) as usize; // Minimum width of 10
        let mut result = String::new();

        for line in text.lines() {
            if line.len() <= width {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            let mut current_line = String::new();
            for word in line.split_whitespace() {
                if current_line.is_empty() {
                    current_line = word.to_string();
                } else if current_line.len() + 1 + word.len() <= width {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    result.push_str(&current_line);
                    result.push('\n');
                    current_line = word.to_string();
                }
            }
            if !current_line.is_empty() {
                result.push_str(&current_line);
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one
        if !text.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        result
    });

    // join(array, separator) -> string - Join array elements with separator
    engine.register_fn("join", |arr: rhai::Array, sep: String| -> String {
        arr.into_iter()
            .filter_map(|d| d.try_cast::<String>())
            .collect::<Vec<_>>()
            .join(&sep)
    });

    // format_columns(items, width, col_padding) -> string - Format items into columns
    // Automatically calculates number of columns based on longest item and terminal width
    // ANSI-aware: uses visible length (ignoring escape sequences) for width calculations
    engine.register_fn("format_columns", |items: rhai::Array, width: i64, padding: i64| -> String {
        let items: Vec<String> = items.into_iter()
            .filter_map(|d| d.try_cast::<String>())
            .collect();

        if items.is_empty() {
            return String::new();
        }

        let width = width.max(20) as usize;
        let padding = padding.max(1) as usize;

        // Visible length ignoring ANSI escape sequences
        let visible_len = |s: &str| -> usize {
            let mut len = 0;
            let mut in_escape = false;
            for c in s.chars() {
                if c == '\x1b' {
                    in_escape = true;
                } else if in_escape {
                    if c == 'm' {
                        in_escape = false;
                    }
                } else {
                    len += 1;
                }
            }
            len
        };

        // Find longest item by visible length
        let max_len = items.iter().map(|s| visible_len(s)).max().unwrap_or(0);
        let col_width = max_len + padding;

        // Calculate number of columns
        let num_cols = (width / col_width).max(1);

        let mut result = String::new();
        for (i, item) in items.iter().enumerate() {
            result.push_str(item);
            let vis_len = visible_len(item);
            if vis_len < col_width {
                result.push_str(&" ".repeat(col_width - vis_len));
            }
            if (i + 1) % num_cols == 0 {
                result.push('\n');
            }
        }

        // Add final newline if needed
        if !result.ends_with('\n') {
            result.push('\n');
        }

        result
    });

    // ========== Safe Parsing Functions ==========

    // try_parse_int(s) -> int or () - Parse string to integer, returning () on failure
    // Unlike Rhai's built-in parse_int which throws on non-numeric input,
    // this returns () so callers can check `if result == ()` safely.
    engine.register_fn("try_parse_int", |s: String| -> rhai::Dynamic {
        match s.parse::<i64>() {
            Ok(n) => rhai::Dynamic::from(n),
            Err(_) => rhai::Dynamic::UNIT,
        }
    });

    // ========== Argument Parsing Functions ==========

    // split_quoted_args(text) -> array - Split string into args, respecting quoted strings
    // Example: split_quoted_args("add foo \"hello world\" bar") -> ["add", "foo", "hello world", "bar"]
    engine.register_fn("split_quoted_args", |text: String| -> Vec<rhai::Dynamic> {
        let mut args = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if in_quotes {
                if ch == quote_char {
                    // End of quoted string
                    in_quotes = false;
                    if !current.is_empty() {
                        args.push(rhai::Dynamic::from(std::mem::take(&mut current)));
                    }
                } else {
                    current.push(ch);
                }
            } else if ch == '"' || ch == '\'' {
                // Start of quoted string
                in_quotes = true;
                quote_char = ch;
                // If we had content before the quote, push it
                if !current.is_empty() {
                    args.push(rhai::Dynamic::from(std::mem::take(&mut current)));
                }
            } else if ch.is_whitespace() {
                // End of unquoted argument
                if !current.is_empty() {
                    args.push(rhai::Dynamic::from(std::mem::take(&mut current)));
                }
            } else {
                current.push(ch);
            }
        }

        // Don't forget the last argument
        if !current.is_empty() {
            args.push(rhai::Dynamic::from(current));
        }

        args
    });
}
