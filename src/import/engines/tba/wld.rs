//! tbaMUD `.wld` extension: T-trailer extraction.
//!
//! Rooms are parsed by the shared CircleMUD `wld` parser (the room-flag line
//! is tolerant of extra tokens). What the circle parser doesn't know about
//! is tbaMUD's `T <vnum>` lines that appear *between* room blocks (after the
//! `S` terminator of one room, before the `#` of the next).
//!
//! `extract_trigger_attachments` does a second pass over the source text and
//! associates each `T` line with the most recently-seen `#vnum`. The caller
//! merges the per-vnum lists onto each parsed room.

use std::collections::HashMap;

/// Walks a `.wld` source and records `T <vnum>` lines bucketed by the
/// preceding room vnum. Subsequent calls accumulate into the same map so
/// a multi-file world tree can share one HashMap.
pub fn extract_trigger_attachments(text: &str, out: &mut HashMap<i32, Vec<i32>>) {
    let mut current_vnum: Option<i32> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('$') {
            current_vnum = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            current_vnum = rest.split_whitespace().next().and_then(|s| s.parse::<i32>().ok());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("T ") {
            if let (Some(vnum), Some(trig)) = (
                current_vnum,
                rest.split_whitespace().next().and_then(|s| s.parse::<i32>().ok()),
            ) {
                out.entry(vnum).or_default().push(trig);
            }
            continue;
        }
        if trimmed == "T" {
            // Bare T line — skip; tbaMUD always has the trigger vnum on the
            // same line.
            continue;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_trigger_after_s() {
        let body = "\
#100
Test Room~
A test room.
~
0 0 0
S
T 555
T 556
#101
Other Room~
Another room.
~
0 0 0
S
$
";
        let mut out = HashMap::new();
        extract_trigger_attachments(body, &mut out);
        assert_eq!(out.get(&100), Some(&vec![555, 556]));
        assert!(out.get(&101).is_none());
    }
}
