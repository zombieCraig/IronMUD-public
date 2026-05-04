//! CircleMUD `.obj` (object / item prototype) file parser.
//!
//! Reference: `parse_object` in `circle-3.1/src/db.c` and the format
//! documented in `circle-3.1/doc/sources/building.tex` (Objects section).
//!
//! File format (one or more objects followed by `$` EOF marker):
//! ```text
//! #VNUM
//! keywords~
//! short_descr~                      ground appearance: "a long sword"
//! long_descr~                       in-room sentence: "A long sword has been left here."
//! action_descr~                     usually empty (bare ~)
//! TYPE EXTRA_FLAGS WEAR_FLAGS       e.g. "5 an 8193"
//! V0 V1 V2 V3                       four type-specific values
//! WEIGHT COST RENT                  e.g. "8 600 10"
//! [zero or more sub-blocks:]
//!   E
//!     keyword(s)~
//!     description~
//!   A
//!     APPLY_LOCATION MODIFIER       e.g. "17 -10"  (APPLY_AC -10)
//! ```
//! Records terminate when the next non-blank line starts with `#` or `$`.
//! Up to `MAX_OBJ_AFFECT = 6` `A`-blocks are allowed in stock Circle.

use anyhow::{Context, Result};
use std::path::Path;

use super::flags::parse_bitvector;
use super::parser::LineParser;
use crate::import::{IrExtraDesc, IrItem};

pub fn parse_file(path: &Path) -> Result<Vec<IrItem>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrItem>> {
    let mut p = ObjParser {
        inner: LineParser::new(text, path),
    };
    let mut items = Vec::new();
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
            items.push(p.parse_obj(vnum)?);
        } else {
            return Err(p.inner.err(&format!("expected '#vnum' or '$', got: {trimmed:?}")));
        }
    }
    Ok(items)
}

struct ObjParser<'a> {
    inner: LineParser<'a>,
}

impl<'a> ObjParser<'a> {
    fn parse_obj(&mut self, vnum: i32) -> Result<IrItem> {
        // SourceLoc.room_vnum doubles as "the entity's source vnum" for
        // warning context — same convention the mob parser uses.
        let source = self.inner.loc().with_room(vnum);

        let keywords_raw = self
            .inner
            .read_string()
            .with_context(|| format!("obj #{vnum}: keywords"))?;
        let keywords: Vec<String> = keywords_raw.split_whitespace().map(str::to_string).collect();

        let short_descr = self
            .inner
            .read_string()
            .with_context(|| format!("obj #{vnum}: short descr"))?
            .trim()
            .to_string();
        let long_descr = self
            .inner
            .read_string()
            .with_context(|| format!("obj #{vnum}: long descr"))?
            .trim_end()
            .to_string();
        let action_descr = self
            .inner
            .read_string()
            .with_context(|| format!("obj #{vnum}: action descr"))?
            .trim_end()
            .to_string();

        // Numeric line 1: TYPE EXTRA_FLAGS WEAR_FLAGS
        let header = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: expected type/flags line, got EOF")))?
            .trim()
            .to_string();
        let mut t = header.split_whitespace();
        let type_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: missing item_type")))?;
        let extra_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: missing extra_flags")))?;
        let wear_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: missing wear_flags")))?;
        let item_type: i32 = type_token
            .parse()
            .map_err(|_| self.inner.err(&format!("obj #{vnum}: non-numeric item_type {type_token:?}")))?;
        let extra_flag_bits = parse_bitvector(extra_token);
        let wear_flag_bits = parse_bitvector(wear_token);

        // Numeric line 2: V0 V1 V2 V3
        let values_line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: expected values line, got EOF")))?
            .trim()
            .to_string();
        let mut t = values_line.split_whitespace();
        let v0 = parse_i32(&mut t, "v0", vnum, &self.inner)?;
        let v1 = parse_i32(&mut t, "v1", vnum, &self.inner)?;
        let v2 = parse_i32(&mut t, "v2", vnum, &self.inner)?;
        let v3 = parse_i32(&mut t, "v3", vnum, &self.inner)?;

        // Numeric line 3: WEIGHT COST RENT
        let stats_line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: expected weight/cost/rent line, got EOF")))?
            .trim()
            .to_string();
        let mut t = stats_line.split_whitespace();
        let weight = parse_i32(&mut t, "weight", vnum, &self.inner)?;
        let cost = parse_i32(&mut t, "cost", vnum, &self.inner)?;
        let rent = parse_i32(&mut t, "rent", vnum, &self.inner)?;

        // Sub-blocks: zero or more E / A blocks until the next `#` or `$` or EOF.
        let mut extra_descs: Vec<IrExtraDesc> = Vec::new();
        let mut affects: Vec<(i32, i32)> = Vec::new();
        loop {
            self.inner.skip_blank();
            let Some(line) = self.inner.peek_line() else { break };
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.starts_with('$') {
                break;
            }
            if trimmed == "E" {
                self.inner.consume_line();
                let kw_line = self
                    .inner
                    .read_string()
                    .with_context(|| format!("obj #{vnum}: E keyword line"))?;
                let desc = self
                    .inner
                    .read_string()
                    .with_context(|| format!("obj #{vnum}: E description"))?;
                let keywords: Vec<String> = kw_line.split_whitespace().map(str::to_string).collect();
                extra_descs.push(IrExtraDesc {
                    keywords,
                    description: desc,
                });
                continue;
            }
            if trimmed == "A" {
                self.inner.consume_line();
                let pair_line = self
                    .inner
                    .consume_line()
                    .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: A block missing location/modifier")))?
                    .trim()
                    .to_string();
                let mut p = pair_line.split_whitespace();
                let loc = parse_i32(&mut p, "apply_location", vnum, &self.inner)?;
                let modifier = parse_i32(&mut p, "apply_modifier", vnum, &self.inner)?;
                affects.push((loc, modifier));
                continue;
            }
            // Stock Circle's parse_object accepts only E/A here, but some
            // patches add more letters. Surface as a hard error so we don't
            // silently misalign and corrupt subsequent records.
            return Err(self
                .inner
                .err(&format!("obj #{vnum}: expected 'E', 'A', '#vnum', or '$' — got {trimmed:?}")));
        }

        Ok(IrItem {
            vnum,
            keywords,
            short_descr,
            long_descr,
            action_descr,
            item_type,
            extra_flag_bits,
            wear_flag_bits,
            unknown_extra_flags: Vec::new(),
            unknown_wear_flags: Vec::new(),
            values: [v0, v1, v2, v3],
            weight,
            cost,
            rent,
            extra_descs,
            affects,
            source,
        })
    }
}

