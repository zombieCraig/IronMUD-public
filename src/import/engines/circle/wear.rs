//! CircleMUD `E`-reset wear-location index → IronMUD [`WearLocation`].
//!
//! CircleMUD `E` reset commands take a slot index (0..17) — distinct from the
//! ITEM_WEAR_* bitmask used in `.obj` files. Authoritative source:
//! `circle-3.1/src/structs.h` (`WEAR_LIGHT`..`WEAR_HOLD`). Slots that have no
//! IronMUD equivalent return `None` (the importer warns and drops the `E`).
//! Paired-slot Circle entries (LEGS/FEET/HANDS/ARMS) collapse to the left
//! variant — IronMUD models each foot/hand/etc. independently.
//!
//! Mapping rationale lives in `docs/import-guide.md` ("CircleMUD zone reset
//! coverage matrix").

use crate::types::WearLocation;

pub fn map_wear_loc(loc: i32) -> Option<WearLocation> {
    match loc {
        0 => None,                              // WEAR_LIGHT — no IronMUD analogue
        1 => Some(WearLocation::FingerRight),   // WEAR_FINGER_R
        2 => Some(WearLocation::FingerLeft),    // WEAR_FINGER_L
        3 => Some(WearLocation::Neck),          // WEAR_NECK_1
        4 => Some(WearLocation::Neck),          // WEAR_NECK_2 (collision; second item dropped per mob)
        5 => Some(WearLocation::Torso),         // WEAR_BODY
        6 => Some(WearLocation::Head),          // WEAR_HEAD
        7 => Some(WearLocation::LeftLeg),       // WEAR_LEGS — paired-slot collapse
        8 => Some(WearLocation::LeftFoot),      // WEAR_FEET — paired-slot collapse
        9 => Some(WearLocation::LeftHand),      // WEAR_HANDS — paired-slot collapse
        10 => Some(WearLocation::LeftArm),      // WEAR_ARMS — paired-slot collapse
        11 => Some(WearLocation::OffHand),      // WEAR_SHIELD
        12 => Some(WearLocation::Back),         // WEAR_ABOUT (cloak-like)
        13 => Some(WearLocation::Waist),        // WEAR_WAIST
        14 => Some(WearLocation::WristRight),   // WEAR_WRIST_R
        15 => Some(WearLocation::WristLeft),    // WEAR_WRIST_L
        16 => Some(WearLocation::Wielded),      // WEAR_WIELD
        17 => Some(WearLocation::Ready),        // WEAR_HOLD
        _ => None,
    }
}

/// Whether a slot is one of the four paired-slot collapses (LEGS/FEET/HANDS/ARMS).
/// Mapping emits an Info note when any of these fire so builders know
/// the right-side counterpart slot stayed empty.
pub fn is_paired_slot_collapse(loc: i32) -> bool {
    matches!(loc, 7 | 8 | 9 | 10)
}

/// Whether a slot is one of the two NECK variants (3 or 4). Mapping warns
/// once per mob if both are used so a dropped second neck-item is auditable.
pub fn is_neck_slot(loc: i32) -> bool {
    matches!(loc, 3 | 4)
}
