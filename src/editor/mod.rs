//! Modern multi-line editor (beta opt-in).
//!
//! Phase 3: power tools. Undo/redo with coalesced batches, kill ring
//! (Ctrl-K cut / Ctrl-U paste), incremental search (Ctrl-W), replace-all
//! (Ctrl-\), goto-line (Ctrl-_), and a help overlay (Ctrl-G). Phase 4 will
//! add DG-script syntax highlighting, tab completion, and live syntax
//! checking on top of this foundation.
//!
//! The editor is opt-in via `set new_editor on` (`CharacterData.new_editor_enabled`).

pub mod dg;

use crate::telnet::KeyEvent;

const MAX_UNDO: usize = 128;

/// Which legacy `collecting_*` OLC mode is this editor session standing in for?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    RoomDesc,
    ItemLong,
    ItemNote,
    BoardPost,
    DgTriggerBody,
    DialogueNodeText,
    ExtraDesc,
    Motd,
}

impl EditorKind {
    pub fn from_olc_mode(mode: &str) -> Option<Self> {
        match mode {
            "collecting_desc" => Some(Self::RoomDesc),
            "collecting_long" => Some(Self::ItemLong),
            "collecting_note" => Some(Self::ItemNote),
            "collecting_board_post" => Some(Self::BoardPost),
            "collecting_dg_body" => Some(Self::DgTriggerBody),
            // Proto bodies share the DG editor surface (same syntax, same
            // colouring). The save dispatcher distinguishes the two modes by
            // the session's `olc_mode` string, routing instance saves vs the
            // proto save-through + sibling refresh.
            "collecting_dg_proto_body" => Some(Self::DgTriggerBody),
            "collecting_dialogue_node_text" => Some(Self::DialogueNodeText),
            "collecting_extra_desc" => Some(Self::ExtraDesc),
            "collecting_motd" => Some(Self::Motd),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::RoomDesc => "Room description",
            Self::ItemLong => "Item long description",
            Self::ItemNote => "Item note",
            Self::BoardPost => "Board post",
            Self::DgTriggerBody => "DG trigger body",
            Self::DialogueNodeText => "Dialogue node",
            Self::ExtraDesc => "Extra description",
            Self::Motd => "MOTD",
        }
    }

    pub fn max_bytes(&self) -> usize {
        match self {
            Self::RoomDesc => crate::api::validate::DESCRIPTION_MAX,
            _ => 32 * 1024,
        }
    }
}

/// What should the caller do after `handle_key` processes a key?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Save,
    Cancel,
}

/// Modal state. `Editing` is the default; the others overlay the footer
/// (or, for `Help`, the whole screen) with a transient prompt or overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorMode {
    Editing,
    /// Ctrl-X with dirty buffer — Y=save, N=discard, ^C=keep editing.
    ConfirmExitSaveOrDiscard,
    /// Ctrl-C with dirty buffer — Y=discard, N/^C=keep editing.
    ConfirmDiscard,
    /// Ctrl-G help screen — any key dismisses.
    Help,
    /// Ctrl-W incremental search prompt.
    Search { query: String },
    /// Ctrl-\ replace, stage 1 — collect the search term.
    ReplaceQuery { query: String },
    /// Ctrl-\ replace, stage 2 — collect the replacement.
    ReplaceWith { query: String, replacement: String },
    /// Ctrl-_ goto line prompt.
    GotoLine { input: String },
}

/// Tags consecutive edits so undo coalesces the "type a sentence then undo"
/// path into one snapshot instead of one per keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchKind {
    Insert,
    Delete,
    LineSplit,
    LineJoin,
    Paste,
}

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

/// In-memory editor state. Held on `PlayerSession.modern_editor` while
/// the player is composing text. Cleared on Save / Cancel.
#[derive(Debug, Clone)]
pub struct EditorSession {
    pub kind: EditorKind,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub width: u16,
    pub height: u16,
    pub dirty: bool,
    pub status: String,

    mode: EditorMode,
    pending_output: Vec<u8>,

    // Undo / redo state.
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    open_batch: Option<BatchKind>,

    // Kill ring (cut buffer). Each cut writes one or more lines; consecutive
    // Ctrl-K presses append, so a multi-line cut survives a single paste.
    kill_ring: Vec<String>,
    last_was_kill: bool,

    /// Emit ANSI colour codes for DG syntax highlighting. Set true when
    /// the player has `colors_enabled` and the editor is hosting a DG
    /// trigger body. Other kinds always render monochrome.
    pub colour_enabled: bool,

    /// DG tab-completion cycle state. Holds the prefix of the in-flight
    /// cycle (the text up to the cursor when the user first hit Tab),
    /// the candidate list, and the next index to return. Reset to None
    /// whenever any non-Tab key arrives.
    dg_tab_state: Option<DgTabState>,

    /// Cached first-error from the live DG syntax check, if any. Updated
    /// after every mutating edit on DG bodies; surfaces in the footer
    /// and highlights the errored line in the body.
    dg_error: Option<(usize, String)>,

    /// Selection anchor — the "other end" of the selection while the
    /// cursor is the active end. None when no selection is active.
    /// Cleared by any non-shift navigation; set on the first Shift+Move
    /// from the prior cursor position.
    selection_anchor: Option<(usize, usize)>,

    /// X10 mouse tracking state. True (the default) means left-clicks
    /// reposition the cursor; false releases the terminal's mouse so
    /// right-click paste from another window works again. Toggled via
    /// Ctrl-T.
    mouse_enabled: bool,
}

#[derive(Debug, Clone)]
struct DgTabState {
    cursor_row: usize,
    cursor_col: usize,
    word_start_col: usize,     // char index of the word we're replacing
    candidates: Vec<String>,
    next_idx: usize,
}

impl EditorSession {
    pub fn new(kind: EditorKind, seed: &str, width: u16, height: u16) -> Self {
        let lines: Vec<String> = if seed.is_empty() {
            vec![String::new()]
        } else {
            seed.split('\n').map(|s| s.to_string()).collect()
        };
        let (w, h) = (width.max(40), height.max(6));
        let mut s = Self {
            kind,
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            width: w,
            height: h,
            dirty: false,
            status: String::from(
                "[BETA] ^O save  ^X exit  ^G help  ^T mouse  ^W search  ^\\ replace  ^_ goto  ^Z undo",
            ),
            mode: EditorMode::Editing,
            pending_output: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            open_batch: None,
            kill_ring: Vec::new(),
            last_was_kill: false,
            colour_enabled: false,
            dg_tab_state: None,
            dg_error: None,
            selection_anchor: None,
            mouse_enabled: true,
        };
        s.refresh_dg_error();
        s.render();
        s
    }

    /// Enable ANSI colour output. Called by the entry helper when the
    /// player has `colors_enabled` and the editor is hosting a DG body.
    /// Re-renders if the value changed so the first-frame draw (done in
    /// `new()` before the caller can flip this) picks up syntax colour
    /// without forcing the player to hit ^L.
    pub fn set_colour_enabled(&mut self, on: bool) {
        if self.colour_enabled == on {
            return;
        }
        self.colour_enabled = on;
        self.render();
    }

    /// Is this an editor session that should activate DG-specific
    /// highlighting, tab completion, and live syntax checks?
    fn dg_mode(&self) -> bool {
        self.kind == EditorKind::DgTriggerBody
    }

