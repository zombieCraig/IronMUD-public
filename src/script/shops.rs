// src/script/shops.rs
// Shop and vending machine system functions

use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Adjust a shop rate by the player's effective charisma.
/// Returns `base_rate` unchanged when `no_charm` is set. Baseline CHA = 10,
/// 2% per point, capped at ±30%, floored at SHOP_RATE_MIN (1).
pub(crate) fn charisma_adjusted_rate(base_rate: i64, effective_cha: i64, player_buying: bool, no_charm: bool) -> i64 {
    if no_charm {
        return base_rate;
    }
    let modifier_pct = ((effective_cha - 10) * 2).clamp(-30, 30);
    let signed = if player_buying {
        100 - modifier_pct
    } else {
        100 + modifier_pct
    };
    (base_rate * signed / 100).max(1)
}

/// Every reason a shop quotes one player a different number than another,
/// applied in one place — see the `shop_rate_for` registration.
///
/// An unparseable or missing mobile falls back to the listed rate rather than
/// erroring: a price is always better than no price, and the caller has
/// already found a shopkeeper to get here.
pub(crate) fn shop_rate_for_player(
    db: &Db,
    state: &crate::SharedState,
    base_rate: i64,
    char_name: &str,
    mobile_id: &str,
    player_buying: bool,
) -> i64 {
    let Ok(uuid) = uuid::Uuid::parse_str(mobile_id) else {
        return base_rate.max(1);
    };
    let Ok(Some(mobile)) = db.get_mobile_data(&uuid) else {
        return base_rate.max(1);
    };
    let cha = crate::script::characters::effective_charisma(db, char_name);
    let rate = charisma_adjusted_rate(base_rate, cha, player_buying, mobile.flags.no_charm);

    // An untagged shopkeeper has no faction to hold an opinion with, which is
    // the default and leaves pricing exactly as it was.
    let Some(faction) = crate::reputation::normalize(mobile.faction.as_deref()) else {
        return rate;
    };
    let standing = match db.get_character_data(&char_name.to_lowercase()) {
        Ok(Some(ch)) => crate::reputation::standing(&ch.reputation, &faction),
        _ => 0,
    };
    if standing == 0 {
        return rate;
    }
    let def = crate::reputation::definition(state, &faction);
    crate::reputation::adjusted_shop_rate(&def, standing, rate, player_buying)
}

