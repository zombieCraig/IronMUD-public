
use crate::import::{
    IrItem,
    IrZone, MappingOptions, PlannedItem, Severity, Warning, WarningKind,
};
use crate::types::{
    CastOnUse, DamageType, ExtraDesc, ItemData, ItemFlags, ItemType, LiquidType, WearLocation,
};

use super::{FlagAction, lookup_circle_spell};

/// Map an `IrItem` to an IronMUD `ItemData` prototype plus warnings for
/// anything we couldn't translate cleanly. This is the item analogue of
/// `map_room` / `map_mob`. The pipeline:
///   1. base `ItemData` from name/short/long
///   2. ITEM_TYPE → `ItemType` and type-specific value handling
///   3. ITEM_* extra-bit decode via the JSON action table
///   4. ITEM_WEAR_* decode (hard-coded; the right-hand side is a Vec)
///   5. APPLY_* affect decode via the JSON action table
///   6. extra descriptions surface as a single `DeferredFeature` warning
pub(super) fn map_item(zone: &IrZone, area_prefix: &str, item: &IrItem, opts: &MappingOptions) -> (PlannedItem, Vec<Warning>) {
    let _ = zone;
    let mut warnings = Vec::new();

    let mut data = ItemData::new(
        item.short_descr.clone(),
        item.short_descr.clone(),
        if item.long_descr.is_empty() {
            item.short_descr.clone()
        } else {
            item.long_descr.clone()
        },
    );
    data.is_prototype = true;
    let vnum = format!("{}_{}", area_prefix, item.vnum);
    data.vnum = Some(vnum.clone());
    data.keywords = item.keywords.clone();
    data.weight = item.weight.max(0);
    data.value = item.cost.max(0);

    if !item.action_descr.is_empty() {
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            item.source.clone(),
            "action description present (CircleMUD use-message); discarded — IronMUD has no analogue".to_string(),
        ));
    }

    // Type-specific decode: sets ItemType and any value-derived fields.
    apply_item_type(item, &mut data, &mut warnings, area_prefix);

    // ITEM_* extra-bit decode via the JSON table.
    let (extra_known, extra_unknown) = crate::import::engines::circle::flags::decode_extra_flags(item.extra_flag_bits);
    for flag in extra_known {
        match opts.circle.extra_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_item_flag(&mut data.flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        item.source.clone(),
                        format!("mapping points ITEM_{flag} → {ironmud_flag}, but no such IronMUD ItemFlag"),
                    ));
                }
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!("ITEM_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetCombatZone { .. })
            | Some(FlagAction::SetStat { .. })
            | Some(FlagAction::SetArmorClass { .. })
            | Some(FlagAction::SetHitBonus)
            | Some(FlagAction::SetDamageBonus)
            | Some(FlagAction::SetMaxHpBonus)
            | Some(FlagAction::SetMaxManaBonus)
            | Some(FlagAction::AddBuff { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!(
                        "mapping uses an action that doesn't apply to extra-bits (ITEM_{flag}); ignored"
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                item.source.clone(),
                format!("no mapping for ITEM_{flag}"),
            )),
        }
    }
    for u in extra_unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            item.source.clone(),
            format!("unrecognised extra-flag bit {u} (likely a patched flag)"),
        ));
    }

    // ITEM_WEAR_* decode. The right-hand side is a Vec<WearLocation> per
    // CircleMUD bit, so we hard-code rather than going through the JSON.
    let (wear_known, wear_unknown) = crate::import::engines::circle::flags::decode_wear_flags(item.wear_flag_bits);
    let mut wear_locations: Vec<WearLocation> = Vec::new();
    let mut takeable = false;
    for flag in wear_known {
        match flag {
            "TAKE" => takeable = true,
            "FINGER" => {
                wear_locations.push(WearLocation::FingerLeft);
                wear_locations.push(WearLocation::FingerRight);
            }
            "NECK" => wear_locations.push(WearLocation::Neck),
            "BODY" => wear_locations.push(WearLocation::Torso),
            "HEAD" => wear_locations.push(WearLocation::Head),
            "LEGS" => {
                wear_locations.push(WearLocation::LeftLeg);
                wear_locations.push(WearLocation::RightLeg);
            }
            "FEET" => {
                wear_locations.push(WearLocation::LeftFoot);
                wear_locations.push(WearLocation::RightFoot);
            }
            "HANDS" => {
                wear_locations.push(WearLocation::LeftHand);
                wear_locations.push(WearLocation::RightHand);
            }
            "ARMS" => {
                wear_locations.push(WearLocation::LeftArm);
                wear_locations.push(WearLocation::RightArm);
            }
            "SHIELD" => wear_locations.push(WearLocation::OffHand),
            "ABOUT" => wear_locations.push(WearLocation::Back),
            "WAIST" => wear_locations.push(WearLocation::Waist),
            "WRIST" => {
                wear_locations.push(WearLocation::WristLeft);
                wear_locations.push(WearLocation::WristRight);
            }
            "WIELD" => wear_locations.push(WearLocation::Wielded),
            "HOLD" => wear_locations.push(WearLocation::Ready),
            _ => {}
        }
    }
    for u in wear_unknown {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            item.source.clone(),
            format!("unrecognised wear-flag bit {u} (likely a patched flag)"),
        ));
    }
    if !takeable {
        // Stock CircleMUD uses !TAKE for fixtures and signs. IronMUD has no
        // "fixed in place" notion, so the import surfaces an Info note.
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            item.source.clone(),
            "ITEM_WEAR_TAKE absent — IronMUD has no immovable-item flag; imported as takeable"
                .to_string(),
        ));
    }
    data.wear_locations = wear_locations;

    // APPLY_* affect decode.
    for (loc, modifier) in &item.affects {
        if *loc == 0 {
            continue; // APPLY_NONE — slot exists but is unused.
        }
        let name = crate::import::engines::circle::flags::apply_type_name(*loc);
        match opts.circle.apply_actions.get(&name) {
            Some(FlagAction::SetStat { ironmud_stat }) => {
                if !apply_named_item_stat(&mut data, ironmud_stat, *modifier) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        item.source.clone(),
                        format!("mapping points APPLY_{name} → {ironmud_stat}, but no such ItemData stat field"),
                    ));
                }
            }
            Some(FlagAction::SetArmorClass { .. }) => {
                // Circle: negative-is-better. IronMUD: positive damage
                // reduction. Sign-flip preserves the relative ordering.
                let prior = data.armor_class.unwrap_or(0);
                data.armor_class = Some(prior + (-modifier));
            }
            Some(FlagAction::SetHitBonus) => {
                data.hit_bonus += modifier;
            }
            Some(FlagAction::SetDamageBonus) => {
                data.damage_bonus += modifier;
            }
            Some(FlagAction::SetMaxHpBonus) => {
                data.max_hp_bonus += modifier;
            }
            Some(FlagAction::SetMaxManaBonus) => {
                data.max_mana_bonus += modifier;
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!("APPLY_{name} ({modifier:+}): {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetFlag { .. })
            | Some(FlagAction::SetCombatZone { .. })
            | Some(FlagAction::AddBuff { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    item.source.clone(),
                    format!(
                        "mapping uses an action that doesn't apply to APPLY_* (APPLY_{name}); ignored"
                    ),
                ));
            }
            // SetHitBonus/SetDamageBonus handled above in their own arms.
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                item.source.clone(),
                format!("no mapping for APPLY_{name} ({modifier:+}) — affect dropped"),
            )),
        }
    }

    // Extra descriptions: copy 1:1 to ItemData.extra_descs (mirrors room handling
    // at map_room above). Surfaced via `look <keyword>` against the item.
    data.extra_descs = item
        .extra_descs
        .iter()
        .map(|e| ExtraDesc {
            keywords: e.keywords.clone(),
            description: e.description.clone(),
        })
        .collect();

    // Sync flag-driven categories (e.g. flags.magical → categories ["magical"]).
    data.sync_flag_categories();

    (
        PlannedItem {
            area_prefix: area_prefix.to_string(),
            source_vnum: item.vnum,
            vnum,
            data,
            source: item.source.clone(),
        },
        warnings,
    )
}

