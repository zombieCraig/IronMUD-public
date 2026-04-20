//! Telnet protocol handling for IronMUD
//!
//! Implements RFC 854 (Telnet Protocol) with support for:
//! - IAC (Interpret As Command) sequence parsing
//! - Option negotiation (WILL/WONT/DO/DONT)
//! - Character-by-character mode for tab completion

use unicode_width::UnicodeWidthStr;

// Telnet protocol constants (RFC 854)
pub const IAC: u8 = 255; // Interpret As Command
pub const DONT: u8 = 254; // Refuse to perform option
pub const DO: u8 = 253; // Request to perform option
pub const WONT: u8 = 252; // Refuse to perform option
pub const WILL: u8 = 251; // Agree to perform option
pub const SB: u8 = 250; // Subnegotiation Begin
pub const GA: u8 = 249; // Go Ahead
pub const EL: u8 = 248; // Erase Line
pub const EC: u8 = 247; // Erase Character
pub const AYT: u8 = 246; // Are You There
pub const AO: u8 = 245; // Abort Output
pub const IP: u8 = 244; // Interrupt Process
pub const BRK: u8 = 243; // Break
pub const DM: u8 = 242; // Data Mark
pub const NOP: u8 = 241; // No Operation
pub const SE: u8 = 240; // Subnegotiation End

// Telnet options
pub const OPT_ECHO: u8 = 1; // Echo (RFC 857)
pub const OPT_SGA: u8 = 3; // Suppress Go Ahead (RFC 858)
pub const OPT_STATUS: u8 = 5; // Status (RFC 859)
pub const OPT_TIMING: u8 = 6; // Timing Mark (RFC 860)
pub const OPT_TTYPE: u8 = 24; // Terminal Type (RFC 1091)
pub const OPT_NAWS: u8 = 31; // Window Size (RFC 1073)
pub const OPT_LINEMODE: u8 = 34; // Linemode (RFC 1184)
pub const OPT_MXP: u8 = 91; // MUD eXtension Protocol

// TTYPE subnegotiation commands (RFC 1091)
pub const TTYPE_IS: u8 = 0; // Client sending terminal type
pub const TTYPE_SEND: u8 = 1; // Server requesting terminal type

// MTTS capability flags (Mud Terminal Type Standard)
// See: https://tintin.mudhalla.net/protocols/mtts/
pub const MTTS_ANSI: u32 = 1; // Supports ANSI color codes
pub const MTTS_VT100: u32 = 2; // Supports VT100 interface
pub const MTTS_UTF8: u32 = 4; // Uses UTF-8 character encoding
pub const MTTS_256_COLORS: u32 = 8; // Supports 256 color palette
pub const MTTS_MOUSE_TRACKING: u32 = 16; // Supports xterm mouse tracking
pub const MTTS_OSC_COLOR_PALETTE: u32 = 32; // Supports OSC color palette
pub const MTTS_SCREEN_READER: u32 = 64; // Screen reader in use
pub const MTTS_PROXY: u32 = 128; // Connection is via proxy
pub const MTTS_TRUECOLOR: u32 = 256; // Supports 24-bit truecolor

// Special input bytes
pub const CHAR_TAB: u8 = 0x09;
pub const CHAR_LF: u8 = 0x0A;
pub const CHAR_CR: u8 = 0x0D;
pub const CHAR_ESC: u8 = 0x1B;
pub const CHAR_BACKSPACE: u8 = 0x08;
pub const CHAR_DEL: u8 = 0x7F;

// Control character constants for readline-like input
pub const CTRL_A: u8 = 0x01; // Beginning of line
pub const CTRL_C: u8 = 0x03; // Cancel/interrupt
pub const CTRL_D: u8 = 0x04; // EOF/logout
pub const CTRL_E: u8 = 0x05; // End of line
pub const CTRL_K: u8 = 0x0B; // Kill to end of line
pub const CTRL_L: u8 = 0x0C; // Clear screen
pub const CTRL_U: u8 = 0x15; // Kill to beginning of line
pub const CTRL_T: u8 = 0x14; // Transpose characters
pub const CTRL_W: u8 = 0x17; // Delete word backward

/// Maximum command history size per session
pub const MAX_HISTORY_SIZE: usize = 100;

/// Telnet negotiation state for a connection
#[derive(Debug, Clone, Default)]
pub struct TelnetState {
    /// Server will echo characters back to client
    pub echo: bool,
    /// Suppress Go Ahead negotiated
    pub suppress_go_ahead: bool,
    /// Client is in character mode (not line mode)
    pub char_mode: bool,
    /// Negotiation is complete (timeout or responses received)
    pub negotiation_complete: bool,
    /// Client terminal width (from NAWS)
    pub window_width: u16,
    /// Client terminal height (from NAWS)
    pub window_height: u16,
    /// Client supports MXP (MUD eXtension Protocol)
    pub mxp_supported: bool,
    /// TTYPE negotiation stage (0=not started, 1-3=stage, 4=complete)
    pub ttype_stage: u8,
    /// Client name from TTYPE stage 1 (e.g., "MUDLET")
    pub client_name: Option<String>,
    /// Terminal type from TTYPE stage 2 (e.g., "XTERM-256COLOR")
    pub terminal_type: Option<String>,
    /// MTTS capability flags from stage 3
    pub mtts_flags: u32,
    /// Derived: client supports UTF-8 (from MTTS or assumed)
    pub utf8_supported: bool,
}

