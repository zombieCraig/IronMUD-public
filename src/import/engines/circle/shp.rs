//! CircleMUD `.shp` (shop) file parser.
//!
//! Reference: `boot_the_shops` in `circle-3.1/src/shop.c` and
//! `circle-3.1/doc/sources/building.tex` (Shops section).
//!
//! File format (a shop-file header on line 1, one or more shops, `$~` EOF):
//! ```text
//! CircleMUD v3.0 Shop File~
//! #VNUM~
//! <int> ...                     producing item vnums (one per line, -1 terminates)
//! -1
//! <float>                       profit_buy   (markup multiplier, e.g. 2.1)
//! <float>                       profit_sell  (sellback multiplier, e.g. 0.5)
//! <token> ...                   buy_types — Circle ITEM_TYPE names (e.g. FOOD,
//! -1                            LIQ CONTAINER, WEAPON), -1 terminates. May be
//!                               followed on the same line by a keyword filter
//!                               (rare; we keep only the type token).
//! <string>~                     no_such_item1
//! <string>~                     no_such_item2
//! <string>~                     do_not_buy
//! <string>~                     missing_cash1
//! <string>~                     missing_cash2
//! <string>~                     message_buy
//! <string>~                     message_sell
//! <int>                         temper (0..=2)
//! <int>                         bitvector (WILL_START_FIGHT=1, WILL_BANK_MONEY=2)
//! <int>                         keeper mob vnum
//! <int>                         with_who bitvector (TRADE_NO* class/align)
//! <int> ...                     room vnums (-1 terminates)
//! -1
//! <int>                         open1 (game-hour 0..=N; values >24 wrap)
//! <int>                         close1
//! <int>                         open2 (0 means "no second shift")
//! <int>                         close2
//! ...
//! $~                            EOF marker
//! ```
//!
//! Stock CircleMUD 3.1 ships eight `.shp` files (zones 25, 30, 31, 33, 54,
//! 65, 120, 150). Most zones have no shop file at all.

use anyhow::{Context, Result};
use std::path::Path;

use super::parser::LineParser;
use crate::import::{IrShop, SourceLoc};

/// Recognised Circle ITEM_TYPE tokens that may appear in a `.shp` `buy_types`
/// list. The shop file format permits the names below verbatim; anything
/// else surfaces as `unknown_buy_types`. (Compare with `item_types[]` in
/// `circle-3.1/src/constants.c`.)
const KNOWN_TYPE_TOKENS: &[&str] = &[
    "LIGHT",
    "SCROLL",
    "WAND",
    "STAFF",
    "WEAPON",
    "FIRE WEAPON",
    "MISSILE",
    "TREASURE",
    "ARMOR",
    "POTION",
    "WORN",
    "OTHER",
    "TRASH",
    "TRAP",
    "CONTAINER",
    "NOTE",
    "LIQ CONTAINER",
    "KEY",
    "FOOD",
    "MONEY",
    "PEN",
    "BOAT",
    "FOUNTAIN",
];

pub fn parse_file(path: &Path) -> Result<Vec<IrShop>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_str(&text, path)
}

pub fn parse_str(text: &str, path: &Path) -> Result<Vec<IrShop>> {
    let mut p = ShpParser {
        inner: LineParser::new(text, path),
    };
    // Optional file header. Stock files always include "CircleMUD v3.0 Shop
    // File~"; tolerate its absence so synthetic test fixtures don't have to
    // reproduce it byte-for-byte.
    p.inner.skip_blank();
    if let Some(line) = p.inner.peek_line() {
        let trimmed = line.trim();
        if trimmed.starts_with("CircleMUD") || trimmed.ends_with('~') && !trimmed.starts_with('#') && !trimmed.starts_with('$') {
            // Consume the header line if it's not a shop record / EOF.
            if !trimmed.starts_with('#') && !trimmed.starts_with('$') {
                p.inner.consume_line();
            }
        }
    }

    let mut shops = Vec::new();
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
            // Vnum line is `#VNUM~` in the v3.0 format. Strip the trailing
            // tilde + anything after.
            let vnum_token = rest.split(['~', ' ', '\t']).next().unwrap_or("").trim();
            let vnum: i32 = vnum_token
                .parse()
                .map_err(|_| p.inner.err(&format!("non-numeric shop vnum: {vnum_token:?}")))?;
            p.inner.consume_line();
            shops.push(p.parse_shop(vnum)?);
        } else {
            return Err(p.inner.err(&format!("expected '#vnum~' or '$~', got: {trimmed:?}")));
        }
    }
    Ok(shops)
}

struct ShpParser<'a> {
    inner: LineParser<'a>,
}

