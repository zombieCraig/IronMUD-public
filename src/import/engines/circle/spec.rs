//! Parser for CircleMUD `src/spec_assign.c` and `src/castle.c`.
//!
//! Stock CircleMUD 3.1 ships with no DG Scripts; the only "trigger"
//! surface is hard-coded vnum→specproc bindings in two C files. This
//! parser extracts the literal `ASSIGN(MOB|OBJ|ROOM)(vnum, fname)` and
//! `castle_mob_spec(vnum, fname)` calls into engine-neutral
//! [`IrTrigger`]s; the mapping layer translates each into either a
//! `MobileFlags` bit or a `*Trigger` push, or surfaces it as a warning.
//!
//! Also opportunistically extracts the literal `do_say` quote strings
//! from `puff()` in `src/spec_procs.c` so the imported Puff can keep her
//! distinctive lines via an `@say_random` template trigger.

use std::path::Path;

use anyhow::{Context, Result};

use crate::import::{AttachType, IrTrigger, SourceLoc};

/// Strip C `/* ... */` block comments and `// ...` line comments. Operates
/// in two passes; not a full C tokenizer (won't notice strings) but stock
/// `spec_assign.c` and `castle.c` don't put `ASSIGN*` tokens inside string
/// literals. Newlines inside block comments are preserved so byte-offset
/// → line-number mapping stays accurate.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Find every literal call of `<verb>(<digits>, <ident>)` in `text` and
/// invoke `emit` with (verb, vnum, fname, line). The match is anchored on
/// a word boundary before `verb` so `foo_ASSIGNMOB(...)` won't trigger.
fn scan_calls<F>(text: &str, verbs: &[&str], mut emit: F)
where
    F: FnMut(&str, i32, &str, u32),
{
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let is_ident_char = c.is_ascii_alphanumeric() || c == b'_';
        let prev_is_ident = i > 0 && {
            let pc = bytes[i - 1];
            pc.is_ascii_alphanumeric() || pc == b'_'
        };
        if !is_ident_char || prev_is_ident {
            i += 1;
            continue;
        }
        // i is at the start of a word. Try each verb.
        let mut matched: Option<&str> = None;
        for v in verbs {
            let vb = v.as_bytes();
            if i + vb.len() <= bytes.len() && &bytes[i..i + vb.len()] == vb {
                let after = i + vb.len();
                // Must be a non-ident char after.
                let next_ok = after >= bytes.len() || !{
                    let nb = bytes[after];
                    nb.is_ascii_alphanumeric() || nb == b'_'
                };
                if next_ok {
                    matched = Some(*v);
                    break;
                }
            }
        }
        let Some(verb) = matched else {
            // Skip the rest of this identifier.
            while i < bytes.len() && {
                let b = bytes[i];
                b.is_ascii_alphanumeric() || b == b'_'
            } {
                i += 1;
            }
            continue;
        };
        let line = 1 + bytes[..i].iter().filter(|&&b| b == b'\n').count() as u32;
        let mut j = i + verb.len();
        // Skip whitespace.
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            i = j;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        // Read vnum digits.
        let v_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == v_start {
            i = j.max(i + 1);
            continue;
        }
        let vnum: i32 = std::str::from_utf8(&bytes[v_start..j]).unwrap().parse().unwrap_or(0);
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b',' {
            i = j.max(i + 1);
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        // Read identifier.
        let n_start = j;
        if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
            while j < bytes.len() && {
                let b = bytes[j];
                b.is_ascii_alphanumeric() || b == b'_'
            } {
                j += 1;
            }
        }
        if j == n_start {
            i = j.max(i + 1);
            continue;
        }
        let name = std::str::from_utf8(&bytes[n_start..j]).unwrap().to_string();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b')' {
            i = j.max(i + 1);
            continue;
        }
        emit(verb, vnum, &name, line);
        i = j + 1;
    }
}

/// Parse `src/spec_assign.c`. Returns one [`IrTrigger`] per literal
/// `ASSIGN(MOB|OBJ|ROOM)(vnum, fname)` line. Runtime / loop assignments
/// (e.g. the `dts_are_dumps` `for` loop) don't match the pattern and are
/// silently ignored.
pub fn parse_spec_assign(path: &Path) -> Result<Vec<IrTrigger>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading spec_assign {}", path.display()))?;
    Ok(parse_spec_assign_str(&text, path))
}

