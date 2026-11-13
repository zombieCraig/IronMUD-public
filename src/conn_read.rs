//! Pieces of the per-connection read task (`handle_read_char_mode`).
//!
//! The read loop itself stays in `lib.rs`; it owns the socket, the telnet
//! parser and the negotiation timer. Everything it does with the bytes it
//! reads lives here, one function per concern:
//!
//! - [`idle_timed_out`] — the inactivity disconnect check
//! - [`handle_telnet_event`] — WILL/WONT/DO/DONT and subnegotiation (NAWS,
//!   TTYPE/MTTS, MSDP)
//! - [`handle_key`] — one decoded key in character mode (modern editor,
//!   line submit, readline editing)
//! - [`handle_line_byte`] — one byte in fallback line mode
//!
//! None of these hold the Connections lock across a send, and none touch the
//! World lock.

use super::*;
use crate::telnet::*;

/// The read-task locals the handlers share.
#[derive(Clone, Copy)]
pub(crate) struct ReadCx<'a> {
    pub addr: std::net::SocketAddr,
    pub connection_id: ConnectionId,
    pub connections: &'a SharedConnections,
    pub tx_input: &'a mpsc::Sender<InputEvent>,
    pub tx_raw: &'a mpsc::UnboundedSender<Vec<u8>>,
}

/// Whether the reader keeps going after handling some input.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadFlow {
    Continue,
    /// Drop the connection (the reason was already sent to the client).
    Disconnect,
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// True once both ECHO and SGA have been agreed for this session.
pub(crate) fn session_char_mode(cx: &ReadCx<'_>) -> bool {
    cx.connections
        .lock()
        .get(&cx.connection_id)
        .map(|s| s.telnet_state.char_mode)
        .unwrap_or(false)
}

/// Idle disconnect — true when the session's `last_activity_time` has not
/// advanced within `IDLE_DISCONNECT_SECS`. Reclaims sockets/RAM held by idle
/// or abandoned connections (HIGH-3 in the security audit). Admins can disable
/// it globally via the `idle_disconnect_enabled` setting (set to "false"); the
/// setting is only consulted once a session is already past the threshold, so
/// this sled read stays rare and cheap. Sends the notice when it fires.
pub(crate) fn idle_timed_out(cx: &ReadCx<'_>, db: &crate::db::Db) -> bool {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let idle_secs = cx
        .connections
        .lock()
        .get(&cx.connection_id)
        .map(|s| now_secs.saturating_sub(s.last_activity_time))
        .unwrap_or(0);
    if idle_secs <= IDLE_DISCONNECT_SECS {
        return false;
    }
    let enabled = db
        .get_setting_or_default("idle_disconnect_enabled", "true")
        .unwrap_or_else(|_| "true".to_string())
        != "false";
    if enabled {
        info!("Client {} idle for {}s, disconnecting.", cx.addr, idle_secs);
        let _ = cx.tx_raw.send(b"\r\nDisconnected for inactivity.\r\n".to_vec());
    }
    enabled
}

// ---------------------------------------------------------------------------
// Telnet negotiation
// ---------------------------------------------------------------------------

/// Apply one telnet protocol event. Returns true when it switched the session
/// into character mode (ECHO and SGA both agreed).
pub(crate) fn handle_telnet_event(cx: &ReadCx<'_>, event: TelnetEvent) -> bool {
    match event {
        TelnetEvent::Will(opt) => handle_will(cx, opt),
        TelnetEvent::Wont(opt) => {
            // Client refused option
            let _ = cx.tx_raw.send(build_negotiation(DONT, opt));
            false
        }
        TelnetEvent::Do(opt) => handle_do(cx, opt),
        TelnetEvent::Dont(opt) => {
            // Client refused our option
            let _ = cx.tx_raw.send(build_negotiation(WONT, opt));
            false
        }
        TelnetEvent::Subnegotiation(opt, data) => {
            match opt {
                OPT_NAWS => handle_naws(cx, &data),
                OPT_TTYPE => handle_ttype_is(cx, &data),
                OPT_MSDP => handle_msdp(cx, &data),
                _ => {}
            }
            false
        }
        _ => false,
    }
}

