//! CircleMUD `.wld` (room) file parser.
//!
//! Reference: `parse_room` and `setup_dir` in `circle-3.1/src/db.c`.
//!
//! File format (one or more rooms followed by `$` EOF marker):
//! ```text
//! #VNUM
//! Room Name~
//! Multi-line description ending with
//! a lone ~ on its own line.
//! ~
//! ZONE_NUM ROOM_FLAGS SECTOR_TYPE
//! D0..D5 blocks (zero or more, in any order)
//! E blocks (zero or more, in any order)
//! S
//! ```
//! `D` block: direction digit (0=N, 1=E, 2=S, 3=W, 4=U, 5=D), tilde-string
//! general desc, tilde-string keyword, then `EXIT_INFO KEY_VNUM TO_VNUM`.
//! `E` block: tilde-string keywords, tilde-string description.

use anyhow::{Context, Result};
use std::path::Path;

use super::flags::parse_bitvector;
use super::parser::LineParser;
use crate::import::{IrExit, IrExtraDesc, IrRoom};

/// Direction names matching CircleMUD's NORTH..DOWN constants.
const DIRECTIONS: &[&str] = &["north", "east", "south", "west", "up", "down"];

pub fn parse_file(path: &Path) -> Result<Vec<IrRoom>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrRoom>> {
    let mut p = WldParser {
        inner: LineParser::new(text, path),
    };
    let mut rooms = Vec::new();
    loop {
        p.inner.skip_blank();
        let Some(line) = p.inner.peek_line() else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            p.inner.consume_line();
            continue;
        }
        if trimmed.starts_with('$') {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let vnum: i32 = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| p.inner.err("expected vnum after '#'"))?
                .parse()
                .map_err(|_| p.inner.err("non-numeric vnum after '#'"))?;
            p.inner.consume_line();
            let room = p.parse_room(vnum)?;
            rooms.push(room);
        } else {
            return Err(p.inner.err(&format!("expected '#vnum' or '$', got: {trimmed:?}")));
        }
    }
    Ok(rooms)
}

struct WldParser<'a> {
    inner: LineParser<'a>,
}