pub fn parse_spec_assign_str(text: &str, path: &Path) -> Vec<IrTrigger> {
    let cleaned = strip_comments(text);
    let mut out = Vec::new();
    scan_calls(&cleaned, &["ASSIGNMOB", "ASSIGNOBJ", "ASSIGNROOM"], |verb, vnum, name, line| {
        let attach = match verb {
            "ASSIGNMOB" => AttachType::Mob,
            "ASSIGNOBJ" => AttachType::Obj,
            "ASSIGNROOM" => AttachType::Room,
            _ => unreachable!(),
        };
        out.push(IrTrigger {
            source_vnum: vnum,
            attach_type: attach,
            specproc_name: name.to_lowercase(),
            args: Vec::new(),
            source: SourceLoc::file(path.to_path_buf()).with_line(line),
        });
    });
    out
}

/// Parse `src/castle.c`. Each `castle_mob_spec(vnum, fname)` line maps
/// to an IrTrigger with attach_type=Mob. The bound functions are bespoke
/// per-NPC C bodies, so the mapping layer warns on every entry.
///
/// CircleMUD's `castle_mob_spec` takes an *offset from the zone's bot
/// vnum* rather than an absolute vnum. The offset is added to
/// `Z_KINGS_C * 100` (stock = 150 → 15000) to get the real vnum, since
/// CircleMUD zones conventionally span `vnum*100..vnum*100+99`. We parse
/// the `#define Z_KINGS_C N` line out of the file to support patched
/// castles in non-stock locations; missing → fall back to 15000.
pub fn parse_castle(path: &Path) -> Result<Vec<IrTrigger>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading castle {}", path.display()))?;
    Ok(parse_castle_str(&text, path))
}

pub fn parse_castle_str(text: &str, path: &Path) -> Vec<IrTrigger> {
    let cleaned = strip_comments(text);
    let base = parse_castle_base(&cleaned).unwrap_or(15000);
    let mut out = Vec::new();
    scan_calls(&cleaned, &["castle_mob_spec"], |_verb, offset, name, line| {
        out.push(IrTrigger {
            source_vnum: base + offset,
            attach_type: AttachType::Mob,
            specproc_name: name.to_lowercase(),
            args: Vec::new(),
            source: SourceLoc::file(path.to_path_buf()).with_line(line),
        });
    });
    out
}

/// Read `#define Z_KINGS_C <N>` and return `N * 100` (the conventional
/// CircleMUD zone-vnum-to-bot mapping). `None` if the define is absent
/// or unparseable — caller falls back to 15000 (stock 3.1 value).
fn parse_castle_base(text: &str) -> Option<i32> {
    for line in text.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("#define") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix("Z_KINGS_C") {
                let num: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num.parse::<i32>() {
                    return Some(n * 100);
                }
            }
        }
    }
    None
}

/// Extract literal quote strings from the `puff()` body in
/// `src/spec_procs.c`. Returns the strings in declaration order. Used as
/// `args` on the puff `@say_random` template trigger so the imported
/// Puff keeps her distinctive lines.
pub fn parse_puff_quotes(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading spec_procs {}", path.display()))?;
    Ok(parse_puff_quotes_str(&text))
}

