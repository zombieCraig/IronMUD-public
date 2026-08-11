//! Character-safe string surgery.
//!
//! Rust strings index by byte, and every count a MUD cares about — how many
//! characters a player typed, how wide a column is, how long a preview should
//! be — is a count of characters. Mixing the two is not a rounding error: a
//! byte index that lands inside a multi-byte character panics on the slice,
//! and the panic takes the connection down with it. Builder prose is full of
//! curly apostrophes and em dashes, so "it is all ASCII in practice" has never
//! been true here.
//!
//! These are the four operations the codebase kept open-coding. Reach for them
//! rather than `&s[..n]`.

/// Byte offset of the `n`th character, or the string's length when it is
/// shorter than that.
pub fn char_index_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

/// The first `max_chars` characters. Counted in characters, cut on a
/// character boundary.
pub fn truncate_chars(s: &str, max_chars: usize) -> &str {
    &s[..char_index_to_byte(s, max_chars)]
}

/// A short display form: at most `max_chars` characters, with an ellipsis
/// standing in for what was cut.
///
/// The ellipsis is inside the budget rather than added to it, so the result
/// never exceeds `max_chars` and a column built on that number stays aligned.
pub fn preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", truncate_chars(s, keep))
}

/// The longest prefix that fits in `max_bytes`, cut on a character boundary.
///
/// For storage and wire limits, which are counted in bytes: a database column
/// or a protocol field does not care how many characters made those bytes.
pub fn truncate_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of these panicked somewhere in the codebase before this module
    /// existed. The character that does it is whatever a builder's editor
    /// turns a straight quote into.
    const CURLY: &str = "a sow\u{2019}s piglet snuffles here.";

    #[test]
    fn truncation_counts_characters_and_cuts_between_them() {
        assert_eq!(truncate_chars(CURLY, 6), "a sow\u{2019}");
        assert_eq!(truncate_chars(CURLY, 5), "a sow");
        // A cut that would land inside the apostrophe takes the whole of it
        // or none of it, never half.
        assert_eq!(truncate_chars("\u{2019}", 0), "");
        assert_eq!(truncate_chars("\u{2019}", 1), "\u{2019}");
        // Asking for more than there is returns what there is.
        assert_eq!(truncate_chars(CURLY, 999), CURLY);
    }

    #[test]
    fn a_preview_stays_inside_its_budget() {
        assert_eq!(preview("short", 10), "short");
        let p = preview(CURLY, 10);
        assert_eq!(p.chars().count(), 10);
        assert!(p.ends_with("..."));
        // Degenerate budgets do not panic and do not overrun.
        assert_eq!(preview(CURLY, 3), "...");
        assert_eq!(preview(CURLY, 0), "...");
    }

    #[test]
    fn a_byte_cap_lands_on_a_boundary() {
        // "…" is three bytes: a cap of 1 or 2 must drop it rather than split
        // it, and a cap of 3 keeps it whole.
        assert_eq!(truncate_bytes("\u{2026}", 1), "");
        assert_eq!(truncate_bytes("\u{2026}", 2), "");
        assert_eq!(truncate_bytes("\u{2026}", 3), "\u{2026}");
        assert_eq!(truncate_bytes("ab\u{2026}", 3), "ab");
        assert_eq!(truncate_bytes("abc", 10), "abc");
    }

    #[test]
    fn char_offsets_are_byte_offsets_only_for_ascii() {
        assert_eq!(char_index_to_byte("abc", 2), 2);
        assert_eq!(char_index_to_byte(CURLY, 6), 8, "the apostrophe is three bytes");
        assert_eq!(char_index_to_byte("abc", 99), 3);
    }
}
