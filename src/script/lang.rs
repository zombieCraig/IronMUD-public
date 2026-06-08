//! Language Rhai bindings: garble, registry lookup, language listing for
//! the `speak` and `languages` commands.
//!
//! Languages share storage with skills (see `CharacterData.skills`). The
//! `LanguageRegistry` (held on `World.language_definitions`) decides which
//! skill keys to surface as languages and supplies phonetic word pools used
//! to garble speech for listeners with insufficient skill.

use std::collections::HashMap;
use std::sync::Arc;

use rand::Rng;
use rhai::Engine;
use uuid::Uuid;

use crate::SharedState;
use crate::db::Db;
use crate::types::LanguageDefinition;

/// Apply `LanguageDefinition`-driven garble to `text` based on `effective_skill`.
/// - Lingua francas pass through unchanged.
/// - Each word survives with probability `effective_skill/10`; otherwise it's
///   replaced with a random entry from the language's phonetic pool. Punctuation
///   and leading-capital case are preserved.
pub fn garble_text(text: &str, lang: &LanguageDefinition, effective_skill: i32) -> String {
    if lang.is_lingua_franca {
        return text.to_string();
    }
    if lang.phonetic_words.is_empty() {
        return "<garbled speech>".to_string();
    }
    let skill = effective_skill.clamp(0, 10) as f32;
    let pass_prob = skill / 10.0;
    let mut rng = rand::thread_rng();
    let mut out: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        if rng.r#gen::<f32>() < pass_prob {
            out.push(word.to_string());
            continue;
        }
        out.push(garble_one_word(word, &lang.phonetic_words, &mut rng));
    }
    out.join(" ")
}

/// Listener-aware wrapper around `garble_text`. Admin and lingua-franca
/// listeners short-circuit to plain `text`. An unknown language or empty
/// key passes through unchanged (callers handle the None-language case
/// upstream by short-circuiting before calling this).
pub fn garble_for_listener(
    text: &str,
    language_key: &str,
    listener_skill: i32,
    listener_is_admin: bool,
    languages: &HashMap<String, LanguageDefinition>,
) -> String {
    if listener_is_admin || language_key.is_empty() {
        return text.to_string();
    }
    let lang = match languages.get(&language_key.to_lowercase()) {
        Some(l) => l,
        None => return text.to_string(),
    };
    garble_text(text, lang, listener_skill)
}