impl TelnetState {
    pub fn new() -> Self {
        Self {
            echo: false,
            suppress_go_ahead: false,
            char_mode: false,
            negotiation_complete: false,
            window_width: 80,
            window_height: 24,
            mxp_supported: false,
            ttype_stage: 0,
            client_name: None,
            terminal_type: None,
            mtts_flags: 0,
            utf8_supported: false,
        }
    }
}

/// State for parsing ANSI escape sequences and UTF-8 multi-byte characters across TCP reads
#[derive(Debug, Clone, Default)]
pub enum EscapeState {
    #[default]
    Normal,
    /// Received ESC (0x1B), waiting for next byte
    GotEsc,
    /// Received ESC [ (CSI), collecting parameter bytes
    GotCsi(Vec<u8>),
    /// Accumulating UTF-8 multi-byte sequence (buffer, expected total length)
    Utf8(Vec<u8>, usize),
}

/// Parsed input key events for readline-like handling
#[derive(Debug, Clone, PartialEq)]
pub enum KeyEvent {
    /// Regular character
    Char(char),
    /// Enter/Return key
    Enter,
    /// Tab key
    Tab,
    /// Backspace/Delete backward
    Backspace,
    /// Delete forward
    Delete,
    /// Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// Home (beginning of line)
    Home,
    /// End (end of line)
    End,
    /// Control key combinations
    CtrlA, // Beginning of line
    CtrlC, // Cancel/interrupt
    CtrlD, // EOF/logout
    CtrlE, // End of line
    CtrlK, // Kill to end of line
    CtrlL, // Clear screen/redraw
    CtrlU, // Kill to beginning of line
    CtrlT, // Transpose characters
    CtrlW, // Delete word backward
    /// Unknown/ignored
    Unknown,
}

/// Parse a single byte in the context of escape sequence state
/// Returns (updated_state, optional_key_event)
pub fn parse_key_byte(state: EscapeState, byte: u8) -> (EscapeState, Option<KeyEvent>) {
    match state {
        EscapeState::Normal => {
            match byte {
                CHAR_ESC => (EscapeState::GotEsc, None),
                CHAR_TAB => (EscapeState::Normal, Some(KeyEvent::Tab)),
                CHAR_CR | CHAR_LF => (EscapeState::Normal, Some(KeyEvent::Enter)),
                CHAR_BACKSPACE | CHAR_DEL => (EscapeState::Normal, Some(KeyEvent::Backspace)),
                CTRL_A => (EscapeState::Normal, Some(KeyEvent::CtrlA)),
                CTRL_C => (EscapeState::Normal, Some(KeyEvent::CtrlC)),
                CTRL_D => (EscapeState::Normal, Some(KeyEvent::CtrlD)),
                CTRL_E => (EscapeState::Normal, Some(KeyEvent::CtrlE)),
                CTRL_K => (EscapeState::Normal, Some(KeyEvent::CtrlK)),
                CTRL_L => (EscapeState::Normal, Some(KeyEvent::CtrlL)),
                CTRL_T => (EscapeState::Normal, Some(KeyEvent::CtrlT)),
                CTRL_U => (EscapeState::Normal, Some(KeyEvent::CtrlU)),
                CTRL_W => (EscapeState::Normal, Some(KeyEvent::CtrlW)),
                0x00..=0x1F => (EscapeState::Normal, Some(KeyEvent::Unknown)),
                _ => {
                    // Determine UTF-8 sequence length from lead byte
                    let expected = if byte & 0x80 == 0 {
                        1 // ASCII (0xxxxxxx)
                    } else if byte & 0xE0 == 0xC0 {
                        2 // 2-byte sequence (110xxxxx)
                    } else if byte & 0xF0 == 0xE0 {
                        3 // 3-byte sequence (1110xxxx) - includes Japanese
                    } else if byte & 0xF8 == 0xF0 {
                        4 // 4-byte sequence (11110xxx) - includes emoji
                    } else {
                        0 // Invalid lead byte (continuation byte or 0xFE/0xFF)
                    };

                    if expected == 1 {
                        // ASCII - handle directly
                        (EscapeState::Normal, Some(KeyEvent::Char(byte as char)))
                    } else if expected > 1 {
                        // Start UTF-8 multi-byte accumulation
                        (EscapeState::Utf8(vec![byte], expected), None)
                    } else {
                        // Invalid byte
                        (EscapeState::Normal, Some(KeyEvent::Unknown))
                    }
                }
            }
        }
        EscapeState::GotEsc => {
            match byte {
                b'[' => (EscapeState::GotCsi(Vec::new()), None),
                b'O' => (EscapeState::GotCsi(Vec::new()), None), // Some terminals use ESC O for arrows
                _ => (EscapeState::Normal, Some(KeyEvent::Unknown)),
            }
        }
        EscapeState::GotCsi(mut params) => {
            match byte {
                // Final byte range for CSI sequences
                b'A' => (EscapeState::Normal, Some(KeyEvent::ArrowUp)),
                b'B' => (EscapeState::Normal, Some(KeyEvent::ArrowDown)),
                b'C' => (EscapeState::Normal, Some(KeyEvent::ArrowRight)),
                b'D' => (EscapeState::Normal, Some(KeyEvent::ArrowLeft)),
                b'H' => (EscapeState::Normal, Some(KeyEvent::Home)),
                b'F' => (EscapeState::Normal, Some(KeyEvent::End)),
                b'~' => {
                    // Extended sequences: ESC [ n ~
                    let key = match params.as_slice() {
                        [b'1'] | [b'7'] => KeyEvent::Home,
                        [b'4'] | [b'8'] => KeyEvent::End,
                        [b'3'] => KeyEvent::Delete,
                        _ => KeyEvent::Unknown,
                    };
                    (EscapeState::Normal, Some(key))
                }
                // Parameter bytes (digits, semicolons)
                b'0'..=b'9' | b';' => {
                    params.push(byte);
                    (EscapeState::GotCsi(params), None)
                }
                // Timeout or unknown - reset
                _ => (EscapeState::Normal, Some(KeyEvent::Unknown)),
            }
        }
        EscapeState::Utf8(mut buf, expected) => {
            // Accumulating UTF-8 multi-byte sequence
            if byte & 0xC0 == 0x80 {
                // Valid continuation byte (10xxxxxx)
                buf.push(byte);
                if buf.len() == expected {
                    // Sequence complete - decode
                    if let Ok(s) = std::str::from_utf8(&buf) {
                        if let Some(c) = s.chars().next() {
                            return (EscapeState::Normal, Some(KeyEvent::Char(c)));
                        }
                    }
                    // Invalid UTF-8, return unknown
                    (EscapeState::Normal, Some(KeyEvent::Unknown))
                } else {
                    // Need more bytes
                    (EscapeState::Utf8(buf, expected), None)
                }
            } else {
                // Invalid continuation byte - abandon sequence
                (EscapeState::Normal, Some(KeyEvent::Unknown))
            }
        }
    }
}

