//! DG Scripts trigger flag → IronMUD `TriggerType` mapping.
//!
//! tbamud encodes trigger event flags as `a`-`z`/`A`-`Z` letters where
//! each letter is a bit position (a=0, b=1, …, z=25, A=26, …, Z=51) per
//! `db.c::asciiflag_conv`. The bit layout per attach type comes from
//! `dg_scripts.h`:
//!
//! ```text
//! Mob  (attach=0): a=GLOBAL b=RANDOM c=COMMAND d=SPEECH e=ACT f=DEATH
//!                  g=GREET h=GREET_ALL i=ENTRY j=RECEIVE k=FIGHT l=HITPRCNT
//!                  m=BRIBE n=LOAD o=MEMORY p=CAST q=LEAVE r=DOOR
//!                  t=TIME u=DAMAGE
//!
//! Obj  (attach=1): a=GLOBAL b=RANDOM c=COMMAND f=TIMER g=GET h=DROP
//!                  i=GIVE j=WEAR l=REMOVE n=LOAD p=CAST q=LEAVE
//!                  s=CONSUME t=TIME
//!
//! Room (attach=2): a=GLOBAL b=RANDOM c=COMMAND d=SPEECH f=RESET g=ENTER
//!                  h=DROP p=CAST q=LEAVE r=DOOR s=LOGIN t=TIME
//! ```
//!
//! Phase 1 only maps the bits that have a native IronMUD `TriggerType`
//! analog. Unmapped letters are dropped silently — the parent record
//! still attaches via any letter that *does* map.

use crate::types::{ItemTriggerType, MobileTriggerType, TriggerType};