pub fn parse_puff_quotes_str(text: &str) -> Vec<String> {
    let cleaned = strip_comments(text);
    let Some(start) = cleaned.find("SPECIAL(puff)") else {
        return Vec::new();
    };
    let bytes = cleaned.as_bytes();
    let Some(brace_off) = cleaned[start..].find('{') else {
        return Vec::new();
    };
    let body_start = start + brace_off;
    let mut depth = 0i32;
    let mut body_end = body_start;
    for (i, &b) in bytes.iter().enumerate().skip(body_start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    body_end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if body_end <= body_start {
        return Vec::new();
    }
    let body = &cleaned[body_start..=body_end];
    // Match every `do_say(...)` call and pull the first quoted string from
    // its arguments (skipping past the `strcpy(actbuf, ...)` wrapper).
    let mut out = Vec::new();
    let body_bytes = body.as_bytes();
    let mut i = 0;
    while i + 7 <= body_bytes.len() {
        if &body_bytes[i..i + 6] == b"do_say" {
            let prev_ok = i == 0 || !{
                let p = body_bytes[i - 1];
                p.is_ascii_alphanumeric() || p == b'_'
            };
            if prev_ok {
                // Find the first `"` after `do_say(...`.
                let mut j = i + 6;
                while j < body_bytes.len() && body_bytes[j] != b'"' && body_bytes[j] != b';' {
                    j += 1;
                }
                if j < body_bytes.len() && body_bytes[j] == b'"' {
                    j += 1;
                    let s = j;
                    while j < body_bytes.len() && body_bytes[j] != b'"' {
                        // Step over escaped quotes ("\"").
                        if body_bytes[j] == b'\\' && j + 1 < body_bytes.len() {
                            j += 2;
                            continue;
                        }
                        j += 1;
                    }
                    if j > s {
                        let lit = &body[s..j];
                        if !lit.is_empty() {
                            out.push(lit.to_string());
                        }
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("/test/spec_assign.c")
    }

    #[test]
    fn parses_basic_assignments() {
        let src = r#"
void assign_mobiles(void) {
  ASSIGNMOB(1, puff);
  ASSIGNMOB(3059, cityguard);
  ASSIGNMOB(3060, cityguard);
}
void assign_objects(void) {
  ASSIGNOBJ(3034, bank);
}
void assign_rooms(void) {
  ASSIGNROOM(3030, dump);
}
"#;
        let out = parse_spec_assign_str(src, &p());
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].source_vnum, 1);
        assert_eq!(out[0].specproc_name, "puff");
        assert_eq!(out[0].attach_type, AttachType::Mob);
        assert_eq!(out[3].source_vnum, 3034);
        assert_eq!(out[3].attach_type, AttachType::Obj);
        assert_eq!(out[4].source_vnum, 3030);
        assert_eq!(out[4].attach_type, AttachType::Room);
    }

    #[test]
    fn ignores_runtime_loop_and_comments() {
        let src = r#"
void assign_rooms(void) {
  ASSIGNROOM(3030, dump);
  /* ASSIGNROOM(9999, dump); commented out */
  // ASSIGNROOM(8888, dump);
  if (dts_are_dumps)
    for (i = 0; i <= top_of_world; i++)
      if (ROOM_FLAGGED(i, ROOM_DEATH))
        world[i].func = dump;
}
"#;
        let out = parse_spec_assign_str(src, &p());
        // Only the literal ASSIGNROOM(3030, dump) survives.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_vnum, 3030);
    }

    #[test]
    fn castle_mob_spec_parses_with_default_base() {
        // No #define present → fall back to 15000.
        let src = r#"
void assign_kings_castle(void) {
  castle_mob_spec(1, king_welmar);
  castle_mob_spec(4, training_master);
}
"#;
        let out = parse_castle_str(src, &p());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source_vnum, 15001, "1 + 15000 default base");
        assert_eq!(out[0].specproc_name, "king_welmar");
        assert_eq!(out[1].source_vnum, 15004);
        assert_eq!(out[1].specproc_name, "training_master");
    }

    #[test]
    fn castle_mob_spec_honours_z_kings_c_define() {
        // Patched location: zone 200 → base 20000.
        let src = r#"
#define Z_KINGS_C 200
void assign_kings_castle(void) {
  castle_mob_spec(0, CastleGuard);
  castle_mob_spec(1, king_welmar);
}
"#;
        let out = parse_castle_str(src, &p());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source_vnum, 20000);
        assert_eq!(out[1].source_vnum, 20001);
    }

    #[test]
    fn puff_quotes_extracted() {
        let src = r#"
SPECIAL(puff)
{
  char actbuf[MAX_INPUT_LENGTH];
  if (cmd) return (FALSE);
  switch (rand_number(0, 60)) {
  case 0:
    do_say(ch, strcpy(actbuf, "My god!  It's full of stars!"), 0, 0);
    return (TRUE);
  case 1:
    do_say(ch, strcpy(actbuf, "How'd all those fish get up here?"), 0, 0);
    return (TRUE);
  default:
    return (FALSE);
  }
}
SPECIAL(other) { do_say(ch, "should not be picked up", 0, 0); }
"#;
        let out = parse_puff_quotes_str(src);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "My god!  It's full of stars!");
        assert_eq!(out[1], "How'd all those fish get up here?");
    }

    #[test]
    fn line_numbers_track_source() {
        let src = "// header\n// header\nASSIGNMOB(1, puff);\n";
        let out = parse_spec_assign_str(src, &p());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source.line, Some(3));
    }

    #[test]
    fn does_not_match_substring_in_identifier() {
        let src = "void foo() { my_ASSIGNMOB(99, fake); }";
        let out = parse_spec_assign_str(src, &p());
        assert_eq!(out.len(), 0);
    }
}