    /// Re-parse the buffer if we're in DG mode and cache the first error.
    fn refresh_dg_error(&mut self) {
        if !self.dg_mode() {
            self.dg_error = None;
            return;
        }
        self.dg_error = dg::syntax_check(&self.take_text());
    }

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width.max(40);
        self.height = height.max(6);
    }

    pub fn take_text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending_output)
    }

    pub fn flash_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.render();
    }

    pub fn mark_saved(&mut self, byte_count: usize) {
        self.dirty = false;
        self.status = format!(
            "[BETA] Saved {} byte{}. ^O save  ^X exit.",
            byte_count,
            if byte_count == 1 { "" } else { "s" }
        );
        self.render();
    }

    /// Flip X10 mouse tracking. When on (default), left-clicks reposition
    /// the cursor. When off, the terminal owns the mouse again so the
    /// player can right-click paste from another window into the editor.
    /// The DECSET/DECRST byte is prepended to `pending_output` so the
    /// next `take_output()` drain (the dispatcher calls it after every
    /// keystroke) flips the terminal mode along with the redraw.
    fn toggle_mouse_tracking(&mut self) {
        self.mouse_enabled = !self.mouse_enabled;
        self.status = if self.mouse_enabled {
            String::from("Mouse tracking ON — left-click positions the cursor.")
        } else {
            String::from("Mouse tracking OFF — right-click paste available. ^T to re-enable.")
        };
        self.render();
        let escape: &[u8] = if self.mouse_enabled {
            b"\x1b[?1000h"
        } else {
            b"\x1b[?1000l"
        };
        let mut prefixed = escape.to_vec();
        prefixed.extend_from_slice(&self.pending_output);
        self.pending_output = prefixed;
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> EditorAction {
        // Track whether this key was a kill (Ctrl-K), so consecutive kills
        // accumulate. Anything else resets the run.
        let was_kill = matches!(key, KeyEvent::CtrlK)
            && matches!(self.mode, EditorMode::Editing);

        let action = match self.mode.clone() {
            EditorMode::ConfirmExitSaveOrDiscard => self.key_confirm_save_or_discard(key),
            EditorMode::ConfirmDiscard => self.key_confirm_discard(key),
            EditorMode::Help => self.key_help_overlay(key),
            EditorMode::Search { query } => self.key_search(key, query),
            EditorMode::ReplaceQuery { query } => self.key_replace_query(key, query),
            EditorMode::ReplaceWith { query, replacement } => {
                self.key_replace_with(key, query, replacement)
            }
            EditorMode::GotoLine { input } => self.key_goto_line(key, input),
            EditorMode::Editing => self.key_editing(key),
        };

        self.last_was_kill = was_kill;
        action
    }

    // ============================================================
    // Editing-mode key handler
    // ============================================================

    fn key_editing(&mut self, key: &KeyEvent) -> EditorAction {
        // Tab-completion cycle state survives only across consecutive Tabs.
        if !matches!(key, KeyEvent::Tab) {
            self.dg_tab_state = None;
        }
        match key {
            KeyEvent::CtrlX => {
                if self.dirty {
                    self.mode = EditorMode::ConfirmExitSaveOrDiscard;
                    self.status = String::from("Save modified buffer? (Y)es  (N)o  ^C cancel");
                    self.render();
                    EditorAction::None
                } else {
                    EditorAction::Cancel
                }
            }
            KeyEvent::CtrlC => {
                if self.dirty {
                    self.mode = EditorMode::ConfirmDiscard;
                    self.status = String::from("Discard unsaved changes? (Y)es  (N)o");
                    self.render();
                    EditorAction::None
                } else {
                    EditorAction::Cancel
                }
            }
            KeyEvent::CtrlO => {
                if self.take_text().len() > self.kind.max_bytes() {
                    self.flash_status(format!(
                        "Buffer too large: {} bytes, max {}. Trim before saving.",
                        self.take_text().len(),
                        self.kind.max_bytes()
                    ));
                    EditorAction::None
                } else {
                    EditorAction::Save
                }
            }
            KeyEvent::CtrlG => {
                self.mode = EditorMode::Help;
                self.render();
                EditorAction::None
            }
            KeyEvent::CtrlL => {
                self.render();
                EditorAction::None
            }
            KeyEvent::CtrlZ => {
                self.undo();
                EditorAction::None
            }
            KeyEvent::CtrlY => {
                self.redo();
                EditorAction::None
            }
            KeyEvent::CtrlK => {
                if self.selection_anchor.is_some() {
                    self.cut_selection();
                } else {
                    self.cut_line();
                }
                EditorAction::None
            }
            KeyEvent::CtrlU => {
                self.paste_kill_ring();
                EditorAction::None
            }
            KeyEvent::CtrlW => {
                self.mode = EditorMode::Search { query: String::new() };
                self.status = String::from("Search: (type query, Enter jumps, ^C cancel)");
                self.close_batch();
                self.render();
                EditorAction::None
            }
            KeyEvent::CtrlBackslash => {
                self.mode = EditorMode::ReplaceQuery { query: String::new() };
                self.status = String::from("Replace — find: (Enter to continue, ^C cancel)");
                self.close_batch();
                self.render();
                EditorAction::None
            }
            KeyEvent::CtrlUnderscore => {
                self.mode = EditorMode::GotoLine { input: String::new() };
                self.status = String::from("Go to line #: (Enter to jump, ^C cancel)");
                self.close_batch();
                self.render();
                EditorAction::None
            }
            KeyEvent::ArrowLeft => {
                self.close_batch();
                self.selection_anchor = None;
                self.move_left();
                self.render();
                EditorAction::None
            }
            KeyEvent::ArrowRight => {
                self.close_batch();
                self.selection_anchor = None;
                self.move_right();
                self.render();
                EditorAction::None
            }
            KeyEvent::ArrowUp => {
                self.close_batch();
                self.selection_anchor = None;
                self.move_up();
                self.render();
                EditorAction::None
            }
            KeyEvent::ArrowDown => {
                self.close_batch();
                self.selection_anchor = None;
                self.move_down();
                self.render();
                EditorAction::None
            }
            KeyEvent::Home | KeyEvent::CtrlA => {
                self.close_batch();
                self.selection_anchor = None;
                self.cursor_col = 0;
                self.render();
                EditorAction::None
            }
            KeyEvent::End | KeyEvent::CtrlE => {
                self.close_batch();
                self.selection_anchor = None;
                self.cursor_col = self.current_line_chars();
                self.render();
                EditorAction::None
            }
            KeyEvent::PageUp => {
                self.close_batch();
                self.selection_anchor = None;
                self.page_up();
                self.render();
                EditorAction::None
            }
            KeyEvent::PageDown => {
                self.close_batch();
                self.selection_anchor = None;
                self.page_down();
                self.render();
                EditorAction::None
            }
            KeyEvent::ShiftArrowLeft => {
                self.extend_selection(|s| s.move_left());
                EditorAction::None
            }
            KeyEvent::ShiftArrowRight => {
                self.extend_selection(|s| s.move_right());
                EditorAction::None
            }
            KeyEvent::ShiftArrowUp => {
                self.extend_selection(|s| s.move_up());
                EditorAction::None
            }
            KeyEvent::ShiftArrowDown => {
                self.extend_selection(|s| s.move_down());
                EditorAction::None
            }
            KeyEvent::ShiftHome => {
                self.extend_selection(|s| s.cursor_col = 0);
                EditorAction::None
            }
            KeyEvent::ShiftEnd => {
                self.extend_selection(|s| s.cursor_col = s.current_line_chars());
                EditorAction::None
            }
            KeyEvent::Enter => {
                self.consume_selection_if_any();
                self.push_undo(BatchKind::LineSplit);
                let auto_indent = if self.dg_mode() {
                    dg_auto_indent_for(&self.lines[self.cursor_row], self.cursor_col)
                } else {
                    String::new()
                };
                self.insert_newline();
                for c in auto_indent.chars() {
                    self.insert_char(c);
                }
                self.close_batch();
                self.render();
                EditorAction::None
            }
            KeyEvent::Backspace => {
                if self.consume_selection_if_any() {
                    self.render();
                } else {
                    self.backspace_with_undo();
                    self.render();
                }
                EditorAction::None
            }
            KeyEvent::Delete => {
                if self.consume_selection_if_any() {
                    self.render();
                } else {
                    self.delete_forward_with_undo();
                    self.render();
                }
                EditorAction::None
            }
            KeyEvent::Tab => {
                if self.dg_mode() && self.selection_anchor.is_none() {
                    self.dg_tab_cycle();
                } else {
                    self.consume_selection_if_any();
                    self.push_undo(BatchKind::Insert);
                    self.insert_char('\t');
                }
                self.render();
                EditorAction::None
            }
            KeyEvent::Char(c) => {
                if !c.is_control() {
                    self.consume_selection_if_any();
                    self.push_undo(BatchKind::Insert);
                    self.insert_char(*c);
                    self.render();
                }
                EditorAction::None
            }
            KeyEvent::MouseClick { row, col } => {
                self.close_batch();
                self.selection_anchor = None;
                if let Some((logical_row, logical_col)) =
                    self.click_to_logical(*row, *col)
                {
                    self.cursor_row = logical_row;
                    self.cursor_col = logical_col;
                    self.render();
                }
                EditorAction::None
            }
            KeyEvent::CtrlT => {
                self.toggle_mouse_tracking();
                EditorAction::None
            }
            KeyEvent::CtrlD | KeyEvent::Unknown => EditorAction::None,
        }
    }

    // ============================================================
    // Modal key handlers
    // ============================================================

    fn key_confirm_save_or_discard(&mut self, key: &KeyEvent) -> EditorAction {
        match key {
            KeyEvent::Char('y') | KeyEvent::Char('Y') => {
                self.mode = EditorMode::Editing;
                if self.take_text().len() > self.kind.max_bytes() {
                    self.flash_status(format!(
                        "Buffer too large: {} bytes, max {}. Trim before saving.",
                        self.take_text().len(),
                        self.kind.max_bytes()
                    ));
                    EditorAction::None
                } else {
                    EditorAction::Save
                }
            }
            KeyEvent::Char('n') | KeyEvent::Char('N') => {
                self.mode = EditorMode::Editing;
                EditorAction::Cancel
            }
            KeyEvent::CtrlC | KeyEvent::Char('c') | KeyEvent::Char('C') => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Cancelled — back to editing.");
                self.render();
                EditorAction::None
            }
            _ => {
                self.render();
                EditorAction::None
            }
        }
    }

    fn key_confirm_discard(&mut self, key: &KeyEvent) -> EditorAction {
        match key {
            KeyEvent::Char('y') | KeyEvent::Char('Y') => {
                self.mode = EditorMode::Editing;
                EditorAction::Cancel
            }
            KeyEvent::Char('n')
            | KeyEvent::Char('N')
            | KeyEvent::CtrlC => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Continuing — your changes are still here.");
                self.render();
                EditorAction::None
            }
            _ => {
                self.render();
                EditorAction::None
            }
        }
    }

    fn key_help_overlay(&mut self, _key: &KeyEvent) -> EditorAction {
        // Any key dismisses the overlay.
        self.mode = EditorMode::Editing;
        self.render();
        EditorAction::None
    }

    fn key_search(&mut self, key: &KeyEvent, mut query: String) -> EditorAction {
        match key {
            KeyEvent::CtrlC => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Search cancelled.");
                self.render();
                EditorAction::None
            }
            KeyEvent::Enter => {
                if query.is_empty() {
                    self.mode = EditorMode::Editing;
                    self.status = String::from("Empty query — search cancelled.");
                    self.render();
                    return EditorAction::None;
                }
                let found = self.find_next(&query);
                if found {
                    self.status = format!("Found '{}'. ^W again to repeat.", query);
                } else {
                    self.status = format!("'{}' not found.", query);
                }
                self.mode = EditorMode::Editing;
                self.render();
                EditorAction::None
            }
            KeyEvent::Backspace => {
                query.pop();
                self.mode = EditorMode::Search { query };
                self.render();
                EditorAction::None
            }
            KeyEvent::Char(c) if !c.is_control() => {
                query.push(*c);
                self.mode = EditorMode::Search { query };
                self.render();
                EditorAction::None
            }
            _ => {
                // Preserve modal state on incidental keys.
                self.mode = EditorMode::Search { query };
                self.render();
                EditorAction::None
            }
        }
    }

    fn key_replace_query(&mut self, key: &KeyEvent, mut query: String) -> EditorAction {
        match key {
            KeyEvent::CtrlC => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Replace cancelled.");
                self.render();
                EditorAction::None
            }
            KeyEvent::Enter => {
                if query.is_empty() {
                    self.mode = EditorMode::Editing;
                    self.status = String::from("Empty query — replace cancelled.");
                    self.render();
                    return EditorAction::None;
                }
                self.mode = EditorMode::ReplaceWith {
                    query,
                    replacement: String::new(),
                };
                self.status = String::from("Replace with: (Enter to apply, ^C cancel)");
                self.render();
                EditorAction::None
            }
            KeyEvent::Backspace => {
                query.pop();
                self.mode = EditorMode::ReplaceQuery { query };
                self.render();
                EditorAction::None
            }
            KeyEvent::Char(c) if !c.is_control() => {
                query.push(*c);
                self.mode = EditorMode::ReplaceQuery { query };
                self.render();
                EditorAction::None
            }
            _ => {
                self.mode = EditorMode::ReplaceQuery { query };
                self.render();
                EditorAction::None
            }
        }
    }

    fn key_replace_with(
        &mut self,
        key: &KeyEvent,
        query: String,
        mut replacement: String,
    ) -> EditorAction {
        match key {
            KeyEvent::CtrlC => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Replace cancelled.");
                self.render();
                EditorAction::None
            }
            KeyEvent::Enter => {
                let count = self.replace_all(&query, &replacement);
                self.mode = EditorMode::Editing;
                self.status = if count == 0 {
                    format!("'{}' not found — nothing changed.", query)
                } else {
                    format!(
                        "Replaced {} occurrence{} of '{}'.",
                        count,
                        if count == 1 { "" } else { "s" },
                        query
                    )
                };
                self.render();
                EditorAction::None
            }
            KeyEvent::Backspace => {
                replacement.pop();
                self.mode = EditorMode::ReplaceWith { query, replacement };
                self.render();
                EditorAction::None
            }
            KeyEvent::Char(c) if !c.is_control() => {
                replacement.push(*c);
                self.mode = EditorMode::ReplaceWith { query, replacement };
                self.render();
                EditorAction::None
            }
            _ => {
                self.mode = EditorMode::ReplaceWith { query, replacement };
                self.render();
                EditorAction::None
            }
        }
    }

    fn key_goto_line(&mut self, key: &KeyEvent, mut input: String) -> EditorAction {
        match key {
            KeyEvent::CtrlC => {
                self.mode = EditorMode::Editing;
                self.status = String::from("Goto cancelled.");
                self.render();
                EditorAction::None
            }
            KeyEvent::Enter => {
                match input.trim().parse::<usize>() {
                    Ok(n) if n >= 1 && n <= self.lines.len() => {
                        self.cursor_row = n - 1;
                        self.cursor_col = self.cursor_col.min(self.current_line_chars());
                        self.status = format!("At line {}.", n);
                    }
                    Ok(n) => {
                        self.status = format!(
                            "Line {} out of range (buffer has {} line{}).",
                            n,
                            self.lines.len(),
                            if self.lines.len() == 1 { "" } else { "s" }
                        );
                    }
                    Err(_) => {
                        self.status = format!("'{}' is not a valid line number.", input);
                    }
                }
                self.mode = EditorMode::Editing;
                self.render();
                EditorAction::None
            }
            KeyEvent::Backspace => {
                input.pop();
                self.mode = EditorMode::GotoLine { input };
                self.render();
                EditorAction::None
            }
            KeyEvent::Char(c) if c.is_ascii_digit() => {
                input.push(*c);
                self.mode = EditorMode::GotoLine { input };
                self.render();
                EditorAction::None
            }
            _ => {
                self.mode = EditorMode::GotoLine { input };
                self.render();
                EditorAction::None
            }
        }
    }

    // ============================================================
    // Cursor primitives
    // ============================================================

    fn current_line_chars(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|s| s.chars().count())
            .unwrap_or(0)
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_chars();
        }
    }

    fn move_right(&mut self) {
        let line_len = self.current_line_chars();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.cursor_row == 0 {
            self.cursor_col = 0;
            return;
        }
        self.cursor_row -= 1;
        self.cursor_col = self.cursor_col.min(self.current_line_chars());
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 >= self.lines.len() {
            self.cursor_col = self.current_line_chars();
            return;
        }
        self.cursor_row += 1;
        self.cursor_col = self.cursor_col.min(self.current_line_chars());
    }

    fn page_up(&mut self) {
        let step = self.body_rows().max(1);
        self.cursor_row = self.cursor_row.saturating_sub(step);
        self.cursor_col = self.cursor_col.min(self.current_line_chars());
    }

    fn page_down(&mut self) {
        let step = self.body_rows().max(1);
        let last = self.lines.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + step).min(last);
        self.cursor_col = self.cursor_col.min(self.current_line_chars());
    }

    // ============================================================
    // Editing primitives
    // ============================================================

    fn insert_char(&mut self, c: char) {
        let line = match self.lines.get_mut(self.cursor_row) {
            Some(l) => l,
            None => {
                self.lines.push(String::new());
                self.lines.last_mut().unwrap()
            }
        };
        let byte_idx = char_index_to_byte(line, self.cursor_col);
        line.insert(byte_idx, c);
        self.cursor_col += 1;
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        let line = self
            .lines
            .get_mut(self.cursor_row)
            .expect("cursor_row in bounds");
        let byte_idx = char_index_to_byte(line, self.cursor_col);
        let tail = line.split_off(byte_idx);
        self.lines.insert(self.cursor_row + 1, tail);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.dirty = true;
    }

    fn backspace_with_undo(&mut self) {
        if self.cursor_col > 0 {
            self.push_undo(BatchKind::Delete);
            let col = self.cursor_col - 1;
            let line = self
                .lines
                .get_mut(self.cursor_row)
                .expect("cursor_row in bounds");
            let byte_idx = char_index_to_byte(line, col);
            line.remove(byte_idx);
            self.cursor_col = col;
            self.dirty = true;
        } else if self.cursor_row > 0 {
            self.push_undo(BatchKind::LineJoin);
            self.close_batch();
            let removed = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len = self.current_line_chars();
            self.lines
                .get_mut(self.cursor_row)
                .expect("cursor_row in bounds")
                .push_str(&removed);
            self.cursor_col = prev_len;
            self.dirty = true;
        }
    }

    fn delete_forward_with_undo(&mut self) {
        let line_len = self.current_line_chars();
        if self.cursor_col < line_len {
            self.push_undo(BatchKind::Delete);
            let line = self
                .lines
                .get_mut(self.cursor_row)
                .expect("cursor_row in bounds");
            let byte_idx = char_index_to_byte(line, self.cursor_col);
            line.remove(byte_idx);
            self.dirty = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.push_undo(BatchKind::LineJoin);
            self.close_batch();
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines
                .get_mut(self.cursor_row)
                .expect("cursor_row in bounds")
                .push_str(&next);
            self.dirty = true;
        }
    }

    // ============================================================
    // Selection
    // ============================================================

    /// Anchor the selection (if not already) at the current cursor and
    /// then run a cursor-moving closure. The cursor is the active end;
    /// the anchor is the other end.
    fn extend_selection<F>(&mut self, mut mv: F)
    where
        F: FnMut(&mut Self),
    {
        self.close_batch();
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_row, self.cursor_col));
        }
        mv(self);
        self.render();
    }

    /// Normalised (start, end) selection range, lexicographically ordered
    /// on (row, col). Returns None if there is no selection or if it
    /// collapses (anchor == cursor).
    fn normalized_selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let anchor = self.selection_anchor?;
        let cur = (self.cursor_row, self.cursor_col);
        if anchor == cur {
            return None;
        }
        let (start, end) = if anchor <= cur { (anchor, cur) } else { (cur, anchor) };
        Some((start, end))
    }

    /// Delete the active selection. Returns true if something was
    /// deleted. Used by char-insert / Backspace / Delete to clear the
    /// selection first.
    fn consume_selection_if_any(&mut self) -> bool {
        let Some((start, end)) = self.normalized_selection() else {
            self.selection_anchor = None;
            return false;
        };
        self.push_undo(BatchKind::Delete);
        self.close_batch();

        let pre: String = self.lines[start.0].chars().take(start.1).collect();
        let post: String = self.lines[end.0].chars().skip(end.1).collect();
        let merged = format!("{pre}{post}");

        // Replace lines[start.0..=end.0] with the single merged line.
        self.lines.splice(start.0..=end.0, std::iter::once(merged));
        self.cursor_row = start.0;
        self.cursor_col = start.1;
        self.selection_anchor = None;
        self.dirty = true;
        true
    }

    /// Capture the selected text and remove it, populating the kill
    /// ring so Ctrl-U pastes it back. Used by Ctrl-K when a selection
    /// is active.
    fn cut_selection(&mut self) {
        let Some((start, end)) = self.normalized_selection() else {
            return;
        };
        let cut_text = self.text_in_range(start, end);
        self.kill_ring.clear();
        self.kill_ring.push(cut_text);
        // consume_selection_if_any performs the actual removal + undo
        // bookkeeping.
        self.consume_selection_if_any();
        self.status = String::from("Cut selection into kill ring. ^U to paste.");
        self.render();
    }

    fn text_in_range(&self, start: (usize, usize), end: (usize, usize)) -> String {
        if start.0 == end.0 {
            return self.lines[start.0]
                .chars()
                .skip(start.1)
                .take(end.1 - start.1)
                .collect();
        }
        let mut out = String::new();
        out.extend(self.lines[start.0].chars().skip(start.1));
        out.push('\n');
        for r in (start.0 + 1)..end.0 {
            out.push_str(&self.lines[r]);
            out.push('\n');
        }
        out.extend(self.lines[end.0].chars().take(end.1));
        out
    }

    // ============================================================
    // DG tab completion
    // ============================================================

    /// Cycle through DG completion candidates at the cursor.
    fn dg_tab_cycle(&mut self) {
        // Resume an in-flight cycle (consecutive Tab presses).
        if let Some(state) = self.dg_tab_state.take() {
            self.apply_dg_completion(state);
            return;
        }

        // Start a fresh cycle. Build the prefix-before-cursor as chars.
        let line = self.lines.get(self.cursor_row).cloned().unwrap_or_default();
        let cursor_byte = char_index_to_byte(&line, self.cursor_col);
        let prefix = line[..cursor_byte].to_string();

        let candidates = dg::completions_for(&prefix);
        if candidates.is_empty() {
            self.status = String::from("No completions available here.");
            return;
        }

        // Determine which slice of the prefix we'll replace.
        let word_start_col = compute_word_start(&prefix);

        let _ = prefix; // computed for word_start_col; not retained
        let state = DgTabState {
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            word_start_col,
            candidates,
            next_idx: 0,
        };
        self.apply_dg_completion(state);
    }

    fn apply_dg_completion(&mut self, mut state: DgTabState) {
        if state.candidates.is_empty() {
            return;
        }
        if state.next_idx >= state.candidates.len() {
            state.next_idx = 0;
        }
        let replacement = state.candidates[state.next_idx].clone();
        let cycle_pos = state.next_idx + 1;
        let total = state.candidates.len();
        state.next_idx += 1;

        self.push_undo(BatchKind::Paste);
        self.close_batch();

        // Replace [word_start_col .. cursor_col] on cursor_row with `replacement`.
        if let Some(line) = self.lines.get_mut(state.cursor_row) {
            let start_byte = char_index_to_byte(line, state.word_start_col);
            let end_byte = char_index_to_byte(line, state.cursor_col);
            line.replace_range(start_byte..end_byte, &replacement);
            self.cursor_row = state.cursor_row;
            self.cursor_col = state.word_start_col + replacement.chars().count();
            self.dirty = true;
        }

        self.status = format!(
            "Completion {}/{}: {}  (Tab again to cycle)",
            cycle_pos, total, replacement
        );
        // Update cursor_col for the next cycle iteration.
        state.cursor_col = self.cursor_col;
        self.dg_tab_state = Some(state);
    }

    // ============================================================
    // Undo / redo
    // ============================================================

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }

    fn push_undo(&mut self, kind: BatchKind) {
        // Coalesce same-kind runs into one batch entry.
        if self.open_batch == Some(kind) {
            self.redo_stack.clear();
            return;
        }
        let snap = self.snapshot();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.open_batch = Some(kind);
        self.redo_stack.clear();
    }

    fn close_batch(&mut self) {
        self.open_batch = None;
    }

    fn undo(&mut self) {
        self.close_batch();
        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.restore(snap);
            self.dirty = true;
            self.status = format!("Undo — {} redo step(s) available.", self.redo_stack.len());
        } else {
            self.status = String::from("Nothing to undo.");
        }
        self.render();
    }

    fn redo(&mut self) {
        self.close_batch();
        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.restore(snap);
            self.dirty = true;
            self.status = format!("Redo — {} undo step(s) available.", self.undo_stack.len());
        } else {
            self.status = String::from("Nothing to redo.");
        }
        self.render();
    }

    fn restore(&mut self, snap: Snapshot) {
        self.lines = snap.lines;
        self.cursor_row = snap.cursor_row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = snap.cursor_col.min(self.current_line_chars());
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
    }

    // ============================================================
    // Kill ring (cut / paste)
    // ============================================================

    fn cut_line(&mut self) {
        // If the previous keystroke was also Ctrl-K, append; otherwise reset.
        if !self.last_was_kill {
            self.kill_ring.clear();
        }
        self.push_undo(BatchKind::Delete);
        self.close_batch();

        if self.cursor_row >= self.lines.len() {
            self.status = String::from("Nothing to cut.");
            self.render();
            return;
        }

        // Nano semantics: Ctrl-K cuts from cursor to end of line. If the
        // cursor is at end of line, it absorbs the trailing newline (joining
        // with the next line). On an empty line that's just a line removal.
        let line = &self.lines[self.cursor_row];
        let line_chars = line.chars().count();
        if self.cursor_col < line_chars {
            let byte_idx = char_index_to_byte(line, self.cursor_col);
            let tail: String = line[byte_idx..].to_string();
            self.lines[self.cursor_row].truncate(byte_idx);
            self.kill_ring.push(tail);
        } else if self.cursor_row + 1 < self.lines.len() {
            // At EOL — eat the newline so the next line joins onto this one.
            let next = self.lines.remove(self.cursor_row + 1);
            // The "cut" includes the line break, represented as an empty
            // element so paste restores the newline.
            self.kill_ring.push(String::new());
            // The joined-in text belongs to the cursor's line now.
            self.lines[self.cursor_row].push_str(&next);
            // But we want paste to bring back the chunk we just joined.
            // Append it as a second kill entry so paste reproduces both.
            self.kill_ring.push(next);
        } else {
            // Last line, cursor at EOL — nothing left to cut.
            self.status = String::from("Nothing to cut.");
            self.render();
            return;
        }

        self.dirty = true;
        self.status = format!(
            "Cut into kill ring ({} chunk{}). ^U to paste.",
            self.kill_ring.len(),
            if self.kill_ring.len() == 1 { "" } else { "s" }
        );
        self.render();
    }

    fn paste_kill_ring(&mut self) {
        if self.kill_ring.is_empty() {
            self.status = String::from("Kill ring is empty.");
            self.render();
            return;
        }
        self.push_undo(BatchKind::Paste);
        self.close_batch();

        let to_paste: String = self.kill_ring.join("\n");
        for c in to_paste.chars() {
            if c == '\n' {
                self.insert_newline();
            } else {
                self.insert_char(c);
            }
        }
        self.dirty = true;
        self.status = String::from("Pasted from kill ring.");
        self.render();
    }

    // ============================================================
    // Search / replace
    // ============================================================

    /// Find the next case-insensitive occurrence of `query` starting one
    /// position past the cursor. Wraps around to the top of the buffer.
    fn find_next(&mut self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        let q = query.to_lowercase();
        let total_lines = self.lines.len();

        let start_row = self.cursor_row;
        let start_col_chars = self.cursor_col;

        // Search from the cursor to end of buffer, then wrap.
        for offset in 0..=total_lines {
            let row = (start_row + offset) % total_lines;
            let line = &self.lines[row];
            let lower = line.to_lowercase();
            let from_byte = if offset == 0 {
                char_index_to_byte(line, (start_col_chars + 1).min(line.chars().count()))
            } else {
                0
            };
            if from_byte <= lower.len() {
                if let Some(pos) = lower[from_byte..].find(&q) {
                    let byte_idx = from_byte + pos;
                    let char_idx = byte_to_char_index(line, byte_idx);
                    self.cursor_row = row;
                    self.cursor_col = char_idx;
                    return true;
                }
            }
        }
        false
    }

    /// Plain case-sensitive replace-all. Returns the number of replacements.
    fn replace_all(&mut self, query: &str, replacement: &str) -> usize {
        if query.is_empty() {
            return 0;
        }
        let mut count = 0;
        // Snapshot for undo BEFORE the bulk mutation.
        let pre = self.snapshot();
        for line in self.lines.iter_mut() {
            // Count occurrences cheaply with `matches`, then replace once.
            let occ = line.matches(query).count();
            if occ > 0 {
                *line = line.replace(query, replacement);
                count += occ;
            }
        }
        if count > 0 {
            self.undo_stack.push(pre);
            if self.undo_stack.len() > MAX_UNDO {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
            self.open_batch = None;
            self.dirty = true;
            // Re-clamp cursor in case the line shrank.
            self.cursor_col = self.cursor_col.min(self.current_line_chars());
        }
        count
    }

    // ============================================================
    // Rendering
    // ============================================================

    fn body_rows(&self) -> usize {
        (self.height as usize).saturating_sub(3)
    }

    fn ensure_scroll_visible(&mut self) {
        let body = self.body_rows();
        if body == 0 {
            self.scroll_row = self.cursor_row;
            return;
        }
        let width = self.width as usize;
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
            return;
        }
        // Walk scroll_row forward until the cursor's visual row fits.
        while self.scroll_row < self.cursor_row && self.cursor_visual_row(width) >= body {
            self.scroll_row += 1;
        }
    }

    /// Map a 1-based terminal (row, col) from a mouse click into the
    /// logical buffer position. Returns None for clicks outside the
    /// body area (header / footers / past-EOF tildes).
    fn click_to_logical(&self, term_row: u16, term_col: u16) -> Option<(usize, usize)> {
        let body_rows = self.body_rows();
        let visual_row = (term_row as usize).checked_sub(2)?;
        if visual_row >= body_rows {
            return None;
        }
        let visual_col = (term_col as usize).saturating_sub(1);
        let width = self.width as usize;
        let mut visual_walker = 0usize;
        let mut logical_row = self.scroll_row;
        while logical_row < self.lines.len() {
            let segments = wrap_line(&self.lines[logical_row], width);
            let breaks = wrap_breaks(&self.lines[logical_row], width);
            for (seg_idx, seg) in segments.iter().enumerate() {
                if visual_walker == visual_row {
                    let seg_start = breaks[seg_idx];
                    let seg_char_len = seg.chars().count();
                    let col_in_seg = visual_col.min(seg_char_len);
                    return Some((logical_row, seg_start + col_in_seg));
                }
                visual_walker += 1;
            }
            logical_row += 1;
        }
        None
    }

    /// Visual row offset (0-based) of the cursor within the current viewport.
    fn cursor_visual_row(&self, width: usize) -> usize {
        let mut visual = 0usize;
        let mut r = self.scroll_row;
        while r < self.cursor_row && r < self.lines.len() {
            visual += wrap_line(&self.lines[r], width).len();
            r += 1;
        }
        if self.cursor_row < self.lines.len() {
            let breaks = wrap_breaks(&self.lines[self.cursor_row], width);
            let (seg_idx, _) = locate_in_wrap(&breaks, self.cursor_col);
            visual += seg_idx;
        }
        visual
    }

    /// Column offset (0-based) of the cursor on its visual row.
    fn cursor_visual_col(&self, width: usize) -> usize {
        if self.cursor_row >= self.lines.len() {
            return 0;
        }
        let breaks = wrap_breaks(&self.lines[self.cursor_row], width);
        let (_, col_in_seg) = locate_in_wrap(&breaks, self.cursor_col);
        col_in_seg
    }

    pub fn render(&mut self) {
        if self.mode == EditorMode::Help {
            self.render_help();
            return;
        }
        self.render_editing();
    }

    fn render_editing(&mut self) {
        // Live syntax check refreshes once per render; cheap parses on
        // typical trigger bodies but skipped entirely for non-DG kinds.
        if self.dg_mode() {
            self.dg_error = dg::syntax_check(&self.take_text());
        }

        self.ensure_scroll_visible();
        let w = self.width as usize;
        let body_rows = self.body_rows();
        let mut out: Vec<u8> = Vec::with_capacity(2048);
        out.extend_from_slice(b"\x1b[2J\x1b[H");

        // Header.
        let dirty_marker = if self.dirty { " *modified*" } else { "" };
        let header = if let Some((line, _)) = &self.dg_error {
            format!(
                "[BETA EDITOR] {} ({}/{} lines{}) — DG error on line {}",
                self.kind.label(),
                self.cursor_row + 1,
                self.lines.len(),
                dirty_marker,
                line
            )
        } else {
            format!(
                "[BETA EDITOR] {} ({}/{} lines{})",
                self.kind.label(),
                self.cursor_row + 1,
                self.lines.len(),
                dirty_marker
            )
        };
        out.extend_from_slice(b"\x1b[7m");
        out.extend_from_slice(pad_to_width(&header, w).as_bytes());
        out.extend_from_slice(b"\x1b[0m");
        out.extend_from_slice(b"\r\n");

        // Body — word-wrap each logical line into visual segments. DG
        // mode tokenizes per segment for syntax highlighting and
        // underlines the first syntax-error line. When a selection is
        // active, DG highlighting is suppressed in favour of an
        // inverse-video span over the selected range.
        let highlight = self.dg_mode() && self.colour_enabled;
        let error_line_idx = self.dg_error.as_ref().map(|(l, _)| l.saturating_sub(1));
        let selection = self.normalized_selection();

        let mut visual_row = 0usize;
        let mut logical_row = self.scroll_row;
        while visual_row < body_rows {
            if logical_row >= self.lines.len() {
                out.extend_from_slice(b"\x1b[2m~\x1b[0m\r\n");
                visual_row += 1;
                logical_row += 1;
                continue;
            }
            let raw = self.lines[logical_row].clone();
            let segments = wrap_line(&raw, w);
            let breaks = wrap_breaks(&raw, w);
            let mark_error = Some(logical_row) == error_line_idx;

            for (seg_idx, seg) in segments.iter().enumerate() {
                if visual_row >= body_rows {
                    break;
                }
                if mark_error {
                    out.extend_from_slice(b"\x1b[31;4m");
                }

                // Compute the selection slice within this segment, if any.
                let sel_in_seg = selection.and_then(|((sr, sc), (er, ec))| {
                    let seg_start = breaks[seg_idx];
                    let seg_len = seg.chars().count();
                    let seg_end = seg_start + seg_len;

                    // Determine selected char range on this logical line.
                    let line_sel_start = if logical_row < sr {
                        return None;
                    } else if logical_row == sr {
                        sc
                    } else {
                        0
                    };
                    let line_sel_end = if logical_row > er {
                        return None;
                    } else if logical_row == er {
                        ec
                    } else {
                        raw.chars().count()
                    };
                    if line_sel_start >= seg_end || line_sel_end <= seg_start {
                        return None;
                    }
                    let s = line_sel_start.saturating_sub(seg_start).min(seg_len);
                    let e = line_sel_end.saturating_sub(seg_start).min(seg_len);
                    if s == e {
                        None
                    } else {
                        Some((s, e))
                    }
                });

                if let Some((s, e)) = sel_in_seg {
                    let chars: Vec<char> = seg.chars().collect();
                    let prefix: String = chars[..s].iter().collect();
                    let middle: String = chars[s..e].iter().collect();
                    let suffix: String = chars[e..].iter().collect();
                    out.extend_from_slice(prefix.as_bytes());
                    out.extend_from_slice(b"\x1b[7m");
                    out.extend_from_slice(middle.as_bytes());
                    out.extend_from_slice(b"\x1b[0m");
                    out.extend_from_slice(suffix.as_bytes());
                } else if highlight {
                    let spans = dg::tokenize_line(seg);
                    out.extend_from_slice(dg::render_line(&spans, true).as_bytes());
                } else {
                    out.extend_from_slice(seg.as_bytes());
                }

                if mark_error {
                    out.extend_from_slice(b"\x1b[0m");
                }
                out.extend_from_slice(b"\r\n");
                visual_row += 1;
            }
            logical_row += 1;
        }

        // Footer status (modal-aware). DG syntax error overrides the
        // ambient status hint when we're in editing mode.
        let status_line = match &self.mode {
            EditorMode::Search { query } => format!("Search: {}_", query),
            EditorMode::ReplaceQuery { query } => format!("Replace — find: {}_", query),
            EditorMode::ReplaceWith { query, replacement } => {
                format!("Replace '{}' with: {}_", query, replacement)
            }
            EditorMode::GotoLine { input } => format!("Go to line #: {}_", input),
            EditorMode::Editing => {
                if let Some((line, msg)) = &self.dg_error {
                    format!("DG syntax error (line {}): {}", line, msg)
                } else {
                    self.status.clone()
                }
            }
            _ => self.status.clone(),
        };
        out.extend_from_slice(b"\x1b[7m");
        out.extend_from_slice(pad_to_width(&status_line, w).as_bytes());
        out.extend_from_slice(b"\x1b[0m");
        out.extend_from_slice(b"\r\n");

        // Key-hint row, varies by mode.
        let hints = match self.mode {
            EditorMode::ConfirmExitSaveOrDiscard => {
                "Y Yes (save)   N No (discard)   ^C Cancel (back to editing)"
            }
            EditorMode::ConfirmDiscard => "Y Yes (discard)   N No (keep editing)",
            EditorMode::Search { .. } => "Enter Find   ^C Cancel",
            EditorMode::ReplaceQuery { .. } => "Enter Next stage   ^C Cancel",
            EditorMode::ReplaceWith { .. } => "Enter Apply replace-all   ^C Cancel",
            EditorMode::GotoLine { .. } => "Enter Jump   ^C Cancel",
            _ => "^O Save  ^X Exit  ^W Search  ^\\ Replace  ^_ Goto  ^Z Undo  ^G Help",
        };
        out.extend_from_slice(b"\x1b[2m");
        out.extend_from_slice(pad_to_width(hints, w).as_bytes());
        out.extend_from_slice(b"\x1b[0m");

        // Cursor: in modal prompts park at end of the prompt input on the
        // status row; otherwise on the actual edit cursor cell.
        let cursor_escape = match &self.mode {
            EditorMode::Search { query } => format!(
                "\x1b[{};{}H",
                self.height as usize - 1,
                "Search: ".chars().count() + query.chars().count() + 1
            ),
            EditorMode::ReplaceQuery { query } => format!(
                "\x1b[{};{}H",
                self.height as usize - 1,
                "Replace — find: ".chars().count() + query.chars().count() + 1
            ),
            EditorMode::ReplaceWith { query, replacement } => format!(
                "\x1b[{};{}H",
                self.height as usize - 1,
                "Replace '".chars().count()
                    + query.chars().count()
                    + "' with: ".chars().count()
                    + replacement.chars().count()
                    + 1
            ),
            EditorMode::GotoLine { input } => format!(
                "\x1b[{};{}H",
                self.height as usize - 1,
                "Go to line #: ".chars().count() + input.chars().count() + 1
            ),
            _ => {
                let vrow = self.cursor_visual_row(w);
                let vcol = self.cursor_visual_col(w);
                let cur_col = (vcol + 1).min(w);
                let cur_row = 2 + vrow;
                format!("\x1b[{};{}H", cur_row, cur_col)
            }
        };
        out.extend_from_slice(cursor_escape.as_bytes());

        self.pending_output = out;
    }

    fn render_help(&mut self) {
        let w = self.width as usize;
        let mut out: Vec<u8> = Vec::with_capacity(2048);
        out.extend_from_slice(b"\x1b[2J\x1b[H");

        out.extend_from_slice(b"\x1b[7m");
        out.extend_from_slice(
            pad_to_width("[BETA EDITOR] Help — press any key to return", w).as_bytes(),
        );
        out.extend_from_slice(b"\x1b[0m\r\n\r\n");

        let lines = [
            "Movement:",
            "  Arrow keys, Home, End, PageUp, PageDown",
            "  Ctrl-A start of line     Ctrl-E end of line",
            "",
            "Editing:",
            "  printable chars insert at cursor",
            "  Enter splits the line at the cursor",
            "  Backspace removes the char to the left (joins lines at col 0)",
            "  Delete removes the char under the cursor (joins lines at EOL)",
            "  Tab inserts a literal tab",
            "",
            "Cut / paste:",
            "  Ctrl-K cut current line (consecutive cuts accumulate)",
            "  Ctrl-U paste the kill ring at the cursor",
            "",
            "Undo / redo:",
            "  Ctrl-Z undo last batch     Ctrl-Y redo",
            "",
            "Search / replace / goto:",
            "  Ctrl-W incremental search (Enter jumps, ^C cancels)",
            "  Ctrl-\\ replace-all  (find, then replacement, Enter applies)",
            "  Ctrl-_ jump to line number",
            "",
            "Save / exit:",
            "  Ctrl-O save and exit",
            "  Ctrl-X exit (prompts on a dirty buffer)",
            "  Ctrl-C cancel (prompts on a dirty buffer)",
            "",
            "  Ctrl-L redraw screen     Ctrl-G this help",
            "  Ctrl-T toggle mouse tracking (turn off to paste from clipboard)",
        ];
        for l in lines.iter() {
            out.extend_from_slice(pad_to_width(l, w).as_bytes());
            out.extend_from_slice(b"\r\n");
        }

        // Parking cursor at the bottom corner so it's out of the way.
        out.extend_from_slice(
            format!("\x1b[{};1H", self.height as usize).as_bytes(),
        );
        self.pending_output = out;
    }
}