/// Client offers an option (`IAC WILL opt`).
fn handle_will(cx: &ReadCx<'_>, opt: u8) -> bool {
    let mut conns = cx.connections.lock();
    let Some(session) = conns.get_mut(&cx.connection_id) else {
        return false;
    };
    match opt {
        OPT_SGA => {
            // Send DO response for SGA
            let _ = cx.tx_raw.send(respond_to_will(opt));
            session.telnet_state.suppress_go_ahead = true;
            // Both sides agreeing to SGA enables char mode
            if session.telnet_state.echo {
                session.telnet_state.char_mode = true;
                return true;
            }
        }
        OPT_MXP => {
            // Client supports MXP - auto-enable it. Don't send DO MXP: we
            // already sent it in the initial negotiation, and answering again
            // would loop forever.
            if !session.telnet_state.mxp_supported {
                session.telnet_state.mxp_supported = true;
                session.mxp_enabled = true;
                let _ = cx.tx_raw.send(build_mxp_activation());
                debug!("MXP auto-enabled for connection {}", cx.connection_id);
            }
        }
        OPT_TTYPE => {
            // Client supports TTYPE - start MTTS 3-stage negotiation (we
            // already sent DO TTYPE in the initial negotiation).
            if session.telnet_state.ttype_stage == 0 {
                session.telnet_state.ttype_stage = 1;
                let _ = cx.tx_raw.send(build_ttype_send());
            }
        }
        OPT_MSDP => {
            session.telnet_state.msdp_supported = true;
        }
        _ => {
            // Send standard response for other options
            let _ = cx.tx_raw.send(respond_to_will(opt));
        }
    }
    false
}

/// Client asks us to enable an option (`IAC DO opt`).
fn handle_do(cx: &ReadCx<'_>, opt: u8) -> bool {
    let _ = cx.tx_raw.send(respond_to_do(opt));

    let mut conns = cx.connections.lock();
    let Some(session) = conns.get_mut(&cx.connection_id) else {
        return false;
    };
    match opt {
        OPT_ECHO => {
            session.telnet_state.echo = true;
            if session.telnet_state.suppress_go_ahead {
                session.telnet_state.char_mode = true;
                return true;
            }
        }
        OPT_SGA => {
            session.telnet_state.suppress_go_ahead = true;
        }
        OPT_MSDP => {
            session.telnet_state.msdp_supported = true;
        }
        _ => {}
    }
    false
}

/// NAWS: the client's window size.
fn handle_naws(cx: &ReadCx<'_>, data: &[u8]) {
    if let Some((width, height)) = parse_naws(data) {
        let mut conns = cx.connections.lock();
        if let Some(session) = conns.get_mut(&cx.connection_id) {
            session.telnet_state.window_width = width;
            session.telnet_state.window_height = height;
        }
    }
}

/// TTYPE IS response — one step of the MTTS 3-stage negotiation.
fn handle_ttype_is(cx: &ReadCx<'_>, data: &[u8]) {
    let Some(ttype_str) = parse_ttype_is(data) else {
        return;
    };
    let mut conns = cx.connections.lock();
    let Some(session) = conns.get_mut(&cx.connection_id) else {
        return;
    };
    match session.telnet_state.ttype_stage {
        1 => {
            // Stage 1: Client name (e.g., "MUDLET")
            session.telnet_state.client_name = Some(ttype_str);
            session.telnet_state.ttype_stage = 2;
            let _ = cx.tx_raw.send(build_ttype_send());
        }
        2 => {
            // Stage 2: Terminal type (e.g., "XTERM-256COLOR")
            session.telnet_state.terminal_type = Some(ttype_str);
            session.telnet_state.ttype_stage = 3;
            let _ = cx.tx_raw.send(build_ttype_send());
        }
        3 => {
            // Stage 3: MTTS bitvector or repeated value
            if let Some(flags) = parse_mtts_flags(&ttype_str) {
                session.telnet_state.mtts_flags = flags;
                session.telnet_state.utf8_supported = (flags & MTTS_UTF8) != 0;
            }
            session.telnet_state.ttype_stage = 4; // Complete

            // Dynamically enable server-side echo for standard terminal
            // emulators. This avoids triggering password masking mode in
            // dedicated MUD clients.
            if should_enable_server_echo(&session.telnet_state) {
                debug!("Enabling server-side echo for terminal client {}", cx.connection_id);
                let _ = cx.tx_raw.send(build_negotiation(WILL, OPT_ECHO));
            }

            let ts = &session.telnet_state;
            info!(
                "Client {} capabilities - name: {:?}, type: {:?}, MTTS: {} (UTF-8: {}, ANSI: {}, 256color: {})",
                cx.connection_id,
                ts.client_name,
                ts.terminal_type,
                ts.mtts_flags,
                ts.utf8_supported,
                (ts.mtts_flags & MTTS_ANSI) != 0,
                (ts.mtts_flags & MTTS_256_COLORS) != 0,
            );
        }
        _ => {}
    }
}

