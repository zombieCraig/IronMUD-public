//! Parser for CircleMUD / tbaMUD `lib/misc/socials.new` (and the older
//! `socials` format).
//!
//! Record format (tbamud extended, 14 lines):
//!
//! ```text
//! ~<name> <abbrev> <hide> <min_vict_pos> <min_char_pos> <min_level>
//! char_no_arg          (shown to actor with no target)
//! others_no_arg        (broadcast to room, no target)
//! char_found           (actor → other)
//! others_found         (bystanders → actor + other)
//! vict_found           (the targeted character)
//! not_found            (actor when target not in room)
//! char_auto            (actor → self)
//! others_auto          (bystanders → actor + self)
//! body_char_found      ($t = body part)
//! body_others_found
//! body_vict_found
//! object_char_found    ($p = object short-desc)
//! object_others_found
//! ```
//!
//! `#` on a line stands for "no message" — stored as `None`. `~` in
//! column 1 of a header line starts the next record; a single `$` line
//! terminates the file. Older Circle `socials` shipped only 8 messages
//! (no body/object trios); the parser falls back gracefully.

use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

use crate::types::{SocialAction, SocialPosition};

/// Parse a `socials.new` (or legacy `socials`) file. Returns one
/// [`SocialAction`] per record. Malformed records emit a warning string
/// and are skipped — the parser favours partial progress over hard
/// failures, matching the rest of the CircleMUD importer.
pub fn parse_file(path: &Path) -> Result<(Vec<SocialAction>, Vec<String>)> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read socials file {}", path.display()))?;
    Ok(parse_str(&raw))
}

/// Parse from an in-memory string. Separated from `parse_file` so unit
/// tests can drive the parser without touching disk.
pub fn parse_str(raw: &str) -> (Vec<SocialAction>, Vec<String>) {
    let mut socials = Vec::new();
    let mut warnings = Vec::new();
    // Split records on lines starting with `~` or `$` (EOF). Iterate
    // line-by-line so we can carry the current record's lines.
    let mut current_header: Option<String> = None;
    let mut body: Vec<String> = Vec::new();
    let mut line_no = 0usize;
    for raw_line in raw.lines() {
        line_no += 1;
        let line = raw_line;
        // EOF is a `$` ALONE on its own line. `$n`, `$N`, etc. are
        // pronoun tokens inside message bodies — never start of EOF.
        if line.trim_end() == "$" {
            // EOF marker. Flush the in-flight record and stop.
            if let Some(h) = current_header.take() {
                match build_record(&h, &body) {
                    Ok(s) => socials.push(s),
                    Err(e) => warnings.push(format!("line {}: {}", line_no, e)),
                }
            }
            break;
        }
        if let Some(rest) = line.strip_prefix('~') {
            // Flush prior record.
            if let Some(h) = current_header.take() {
                match build_record(&h, &body) {
                    Ok(s) => socials.push(s),
                    Err(e) => warnings.push(format!("line {}: {}", line_no, e)),
                }
                body.clear();
            }
            current_header = Some(rest.to_string());
            continue;
        }
        // Inside a record. A blank line terminates the current body —
        // tbamud writes a single blank separator between records.
        if line.is_empty() {
            if let Some(h) = current_header.take() {
                match build_record(&h, &body) {
                    Ok(s) => socials.push(s),
                    Err(e) => warnings.push(format!("line {}: {}", line_no, e)),
                }
                body.clear();
            }
            continue;
        }
        if current_header.is_some() {
            body.push(line.to_string());
        }
    }
    // Trailing record without an EOF marker.
    if let Some(h) = current_header.take() {
        match build_record(&h, &body) {
            Ok(s) => socials.push(s),
            Err(e) => warnings.push(format!("(end of file): {}", e)),
        }
    }
    (socials, warnings)
}