/// ANSI escape sequences for cursor control
pub mod ansi {
    /// Move cursor left N positions
    pub fn cursor_left(n: usize) -> Vec<u8> {
        if n == 0 {
            return Vec::new();
        }
        format!("\x1b[{}D", n).into_bytes()
    }

    /// Move cursor right N positions
    pub fn cursor_right(n: usize) -> Vec<u8> {
        if n == 0 {
            return Vec::new();
        }
        format!("\x1b[{}C", n).into_bytes()
    }

    /// Clear from cursor to end of line
    pub fn clear_to_eol() -> Vec<u8> {
        b"\x1b[K".to_vec()
    }

    /// Clear entire line and move to beginning
    pub fn clear_line() -> Vec<u8> {
        b"\x1b[2K\r".to_vec()
    }

    /// Ring the terminal bell
    pub fn bell() -> Vec<u8> {
        b"\x07".to_vec()
    }
}

/// Calculate display width of a string in terminal columns.
/// Handles emoji, CJK characters, and combining marks correctly.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Calculate display width of characters up to a given character index.
/// Used for cursor positioning when cursor_pos is in character units.
pub fn display_width_up_to(s: &str, char_count: usize) -> usize {
    let prefix: String = s.chars().take(char_count).collect();
    UnicodeWidthStr::width(prefix.as_str())
}

/// Redraw the input line from scratch
/// This handles the common case of needing to update the display after
/// history navigation or major edits
pub fn redraw_input_line(prompt: &str, buffer: &str, cursor_pos: usize) -> Vec<u8> {
    let mut output = Vec::new();
    // Move to beginning, clear line
    output.push(b'\r');
    output.extend(ansi::clear_to_eol());
    // Write prompt and buffer
    output.extend_from_slice(prompt.as_bytes());
    output.extend_from_slice(buffer.as_bytes());
    // Position cursor if not at end (use display width for proper emoji/CJK handling)
    let char_count = buffer.chars().count();
    if cursor_pos < char_count {
        let chars_after: String = buffer.chars().skip(cursor_pos).collect();
        let cols_to_move = display_width(&chars_after);
        output.extend(ansi::cursor_left(cols_to_move));
    }
    output
}

/// Events parsed from telnet input stream
#[derive(Debug, Clone, PartialEq)]
pub enum TelnetEvent {
    /// Regular data bytes (with IAC sequences removed)
    Data(Vec<u8>),
    /// Client agrees to option: WILL <option>
    Will(u8),
    /// Client refuses option: WONT <option>
    Wont(u8),
    /// Client requests option: DO <option>
    Do(u8),
    /// Client refuses option request: DONT <option>
    Dont(u8),
    /// Subnegotiation data: SB <option> <data...> SE
    Subnegotiation(u8, Vec<u8>),
    /// Go Ahead signal
    GoAhead,
    /// Interrupt Process
    InterruptProcess,
}

