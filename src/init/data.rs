//! Game data loading functions

use anyhow::Result;
use std::collections::HashMap;
use tracing::{error, info};

use crate::{ClassDefinition, CommandMeta, RaceDefinition, RaceSuggestion, SharedState, SpellDefinition};

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
                },
            );
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

    Ok(())
}