/// Build a `SocialAction` from one header line and its body lines.
/// Header layout: `<name> <abbrev> <hide> <min_vict_pos> <min_char_pos> <min_level>`.
fn build_record(header: &str, body: &[String]) -> Result<SocialAction> {
    let mut parts = header.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow!("missing name in social header `{}`", header))?
        .to_string();
    let abbrev_raw = parts.next();
    let hide_raw = parts.next();
    let min_vict_raw = parts.next();
    let min_char_raw = parts.next();
    let min_level_raw = parts.next();

    let abbrev = abbrev_raw
        .filter(|s| !s.is_empty() && *s != "0")
        .map(|s| s.to_string());
    let hide = hide_raw
        .and_then(|s| s.parse::<i32>().ok())
        .map(|n| n != 0)
        .unwrap_or(false);
    let min_victim_position = min_vict_raw
        .and_then(|s| s.parse::<u8>().ok())
        .map(SocialPosition::from_circle)
        .unwrap_or_default();
    let min_char_position = min_char_raw
        .and_then(|s| s.parse::<u8>().ok())
        .map(SocialPosition::from_circle)
        .unwrap_or_default();
    let min_level = min_level_raw
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(0);

    let g = |idx: usize| -> Option<String> {
        body.get(idx)
            .map(|s| s.trim_end_matches('\r'))
            .and_then(|s| {
                if s.trim() == "#" || s.trim().is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            })
    };

    Ok(SocialAction {
        name,
        abbrev,
        hide,
        min_victim_position,
        min_char_position,
        min_level,
        char_no_arg: g(0),
        others_no_arg: g(1),
        char_found: g(2),
        others_found: g(3),
        vict_found: g(4),
        not_found: g(5),
        char_auto: g(6),
        others_auto: g(7),
        body_char_found: g(8),
        body_others_found: g(9),
        body_vict_found: g(10),
        object_char_found: g(11),
        object_others_found: g(12),
        tags: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
~accuse accuse 0 5 0 0
Accuse who??
$n accuses everyone.
You look accusingly at $M.
$n looks accusingly at $N.
$n looks accusingly at you.
Accuse somebody who's not even there??
You accuse yourself.
$n seems to have a bad conscience.
You look accusingly at $S $t.
$n looks accusingly at $N's $t.
$n looks at your $t accusingly.
You look accusingly at $p.
$n looks accusingly at $p.

~wave wave 0 5 8 0
Wave at who?
#
You wave at $M.
$n waves at $N.
$n waves at you.
They aren't here.
#
#
#
#
#
#
#

$
";

    #[test]
    fn parses_two_records() {
        let (socials, warnings) = parse_str(FIXTURE);
        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        assert_eq!(socials.len(), 2);
        let accuse = &socials[0];
        assert_eq!(accuse.name, "accuse");
        assert_eq!(accuse.abbrev.as_deref(), Some("accuse"));
        assert!(!accuse.hide);
        assert_eq!(accuse.char_no_arg.as_deref(), Some("Accuse who??"));
        assert_eq!(accuse.body_others_found.as_deref(), Some("$n looks accusingly at $N's $t."));
        assert_eq!(accuse.object_char_found.as_deref(), Some("You look accusingly at $p."));

        let wave = &socials[1];
        assert_eq!(wave.name, "wave");
        // `wave` requires standing (min_char_position from circle 8 → Standing).
        assert_eq!(wave.min_char_position, SocialPosition::Standing);
        // others_no_arg was `#` → None.
        assert!(wave.others_no_arg.is_none());
        assert_eq!(wave.char_found.as_deref(), Some("You wave at $M."));
    }

    #[test]
    fn hidden_flag() {
        let raw = "~peek peek 1 5 0 0\nYou peek.\n$n peeks.\n#\n#\n#\n#\n#\n#\n#\n#\n#\n#\n#\n\n$\n";
        let (socials, warnings) = parse_str(raw);
        assert!(warnings.is_empty());
        assert_eq!(socials.len(), 1);
        assert!(socials[0].hide);
    }
}