/// Parser state machine states
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    Normal,
    InIAC,
    InOption(u8), // Holds the WILL/WONT/DO/DONT byte
    InSubnegotiation,
    #[allow(dead_code)]
    InSubnegotiationOption(u8), // Holds the option being negotiated (reserved for future use)
    InSubnegotiationData(u8), // Holds the option, collecting data
    InSubnegotiationIAC(u8),  // Saw IAC during subneg, waiting for SE or escaped IAC
}

/// Telnet protocol parser
///
/// Parses raw byte stream and separates data from telnet commands.
/// Handles IAC escaping (IAC IAC -> single 0xFF byte).
#[derive(Debug)]
pub struct TelnetParser {
    state: ParserState,
    subneg_buffer: Vec<u8>,
}

impl TelnetParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::Normal,
            subneg_buffer: Vec::new(),
        }
    }

    /// Process incoming bytes and return data and events
    ///
    /// Returns (data_bytes, events) where:
    /// - data_bytes: Application-level data with IAC sequences removed
    /// - events: Telnet protocol events (negotiations, etc.)
    pub fn process_bytes(&mut self, input: &[u8]) -> (Vec<u8>, Vec<TelnetEvent>) {
        let mut data = Vec::new();
        let mut events = Vec::new();

        for &byte in input {
            match self.state {
                ParserState::Normal => {
                    if byte == IAC {
                        self.state = ParserState::InIAC;
                    } else {
                        data.push(byte);
                    }
                }

                ParserState::InIAC => {
                    match byte {
                        IAC => {
                            // Escaped IAC: IAC IAC -> single 0xFF
                            data.push(IAC);
                            self.state = ParserState::Normal;
                        }
                        WILL => self.state = ParserState::InOption(WILL),
                        WONT => self.state = ParserState::InOption(WONT),
                        DO => self.state = ParserState::InOption(DO),
                        DONT => self.state = ParserState::InOption(DONT),
                        SB => self.state = ParserState::InSubnegotiation,
                        GA => {
                            events.push(TelnetEvent::GoAhead);
                            self.state = ParserState::Normal;
                        }
                        IP => {
                            events.push(TelnetEvent::InterruptProcess);
                            self.state = ParserState::Normal;
                        }
                        NOP | AYT | AO | EC | EL | BRK | DM => {
                            // Ignore these commands for now
                            self.state = ParserState::Normal;
                        }
                        _ => {
                            // Unknown command, return to normal
                            self.state = ParserState::Normal;
                        }
                    }
                }

                ParserState::InOption(cmd) => {
                    let event = match cmd {
                        WILL => TelnetEvent::Will(byte),
                        WONT => TelnetEvent::Wont(byte),
                        DO => TelnetEvent::Do(byte),
                        DONT => TelnetEvent::Dont(byte),
                        _ => unreachable!(),
                    };
                    events.push(event);
                    self.state = ParserState::Normal;
                }

                ParserState::InSubnegotiation => {
                    // First byte after SB is the option
                    self.subneg_buffer.clear();
                    self.state = ParserState::InSubnegotiationData(byte);
                }

                ParserState::InSubnegotiationOption(opt) => {
                    // This state isn't needed with current flow, but keeping for clarity
                    self.subneg_buffer.clear();
                    self.state = ParserState::InSubnegotiationData(opt);
                }

                ParserState::InSubnegotiationData(opt) => {
                    if byte == IAC {
                        self.state = ParserState::InSubnegotiationIAC(opt);
                    } else {
                        self.subneg_buffer.push(byte);
                    }
                }

                ParserState::InSubnegotiationIAC(opt) => {
                    if byte == SE {
                        // End of subnegotiation
                        events.push(TelnetEvent::Subnegotiation(opt, self.subneg_buffer.clone()));
                        self.subneg_buffer.clear();
                        self.state = ParserState::Normal;
                    } else if byte == IAC {
                        // Escaped IAC within subnegotiation
                        self.subneg_buffer.push(IAC);
                        self.state = ParserState::InSubnegotiationData(opt);
                    } else {
                        // Unexpected byte after IAC in subneg, treat as data
                        self.subneg_buffer.push(IAC);
                        self.subneg_buffer.push(byte);
                        self.state = ParserState::InSubnegotiationData(opt);
                    }
                }
            }
        }

        // If we have accumulated data, wrap it in an event
        if !data.is_empty() {
            // Return data separately from events for efficiency
        }

        (data, events)
    }

    /// Reset parser state (for connection reuse or error recovery)
    pub fn reset(&mut self) {
        self.state = ParserState::Normal;
        self.subneg_buffer.clear();
    }
}

impl Default for TelnetParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Build telnet negotiation sequence
pub fn build_negotiation(cmd: u8, option: u8) -> Vec<u8> {
    vec![IAC, cmd, option]
}

