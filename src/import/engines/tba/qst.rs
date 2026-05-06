//! tbaMUD `.qst` (quest) header parser.
//!
//! IronMUD has no quest system; this parser only captures the header so the
//! mapping layer can emit one Warn per quest. Bodies (stat lines, reward
//! lines) are skipped entirely.
//!
//! Record format:
//! ```text
//! #VNUM
//! Quest Name~
//! keyword(s)~
//! accept message (multi-line, ~-terminated)
//! complete message (multi-line, ~-terminated)
//! quit message (multi-line, ~-terminated)
//! <stat line ints>
//! <reward line ints>
//! <reward line ints>
//! S
//! ```

use anyhow::{Context, Result};
use std::path::Path;

use crate::import::IrQuest;
use crate::import::engines::circle::parser::LineParser;

pub fn parse_file(path: &Path) -> Result<Vec<IrQuest>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrQuest>> {
    let mut p = LineParser::new(text, path);
    let mut out = Vec::new();
    loop {
        p.skip_blank();
        let Some(line) = p.peek_line() else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            p.consume_line();
            continue;
        }
        if trimmed.starts_with('$') {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let vnum: i32 = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| p.err("expected vnum after '#'"))?
                .parse()
                .map_err(|_| p.err("non-numeric vnum after '#'"))?;
            p.consume_line();
            let source = p.loc().with_room(vnum);
            let name = p
                .read_string()
                .with_context(|| format!("qst #{vnum}: name"))?
                .trim()
                .to_string();
            let keywords = p
                .read_string()
                .with_context(|| format!("qst #{vnum}: keywords"))?
                .trim()
                .to_string();
            // Three description messages (accept, complete, quit) — discard.
            for label in ["accept", "complete", "quit"] {
                let _ = p
                    .read_string()
                    .with_context(|| format!("qst #{vnum}: {label} message"))?;
            }
            // Skip remaining stat / reward lines until 'S' terminator (or
            // start of next record / EOF).
            loop {
                p.skip_blank();
                let Some(line) = p.peek_line() else { break };
                let t = line.trim();
                if t == "S" {
                    p.consume_line();
                    break;
                }
                if t.starts_with('#') || t.starts_with('$') {
                    break;
                }
                p.consume_line();
            }
            out.push(IrQuest {
                vnum,
                name,
                keywords,
                source,
            });
        } else {
            return Err(p.err(&format!("expected '#vnum' or '$', got: {trimmed:?}")));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_minimal_quest() {
        let body = "\
#100
Kill the Mice!~
mice~
   I really need help killing these mice.
~
   Well done!  Quest complete.
~
You have abandoned the quest.
~
3 179 0 194 -1 -1 -1
0 0 1 34 60 -1 3
10 0 65535
S
$~
";
        let qs = parse_str(body, &PathBuf::from("100.qst")).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].vnum, 100);
        assert_eq!(qs[0].name, "Kill the Mice!");
        assert_eq!(qs[0].keywords, "mice");
    }
}
