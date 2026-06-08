//! tbaMUD `.zon` parser.
//!
//! Header line is longer than stock CircleMUD: `bot top lifespan reset_mode
//! [zone_flags level_min level_max builders ...]` (10 tokens in stock
//! tbaMUD output). The first 4 fields match CircleMUD; the rest are
//! tolerated but dropped.
//!
//! Reset-command alphabet adds **T** (DG Scripts trigger attachment via
//! reset) and **V** (DG variable assignment). Both surface as `DeferredItem`
//! warnings — IronMUD has no equivalent. M/O/G/E/P/D/R commands behave
//! identically to CircleMUD.

use anyhow::{Result, anyhow};
use std::path::Path;

use crate::import::engines::circle::zon::{ParsedZon, ZonHeader};
use crate::import::{DeferredItem, IrReset, IrResetKind, SourceLoc};

pub fn parse_file(path: &Path) -> Result<ParsedZon> {
    let text = std::fs::read_to_string(path).map_err(|e| anyhow!("reading {}: {}", path.display(), e))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<ParsedZon> {
    let mut lines = text.lines().enumerate();

    let (vnum_line_no, vnum_line) =
        next_significant(&mut lines).ok_or_else(|| anyhow!("{}: empty zone file", path.display()))?;
    let vnum: i32 = vnum_line
        .trim()
        .strip_prefix('#')
        .ok_or_else(|| anyhow!("{}:{}: expected '#vnum'", path.display(), vnum_line_no + 1))?
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("{}:{}: bad zone vnum", path.display(), vnum_line_no + 1))?;

    // tbaMUD has TWO `~`-terminated strings in the zone header: builder
    // (line 2) and zone name (line 3). Stock CircleMUD has just zone name.
    // Detect by counting `~` occurrences in the next ~5 lines. Simpler
    // strategy: read the first tilde-string, peek the next non-empty line:
    // if it looks like a header (starts with digit), the first string was
    // the name; otherwise read another tilde-string and use that as name.
    let first_string = read_tilde_string(&mut lines, path)?;
    let mut header_line_no: usize;
    let mut header_line: String;
    let mut name: String;
    let (no, peek) =
        next_significant(&mut lines).ok_or_else(|| anyhow!("{}: missing zone header line", path.display()))?;
    if peek
        .trim()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<i32>().ok())
        .is_some()
    {
        // Single-string layout (CircleMUD stock): first string was name,
        // peek line is the bot/top/lifespan/reset_mode header.
        name = first_string;
        header_line_no = no;
        header_line = peek.to_string();
    } else {
        // tbaMUD layout: first string was builder, peek line should be a
        // second tilde-string (zone name). If peek already contains '~' it's
        // a single-line tilde-string; otherwise it spans more lines.
        let second_string = if let Some(idx) = peek.find('~') {
            peek[..idx].to_string()
        } else {
            // Multi-line: prepend peek to subsequent lines until ~.
            let mut buf = String::from(peek);
            loop {
                let (_, l) = lines
                    .next()
                    .ok_or_else(|| anyhow!("{}: EOF inside zone-name string", path.display()))?;
                if let Some(idx) = l.find('~') {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(&l[..idx]);
                    break;
                }
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(l);
            }
            buf
        };
        name = second_string;
        let (no2, hdr) = next_significant(&mut lines)
            .ok_or_else(|| anyhow!("{}: missing zone header line after name", path.display()))?;
        header_line_no = no2;
        header_line = hdr.to_string();
    }
    let _ = (&mut header_line_no, &mut header_line, &mut name);

    let mut tokens = header_line.split_whitespace();
    let bot: i32 = tokens
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("{}:{}: bad bot vnum", path.display(), header_line_no + 1))?;
    let top: i32 = tokens
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("{}:{}: bad top vnum", path.display(), header_line_no + 1))?;
    let lifespan: i32 = tokens
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("{}:{}: bad lifespan", path.display(), header_line_no + 1))?;
    let reset_mode: i32 = tokens
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("{}:{}: bad reset_mode", path.display(), header_line_no + 1))?;
    // Trailing tokens (zone_flags, level_min, level_max, builders, ...) are
    // dropped — IronMUD has no analog.

    let mut resets: Vec<IrReset> = Vec::new();
    let mut deferred: Vec<DeferredItem> = Vec::new();
    for (idx, line) in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') {
            continue;
        }
        if trimmed == "S" || trimmed.starts_with('$') {
            break;
        }
        let mut chars = trimmed.chars();
        let Some(cmd) = chars.next() else { continue };
        let source = SourceLoc::file(path.to_path_buf())
            .with_line((idx + 1) as u32)
            .with_zone(vnum);
        if "MOGEPDR".contains(cmd) {
            match parse_reset_line(cmd, trimmed, source.clone()) {
                Some(r) => resets.push(r),
                None => deferred.push(DeferredItem {
                    category: "zone_reset".into(),
                    summary: format!("malformed: {trimmed}"),
                    source,
                }),
            }
        } else if cmd == 'T' {
            // Zone-level DG trigger attachment (`T if attach_type trig_vnum
            // [room_vnum]`). IronMUD has no DG Scripts; surface as warn.
            deferred.push(DeferredItem {
                category: "zone_reset_dg_trigger".into(),
                summary: format!("DG trigger attachment: {trimmed}"),
                source,
            });
        } else if cmd == 'V' {
            deferred.push(DeferredItem {
                category: "zone_reset_dg_var".into(),
                summary: format!("DG variable assignment: {trimmed}"),
                source,
            });
        }
        // Other letters silently ignored (matches CircleMUD parser).
    }

    Ok(ParsedZon {
        header: ZonHeader {
            vnum,
            name,
            bot,
            top,
            lifespan,
            reset_mode,
        },
        resets,
        deferred,
    })
}

