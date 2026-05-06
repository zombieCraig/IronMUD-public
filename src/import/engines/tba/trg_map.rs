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
        // 13 OTRIG_LOAD
        13 => Some(ItemTriggerType::OnLoad),
        // 18 OTRIG_CONSUME — closest IronMUD analog is OnUse (drink/eat).
        18 => Some(ItemTriggerType::OnUse),
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
        _ => None,
    }
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
        // `e` (ACT, mob) and `f` (RESET, room) currently have no IronMUD analog.
        assert_eq!(mobile_trigger_types("e").len(), 0);
        assert_eq!(room_trigger_types("f").len(), 0);
        // Truly unmapped letter for items.
        assert_eq!(item_trigger_types("p").len(), 0);
    }

    #[test]
    fn command_letter_maps_for_all_three() {
        // `c` = COMMAND across mob/obj/room.
        assert_eq!(mobile_trigger_types("c"), vec![MobileTriggerType::OnCommand]);
        assert_eq!(item_trigger_types("c"), vec![ItemTriggerType::OnCommand]);
        assert_eq!(room_trigger_types("c"), vec![TriggerType::OnCommand]);
    }
}
