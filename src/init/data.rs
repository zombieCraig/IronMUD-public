//! Game data loading functions

use anyhow::Result;
use std::collections::HashMap;
use tracing::{error, info};

use crate::{
    AchievementCriterion, AchievementDef, AchievementSource, ClassDefinition, CommandMeta, LanguageDefinition,
    RaceDefinition, RaceSuggestion, SharedState, SpellDefinition,
};

/// Load command metadata from scripts/commands.json
pub fn load_command_metadata() -> Result<HashMap<String, CommandMeta>> {
    let content = std::fs::read_to_string("scripts/commands.json")?;
    let metadata: HashMap<String, CommandMeta> = serde_json::from_str(&content)?;
    Ok(metadata)
}

/// Resolve the class definitions file path based on the `class_preset` setting.
/// Defaults to "fantasy" for new installs. Falls back to legacy `classes.json`.
fn resolve_classes_path(preset: Option<String>) -> String {
    let preset = preset.unwrap_or_else(|| "fantasy".to_string());
    let path = format!("scripts/data/classes_{}.json", preset);
    if std::path::Path::new(&path).exists() {
        return path;
    }
    // Fall back to legacy file
    "scripts/data/classes.json".to_string()
}

/// Resolve the race suggestions file path based on the `race_preset` setting.
/// Defaults to "fantasy" for new installs. Falls back to legacy `race_suggestions.json`.
fn resolve_races_path(preset: Option<String>) -> String {
    let preset = preset.unwrap_or_else(|| "fantasy".to_string());
    let path = format!("scripts/data/race_suggestions_{}.json", preset);
    if std::path::Path::new(&path).exists() {
        return path;
    }
    // Fall back to legacy file
    "scripts/data/race_suggestions.json".to_string()
}

/// Resolve the race definitions file path based on the `race_preset` setting.
fn resolve_race_definitions_path(preset: Option<String>) -> String {
    let preset = preset.unwrap_or_else(|| "fantasy".to_string());
    format!("scripts/data/races_{}.json", preset)
}

/// Resolve the language definitions file path. Tracks the same preset as
/// classes/races by default; admins can override via `language_preset` setting.
fn resolve_languages_path(preset: Option<String>) -> String {
    let preset = preset.unwrap_or_else(|| "fantasy".to_string());
    format!("scripts/data/languages_{}.json", preset)
}