/// Whether `mobile`'s shop would take `item` at all — types, categories, the
/// shop preset behind them, and the value range.
///
/// Extracted from the `shop_will_buy_item` binding so consignment can ask the
/// same question. A broker's shelf and its counter must agree about what the
/// shop deals in, and two copies of a filter this long would not stay agreed.
pub fn shop_accepts_item(db: &Db, mobile: &crate::types::MobileData, item: &crate::types::ItemData) -> bool {
    // Resolve preset if set
    let preset = if !mobile.shop_preset_vnum.is_empty() {
        db.get_shop_preset_by_vnum(&mobile.shop_preset_vnum).ok().flatten()
    } else {
        None
    };

    // Build effective types
    let mut effective_types: Vec<String> = Vec::new();
    if let Some(ref p) = preset {
        for t in &p.buy_types {
            effective_types.push(t.to_lowercase());
        }
    }
    for t in &mobile.shop_extra_types {
        let lower = t.to_lowercase();
        if !effective_types.contains(&lower) {
            effective_types.push(lower);
        }
    }
    for t in &mobile.shop_buys_types {
        let lower = t.to_lowercase();
        if !effective_types.contains(&lower) {
            effective_types.push(lower);
        }
    }
    // Remove denied types
    for t in &mobile.shop_deny_types {
        let lower = t.to_lowercase();
        effective_types.retain(|et| et != &lower);
    }

    // Build effective categories
    let mut effective_categories: Vec<String> = Vec::new();
    if let Some(ref p) = preset {
        for c in &p.buy_categories {
            effective_categories.push(c.to_lowercase());
        }
    }
    for c in &mobile.shop_extra_categories {
        let lower = c.to_lowercase();
        if !effective_categories.contains(&lower) {
            effective_categories.push(lower);
        }
    }
    for c in &mobile.shop_buys_categories {
        let lower = c.to_lowercase();
        if !effective_categories.contains(&lower) {
            effective_categories.push(lower);
        }
    }
    // Remove denied categories
    for c in &mobile.shop_deny_categories {
        let lower = c.to_lowercase();
        effective_categories.retain(|ec| ec != &lower);
    }

    // If both empty, buys nothing
    if effective_types.is_empty() && effective_categories.is_empty() {
        return false;
    }

    // Check type filter
    if !effective_types.is_empty() {
        let has_all = effective_types.iter().any(|t| t == "all");
        if !has_all {
            let item_type_lower = item.item_type.to_display_string().to_lowercase();
            if !effective_types.contains(&item_type_lower) {
                return false;
            }
        }
    }

    // Check category filter
    if !effective_categories.is_empty() {
        let item_cats: Vec<String> = item.categories.iter().map(|c| c.to_lowercase()).collect();
        let has_match = effective_categories.iter().any(|ec| item_cats.contains(ec));
        if !has_match {
            return false;
        }
    }

    // Check value range
    let min_val = if let Some(ref p) = preset {
        if mobile.shop_min_value > 0 {
            mobile.shop_min_value
        } else {
            p.min_value
        }
    } else {
        mobile.shop_min_value
    };
    let max_val = if let Some(ref p) = preset {
        if mobile.shop_max_value > 0 {
            mobile.shop_max_value
        } else {
            p.max_value
        }
    } else {
        mobile.shop_max_value
    };

    if min_val > 0 && item.value < min_val {
        return false;
    }
    if max_val > 0 && item.value > max_val {
        return false;
    }

    true
}

