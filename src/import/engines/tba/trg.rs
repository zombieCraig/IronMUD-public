//! tbaMUD `.trg` (DG Scripts trigger) parser.
//!
//! Captures vnum, name, attach type, flags, arglist, and body. The body is
//! the source for IronMUD's runtime DG interpreter (`src/script/dg/`); the
//! body text is not translated at import time.
//!
//! Record format:
//! ```text
//! #VNUM
//! Trigger Name~
//! ATTACH_TYPE TRIG_FLAGS NUMERIC_ARG
//! arglist (single line ending in ~, may be empty)
//! body (multi-line; ~ on its own line terminates)
//! ```
//!
//! NOTE on body termination: DG Script bodies legitimately contain `~`
//! characters in comments and string-comparison operators, so the standard
//! CircleMUD "~ anywhere in line" rule (used by
//! [`crate::import::engines::circle::parser::LineParser::read_string`]) is
//! too eager. Instead, we treat a `~` on its own line (after optional
//! leading whitespace) as the body terminator, and fall back to the next
//! record-boundary line (`#<vnum>` or `$` at start-of-line) if no lone-`~`
//! is found. This matches tbaMUD's parser behavior on every stock trigger
//! while staying robust against `~` inside body lines.

use anyhow::{Context, Result};
use std::path::Path;

use crate::import::IrDgTrigger;
use crate::import::SourceLoc;

pub fn parse_file(path: &Path) -> Result<Vec<IrDgTrigger>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrDgTrigger>> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().collect();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < lines.len() {
        let (line_no, raw) = lines[i];
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if trimmed.starts_with('$') {
            break;
        }
        let Some(rest) = trimmed.strip_prefix('#') else {
            // Stray content between records — keep scanning until the next
            // boundary. tbaMUD's parser would error here, but we want to
            // be tolerant: malformed bodies shouldn't drop the whole file.
            i += 1;
            continue;
        };
        let Some(vnum) = rest.split_whitespace().next().and_then(|s| s.parse::<i32>().ok()) else {
            i += 1;
            continue;
        };
        i += 1;
        let source = SourceLoc::file(path.to_path_buf())
            .with_line((line_no + 1) as u32)
            .with_room(vnum);

        // Name: tilde-terminated, single-line in stock tbaMUD.
        let mut name = String::new();
        while i < lines.len() {
            let l = lines[i].1;
            if let Some(idx) = l.find('~') {
                let head = &l[..idx];
                if !name.is_empty() {
                    name.push('\n');
                }
                name.push_str(head);
                i += 1;
                break;
            }
            if !name.is_empty() {
                name.push('\n');
            }
            name.push_str(l);
            i += 1;
        }
        let name = name.trim().to_string();

        // Header line: attach_type flags numeric_arg.
        if i >= lines.len() {
            break;
        }
        let header = lines[i].1.trim().to_string();
        i += 1;
        let mut t = header.split_whitespace();
        let attach_type_raw: i32 = t.next().and_then(|s| s.parse().ok()).unwrap_or(-1);
        let trigger_flags = t.next().unwrap_or("0").to_string();
        let numeric_arg: i32 = t.next().and_then(|s| s.parse().ok()).unwrap_or(100);

        // Arglist: single line, `~`-terminated. Stock tbaMUD format.
        let mut arglist = String::new();
        if i < lines.len() {
            let l = lines[i].1;
            if let Some(idx) = l.find('~') {
                arglist.push_str(&l[..idx]);
                i += 1;
            } else {
                // Malformed — treat the line as the arglist body and move on.
                arglist.push_str(l);
                i += 1;
            }
        }

        // Body: lines until a `~` on its own line (optionally indented),
        // OR until the next record boundary (`#<vnum>` or `$`). The first
        // condition matches tbaMUD's terminator; the second is the fallback
        // for records missing the closing tilde.
        let mut body_lines: Vec<&str> = Vec::new();
        while i < lines.len() {
            let raw = lines[i].1;
            let l = raw.trim();
            // Lone-tilde terminator (the standard tbaMUD body close).
            if l == "~" {
                i += 1;
                break;
            }
            // Record-boundary fallback.
            if l.starts_with('$') {
                break;
            }
            if l.starts_with('#')
                && l.trim_start_matches('#')
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<i32>().ok())
                    .is_some()
            {
                break;
            }
            body_lines.push(raw);
            i += 1;
        }
        let body = body_lines.join("\n");

        out.push(IrDgTrigger {
            vnum,
            name,
            attach_type_raw,
            trigger_flags,
            numeric_arg,
            arglist,
            body,
            source,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_minimal_trigger() {
        let body = "\
#3000
Mage Guildguard - 3024~
0 q 100
~
* Check the direction.
if %direction% == south
  return 0
end
~
#3001
Cleric Guildguard - 3025~
0 q 100
~
~
$
";
        let ts = parse_str(body, &PathBuf::from("test.trg")).unwrap();
        assert_eq!(ts.len(), 2);
        assert_eq!(ts[0].vnum, 3000);
        assert_eq!(ts[0].name, "Mage Guildguard - 3024");
        assert_eq!(ts[0].attach_type_raw, 0);
        assert_eq!(ts[0].trigger_flags, "q");
        assert_eq!(ts[0].numeric_arg, 100);
        assert!(ts[0].body.contains("if %direction% == south"));
        assert!(ts[0].body.contains("return 0"));
        assert_eq!(ts[1].vnum, 3001);
        assert_eq!(ts[1].body, "");
    }

    #[test]
    fn handles_tilde_in_body() {
        // DG Scripts bodies legitimately contain ~ in comments and string
        // operators. The body terminator is a `~` on its own line; inline
        // `~`s pass through.
        let body = "\
#100
Reverse Card~
1 c 1
say~
* The ~ anchors comparison to the front of the word.
* ~rd is part of ~card but rd is not.
set arg ~%arg%
~
#101
Other~
1 c 1
foo~
* simple
~
$
";
        let ts = parse_str(body, &PathBuf::from("test.trg")).unwrap();
        assert_eq!(ts.len(), 2);
        assert_eq!(ts[0].vnum, 100);
        assert_eq!(ts[0].arglist, "say");
        assert!(ts[0].body.contains("set arg ~%arg%"));
        assert!(!ts[0].body.contains("#101"));
        assert_eq!(ts[1].vnum, 101);
        assert_eq!(ts[1].arglist, "foo");
        assert_eq!(ts[1].body, "* simple");
    }
}