/// MSDP REPORT / UNREPORT requests.
fn handle_msdp(cx: &ReadCx<'_>, data: &[u8]) {
    let vars = parse_msdp(data);
    let mut conns = cx.connections.lock();
    let Some(session) = conns.get_mut(&cx.connection_id) else {
        return;
    };
    for (var, val) in vars {
        if var == "REPORT" {
            debug!("MSDP Client {} requested REPORT for {}", cx.connection_id, val);
            if let Some(name) = msdp_reportable(&val) {
                session.telnet_state.msdp_reported_variables.insert(name.to_string());
            }
        } else if var == "UNREPORT" {
            debug!("MSDP Client {} requested UNREPORT for {}", cx.connection_id, val);
            session.telnet_state.msdp_reported_variables.remove(&val.to_uppercase());
        }
    }
}

// ---------------------------------------------------------------------------
// Character mode
// ---------------------------------------------------------------------------

/// Feed one byte through the session's escape-sequence state machine,
/// returning a key once one is complete.
pub(crate) fn parse_key(cx: &ReadCx<'_>, byte: u8) -> Option<KeyEvent> {
    let mut conns = cx.connections.lock();
    let session = conns.get_mut(&cx.connection_id)?;
    let (new_state, event) = parse_key_byte(std::mem::take(&mut session.escape_state), byte);
    session.escape_state = new_state;
    event
}