/// Apply CircleMUD ITEM_TYPE-specific value semantics to `data`. Each branch
/// sets ItemType and type-relevant fields; lossy bits surface as warnings.
pub(super) fn apply_item_type(item: &IrItem, data: &mut ItemData, warnings: &mut Vec<Warning>, area_prefix: &str) {
    let v = item.values;
    let type_name = crate::import::engines::circle::flags::item_type_name(item.item_type);
    match item.item_type {
        // ITEM_LIGHT — capacity hours map onto `light_hours_remaining`.
        // CircleMUD convention: v[2] == -1 (or 0) means permanent torch; positive = hours.
        1 => {
            data.item_type = ItemType::Misc;
            data.flags.provides_light = true;
            data.light_hours_remaining = if v[2] > 0 { v[2] } else { 0 };
        }
        // ITEM_SCROLL — IronMUD scrolls *teach* (learn-on-read) rather than
        // cast-on-use. We map Circle's first spell slot (`v[1]`) into
        // `teaches_spell` when the spell is known; warn for additional slots.
        2 => {
            data.item_type = ItemType::Misc;
            if let Some(mapped) = lookup_circle_spell(v[1]) {
                data.teaches_spell = Some(mapped.to_string());
            } else if v[1] != 0 {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedValueSemantic,
                    Severity::Warn,
                    item.source.clone(),
                    format!("ITEM_SCROLL references unmapped Circle spell #{} (slot 1)", v[1]),
                ));
            }
            for (slot, num) in [(2, v[2]), (3, v[3])] {
                if num != 0 {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedValueSemantic,
                        Severity::Warn,
                        item.source.clone(),
                        format!(
                            "ITEM_SCROLL slot {} (Circle spell #{}) dropped — IronMUD scrolls teach a single spell.",
                            slot, num
                        ),
                    ));
                }
            }
        }
        // ITEM_WAND / STAFF — single-spell, charge-based items. v[0]=min level,
        // v[1]=spell number, v[2]=max charges, v[3]=current charges.
        3 | 4 => {
            data.item_type = if item.item_type == 3 { ItemType::Wand } else { ItemType::Staff };
            if let Some(mapped) = lookup_circle_spell(v[1]) {
                let max_charges = v[2].max(0);
                let charges = v[3].max(0).min(max_charges.max(0));
                data.cast_on_use = Some(CastOnUse {
                    spell: mapped.to_string(),
                    min_level: v[0].max(0),
                    charges,
                    max_charges,
                    cooldown_secs: None,
                });
            } else {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedValueSemantic,
                    Severity::Warn,
                    item.source.clone(),
                    format!(
                        "ITEM_{type_name} references unmapped Circle spell #{}; cast_on_use left empty (item becomes a dud)",
                        v[1]
                    ),
                ));
            }
        }
        // ITEM_WEAPON — values map cleanly to damage dice + damage type.
        5 => {
            data.item_type = ItemType::Weapon;
            data.damage_dice_count = v[1].max(0);
            data.damage_dice_sides = v[2].max(0);
            data.damage_type = circle_weapon_damage_type(v[3]);
        }
        // ITEM_FIRE_WEAPON / MISSILE — unimplemented in stock Circle.
        6 | 7 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!("ITEM_{type_name} unimplemented in stock CircleMUD; imported as Misc"),
            ));
        }
        // ITEM_TREASURE — Misc + categorised so shopkeeper / craft logic can
        // find it later.
        8 => {
            data.item_type = ItemType::Misc;
            data.categories.push("treasure".to_string());
        }
        // ITEM_ARMOR — v0 is the AC bonus. Sign-flip (negative-is-better in
        // Circle, positive-is-better in IronMUD).
        9 => {
            data.item_type = ItemType::Armor;
            data.armor_class = Some(-v[0]);
        }
        // ITEM_POTION — single-spell, single-charge magical potion. v[0]=min level
        // (ignored; potions are universal in IronMUD), v[1]=primary spell. We
        // use the first spell slot and warn for slots 2-3.
        10 => {
            data.item_type = ItemType::Potion;
            if let Some(mapped) = lookup_circle_spell(v[1]) {
                data.cast_on_use = Some(CastOnUse {
                    spell: mapped.to_string(),
                    min_level: 0,
                    charges: 1,
                    max_charges: 1,
                    cooldown_secs: None,
                });
            } else if v[1] != 0 {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedValueSemantic,
                    Severity::Warn,
                    item.source.clone(),
                    format!("ITEM_POTION references unmapped Circle spell #{}; cast_on_use left empty", v[1]),
                ));
            }
            for (slot, num) in [(2, v[2]), (3, v[3])] {
                if num != 0 {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedValueSemantic,
                        Severity::Warn,
                        item.source.clone(),
                        format!(
                            "ITEM_POTION slot {} (Circle spell #{}) dropped — only the primary spell is imported.",
                            slot, num
                        ),
                    ));
                }
            }
        }
        // ITEM_WORN — unimplemented in stock; treat as Misc and let
        // wear_locations carry the slot info.
        11 => {
            data.item_type = ItemType::Misc;
        }
        // ITEM_OTHER — clean Misc.
        12 => {
            data.item_type = ItemType::Misc;
        }
        // ITEM_TRASH — Misc + categories tag.
        13 => {
            data.item_type = ItemType::Misc;
            data.categories.push("trash".to_string());
        }
        // ITEM_TRAP — unimplemented in stock CircleMUD.
        14 => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                "ITEM_TRAP unimplemented in stock CircleMUD; imported as Misc".to_string(),
            ));
        }
        // ITEM_CONTAINER — full-fidelity mapping.
        15 => {
            data.item_type = ItemType::Container;
            data.container_max_weight = v[0].max(0);
            // v1 is a small numeric bitvector (closeable/pickproof/closed/locked).
            let bits = v[1] as u32;
            // bit 0 (1) = CLOSEABLE — IronMUD has no "non-closeable" notion; drop.
            if bits & 0b0010 != 0 {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedDoorFlag,
                    Severity::Warn,
                    item.source.clone(),
                    "container PICKPROOF flag not modeled; treated as locked".to_string(),
                ));
            }
            if bits & 0b0100 != 0 {
                data.container_closed = true;
            }
            if bits & 0b1000 != 0 {
                data.container_locked = true;
            }
            // v2 is the key vnum (or -1 for "no key"). Rewrite to the
            // prefixed IronMUD form.
            if v[2] > 0 {
                data.container_key_vnum = Some(format!("{}_{}", area_prefix, v[2]));
            }
        }
        // ITEM_NOTE — blank or pre-authored paper. Players with a Pen in
        // their inventory can use `write <paper>` to set `note_content`.
        // Circle's per-language literacy gating is not modeled.
        16 => {
            data.item_type = ItemType::Note;
        }
        // ITEM_DRINKCON — full mapping; v2 indexes the drink table.
        17 => {
            data.item_type = ItemType::LiquidContainer;
            data.liquid_max = v[0].max(0);
            data.liquid_current = v[1].max(0);
            let (lt, info) = circle_liquid_index_to_type(v[2]);
            data.liquid_type = lt;
            data.liquid_poisoned = v[3] != 0;
            if let Some(msg) = info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    item.source.clone(),
                    msg,
                ));
            }
        }
        // ITEM_KEY — clean.
        18 => {
            data.item_type = ItemType::Key;
        }
        // ITEM_FOOD — v0 = hours of hunger satisfied → nutrition; v3 = poisoned.
        19 => {
            data.item_type = ItemType::Food;
            data.food_nutrition = v[0].max(0);
            data.food_poisoned = v[3] != 0;
        }
        // ITEM_MONEY — Gold; v0 = number of coins.
        20 => {
            data.item_type = ItemType::Gold;
            data.value = v[0].max(0);
        }
        // ITEM_PEN — writing tool. Required to be in the writer's
        // inventory to use `write <paper>`.
        21 => {
            data.item_type = ItemType::Pen;
        }
        // ITEM_BOAT — Misc + IronMUD's flags.boat which already exists.
        22 => {
            data.item_type = ItemType::Misc;
            data.flags.boat = true;
        }
        // ITEM_BOARD — bulletin board. Stock CircleMUD vnums 3096..=3099 ride
        // hardcoded levels from `gen_board.c::board_info[]` (NOT stored in
        // .obj values), so we look up the per-vnum preset; non-stock boards
        // default to all-public + engine default (60-post) cap. The actual
        // post storage lives in IronMUD's `boards` sled tree, keyed by vnum.
        24 => {
            data.item_type = ItemType::Board;
            let preset = stock_board_preset(item.vnum);
            data.board_read_admin_only = preset.read_admin_only;
            data.board_write_admin_only = preset.write_admin_only;
            data.board_max_messages = preset.max_messages;
            if !preset.is_stock {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    item.source.clone(),
                    format!(
                        "ITEM_BOARD vnum {} not in stock-board preset table; defaulting to public read/write. Tune via `oedit {} board_read_admin|board_write_admin|board_max`.",
                        item.vnum, item.vnum
                    ),
                ));
            }
        }
        // ITEM_FOUNTAIN — same shape as DRINKCON, but stock Circle fountains
        // refill themselves on use. IronMUD's LiquidContainer treats
        // `liquid_max == -1` as infinite (drink_from / fill_liquid_container
        // skip decrement); set both fields to that sentinel so fountains
        // never run dry.
        23 => {
            data.item_type = ItemType::LiquidContainer;
            data.liquid_max = -1;
            data.liquid_current = -1;
            let (lt, info) = circle_liquid_index_to_type(v[2]);
            data.liquid_type = lt;
            data.liquid_poisoned = v[3] != 0;
            if let Some(msg) = info {
                warnings.push(Warning::new(
                    WarningKind::Info,
                    Severity::Info,
                    item.source.clone(),
                    msg,
                ));
            }
        }
        _ => {
            data.item_type = ItemType::Misc;
            warnings.push(Warning::new(
                WarningKind::UnsupportedValueSemantic,
                Severity::Warn,
                item.source.clone(),
                format!("unknown CircleMUD item_type {} ({type_name}); imported as Misc", item.item_type),
            ));
        }
    }
}

