//! CircleMUD bitvector decoding and named-flag tables.
//!
//! Stock CircleMUD 3.x stores flag bits in two interchangeable formats inside
//! world files: the legacy decimal integer and an ASCII alphabet encoding
//! (`asciiflag_conv` in `db.c`). The ASCII format maps `a..z` to bits 0..25
//! and `A..Z` to bits 26..51, e.g. `abc` is bits 0|1|2 = 7.

/// Decode a CircleMUD bitvector field. Accepts both ASCII alpha encoding
/// (e.g. `abdo`) and the legacy decimal form (e.g. `15`). Empty / "0" → 0.
/// Unknown characters are ignored, matching the C parser's behavior.
pub fn parse_bitvector(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }
    // Decimal-only form (CircleMUD legacy and many patches still use it).
    if s.chars().all(|c| c.is_ascii_digit()) {
        return s.parse::<u64>().unwrap_or(0);
    }
    let mut bits: u64 = 0;
    for ch in s.chars() {
        match ch {
            'a'..='z' => bits |= 1u64 << (ch as u32 - 'a' as u32),
            'A'..='Z' => bits |= 1u64 << (26 + ch as u32 - 'A' as u32),
            _ => {}
        }
    }
    bits
}

/// Stock CircleMUD ROOM_* flag names by bit position (see `structs.h`).
/// Patches commonly add bits 16+; anything beyond the table surfaces as
/// `unknown_flag_names` to keep custom installs round-tripping cleanly.
pub const ROOM_FLAG_NAMES: &[&str] = &[
    "DARK",        // 0
    "DEATH",       // 1
    "NOMOB",       // 2
    "INDOORS",     // 3
    "PEACEFUL",    // 4
    "SOUNDPROOF",  // 5
    "NOTRACK",     // 6
    "NOMAGIC",     // 7
    "TUNNEL",      // 8
    "PRIVATE",     // 9
    "GODROOM",     // 10
    "HOUSE",       // 11
    "HOUSE_CRASH", // 12
    "ATRIUM",      // 13
    "OLC",         // 14
    "BFS_MARK",    // 15
];

pub fn decode_room_flags(bits: u64) -> (Vec<&'static str>, Vec<String>) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for bit in 0..64 {
        if bits & (1u64 << bit) == 0 {
            continue;
        }
        if let Some(name) = ROOM_FLAG_NAMES.get(bit) {
            known.push(*name);
        } else {
            unknown.push(format!("BIT_{bit}"));
        }
    }
    (known, unknown)
}

/// CircleMUD sector type → human-readable name. Used for warnings and the
/// dry-run report; the actual flag mapping is data-driven via the JSON table.
pub const SECTOR_NAMES: &[&str] = &[
    "INSIDE",       // 0
    "CITY",         // 1
    "FIELD",        // 2
    "FOREST",       // 3
    "HILLS",        // 4
    "MOUNTAIN",     // 5
    "WATER_SWIM",   // 6
    "WATER_NOSWIM", // 7
    "FLYING",       // 8
    "UNDERWATER",   // 9
];

pub fn sector_name(sector: i32) -> String {
    if sector >= 0 && (sector as usize) < SECTOR_NAMES.len() {
        SECTOR_NAMES[sector as usize].to_string()
    } else {
        format!("SECTOR_{sector}")
    }
}

// Exit info bits (structs.h).
pub const EX_ISDOOR: u32 = 1 << 0;
pub const EX_CLOSED: u32 = 1 << 1;
pub const EX_LOCKED: u32 = 1 << 2;
pub const EX_PICKPROOF: u32 = 1 << 3;

pub const EXIT_FLAG_NAMES: &[&str] = &["ISDOOR", "CLOSED", "LOCKED", "PICKPROOF"];

/// Stock CircleMUD MOB_* action-bit names by bit position (`structs.h`).
/// Patches commonly add bits beyond the table; anything outside surfaces
/// as `unrecognised mob flag bit N` during mapping.
pub const MOB_FLAG_NAMES: &[&str] = &[
    "SPEC",         // 0  has special procedure
    "SENTINEL",     // 1  never wanders
    "SCAVENGER",    // 2  picks up items
    "ISNPC",        // 3  implicit on all mobs
    "AWARE",        // 4  detects hidden players
    "AGGRESSIVE",   // 5  attacks on sight
    "STAY_ZONE",    // 6  won't leave zone
    "WIMPY",        // 7  flees at low HP
    "AGGR_EVIL",    // 8  attacks evil-aligned
    "AGGR_GOOD",    // 9  attacks good-aligned
    "AGGR_NEUTRAL", // 10 attacks neutral-aligned
    "MEMORY",       // 11 remembers attackers
    "HELPER",       // 12 assists groupmates
    "NOCHARM",      // 13 cannot be charmed
    "NOSUMMON",     // 14 cannot be summoned
    "NOSLEEP",      // 15 cannot be slept
    "NOBASH",       // 16 cannot be bashed
    "NOBLIND",      // 17 cannot be blinded
];

