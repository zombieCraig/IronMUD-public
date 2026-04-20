// src/script/spells.rs
// Spell system functions: spell definitions, cooldowns, mana, and reagents

use crate::EffectType;
use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use rhai::Engine;
use std::sync::Arc;

pub fn register(engine: &mut Engine, db: Arc<Db>, _connections: SharedConnections, state: SharedState) {
    // get_spell_list() -> Array of spell IDs
    let state_clone = state.clone();
    engine.register_fn("get_spell_list", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        world
            .spell_definitions
            .keys()
            .map(|k| rhai::Dynamic::from(k.clone()))
            .collect()
    });

    // get_spell_info(spell_id) -> Map with all spell fields, or empty map
    let state_clone = state.clone();
    engine.register_fn("get_spell_info", move |spell_id: String| -> rhai::Map {
        let world = state_clone.lock().unwrap();
        let mut map = rhai::Map::new();
        if let Some(spell) = world.spell_definitions.get(&spell_id) {
            map.insert("id".into(), rhai::Dynamic::from(spell.id.clone()));
            map.insert("name".into(), rhai::Dynamic::from(spell.name.clone()));
            map.insert("description".into(), rhai::Dynamic::from(spell.description.clone()));
            map.insert(
                "skill_required".into(),
                rhai::Dynamic::from(spell.skill_required as i64),
            );
            map.insert("scroll_only".into(), rhai::Dynamic::from(spell.scroll_only));
            map.insert("mana_cost".into(), rhai::Dynamic::from(spell.mana_cost as i64));
            map.insert("cooldown_secs".into(), rhai::Dynamic::from(spell.cooldown_secs as i64));
            map.insert("spell_type".into(), rhai::Dynamic::from(spell.spell_type.clone()));
            map.insert("damage_base".into(), rhai::Dynamic::from(spell.damage_base as i64));
            map.insert(
                "damage_per_skill".into(),
                rhai::Dynamic::from(spell.damage_per_skill as i64),
            );
            map.insert(
                "damage_int_scaling".into(),
                rhai::Dynamic::from(spell.damage_int_scaling as i64),
            );
            map.insert("damage_type".into(), rhai::Dynamic::from(spell.damage_type.clone()));
            map.insert("buff_effect".into(), rhai::Dynamic::from(spell.buff_effect.clone()));
            map.insert(
                "buff_magnitude".into(),
                rhai::Dynamic::from(spell.buff_magnitude as i64),
            );
            map.insert(
                "buff_duration_secs".into(),
                rhai::Dynamic::from(spell.buff_duration_secs as i64),
            );
            map.insert("heal_base".into(), rhai::Dynamic::from(spell.heal_base as i64));
            map.insert(
                "heal_per_skill".into(),
                rhai::Dynamic::from(spell.heal_per_skill as i64),
            );
            map.insert(
                "heal_int_scaling".into(),
                rhai::Dynamic::from(spell.heal_int_scaling as i64),
            );
            map.insert("target_type".into(), rhai::Dynamic::from(spell.target_type.clone()));
            map.insert("requires_combat".into(), rhai::Dynamic::from(spell.requires_combat));
            map.insert(
                "reagent_vnum".into(),
                rhai::Dynamic::from(spell.reagent_vnum.clone().unwrap_or_default()),
            );
            map.insert("xp_award".into(), rhai::Dynamic::from(spell.xp_award as i64));
        }
        map
    });

    // get_available_spells(char_name) -> Array of spell ID strings the character can cast
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("get_available_spells", move |char_name: String| -> rhai::Array {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return rhai::Array::new(),
        };
        let magic_level = char.skills.get("magic").map(|s| s.level).unwrap_or(0);
        let world = state_clone.lock().unwrap();
        let mut available = rhai::Array::new();
        for (id, spell) in &world.spell_definitions {
            if magic_level >= spell.skill_required {
                if spell.scroll_only {
                    // Must have learned it
                    if char.learned_spells.contains(id) {
                        available.push(rhai::Dynamic::from(id.clone()));
                    }
                } else {
                    available.push(rhai::Dynamic::from(id.clone()));
                }
            }
        }
        available
    });

    // has_learned_spell(char_name, spell_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "has_learned_spell",
        move |char_name: String, spell_id: String| -> bool {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c.learned_spells.contains(&spell_id),
                _ => false,
            }
        },
    );

    // learn_spell(char_name, spell_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("learn_spell", move |char_name: String, spell_id: String| -> bool {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut c)) => {
                if !c.learned_spells.contains(&spell_id) {
                    c.learned_spells.push(spell_id);
                    cloned_db.save_character_data(c).is_ok()
                } else {
                    true // Already learned
                }
            }
            _ => false,
        }
    });

    // get_spell_cooldown_remaining(char_name, spell_id) -> i64 (seconds remaining, 0 if ready)
    let cloned_db = db.clone();
    engine.register_fn(
        "get_spell_cooldown_remaining",
        move |char_name: String, spell_id: String| -> i64 {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    match c.spell_cooldowns.get(&spell_id) {
                        Some(&ready_at) => (ready_at - now).max(0),
                        None => 0,
                    }
                }
                _ => 0,
            }
        },
    );

    // set_spell_cooldown(char_name, spell_id, secs) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_spell_cooldown",
        move |char_name: String, spell_id: String, secs: i64| -> bool {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(mut c)) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    c.spell_cooldowns.insert(spell_id, now + secs);
                    cloned_db.save_character_data(c).is_ok()
                }
                _ => false,
            }
        },
    );

    // recalculate_max_mana(char_name) -> i64 (new max_mana value)
    // Formula: 50 + (magic_skill * 10) + (max(0, effective_int - 10) * 5)
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("recalculate_max_mana", move |char_name: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut c)) => {
                let magic_level = c.skills.get("magic").map(|s| s.level).unwrap_or(0);
                // Calculate effective INT (base + buffs + racial)
                let mut effective_int = c.stat_int;
                let buff_bonus: i32 = c
                    .active_buffs
                    .iter()
                    .filter(|b| b.effect_type == EffectType::IntelligenceBoost)
                    .map(|b| b.magnitude)
                    .sum();
                effective_int += buff_bonus;
                // Add racial stat modifiers
                let world = state_clone.lock().unwrap();
                if let Some(race_def) = world.race_definitions.get(&c.race.to_lowercase()) {
                    if let Some(&modifier) = race_def.stat_modifiers.get("int") {
                        effective_int += modifier;
                    }
                }
                drop(world);

                let new_max = 50 + (magic_level * 10) + ((effective_int - 10).max(0) * 5);
                c.max_mana = new_max;
                if c.mana > c.max_mana {
                    c.mana = c.max_mana;
                }
                let _ = cloned_db.save_character_data(c);
                new_max as i64
            }
            _ => 0,
        }
    });

    // consume_reagent(char_name, reagent_vnum) -> bool
    // Finds one item with matching vnum in player inventory and deletes it
    let cloned_db = db.clone();
    engine.register_fn(
        "consume_reagent",
        move |char_name: String, reagent_vnum: String| -> bool {
            if reagent_vnum.is_empty() {
                return true; // No reagent needed
            }
            let items = match cloned_db.get_items_in_inventory(&char_name.to_lowercase()) {
                Ok(items) => items,
                Err(_) => return false,
            };
            for item in items {
                if item.vnum.as_deref() == Some(&reagent_vnum) {
                    return cloned_db.delete_item(&item.id).is_ok();
                }
            }
            false
        },
    );

    // has_reagent(char_name, reagent_vnum) -> bool
    let cloned_db = db.clone();
    engine.register_fn("has_reagent", move |char_name: String, reagent_vnum: String| -> bool {
        if reagent_vnum.is_empty() {
            return true;
        }
        let items = match cloned_db.get_items_in_inventory(&char_name.to_lowercase()) {
            Ok(items) => items,
            Err(_) => return false,
        };
        items.iter().any(|item| item.vnum.as_deref() == Some(&reagent_vnum))
    });

    // get_item_teaches_spell(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_teaches_spell", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => match item.teaches_spell {
                    Some(spell_id) => rhai::Dynamic::from(spell_id),
                    None => rhai::Dynamic::UNIT,
                },
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // set_item_teaches_spell(item_id, spell_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "set_item_teaches_spell",
        move |item_id: String, spell_id: String| -> bool {
            if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
                match cloned_db.get_item_data(&uuid) {
                    Ok(Some(mut item)) => {
                        item.teaches_spell = if spell_id.is_empty() { None } else { Some(spell_id) };
                        cloned_db.save_item_data(item).is_ok()
                    }
                    _ => false,
                }
            } else {
                false
            }
        },
    );
}
