//! tbaMUD `.mob` parser. Forks the CircleMUD parser to handle:
//!
//! - **10-token action line** (`f1 f2 f3 f4 f5 f6 f7 f8 align letter`) where
//!   f1..f4 are 32-bit MOB_FLAGS banks and f5..f8 are 32-bit AFF_FLAGS banks.
//!   Each field accepts ASCII-letter (`abdh` = bits 0|1|3|7) or decimal
//!   encoding. Stock CircleMUD uses 4 tokens (`act aff align letter`).
//!   Reference: `parse_mobile` in `tbamud/src/db.c` ~line 1820.
//! - **T trailers** following the S/E body, attaching DG Scripts triggers to
//!   the mob via vnum reference.
//!
//! High banks (f2..f4 / f6..f8) are dropped silently — IronMUD's flag model
//! caps at 64 bits and the stock tbaMUD mob bits all live in f1 / f5.

use anyhow::{Context, Result};
use std::path::Path;

use crate::import::IrMob;
use crate::import::engines::circle::flags::parse_bitvector;
use crate::import::engines::circle::parser::LineParser;

pub fn parse_file(path: &Path) -> Result<Vec<IrMob>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrMob>> {
    let mut p = MobParser {
        inner: LineParser::new(text, path),
    };
    let mut mobs = Vec::new();
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
            mobs.push(p.parse_mob(vnum)?);
        } else if trimmed.starts_with("T ") {
            // Trigger attachment for the most recently parsed mob. Last mob
            // owns it; we look it up by mutable index.
            p.inner.consume_line();
            if let Some(vnum) = trimmed[2..].split_whitespace().next().and_then(|s| s.parse::<i32>().ok()) {
                if let Some(last) = mobs.last_mut() {
                    last.trigger_vnums.push(vnum);
                }
            }
        } else {
            return Err(p.inner.err(&format!("expected '#vnum', 'T <vnum>', or '$', got: {trimmed:?}")));
        }
    }
    Ok(mobs)
}

struct MobParser<'a> {
    inner: LineParser<'a>,
}

impl<'a> MobParser<'a> {
    fn parse_mob(&mut self, vnum: i32) -> Result<IrMob> {
        let source = self.inner.loc().with_room(vnum);

        let keywords_raw = self
            .inner
            .read_string()
            .with_context(|| format!("mob #{vnum}: keywords"))?;
        let keywords: Vec<String> = keywords_raw.split_whitespace().map(str::to_string).collect();

        let short_descr = self
            .inner
            .read_string()
            .with_context(|| format!("mob #{vnum}: short descr"))?
            .trim()
            .to_string();
        let long_descr_raw = self
            .inner
            .read_string()
            .with_context(|| format!("mob #{vnum}: long descr"))?;
        let long_descr = long_descr_raw.trim_end().to_string();
        let description = self
            .inner
            .read_string()
            .with_context(|| format!("mob #{vnum}: description"))?
            .trim_end()
            .to_string();

        // Action line: tbaMUD format = `f1 f2 f3 f4 f5 f6 f7 f8 align letter`.
        // Stock CircleMUD format = `act aff align letter` (4 tokens). We
        // accept both: count tokens to decide. Mixed fixtures (some mobs
        // 4-token, others 10-token) are rare in practice but handled.
        let action_line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: expected action line, got EOF")))?
            .trim()
            .to_string();
        let parts: Vec<&str> = action_line.split_whitespace().collect();
        let (mob_flag_bits, aff_flag_bits, alignment, format_token) = match parts.len() {
            10 => {
                // tbaMUD format. f1 = MOB bits 0..31; high banks dropped.
                let mob_flag_bits = parse_bitvector(parts[0]);
                let aff_flag_bits = parse_bitvector(parts[4]);
                let alignment: i32 = parts[8]
                    .parse()
                    .map_err(|_| self.inner.err(&format!("mob #{vnum}: non-numeric alignment {:?}", parts[8])))?;
                (mob_flag_bits, aff_flag_bits, alignment, parts[9])
            }
            n if n >= 4 => {
                // Stock CircleMUD format (or a tbaMUD file authored without
                // the extension — rare but present in some patches).
                let mob_flag_bits = parse_bitvector(parts[0]);
                let aff_flag_bits = parse_bitvector(parts[1]);
                let alignment: i32 = parts[2]
                    .parse()
                    .map_err(|_| self.inner.err(&format!("mob #{vnum}: non-numeric alignment {:?}", parts[2])))?;
                (mob_flag_bits, aff_flag_bits, alignment, parts[3])
            }
            other => {
                return Err(self.inner.err(&format!(
                    "mob #{vnum}: action line has {other} tokens; expected 4 (CircleMUD) or 10 (tbaMUD)"
                )));
            }
        };
        let format = format_token.chars().next().unwrap_or('S').to_ascii_uppercase();
        if format != 'S' && format != 'E' {
            return Err(self
                .inner
                .err(&format!("mob #{vnum}: unknown format {format_token:?} (expected S or E)")));
        }

        // Stat line 1: LEVEL THAC0 AC HP_DICE DAMAGE_DICE
        let stat1 = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: expected stat line 1, got EOF")))?
            .trim()
            .to_string();
        let mut t = stat1.split_whitespace();
        let level = parse_i32(&mut t, "level", vnum, &self.inner)?;
        let thac0 = parse_i32(&mut t, "thac0", vnum, &self.inner)?;
        let ac = parse_i32(&mut t, "ac", vnum, &self.inner)?;
        let hp_dice = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing HP_DICE")))?
            .to_string();
        let damage_dice = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing DAMAGE_DICE")))?
            .to_string();

        // Stat line 2: GOLD EXP
        let stat2 = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: expected stat line 2, got EOF")))?
            .trim()
            .to_string();
        let mut t = stat2.split_whitespace();
        let gold = parse_i32(&mut t, "gold", vnum, &self.inner)?;
        let exp = parse_i32(&mut t, "exp", vnum, &self.inner)?;

        // Stat line 3: POSITION DEFAULT_POSITION SEX
        let stat3 = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: expected stat line 3, got EOF")))?
            .trim()
            .to_string();
        let mut t = stat3.split_whitespace();
        let position = parse_i32(&mut t, "position", vnum, &self.inner)?;
        let default_position = parse_i32(&mut t, "default_position", vnum, &self.inner)?;
        let sex = parse_i32(&mut t, "sex", vnum, &self.inner)?;

        // E-format: read named attributes until a lone 'E' line.
        let mut extra_attrs: Vec<(String, String)> = Vec::new();
        if format == 'E' {
            loop {
                self.inner.skip_blank();
                let Some(line) = self.inner.peek_line() else {
                    return Err(self
                        .inner
                        .err(&format!("mob #{vnum}: EOF before E-block 'E' terminator")));
                };
                let trimmed = line.trim();
                if trimmed == "E" {
                    self.inner.consume_line();
                    break;
                }
                self.inner.consume_line();
                if let Some((name, value)) = trimmed.split_once(':') {
                    extra_attrs.push((name.trim().to_string(), value.trim().to_string()));
                }
            }
        }

        Ok(IrMob {
            vnum,
            keywords,
            short_descr,
            long_descr,
            description,
            mob_flag_bits,
            aff_flag_bits,
            alignment,
            level,
            thac0,
            ac,
            hp_dice,
            damage_dice,
            gold,
            exp,
            position,
            default_position,
            sex,
            format,
            extra_attrs,
            trigger_vnums: Vec::new(),
            source,
        })
    }
}