/// Handle one decoded key in character mode.
pub(crate) fn handle_key(cx: &ReadCx<'_>, key: KeyEvent) -> ReadFlow {
    // While a modern editor owns the screen, every key goes to it.
    if route_to_modern_editor(cx, &key) {
        return ReadFlow::Continue;
    }

    let (conns, id, tx_raw) = (cx.connections, cx.connection_id, cx.tx_raw);
    match key {
        KeyEvent::Tab => {
            if cx.tx_input.try_send(InputEvent::Tab).is_err() {
                let _ = tx_raw.send(b"\r\nSlow down -- input queue full.\r\n".to_vec());
            }
        }
        KeyEvent::Enter => submit_line(cx),
        KeyEvent::Backspace => handle_readline_backspace(conns, id, tx_raw),
        KeyEvent::Delete => handle_readline_delete(conns, id, tx_raw),
        KeyEvent::ArrowUp => handle_readline_history_up(conns, id, tx_raw),
        KeyEvent::ArrowDown => handle_readline_history_down(conns, id, tx_raw),
        KeyEvent::ArrowLeft => handle_readline_cursor_left(conns, id, tx_raw),
        KeyEvent::ArrowRight => handle_readline_cursor_right(conns, id, tx_raw),
        KeyEvent::Home | KeyEvent::CtrlA => handle_readline_cursor_home(conns, id, tx_raw),
        KeyEvent::End | KeyEvent::CtrlE => handle_readline_cursor_end(conns, id, tx_raw),
        KeyEvent::CtrlT => handle_readline_transpose(conns, id, tx_raw),
        KeyEvent::CtrlW => handle_readline_delete_word(conns, id, tx_raw),
        KeyEvent::CtrlU => handle_readline_delete_to_start(conns, id, tx_raw),
        KeyEvent::CtrlK => handle_readline_delete_to_end(conns, id, tx_raw),
        KeyEvent::CtrlC => handle_readline_cancel_line(conns, id, tx_raw),
        KeyEvent::CtrlL => handle_readline_redraw_screen(conns, id, tx_raw),
        KeyEvent::CtrlD => {
            // Logout on empty line
            let empty = conns
                .lock()
                .get(&id)
                .map(|s| s.input_buffer.is_empty())
                .unwrap_or(false);
            if empty {
                let _ = cx.tx_input.try_send(InputEvent::Line("quit".to_string()));
            }
        }
        KeyEvent::Char(c) => {
            // Measure the line actually being edited, so backspacing and
            // retyping never trips the cap.
            let current = conns.lock().get(&id).map(|s| s.input_buffer.len()).unwrap_or(0);
            if current + c.len_utf8() > MAX_INPUT_LINE {
                let _ = tx_raw.send(b"\r\nInput line too long.\r\n".to_vec());
                info!("Client {} sent oversized line, disconnecting.", cx.addr);
                return ReadFlow::Disconnect;
            }
            handle_readline_insert_char(conns, id, c, tx_raw);
        }
        KeyEvent::Unknown => {}
        // Modern-editor-only keys (and mouse reports, which are only enabled
        // inside the editor); ignore in readline mode.
        KeyEvent::CtrlX
        | KeyEvent::CtrlO
        | KeyEvent::CtrlG
        | KeyEvent::CtrlY
        | KeyEvent::CtrlZ
        | KeyEvent::CtrlBackslash
        | KeyEvent::CtrlUnderscore
        | KeyEvent::PageUp
        | KeyEvent::PageDown
        | KeyEvent::MouseClick { .. }
        | KeyEvent::ShiftArrowUp
        | KeyEvent::ShiftArrowDown
        | KeyEvent::ShiftArrowLeft
        | KeyEvent::ShiftArrowRight
        | KeyEvent::ShiftHome
        | KeyEvent::ShiftEnd
        | KeyEvent::CtrlQ
        | KeyEvent::CtrlV
        | KeyEvent::BracketedPasteStart
        | KeyEvent::BracketedPasteEnd => {}
    }
    ReadFlow::Continue
}

/// When a modern (beta) editor session is active on this connection, hand it
/// the key, flush what it rendered, and forward save/cancel to the main loop.
/// Returns false when no editor is open (the key is for readline).
fn route_to_modern_editor(cx: &ReadCx<'_>, key: &KeyEvent) -> bool {
    use crate::editor::EditorAction;

    let routed = {
        let mut conns = cx.connections.lock();
        conns.get_mut(&cx.connection_id).and_then(|session| {
            let (w, h) = (session.telnet_state.window_width, session.telnet_state.window_height);
            session.modern_editor.as_mut().map(|editor| {
                editor.set_size(w, h);
                let action = editor.handle_key(key);
                (action, editor.take_output())
            })
        })
    };
    let Some((action, bytes)) = routed else {
        return false;
    };

    if !bytes.is_empty() {
        let _ = cx.tx_raw.send(bytes);
    }
    match action {
        EditorAction::None => {}
        EditorAction::Save => {
            let content = cx
                .connections
                .lock()
                .get_mut(&cx.connection_id)
                .and_then(|s| s.modern_editor.take())
                .map(|e| e.take_text())
                .unwrap_or_default();
            let _ = cx.tx_input.try_send(InputEvent::ModernEditorSave(content));
        }
        EditorAction::Cancel => {
            if let Some(s) = cx.connections.lock().get_mut(&cx.connection_id) {
                s.modern_editor = None;
            }
            let _ = cx.tx_input.try_send(InputEvent::ModernEditorCancel);
        }
    }
    true
}

