//! CircleMUD `.zon` (zone) file parser.
//!
//! Reference: `load_zones` in `circle-3.1/src/db.c`.
//!
//! Format:
//! ```text
//! #ZONE_VNUM
//! Zone Name~
//! BOT TOP LIFESPAN RESET_MODE
//! <reset commands>
//! S
//! ```
//! Reset commands look like `M 0 6100 2 6116` (M/O/G/E/P/D/R + arguments).
//! Lines beginning with `*` are comments. Trailing per-line comments
//! (after the required tokens) are also ignored. Recognised resets are
//! tokenised into structured [`IrReset`] values so the mapping layer can
//! translate them into IronMUD spawn points + door overrides; anything
//! that fails to parse falls into [`DeferredItem`] for warning-only surfacing.

use anyhow::{Context, Result, anyhow};
use std::path::Path;

use crate::import::{DeferredItem, IrReset, IrResetKind, SourceLoc};

#[derive(Debug, Clone)]
pub struct ZonHeader {
    pub vnum: i32,
    pub name: String,
    pub bot: i32,
    pub top: i32,
    pub lifespan: i32,
    pub reset_mode: i32,
}

#[derive(Debug, Clone)]
pub struct ParsedZon {
    pub header: ZonHeader,
    /// Structured M/O/G/E/P/D/R commands the parser recognised.
    pub resets: Vec<IrReset>,
    /// Lines that started with a reset letter but failed to parse out
    /// the required argument count. Surface as warnings; don't try to
    /// guess at intent.
    pub deferred: Vec<DeferredItem>,
}

pub fn parse_file(path: &Path) -> Result<ParsedZon> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<ParsedZon> {
    let mut lines = text.lines().enumerate();
    // First non-blank, non-comment line: #VNUM
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

    // Name: tilde-terminated, may span multiple lines (rare, but allowed).
    let mut name = String::new();
    loop {
        let (_, line) = lines
            .next()
            .ok_or_else(|| anyhow!("{}: EOF while reading zone name", path.display()))?;
        if let Some(idx) = line.find('~') {
            let head = &line[..idx];
            if !head.is_empty() {
                if !name.is_empty() {
                    name.push('\n');
                }
                name.push_str(head);
            }
            break;
        }
        if !name.is_empty() {
            name.push('\n');
        }
        name.push_str(line);
    }

    // Header line: BOT TOP LIFESPAN RESET_MODE
    let (header_line_no, header_line) =
        next_significant(&mut lines).ok_or_else(|| anyhow!("{}: missing zone header line", path.display()))?;
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
        if !"MOGEPDR".contains(cmd) {
            continue;
        }
        let source = SourceLoc::file(path.to_path_buf())
            .with_line((idx + 1) as u32)
            .with_zone(vnum);
        match parse_reset_line(cmd, trimmed, source.clone()) {
            Some(r) => resets.push(r),
            None => deferred.push(DeferredItem {
                category: "zone_reset".into(),
                summary: format!("malformed: {trimmed}"),
                source,
            }),
        }
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

fn parse_reset_line(cmd: char, line: &str, source: SourceLoc) -> Option<IrReset> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let nums: Vec<i32> = parts.iter().skip(1).filter_map(|t| t.parse::<i32>().ok()).collect();
    let if_flag_int = *nums.first()?;
    let if_flag = if_flag_int != 0;
    let kind = match cmd {
        // M IF MOB MAX ROOM
        'M' => IrResetKind::LoadMob {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            room_vnum: *nums.get(3)?,
        },
        // O IF OBJ MAX ROOM
        'O' => IrResetKind::LoadObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            room_vnum: *nums.get(3)?,
        },
        // G IF OBJ MAX
        'G' => IrResetKind::GiveObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
        },
        // E IF OBJ MAX WEAR_LOC
        'E' => IrResetKind::EquipObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            wear_loc: *nums.get(3)?,
        },
        // P IF OBJ MAX CONTAINER
        'P' => IrResetKind::PutObj {
            vnum: *nums.get(1)?,
            max: *nums.get(2)?,
            container_vnum: *nums.get(3)?,
        },
        // D IF ROOM DIR STATE
        'D' => IrResetKind::SetDoor {
            room_vnum: *nums.get(1)?,
            dir: *nums.get(2)?,
            state: *nums.get(3)?,
        },
        // R IF ROOM OBJ
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
    fn parses_zon_header_and_resets() {
        let body = "\
#61
Haon-Dor~
6100 6199 30 2
* Mobiles
M 0 6100 2 6116         Vicious Warg
G 1 6108 10                     10,000 Gold
S
$
";
        let parsed = parse_str(body, &PathBuf::from("test.zon")).unwrap();
        assert_eq!(parsed.header.vnum, 61);
        assert_eq!(parsed.header.name, "Haon-Dor");
        assert_eq!(parsed.header.bot, 6100);
        assert_eq!(parsed.header.top, 6199);
        assert_eq!(parsed.resets.len(), 2);
        match &parsed.resets[0].kind {
            IrResetKind::LoadMob { vnum, max, room_vnum } => {
                assert_eq!(*vnum, 6100);
                assert_eq!(*max, 2);
                assert_eq!(*room_vnum, 6116);
            }
            other => panic!("expected LoadMob, got {other:?}"),
        }
        assert!(parsed.resets[1].if_flag);
        match &parsed.resets[1].kind {
            IrResetKind::GiveObj { vnum, max } => {
                assert_eq!(*vnum, 6108);
                assert_eq!(*max, 10);
            }
            other => panic!("expected GiveObj, got {other:?}"),
        }
    }

    #[test]
    fn parses_door_and_remove_resets() {
        let body = "\
#1
Test~
1 99 30 2
D 0 5 1 1
R 1 5 100
S
$
";
        let parsed = parse_str(body, &PathBuf::from("test.zon")).unwrap();
        assert_eq!(parsed.resets.len(), 2);
        match &parsed.resets[0].kind {
            IrResetKind::SetDoor { room_vnum, dir, state } => {
                assert_eq!(*room_vnum, 5);
                assert_eq!(*dir, 1);
                assert_eq!(*state, 1);
            }
            other => panic!("expected SetDoor, got {other:?}"),
        }
        match &parsed.resets[1].kind {
            IrResetKind::RemoveObj { room_vnum, vnum } => {
                assert_eq!(*room_vnum, 5);
                assert_eq!(*vnum, 100);
            }
            other => panic!("expected RemoveObj, got {other:?}"),
        }
    }
}
