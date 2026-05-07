//! Language slice 1 tests: data-file parsing, serde defaults, race/class
//! starting_languages, and garble helper edge cases.
//!
//! Full Rhai-driven integration tests (say/tell/whisper/shout end-to-end)
//! aren't covered here — those exercise a registered engine + World which
//! belongs in a future test harness. Slice 1 verifies the data plumbing.

#![recursion_limit = "256"]

use ironmud::types::{CharacterData, ClassDefinition, LanguageDefinition, RaceDefinition};
use std::collections::HashMap;
use uuid::Uuid;

fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn fantasy_languages_file_loads() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let common = langs.get("common").expect("common must be present");
    assert!(common.is_lingua_franca, "common must be a lingua franca");
    let elvish = langs.get("elvish").expect("elvish must be present");
    assert!(!elvish.is_lingua_franca, "elvish is not a lingua franca");
    assert!(
        elvish.phonetic_words.len() >= 30,
        "elvish needs a usable phonetic pool, got {}",
        elvish.phonetic_words.len()
    );
    assert!(langs.contains_key("dwarvish"));
    assert!(langs.contains_key("orcish"));
}

#[test]
fn modern_languages_file_loads() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_modern.json");
    let common = langs.get("common").expect("common must be present");
    assert!(common.is_lingua_franca);
    assert!(langs.contains_key("street_slang"));
    assert!(langs.contains_key("high_speak"));
    let proto = langs.get("protocol").expect("protocol must be present");
    assert!(!proto.is_lingua_franca);
    assert!(proto.phonetic_words.len() >= 30);
}

#[test]
fn current_language_defaults_to_common_for_legacy_chars() {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build char");
    assert_eq!(
        ch.current_language, "common",
        "missing current_language must default to 'common'"
    );
}

#[test]
fn race_starting_languages_deserialize() {
    let races: HashMap<String, RaceDefinition> = read_json("scripts/data/races_fantasy.json");
    let elf = races.get("elf").expect("elf race must exist");
    assert_eq!(
        elf.starting_languages.get("elvish").copied(),
        Some(10),
        "elf must start with elvish 10"
    );
    let half_elf = races.get("half-elf").expect("half-elf must exist");
    assert_eq!(half_elf.starting_languages.get("elvish").copied(), Some(5));
    let dwarf = races.get("dwarf").expect("dwarf must exist");
    assert_eq!(dwarf.starting_languages.get("dwarvish").copied(), Some(10));
    let orc = races.get("orc").expect("orc must exist");
    assert_eq!(orc.starting_languages.get("orcish").copied(), Some(10));
}

#[test]
fn modern_race_starting_languages_deserialize() {
    let races: HashMap<String, RaceDefinition> = read_json("scripts/data/races_modern.json");
    let bioroid = races.get("bioroid").expect("bioroid must exist");
    assert_eq!(bioroid.starting_languages.get("protocol").copied(), Some(10));
    let mutant = races.get("mutant").expect("mutant must exist");
    assert_eq!(mutant.starting_languages.get("street_slang").copied(), Some(10));
    let revenant = races.get("revenant").expect("revenant must exist");
    assert_eq!(revenant.starting_languages.get("high_speak").copied(), Some(8));
}

#[test]
fn class_starting_languages_deserialize() {
    let classes: HashMap<String, ClassDefinition> = read_json("scripts/data/classes_modern.json");
    let pi = classes
        .get("private_investigator")
        .expect("private_investigator must exist");
    assert_eq!(pi.starting_languages.get("street_slang").copied(), Some(4));
    let soldier = classes.get("soldier").expect("soldier must exist");
    assert_eq!(soldier.starting_languages.get("protocol").copied(), Some(2));
}

#[test]
fn linguist_and_tongue_tied_traits_present() {
    use ironmud::types::TraitDefinition;
    let traits: HashMap<String, TraitDefinition> = read_json("scripts/data/traits.json");
    let linguist = traits.get("linguist").expect("linguist trait must exist");
    let tongue_tied = traits
        .get("tongue_tied")
        .expect("tongue_tied trait must exist");
    assert!(
        linguist.conflicts_with.iter().any(|c| c == "tongue_tied"),
        "linguist must conflict with tongue_tied"
    );
    assert!(
        tongue_tied.conflicts_with.iter().any(|c| c == "linguist"),
        "tongue_tied must conflict with linguist"
    );
    assert_eq!(
        linguist.effects.get("language_xp_bonus").copied(),
        Some(50),
        "linguist effect should be +50%"
    );
    assert_eq!(
        tongue_tied.effects.get("language_xp_bonus").copied(),
        Some(-35),
        "tongue_tied effect should be -35%"
    );
}

#[test]
fn lingua_franca_marker_isolated_to_common() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let lingua_count = langs.values().filter(|d| d.is_lingua_franca).count();
    assert_eq!(
        lingua_count, 1,
        "exactly one fantasy language should be a lingua franca; got {}",
        lingua_count
    );
}