impl<'a> ShpParser<'a> {
    fn parse_shop(&mut self, vnum: i32) -> Result<IrShop> {
        let source = self.inner.loc().with_room(vnum);

        let producing = self
            .read_int_list_terminated()
            .with_context(|| format!("shop #{vnum}: producing list"))?;

        let profit_buy = self
            .read_float_line()
            .with_context(|| format!("shop #{vnum}: profit_buy"))?;
        let profit_sell = self
            .read_float_line()
            .with_context(|| format!("shop #{vnum}: profit_sell"))?;

        let (buy_types, unknown_buy_types) = self
            .read_buy_types()
            .with_context(|| format!("shop #{vnum}: buy_types"))?;

        let mut messages: [String; 7] = Default::default();
        for (i, slot) in messages.iter_mut().enumerate() {
            *slot = self
                .inner
                .read_string()
                .with_context(|| format!("shop #{vnum}: message {}", i + 1))?
                .trim()
                .to_string();
        }

        let temper = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: temper"))?;
        let bitvector = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: bitvector"))? as u32;
        let keeper_vnum = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: keeper vnum"))?;
        let with_who = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: with_who"))? as u32;

        let rooms = self
            .read_int_list_terminated()
            .with_context(|| format!("shop #{vnum}: room list"))?;

        let open1 = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: open1"))?;
        let close1 = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: close1"))?;
        let open2 = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: open2"))?;
        let close2 = self
            .read_int_line()
            .with_context(|| format!("shop #{vnum}: close2"))?;

        Ok(IrShop {
            vnum,
            keeper_vnum,
            producing,
            profit_buy,
            profit_sell,
            buy_types,
            unknown_buy_types,
            messages,
            temper,
            bitvector,
            with_who,
            rooms,
            open1,
            close1,
            open2,
            close2,
            source,
        })
    }

    /// Read `<int>...\n-1\n` into a Vec, stopping when we hit `-1`.
    fn read_int_list_terminated(&mut self) -> Result<Vec<i32>> {
        let mut out = Vec::new();
        loop {
            self.inner.skip_blank();
            let line = self
                .inner
                .consume_line()
                .ok_or_else(|| self.inner.err("EOF inside int list (no -1 terminator)"))?
                .trim();
            if line.is_empty() {
                continue;
            }
            let n: i32 = line
                .split_whitespace()
                .next()
                .ok_or_else(|| self.inner.err("blank line in int list"))?
                .parse()
                .map_err(|_| self.inner.err(&format!("non-numeric int list entry: {line:?}")))?;
            if n == -1 {
                return Ok(out);
            }
            out.push(n);
        }
    }

    /// Read CircleMUD ITEM_TYPE tokens until a `-1`. Each line is checked
    /// against `KNOWN_TYPE_TOKENS`; multi-word tokens like `LIQ CONTAINER`
    /// are matched as a prefix (anything trailing is a keyword filter we
    /// don't model). Unknown tokens are returned in the second slot so the
    /// mapping layer can warn.
    fn read_buy_types(&mut self) -> Result<(Vec<String>, Vec<String>)> {
        let mut known = Vec::new();
        let mut unknown = Vec::new();
        loop {
            self.inner.skip_blank();
            let line = self
                .inner
                .consume_line()
                .ok_or_else(|| self.inner.err("EOF inside buy_types list (no -1 terminator)"))?
                .trim()
                .to_string();
            if line.is_empty() {
                continue;
            }
            // Numeric `-1` ends the list. Numeric anything-else means the
            // file uses the legacy decimal-only form; treat it as a known
            // token by index for now (none of the stock files do this).
            if let Ok(n) = line.parse::<i32>() {
                if n == -1 {
                    return Ok((known, unknown));
                }
                // Numeric type code — accept and let the mapping layer
                // translate via the index→name table.
                if let Some(name) = circle_item_type_index_to_name(n) {
                    known.push(name.to_string());
                } else {
                    unknown.push(format!("type#{n}"));
                }
                continue;
            }
            // Match against the longest known token prefix. `LIQ CONTAINER`
            // and `FIRE WEAPON` are the only multi-word names.
            let upper = line.to_uppercase();
            let matched = KNOWN_TYPE_TOKENS.iter().find(|tok| {
                upper == **tok || upper.starts_with(&format!("{tok} ")) || upper.starts_with(&format!("{tok}\t"))
            });
            match matched {
                Some(t) => known.push((*t).to_string()),
                None => unknown.push(line),
            }
        }
    }

    fn read_int_line(&mut self) -> Result<i32> {
        self.inner.skip_blank();
        let line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err("EOF where int expected"))?
            .trim()
            .to_string();
        line.split_whitespace()
            .next()
            .ok_or_else(|| self.inner.err("blank line where int expected"))?
            .parse()
            .map_err(|_| self.inner.err(&format!("non-numeric: {line:?}")))
    }

    fn read_float_line(&mut self) -> Result<f32> {
        self.inner.skip_blank();
        let line = self
            .inner
            .consume_line()
            .ok_or_else(|| self.inner.err("EOF where float expected"))?
            .trim()
            .to_string();
        line.split_whitespace()
            .next()
            .ok_or_else(|| self.inner.err("blank line where float expected"))?
            .parse()
            .map_err(|_| self.inner.err(&format!("non-numeric float: {line:?}")))
    }
}