/// DG auto-indent: produce the whitespace to prepend on a new line
/// after Enter. Copies the leading indent of the current line; if the
/// trimmed line starts with a block-opening keyword, adds two spaces.
/// Only consulted when the editor is hosting a DG trigger body.
fn dg_auto_indent_for(current_line: &str, cursor_col: usize) -> String {
    // Leading whitespace of the existing line, char-truncated to the
    // cursor position so splitting mid-indent doesn't double up.
    let leading: String = current_line
        .chars()
        .take(cursor_col)
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();

    let trimmed = current_line.trim_start();
    let first_word = trimmed
        .split_whitespace()
        .next()
        .map(|w| w.to_ascii_lowercase())
        .unwrap_or_default();
    let opens_block = matches!(
        first_word.as_str(),
        "if" | "elseif" | "else" | "while" | "switch" | "case" | "default"
    );

    if opens_block {
        format!("{}  ", leading)
    } else {
        leading
    }
}

/// Word-wrap a single logical line into visual segments that each fit
/// within `width` display columns. Breaks at the last whitespace before
/// the wrap point; falls back to a hard break when a word exceeds
/// `width`. An empty input yields a single empty segment so the line
/// still occupies one visual row.
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_string()];
    }
    if line.is_empty() {
        return vec![String::new()];
    }
    let chars: Vec<char> = line.chars().collect();
    let mut segments: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let mut end = (start + width).min(chars.len());
        if end < chars.len() {
            let mut break_at = end;
            while break_at > start && !chars[break_at - 1].is_whitespace() {
                break_at -= 1;
            }
            if break_at <= start {
                break_at = end; // no whitespace in window — hard wrap
            }
            end = break_at;
        }
        let seg: String = chars[start..end].iter().collect();
        segments.push(seg);
        start = end;
    }
    if segments.is_empty() {
        segments.push(String::new());
    }
    segments
}

