use std::collections::HashMap;

use crate::import::{
    IrShop, MappingOptions, PlannedShopOverlay, Severity, Warning, WarningKind,
};
use crate::types::{
    ActivityState, RoutineEntry,
};

use super::FlagAction;

/// Map an `IrShop` to a [`PlannedShopOverlay`]. Returns `None` (with a
/// Warn) if the keeper mob isn't in this import — those shops can't be
/// applied. Most other gaps surface as advisory warnings: messages,
/// temper, bitvector, with_who, multi-room, non-default hours.
pub(super) fn map_shop(
    shop: &IrShop,
    mob_index: &HashMap<i32, String>,
    item_index: &HashMap<i32, String>,
    opts: &MappingOptions,
) -> (Option<PlannedShopOverlay>, Vec<Warning>) {
    let mut warnings = Vec::new();

    // Resolve the keeper mob. Without it we can't apply the shop.
    let Some(keeper_vnum) = mob_index.get(&shop.keeper_vnum).cloned() else {
        warnings.push(Warning::new(
            WarningKind::DanglingExit,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} keeper mob #{} is not in the import set; shop dropped",
                shop.vnum, shop.keeper_vnum
            ),
        ));
        return (None, warnings);
    };

    // Producing list. Drop entries we can't resolve, with a per-entry warn.
    let mut stock_vnums: Vec<String> = Vec::new();
    for v in &shop.producing {
        match item_index.get(v) {
            Some(rewritten) => stock_vnums.push(rewritten.clone()),
            None => warnings.push(Warning::new(
                WarningKind::DanglingExit,
                Severity::Warn,
                shop.source.clone(),
                format!(
                    "shop #{}: producing item #{} is not in the import set; entry dropped",
                    shop.vnum, v
                ),
            )),
        }
    }

    // Profit multipliers. Circle profit_buy = markup the shop charges =
    // IronMUD shop_sell_rate (% of base value the player pays). Circle
    // profit_sell = fraction the shop pays = IronMUD shop_buy_rate.
    let sell_rate = (shop.profit_buy * 100.0).round() as i32;
    let buy_rate = (shop.profit_sell * 100.0).round() as i32;
    let sell_rate = sell_rate.clamp(0, 10_000);
    let buy_rate = buy_rate.clamp(0, 10_000);

    // Buy types via the JSON action table. Dedupe.
    let mut buys_types: Vec<String> = Vec::new();
    for token in &shop.buy_types {
        match opts.circle.buy_type_actions.get(token) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                let v = ironmud_flag.to_lowercase();
                if !buys_types.contains(&v) {
                    buys_types.push(v);
                }
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    shop.source.clone(),
                    format!("shop #{} buy_type {token}: {message}", shop.vnum),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(_) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    shop.source.clone(),
                    format!(
                        "shop #{} buy_type {token}: mapping uses an action that doesn't apply to buy_types",
                        shop.vnum
                    ),
                ));
            }
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                shop.source.clone(),
                format!("shop #{} buy_type {token}: no mapping (entry dropped)", shop.vnum),
            )),
        }
    }
    for raw in &shop.unknown_buy_types {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} unrecognised buy_type token {raw:?} (entry dropped)",
                shop.vnum
            ),
        ));
    }

    // Advisory warnings for unsupported features. Builders can revisit
    // these manually; the importer doesn't translate them.
    if shop.messages.iter().any(|m| !m.is_empty()) {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} carries {} custom message string(s); IronMUD has no per-shop messages — discarded",
                shop.vnum,
                shop.messages.iter().filter(|m| !m.is_empty()).count()
            ),
        ));
    }
    if shop.temper != 0 {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Info,
            shop.source.clone(),
            format!("shop #{} temper={} dropped (no IronMUD analogue)", shop.vnum, shop.temper),
        ));
    }
    // CircleMUD shop bitvector: bit 0 = WILL_START_FIGHT (translated),
    // bit 1 = WILL_BANK_MONEY (still warn-only — IronMUD has no shop-bank
    // link). Higher bits are unused in stock Circle.
    let hostile_on_steal = shop.bitvector & 0b01 != 0;
    let will_bank_money = shop.bitvector & 0b10 != 0;
    let unknown_bits = shop.bitvector & !0b11;
    if will_bank_money {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} WILL_BANK_MONEY not modeled (no IronMUD shop-bank link)",
                shop.vnum
            ),
        ));
    }
    if unknown_bits != 0 {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} bitvector has unknown bits 0x{:x} (only WILL_START_FIGHT and WILL_BANK_MONEY are recognised)",
                shop.vnum, unknown_bits
            ),
        ));
    }
    if shop.with_who != 0 {
        warnings.push(Warning::new(
            WarningKind::UnsupportedFlag,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} with_who={} (TRADE_NO* alignment/class trade gates) not modeled — shop will trade with anyone",
                shop.vnum, shop.with_who
            ),
        ));
    }
    if shop.rooms.len() > 1 {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Warn,
            shop.source.clone(),
            format!(
                "shop #{} operates in {} rooms; IronMUD shopkeepers travel with their shop — only the keeper's current room is honored",
                shop.vnum,
                shop.rooms.len()
            ),
        ));
    }
    let daily_routine = synthesize_shop_routine(shop, &mut warnings);

    (
        Some(PlannedShopOverlay {
            shop_source_vnum: shop.vnum,
            keeper_source_vnum: shop.keeper_vnum,
            keeper_vnum,
            stock_vnums,
            buy_rate,
            sell_rate,
            buys_types,
            daily_routine,
            hostile_on_steal,
            source: shop.source.clone(),
        }),
        warnings,
    )
}