/// Walk every letter in `flag_str` and map it to a [`TriggerType`].
/// Returns the deduped set of room trigger types implied. Empty when
/// no letter maps (caller should emit an Info warning).
pub fn room_trigger_types(flag_str: &str) -> Vec<TriggerType> {
    let mut out = Vec::new();
    for c in flag_str.chars() {
        let bit = letter_bit(c);
        if let Some(b) = bit {
            if let Some(t) = bit_to_room(b) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

pub fn item_trigger_types(flag_str: &str) -> Vec<ItemTriggerType> {
    let mut out = Vec::new();
    for c in flag_str.chars() {
        let bit = letter_bit(c);
        if let Some(b) = bit {
            if let Some(t) = bit_to_item(b) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

pub fn mobile_trigger_types(flag_str: &str) -> Vec<MobileTriggerType> {
    let mut out = Vec::new();
    for c in flag_str.chars() {
        let bit = letter_bit(c);
        if let Some(b) = bit {
            if let Some(t) = bit_to_mobile(b) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

fn letter_bit(c: char) -> Option<u32> {
    if c.is_ascii_lowercase() {
        Some(c as u32 - 'a' as u32)
    } else if c.is_ascii_uppercase() {
        Some(26 + (c as u32 - 'A' as u32))
    } else {
        None
    }
}

fn bit_to_room(bit: u32) -> Option<TriggerType> {
    match bit {
        // 1 RANDOM → Periodic
        1 => Some(TriggerType::Periodic),
        // 2 WTRIG_COMMAND
        2 => Some(TriggerType::OnCommand),
        // 6 WTRIG_ENTER
        6 => Some(TriggerType::OnEnter),
        // 16 WTRIG_LEAVE
        16 => Some(TriggerType::OnExit),
        // 19 WTRIG_TIME
        19 => Some(TriggerType::OnTimeChange),
        // IronMUD-native (bits stock tbamud doesn't define for rooms):
        // OnLook (y=24), WeatherChange (w=22), MonthChange (m=12 unused
        // for rooms in stock), SeasonChange (z=25 — moved off 18=LOGIN).
        // These only fire on the proto round-trip path from
        // `flags_for_room_trigger`; stock imports never produce them.
        24 => Some(TriggerType::OnLook),
        22 => Some(TriggerType::OnWeatherChange),
        12 => Some(TriggerType::OnMonthChange),
        25 => Some(TriggerType::OnSeasonChange),
        _ => None,
    }
}

fn bit_to_item(bit: u32) -> Option<ItemTriggerType> {
    match bit {
        // 2 OTRIG_COMMAND
        2 => Some(ItemTriggerType::OnCommand),
        // 6 OTRIG_GET
        6 => Some(ItemTriggerType::OnGet),
        // 7 OTRIG_DROP
        7 => Some(ItemTriggerType::OnDrop),
        // 9 OTRIG_WEAR — IronMUD-native OnWear. tbamud bit name is
        // OTRIG_GIVE in stock; we co-opt it for OnWear so promoted
        // host-local triggers round-trip through `flags_for_item_trigger`.
        9 => Some(ItemTriggerType::OnWear),
        // 11 OTRIG_REMOVE — IronMUD-native counterpart to WEAR.
        11 => Some(ItemTriggerType::OnRemove),
        // 13 OTRIG_LOAD
        13 => Some(ItemTriggerType::OnLoad),
        // 18 OTRIG_CONSUME — closest IronMUD analog is OnUse (drink/eat).
        18 => Some(ItemTriggerType::OnUse),
        // IronMUD-native: OnExamine (x=23), OnLook (y=24), OnPrompt (z=25).
        // No tbamud analog; only matter when a builder promotes a
        // host-local trigger to a proto.
        23 => Some(ItemTriggerType::OnExamine),
        24 => Some(ItemTriggerType::OnLook),
        25 => Some(ItemTriggerType::OnPrompt),
        _ => None,
    }
}

fn bit_to_mobile(bit: u32) -> Option<MobileTriggerType> {
    match bit {
        // 1 MTRIG_RANDOM → OnIdle (closest periodic-ish hook).
        1 => Some(MobileTriggerType::OnIdle),
        // 2 MTRIG_COMMAND
        2 => Some(MobileTriggerType::OnCommand),
        // 3 MTRIG_SPEECH → OnSay
        3 => Some(MobileTriggerType::OnSay),
        // 5 MTRIG_DEATH
        5 => Some(MobileTriggerType::OnDeath),
        // 6 MTRIG_GREET, 7 MTRIG_GREET_ALL
        6 | 7 => Some(MobileTriggerType::OnGreet),
        // 9 MTRIG_RECEIVE
        9 => Some(MobileTriggerType::OnReceive),
        // 10 MTRIG_FIGHT
        10 => Some(MobileTriggerType::OnFight),
        // 11 MTRIG_HITPRCNT
        11 => Some(MobileTriggerType::OnHitPercent),
        // 12 MTRIG_BRIBE
        12 => Some(MobileTriggerType::OnBribe),
        // 13 MTRIG_LOAD
        13 => Some(MobileTriggerType::OnLoad),
        // IronMUD-native (no stock tbamud bit). Use bits that stock
        // tbamud doesn't define (s=18 onward) so importing real .trg
        // content never accidentally promotes to one of these types.
        // Mirrors `flags_for_mobile_trigger` for round-trip on
        // promoted host-local triggers.
        21 => Some(MobileTriggerType::OnAttack),
        22 => Some(MobileTriggerType::OnAlways),
        23 => Some(MobileTriggerType::OnFlee),
        _ => None,
    }
}

/// Reverse mapping for `MobileTriggerType` → single letter flag. Used by
/// `dg_makeproto_from_mobile_trigger` to round-trip a host-local trigger
/// into a proto whose flags will re-derive the same trigger type on attach.
pub fn flags_for_mobile_trigger(t: MobileTriggerType) -> String {
    match t {
        MobileTriggerType::OnIdle => "b",
        MobileTriggerType::OnCommand => "c",
        MobileTriggerType::OnSay => "d",
        MobileTriggerType::OnDeath => "f",
        MobileTriggerType::OnGreet => "g",
        MobileTriggerType::OnReceive => "j",
        MobileTriggerType::OnFight => "k",
        MobileTriggerType::OnHitPercent => "l",
        MobileTriggerType::OnBribe => "m",
        MobileTriggerType::OnLoad => "n",
        // No clean tbamud analog. We pick bits that stock tbamud
        // never uses (s=18 onward) so importing stock `.trg` files
        // with one of these letters still drops silently as Info
        // warnings rather than promoting to an IronMUD-native type.
        MobileTriggerType::OnAttack => "v",   // bit 21 (unused in stock)
        MobileTriggerType::OnAlways => "w",   // bit 22 (unused in stock)
        MobileTriggerType::OnFlee => "x",     // bit 23 (unused in stock)
    }
    .to_string()
}

/// Reverse mapping for `ItemTriggerType` → single letter flag.
pub fn flags_for_item_trigger(t: ItemTriggerType) -> String {
    match t {
        ItemTriggerType::OnCommand => "c",
        ItemTriggerType::OnGet => "g",
        ItemTriggerType::OnDrop => "h",
        ItemTriggerType::OnLoad => "n",
        ItemTriggerType::OnUse => "s",
        // IronMUD-native types with no tbamud bit — use unused letters
        // so the round-trip stays distinct. These never appear in stock
        // imports, only in host-local triggers being promoted to protos.
        ItemTriggerType::OnExamine => "x",
        ItemTriggerType::OnLook => "y",
        ItemTriggerType::OnPrompt => "z",
        ItemTriggerType::OnWear => "j",
        ItemTriggerType::OnRemove => "l",
    }
    .to_string()
}

/// Reverse mapping for `TriggerType` → single letter flag.
pub fn flags_for_room_trigger(t: TriggerType) -> String {
    match t {
        TriggerType::Periodic => "b",
        TriggerType::OnCommand => "c",
        TriggerType::OnEnter => "g",
        TriggerType::OnExit => "q",
        TriggerType::OnTimeChange => "t",
        // IronMUD-native — bits stock tbamud doesn't define for rooms.
        TriggerType::OnLook => "y",            // 24
        TriggerType::OnWeatherChange => "w",   // 22
        TriggerType::OnSeasonChange => "z",    // 25 (s=18 is stock LOGIN)
        TriggerType::OnMonthChange => "m",     // 12 (unused for rooms in stock)
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_map_to_bits() {
        assert_eq!(letter_bit('a'), Some(0));
        assert_eq!(letter_bit('z'), Some(25));
        assert_eq!(letter_bit('A'), Some(26));
        assert_eq!(letter_bit('Z'), Some(51));
        assert_eq!(letter_bit('1'), None);
    }

    #[test]
    fn room_enter_letter() {
        // `2 g 100` from 52.trg #5200 — `g` = bit 6 = WTRIG_ENTER.
        assert_eq!(room_trigger_types("g"), vec![TriggerType::OnEnter]);
    }

    #[test]
    fn obj_get_drop_letters() {
        assert_eq!(item_trigger_types("g"), vec![ItemTriggerType::OnGet]);
        assert_eq!(item_trigger_types("h"), vec![ItemTriggerType::OnDrop]);
        // Combined: both flags set.
        assert_eq!(item_trigger_types("gh"), vec![ItemTriggerType::OnGet, ItemTriggerType::OnDrop]);
    }

    #[test]
    fn mob_greet_letters() {
        assert_eq!(mobile_trigger_types("g"), vec![MobileTriggerType::OnGreet]);
        assert_eq!(mobile_trigger_types("h"), vec![MobileTriggerType::OnGreet]);
        // h+g dedupe to one.
        assert_eq!(mobile_trigger_types("gh"), vec![MobileTriggerType::OnGreet]);
    }

    #[test]
    fn unsupported_letters_drop_silently() {
        // `a` (GLOBAL across all 3 kinds) has no IronMUD analog and is a
        // safe "nothing maps" probe for all three forward maps.
        assert_eq!(mobile_trigger_types("a").len(), 0);
        assert_eq!(room_trigger_types("a").len(), 0);
        assert_eq!(item_trigger_types("a").len(), 0);
        // `f` (RESET, room) likewise unmapped.
        assert_eq!(room_trigger_types("f").len(), 0);
        // `p` (CAST) unmapped for items.
        assert_eq!(item_trigger_types("p").len(), 0);
    }

    #[test]
    fn ironmud_native_letters_round_trip() {
        // Letters claimed for IronMUD-native trigger types (no stock
        // tbamud analog) must round-trip cleanly through
        // `flags_for_*` → `*_trigger_types` so promoted host-local
        // triggers stay structurally intact across a proto refresh sweep.
        // Item OnWear (j=9) / OnRemove (l=11).
        assert_eq!(item_trigger_types("j"), vec![ItemTriggerType::OnWear]);
        assert_eq!(item_trigger_types("l"), vec![ItemTriggerType::OnRemove]);
        assert_eq!(
            item_trigger_types(&flags_for_item_trigger(ItemTriggerType::OnWear)),
            vec![ItemTriggerType::OnWear]
        );
        // Mob OnAttack (e=4) / OnAlways (o=14) / OnFlee (q=16).
        assert_eq!(
            mobile_trigger_types(&flags_for_mobile_trigger(MobileTriggerType::OnAttack)),
            vec![MobileTriggerType::OnAttack]
        );
        assert_eq!(
            mobile_trigger_types(&flags_for_mobile_trigger(MobileTriggerType::OnAlways)),
            vec![MobileTriggerType::OnAlways]
        );
        // Room WeatherChange (w=22) / MonthChange (m=12).
        assert_eq!(
            room_trigger_types(&flags_for_room_trigger(TriggerType::OnWeatherChange)),
            vec![TriggerType::OnWeatherChange]
        );
        assert_eq!(
            room_trigger_types(&flags_for_room_trigger(TriggerType::OnMonthChange)),
            vec![TriggerType::OnMonthChange]
        );
    }

    #[test]
    fn command_letter_maps_for_all_three() {
        // `c` = COMMAND across mob/obj/room.
        assert_eq!(mobile_trigger_types("c"), vec![MobileTriggerType::OnCommand]);
        assert_eq!(item_trigger_types("c"), vec![ItemTriggerType::OnCommand]);
        assert_eq!(room_trigger_types("c"), vec![TriggerType::OnCommand]);
    }
}
