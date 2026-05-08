//! tbaMUD `.qst` (quest) parser. Captures full record (header + body) so the
//! mapping layer can translate supported `AQ_*` types into `QuestData`.
//!
//! Record format:
//! ```text
//! #VNUM
//! Quest Name~
//! keyword(s)~
//! accept message (multi-line, ~-terminated)
//! complete message (multi-line, ~-terminated)
//! quit message (multi-line, ~-terminated)
//! <stat line>:    type qm_vnum flags target_vnum prev_quest unused unused
//! <reward line 1>: gold_reward exp_reward obj_reward_vnum return_mob_vnum_or_other ints...
//! <reward line 2>: more ints (penalties / time / etc — not consumed)
//! S
//! ```
//!
//! The exact stat-line schema varies a bit by tbamud version, but the first
//! few ints are stable: `type target_vnum prev_quest qm_vnum ...`. We extract
//! a minimal slice and surface anything we don't recognise via the mapper's
//! warn path.

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
            let accept_msg = p
                .read_string()
                .with_context(|| format!("qst #{vnum}: accept message"))?
                .to_string();
            let complete_msg = p
                .read_string()
                .with_context(|| format!("qst #{vnum}: complete message"))?
                .to_string();
            let quit_msg = p
                .read_string()
                .with_context(|| format!("qst #{vnum}: quit message"))?
                .to_string();

            // Stat line 1: type qm_vnum flags target_vnum prev_quest unused unused
            // (tbamud's actual order is `type qm_vnum flags target_vnum prev_quest`
            // followed by two unused ints in stock 1.qst).
            let stat_line = read_int_line(&mut p);
            let (quest_type, qm_vnum_raw, flags, target_vnum) = parse_stat_line(&stat_line);

            // Reward line 1: gold_reward exp_reward obj_reward_vnum returnmob/quantity ints
            let reward_line_1 = read_int_line(&mut p);
            let (gold_reward, _exp_reward, obj_reward_vnum, mb_quantity_or_returnmob) =
                parse_reward_line(&reward_line_1);

            // Reward line 2: penalties/time — currently unused.
            let _ = read_int_line(&mut p);

            // Walk to the 'S' terminator (some `.qst` records have extra body
            // ints we don't model).
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

            // Stock 1.qst's reward-line-1 has 60 in slot 0 (gold), -1 slot 2,
            // 3 slot 3 — quantity. We pick `mb_quantity_or_returnmob` as
            // quantity when quest_type is MOB_KILL (3) / OBJ_FIND/RETURN
            // (0/5); for OBJ_RETURN it's also typically the return mob.
            // Stock layout is loose — at the very least we capture quantity.
            let quantity = if mb_quantity_or_returnmob > 0 {
                mb_quantity_or_returnmob
            } else {
                1
            };

            out.push(IrQuest {
                vnum,
                name,
                keywords,
                source,
                qm_vnum: qm_vnum_raw,
                quest_type,
                target_vnum,
                quantity,
                gold_reward,
                obj_reward_vnum,
                flags,
                accept_msg,
                complete_msg,
                quit_msg,
            });
        } else {
            return Err(p.err(&format!("expected '#vnum' or '$', got: {trimmed:?}")));
        }
    }
    Ok(out)
}

/// Read one non-blank, non-record-boundary line from `p` and return its raw
/// trimmed text. Returns "" on EOF / boundary.
fn read_int_line(p: &mut LineParser) -> String {
    p.skip_blank();
    let line = match p.peek_line() {
        Some(l) => l.to_string(),
        None => return String::new(),
    };
    let t = line.trim();
    if t.starts_with('S') || t.starts_with('#') || t.starts_with('$') {
        return String::new();
    }
    p.consume_line();
    t.to_string()
}

/// Parse the stat line "type qm_vnum flags target_vnum ...".
/// Returns (quest_type, qm_vnum, flags, target_vnum).
fn parse_stat_line(s: &str) -> (i32, i32, i32, i32) {
    let mut iter = s.split_whitespace().filter_map(|t| t.parse::<i32>().ok());
    let quest_type = iter.next().unwrap_or(-1);
    let qm_vnum = iter.next().unwrap_or(-1);
    let flags = iter.next().unwrap_or(0);
    let target_vnum = iter.next().unwrap_or(-1);
    (quest_type, qm_vnum, flags, target_vnum)
}

/// Parse the reward line "gold exp obj_reward_vnum quantity_or_return ...".
fn parse_reward_line(s: &str) -> (i64, i64, i32, i32) {
    let mut iter = s.split_whitespace();
    let gold = iter.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
    let exp = iter.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
    let obj = iter.next().and_then(|t| t.parse::<i32>().ok()).unwrap_or(-1);
    let q = iter.next().and_then(|t| t.parse::<i32>().ok()).unwrap_or(-1);
    (gold, exp, obj, q)
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
        let q = &qs[0];
        assert_eq!(q.vnum, 100);
        assert_eq!(q.name, "Kill the Mice!");
        assert_eq!(q.keywords, "mice");
        // Stat line: "3 179 0 194 -1 -1 -1" → type=3 qm=179 flags=0 target=194
        // The actual layout in stock 1.qst is `type qm flags target prev`; the
        // qm_vnum 179 is the questmaster mob.
        assert_eq!(q.quest_type, 3);
        assert_eq!(q.qm_vnum, 179);
        assert_eq!(q.target_vnum, 194);
        assert!(!q.accept_msg.is_empty());
        assert!(!q.complete_msg.is_empty());
        assert!(!q.quit_msg.is_empty());
    }
}
