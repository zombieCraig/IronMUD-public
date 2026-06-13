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
            map.insert(
                "requires_skill".into(),
                rhai::Dynamic::from(spell.requires_skill.clone().unwrap_or_else(|| "magic".to_string())),
            );
            map.insert("requires_vampire".into(), rhai::Dynamic::from(spell.requires_vampire));
            let clans: Vec<rhai::Dynamic> = spell
                .requires_clan
                .iter()
                .map(|c| rhai::Dynamic::from(c.clone()))
                .collect();
            map.insert("requires_clan".into(), rhai::Dynamic::from(clans));
            map.insert("requires_werewolf".into(), rhai::Dynamic::from(spell.requires_werewolf));
            let tribes: Vec<rhai::Dynamic> = spell
                .requires_tribe
                .iter()
                .map(|t| rhai::Dynamic::from(t.clone()))
                .collect();
            map.insert("requires_tribe".into(), rhai::Dynamic::from(tribes));
            map.insert(
                "damage_per_spell_level".into(),
                rhai::Dynamic::from(spell.damage_per_spell_level as i64),
            );
            map.insert(
                "heal_per_spell_level".into(),
                rhai::Dynamic::from(spell.heal_per_spell_level as i64),
            );
            map.insert(
                "buff_magnitude_per_spell_level".into(),
                rhai::Dynamic::from(spell.buff_magnitude_per_spell_level as i64),
            );
            map.insert(
                "buff_duration_per_spell_level".into(),
                rhai::Dynamic::from(spell.buff_duration_per_spell_level as i64),
            );
            let (ev_id, ev_lvl) = match &spell.evolves_to {
                Some(ev) => (ev.spell_id.clone(), ev.level_required as i64),
                None => (String::new(), 0),
            };
            map.insert("evolves_to_id".into(), rhai::Dynamic::from(ev_id));
            map.insert("evolves_at_level".into(), rhai::Dynamic::from(ev_lvl));
        }
        map
    });

    // get_available_spells(char_name) -> Array of spell ID strings the character can cast
    //
    // Gating order:
    //   1. requires_vampire — non-vampires never see vampire-only spells.
    //   2. requires_clan    — caster must have at least one of the named clan_* traits.
    //   3. skill_required   — checks the spell's `requires_skill` (or "magic" by default).
    //   4. scroll_only      — must be in `learned_spells`.
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("get_available_spells", move |char_name: String| -> rhai::Array {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return rhai::Array::new(),
        };
        let world = state_clone.lock().unwrap();
        let mut available = rhai::Array::new();
        for (id, spell) in &world.spell_definitions {
            if spell.requires_vampire && char.vampire_state.is_none() {
                continue;
            }
            if !spell.requires_clan.is_empty() {
                let has_clan = spell.requires_clan.iter().any(|c| char.traits.iter().any(|t| t == c));
                if !has_clan {
                    continue;
                }
            }
            if spell.requires_werewolf && char.werewolf_state.is_none() {
                continue;
            }
            if !spell.requires_tribe.is_empty() {
                let has_tribe = spell.requires_tribe.iter().any(|c| char.traits.iter().any(|t| t == c));
                if !has_tribe {
                    continue;
                }
            }
            let gate_skill = spell.requires_skill.as_deref().unwrap_or("magic");
            let level = char.skills.get(gate_skill).map(|s| s.level).unwrap_or(0);
            if level < spell.skill_required {
                continue;
            }
            if spell.scroll_only {
                if !char.learned_spells.contains(id) {
                    continue;
                }
            }
            available.push(rhai::Dynamic::from(id.clone()));
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
                // Clamp at effective ceiling (base + MaxManaBonus buffs).
                let eq_bonus: i32 = c
                    .active_buffs
                    .iter()
                    .filter(|b| b.effect_type == crate::EffectType::MaxManaBonus)
                    .map(|b| b.magnitude)
                    .sum();
                let effective = c.max_mana + eq_bonus;
                if c.mana > effective {
                    c.mana = effective;
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

    // count_reagent(char_name, reagent_vnum) -> i64
    // How many items with this vnum the player carries (synth repair kits
    // need two at once for a critical chassis).
    let cloned_db = db.clone();
    engine.register_fn("count_reagent", move |char_name: String, reagent_vnum: String| -> i64 {
        if reagent_vnum.is_empty() {
            return 0;
        }
        let items = match cloned_db.get_items_in_inventory(&char_name.to_lowercase()) {
            Ok(items) => items,
            Err(_) => return 0,
        };
        items
            .iter()
            .filter(|item| item.vnum.as_deref() == Some(&reagent_vnum))
            .count() as i64
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

    // ========== Per-Spell Mastery ==========
    // Each learned spell tracks its own level + XP (0-10 cap, same curve as
    // skills). The unified `magic` skill XP is awarded separately by cast.rhai
    // and is unaffected by these fns.

    // get_spell_level(char_name, spell_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_spell_level", move |char_name: String, spell_id: String| -> i64 {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c.spell_progress.get(&spell_id).map(|p| p.level as i64).unwrap_or(0),
            _ => 0,
        }
    });

    // get_spell_experience(char_name, spell_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn(
        "get_spell_experience",
        move |char_name: String, spell_id: String| -> i64 {
            match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c
                    .spell_progress
                    .get(&spell_id)
                    .map(|p| p.experience as i64)
                    .unwrap_or(0),
                _ => 0,
            }
        },
    );

    // get_all_spell_progress(char_name) -> Map of spell_id -> {level, experience, xp_to_level}
    let cloned_db = db.clone();
    engine.register_fn("get_all_spell_progress", move |char_name: String| -> rhai::Map {
        let mut result = rhai::Map::new();
        if let Ok(Some(c)) = cloned_db.get_character_data(&char_name.to_lowercase()) {
            for (id, progress) in &c.spell_progress {
                let mut entry = rhai::Map::new();
                entry.insert("level".into(), rhai::Dynamic::from(progress.level as i64));
                entry.insert("experience".into(), rhai::Dynamic::from(progress.experience as i64));
                entry.insert(
                    "xp_to_level".into(),
                    rhai::Dynamic::from(crate::script::characters::xp_for_level(progress.level) as i64),
                );
                result.insert(id.clone().into(), rhai::Dynamic::from(entry));
            }
        }
        result
    });

    // add_spell_experience(char_name, spell_id, amount) -> Map
    //   { leveled_up: bool, evolved: bool, new_spell_id: String, new_level: i64 }
    // Handles in-place leveling AND atomic evolution: if the spell defines
    // `evolves_to` and the new level reaches the threshold, swap the spell ID
    // in `learned_spells`, drop the old cooldown + progress entries, and seed
    // a fresh SpellProgress for the evolved spell. The evolved spell does NOT
    // chain-evolve in the same call.
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn(
        "add_spell_experience",
        move |char_name: String, spell_id: String, amount: i64| -> rhai::Map {
            let mut out = rhai::Map::new();
            out.insert("leveled_up".into(), rhai::Dynamic::from(false));
            out.insert("evolved".into(), rhai::Dynamic::from(false));
            out.insert("new_spell_id".into(), rhai::Dynamic::from(String::new()));
            out.insert("new_level".into(), rhai::Dynamic::from(0i64));

            let mut char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return out,
            };

            let entry = char
                .spell_progress
                .entry(spell_id.clone())
                .or_insert_with(crate::SpellProgress::default);

            // Track whether we did anything that needs persisting.
            let already_at_cap_no_evolve = entry.level >= 10;

            let mut leveled_up = false;
            if !already_at_cap_no_evolve {
                // Apply learning-rate trait modifiers (mirror skill XP path).
                let has_prodigy = char.traits.iter().any(|t| t == "prodigy");
                let has_quick_study = char.traits.iter().any(|t| t == "quick_study");
                let has_slow_learner = char.traits.iter().any(|t| t == "slow_learner");
                let mut xp = amount as i32;
                if has_prodigy {
                    xp = xp * 150 / 100;
                } else if has_quick_study {
                    xp = xp * 125 / 100;
                }
                if has_slow_learner {
                    xp = xp * 65 / 100;
                }
                xp = xp.max(1);

                let entry = char
                    .spell_progress
                    .entry(spell_id.clone())
                    .or_insert_with(crate::SpellProgress::default);
                entry.experience += xp;

                loop {
                    let xp_needed = crate::script::characters::xp_for_level(entry.level);
                    if xp_needed == 0 || entry.experience < xp_needed || entry.level >= 10 {
                        break;
                    }
                    entry.experience -= xp_needed;
                    entry.level += 1;
                    leveled_up = true;
                    if entry.level >= 10 {
                        entry.experience = 0;
                        break;
                    }
                }
            }

            let final_level = char.spell_progress.get(&spell_id).map(|p| p.level).unwrap_or(0);

            // Check evolution. Re-lookup the SpellDefinition each call so JSON
            // hot-reloads pick up new chains.
            let evolve_target: Option<(i32, String)> = {
                let world = state_clone.lock().unwrap();
                world
                    .spell_definitions
                    .get(&spell_id)
                    .and_then(|s| s.evolves_to.as_ref())
                    .map(|ev| (ev.level_required, ev.spell_id.clone()))
            };

            let mut evolved_to: Option<String> = None;
            if let Some((req, new_id)) = evolve_target {
                if final_level >= req {
                    // Verify the evolved target exists and is not already learned.
                    let target_exists = state_clone
                        .lock()
                        .ok()
                        .map(|w| w.spell_definitions.contains_key(&new_id))
                        .unwrap_or(false);
                    let already_has_new = char.learned_spells.iter().any(|s| s == &new_id);
                    let has_old = char.learned_spells.iter().any(|s| s == &spell_id);
                    if target_exists && has_old && !already_has_new {
                        // Swap ID in learned_spells (preserve order).
                        for slot in char.learned_spells.iter_mut() {
                            if slot == &spell_id {
                                *slot = new_id.clone();
                                break;
                            }
                        }
                        char.spell_cooldowns.remove(&spell_id);
                        char.spell_progress.remove(&spell_id);
                        char.spell_progress.insert(
                            new_id.clone(),
                            crate::SpellProgress {
                                level: 1,
                                experience: 0,
                            },
                        );
                        evolved_to = Some(new_id);
                    }
                }
            }

            let _ = cloned_db.save_character_data(char);

            if let Some(new_id) = &evolved_to {
                out.insert("evolved".into(), rhai::Dynamic::from(true));
                out.insert("new_spell_id".into(), rhai::Dynamic::from(new_id.clone()));
                out.insert("new_level".into(), rhai::Dynamic::from(1i64));
            } else {
                out.insert("leveled_up".into(), rhai::Dynamic::from(leveled_up));
                out.insert("new_level".into(), rhai::Dynamic::from(final_level as i64));
            }
            out
        },
    );
}