/// Stock CircleMUD AFF_* affected-by bit names (`structs.h`). Mostly
/// permanent buffs/debuffs that don't translate cleanly to IronMUD, so
/// the default mapping warns on each.
pub const AFF_FLAG_NAMES: &[&str] = &[
    "BLIND",         // 0
    "INVISIBLE",     // 1
    "DETECT_ALIGN",  // 2
    "DETECT_INVIS",  // 3
    "DETECT_MAGIC",  // 4
    "SENSE_LIFE",    // 5
    "WATERWALK",     // 6
    "SANCTUARY",     // 7
    "GROUP",         // 8  transient runtime flag
    "CURSE",         // 9
    "INFRAVISION",   // 10
    "POISON",        // 11
    "PROTECT_EVIL",  // 12
    "PROTECT_GOOD",  // 13
    "SLEEP",         // 14
    "NOTRACK",       // 15
    "UNUSED16",      // 16
    "UNUSED17",      // 17
    "SNEAK",         // 18
    "HIDE",          // 19
    "UNUSED20",      // 20
    "CHARM",         // 21
];

pub fn decode_mob_flags(bits: u64) -> (Vec<&'static str>, Vec<String>) {
    decode_named_bits(bits, MOB_FLAG_NAMES)
}

pub fn decode_aff_flags(bits: u64) -> (Vec<&'static str>, Vec<String>) {
    decode_named_bits(bits, AFF_FLAG_NAMES)
}

fn decode_named_bits(bits: u64, table: &'static [&'static str]) -> (Vec<&'static str>, Vec<String>) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for bit in 0..64 {
        if bits & (1u64 << bit) == 0 {
            continue;
        }
        if let Some(name) = table.get(bit) {
            known.push(*name);
        } else {
            unknown.push(format!("BIT_{bit}"));
        }
    }
    (known, unknown)
}

pub fn decode_exit_flags(bits: u32) -> (Vec<&'static str>, Vec<String>) {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for bit in 0..32 {
        if bits & (1 << bit) == 0 {
            continue;
        }
        if let Some(name) = EXIT_FLAG_NAMES.get(bit) {
            known.push(*name);
        } else {
            unknown.push(format!("BIT_{bit}"));
        }
    }
    (known, unknown)
}

/// Stock CircleMUD ITEM_TYPE constants by index (`structs.h`). Index 0 is
/// "UNDEFINED" and never appears on a real prototype — but we keep it so
/// `item_type_name(0)` doesn't underflow.
pub const ITEM_TYPE_NAMES: &[&str] = &[
    "UNDEFINED",   // 0
    "LIGHT",       // 1
    "SCROLL",      // 2
    "WAND",        // 3
    "STAFF",       // 4
    "WEAPON",      // 5
    "FIRE_WEAPON", // 6  (unimplemented in stock Circle)
    "MISSILE",     // 7  (unimplemented)
    "TREASURE",    // 8
    "ARMOR",       // 9
    "POTION",      // 10
    "WORN",        // 11 (unimplemented)
    "OTHER",       // 12
    "TRASH",       // 13
    "TRAP",        // 14 (unimplemented)
    "CONTAINER",   // 15
    "NOTE",        // 16
    "DRINKCON",    // 17
    "KEY",         // 18
    "FOOD",        // 19
    "MONEY",       // 20
    "PEN",         // 21
    "BOAT",        // 22
    "FOUNTAIN",    // 23
];

pub fn item_type_name(t: i32) -> String {
    if t >= 0 && (t as usize) < ITEM_TYPE_NAMES.len() {
        ITEM_TYPE_NAMES[t as usize].to_string()
    } else {
        format!("ITEM_TYPE_{t}")
    }
}

/// Stock CircleMUD ITEM_* (extra) bit names by bit position (`structs.h`).
/// Bits ≥ 17 are patched extensions; surface as warnings so they don't get
/// silently dropped.
pub const EXTRA_BIT_NAMES: &[&str] = &[
    "GLOW",         // 0
    "HUM",          // 1
    "NORENT",       // 2
    "NODONATE",     // 3
    "NOINVIS",      // 4
    "INVISIBLE",    // 5
    "MAGIC",        // 6
    "NODROP",       // 7
    "BLESS",        // 8
    "ANTI_GOOD",    // 9
    "ANTI_EVIL",    // 10
    "ANTI_NEUTRAL", // 11
    "ANTI_MAGE",    // 12
    "ANTI_CLERIC",  // 13
    "ANTI_THIEF",   // 14
    "ANTI_WARRIOR", // 15
    "NOSELL",       // 16
];

