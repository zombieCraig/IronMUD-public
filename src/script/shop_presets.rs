// src/script/shop_presets.rs
// Shop buy preset system functions

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::ShopPreset;

/// Register shop preset functions
pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // Register ShopPreset type with getters/setters
    engine.register_type_with_name::<ShopPreset>("ShopPreset")
        .register_get("id", |p: &mut ShopPreset| p.id.to_string())
        .register_get("vnum", |p: &mut ShopPreset| p.vnum.clone())
        .register_set("vnum", |p: &mut ShopPreset, val: String| p.vnum = val)
        .register_get("name", |p: &mut ShopPreset| p.name.clone())
        .register_set("name", |p: &mut ShopPreset, val: String| p.name = val)
        .register_get("description", |p: &mut ShopPreset| p.description.clone())
        .register_set("description", |p: &mut ShopPreset, val: String| p.description = val)
        .register_get("buy_types", |p: &mut ShopPreset| {
            p.buy_types.iter().map(|s| rhai::Dynamic::from(s.clone())).collect::<Vec<_>>()
        })
        .register_set("buy_types", |p: &mut ShopPreset, val: rhai::Array| {
            p.buy_types = val.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
        })
        .register_get("buy_categories", |p: &mut ShopPreset| {
            p.buy_categories.iter().map(|s| rhai::Dynamic::from(s.clone())).collect::<Vec<_>>()
        })
        .register_set("buy_categories", |p: &mut ShopPreset, val: rhai::Array| {
            p.buy_categories = val.into_iter().filter_map(|d| d.try_cast::<String>()).collect();
        })
        .register_get("min_value", |p: &mut ShopPreset| p.min_value as i64)
        .register_set("min_value", |p: &mut ShopPreset, val: i64| p.min_value = val as i32)
        .register_get("max_value", |p: &mut ShopPreset| p.max_value as i64)
        .register_set("max_value", |p: &mut ShopPreset, val: i64| p.max_value = val as i32);

    // new_shop_preset(vnum, name) -> ShopPreset
    engine.register_fn("new_shop_preset", |vnum: String, name: String| {
        ShopPreset::new(vnum, name)
    });

    // get_shop_preset(vnum) -> ShopPreset or ()
    let cloned_db = db.clone();
    engine.register_fn("get_shop_preset", move |vnum: String| -> rhai::Dynamic {
        // Try by vnum first
        match cloned_db.get_shop_preset_by_vnum(&vnum) {
            Ok(Some(preset)) => rhai::Dynamic::from(preset),
            _ => {
                // Try by UUID
                if let Ok(uuid) = uuid::Uuid::parse_str(&vnum) {
                    match cloned_db.get_shop_preset(&uuid) {
                        Ok(Some(preset)) => rhai::Dynamic::from(preset),
                        _ => rhai::Dynamic::UNIT,
                    }
                } else {
                    rhai::Dynamic::UNIT
                }
            }
        }
    });

    // save_shop_preset(preset) -> bool
    let cloned_db = db.clone();
    engine.register_fn("save_shop_preset", move |preset: ShopPreset| -> bool {
        cloned_db.save_shop_preset(&preset).is_ok()
    });

    // delete_shop_preset(vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("delete_shop_preset", move |vnum: String| -> bool {
        // Try by vnum first
        if let Ok(Some(preset)) = cloned_db.get_shop_preset_by_vnum(&vnum) {
            return cloned_db.delete_shop_preset(&preset.id).unwrap_or(false);
        }
        // Try by UUID
        if let Ok(uuid) = uuid::Uuid::parse_str(&vnum) {
            return cloned_db.delete_shop_preset(&uuid).unwrap_or(false);
        }
        false
    });

    // list_shop_presets() -> Array of ShopPreset
    let cloned_db = db.clone();
    engine.register_fn("list_shop_presets", move || -> Vec<rhai::Dynamic> {
        cloned_db.list_all_shop_presets()
            .unwrap_or_default()
            .into_iter()
            .map(rhai::Dynamic::from)
            .collect()
    });
}
