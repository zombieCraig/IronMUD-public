//! tbaMUD `.obj` parser. Forks the CircleMUD parser to handle:
//!
//! - **13-token type/flags line**: `type extra1..extra4 wear1..wear4
//!   aff1..aff4`. Each flag bank is 32 bits, ASCII-letter or decimal. Stock
//!   CircleMUD uses 3 tokens (`type extra wear`). Reference: `parse_object`
//!   in `tbamud/src/db.c` ~line 1956.
//! - **5-token weight/cost/rent line**: `weight cost rent level timer`.
//!   `level` (min wear level) and `timer` (item lifetime) are dropped —
//!   no IronMUD analog. Stock circle uses 3 tokens.
//! - **`A`-blocks with 3 fields** (`location modifier bitvector`) — tbaMUD's
//!   third field carries an aff_bitvector to apply alongside the modifier.
//!   We accept it but drop bits ≥ 0 (warn-only via mapping).
//! - **T trailers inside the record** (after the weight line, before E/A
//!   subblocks).
//!
//! High flag banks (extra2..extra4 / wear2..wear4 / aff1..aff4) are dropped
//! silently — IronMUD's flag model caps at 64 bits and the stock tbaMUD obj
//! bits all live in extra1 / wear1.

use anyhow::{Context, Result};
use std::path::Path;

use crate::import::{IrExtraDesc, IrItem};
use crate::import::engines::circle::flags::parse_bitvector;
use crate::import::engines::circle::parser::LineParser;

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

        // Numeric line 1: type + flag banks.
        // tbaMUD = 13 tokens; circle stock = 3 tokens. Accept both.
        let header = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("obj #{vnum}: expected type/flags line, got EOF")))?
            .trim()
            .to_string();
        let parts: Vec<&str> = header.split_whitespace().collect();
        let (item_type, extra_flag_bits, wear_flag_bits) = match parts.len() {
            13 => {
                let item_type: i32 = parts[0]
                    .parse()
                    .map_err(|_| self.inner.err(&format!("obj #{vnum}: non-numeric item_type {:?}", parts[0])))?;
                // extra1 = parts[1] (bits 0..31 = stock ITEM_*)
                // wear1 = parts[5] (bits 0..31 = stock WEAR_*)
                let extra_flag_bits = parse_bitvector(parts[1]);
                let wear_flag_bits = parse_bitvector(parts[5]);
                // parts[9..13] are obj-attached AFF banks; not surfaced today
                // (IronMUD obj-attached AFF buffs would be redundant with
                // existing item flag handling).
                (item_type, extra_flag_bits, wear_flag_bits)
            }
            n if n >= 3 => {
                let item_type: i32 = parts[0]
                    .parse()
                    .map_err(|_| self.inner.err(&format!("obj #{vnum}: non-numeric item_type {:?}", parts[0])))?;
                let extra_flag_bits = parse_bitvector(parts[1]);
                let wear_flag_bits = parse_bitvector(parts[2]);
                (item_type, extra_flag_bits, wear_flag_bits)
            }
            other => {
                return Err(self.inner.err(&format!(
                    "obj #{vnum}: type/flags line has {other} tokens; expected 3 (CircleMUD) or 13 (tbaMUD)"
                )));
            }
        };

        // Numeric line 2: V0 V1 V2 V3 (same as stock circle).
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

        // Numeric line 3: weight cost rent [level timer]. tbaMUD = 5 tokens;
        // circle stock = 3. The C parser at db.c:2001 accepts 3, 4, or 5.
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
        // Optional level + timer trailers — silently dropped.
        let _ = t.next();
        let _ = t.next();

        // Sub-blocks: T (trigger attach), E, A — until next # / $ / EOF.
        let mut trigger_vnums: Vec<i32> = Vec::new();
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
                let mut pp = pair_line.split_whitespace();
                let loc = parse_i32(&mut pp, "apply_location", vnum, &self.inner)?;
                let modifier = parse_i32(&mut pp, "apply_modifier", vnum, &self.inner)?;
                // tbaMUD A-block has an optional 3rd field (aff_bitvector).
                // Drop it — IronMUD's A-block model is just (loc, modifier).
                let _ = pp.next();
                affects.push((loc, modifier));
                continue;
            }
            if trimmed.starts_with("T ") {
                self.inner.consume_line();
                if let Some(tv) = trimmed[2..]
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    trigger_vnums.push(tv);
                }
                continue;
            }
            return Err(self
                .inner
                .err(&format!("obj #{vnum}: expected 'E', 'A', 'T <vnum>', '#vnum', or '$' — got {trimmed:?}")));
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
            trigger_vnums,
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
    fn parses_tba_13_token_obj_with_t_trailers() {
        let body = "\
#3008
teleporter~
the teleporter~
A strange device labeled \"teleporter\" was left here.~
~
12 0 0 0 0 a 0 0 0 0 0 0 0
0 0 0 0
1 10 0 0 0
T 3014
T 3015
E
teleporter~
This teleporter is used to transfer players between zones.
~
$
";
        let items = p(body);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.vnum, 3008);
        assert_eq!(it.item_type, 12); // OTHER
        assert_eq!(it.wear_flag_bits, 1); // 'a' = bit 0 = TAKE
        assert_eq!(it.weight, 1);
        assert_eq!(it.cost, 10);
        assert_eq!(it.rent, 0);
        assert_eq!(it.trigger_vnums, vec![3014, 3015]);
        assert_eq!(it.extra_descs.len(), 1);
    }

    #[test]
    fn falls_back_to_circle_3_token_format() {
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
$
";
        let items = p(body);
        let it = &items[0];
        assert_eq!(it.item_type, 11);
        assert_eq!(it.extra_flag_bits, 1u64 << 6); // 'g' = bit 6 = MAGIC
        assert_eq!(it.wear_flag_bits, 3);
        assert_eq!(it.weight, 9);
        assert_eq!(it.affects, vec![(17, -10)]);
    }

    #[test]
    fn drops_three_field_a_block() {
        // tbaMUD A blocks: location modifier aff_bitvector
        let body = "\
#1
ring~
a ring~
A ring is here.~
~
11 0 0 0 0 a 0 0 0 0 0 0 0
0 0 0 0
1 100 50 0 0
A
1 2 0
$
";
        let items = p(body);
        let it = &items[0];
        assert_eq!(it.affects, vec![(1, 2)]);
    }
}
