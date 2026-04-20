// src/script/shops.rs
// Shop and vending machine system functions

use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

/// Register shop-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
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

    // set_shop_buy_rate(mobile_id, rate) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_shop_buy_rate", move |mobile_id: String, rate: i64| -> bool {
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

    // set_shop_sell_rate(mobile_id, rate) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_shop_sell_rate", move |mobile_id: String, rate: i64| -> bool {
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

            // Resolve preset if set
            let preset = if !mobile.shop_preset_vnum.is_empty() {
                cloned_db
                    .get_shop_preset_by_vnum(&mobile.shop_preset_vnum)
                    .ok()
                    .flatten()
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
