// src/script/lookup.rs
// Builder knowledge-base accessors: effects + skill cross-references.
// Spell + trait catalogs already have accessors in spells.rs / characters.rs.

use crate::SharedState;
use crate::db::Db;
use crate::types::EffectType;
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
    engine.register_fn(
        "get_skill_references",
        move |skill_name: String| -> rhai::Map {
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
                        entry.insert(
                            "skill_required".into(),
                            rhai::Dynamic::from(s.skill_required as i64),
                        );
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
                        entry.insert(
                            "skill_required".into(),
                            rhai::Dynamic::from(r.skill_required as i64),
                        );
                        rhai::Dynamic::from_map(entry)
                    })
                    .collect(),
                Err(_) => rhai::Array::new(),
            };
            map.insert("recipes".into(), rhai::Dynamic::from(recipes));

            map
        },
    );
}
