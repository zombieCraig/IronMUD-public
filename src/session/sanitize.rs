//! Strip control characters from player-controlled text before it
//! reaches commands or other players' screens.
//!
//! The threat: any player line that ends up echoed to other players
//! (`say`, `tell`, `shout`, `emote`, board posts, room/item descriptions
//! authored via OLC) is a vector for ANSI/MXP/MSP escape injection,
//! screen-clearing DoS (`\x1b[2J\x1b[H`), and CRLF protocol breaks that
//! also corrupt log lines. Sanitizing once at the input boundary —
//! the dispatcher's `InputEvent` extraction point — neutralizes the
//! attack across every command and OLC path without touching any
//! server-generated output (color codes, prompts, room renders).

/// Strip C0 controls (0x00-0x1F), DEL (0x7F), and the C1 range (0x80-0x9F)
/// from a line of player-controlled text. Keeps tab as a printable
/// whitespace; CR / LF are not present at this point because they are
/// line terminators consumed by the read loop.
pub fn sanitize_player_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\t').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_csi_sequences() {
        // Classic screen-clear-and-home payload.
        let attack = "hello\x1b[2J\x1b[Hworld";
        assert_eq!(sanitize_player_text(attack), "hello[2J[Hworld");
    }

    #[test]
    fn strips_c0_controls_except_tab() {
        let mut input = String::new();
        for byte in 0u8..=0x1F {
            input.push(byte as char);
        }
        input.push_str("ok");
        // Tab survives, everything else (incl. NUL, BEL, BS) is removed.
        assert_eq!(sanitize_player_text(&input), "\tok");
    }

    #[test]
    fn strips_del_and_c1_range() {
        let mut input = String::from("a");
        input.push('\u{7F}'); // DEL
        for cp in 0x80u32..=0x9F {
            input.push(char::from_u32(cp).unwrap());
        }
        input.push('z');
        assert_eq!(sanitize_player_text(&input), "az");
    }

    #[test]
    fn preserves_printable_ascii_and_unicode() {
        let s = "Hello, world! 1234 ~`@#$%^&*() 日本語 🦀";
        assert_eq!(sanitize_player_text(s), s);
    }

    #[test]
    fn strips_crlf_if_somehow_present() {
        // The read loop normally consumes line terminators, but any that
        // survive (e.g. a stray \r in a paste) must not reach broadcasts.
        assert_eq!(sanitize_player_text("a\rb\nc"), "abc");
    }
}