impl<'a> WldParser<'a> {
    fn parse_room(&mut self, vnum: i32) -> Result<IrRoom> {
        let source = self.inner.loc().with_room(vnum);
        let name = self.inner.read_string().with_context(|| format!("room #{vnum}: name"))?;
        let description = self
            .inner.read_string()
            .with_context(|| format!("room #{vnum}: description"))?;

        // Flag line: ZONE_NUM ROOM_FLAGS SECTOR_TYPE
        let flag_line = self
            .inner.consume_line()
            .ok_or_else(|| self.inner.err(&format!("room #{vnum}: expected flag line, got EOF")))?
            .trim()
            .to_string();
        let mut tokens = flag_line.split_whitespace();
        let _zone_num = tokens.next();
        let flag_token = tokens
            .next()
            .ok_or_else(|| self.inner.err(&format!("room #{vnum}: missing room flag bitvector")))?;
        let sector_token = tokens
            .next()
            .ok_or_else(|| self.inner.err(&format!("room #{vnum}: missing sector type")))?;
        let flag_bits = parse_bitvector(flag_token);
        let sector: i32 = sector_token
            .parse()
            .map_err(|_| self.inner.err(&format!("room #{vnum}: non-numeric sector {sector_token}")))?;

        let mut exits = Vec::new();
        let mut extras = Vec::new();

        // Loop over D / E blocks until S terminator.
        loop {
            self.inner.skip_blank();
            let Some(line) = self.inner.peek_line() else {
                return Err(self.inner.err(&format!("room #{vnum}: EOF before 'S' terminator")));
            };
            let trimmed = line.trim();
            if trimmed == "S" {
                self.inner.consume_line();
                break;
            }
            if trimmed.starts_with('D') && trimmed.len() >= 2 {
                let dir_char = trimmed.as_bytes()[1];
                if !dir_char.is_ascii_digit() {
                    return Err(self.inner.err(&format!(
                        "room #{vnum}: expected direction digit after 'D', got {trimmed:?}"
                    )));
                }
                let dir_idx = (dir_char - b'0') as usize;
                if dir_idx >= DIRECTIONS.len() {
                    return Err(self.inner.err(&format!("room #{vnum}: direction {dir_idx} out of range (0..=5)")));
                }
                self.inner.consume_line();
                let direction = DIRECTIONS[dir_idx].to_string();
                let general = self
                    .inner.read_string()
                    .with_context(|| format!("room #{vnum}: D{dir_idx} general desc"))?;
                let keyword = self
                    .inner.read_string()
                    .with_context(|| format!("room #{vnum}: D{dir_idx} keyword"))?;
                let stats_line = self
                    .inner.consume_line()
                    .ok_or_else(|| self.inner.err(&format!("room #{vnum}: D{dir_idx} EOF before stats line")))?
                    .trim()
                    .to_string();
                let mut t = stats_line.split_whitespace();
                let exit_info: u32 = t
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| self.inner.err(&format!("room #{vnum}: D{dir_idx} bad exit_info")))?;
                let key: i32 = t
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| self.inner.err(&format!("room #{vnum}: D{dir_idx} bad key vnum")))?;
                let to_room: i32 = t
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| self.inner.err(&format!("room #{vnum}: D{dir_idx} bad to_room vnum")))?;
                let general_opt = if general.trim().is_empty() { None } else { Some(general) };
                let keyword_opt = if keyword.trim().is_empty() { None } else { Some(keyword) };
                let key_opt = if key < 0 { None } else { Some(key) };
                exits.push(IrExit {
                    direction,
                    general_description: general_opt,
                    keyword: keyword_opt,
                    door_flags: exit_info,
                    unknown_door_flags: Vec::new(),
                    key_vnum: key_opt,
                    to_room_vnum: to_room,
                });
                continue;
            }
            if trimmed == "E" {
                self.inner.consume_line();
                let kw_line = self
                    .inner.read_string()
                    .with_context(|| format!("room #{vnum}: E keyword line"))?;
                let desc = self
                    .inner.read_string()
                    .with_context(|| format!("room #{vnum}: E description"))?;
                let keywords: Vec<String> = kw_line.split_whitespace().map(str::to_string).collect();
                extras.push(IrExtraDesc {
                    keywords,
                    description: desc,
                });
                continue;
            }
            return Err(self.inner.err(&format!(
                "room #{vnum}: expected 'D[0-5]', 'E', or 'S' — got {trimmed:?}"
            )));
        }

        Ok(IrRoom {
            vnum,
            name,
            description,
            sector,
            flag_bits,
            unknown_flag_names: Vec::new(),
            exits,
            extras,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> Vec<IrRoom> {
        parse_str(s, &PathBuf::from("test.wld")).expect("parse")
    }

    #[test]
    fn parses_minimal_room() {
        let body = "\
#100
Test Room~
A simple test room
spans multiple lines.
~
0 0 0
S
$
";
        let rooms = p(body);
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].vnum, 100);
        assert_eq!(rooms[0].name, "Test Room");
        assert!(rooms[0].description.contains("multiple lines"));
        assert_eq!(rooms[0].sector, 0);
    }

    #[test]
    fn parses_room_with_exit_and_extra() {
        let body = "\
#101
Hall~
A long hall.
~
0 d 1
D1
You see east.
~
east door~
3 -1 102
E
hall corridor~
A polished marble corridor.
~
S
$
";
        let rooms = p(body);
        let r = &rooms[0];
        assert_eq!(r.exits.len(), 1);
        let e = &r.exits[0];
        assert_eq!(e.direction, "east");
        assert_eq!(e.to_room_vnum, 102);
        assert_eq!(e.door_flags, 3); // ISDOOR | CLOSED
        assert_eq!(e.key_vnum, None);
        assert_eq!(r.extras.len(), 1);
        assert_eq!(r.extras[0].keywords, vec!["hall", "corridor"]);
        assert_eq!(r.flag_bits, 1 << 3); // 'd' = bit 3 = INDOORS
    }
}
