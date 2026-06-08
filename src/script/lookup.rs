// src/script/lookup.rs
// Builder knowledge-base accessors: effects + skill cross-references.
// Spell + trait catalogs already have accessors in spells.rs / characters.rs.

use crate::SharedState;
use crate::db::Db;
use crate::types::{CustomSkillDefinition, EffectType, is_valid_custom_skill_key};
use rhai::Engine;
use std::sync::Arc;

/// Canonical skill names surfaced by `lookup skill`. Combines the static
/// `completion::consts::SKILL_NAMES` set with combat/magic skills referenced
/// via `skill_required` in spells/items. Hard-coded because no first-class
/// `SkillDefinition` catalog exists yet.
pub const KNOWN_SKILLS: &[&str] = &[
    "magic",
    "melee",
    "ranged",
    "stealth",
    "perception",
    "lockpick",
    "tracking",
    "sneak",
    "hide",
    "cooking",
    "crafting",
    "fishing",
    "foraging",
    "gardening",
    "swimming",
];

pub fn register(engine: &mut Engine, db: Arc<Db>, state: SharedState) {
    // get_effect_list() -> Array<String> — all EffectType variants as snake_case names.
    engine.register_fn("get_effect_list", || -> rhai::Array {
        EffectType::all()
            .into_iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect()
    });

    // get_effect_info(name) -> Map
    // Returns `#{name, display, recognized, used_by_spells}` — empty map if name
    // doesn't parse. `used_by_spells` is an Array of `#{id, name}` for spells
    // whose `buff_effect` matches this variant's display string.
    let state_clone = state.clone();
    engine.register_fn("get_effect_info", move |name: String| -> rhai::Map {
        let mut map = rhai::Map::new();
        let parsed = EffectType::from_str(&name);
        let display = match parsed {
            Some(e) => e.to_display_string().to_string(),
            None => return map,
        };
        map.insert("name".into(), rhai::Dynamic::from(name.clone()));
        map.insert("display".into(), rhai::Dynamic::from(display.clone()));
        map.insert("recognized".into(), rhai::Dynamic::from(true));
        let world = state_clone.lock().unwrap();
        let used: rhai::Array = world
            .spell_definitions
            .values()
            .filter(|s| s.buff_effect == display)
            .map(|s| {
                let mut entry = rhai::Map::new();
                entry.insert("id".into(), rhai::Dynamic::from(s.id.clone()));
                entry.insert("name".into(), rhai::Dynamic::from(s.name.clone()));
                rhai::Dynamic::from_map(entry)
            })
            .collect();
        map.insert("used_by_spells".into(), rhai::Dynamic::from(used));
        map
    });

    // get_skill_list() -> Array<String> — known skills (hard-coded; no catalog yet).
    engine.register_fn("get_skill_list", || -> rhai::Array {
        KNOWN_SKILLS
            .iter()
            .map(|s| rhai::Dynamic::from(s.to_string()))
            .collect()
    });

    // get_skill_references(skill_name) -> Map
    // Returns `#{name, spells, recipes}` where spells/recipes are Arrays of
    // `#{id_or_vnum, name, skill_required}` for entries that gate on this skill.
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("get_skill_references", move |skill_name: String| -> rhai::Map {
        let mut map = rhai::Map::new();
        let lower = skill_name.to_lowercase();
        map.insert("name".into(), rhai::Dynamic::from(lower.clone()));

        // Spells: scan world.spell_definitions. The "magic" skill is the
        // implicit gate for all spells with `skill_required > 0`; other
        // combat skills (melee/ranged/stealth) don't gate spells in v1.
        let spells: rhai::Array = if lower == "magic" {
            let world = state_clone.lock().unwrap();
            world
                .spell_definitions
                .values()
                .filter(|s| s.skill_required > 0)
                .map(|s| {
                    let mut entry = rhai::Map::new();
                    entry.insert("id".into(), rhai::Dynamic::from(s.id.clone()));
                    entry.insert("name".into(), rhai::Dynamic::from(s.name.clone()));
                    entry.insert("skill_required".into(), rhai::Dynamic::from(s.skill_required as i64));
                    rhai::Dynamic::from_map(entry)
                })
                .collect()
        } else {
            rhai::Array::new()
        };
        map.insert("spells".into(), rhai::Dynamic::from(spells));

        // Recipes: filter by `Recipe.skill == skill_name`.
        let recipes: rhai::Array = match cloned_db.list_all_recipes() {
            Ok(list) => list
                .into_iter()
                .filter(|r| r.skill.to_lowercase() == lower)
                .map(|r| {
                    let mut entry = rhai::Map::new();
                    entry.insert("id".into(), rhai::Dynamic::from(r.id.clone()));
                    entry.insert("name".into(), rhai::Dynamic::from(r.name.clone()));
                    entry.insert("skill_required".into(), rhai::Dynamic::from(r.skill_required as i64));
                    rhai::Dynamic::from_map(entry)
                })
                .collect(),
            Err(_) => rhai::Array::new(),
        };
        map.insert("recipes".into(), rhai::Dynamic::from(recipes));

        map
    });

    // ========== Custom skill registry ==========

    // list_custom_skills() -> Array<String>
    // Returns every published custom-skill key, sorted ascending.
    let state_clone = state.clone();
    engine.register_fn("list_custom_skills", move || -> rhai::Array {
        let world = state_clone.lock().unwrap();
        let mut keys: Vec<String> = world.custom_skill_definitions.keys().cloned().collect();
        keys.sort();
        keys.into_iter().map(rhai::Dynamic::from).collect()
    });

    // is_custom_skill(key) -> bool
    // True iff the key is published in the registry. Used by writers to
    // refuse ghost keys.
    let state_clone = state.clone();
    engine.register_fn("is_custom_skill", move |key: String| -> bool {
        let world = state_clone.lock().unwrap();
        world.custom_skill_definitions.contains_key(&key.to_lowercase())
    });

    // get_custom_skill_info(key) -> Map
    // Returns `#{key, description, author, created_at}` for a published key
    // or an empty map if not found.
    let state_clone = state.clone();
    engine.register_fn("get_custom_skill_info", move |key: String| -> rhai::Map {
        let mut map = rhai::Map::new();
        let world = state_clone.lock().unwrap();
        if let Some(def) = world.custom_skill_definitions.get(&key.to_lowercase()) {
            map.insert("key".into(), rhai::Dynamic::from(def.key.clone()));
            map.insert("description".into(), rhai::Dynamic::from(def.description.clone()));
            map.insert("author".into(), rhai::Dynamic::from(def.author.clone()));
            map.insert("created_at".into(), rhai::Dynamic::from(def.created_at));
        }
        map
    });

    // publish_custom_skill(key, description, author) -> bool
    // Validates key (syntax + no collision with KNOWN_SKILLS), upserts into
    // sled + cache, and returns true on success. Returns false on invalid
    // key or collision; callers can fall back to `is_custom_skill` /
    // `get_custom_skill_info` for diagnostics.
    let state_clone = state.clone();
    let cloned_db = db.clone();
    engine.register_fn(
        "publish_custom_skill",
        move |key: String, description: String, author: String| -> bool {
            let key_lc = key.to_lowercase();
            if !is_valid_custom_skill_key(&key_lc) {
                return false;
            }
            if KNOWN_SKILLS.iter().any(|s| *s == key_lc.as_str()) {
                return false;
            }
            let def = CustomSkillDefinition {
                key: key_lc.clone(),
                description,
                author,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            };
            if cloned_db.save_custom_skill(&def).is_err() {
                return false;
            }
            let mut world = state_clone.lock().unwrap();
            world.custom_skill_definitions.insert(key_lc, def);
            true
        },
    );

    // unpublish_custom_skill(key) -> bool
    // Removes the key from sled + cache. Returns true if removed, false if
    // it wasn't present or the DB op failed. Gating (author-only,
    // admin-override) is enforced by the Rhai script caller.
    let state_clone = state.clone();
    let cloned_db = db.clone();
    engine.register_fn("unpublish_custom_skill", move |key: String| -> bool {
        let key_lc = key.to_lowercase();
        match cloned_db.delete_custom_skill(&key_lc) {
            Ok(true) => {
                let mut world = state_clone.lock().unwrap();
                world.custom_skill_definitions.remove(&key_lc);
                true
            }
            _ => false,
        }
    });
}