pub fn register(engine: &mut Engine, db: Arc<Db>, state: SharedState) {
    // is_language(key) -> bool
    {
        let state_clone = state.clone();
        engine.register_fn("is_language", move |key: String| -> bool {
            let world = state_clone.lock().unwrap();
            world.language_definitions.contains_key(&key.to_lowercase())
        });
    }

    // is_lingua_franca(key) -> bool
    {
        let state_clone = state.clone();
        engine.register_fn("is_lingua_franca", move |key: String| -> bool {
            let world = state_clone.lock().unwrap();
            world
                .language_definitions
                .get(&key.to_lowercase())
                .map(|d| d.is_lingua_franca)
                .unwrap_or(false)
        });
    }

    // get_language_display_name(key) -> String (display_name or key if missing)
    {
        let state_clone = state.clone();
        engine.register_fn("get_language_display_name", move |key: String| -> String {
            let world = state_clone.lock().unwrap();
            world
                .language_definitions
                .get(&key.to_lowercase())
                .map(|d| d.display_name.clone())
                .unwrap_or_else(|| key.clone())
        });
    }

    // get_language_keys() -> Array<String> of all registered language keys
    {
        let state_clone = state.clone();
        engine.register_fn("get_language_keys", move || -> rhai::Array {
            let world = state_clone.lock().unwrap();
            world
                .language_definitions
                .keys()
                .map(|k| rhai::Dynamic::from(k.clone()))
                .collect()
        });
    }

    // resolve_language_key(input) -> String (empty if no match).
    // Accepts either the canonical key ("elvish") or the display name in any case
    // ("Elvish", "elvish", "ELVISH"). Returns the canonical key.
    {
        let state_clone = state.clone();
        engine.register_fn("resolve_language_key", move |input: String| -> String {
            let needle = input.to_lowercase();
            let world = state_clone.lock().unwrap();
            if world.language_definitions.contains_key(&needle) {
                return needle;
            }
            for (k, d) in world.language_definitions.iter() {
                if d.display_name.to_lowercase() == needle {
                    return k.clone();
                }
            }
            String::new()
        });
    }

    // garble_language(text, language_key, effective_skill) -> String
    {
        let state_clone = state.clone();
        engine.register_fn(
            "garble_language",
            move |text: String, language_key: String, effective_skill: i64| -> String {
                let world = state_clone.lock().unwrap();
                let lang = match world.language_definitions.get(&language_key.to_lowercase()) {
                    Some(d) => d,
                    None => return "<garbled speech>".to_string(),
                };
                garble_text(&text, lang, effective_skill as i32)
            },
        );
    }

    // garble_for_mob_listener(text, language_key, listener_name) -> String
    //
    // Look up the listener by name; admin or lingua-franca passes through.
    // Otherwise the listener's skill level in `language_key` drives the
    // garble. Unknown listener / language gracefully returns `text`.
    {
        let state_clone = state.clone();
        let cloned_db = db.clone();
        engine.register_fn(
            "garble_for_mob_listener",
            move |text: String, language_key: String, listener_name: String| -> String {
                if language_key.is_empty() {
                    return text;
                }
                let ch = match cloned_db.get_character_data(&listener_name.to_lowercase()) {
                    Ok(Some(c)) => c,
                    _ => return text,
                };
                if ch.is_admin {
                    return text;
                }
                let world = state_clone.lock().unwrap();
                let lang_lc = language_key.to_lowercase();
                let lang = match world.language_definitions.get(&lang_lc) {
                    Some(d) => d,
                    None => return text,
                };
                let skill_level = ch.skills.get(&lang_lc).map(|p| p.level).unwrap_or(0);
                garble_text(&text, lang, skill_level)
            },
        );
    }

    // set_mobile_spoken_language(mobile_id, key) -> String
    //   "" on success, error message otherwise. Empty key clears (back to
    //   lingua franca / Common). Unknown key is rejected.
    {
        let state_clone = state.clone();
        let cloned_db = db.clone();
        engine.register_fn(
            "set_mobile_spoken_language",
            move |mobile_id: String, key: String| -> String {
                let mob_uuid = match Uuid::parse_str(&mobile_id) {
                    Ok(u) => u,
                    Err(_) => return "invalid mobile id".to_string(),
                };
                let mut mob = match cloned_db.get_mobile_data(&mob_uuid) {
                    Ok(Some(m)) => m,
                    _ => return "mobile not found".to_string(),
                };
                if key.is_empty() {
                    mob.spoken_language = None;
                } else {
                    let lc = key.to_lowercase();
                    let world = state_clone.lock().unwrap();
                    if !world.language_definitions.contains_key(&lc) {
                        return format!("unknown language `{}`", key);
                    }
                    drop(world);
                    mob.spoken_language = Some(lc);
                }
                if let Err(e) = cloned_db.save_mobile_data(mob) {
                    return format!("save error: {}", e);
                }
                String::new()
            },
        );
    }

    // get_mobile_spoken_language(mobile_id) -> String (empty if None)
    {
        let cloned_db = db.clone();
        engine.register_fn("get_mobile_spoken_language", move |mobile_id: String| -> String {
            let mob_uuid = match Uuid::parse_str(&mobile_id) {
                Ok(u) => u,
                Err(_) => return String::new(),
            };
            cloned_db
                .get_mobile_data(&mob_uuid)
                .ok()
                .flatten()
                .and_then(|m| m.spoken_language)
                .unwrap_or_default()
        });
    }

    // get_languages_known(char_name) -> Array<Map>
    //
    // Filters the character's `skills` map by language registry membership.
    // Each map: { key, display_name, level, experience, is_lingua_franca,
    // is_current }.
    {
        let state_clone = state.clone();
        let cloned_db = db.clone();
        engine.register_fn("get_languages_known", move |char_name: String| -> rhai::Array {
            let ch = match cloned_db.get_character_data(&char_name.to_lowercase()) {
                Ok(Some(c)) => c,
                _ => return rhai::Array::new(),
            };
            let world = state_clone.lock().unwrap();
            let mut entries: Vec<rhai::Dynamic> = Vec::new();
            for (key, def) in world.language_definitions.iter() {
                let progress = ch.skills.get(key);
                let level = progress.map(|p| p.level as i64).unwrap_or(0);
                let xp = progress.map(|p| p.experience as i64).unwrap_or(0);
                if level <= 0 && !def.is_lingua_franca {
                    continue;
                }
                let mut m = rhai::Map::new();
                m.insert("key".into(), rhai::Dynamic::from(def.key.clone()));
                m.insert("display_name".into(), rhai::Dynamic::from(def.display_name.clone()));
                m.insert("level".into(), rhai::Dynamic::from(level));
                m.insert("experience".into(), rhai::Dynamic::from(xp));
                m.insert("is_lingua_franca".into(), rhai::Dynamic::from(def.is_lingua_franca));
                m.insert("is_current".into(), rhai::Dynamic::from(ch.current_language == def.key));
                entries.push(rhai::Dynamic::from(m));
            }
            entries
        });
    }
}

fn garble_one_word(word: &str, pool: &[String], rng: &mut impl Rng) -> String {
    // Strip leading/trailing ASCII punctuation; remember it.
    let chars: Vec<char> = word.chars().collect();
    let leading_punct: String = chars.iter().take_while(|c| !c.is_alphanumeric()).collect();
    let trailing_punct: String = chars
        .iter()
        .rev()
        .take_while(|c| !c.is_alphanumeric())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let core_start = leading_punct.chars().count();
    let core_end = chars.len().saturating_sub(trailing_punct.chars().count());
    let core: String = if core_start < core_end {
        chars[core_start..core_end].iter().collect()
    } else {
        word.to_string()
    };
    if core.is_empty() {
        return word.to_string();
    }
    let was_capitalized = core.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
    let idx = rng.gen_range(0..pool.len());
    let mut replacement = pool[idx].clone();
    if was_capitalized {
        let mut chars = replacement.chars();
        if let Some(first) = chars.next() {
            replacement = first.to_uppercase().collect::<String>() + chars.as_str();
        }
    }
    format!("{}{}{}", leading_punct, replacement, trailing_punct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn garble_one_word_preserves_punctuation() {
        let pool = vec!["aelin".to_string()];
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        assert_eq!(garble_one_word("Hello,", &pool, &mut rng), "Aelin,");
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        assert_eq!(garble_one_word("hello", &pool, &mut rng), "aelin");
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        assert_eq!(garble_one_word("(world)", &pool, &mut rng), "(aelin)");
    }
}
