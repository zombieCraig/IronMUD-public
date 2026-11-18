// src/script/spells.rs
// Spell system functions: spell definitions, cooldowns, mana, and reagents

use crate::EffectType;
use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;
use crate::script::achievements::apply_to_character;
use rhai::Engine;
use std::collections::HashMap;
use std::sync::Arc;

pub fn register(engine: &mut Engine, db: Arc<Db>, connections: SharedConnections, state: SharedState) {
    // get_spell_list() -> Array of spell IDs
    let state_clone = state.clone();
    engine.register_fn("get_spell_list", move || -> rhai::Array {
        let world = state_clone.lock();
        world
            .spell_definitions
            .keys()
            .map(|k| rhai::Dynamic::from(k.clone()))
            .collect()
    });

    // get_spell_info(spell_id) -> Map with all spell fields, or empty map
    let state_clone = state.clone();
    engine.register_fn("get_spell_info", move |spell_id: String| -> rhai::Map {
        let world = state_clone.lock();
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
        let world = state_clone.lock();
        let mut available = rhai::Array::new();
        for (id, spell) in &world.spell_definitions {
            if !identity_allows(spell, &char) {
                continue;
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

    // get_eligible_spells(char_name) -> Array of spell IDs the character could
    // ever cast: the identity gates of `get_available_spells` (vampire, clan,
    // werewolf, tribe) without the skill or scroll gates. This is what the
    // `spells` listing shows, so a mage isn't shown every vampire discipline.
    // Codex spells appear only once learned; `spells codex` is where the
    // world's divine spells are browsed (and where Lost/vetoed stay hidden).
    let cloned_db = db.clone();
    let state_clone = state.clone();
    engine.register_fn("get_eligible_spells", move |char_name: String| -> rhai::Array {
        let char = match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(c)) => c,
            _ => return rhai::Array::new(),
        };
        let world = state_clone.lock();
        world
            .spell_definitions
            .iter()
            .filter(|(id, spell)| identity_allows(spell, &char) && (!spell.codex || char.learned_spells.contains(id)))
            .map(|(id, _)| rhai::Dynamic::from(id.clone()))
            .collect()
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
    let cloned_conns = connections.clone();
    engine.register_fn("learn_spell", move |char_name: String, spell_id: String| -> bool {
        learn_spell(&cloned_db, &cloned_conns, &char_name, &spell_id)
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
    let cloned_conns = connections.clone();
    engine.register_fn(
        "set_spell_cooldown",
        move |char_name: String, spell_id: String, secs: i64| -> bool {
            set_spell_cooldown(&cloned_db, &cloned_conns, &char_name, &spell_id, secs)
        },
    );

    // recalculate_max_mana(char_name) -> i64 (new max_mana value)
    // Formula: 50 + (magic_skill * 10) + (max(0, effective_int - 10) * 5)
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let state_clone = state.clone();
    engine.register_fn("recalculate_max_mana", move |char_name: String| -> i64 {
        recalculate_max_mana(&cloned_db, &cloned_conns, &state_clone, &char_name)
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
    // See `add_spell_experience` (the Rust core) for leveling and evolution.
    let cloned_db = db.clone();
    let cloned_conns = connections.clone();
    let state_clone = state.clone();
    engine.register_fn(
        "add_spell_experience",
        move |char_name: String, spell_id: String, amount: i64| -> rhai::Map {
            let outcome = add_spell_experience(&cloned_db, &cloned_conns, &state_clone, &char_name, &spell_id, amount);
            let mut out = rhai::Map::new();
            out.insert("leveled_up".into(), rhai::Dynamic::from(outcome.leveled_up));
            out.insert("evolved".into(), rhai::Dynamic::from(outcome.evolved_to.is_some()));
            out.insert(
                "new_spell_id".into(),
                rhai::Dynamic::from(outcome.evolved_to.clone().unwrap_or_default()),
            );
            out.insert("new_level".into(), rhai::Dynamic::from(outcome.new_level as i64));
            out
        },
    );
}

/// The identity gates on a spell: vampire/werewolf state and clan/tribe
/// traits. A spell that fails these can never be cast by this character,
/// however much skill they gain.
fn identity_allows(spell: &crate::SpellDefinition, char: &crate::CharacterData) -> bool {
    if spell.requires_vampire && char.vampire_state.is_none() {
        return false;
    }
    if !spell.requires_clan.is_empty() && !spell.requires_clan.iter().any(|c| char.traits.iter().any(|t| t == c)) {
        return false;
    }
    if spell.requires_werewolf && char.werewolf_state.is_none() {
        return false;
    }
    if !spell.requires_tribe.is_empty() && !spell.requires_tribe.iter().any(|c| char.traits.iter().any(|t| t == c)) {
        return false;
    }
    true
}

// ========== Session-first spell writers ==========
//
// Every write below goes through `achievements::apply_to_character`, which
// mutates the online player's *session* copy (and saves it) or, for an offline
// player, the DB record. A DB-only write to an online player is reverted the
// next time a tick flushes `session.character` wholesale (regen, hunger,
// thirst — see `src/ticks/character.rs`), which is how learned spells,
// cooldowns and per-spell mastery used to vanish.
//
// `apply_to_character` runs its closure under the Connections lock, so none of
// these closures may take the World lock: anything they need from World is
// read first and captured.

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Add `spell_id` to the character's learned spells. True if the character
/// now knows the spell (including when it already did).
pub fn learn_spell(db: &Db, connections: &SharedConnections, char_name: &str, spell_id: &str) -> bool {
    crate::spellcraft::add_learned_spell(db, connections, char_name, spell_id)
        != crate::spellcraft::LearnResult::NoCharacter
}

/// Put `spell_id` on cooldown for `secs` seconds from now.
pub fn set_spell_cooldown(
    db: &Db,
    connections: &SharedConnections,
    char_name: &str,
    spell_id: &str,
    secs: i64,
) -> bool {
    let ready_at = now_secs() + secs;
    apply_to_character(db, connections, char_name, |c| {
        c.spell_cooldowns.insert(spell_id.to_string(), ready_at);
        true
    })
    .is_some()
}

/// Recompute and store `max_mana`, clamping current mana to the new ceiling.
/// Returns the new base max (0 if the character can't be found).
/// Formula: 50 + (magic_skill * 10) + (max(0, effective_int - 10) * 5)
pub fn recalculate_max_mana(db: &Db, connections: &SharedConnections, state: &SharedState, char_name: &str) -> i64 {
    // Racial INT modifiers, read before the Connections lock is taken.
    let int_mods: HashMap<String, i32> = {
        let world = state.lock();
        world
            .race_definitions
            .iter()
            .filter_map(|(id, race)| race.stat_modifiers.get("int").map(|m| (id.clone(), *m)))
            .collect()
    };

    let mut new_max = 0i64;
    let found = apply_to_character(db, connections, char_name, |c| {
        let magic_level = c.skills.get("magic").map(|s| s.level).unwrap_or(0);
        let buff_bonus: i32 = c
            .active_buffs
            .iter()
            .filter(|b| b.effect_type == EffectType::IntelligenceBoost)
            .map(|b| b.magnitude)
            .sum();
        let racial = int_mods.get(&c.race.to_lowercase()).copied().unwrap_or(0);
        let effective_int = c.stat_int + buff_bonus + racial;

        let max = 50 + (magic_level * 10) + ((effective_int - 10).max(0) * 5);
        // Stored net of the divine mana pledge (spellcraft::price).
        c.max_mana = c.after_pledges(max as i64, crate::types::PledgeKind::Mana) as i32;
        // Clamp at effective ceiling (base + MaxManaBonus buffs).
        let eq_bonus: i32 = c
            .active_buffs
            .iter()
            .filter(|b| b.effect_type == EffectType::MaxManaBonus)
            .map(|b| b.magnitude)
            .sum();
        let effective = c.max_mana + eq_bonus;
        if c.mana > effective {
            c.mana = effective;
        }
        new_max = max as i64;
        true
    });
    if found.is_some() { new_max } else { 0 }
}

/// Result of [`add_spell_experience`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SpellXpOutcome {
    pub leveled_up: bool,
    /// The spell this one evolved into, if it did.
    pub evolved_to: Option<String>,
    /// Level of the spell now held: the evolved spell's (1) after an evolution.
    pub new_level: i32,
    /// The spell reached mastery 10 in this call (true even if it then
    /// evolved). Happens once per spell.
    pub mastered: bool,
}

/// Award per-spell mastery XP. Handles in-place leveling AND atomic evolution:
/// if the spell defines `evolves_to` and the new level reaches the threshold,
/// swap the spell ID in `learned_spells`, drop the old cooldown + progress
/// entries, and seed a fresh SpellProgress for the evolved spell. The evolved
/// spell does NOT chain-evolve in the same call.
pub fn add_spell_experience(
    db: &Db,
    connections: &SharedConnections,
    state: &SharedState,
    char_name: &str,
    spell_id: &str,
    amount: i64,
) -> SpellXpOutcome {
    // Re-read the SpellDefinition each call so JSON hot-reloads pick up new
    // chains. Done before apply_to_character: World is never locked under
    // Connections. A target that doesn't exist is treated as no evolution.
    let evolve_target: Option<(i32, String)> = {
        let world = state.lock();
        world
            .spell_definitions
            .get(spell_id)
            .and_then(|s| s.evolves_to.as_ref())
            .filter(|ev| world.spell_definitions.contains_key(&ev.spell_id))
            .map(|ev| (ev.level_required, ev.spell_id.clone()))
    };

    // Codex spells cap learners below mastery; the author reaches 10.
    let cap = crate::spellcraft::canon::mastery_cap(db, char_name, spell_id);

    let mut out = SpellXpOutcome::default();
    apply_to_character(db, connections, char_name, |char| {
        let already_at_cap = char
            .spell_progress
            .entry(spell_id.to_string())
            .or_insert_with(crate::SpellProgress::default)
            .level
            >= cap;

        if !already_at_cap {
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
                .entry(spell_id.to_string())
                .or_insert_with(crate::SpellProgress::default);
            entry.experience += xp;

            loop {
                let xp_needed = crate::script::characters::xp_for_level(entry.level);
                if xp_needed == 0 || entry.experience < xp_needed || entry.level >= cap {
                    break;
                }
                entry.experience -= xp_needed;
                entry.level += 1;
                out.leveled_up = true;
                if entry.level >= cap {
                    entry.experience = 0;
                    out.mastered = entry.level >= 10;
                    break;
                }
            }
        }

        out.new_level = char.spell_progress.get(spell_id).map(|p| p.level).unwrap_or(0);

        if let Some((req, new_id)) = &evolve_target {
            let has_old = char.learned_spells.iter().any(|s| s == spell_id);
            let already_has_new = char.learned_spells.iter().any(|s| s == new_id);
            if out.new_level >= *req && has_old && !already_has_new {
                // Swap ID in learned_spells (preserve order).
                if let Some(slot) = char.learned_spells.iter_mut().find(|s| *s == spell_id) {
                    *slot = new_id.clone();
                }
                char.spell_cooldowns.remove(spell_id);
                char.spell_progress.remove(spell_id);
                char.spell_progress.insert(
                    new_id.clone(),
                    crate::SpellProgress {
                        level: 1,
                        experience: 0,
                    },
                );
                out.evolved_to = Some(new_id.clone());
                out.leveled_up = false;
                out.new_level = 1;
            }
        }
        if out.mastered {
            *char
                .achievement_counters
                .entry(crate::spellcraft::gate::SPELLS_MASTERED_COUNTER.to_string())
                .or_insert(0) += 1;
        }
        true
    });
    // After the write, never inside it: award_xp takes its own locks.
    if out.mastered {
        crate::spellcraft::on_spell_mastered(db, connections, state, char_name);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharacterData, PlayerSession, SpellDefinition, World};
    use parking_lot::Mutex;

    fn temp_db() -> (Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let db = Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn saved(db: &Db, name: &str) -> CharacterData {
        let ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": name,
            "password_hash": "",
            "current_room_id": uuid::Uuid::new_v4(),
        }))
        .expect("build character");
        db.save_character_data(ch.clone()).expect("save");
        ch
    }

    /// Put a character online so the session-first path is the one under test.
    fn online(ch: &CharacterData) -> SharedConnections {
        let conns: SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut session = PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(ch.clone());
        conns.lock().insert(uuid::Uuid::new_v4(), session);
        std::mem::forget((rx, rx_in));
        conns
    }

    fn session_char(conns: &SharedConnections, name: &str) -> CharacterData {
        conns
            .lock()
            .values()
            .find_map(|s| s.character.clone().filter(|c| c.name.eq_ignore_ascii_case(name)))
            .expect("character is online")
    }

    /// What the regen/hunger/thirst ticks do every minute: write the session
    /// copy over the DB record wholesale.
    fn flush_session(db: &Db, conns: &SharedConnections, name: &str) {
        db.save_character_data(session_char(conns, name)).expect("flush");
    }

    fn db_char(db: &Db, name: &str) -> CharacterData {
        db.get_character_data(name).expect("read").expect("exists")
    }

    fn spell(json: serde_json::Value) -> SpellDefinition {
        serde_json::from_value(json).expect("build spell")
    }

    fn world_with(db: &Db, conns: &SharedConnections, spells: Vec<SpellDefinition>) -> SharedState {
        let state = World::minimal_shared(db.clone(), conns.clone());
        {
            let mut world = state.lock();
            for s in spells {
                world.spell_definitions.insert(s.id.clone(), s);
            }
        }
        state
    }

    #[test]
    fn a_learned_spell_survives_a_session_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "mage");
        let conns = online(&ch);

        assert!(learn_spell(&db, &conns, "mage", "fireball"));
        assert!(
            session_char(&conns, "mage")
                .learned_spells
                .contains(&"fireball".to_string())
        );

        flush_session(&db, &conns, "mage");
        assert!(
            db_char(&db, "mage").learned_spells.contains(&"fireball".to_string()),
            "the tick flush must not forget the spell"
        );

        // Learning twice is a no-op, still reported as known.
        assert!(learn_spell(&db, &conns, "mage", "fireball"));
        assert_eq!(session_char(&conns, "mage").learned_spells.len(), 1);
    }

    #[test]
    fn a_spell_cooldown_survives_a_session_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "mage");
        let conns = online(&ch);

        assert!(set_spell_cooldown(&db, &conns, "mage", "fireball", 60));
        flush_session(&db, &conns, "mage");
        let ready_at = *db_char(&db, "mage")
            .spell_cooldowns
            .get("fireball")
            .expect("cooldown kept");
        assert!(ready_at > now_secs(), "cooldown is in the future");
    }

    #[test]
    fn spell_mastery_reaches_the_session_and_survives_a_flush() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "mage");
        let conns = online(&ch);
        let state = world_with(
            &db,
            &conns,
            vec![spell(
                serde_json::json!({"id": "spark", "name": "Spark", "description": ""}),
            )],
        );

        let needed = crate::script::characters::xp_for_level(0) as i64;
        let outcome = add_spell_experience(&db, &conns, &state, "mage", "spark", needed);
        assert!(outcome.leveled_up);
        assert_eq!(outcome.new_level, 1);

        // cast.rhai re-saves the session copy right after awarding XP; the XP
        // has to be in that copy or the save erases it.
        assert_eq!(session_char(&conns, "mage").spell_progress["spark"].level, 1);
        flush_session(&db, &conns, "mage");
        assert_eq!(db_char(&db, "mage").spell_progress["spark"].level, 1);
    }

    #[test]
    fn evolution_swaps_the_learned_spell() {
        let (db, _t) = temp_db();
        let mut ch = saved(&db, "mage");
        ch.learned_spells.push("missile".into());
        ch.spell_progress.insert(
            "missile".into(),
            crate::SpellProgress {
                level: 1,
                experience: 0,
            },
        );
        db.save_character_data(ch.clone()).expect("save");
        let conns = online(&ch);
        let state = world_with(
            &db,
            &conns,
            vec![
                spell(serde_json::json!({
                    "id": "missile", "name": "Missile", "description": "",
                    "evolves_to": {"level_required": 2, "spell_id": "lance"}
                })),
                spell(serde_json::json!({"id": "lance", "name": "Lance", "description": ""})),
            ],
        );

        let needed = crate::script::characters::xp_for_level(1) as i64;
        let outcome = add_spell_experience(&db, &conns, &state, "mage", "missile", needed);
        assert_eq!(outcome.evolved_to.as_deref(), Some("lance"));
        assert_eq!(outcome.new_level, 1);

        let c = session_char(&conns, "mage");
        assert_eq!(c.learned_spells, vec!["lance".to_string()]);
        assert!(!c.spell_progress.contains_key("missile"));
        assert_eq!(c.spell_progress["lance"].level, 1);
    }

    #[test]
    fn max_mana_recalculation_reaches_the_session() {
        let (db, _t) = temp_db();
        let mut ch = saved(&db, "mage");
        ch.stat_int = 14;
        ch.mana = 999;
        db.save_character_data(ch.clone()).expect("save");
        let conns = online(&ch);
        let state = world_with(&db, &conns, vec![]);

        let new_max = recalculate_max_mana(&db, &conns, &state, "mage");
        assert_eq!(new_max, 50 + 4 * 5);
        let c = session_char(&conns, "mage");
        assert_eq!(c.max_mana, 70);
        assert_eq!(c.mana, 70, "current mana clamps to the new ceiling");
    }

    #[test]
    fn identity_gates_hide_other_kinds_spells() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "mage");
        let vamp = spell(serde_json::json!({
            "id": "dominate", "name": "Dominate", "description": "", "requires_vampire": true
        }));
        let gift = spell(serde_json::json!({
            "id": "howl", "name": "Howl", "description": "", "requires_werewolf": true
        }));
        let clan = spell(serde_json::json!({
            "id": "clan_art", "name": "Clan Art", "description": "", "requires_clan": ["clan_x"]
        }));
        let open = spell(serde_json::json!({
            "id": "spark", "name": "Spark", "description": "", "skill_required": 9
        }));
        assert!(!identity_allows(&vamp, &ch));
        assert!(!identity_allows(&gift, &ch));
        assert!(!identity_allows(&clan, &ch));
        assert!(
            identity_allows(&open, &ch),
            "skill gates don't hide a spell from the listing"
        );
    }
}