/// Map CircleMUD's WEAPON v3 (damage-message verb index) to an IronMUD
/// `DamageType`. The choice of bucket is the one that best matches the
/// English verb; a few are lossy (notably `blast` → Lightning).
/// CircleMUD `gen_board.c::board_info[]` levels translated to IronMUD admin
/// gating. CircleMUD stores board access by character level (e.g. LVL_IMMORT
/// ≈ 31 for the immortal/freeze boards); IronMUD has no level system, so
/// any level >= 30 collapses to `is_admin = true`. Per-vnum because the
/// source levels are NOT in `.obj` values — they're hardcoded in C.
pub(super) struct StockBoardPreset {
    read_admin_only: bool,
    write_admin_only: bool,
    max_messages: Option<i32>,
    is_stock: bool,
}

pub(super) fn stock_board_preset(vnum: i32) -> StockBoardPreset {
    match vnum {
        // Mortal board — public read, public write.
        3098 => StockBoardPreset {
            read_admin_only: false,
            write_admin_only: false,
            max_messages: Some(60),
            is_stock: true,
        },
        // Social board — public, lower-level write floor in stock; collapses
        // to public for IronMUD.
        3097 => StockBoardPreset {
            read_admin_only: false,
            write_admin_only: false,
            max_messages: Some(60),
            is_stock: true,
        },
        // Freeze (admin disciplinary) board — admin only.
        3096 => StockBoardPreset {
            read_admin_only: true,
            write_admin_only: true,
            max_messages: Some(60),
            is_stock: true,
        },
        // Immortal board — admin only.
        3099 => StockBoardPreset {
            read_admin_only: true,
            write_admin_only: true,
            max_messages: Some(60),
            is_stock: true,
        },
        _ => StockBoardPreset {
            read_admin_only: false,
            write_admin_only: false,
            max_messages: Some(60),
            is_stock: false,
        },
    }
}