fn circle_item_type_index_to_name(n: i32) -> Option<&'static str> {
    // Mirrors `item_types[]` in circle-3.1/src/constants.c (UNDEFINED=0).
    match n {
        1 => Some("LIGHT"),
        2 => Some("SCROLL"),
        3 => Some("WAND"),
        4 => Some("STAFF"),
        5 => Some("WEAPON"),
        6 => Some("FIRE WEAPON"),
        7 => Some("MISSILE"),
        8 => Some("TREASURE"),
        9 => Some("ARMOR"),
        10 => Some("POTION"),
        11 => Some("WORN"),
        12 => Some("OTHER"),
        13 => Some("TRASH"),
        14 => Some("TRAP"),
        15 => Some("CONTAINER"),
        16 => Some("NOTE"),
        17 => Some("LIQ CONTAINER"),
        18 => Some("KEY"),
        19 => Some("FOOD"),
        20 => Some("MONEY"),
        21 => Some("PEN"),
        22 => Some("BOAT"),
        23 => Some("FOUNTAIN"),
        _ => None,
    }
}

/// Suppress "unused" warning on SourceLoc when no test imports it directly.
#[allow(dead_code)]
fn _src_keep(_: &SourceLoc) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> Vec<IrShop> {
        parse_str(s, &PathBuf::from("test.shp")).expect("parse")
    }

    #[test]
    fn parses_stock_tavern_shop() {
        // Reduced version of circle-3.1/lib/world/shp/25.shp #2505.
        let body = "\
CircleMUD v3.0 Shop File~
#2505~
2504
2545
-1
2.1
0.5
FOOD
LIQ CONTAINER
-1
%s Haven't got that in storage, try LIST!~
%s You can't sell what you don't HAVE!~
%s Sorry, I'm not a fence.~
%s I'd love to buy it, I just can't spare the coinage~
%s Bah, come back when you can pay!~
%s That'll be %d coins -- thank you.~
%s I'll give ya %d coins for that!~
0
2
2505
112
2518
-1
0
28
0
0
$~
";
        let shops = p(body);
        assert_eq!(shops.len(), 1);
        let s = &shops[0];
        assert_eq!(s.vnum, 2505);
        assert_eq!(s.keeper_vnum, 2505);
        assert_eq!(s.producing, vec![2504, 2545]);
        assert!((s.profit_buy - 2.1).abs() < 1e-4);
        assert!((s.profit_sell - 0.5).abs() < 1e-4);
        assert_eq!(s.buy_types, vec!["FOOD".to_string(), "LIQ CONTAINER".to_string()]);
        assert!(s.unknown_buy_types.is_empty());
        assert_eq!(s.temper, 0);
        assert_eq!(s.bitvector, 2);
        assert_eq!(s.with_who, 112);
        assert_eq!(s.rooms, vec![2518]);
        assert_eq!(s.open1, 0);
        assert_eq!(s.close1, 28);
        assert!(s.messages[0].contains("Haven't got that"));
        assert!(s.messages[6].contains("I'll give ya"));
    }

    #[test]
    fn parses_multi_room_dual_shift() {
        // Synthetic: two rooms, dual shift, no producing list, no buy_types.
        let body = "\
#100~
-1
1.6
0.3
-1
.~
.~
.~
.~
.~
.~
.~
1
0
12016
0
12003
12004
-1
9
17
18
22
$~
";
        let shops = p(body);
        let s = &shops[0];
        assert_eq!(s.producing, Vec::<i32>::new());
        assert_eq!(s.buy_types, Vec::<String>::new());
        assert_eq!(s.rooms, vec![12003, 12004]);
        assert_eq!(s.open1, 9);
        assert_eq!(s.close1, 17);
        assert_eq!(s.open2, 18);
        assert_eq!(s.close2, 22);
    }

    #[test]
    fn parses_multiple_shops_in_one_file() {
        let body = "\
CircleMUD v3.0 Shop File~
#1~
-1
1.0
1.0
-1
.~
.~
.~
.~
.~
.~
.~
0
0
1
0
10
-1
0
24
0
0
#2~
-1
1.0
1.0
-1
.~
.~
.~
.~
.~
.~
.~
0
0
2
0
20
-1
0
24
0
0
$~
";
        let shops = p(body);
        assert_eq!(shops.len(), 2);
        assert_eq!(shops[0].vnum, 1);
        assert_eq!(shops[1].vnum, 2);
        assert_eq!(shops[0].keeper_vnum, 1);
        assert_eq!(shops[1].keeper_vnum, 2);
    }
}