fn parse_i32(it: &mut std::str::SplitWhitespace<'_>, field: &str, vnum: i32, p: &LineParser<'_>) -> Result<i32> {
    let tok = it
        .next()
        .ok_or_else(|| p.err(&format!("obj #{vnum}: missing {field}")))?;
    tok.parse()
        .map_err(|_| p.err(&format!("obj #{vnum}: non-numeric {field}: {tok:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> Vec<IrItem> {
        parse_str(s, &PathBuf::from("test.obj")).expect("parse")
    }

    #[test]
    fn parses_silver_ring_with_two_affects() {
        // Reduced version of circle-3.1/lib/world/obj/71.obj #7190.
        let body = "\
#7190
ring silver~
a glinting silver ring~
A lovely silver ring has been left here.~
~
11 g 3
0 0 10 0
9 16000 5000
A
17 -10
A
18 2
$
";
        let items = p(body);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.vnum, 7190);
        assert_eq!(it.keywords, vec!["ring", "silver"]);
        assert_eq!(it.short_descr, "a glinting silver ring");
        assert!(it.long_descr.starts_with("A lovely silver ring"));
        assert!(!it.long_descr.ends_with('\n'));
        assert_eq!(it.action_descr, "");
        assert_eq!(it.item_type, 11); // ITEM_WORN
        assert_eq!(it.extra_flag_bits, 1u64 << 6); // 'g' = bit 6 = MAGIC
        assert_eq!(it.wear_flag_bits, 3); // numeric 3 = TAKE | FINGER
        assert_eq!(it.values, [0, 0, 10, 0]);
        assert_eq!(it.weight, 9);
        assert_eq!(it.cost, 16000);
        assert_eq!(it.rent, 5000);
        assert_eq!(it.affects.len(), 2);
        assert_eq!(it.affects[0], (17, -10));
        assert_eq!(it.affects[1], (18, 2));
        assert!(it.extra_descs.is_empty());
    }

    #[test]
    fn parses_container_with_extra_desc() {
        // Reduced version of obj/60.obj #6004 (hooded brass lantern with E).
        let body = "\
#6004
lantern brass hooded~
a hooded brass lantern~
A hooded brass lantern has been left here.~
~
1 0 16385
0 0 100 0
4 60 10
E
lantern brass hooded~
A robust brass oil lantern.
~
$
";
        let items = p(body);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.item_type, 1); // ITEM_LIGHT
        assert_eq!(it.extra_descs.len(), 1);
        assert_eq!(it.extra_descs[0].keywords, vec!["lantern", "brass", "hooded"]);
        assert!(it.extra_descs[0].description.contains("oil lantern"));
    }

    #[test]
    fn parses_multiple_objects_in_one_file() {
        let body = "\
#1
key brass~
a small brass key~
A key lies here.~
~
18 0 1
0 0 0 0
1 5 1
#2
gold~
some gold coins~
Some coins lie here.~
~
20 0 1
50 0 0 0
1 50 0
$
";
        let items = p(body);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].vnum, 1);
        assert_eq!(items[0].item_type, 18);
        assert_eq!(items[1].vnum, 2);
        assert_eq!(items[1].item_type, 20);
        assert_eq!(items[1].values[0], 50);
    }

    #[test]
    fn rejects_unknown_subblock_letter() {
        let body = "\
#1
x~
x~
x~
~
12 0 1
0 0 0 0
1 0 0
Z
$
";
        let err = parse_str(body, &PathBuf::from("t.obj")).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("expected 'E', 'A'"), "unexpected error: {msg}");
    }
}