pub fn decode_extra_flags(bits: u64) -> (Vec<&'static str>, Vec<String>) {
    decode_named_bits(bits, EXTRA_BIT_NAMES)
}

/// Stock CircleMUD ITEM_WEAR_* bit names (`structs.h`). The mapping from
/// these bits to IronMUD `WearLocation`s is hard-coded (in mapping.rs) since
/// the right-hand side is a list, not a single flag.
pub const WEAR_BIT_NAMES: &[&str] = &[
    "TAKE",   // 0  implicit; objects without it cannot be picked up
    "FINGER", // 1
    "NECK",   // 2
    "BODY",   // 3
    "HEAD",   // 4
    "LEGS",   // 5
    "FEET",   // 6
    "HANDS",  // 7
    "ARMS",   // 8
    "SHIELD", // 9
    "ABOUT",  // 10
    "WAIST",  // 11
    "WRIST",  // 12
    "WIELD",  // 13
    "HOLD",   // 14
];

pub fn decode_wear_flags(bits: u64) -> (Vec<&'static str>, Vec<String>) {
    decode_named_bits(bits, WEAR_BIT_NAMES)
}

/// CircleMUD APPLY_* names indexed by location number (used in object `A`
/// blocks: `A\n<location> <modifier>`). See `constants.c::apply_types`.
pub const APPLY_TYPE_NAMES: &[&str] = &[
    "NONE",          // 0
    "STR",           // 1
    "DEX",           // 2
    "INT",           // 3
    "WIS",           // 4
    "CON",           // 5
    "CHA",           // 6
    "CLASS",         // 7  (unimplemented in stock Circle)
    "LEVEL",         // 8  (unimplemented)
    "AGE",           // 9
    "CHAR_WEIGHT",   // 10
    "CHAR_HEIGHT",   // 11
    "MAXMANA",       // 12
    "MAXHIT",        // 13
    "MAXMOVE",       // 14
    "GOLD",          // 15 (unimplemented)
    "EXP",           // 16 (unimplemented)
    "ARMOR",         // 17
    "HITROLL",       // 18
    "DAMROLL",       // 19
    "SAVING_PARA",   // 20
    "SAVING_ROD",    // 21
    "SAVING_PETRI",  // 22
    "SAVING_BREATH", // 23
    "SAVING_SPELL",  // 24
];

pub fn apply_type_name(loc: i32) -> String {
    if loc >= 0 && (loc as usize) < APPLY_TYPE_NAMES.len() {
        APPLY_TYPE_NAMES[loc as usize].to_string()
    } else {
        format!("APPLY_{loc}")
    }
}

/// CircleMUD `LIQ_*` index → drink name (`constants.c::drinks`). The mapping
/// layer turns this into an `IronMUD::LiquidType`.
pub const LIQUID_NAMES: &[&str] = &[
    "water",            // 0
    "beer",             // 1
    "wine",             // 2
    "ale",              // 3
    "dark ale",         // 4
    "whisky",           // 5
    "lemonade",         // 6
    "firebreather",     // 7
    "local speciality", // 8
    "slime mold juice", // 9
    "milk",             // 10
    "tea",              // 11
    "coffee",           // 12
    "blood",            // 13
    "salt water",       // 14
    "clear water",      // 15
];

/// CircleMUD weapon "damage type" index used in WEAPON value3. Maps to the
/// verb the engine prints in attack messages. Values past 14 fall back to
/// "hit". The mapping layer turns these into IronMUD `DamageType`.
pub const WEAPON_DAMAGE_VERBS: &[&str] = &[
    "hit",      // 0
    "sting",    // 1
    "whip",     // 2
    "slash",    // 3
    "bite",     // 4
    "bludgeon", // 5
    "crush",    // 6
    "pound",    // 7
    "claw",     // 8
    "maul",     // 9
    "thrash",   // 10
    "pierce",   // 11
    "blast",    // 12
    "punch",    // 13
    "stab",     // 14
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bitvector_decimal() {
        assert_eq!(parse_bitvector("0"), 0);
        assert_eq!(parse_bitvector("5"), 5);
        assert_eq!(parse_bitvector(""), 0);
    }

    #[test]
    fn parse_bitvector_ascii() {
        // 'a' = bit 0, 'b' = bit 1, 'c' = bit 2 => 0b111 = 7
        assert_eq!(parse_bitvector("abc"), 0b111);
        // 'A' = bit 26
        assert_eq!(parse_bitvector("A"), 1 << 26);
    }

    #[test]
    fn decode_room_flags_handles_unknown_bits() {
        let (known, unknown) = decode_room_flags(1 | (1 << 20));
        assert_eq!(known, vec!["DARK"]);
        assert_eq!(unknown, vec!["BIT_20"]);
    }
}