/// Build initial negotiation sequence for character mode
///
/// Sends:
/// - WILL ECHO (server will echo characters)
/// - WILL SGA (suppress go ahead)
/// - DO SGA (request client suppress go ahead)
/// - DO NAWS (request window size)
/// - DO TTYPE (request terminal type for MTTS detection)
/// - DO MXP (request MXP support)
pub fn build_initial_negotiations() -> Vec<u8> {
    let mut bytes = Vec::new();

    // Server will echo characters back
    bytes.extend_from_slice(&[IAC, WILL, OPT_ECHO]);

    // Suppress Go Ahead (required for character mode)
    bytes.extend_from_slice(&[IAC, WILL, OPT_SGA]);
    bytes.extend_from_slice(&[IAC, DO, OPT_SGA]);

    // Request window size
    bytes.extend_from_slice(&[IAC, DO, OPT_NAWS]);

    // Request terminal type (for MTTS client detection)
    bytes.extend_from_slice(&[IAC, DO, OPT_TTYPE]);

    // Request MXP support
    bytes.extend_from_slice(&[IAC, DO, OPT_MXP]);

    bytes
}

/// Build response to client's DO request
pub fn respond_to_do(option: u8) -> Vec<u8> {
    match option {
        OPT_ECHO | OPT_SGA => build_negotiation(WILL, option),
        _ => build_negotiation(WONT, option),
    }
}

/// Build response to client's WILL offer
pub fn respond_to_will(option: u8) -> Vec<u8> {
    match option {
        OPT_SGA | OPT_NAWS | OPT_TTYPE | OPT_MXP => build_negotiation(DO, option),
        _ => build_negotiation(DONT, option),
    }
}

/// Build MXP activation sequence
///
/// After client responds with WILL MXP, send this to enter MXP mode.
/// Format: IAC SB MXP IAC SE
pub fn build_mxp_activation() -> Vec<u8> {
    vec![IAC, SB, OPT_MXP, IAC, SE]
}

/// Build TTYPE SEND subnegotiation request
///
/// Requests the client to send its terminal type.
/// Used in the 3-stage MTTS negotiation.
/// Format: IAC SB TTYPE SEND IAC SE
pub fn build_ttype_send() -> Vec<u8> {
    vec![IAC, SB, OPT_TTYPE, TTYPE_SEND, IAC, SE]
}

/// Parse TTYPE IS subnegotiation response
///
/// Extracts the terminal type string from TTYPE IS subnegotiation data.
/// The data format is: [IS(0), ...terminal_type_bytes...]
///
/// Returns the terminal type string if valid.
pub fn parse_ttype_is(data: &[u8]) -> Option<String> {
    if data.len() > 1 && data[0] == TTYPE_IS {
        String::from_utf8(data[1..].to_vec()).ok()
    } else {
        None
    }
}

/// Parse MTTS bitvector from "MTTS <number>" string
///
/// MTTS-compliant clients send their capability flags as "MTTS <number>"
/// in the third stage of TTYPE negotiation.
///
/// Returns the flags value if the string is a valid MTTS response.
pub fn parse_mtts_flags(ttype: &str) -> Option<u32> {
    if ttype.starts_with("MTTS ") {
        ttype[5..].trim().parse().ok()
    } else {
        None
    }
}

/// Parse NAWS subnegotiation data
///
/// NAWS format: width_high width_low height_high height_low
pub fn parse_naws(data: &[u8]) -> Option<(u16, u16)> {
    if data.len() >= 4 {
        let width = ((data[0] as u16) << 8) | (data[1] as u16);
        let height = ((data[2] as u16) << 8) | (data[3] as u16);
        Some((width, height))
    } else {
        None
    }
}

/// Check if terminal supports OSC title updates
///
/// Returns true for terminals known to support OSC escape sequences
/// for setting window titles. Skips screen readers for accessibility.
pub fn supports_title_updates(state: &TelnetState) -> bool {
    // Skip for screen readers - title changes can be disruptive
    if (state.mtts_flags & MTTS_SCREEN_READER) != 0 {
        return false;
    }

    // Check terminal type for known supporters
    if let Some(ref ttype) = state.terminal_type {
        let ttype_lower = ttype.to_lowercase();
        if ttype_lower.contains("xterm")
            || ttype_lower.contains("kitty")
            || ttype_lower.contains("iterm")
            || ttype_lower.contains("putty")
            || ttype_lower.contains("vte")
            || ttype_lower.contains("gnome")
            || ttype_lower.contains("konsole")
        {
            return true;
        }
    }

    // Check client name for MUD clients with OSC support
    if let Some(ref client) = state.client_name {
        let client_lower = client.to_lowercase();
        if client_lower.contains("mudlet") || client_lower.contains("tintin") || client_lower.contains("blightmud") {
            return true;
        }
    }

    // VT100 flag suggests modern terminal with OSC support
    if (state.mtts_flags & MTTS_VT100) != 0 {
        return true;
    }

    false
}

/// Sanitize title string for safe OSC output
///
/// Removes control characters and escape sequences that could
/// break the OSC sequence. Limits length to 64 characters.
fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_control() && *c != '\x1b' && *c != '\x07')
        .take(64)
        .collect()
}