/// Char index at which each wrapped segment starts. Always begins with 0.
fn wrap_breaks(line: &str, width: usize) -> Vec<usize> {
    if width == 0 || line.is_empty() {
        return vec![0];
    }
    let segments = wrap_line(line, width);
    let mut breaks = Vec::with_capacity(segments.len());
    let mut acc = 0usize;
    for seg in &segments {
        breaks.push(acc);
        acc += seg.chars().count();
    }
    breaks
}

/// Map a char-index within a logical line to (visual_segment, col_in_segment).
fn locate_in_wrap(breaks: &[usize], col: usize) -> (usize, usize) {
    for i in (0..breaks.len()).rev() {
        if breaks[i] <= col {
            return (i, col - breaks[i]);
        }
    }
    (0, col)
}

/// Locate the char index at which the tab-completion replacement should
/// start. For `%`-anchored completions that's the position of the `%`;
/// for first-token completions it's column 0 after any leading
/// whitespace.
fn compute_word_start(prefix_before_cursor: &str) -> usize {
    if let Some(pct_byte) = prefix_before_cursor.rfind('%') {
        let after = &prefix_before_cursor[pct_byte + 1..];
        if !after.contains('%') {
            return byte_to_char_index(prefix_before_cursor, pct_byte);
        }
    }
    let trimmed_chars = prefix_before_cursor
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    trimmed_chars
}