fn parse_i32(it: &mut std::str::SplitWhitespace<'_>, field: &str, vnum: i32, p: &LineParser<'_>) -> Result<i32> {
    let tok = it
        .next()
        .ok_or_else(|| p.err(&format!("mob #{vnum}: missing {field}")))?;
    tok.parse()
        .map_err(|_| p.err(&format!("mob #{vnum}: non-numeric {field}: {tok:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> Vec<IrMob> {
        parse_str(s, &PathBuf::from("test.mob")).expect("parse")
    }

    #[test]
    fn parses_tba_10_token_action_line_decimal() {
        let body = "\
#3000
wizard~
the wizard~
A wizard walks around behind the counter.
~
   The wizard looks old and senile.
~
26635 0 0 0 16 0 0 0 900 E
33 9 -9 6d6+330 5d5+5
330 108900
8 8 1
E
$
";
        let mobs = p(body);
        assert_eq!(mobs.len(), 1);
        let m = &mobs[0];
        assert_eq!(m.vnum, 3000);
        assert_eq!(m.mob_flag_bits, 26635); // f1 decimal
        assert_eq!(m.aff_flag_bits, 16); // f5 decimal
        assert_eq!(m.alignment, 900);
        assert_eq!(m.format, 'E');
        assert_eq!(m.level, 33);
    }

    #[test]
    fn parses_tba_10_token_action_line_ascii_letters() {
        let body = "\
#3001
guard~
a city guard~
A burly guard stands here.
~
A burly guard.
~
abdf 0 0 0 cd 0 0 0 0 S
30 0 0 5d10+200 3d6+10
1000 0
8 8 1
$
";
        let mobs = p(body);
        let m = &mobs[0];
        // a|b|d|f = bits 0|1|3|5 = 1+2+8+32 = 43
        assert_eq!(m.mob_flag_bits, 0b101011);
        // c|d = bits 2|3 = 12
        assert_eq!(m.aff_flag_bits, 0b1100);
        assert_eq!(m.format, 'S');
    }

    #[test]
    fn falls_back_to_circle_4_token_format() {
        let body = "\
#5
Puff fractal~
Puff~
Puff is here.
~
A puff of dragon.
~
anopqr dkp 1000 E
26 1 -1 5d10+550 4d6+3
10000 155000
8 8 2
BareHandAttack: 12
E
$
";
        let mobs = p(body);
        let m = &mobs[0];
        assert_eq!(m.format, 'E');
        assert_eq!(m.alignment, 1000);
        assert_eq!(m.extra_attrs.len(), 1);
    }

    #[test]
    fn captures_trigger_trailers() {
        let body = "\
#10
mob~
a mob~
A mob is here.
~
~
0 0 0 0 0 0 0 0 0 S
1 0 0 1d1+1 1d1+1
0 0
8 8 0
T 555
T 556
#11
mob2~
another mob~
Another mob.
~
~
0 0 0 0 0 0 0 0 0 S
1 0 0 1d1+1 1d1+1
0 0
8 8 0
$
";
        let mobs = p(body);
        assert_eq!(mobs.len(), 2);
        assert_eq!(mobs[0].trigger_vnums, vec![555, 556]);
        assert!(mobs[1].trigger_vnums.is_empty());
    }
}