pub(super) fn circle_weapon_damage_type(v3: i32) -> DamageType {
    match v3 {
        0 | 5 | 6 | 7 | 9 | 10 | 13 => DamageType::Bludgeoning, // hit, bludgeon, crush, pound, maul, thrash, punch
        2 | 3 | 8 => DamageType::Slashing,                       // whip, slash, claw
        1 | 11 | 14 => DamageType::Piercing,                     // sting, pierce, stab
        4 => DamageType::Bite,                                    // bite
        12 => DamageType::Lightning,                              // blast (lossy)
        _ => DamageType::Bludgeoning,
    }
}

/// CircleMUD `LIQ_*` index → IronMUD `LiquidType`. Returns an Info message
/// when the source liquid has no exact equivalent and we picked the closest
/// IronMUD bucket (e.g. "dark ale" → Ale).
pub(super) fn circle_liquid_index_to_type(idx: i32) -> (LiquidType, Option<String>) {
    match idx {
        0 => (LiquidType::Water, None),
        1 => (LiquidType::Beer, None),
        2 => (LiquidType::Wine, None),
        3 => (LiquidType::Ale, None),
        4 => (LiquidType::Ale, Some("Circle 'dark ale' folded into Ale (no distinct IronMUD type)".into())),
        5 => (LiquidType::Spirits, None),
        6 => (LiquidType::Juice, Some("Circle 'lemonade' folded into Juice".into())),
        7 => (LiquidType::Spirits, Some("Circle 'firebreather' folded into Spirits".into())),
        8 => (LiquidType::Ale, Some("Circle 'local speciality' folded into Ale".into())),
        9 => (LiquidType::Juice, Some("Circle 'slime mold juice' folded into Juice".into())),
        10 => (LiquidType::Milk, None),
        11 => (LiquidType::Tea, None),
        12 => (LiquidType::Coffee, None),
        13 => (LiquidType::Blood, None),
        14 => (LiquidType::Water, Some("Circle 'salt water' folded into Water".into())),
        15 => (LiquidType::Water, None),
        _ => (
            LiquidType::Water,
            Some(format!("unknown Circle liquid index {idx}; defaulted to Water")),
        ),
    }
}