/// Register shop-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, state: crate::SharedState) {
    // ========== Shopkeeper Functions ==========

    // find_shopkeeper_in_room(room_id) -> MobileData or ()
    let cloned_db = db.clone();
    engine.register_fn("find_shopkeeper_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_mobiles_in_room(&uuid) {
                Ok(mobiles) => {
                    for mobile in mobiles {
                        if mobile.flags.shopkeeper && !mobile.is_prototype {
                            return rhai::Dynamic::from(mobile);
                        }
                    }
                    rhai::Dynamic::UNIT
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // add_shop_stock(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_shop_stock", move |mobile_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    if !mobile.shop_stock.contains(&vnum) {
                        mobile.shop_stock.push(vnum);
                        return cloned_db.save_mobile_data(mobile).is_ok();
                    }
                    true // Already in stock
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // remove_shop_stock(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_shop_stock", move |mobile_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_stock.retain(|v| v != &vnum);
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_shop_inventory(mobile_id) -> Array of ItemData
    let cloned_db = db.clone();
    engine.register_fn("get_shop_inventory", move |mobile_id: String| -> Vec<rhai::Dynamic> {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mobile)) => {
                    let mut items = Vec::new();
                    for item_id in &mobile.shop_inventory {
                        if let Ok(Some(item)) = cloned_db.get_item_data(item_id) {
                            items.push(rhai::Dynamic::from(item));
                        }
                    }
                    items
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    });

    // add_to_shop_inventory(mobile_id, item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "add_to_shop_inventory",
        move |mobile_id: String, item_id: String| -> bool {
            if let Ok(mobile_uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(item_uuid) = uuid::Uuid::parse_str(&item_id) {
                    match cloned_db.get_mobile_data(&mobile_uuid) {
                        Ok(Some(mut mobile)) => {
                            if !mobile.shop_inventory.contains(&item_uuid) {
                                mobile.shop_inventory.push(item_uuid);
                                return cloned_db.save_mobile_data(mobile).is_ok();
                            }
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        },
    );

    // remove_from_shop_inventory(mobile_id, item_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "remove_from_shop_inventory",
        move |mobile_id: String, item_id: String| -> bool {
            if let Ok(mobile_uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(item_uuid) = uuid::Uuid::parse_str(&item_id) {
                    match cloned_db.get_mobile_data(&mobile_uuid) {
                        Ok(Some(mut mobile)) => {
                            mobile.shop_inventory.retain(|id| id != &item_uuid);
                            cloned_db.save_mobile_data(mobile).is_ok()
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        },
    );

    // set_shop_buy_rate(mobile_id, rate) -> bool. Rejects rates outside [SHOP_RATE_MIN, SHOP_RATE_MAX]
    // to prevent divide-by-zero, sign flips, and gold-arithmetic overflow.
    let cloned_db = db.clone();
    engine.register_fn("set_shop_buy_rate", move |mobile_id: String, rate: i64| -> bool {
        let lo = crate::api::validate::SHOP_RATE_MIN as i64;
        let hi = crate::api::validate::SHOP_RATE_MAX as i64;
        if !(lo..=hi).contains(&rate) {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_buy_rate = rate as i32;
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // set_shop_sell_rate(mobile_id, rate) -> bool. Same band check as buy_rate.
    let cloned_db = db.clone();
    engine.register_fn("set_shop_sell_rate", move |mobile_id: String, rate: i64| -> bool {
        let lo = crate::api::validate::SHOP_RATE_MIN as i64;
        let hi = crate::api::validate::SHOP_RATE_MAX as i64;
        if !(lo..=hi).contains(&rate) {
            return false;
        }
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_sell_rate = rate as i32;
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // calculate_buy_price(base_value, buy_rate) -> i64
    // Price shop pays to player = base_value * buy_rate / 100
    engine.register_fn("calculate_buy_price", |base_value: i64, buy_rate: i64| -> i64 {
        base_value * buy_rate / 100
    });

    // calculate_sell_price(base_value, sell_rate) -> i64
    // Price player pays to shop = base_value * sell_rate / 100
    engine.register_fn("calculate_sell_price", |base_value: i64, sell_rate: i64| -> i64 {
        base_value * sell_rate / 100
    });

    // shop_charisma_rate(base_rate, effective_cha, player_buying, no_charm) -> i64
    // Adjust a shop rate by the player's effective charisma (base + gear + buffs).
    // The primitive; `shop_rate_for` below is what command scripts should call.
    engine.register_fn(
        "shop_charisma_rate",
        |base_rate: i64, effective_cha: i64, player_buying: bool, no_charm: bool| -> i64 {
            charisma_adjusted_rate(base_rate, effective_cha, player_buying, no_charm)
        },
    );

    // shop_rate_for(base_rate, char_name, mobile_id, player_buying) -> i64
    //
    // The single rate chokepoint: every reason a shop quotes one player a
    // different number than another applies here, so `buy`, `sell`, `list` and
    // `appraise` cannot drift apart on which modifiers they honour. Today that
    // is charisma (unchanged from `shop_charisma_rate`) and faction standing.
    //
    // `player_buying` flips the sense of both: a shopkeeper who likes you
    // charges you less AND pays you more.
    let cloned_db = db.clone();
    let rate_state = state.clone();
    engine.register_fn(
        "shop_rate_for",
        move |base_rate: i64, char_name: String, mobile_id: String, player_buying: bool| -> i64 {
            shop_rate_for_player(
                &cloned_db,
                &rate_state,
                base_rate,
                &char_name,
                &mobile_id,
                player_buying,
            )
        },
    );

    // get_shop_buys_types(mobile_id) -> Array of strings
    let cloned_db = db.clone();
    engine.register_fn("get_shop_buys_types", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_buys_types
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_buys_types(mobile_id, types_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_buys_types",
        move |mobile_id: String, types: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_buys_types = types.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // shop_will_buy_type(mobile_id, item_type_str) -> bool
    // Checks if the shopkeeper will buy items of this type
    let cloned_db = db.clone();
    engine.register_fn(
        "shop_will_buy_type",
        move |mobile_id: String, item_type: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                    // Empty list = buys nothing
                    if mobile.shop_buys_types.is_empty() {
                        return false;
                    }
                    // Check for "all" - buys any type
                    let item_type_lower = item_type.to_lowercase();
                    for buy_type in &mobile.shop_buys_types {
                        if buy_type.to_lowercase() == "all" {
                            return true;
                        }
                        if buy_type.to_lowercase() == item_type_lower {
                            return true;
                        }
                    }
                    return false;
                }
            }
            false
        },
    );

    // shop_will_buy_item(mobile_id, item_id) -> bool
    // Full validation: types, categories, preset, value range
    let cloned_db = db.clone();
    engine.register_fn(
        "shop_will_buy_item",
        move |mobile_id: String, item_id: String| -> bool {
            let mobile_uuid = match uuid::Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let item_uuid = match uuid::Uuid::parse_str(&item_id) {
                Ok(u) => u,
                Err(_) => return false,
            };
            let mobile = match cloned_db.get_mobile_data(&mobile_uuid) {
                Ok(Some(m)) => m,
                _ => return false,
            };
            let item = match cloned_db.get_item_data(&item_uuid) {
                Ok(Some(i)) => i,
                _ => return false,
            };

            shop_accepts_item(&cloned_db, &mobile, &item)
        },
    );

    // set_consignment_commission(mobile_id, pct) -> bool
    // set_consignment_listing_cap(mobile_id, cap) -> bool
    //
    // Both write the prototype the shelf is keyed by if the target is one, and
    // the instance otherwise — the same object `medit` is editing.
    let cloned_db = db.clone();
    engine.register_fn(
        "set_consignment_commission",
        move |mobile_id: String, pct: i64| -> bool {
            let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) else {
                return false;
            };
            cloned_db
                .update_mobile(&uuid, |m| m.consignment_commission_pct = pct.clamp(0, 100) as i32)
                .is_ok()
        },
    );

    let cloned_db = db.clone();
    engine.register_fn(
        "set_consignment_listing_cap",
        move |mobile_id: String, cap: i64| -> bool {
            let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) else {
                return false;
            };
            cloned_db
                .update_mobile(&uuid, |m| {
                    m.consignment_max_listings_per_player = cap.clamp(0, 1000) as i32
                })
                .is_ok()
        },
    );

    // ========== Shop Buy Category/Preset Getters & Setters ==========

    // get_shop_buys_categories(mobile_id) -> Array of strings
    let cloned_db = db.clone();
    engine.register_fn("get_shop_buys_categories", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_buys_categories
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_buys_categories(mobile_id, categories) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_buys_categories",
        move |mobile_id: String, cats: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_buys_categories =
                            cats.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_shop_preset_vnum(mobile_id) -> String
    let cloned_db = db.clone();
    engine.register_fn("get_shop_preset_vnum", move |mobile_id: String| -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.shop_preset_vnum.clone();
            }
        }
        String::new()
    });

    // set_shop_preset_vnum(mobile_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_shop_preset_vnum", move |mobile_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_preset_vnum = vnum;
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_shop_extra_types(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_shop_extra_types", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_extra_types
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_extra_types(mobile_id, types) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_extra_types",
        move |mobile_id: String, types: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_extra_types = types.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_shop_extra_categories(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_shop_extra_categories", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_extra_categories
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_extra_categories(mobile_id, cats) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_extra_categories",
        move |mobile_id: String, cats: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_extra_categories =
                            cats.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_shop_deny_types(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_shop_deny_types", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_deny_types
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_deny_types(mobile_id, types) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_deny_types",
        move |mobile_id: String, types: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_deny_types = types.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_shop_deny_categories(mobile_id) -> Array
    let cloned_db = db.clone();
    engine.register_fn("get_shop_deny_categories", move |mobile_id: String| -> rhai::Array {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile
                    .shop_deny_categories
                    .iter()
                    .map(|s| rhai::Dynamic::from(s.clone()))
                    .collect();
            }
        }
        rhai::Array::new()
    });

    // set_shop_deny_categories(mobile_id, cats) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_shop_deny_categories",
        move |mobile_id: String, cats: rhai::Array| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
                match cloned_db.get_mobile_data(&uuid) {
                    Ok(Some(mut mobile)) => {
                        mobile.shop_deny_categories =
                            cats.iter().filter_map(|v| v.clone().into_string().ok()).collect();
                        cloned_db.save_mobile_data(mobile).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );

    // get_shop_min_value(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_shop_min_value", move |mobile_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.shop_min_value as i64;
            }
        }
        0
    });

    // set_shop_min_value(mobile_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_shop_min_value", move |mobile_id: String, value: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_min_value = value as i32;
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_shop_max_value(mobile_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_shop_max_value", move |mobile_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            if let Ok(Some(mobile)) = cloned_db.get_mobile_data(&uuid) {
                return mobile.shop_max_value as i64;
            }
        }
        0
    });

    // set_shop_max_value(mobile_id, value) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_shop_max_value", move |mobile_id: String, value: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&mobile_id) {
            match cloned_db.get_mobile_data(&uuid) {
                Ok(Some(mut mobile)) => {
                    mobile.shop_max_value = value as i32;
                    cloned_db.save_mobile_data(mobile).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // ========== Vending Machine Functions ==========

    // find_vending_machine_in_room(room_id) -> ItemData or ()
    let cloned_db = db.clone();
    engine.register_fn("find_vending_machine_in_room", move |room_id: String| {
        if let Ok(uuid) = uuid::Uuid::parse_str(&room_id) {
            match cloned_db.get_items_in_room(&uuid) {
                Ok(items) => {
                    for item in items {
                        if item.flags.vending && !item.is_prototype {
                            return rhai::Dynamic::from(item);
                        }
                    }
                    rhai::Dynamic::UNIT
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // add_vending_stock(item_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_vending_stock", move |item_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    if !item.vending_stock.contains(&vnum) {
                        item.vending_stock.push(vnum);
                        return cloned_db.save_item_data(item).is_ok();
                    }
                    true // Already in stock
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // remove_vending_stock(item_id, vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_vending_stock", move |item_id: String, vnum: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.vending_stock.retain(|v| v != &vnum);
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // set_vending_sell_rate(item_id, rate) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_vending_sell_rate", move |item_id: String, rate: i64| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.vending_sell_rate = rate as i32;
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::charisma_adjusted_rate;

    #[test]
    fn baseline_charisma_leaves_rate_unchanged() {
        // CHA 10 = no adjustment in either direction.
        assert_eq!(charisma_adjusted_rate(150, 10, true, false), 150);
        assert_eq!(charisma_adjusted_rate(50, 10, false, false), 50);
    }

    #[test]
    fn high_charisma_helps_both_directions() {
        // CHA 18 -> 16% better: pay less buying, earn more selling.
        assert_eq!(charisma_adjusted_rate(150, 18, true, false), 126);
        assert_eq!(charisma_adjusted_rate(50, 18, false, false), 58);
    }

    #[test]
    fn modifier_caps_at_thirty_percent() {
        // CHA 30 would be +40% uncapped; clamps to ±30%.
        assert_eq!(charisma_adjusted_rate(150, 30, true, false), 105);
        assert_eq!(charisma_adjusted_rate(50, 30, false, false), 65);
    }

    #[test]
    fn low_charisma_hurts_both_directions() {
        // CHA 5 -> -10%: pay more buying, earn less selling.
        assert_eq!(charisma_adjusted_rate(150, 5, true, false), 165);
        assert_eq!(charisma_adjusted_rate(50, 5, false, false), 45);
    }

    #[test]
    fn no_charm_ignores_charisma() {
        assert_eq!(charisma_adjusted_rate(150, 30, true, true), 150);
        assert_eq!(charisma_adjusted_rate(50, 5, false, true), 50);
    }

    #[test]
    fn rate_never_drops_below_one() {
        // Tiny base rate with max discount still floors at 1, never 0.
        assert_eq!(charisma_adjusted_rate(1, 30, true, false), 1);
    }

    // ===== shop_rate_for_player: charisma and faction standing composed =====

    use super::shop_rate_for_player;

    struct Fixture {
        db: crate::db::Db,
        state: crate::SharedState,
        shop_id: String,
        _temp: tempfile::TempDir,
    }

    /// A shopkeeper tagged `iron_guard`, a customer at baseline CHA 10, and a
    /// registry with one faction in it.
    fn fixture(shop_faction: Option<&str>, standing: i32) -> Fixture {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = crate::db::Db::open(temp.path()).expect("open db");

        let mut ch: crate::types::CharacterData = serde_json::from_value(serde_json::json!({
            "name": "rook",
            "password_hash": "",
            "current_room_id": uuid::Uuid::nil(),
        }))
        .expect("build character");
        ch.stat_cha = 10;
        if standing != 0 {
            ch.reputation.insert("iron_guard".to_string(), standing);
        }
        db.save_character_data(ch).expect("save char");

        let mut mob = crate::types::MobileData::new("Quartermaster".into());
        mob.faction = shop_faction.map(|s| s.to_string());
        let shop_id = mob.id.to_string();
        db.save_mobile_data(mob).expect("save mob");

        let conns: crate::SharedConnections =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let state = crate::World::minimal_shared(db.clone(), conns);
        {
            let mut w = state.lock().unwrap();
            w.faction_definitions.insert(
                "iron_guard".to_string(),
                crate::reputation::FactionDefinition::unregistered("iron_guard"),
            );
        }
        Fixture {
            db,
            state,
            shop_id,
            _temp: temp,
        }
    }

    #[test]
    fn an_untagged_shopkeeper_prices_exactly_as_before() {
        let f = fixture(None, 0);
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", &f.shop_id, true),
            150
        );
        assert_eq!(shop_rate_for_player(&f.db, &f.state, 50, "rook", &f.shop_id, false), 50);
    }

    #[test]
    fn a_stranger_to_a_tagged_faction_also_pays_the_listed_rate() {
        // Standing 0 is the default for a faction you have never dealt with,
        // so tagging a shopkeeper must not move prices on its own.
        let f = fixture(Some("iron_guard"), 0);
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", &f.shop_id, true),
            150
        );
    }

    #[test]
    fn standing_discounts_purchases_and_improves_offers() {
        let f = fixture(Some("iron_guard"), crate::reputation::REPUTATION_MAX);
        // -20% at the top of the ladder on a 150% markup.
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", &f.shop_id, true),
            120
        );
        // ...and the same 20% the other way on what they pay you.
        assert_eq!(shop_rate_for_player(&f.db, &f.state, 50, "rook", &f.shop_id, false), 60);
    }

    #[test]
    fn a_faction_you_have_wronged_charges_you_more() {
        let f = fixture(Some("iron_guard"), crate::reputation::REPUTATION_MIN);
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", &f.shop_id, true),
            180
        );
        assert_eq!(shop_rate_for_player(&f.db, &f.state, 50, "rook", &f.shop_id, false), 40);
    }

    #[test]
    fn a_missing_shopkeeper_still_quotes_the_listed_rate() {
        // `appraise` and `list` reach here having already found a shopkeeper;
        // erroring out on a race with its deletion would be worse than a price.
        let f = fixture(None, 0);
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", "not-a-uuid", true),
            150
        );
        assert_eq!(
            shop_rate_for_player(&f.db, &f.state, 150, "rook", &uuid::Uuid::new_v4().to_string(), true),
            150
        );
    }
}
