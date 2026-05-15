// src/script/editor.rs
// Rhai bindings for the modern multi-line editor (beta opt-in).
//
// `try_enter_modern_editor(connection_id) -> bool` is called by each OLC
// entry script (redit/oedit/medit/board/etc.) after it has set the legacy
// `olc_mode` and seeded `olc_buffer`. If the player has `set new_editor on`
// and their client negotiated NAWS / ANSI / char-mode, the call swaps in
// a `modern_editor` session, renders its splash, and returns true — the
// caller then skips printing the legacy `.help` text. Otherwise it returns
// false and the legacy line editor proceeds unchanged.

use crate::SharedConnections;
use crate::editor::{EditorKind, EditorSession};
use crate::telnet::MTTS_ANSI;
use rhai::Engine;

pub fn register(engine: &mut Engine, connections: SharedConnections) {
    let conns = connections;
    engine.register_fn(
        "try_enter_modern_editor",
        move |connection_id: String| -> bool {
            let conn_id = match uuid::Uuid::parse_str(&connection_id) {
                Ok(id) => id,
                Err(_) => return false,
            };

            let mut conns_lock = conns.lock().unwrap();
            let session = match conns_lock.get_mut(&conn_id) {
                Some(s) => s,
                None => return false,
            };

            // Flag-gated.
            let flag_on = session
                .character
                .as_ref()
                .map(|c| c.new_editor_enabled)
                .unwrap_or(false);
            if !flag_on {
                return false;
            }

            // Capability gate — char mode, valid window size, ANSI.
            if !session.telnet_state.char_mode {
                return false;
            }
            if session.telnet_state.window_width == 0 || session.telnet_state.window_height == 0 {
                return false;
            }
            if session.telnet_state.mtts_flags & MTTS_ANSI == 0 {
                // No MTTS ANSI bit. Many clients still render ANSI without
                // negotiating MTTS, so honour the player's `colors_enabled`
                // override before refusing.
                if !session.colors_enabled {
                    return false;
                }
            }

            // Must already be in a text-editor OLC mode.
            let kind = match session.olc_mode.as_deref() {
                Some(m) => match EditorKind::from_olc_mode(m) {
                    Some(k) => k,
                    None => return false,
                },
                None => return false,
            };

            let seed = session.olc_buffer.join("\n");
            let mut editor = EditorSession::new(
                kind,
                &seed,
                session.telnet_state.window_width,
                session.telnet_state.window_height,
            );
            // DG bodies get syntax colour when the player has colours on
            // and the client negotiated an ANSI-capable terminal.
            let ansi_ok = (session.telnet_state.mtts_flags & MTTS_ANSI) != 0
                || session.colors_enabled;
            editor.set_colour_enabled(ansi_ok && session.colors_enabled);
            let mut initial = editor.take_output();
            // Enable X10 mouse tracking — left-clicks position the cursor.
            // Disabled symmetrically in the dispatcher's exit branches.
            let mut payload = b"\x1b[?1000h".to_vec();
            payload.append(&mut initial);
            session.modern_editor = Some(editor);

            if let Some(raw) = session.raw_sender.as_ref() {
                let _ = raw.send(payload);
            }
            true
        },
    );
}
