//! CircleMUD `.mob` (mobile / NPC) file parser.
//!
//! Reference: `parse_mobile` in `circle-3.1/src/db.c`.
//!
//! File format (one or more mobs followed by `$` EOF marker):
//! ```text
//! #VNUM
//! keywords~                                           (line 2)
//! short_descr~                                        (line 3, e.g. "the wizard")
//! long_descr (multi-line)~                            (in-room sentence)
//! description (multi-line, may be empty)~             (look/examine body)
//! MOB_FLAGS AFF_FLAGS ALIGNMENT FORMAT                ("ablno d 900 S")
//! LEVEL THAC0 AC HP_DICE DAMAGE_DICE                  ("33 2 2 1d1+30000 2d8+18")
//! GOLD EXP                                            ("30000 160000")
//! POSITION DEFAULT_POSITION SEX                       ("8 8 1")
//! [if FORMAT == 'E':]
//! NamedAttr: value                                    ("BareHandAttack: 12")
//! ...
//! E
//! ```
//! `S` (simple) terminates the mob with no extra block; `E` (enhanced)
//! reads named attribute lines until a lone `E` is encountered.

use anyhow::{Context, Result};
use std::path::Path;

use super::flags::parse_bitvector;
use super::parser::LineParser;
use crate::import::IrMob;

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
        // `$` terminates the file. Anything after (e.g. credits in some
        // patched stocks) is silently ignored.
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
        } else {
            return Err(p.inner.err(&format!("expected '#vnum' or '$', got: {trimmed:?}")));
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
        // Stock format is "long_descr\n~"; the parser captures the trailing
        // blank line. Trim trailing whitespace/newlines so the in-room line
        // doesn't have a dangling LF.
        let long_descr = long_descr_raw.trim_end().to_string();
        let description = self
            .inner
            .read_string()
            .with_context(|| format!("mob #{vnum}: description"))?
            .trim_end()
            .to_string();

        // Action line: MOB_FLAGS AFF_FLAGS ALIGNMENT FORMAT
        let action_line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: expected action line, got EOF")))?
            .trim()
            .to_string();
        let mut t = action_line.split_whitespace();
        let mob_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing MOB_FLAGS")))?;
        let aff_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing AFF_FLAGS")))?;
        let align_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing ALIGNMENT")))?;
        let format_token = t
            .next()
            .ok_or_else(|| self.inner.err(&format!("mob #{vnum}: missing FORMAT (S/E)")))?;
        let mob_flag_bits = parse_bitvector(mob_token);
        let aff_flag_bits = parse_bitvector(aff_token);
        let alignment: i32 = align_token
            .parse()
            .map_err(|_| self.inner.err(&format!("mob #{vnum}: non-numeric alignment {align_token}")))?;
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
                // Some E-attrs (e.g. patched 'Description:') span multiple
                // lines via tilde-strings. Stock CircleMUD only uses
                // single-line `Name: Value` pairs, which is what we model
                // here. Fancier patches surface in the warning report via
                // the unique-name set built in mapping.rs.
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
    fn parses_simple_mob() {
        let body = "\
#3000
wizard~
the wizard~
A wizard walks around behind the counter, talking to himself.
~
The wizard looks old and senile.
~
ablno d 900 S
33 2 2 1d1+30000 2d8+18
30000 160000
8 8 1
$
";
        let mobs = p(body);
        assert_eq!(mobs.len(), 1);
        let m = &mobs[0];
        assert_eq!(m.vnum, 3000);
        assert_eq!(m.keywords, vec!["wizard"]);
        assert_eq!(m.short_descr, "the wizard");
        assert!(m.long_descr.starts_with("A wizard walks"));
        assert!(!m.long_descr.ends_with('\n'));
        assert!(m.description.starts_with("The wizard looks"));
        assert_eq!(m.alignment, 900);
        assert_eq!(m.format, 'S');
        assert_eq!(m.level, 33);
        assert_eq!(m.thac0, 2);
        assert_eq!(m.ac, 2);
        assert_eq!(m.hp_dice, "1d1+30000");
        assert_eq!(m.damage_dice, "2d8+18");
        assert_eq!(m.gold, 30000);
        assert_eq!(m.exp, 160000);
        assert_eq!(m.position, 8);
        assert_eq!(m.default_position, 8);
        assert_eq!(m.sex, 1);
        // 'a','b','l','n','o' = bits 0,1,11,13,14
        let expected = (1u64 << 0) | (1 << 1) | (1 << 11) | (1 << 13) | (1 << 14);
        assert_eq!(m.mob_flag_bits, expected);
        assert_eq!(m.aff_flag_bits, 1u64 << 3); // 'd'
    }

    #[test]
    fn parses_enhanced_mob_with_named_attr() {
        let body = "\
#1
Puff dragon fractal~
Puff~
Puff the Fractal Dragon is here.
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
        assert_eq!(m.extra_attrs.len(), 1);
        assert_eq!(m.extra_attrs[0], ("BareHandAttack".to_string(), "12".to_string()));
    }

    #[test]
    fn empty_description_block_is_ok() {
        let body = "\
#10
clone~
the clone~
A boring old clone is standing here.
~
~
b 0 0 S
1 0 0 1d1+1 1d1+1
0 0
8 8 0
$
";
        let mobs = p(body);
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0].description, "");
    }

    #[test]
    fn ignores_trailing_after_dollar() {
        let body = "\
#1
a~
a~
b
~
~
0 0 0 S
1 0 0 1d1+1 1d1+1
0 0
8 8 0
$
some credits or junk after EOF
more junk
";
        let mobs = p(body);
        assert_eq!(mobs.len(), 1);
    }
}