fn char_index_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn byte_to_char_index(s: &str, byte_idx: usize) -> usize {
    s.char_indices()
        .take_while(|(b, _)| *b < byte_idx)
        .count()
}

fn pad_to_width(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        if used + 1 > width {
            break;
        }
        out.push(c);
        used += 1;
    }
    while used < width {
        out.push(' ');
        used += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_kind_round_trips() {
        for mode in [
            "collecting_desc",
            "collecting_long",
            "collecting_note",
            "collecting_board_post",
            "collecting_dg_body",
            "collecting_dg_proto_body",
            "collecting_dialogue_node_text",
            "collecting_extra_desc",
            "collecting_motd",
        ] {
            assert!(EditorKind::from_olc_mode(mode).is_some(), "{mode} should map");
        }
        assert!(EditorKind::from_olc_mode("ai_confirm").is_none());
    }

    #[test]
    fn seeds_empty_into_single_line() {
        let s = EditorSession::new(EditorKind::RoomDesc, "", 80, 24);
        assert_eq!(s.lines, vec![String::new()]);
    }

    #[test]
    fn seeds_multi_line_text() {
        let s = EditorSession::new(EditorKind::ItemNote, "alpha\nbeta\ngamma", 80, 24);
        assert_eq!(s.lines, vec!["alpha", "beta", "gamma"]);
        assert_eq!(s.take_text(), "alpha\nbeta\ngamma");
    }

    #[test]
    fn ctrl_x_cancels_clean_buffer() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "hello", 80, 24);
        assert_eq!(s.handle_key(&KeyEvent::CtrlX), EditorAction::Cancel);
    }

    #[test]
    fn ctrl_x_dirty_then_y_saves() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "hi", 80, 24);
        s.handle_key(&KeyEvent::Char('!'));
        s.handle_key(&KeyEvent::CtrlX);
        assert_eq!(s.handle_key(&KeyEvent::Char('Y')), EditorAction::Save);
    }

    #[test]
    fn ctrl_x_dirty_then_n_discards() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "hi", 80, 24);
        s.handle_key(&KeyEvent::Char('!'));
        s.handle_key(&KeyEvent::CtrlX);
        assert_eq!(s.handle_key(&KeyEvent::Char('N')), EditorAction::Cancel);
    }

    #[test]
    fn ctrl_c_dirty_then_n_keeps_editing() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "hi", 80, 24);
        s.handle_key(&KeyEvent::End);
        s.handle_key(&KeyEvent::Char('!'));
        s.handle_key(&KeyEvent::CtrlC);
        assert_eq!(s.handle_key(&KeyEvent::Char('N')), EditorAction::None);
        // Buffer still dirty, can keep typing.
        s.handle_key(&KeyEvent::Char('?'));
        assert_eq!(s.take_text(), "hi!?");
    }

    #[test]
    fn ctrl_c_dirty_then_y_discards() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "hi", 80, 24);
        s.handle_key(&KeyEvent::Char('!'));
        s.handle_key(&KeyEvent::CtrlC);
        assert_eq!(s.handle_key(&KeyEvent::Char('Y')), EditorAction::Cancel);
    }

    #[test]
    fn char_insert_updates_cursor_and_text() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "", 80, 24);
        s.handle_key(&KeyEvent::Char('a'));
        s.handle_key(&KeyEvent::Char('b'));
        s.handle_key(&KeyEvent::Char('c'));
        assert_eq!(s.take_text(), "abc");
        assert_eq!((s.cursor_row, s.cursor_col), (0, 3));
        assert!(s.dirty);
    }

    #[test]
    fn enter_splits_line() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "abcdef", 80, 24);
        s.cursor_col = 3;
        s.handle_key(&KeyEvent::Enter);
        assert_eq!(s.lines, vec!["abc", "def"]);
        assert_eq!((s.cursor_row, s.cursor_col), (1, 0));
    }

    #[test]
    fn backspace_at_col_zero_joins_lines() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "abc\ndef", 80, 24);
        s.cursor_row = 1;
        s.cursor_col = 0;
        s.handle_key(&KeyEvent::Backspace);
        assert_eq!(s.lines, vec!["abcdef"]);
        assert_eq!((s.cursor_row, s.cursor_col), (0, 3));
    }

    #[test]
    fn undo_redo_round_trips_a_typing_run() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "", 80, 24);
        s.handle_key(&KeyEvent::Char('h'));
        s.handle_key(&KeyEvent::Char('i'));
        assert_eq!(s.take_text(), "hi");
        s.handle_key(&KeyEvent::CtrlZ);
        // Coalesced into one batch.
        assert_eq!(s.take_text(), "");
        s.handle_key(&KeyEvent::CtrlY);
        assert_eq!(s.take_text(), "hi");
    }

    #[test]
    fn cursor_move_closes_undo_batch() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "", 80, 24);
        s.handle_key(&KeyEvent::Char('a'));
        s.handle_key(&KeyEvent::ArrowLeft);
        s.handle_key(&KeyEvent::Char('b'));
        s.handle_key(&KeyEvent::CtrlZ);
        assert_eq!(s.take_text(), "a"); // only the 'b' insert undone
    }

    #[test]
    fn cut_and_paste_round_trip() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello world", 80, 24);
        s.handle_key(&KeyEvent::Home);
        s.handle_key(&KeyEvent::CtrlK);
        assert_eq!(s.lines, vec![""]);
        s.handle_key(&KeyEvent::CtrlU);
        assert_eq!(s.take_text(), "hello world");
    }

    #[test]
    fn search_finds_next_match() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "alpha\nbeta\nALPHA tail", 80, 24);
        s.handle_key(&KeyEvent::CtrlW);
        for c in "alpha".chars() {
            s.handle_key(&KeyEvent::Char(c));
        }
        s.handle_key(&KeyEvent::Enter);
        // Search starts past the cursor; we began at (0,0), so the next
        // occurrence after that is "ALPHA" on row 2.
        assert_eq!(s.cursor_row, 2);
        assert_eq!(s.cursor_col, 0);
    }

    #[test]
    fn replace_all_replaces_every_occurrence() {
        let mut s =
            EditorSession::new(EditorKind::ItemNote, "foo bar foo\nbaz foo qux", 80, 24);
        s.handle_key(&KeyEvent::CtrlBackslash);
        for c in "foo".chars() {
            s.handle_key(&KeyEvent::Char(c));
        }
        s.handle_key(&KeyEvent::Enter);
        for c in "X".chars() {
            s.handle_key(&KeyEvent::Char(c));
        }
        s.handle_key(&KeyEvent::Enter);
        assert_eq!(s.take_text(), "X bar X\nbaz X qux");
        assert!(s.status.contains("Replaced 3"));
    }

    #[test]
    fn goto_line_jumps_to_target() {
        let many: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let mut s = EditorSession::new(EditorKind::Motd, &many.join("\n"), 80, 24);
        s.handle_key(&KeyEvent::CtrlUnderscore);
        for c in "15".chars() {
            s.handle_key(&KeyEvent::Char(c));
        }
        s.handle_key(&KeyEvent::Enter);
        assert_eq!(s.cursor_row, 14);
    }

    #[test]
    fn goto_line_out_of_range_reports_error() {
        let mut s = EditorSession::new(EditorKind::Motd, "only\ntwo\nlines", 80, 24);
        s.handle_key(&KeyEvent::CtrlUnderscore);
        for c in "99".chars() {
            s.handle_key(&KeyEvent::Char(c));
        }
        s.handle_key(&KeyEvent::Enter);
        assert!(s.status.contains("out of range"));
    }

    #[test]
    fn help_overlay_dismisses_on_any_key() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "x", 80, 24);
        s.handle_key(&KeyEvent::CtrlG);
        // Overlay is up — any key (including a normal char) dismisses it.
        s.handle_key(&KeyEvent::Char('a'));
        // Buffer unchanged because the key only dismissed the overlay.
        assert_eq!(s.take_text(), "x");
    }

    #[test]
    fn ctrl_o_refuses_oversize() {
        let oversize: String = "a".repeat(32 * 1024 + 1);
        let mut s = EditorSession::new(EditorKind::ItemNote, &oversize, 80, 24);
        assert_eq!(s.handle_key(&KeyEvent::CtrlO), EditorAction::None);
        assert!(s.status.contains("too large"));
    }

    #[test]
    fn render_emits_cursor_positioning_escape() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "abc", 80, 24);
        s.cursor_col = 2;
        s.render();
        let out = String::from_utf8_lossy(&s.pending_output).into_owned();
        assert!(out.contains("\x1b[2;3H"), "cursor escape missing: {out:?}");
    }

    #[test]
    fn wrap_line_breaks_at_whitespace() {
        let segs = wrap_line("the quick brown fox jumps over the lazy dog", 12);
        assert!(segs.iter().all(|s| s.chars().count() <= 12));
        // First segment should end at a whitespace boundary, not mid-word.
        assert!(segs[0].ends_with(' ') || segs[0].chars().count() <= 12);
        assert_eq!(segs.concat(), "the quick brown fox jumps over the lazy dog");
    }

    #[test]
    fn wrap_line_hard_wraps_long_words() {
        let segs = wrap_line("supercalifragilisticexpialidocious", 10);
        assert!(segs.iter().all(|s| s.chars().count() <= 10));
        assert_eq!(segs.concat(), "supercalifragilisticexpialidocious");
    }

    #[test]
    fn wrap_empty_line_yields_one_empty_segment() {
        let segs = wrap_line("", 80);
        assert_eq!(segs, vec![String::new()]);
    }

    #[test]
    fn cursor_visual_row_tracks_across_wrapped_segments() {
        let long_line = "alpha beta gamma delta epsilon zeta eta theta";
        let mut s = EditorSession::new(EditorKind::RoomDesc, long_line, 20, 24);
        // Place the cursor near the end; it should fall onto the second
        // (or later) wrapped row, not row 0.
        s.cursor_col = long_line.chars().count();
        let vrow = s.cursor_visual_row(s.width as usize);
        assert!(vrow >= 1, "expected wrapped cursor row, got {vrow}");
    }

    #[test]
    fn mouse_click_in_body_positions_cursor() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "alpha\nbeta\ngamma", 80, 24);
        // Terminal row 4 = body visual row 2 = logical line 2 ("gamma").
        // Terminal col 3 = visual col 2 = logical col 2.
        let action = s.handle_key(&KeyEvent::MouseClick { row: 4, col: 3 });
        assert_eq!(action, EditorAction::None);
        assert_eq!((s.cursor_row, s.cursor_col), (2, 2));
    }

    #[test]
    fn ctrl_t_toggles_mouse_tracking_and_emits_decset_decrst() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "alpha", 80, 24);
        assert!(s.mouse_enabled, "mouse tracking should default on");
        let _ = s.take_output();

        s.handle_key(&KeyEvent::CtrlT);
        assert!(!s.mouse_enabled, "first ^T should disable tracking");
        let out = s.take_output();
        assert!(
            out.windows(8).any(|w| w == b"\x1b[?1000l"),
            "expected DECRST 1000 in output after toggle-off"
        );
        assert!(s.status.contains("OFF"));

        s.handle_key(&KeyEvent::CtrlT);
        assert!(s.mouse_enabled, "second ^T should re-enable tracking");
        let out = s.take_output();
        assert!(
            out.windows(8).any(|w| w == b"\x1b[?1000h"),
            "expected DECSET 1000 in output after toggle-on"
        );
        assert!(s.status.contains("ON"));
    }

    #[test]
    fn mouse_click_past_eol_clamps_to_line_end() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "abc", 80, 24);
        // Clicking far past the end of line 1 (3 chars) should clamp.
        s.handle_key(&KeyEvent::MouseClick { row: 2, col: 50 });
        assert_eq!((s.cursor_row, s.cursor_col), (0, 3));
    }

    #[test]
    fn shift_arrow_creates_selection() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello", 80, 24);
        s.handle_key(&KeyEvent::ShiftArrowRight);
        s.handle_key(&KeyEvent::ShiftArrowRight);
        assert_eq!(s.selection_anchor, Some((0, 0)));
        assert_eq!((s.cursor_row, s.cursor_col), (0, 2));
    }

    #[test]
    fn plain_arrow_clears_selection() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello", 80, 24);
        s.handle_key(&KeyEvent::ShiftArrowRight);
        s.handle_key(&KeyEvent::ArrowRight);
        assert!(s.selection_anchor.is_none());
    }

    #[test]
    fn char_insert_replaces_selection() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello world", 80, 24);
        // Select "hello" by shift-right ×5
        for _ in 0..5 {
            s.handle_key(&KeyEvent::ShiftArrowRight);
        }
        s.handle_key(&KeyEvent::Char('h'));
        s.handle_key(&KeyEvent::Char('i'));
        assert_eq!(s.take_text(), "hi world");
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello world", 80, 24);
        for _ in 0..5 {
            s.handle_key(&KeyEvent::ShiftArrowRight);
        }
        s.handle_key(&KeyEvent::Backspace);
        assert_eq!(s.take_text(), " world");
        assert!(s.selection_anchor.is_none());
    }

    #[test]
    fn shift_arrow_down_selects_across_lines() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "line one\nline two", 80, 24);
        s.handle_key(&KeyEvent::ShiftArrowDown);
        let (start, end) = s.normalized_selection().expect("selection set");
        assert_eq!(start, (0, 0));
        assert_eq!(end, (1, 0));
    }

    #[test]
    fn ctrl_k_cuts_selection_not_line() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello world", 80, 24);
        for _ in 0..5 {
            s.handle_key(&KeyEvent::ShiftArrowRight);
        }
        s.handle_key(&KeyEvent::CtrlK);
        assert_eq!(s.take_text(), " world");
        s.handle_key(&KeyEvent::CtrlU);
        // Paste re-inserts at the cursor.
        assert_eq!(s.take_text(), "hello world");
    }

    #[test]
    fn render_inverse_video_when_selection_active() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "hello", 80, 24);
        for _ in 0..3 {
            s.handle_key(&KeyEvent::ShiftArrowRight);
        }
        s.render();
        let out = String::from_utf8_lossy(&s.pending_output).into_owned();
        assert!(out.contains("\x1b[7m"), "inverse not emitted: {out:?}");
    }

    #[test]
    fn mouse_click_outside_body_is_ignored() {
        let mut s = EditorSession::new(EditorKind::ItemNote, "abc", 80, 24);
        // Header row.
        s.handle_key(&KeyEvent::MouseClick { row: 1, col: 5 });
        assert_eq!((s.cursor_row, s.cursor_col), (0, 0));
    }

    #[test]
    fn dg_enter_copies_leading_indent() {
        let mut s = EditorSession::new(EditorKind::DgTriggerBody, "  set x 1", 80, 24);
        s.handle_key(&KeyEvent::End);
        s.handle_key(&KeyEvent::Enter);
        // New line starts with the same two-space indent.
        assert_eq!(s.lines, vec!["  set x 1", "  "]);
        assert_eq!((s.cursor_row, s.cursor_col), (1, 2));
    }

    #[test]
    fn dg_enter_after_if_adds_extra_indent() {
        let mut s = EditorSession::new(EditorKind::DgTriggerBody, "if %actor.is_pc%", 80, 24);
        s.handle_key(&KeyEvent::End);
        s.handle_key(&KeyEvent::Enter);
        // 0 leading + 2 extra = "  "
        assert_eq!(s.lines, vec!["if %actor.is_pc%", "  "]);
    }

    #[test]
    fn non_dg_enter_does_not_auto_indent() {
        let mut s = EditorSession::new(EditorKind::RoomDesc, "  some line", 80, 24);
        s.handle_key(&KeyEvent::End);
        s.handle_key(&KeyEvent::Enter);
        assert_eq!(s.lines, vec!["  some line", ""]);
    }

    #[test]
    fn long_ai_paragraph_renders_without_truncation_marker() {
        let paragraph = "This is a long AI-generated description that would normally overflow the viewport width and clip with a truncation marker but should now wrap onto multiple visual rows so the builder can read the whole thing.";
        let mut s = EditorSession::new(EditorKind::RoomDesc, paragraph, 40, 24);
        s.render();
        let out = String::from_utf8_lossy(&s.pending_output).into_owned();
        // The dim '>' marker comes from the legacy clip path; word wrap
        // should never emit it.
        assert!(!out.contains("\x1b[2m>\x1b[0m"), "found truncation marker in wrapped render");
    }
}