/// Treat "open all the time" as the default. Stock CircleMUD encodes that
/// as `0 28 0 0` (open at midnight, close at hour 28 which the runtime
/// reads as "always open"; second shift unused).
pub(super) fn is_default_hours(shop: &IrShop) -> bool {
    let always_first_shift = shop.open1 == 0 && shop.close1 >= 24;
    let no_second_shift = shop.open2 == 0 && shop.close2 == 0;
    always_first_shift && no_second_shift
}

/// Translate CircleMUD `open1/close1/open2/close2` into IronMUD
/// `RoutineEntry`s. Each open window emits a `Working` entry at the open
/// hour and an `OffDuty` entry at the close hour; together they partition
/// the 24-hour day so `find_active_entry` flips the keeper between
/// trading and not-trading. Returns an empty vec for "always open" shops
/// (so the writer leaves the keeper's routine untouched). Wrap-around
/// windows (e.g. open=20 close=4) need no special handling — entries
/// are stored as `hour % 24` and `find_active_entry` already wraps past
/// midnight via its `best.or(best_wrap)` fallback.
pub(super) fn synthesize_shop_routine(shop: &IrShop, warnings: &mut Vec<Warning>) -> Vec<RoutineEntry> {
    if is_default_hours(shop) {
        return Vec::new();
    }

    let mut shifts: Vec<(i32, i32, u8)> = Vec::new();
    // A non-default shop always has a meaningful first shift.
    shifts.push((shop.open1, shop.close1, 1));
    if !(shop.open2 == 0 && shop.close2 == 0) {
        shifts.push((shop.open2, shop.close2, 2));
    }

    let mut entries: Vec<RoutineEntry> = Vec::new();
    for (open, close, idx) in shifts {
        let open_h = open.rem_euclid(24) as u8;
        let close_h = close.rem_euclid(24) as u8;
        if open_h == close_h {
            warnings.push(Warning::new(
                WarningKind::DeferredFeature,
                Severity::Warn,
                shop.source.clone(),
                format!(
                    "shop #{} shift {} has open==close ({}/{}) after mod 24; window dropped",
                    shop.vnum, idx, open, close
                ),
            ));
            continue;
        }
        entries.push(routine_entry(open_h, ActivityState::Working));
        entries.push(routine_entry(close_h, ActivityState::OffDuty));
    }

    if entries.is_empty() {
        return entries;
    }

    // Sort, then collapse colliding start_hours preferring Working — if
    // two shifts share a boundary (e.g. shift1 closes at the same hour
    // shift2 opens), the keeper should be on duty, not off.
    entries.sort_by_key(|e| e.start_hour);
    let original_len = entries.len();
    let mut deduped: Vec<RoutineEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(last) = deduped.last_mut() {
            if last.start_hour == entry.start_hour {
                if matches!(entry.activity, ActivityState::Working) {
                    *last = entry;
                }
                continue;
            }
        }
        deduped.push(entry);
    }
    if deduped.len() != original_len {
        warnings.push(Warning::new(
            WarningKind::DeferredFeature,
            Severity::Info,
            shop.source.clone(),
            format!(
                "shop #{} has overlapping shifts after mod 24; collapsed to {} routine entr{}",
                shop.vnum,
                deduped.len(),
                if deduped.len() == 1 { "y" } else { "ies" }
            ),
        ));
    }

    deduped
}

pub(super) fn routine_entry(start_hour: u8, activity: ActivityState) -> RoutineEntry {
    RoutineEntry {
        start_hour,
        activity,
        destination_vnum: None,
        transition_message: None,
        suppress_wander: false,
        dialogue_overrides: HashMap::new(),
    }
}