/// Load game data (classes, traits, race suggestions) from scripts/data/*.json
pub fn load_game_data(state: SharedState) -> Result<()> {
    let mut world = state.lock().unwrap();

    // Determine presets from settings
    let class_preset = world.db.get_setting("class_preset").unwrap_or(None);
    let race_preset = world.db.get_setting("race_preset").unwrap_or(None);

    let classes_path = resolve_classes_path(class_preset);
    let races_path = resolve_races_path(race_preset.clone());

    // Load class definitions
    match std::fs::read_to_string(&classes_path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(classes) => {
                world.class_definitions = classes;
                info!(
                    "Loaded {} class definitions from {}",
                    world.class_definitions.len(),
                    classes_path
                );
            }
            Err(e) => {
                error!("Failed to parse {}: {}", classes_path, e);
            }
        },
        Err(_) => {
            info!("No class definitions file found, using default class");
            world.class_definitions.insert(
                "unemployed".to_string(),
                ClassDefinition {
                    id: "unemployed".to_string(),
                    name: "Peasant".to_string(),
                    description: "No particular profession.".to_string(),
                    starting_skills: HashMap::new(),
                    stat_bonuses: HashMap::new(),
                    available: true,
                    starting_languages: HashMap::new(),
                },
            );
        }
    }

    // Theme-agnostic vampire overlay: any theme picks it up when
    // `enable_vampire_creation` is toggled on at runtime. The runtime gate in
    // get_class_list still hides the entry until the setting flips.
    let vampire_path = "scripts/data/classes_vampire.json";
    if let Ok(content) = std::fs::read_to_string(vampire_path) {
        match serde_json::from_str::<HashMap<String, ClassDefinition>>(&content) {
            Ok(extras) => {
                let count = extras.len();
                world.class_definitions.extend(extras);
                info!("Loaded {} vampire class definition(s) from {}", count, vampire_path);
            }
            Err(e) => error!("Failed to parse {}: {}", vampire_path, e),
        }
    }

    // Load trait definitions
    match std::fs::read_to_string("scripts/data/traits.json") {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(traits) => {
                world.trait_definitions = traits;
                info!("Loaded {} trait definitions", world.trait_definitions.len());
            }
            Err(e) => {
                error!("Failed to parse traits.json: {}", e);
            }
        },
        Err(_) => {
            info!("No traits.json found, starting with no traits");
        }
    }

    // Load race suggestions
    match std::fs::read_to_string(&races_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(data) => {
                if let Some(races) = data.get("races").and_then(|r| r.as_array()) {
                    world.race_suggestions = races
                        .iter()
                        .filter_map(|r| serde_json::from_value(r.clone()).ok())
                        .collect();
                    info!(
                        "Loaded {} race suggestions from {}",
                        world.race_suggestions.len(),
                        races_path
                    );
                }
            }
            Err(e) => {
                error!("Failed to parse {}: {}", races_path, e);
            }
        },
        Err(_) => {
            info!("No race suggestions file found, using defaults");
            world.race_suggestions = vec![RaceSuggestion {
                name: "Human".to_string(),
                description: "Versatile and adaptable.".to_string(),
            }];
        }
    }

    // Load race definitions (mechanical race system)
    let race_defs_path = resolve_race_definitions_path(race_preset);
    match std::fs::read_to_string(&race_defs_path) {
        Ok(content) => match serde_json::from_str::<HashMap<String, RaceDefinition>>(&content) {
            Ok(defs) => {
                info!("Loaded {} race definitions from {}", defs.len(), race_defs_path);
                world.race_definitions = defs;
            }
            Err(e) => {
                error!("Failed to parse {}: {}", race_defs_path, e);
            }
        },
        Err(_) => {
            info!(
                "No race definitions file found at {}, using default human race",
                race_defs_path
            );
            world.race_definitions.insert(
                "human".to_string(),
                RaceDefinition {
                    id: "human".to_string(),
                    name: "Human".to_string(),
                    description: "Versatile and adaptable, humans thrive in any environment.".to_string(),
                    stat_modifiers: HashMap::new(),
                    granted_traits: Vec::new(),
                    resistances: HashMap::new(),
                    passive_abilities: Vec::new(),
                    active_abilities: Vec::new(),
                    available: true,
                    starting_languages: HashMap::new(),
                },
            );
        }
    }

    // Load spell definitions
    let spell_preset = world
        .db
        .get_setting("spell_preset")
        .unwrap_or(None)
        .unwrap_or_else(|| "fantasy".to_string());
    let spells_path = format!("scripts/data/spells_{}.json", spell_preset);
    match std::fs::read_to_string(&spells_path) {
        Ok(content) => match serde_json::from_str::<HashMap<String, SpellDefinition>>(&content) {
            Ok(spells) => {
                info!("Loaded {} spell definitions from {}", spells.len(), spells_path);
                world.spell_definitions = spells;
            }
            Err(e) => {
                error!("Failed to parse {}: {}", spells_path, e);
            }
        },
        Err(_) => {
            info!("No spell definitions file found at {}", spells_path);
        }
    }

    // Vampire disciplines layer on top of whichever preset is active —
    // optional file. Disciplines gate independently via `requires_vampire`
    // / `requires_clan` so they can't be cast by non-vampires.
    let vampire_spells_path = "scripts/data/spells_vampire.json";
    match std::fs::read_to_string(vampire_spells_path) {
        Ok(content) => match serde_json::from_str::<HashMap<String, SpellDefinition>>(&content) {
            Ok(spells) => {
                info!(
                    "Loaded {} vampire discipline spells from {}",
                    spells.len(),
                    vampire_spells_path
                );
                for (id, spell) in spells {
                    world.spell_definitions.insert(id, spell);
                }
            }
            Err(e) => {
                error!("Failed to parse {}: {}", vampire_spells_path, e);
            }
        },
        Err(_) => {
            // Optional — quiet when missing.
        }
    }

    // Load language definitions. Falls back to the class_preset (or "fantasy")
    // if `language_preset` is unset, so a fantasy world gets fantasy languages
    // without extra config.
    let language_preset = world
        .db
        .get_setting("language_preset")
        .unwrap_or(None)
        .or_else(|| world.db.get_setting("class_preset").unwrap_or(None));
    let languages_path = resolve_languages_path(language_preset);
    match std::fs::read_to_string(&languages_path) {
        Ok(content) => match serde_json::from_str::<HashMap<String, LanguageDefinition>>(&content) {
            Ok(langs) => {
                info!(
                    "Loaded {} language definitions from {}",
                    langs.len(),
                    languages_path
                );
                world.language_definitions = langs;
            }
            Err(e) => {
                error!("Failed to parse {}: {}", languages_path, e);
            }
        },
        Err(_) => {
            info!(
                "No language definitions file at {}, seeding Common only",
                languages_path
            );
            world.language_definitions.insert(
                "common".to_string(),
                LanguageDefinition {
                    key: "common".to_string(),
                    display_name: "Common".to_string(),
                    description: "The lingua franca; understood by everyone.".to_string(),
                    is_lingua_franca: true,
                    phonetic_words: Vec::new(),
                },
            );
        }
    }

    // Load recipes from database (created via recedit command)
    match world.db.list_all_recipes() {
        Ok(recipes) => {
            for recipe in recipes {
                world.recipes.insert(recipe.id.clone(), recipe);
            }
            info!("Loaded {} recipes from database", world.recipes.len());
        }
        Err(e) => {
            error!("Failed to load recipes from database: {}", e);
        }
    }

    // Load achievement definitions: JSON first, then sled tree (DB wins on
    // collision, with a warning). Builds the counter-key index used by
    // `notify_achievement_counter`.
    load_achievements(&mut world);

    Ok(())
}