/// Set an ItemFlags bool by snake_case name. Returns false if the name isn't
/// a known flag — surfaces typos in the mapping JSON.
pub(super) fn apply_named_item_flag(flags: &mut ItemFlags, name: &str) -> bool {
    match name {
        "no_drop" => flags.no_drop = true,
        "no_get" => flags.no_get = true,
        "no_remove" => flags.no_remove = true,
        "invisible" => flags.invisible = true,
        "glow" => flags.glow = true,
        "hum" => flags.hum = true,
        "magical" => flags.magical = true,
        "no_sell" => flags.no_sell = true,
        "no_donate" => flags.no_donate = true,
        "unique" => flags.unique = true,
        "quest_item" => flags.quest_item = true,
        "provides_light" => flags.provides_light = true,
        "boat" => flags.boat = true,
        "waterproof" => flags.waterproof = true,
        _ => return false,
    }
    true
}

/// Set an ItemData stat-bonus field by snake_case name. Returns false if the
/// name isn't recognised.
pub(super) fn apply_named_item_stat(data: &mut ItemData, name: &str, modifier: i32) -> bool {
    match name {
        "stat_str" => data.stat_str += modifier,
        "stat_dex" => data.stat_dex += modifier,
        "stat_con" => data.stat_con += modifier,
        "stat_int" => data.stat_int += modifier,
        "stat_wis" => data.stat_wis += modifier,
        "stat_cha" => data.stat_cha += modifier,
        _ => return false,
    }
    true
}