/// Enter: take the edited line, record it in history, and queue it.
///
/// History skips anything typed before login and any line carrying a
/// password or other secret (`player_log::is_sensitive_line`).
fn submit_line(cx: &ReadCx<'_>) {
    let line = {
        let mut conns = cx.connections.lock();
        match conns.get_mut(&cx.connection_id) {
            Some(session) => {
                let line = session.input_buffer.trim().to_string();
                if !line.is_empty() && session.character.is_some() && !player_log::is_sensitive_line(&line) {
                    let repeat = session.command_history.front().is_some_and(|last| last == &line);
                    if !repeat {
                        session.command_history.push_front(line.clone());
                        if session.command_history.len() > MAX_HISTORY_SIZE {
                            session.command_history.pop_back();
                        }
                    }
                }
                session.input_buffer.clear();
                session.cursor_pos = 0;
                session.history_index = None;
                session.saved_input.clear();
                line
            }
            None => String::new(),
        }
    };
    let _ = cx.tx_raw.send(b"\r\n".to_vec());
    if cx.tx_input.try_send(InputEvent::Line(line)).is_err() {
        let _ = cx.tx_raw.send(b"Slow down -- input queue full.\r\n".to_vec());
    }
}

// ---------------------------------------------------------------------------
// Line mode (clients that never agreed to character mode)
// ---------------------------------------------------------------------------

/// Accumulate one byte of a line-mode line, queueing the line at LF.
pub(crate) fn handle_line_byte(cx: &ReadCx<'_>, line_buffer: &mut Vec<u8>, byte: u8) -> ReadFlow {
    match byte {
        CHAR_CR => {}
        CHAR_LF => {
            let line = decode_line_mode_input(line_buffer);
            line_buffer.clear();
            if cx.tx_input.try_send(InputEvent::RawLine(line)).is_err() {
                let _ = cx.tx_raw.send(b"Slow down -- input queue full.\r\n".to_vec());
            }
        }
        // Backspace / DEL from clients without local editing: drop the last
        // whole character.
        0x08 | 0x7f => pop_utf8_char(line_buffer),
        _ => {
            if line_buffer.len() + 1 > MAX_INPUT_LINE {
                // Pre-auth memory DoS guard. Drop the connection rather than
                // silently truncating — a legitimate client never sends an
                // 8 KiB+ line.
                let _ = cx.tx_raw.send(b"\r\nInput line too long.\r\n".to_vec());
                info!("Client {} sent oversized line, disconnecting.", cx.addr);
                return ReadFlow::Disconnect;
            }
            line_buffer.push(byte);
        }
    }
    ReadFlow::Continue
}

/// A line-mode input line: bytes decoded as UTF-8 (invalid sequences become
/// U+FFFD), control characters other than tab removed, then trimmed.
fn decode_line_mode_input(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect::<String>()
        .trim()
        .to_string()
}

