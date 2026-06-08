//! Language slice 1 tests: data-file parsing, serde defaults, race/class
//! starting_languages, and garble helper edge cases.
//!
//! Full Rhai-driven integration tests (say/tell/whisper/shout end-to-end)
//! aren't covered here — those exercise a registered engine + World which
//! belongs in a future test harness. Slice 1 verifies the data plumbing.

#![recursion_limit = "256"]

use ironmud::script::lang::{garble_for_listener, garble_text};
use ironmud::types::{CharacterData, ClassDefinition, LanguageDefinition, MobileData, RaceDefinition};
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
    let tongue_tied = traits.get("tongue_tied").expect("tongue_tied trait must exist");
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

// ---------------------------------------------------------------------------
// Slice 2 tests: NPCs as language speakers
// ---------------------------------------------------------------------------

#[test]
fn mobile_spoken_language_round_trips_through_json() {
    // Default mob has no spoken language; missing field deserializes to None.
    let mut m = MobileData::new("villager".into());
    m.vnum = "9100".into();
    assert!(m.spoken_language.is_none());
    let json = serde_json::to_string(&m).expect("serialize default");
    assert!(!json.contains("spoken_language"), "absent field should not serialize");
    let back: MobileData = serde_json::from_str(&json).expect("deserialize default");
    assert!(back.spoken_language.is_none());

    // Explicit set round-trips.
    m.spoken_language = Some("orcish".to_string());
    let json = serde_json::to_string(&m).expect("serialize set");
    assert!(json.contains("\"spoken_language\":\"orcish\""));
    let back: MobileData = serde_json::from_str(&json).expect("deserialize set");
    assert_eq!(back.spoken_language.as_deref(), Some("orcish"));
}

#[test]
fn garble_text_passes_through_for_lingua_franca() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let common = langs.get("common").expect("common loaded");
    // Even with skill 0, a lingua franca should pass through unchanged.
    let original = "The road north winds through the hills.";
    assert_eq!(garble_text(original, common, 0), original);
    assert_eq!(garble_text(original, common, 5), original);
}

#[test]
fn garble_text_garbles_for_low_skill_listener() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let orcish = langs.get("orcish").expect("orcish loaded");
    let original = "The chief demands tribute from the southern villages.";
    // Run with skill 0 multiple times — at least one run must produce a
    // string that differs from the original (otherwise the garble is broken).
    // Tolerates RNG by retrying; with pass_prob=0 every word is replaced.
    let mut differed = false;
    for _ in 0..5 {
        let garbled = garble_text(original, orcish, 0);
        if garbled != original {
            differed = true;
            break;
        }
    }
    assert!(
        differed,
        "skill-0 listener must hear a garbled version of orcish speech"
    );
}

#[test]
fn garble_for_listener_passes_for_admin() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let original = "Secret battle plans, comrade.";
    // Admin listener at skill 0 with a non-lingua-franca language: still passes.
    let heard = garble_for_listener(original, "orcish", 0, true, &langs);
    assert_eq!(heard, original, "admin must hear plaintext regardless of skill");
}

#[test]
fn garble_for_listener_passes_through_unknown_language() {
    let langs: HashMap<String, LanguageDefinition> = read_json("scripts/data/languages_fantasy.json");
    let original = "Qa'pla'!";
    // Unknown language key: passes unchanged (no garble engine to run).
    let heard = garble_for_listener(original, "klingon", 0, false, &langs);
    assert_eq!(heard, original);
    // Empty key short-circuits too (mob has no language set).
    let heard = garble_for_listener(original, "", 0, false, &langs);
    assert_eq!(heard, original);
}