/// Build OSC sequence to set terminal title
///
/// Returns the escape sequence bytes to set the terminal window title,
/// or an empty vec if title updates are not supported by the terminal.
///
/// Uses OSC 0 (set both icon name and window title) with ST terminator.
pub fn build_title_sequence(state: &TelnetState, title: &str) -> Vec<u8> {
    if !supports_title_updates(state) {
        return Vec::new();
    }
    // OSC 0 ; title ST (where ST = ESC \)
    format!("\x1b]0;{}\x1b\\", sanitize_title(title)).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_data() {
        let mut parser = TelnetParser::new();
        let input = b"hello world";
        let (data, events) = parser.process_bytes(input);

        assert_eq!(data, b"hello world");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_escaped_iac() {
        let mut parser = TelnetParser::new();
        // IAC IAC should produce single 0xFF
        let input = &[b'a', IAC, IAC, b'b'];
        let (data, events) = parser.process_bytes(input);

        assert_eq!(data, vec![b'a', 0xFF, b'b']);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_will_echo() {
        let mut parser = TelnetParser::new();
        let input = &[IAC, WILL, OPT_ECHO];
        let (data, events) = parser.process_bytes(input);

        assert!(data.is_empty());
        assert_eq!(events, vec![TelnetEvent::Will(OPT_ECHO)]);
    }

    #[test]
    fn test_parse_mixed_data_and_commands() {
        let mut parser = TelnetParser::new();
        let input = &[b'h', b'i', IAC, DO, OPT_SGA, b'!'];
        let (data, events) = parser.process_bytes(input);

        assert_eq!(data, vec![b'h', b'i', b'!']);
        assert_eq!(events, vec![TelnetEvent::Do(OPT_SGA)]);
    }

    #[test]
    fn test_parse_subnegotiation() {
        let mut parser = TelnetParser::new();
        // NAWS subnegotiation: width=80, height=24
        let input = &[IAC, SB, OPT_NAWS, 0, 80, 0, 24, IAC, SE];
        let (data, events) = parser.process_bytes(input);

        assert!(data.is_empty());
        assert_eq!(events.len(), 1);
        if let TelnetEvent::Subnegotiation(opt, subneg_data) = &events[0] {
            assert_eq!(*opt, OPT_NAWS);
            assert_eq!(subneg_data, &[0, 80, 0, 24]);
        } else {
            panic!("Expected Subnegotiation event");
        }
    }

    #[test]
    fn test_parse_naws() {
        let data = &[0, 132, 0, 43]; // 132x43
        let result = parse_naws(data);
        assert_eq!(result, Some((132, 43)));
    }

    #[test]
    fn test_build_initial_negotiations() {
        let negs = build_initial_negotiations();
        // Should contain WILL ECHO, WILL SGA, DO SGA, DO NAWS
        assert!(negs.len() >= 12); // At least 4 commands * 3 bytes
    }

    // Tests for escape sequence parsing

    #[test]
    fn test_parse_arrow_up() {
        let state = EscapeState::Normal;

        let (new_state, event) = parse_key_byte(state, CHAR_ESC);
        assert!(matches!(new_state, EscapeState::GotEsc));
        assert!(event.is_none());

        let (new_state, event) = parse_key_byte(new_state, b'[');
        assert!(matches!(new_state, EscapeState::GotCsi(_)));
        assert!(event.is_none());

        let (new_state, event) = parse_key_byte(new_state, b'A');
        assert!(matches!(new_state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::ArrowUp));
    }

    #[test]
    fn test_parse_arrow_down() {
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (state, event) = parse_key_byte(state, b'B');
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::ArrowDown));
    }

    #[test]
    fn test_parse_arrow_left_right() {
        // Left arrow
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (_, event) = parse_key_byte(state, b'D');
        assert_eq!(event, Some(KeyEvent::ArrowLeft));

        // Right arrow
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (_, event) = parse_key_byte(state, b'C');
        assert_eq!(event, Some(KeyEvent::ArrowRight));
    }

    #[test]
    fn test_parse_home_end() {
        // Home (ESC [ H)
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (_, event) = parse_key_byte(state, b'H');
        assert_eq!(event, Some(KeyEvent::Home));

        // End (ESC [ F)
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (_, event) = parse_key_byte(state, b'F');
        assert_eq!(event, Some(KeyEvent::End));
    }

    #[test]
    fn test_parse_home_extended() {
        // ESC [ 1 ~
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (state, _) = parse_key_byte(state, b'1');
        let (state, event) = parse_key_byte(state, b'~');
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Home));
    }

    #[test]
    fn test_parse_delete_key() {
        // ESC [ 3 ~
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, CHAR_ESC);
        let (state, _) = parse_key_byte(state, b'[');
        let (state, _) = parse_key_byte(state, b'3');
        let (_, event) = parse_key_byte(state, b'~');
        assert_eq!(event, Some(KeyEvent::Delete));
    }

    #[test]
    fn test_parse_ctrl_keys() {
        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_A);
        assert_eq!(event, Some(KeyEvent::CtrlA));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_C);
        assert_eq!(event, Some(KeyEvent::CtrlC));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_D);
        assert_eq!(event, Some(KeyEvent::CtrlD));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_E);
        assert_eq!(event, Some(KeyEvent::CtrlE));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_K);
        assert_eq!(event, Some(KeyEvent::CtrlK));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_L);
        assert_eq!(event, Some(KeyEvent::CtrlL));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_U);
        assert_eq!(event, Some(KeyEvent::CtrlU));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_T);
        assert_eq!(event, Some(KeyEvent::CtrlT));

        let (_, event) = parse_key_byte(EscapeState::Normal, CTRL_W);
        assert_eq!(event, Some(KeyEvent::CtrlW));
    }

    #[test]
    fn test_parse_regular_chars() {
        let (_, event) = parse_key_byte(EscapeState::Normal, b'a');
        assert_eq!(event, Some(KeyEvent::Char('a')));

        let (_, event) = parse_key_byte(EscapeState::Normal, b'Z');
        assert_eq!(event, Some(KeyEvent::Char('Z')));

        let (_, event) = parse_key_byte(EscapeState::Normal, b'5');
        assert_eq!(event, Some(KeyEvent::Char('5')));
    }

    #[test]
    fn test_parse_special_keys() {
        let (_, event) = parse_key_byte(EscapeState::Normal, CHAR_TAB);
        assert_eq!(event, Some(KeyEvent::Tab));

        let (_, event) = parse_key_byte(EscapeState::Normal, CHAR_CR);
        assert_eq!(event, Some(KeyEvent::Enter));

        let (_, event) = parse_key_byte(EscapeState::Normal, CHAR_LF);
        assert_eq!(event, Some(KeyEvent::Enter));

        let (_, event) = parse_key_byte(EscapeState::Normal, CHAR_BACKSPACE);
        assert_eq!(event, Some(KeyEvent::Backspace));

        let (_, event) = parse_key_byte(EscapeState::Normal, CHAR_DEL);
        assert_eq!(event, Some(KeyEvent::Backspace));
    }

    #[test]
    fn test_ansi_cursor_left() {
        assert_eq!(ansi::cursor_left(5), b"\x1b[5D".to_vec());
        assert_eq!(ansi::cursor_left(1), b"\x1b[1D".to_vec());
        assert_eq!(ansi::cursor_left(0), Vec::<u8>::new());
    }

    #[test]
    fn test_ansi_cursor_right() {
        assert_eq!(ansi::cursor_right(3), b"\x1b[3C".to_vec());
        assert_eq!(ansi::cursor_right(1), b"\x1b[1C".to_vec());
        assert_eq!(ansi::cursor_right(0), Vec::<u8>::new());
    }

    #[test]
    fn test_ansi_clear() {
        assert_eq!(ansi::clear_to_eol(), b"\x1b[K".to_vec());
        assert_eq!(ansi::clear_line(), b"\x1b[2K\r".to_vec());
    }

    #[test]
    fn test_redraw_input_line() {
        // Cursor at end - no cursor left movement needed
        let output = redraw_input_line("> ", "hello", 5);
        assert!(output.starts_with(b"\r"));
        // Should contain prompt and buffer
        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("> "));
        assert!(output_str.contains("hello"));
        // Contains clear_to_eol but NOT cursor left sequence
        assert!(output_str.contains("\x1b[K")); // clear to eol
        assert!(!output_str.ends_with("D")); // no cursor left at end

        // Cursor in middle - should include cursor left sequence
        let output = redraw_input_line("> ", "hello", 2);
        let output_str = String::from_utf8_lossy(&output);
        // Should move cursor left 3 positions (5-2)
        assert!(
            output_str.ends_with("\x1b[3D"),
            "Expected cursor left at end: {:?}",
            output_str
        );

        // Emoji test - cursor before emoji should account for 2-column width
        // "hi😀" = 4 chars but "😀" is 2 columns, so cursor at pos 2 needs to move 2 cols
        let output = redraw_input_line("> ", "hi😀", 2);
        let output_str = String::from_utf8_lossy(&output);
        // Emoji takes 2 display columns, so move cursor left 2
        assert!(
            output_str.ends_with("\x1b[2D"),
            "Expected 2 cols for emoji: {:?}",
            output_str
        );
    }

    #[test]
    fn test_display_width() {
        // ASCII
        assert_eq!(display_width("hello"), 5);
        // Emoji (2 columns each)
        assert_eq!(display_width("😀"), 2);
        assert_eq!(display_width("hi😀"), 4); // 2 + 2
        // CJK characters (2 columns each)
        assert_eq!(display_width("日本語"), 6); // 3 chars * 2 cols
        // Mixed
        assert_eq!(display_width("hi日本"), 6); // 2 + 2 + 2
    }

    #[test]
    fn test_display_width_up_to() {
        // First 2 chars of "hi😀" = "hi" = 2 columns
        assert_eq!(display_width_up_to("hi😀", 2), 2);
        // First 3 chars = "hi😀" = 4 columns (2 + 2)
        assert_eq!(display_width_up_to("hi😀", 3), 4);
        // CJK
        assert_eq!(display_width_up_to("日本語", 2), 4); // 2 * 2
    }

    // UTF-8 multi-byte sequence tests

    #[test]
    fn test_parse_utf8_2byte() {
        // é = U+00E9 = C3 A9 in UTF-8
        let state = EscapeState::Normal;
        let (state, event) = parse_key_byte(state, 0xC3);
        assert!(matches!(state, EscapeState::Utf8(_, 2)));
        assert!(event.is_none());

        let (state, event) = parse_key_byte(state, 0xA9);
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Char('é')));
    }

    #[test]
    fn test_parse_utf8_3byte_japanese() {
        // あ = U+3042 = E3 81 82 in UTF-8
        let state = EscapeState::Normal;
        let (state, event) = parse_key_byte(state, 0xE3);
        assert!(matches!(state, EscapeState::Utf8(_, 3)));
        assert!(event.is_none());

        let (state, event) = parse_key_byte(state, 0x81);
        assert!(matches!(state, EscapeState::Utf8(_, 3)));
        assert!(event.is_none());

        let (state, event) = parse_key_byte(state, 0x82);
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Char('あ')));
    }

    #[test]
    fn test_parse_utf8_4byte_emoji() {
        // 😀 = U+1F600 = F0 9F 98 80 in UTF-8
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, 0xF0);
        let (state, _) = parse_key_byte(state, 0x9F);
        let (state, _) = parse_key_byte(state, 0x98);
        let (state, event) = parse_key_byte(state, 0x80);
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Char('😀')));
    }

    #[test]
    fn test_parse_utf8_invalid_continuation() {
        // Start 2-byte sequence but receive non-continuation byte
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, 0xC3); // Start 2-byte
        let (state, event) = parse_key_byte(state, b'a'); // Invalid continuation
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Unknown));
    }

    #[test]
    fn test_parse_utf8_invalid_lead() {
        // 0xFF is never a valid UTF-8 byte
        let (state, event) = parse_key_byte(EscapeState::Normal, 0xFF);
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Unknown));
    }

    #[test]
    fn test_parse_utf8_katakana() {
        // カ = U+30AB = E3 82 AB in UTF-8
        let state = EscapeState::Normal;
        let (state, _) = parse_key_byte(state, 0xE3);
        let (state, _) = parse_key_byte(state, 0x82);
        let (state, event) = parse_key_byte(state, 0xAB);
        assert!(matches!(state, EscapeState::Normal));
        assert_eq!(event, Some(KeyEvent::Char('カ')));
    }

    // MTTS/TTYPE tests

    #[test]
    fn test_build_ttype_send() {
        let msg = build_ttype_send();
        assert_eq!(msg, vec![IAC, SB, OPT_TTYPE, TTYPE_SEND, IAC, SE]);
    }

    #[test]
    fn test_parse_ttype_is() {
        // Valid TTYPE IS response
        let data = vec![TTYPE_IS, b'M', b'U', b'D', b'L', b'E', b'T'];
        assert_eq!(parse_ttype_is(&data), Some("MUDLET".to_string()));

        // XTERM-256COLOR
        let data = vec![
            TTYPE_IS, b'X', b'T', b'E', b'R', b'M', b'-', b'2', b'5', b'6', b'C', b'O', b'L', b'O', b'R',
        ];
        assert_eq!(parse_ttype_is(&data), Some("XTERM-256COLOR".to_string()));

        // Invalid - wrong prefix byte
        let data = vec![TTYPE_SEND, b'T', b'E', b'S', b'T'];
        assert_eq!(parse_ttype_is(&data), None);

        // Invalid - empty
        let data = vec![];
        assert_eq!(parse_ttype_is(&data), None);

        // Invalid - only IS byte
        let data = vec![TTYPE_IS];
        assert_eq!(parse_ttype_is(&data), None);
    }

    #[test]
    fn test_parse_mtts_flags() {
        // Valid MTTS strings
        assert_eq!(parse_mtts_flags("MTTS 137"), Some(137));
        assert_eq!(parse_mtts_flags("MTTS 4"), Some(4));
        assert_eq!(parse_mtts_flags("MTTS 0"), Some(0));
        assert_eq!(parse_mtts_flags("MTTS 255"), Some(255));

        // Invalid - not MTTS format
        assert_eq!(parse_mtts_flags("XTERM"), None);
        assert_eq!(parse_mtts_flags("MUDLET"), None);
        assert_eq!(parse_mtts_flags(""), None);

        // Invalid - non-numeric value
        assert_eq!(parse_mtts_flags("MTTS abc"), None);
    }

    #[test]
    fn test_mtts_utf8_flag() {
        // No UTF-8 support (PROXY + 256_COLORS + ANSI = 137)
        let flags = 137u32;
        assert!((flags & MTTS_UTF8) == 0);
        assert!((flags & MTTS_ANSI) != 0);
        assert!((flags & MTTS_256_COLORS) != 0);
        assert!((flags & MTTS_PROXY) != 0);

        // With UTF-8 support (PROXY + 256_COLORS + UTF8 + ANSI = 141)
        let flags = 141u32;
        assert!((flags & MTTS_UTF8) != 0);
        assert!((flags & MTTS_ANSI) != 0);
        assert!((flags & MTTS_256_COLORS) != 0);
        assert!((flags & MTTS_PROXY) != 0);

        // Just UTF-8
        let flags = MTTS_UTF8;
        assert!((flags & MTTS_UTF8) != 0);
        assert!((flags & MTTS_ANSI) == 0);
    }
}