fn read_tilde_string<'a, I>(lines: &mut I, path: &Path) -> Result<String>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    let mut buf = String::new();
    loop {
        let (_, l) = lines
            .next()
            .ok_or_else(|| anyhow!("{}: EOF inside tilde-string", path.display()))?;
        if let Some(idx) = l.find('~') {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(&l[..idx]);
            return Ok(buf);
        }
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(l);
    }
}

fn parse_reset_line(cmd: char, line: &str, source: SourceLoc) -> Option<IrReset> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let nums: Vec<i32> = parts.iter().skip(1).filter_map(|t| t.parse::<i32>().ok()).collect();
    let if_flag_int = *nums.first()?;
    let if_flag = if_flag_int != 0;
    let kind = match cmd {
        'M' => IrResetKind::LoadMob {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            room_vnum: *nums.get(3)?,
        },
        'O' => IrResetKind::LoadObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            room_vnum: *nums.get(3)?,
        },
        'G' => IrResetKind::GiveObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
        },
        'E' => IrResetKind::EquipObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            wear_loc: *nums.get(3)?,
        },
        'P' => IrResetKind::PutObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            container_vnum: *nums.get(3)?,
        },
        'D' => IrResetKind::SetDoor {
            room_vnum: *nums.get(1)?,
            dir: *nums.get(2)?,
            state: *nums.get(3)?,
        },
        'R' => IrResetKind::RemoveObj {
            room_vnum: *nums.get(1)?,
            vnum: *nums.get(2)?,
        },
        _ => return None,
    };
    Some(IrReset { if_flag, kind, source })
}

fn next_significant<'a, I>(it: &mut I) -> Option<(usize, &'a str)>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    for (idx, line) in it.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('*') {
            continue;
        }
        return Some((idx, line));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_tba_two_string_header() {
        let body = "\
#30
DikuMUD~
Northern Midgaard~
3000 3099 15 2 d 0 0 0 1 33
M 0 3011 1 3000 (the travelling saleswoman)
T 0 0 555 3000
S
$
";
        let parsed = parse_str(body, &PathBuf::from("30.zon")).unwrap();
        assert_eq!(parsed.header.vnum, 30);
        assert_eq!(parsed.header.name, "Northern Midgaard");
        assert_eq!(parsed.header.bot, 3000);
        assert_eq!(parsed.header.top, 3099);
        assert_eq!(parsed.header.lifespan, 15);
        assert_eq!(parsed.resets.len(), 1);
        match &parsed.resets[0].kind {
            IrResetKind::LoadMob { vnum, max, room_vnum } => {
                assert_eq!(*vnum, 3011);
                assert_eq!(*max, 1);
                assert_eq!(*room_vnum, 3000);
            }
            other => panic!("expected LoadMob, got {other:?}"),
        }
        assert_eq!(parsed.deferred.len(), 1);
        assert_eq!(parsed.deferred[0].category, "zone_reset_dg_trigger");
    }

    #[test]
    fn parses_circle_single_string_header() {
        // Stock-style fallback path (1 tilde-string, 4-token header).
        let body = "\
#61
Haon-Dor~
6100 6199 30 2
M 0 6100 2 6116
S
$
";
        let parsed = parse_str(body, &PathBuf::from("61.zon")).unwrap();
        assert_eq!(parsed.header.vnum, 61);
        assert_eq!(parsed.header.name, "Haon-Dor");
        assert_eq!(parsed.header.bot, 6100);
        assert_eq!(parsed.resets.len(), 1);
    }

    #[test]
    fn surfaces_v_command_as_deferred() {
        let body = "\
#1
builder~
Test~
1 99 30 2
V 0 0 0 0 myvar 42
S
$
";
        let parsed = parse_str(body, &PathBuf::from("1.zon")).unwrap();
        assert_eq!(parsed.deferred.len(), 1);
        assert_eq!(parsed.deferred[0].category, "zone_reset_dg_var");
    }
}