/// Load achievement definitions into the world. JSON files in
/// `scripts/data/achievements/` populate the canonical engine-detected set;
/// the sled `achievements` tree contains builder-authored entries (typically
/// `criterion: Manual`). On key collision the DB entry wins with a warning.
fn load_achievements(world: &mut crate::World) {
    use std::path::PathBuf;

    let mut defs: HashMap<String, AchievementDef> = HashMap::new();
    let dir = PathBuf::from("scripts/data/achievements");
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let file_label = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                let content = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to read achievements file {}: {}", file_label, e);
                        continue;
                    }
                };
                let parsed: Result<Vec<AchievementDef>, _> = serde_json::from_str(&content);
                match parsed {
                    Ok(list) => {
                        for mut def in list {
                            def.source = AchievementSource::Json {
                                file: file_label.clone(),
                            };
                            let key = def.key.to_lowercase();
                            if defs.contains_key(&key) {
                                tracing::warn!("Duplicate achievement key '{}' in {}", key, file_label);
                            }
                            defs.insert(key, def);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse achievements file {}: {}", file_label, e);
                    }
                }
            }
        }
    } else {
        info!("No achievements directory at {}", dir.display());
    }

    // Sled-stored builder achievements override JSON on key collision.
    match world.db.list_all_achievements() {
        Ok(db_defs) => {
            for def in db_defs {
                let key = def.key.to_lowercase();
                if defs.contains_key(&key) {
                    tracing::warn!(
                        "Achievement key '{}' from database overrides JSON definition",
                        key
                    );
                }
                defs.insert(key, def);
            }
        }
        Err(e) => {
            error!("Failed to load achievements from database: {}", e);
        }
    }

    // Build the counter index.
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for (key, def) in &defs {
        if let AchievementCriterion::Counter { counter, .. } = &def.criterion {
            index.entry(counter.clone()).or_default().push(key.clone());
        }
    }

    info!(
        "Loaded {} achievement definitions ({} counter-indexed)",
        defs.len(),
        index.values().map(|v| v.len()).sum::<usize>()
    );
    world.achievement_definitions = defs;
    world.achievement_index_by_counter = index;
}