/// Remove the last UTF-8 character (all of its bytes) from `buf`.
fn pop_utf8_char(buf: &mut Vec<u8>) {
    while let Some(b) = buf.pop() {
        // Continuation bytes are 0b10xxxxxx; stop after the lead byte.
        if b & 0xC0 != 0x80 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        connections: SharedConnections,
        id: ConnectionId,
        tx_input: mpsc::Sender<InputEvent>,
        rx_input: mpsc::Receiver<InputEvent>,
        tx_raw: mpsc::UnboundedSender<Vec<u8>>,
        _rx_raw: mpsc::UnboundedReceiver<Vec<u8>>,
        _rx_client: mpsc::UnboundedReceiver<String>,
    }

    impl Harness {
        fn new() -> Self {
            let (tx_client, _rx_client) = mpsc::unbounded_channel();
            let (tx_input, rx_input) = mpsc::channel(8);
            let (tx_raw, _rx_raw) = mpsc::unbounded_channel();
            let id = Uuid::new_v4();
            let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
            connections
                .lock()
                .insert(id, PlayerSession::new_for_test(tx_client, tx_input.clone()));
            Harness {
                connections,
                id,
                tx_input,
                rx_input,
                tx_raw,
                _rx_raw,
                _rx_client,
            }
        }

        fn cx(&self) -> ReadCx<'_> {
            ReadCx {
                addr: "127.0.0.1:4000".parse().unwrap(),
                connection_id: self.id,
                connections: &self.connections,
                tx_input: &self.tx_input,
                tx_raw: &self.tx_raw,
            }
        }
    }

    #[test]
    fn line_mode_decodes_utf8_and_handles_backspace() {
        assert_eq!(decode_line_mode_input("say héllo ✓\r".as_bytes()), "say héllo ✓");
        assert_eq!(decode_line_mode_input(b"say \x1b[31mred"), "say [31mred");
        let mut buf = "ab✓".as_bytes().to_vec();
        pop_utf8_char(&mut buf);
        assert_eq!(buf, b"ab");
        pop_utf8_char(&mut buf);
        assert_eq!(buf, b"a");
        let mut empty = Vec::new();
        pop_utf8_char(&mut empty);
        assert!(empty.is_empty());
    }

    #[test]
    fn echo_and_sga_in_either_order_enable_char_mode() {
        let h = Harness::new();
        assert!(!handle_telnet_event(&h.cx(), TelnetEvent::Will(OPT_SGA)));
        assert!(handle_telnet_event(&h.cx(), TelnetEvent::Do(OPT_ECHO)));
        assert!(session_char_mode(&h.cx()));

        let h = Harness::new();
        assert!(!handle_telnet_event(&h.cx(), TelnetEvent::Do(OPT_ECHO)));
        assert!(handle_telnet_event(&h.cx(), TelnetEvent::Will(OPT_SGA)));
        assert!(session_char_mode(&h.cx()));
    }

    #[test]
    fn naws_and_ttype_stages_update_the_session() {
        let h = Harness::new();
        handle_telnet_event(&h.cx(), TelnetEvent::Subnegotiation(OPT_NAWS, vec![0, 120, 0, 40]));
        handle_telnet_event(&h.cx(), TelnetEvent::Will(OPT_TTYPE));
        handle_telnet_event(&h.cx(), TelnetEvent::Subnegotiation(OPT_TTYPE, b"\x00MUDLET".to_vec()));
        handle_telnet_event(&h.cx(), TelnetEvent::Subnegotiation(OPT_TTYPE, b"\x00XTERM".to_vec()));
        let conns = h.connections.lock();
        let ts = &conns[&h.id].telnet_state;
        assert_eq!((ts.window_width, ts.window_height), (120, 40));
        assert_eq!(ts.client_name.as_deref(), Some("MUDLET"));
        assert_eq!(ts.terminal_type.as_deref(), Some("XTERM"));
        assert_eq!(ts.ttype_stage, 3);
    }

    #[test]
    fn enter_queues_the_line_but_keeps_pre_login_input_out_of_history() {
        let mut h = Harness::new();
        for c in "  hello ".chars() {
            assert_eq!(handle_key(&h.cx(), KeyEvent::Char(c)), ReadFlow::Continue);
        }
        handle_key(&h.cx(), KeyEvent::Enter);
        match h.rx_input.try_recv() {
            Ok(InputEvent::Line(l)) => assert_eq!(l, "hello"),
            other => panic!("expected a line, got {:?}", other.map(|_| ())),
        }
        let conns = h.connections.lock();
        let s = &conns[&h.id];
        assert!(s.input_buffer.is_empty());
        assert!(s.command_history.is_empty(), "no character yet: nothing recorded");
    }

    #[test]
    fn oversized_input_disconnects_in_both_modes() {
        let h = Harness::new();
        h.connections.lock().get_mut(&h.id).unwrap().input_buffer = "x".repeat(MAX_INPUT_LINE);
        assert_eq!(handle_key(&h.cx(), KeyEvent::Char('y')), ReadFlow::Disconnect);

        let mut buf = vec![b'x'; MAX_INPUT_LINE];
        assert_eq!(handle_line_byte(&h.cx(), &mut buf, b'y'), ReadFlow::Disconnect);
    }

    #[test]
    fn line_mode_queues_a_raw_line_at_lf() {
        let mut h = Harness::new();
        let mut buf = Vec::new();
        for b in b"look\r\n" {
            assert_eq!(handle_line_byte(&h.cx(), &mut buf, *b), ReadFlow::Continue);
        }
        match h.rx_input.try_recv() {
            Ok(InputEvent::RawLine(l)) => assert_eq!(l, "look"),
            other => panic!("expected a raw line, got {:?}", other.map(|_| ())),
        }
        assert!(buf.is_empty());
    }
}
