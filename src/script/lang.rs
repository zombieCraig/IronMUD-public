//! Language Rhai bindings: garble, registry lookup, language listing for
//! the `speak` and `languages` commands.
//!
//! Languages share storage with skills (see `CharacterData.skills`). The
//! `LanguageRegistry` (held on `World.language_definitions`) decides which
//! skill keys to surface as languages and supplies phonetic word pools used
//! to garble speech for listeners with insufficient skill.

use std::sync::Arc;

use rand::Rng;
use rhai::Engine;

use crate::SharedState;
use crate::db::Db;

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
    //
    // - Lingua francas pass through unchanged regardless of skill.
    // - Each word in `text` survives with probability effective_skill/10;
    //   otherwise it's replaced with a random word from the language's
    //   phonetic pool. Punctuation is preserved; capitalization on the
    //   original word's leading letter is mirrored on the replacement.
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
                if lang.is_lingua_franca {
                    return text;
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
            },
        );
    }

    // get_languages_known(char_name) -> Array<Map>
    //
    // Filters the character's `skills` map by language registry membership.
    // Each map: { key, display_name, level, experience, is_lingua_franca,
    // is_current }.
    {
        let state_clone = state.clone();
        let cloned_db = db.clone();
        engine.register_fn(
            "get_languages_known",
            move |char_name: String| -> rhai::Array {
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
                    m.insert(
                        "display_name".into(),
                        rhai::Dynamic::from(def.display_name.clone()),
                    );
                    m.insert("level".into(), rhai::Dynamic::from(level));
                    m.insert("experience".into(), rhai::Dynamic::from(xp));
                    m.insert(
                        "is_lingua_franca".into(),
                        rhai::Dynamic::from(def.is_lingua_franca),
                    );
                    m.insert(
                        "is_current".into(),
                        rhai::Dynamic::from(ch.current_language == def.key),
                    );
                    entries.push(rhai::Dynamic::from(m));
                }
                entries
            },
        );
    }
}

fn garble_one_word(word: &str, pool: &[String], rng: &mut impl Rng) -> String {
    // Strip leading/trailing ASCII punctuation; remember it.
    let chars: Vec<char> = word.chars().collect();
    let leading_punct: String = chars
        .iter()
        .take_while(|c| !c.is_alphanumeric())
        .collect();
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
