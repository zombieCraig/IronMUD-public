//! Integration tests for the Slice 1 achievement system.
//!
//! Exercises core unlock pipeline behavior without spinning up a full
//! server: we construct a minimal `World`, hand-populate
//! `achievement_definitions`, save a character through the live `Db`, and
//! call the public `notify_counter_core` / `notify_event_core` / `award_core`
//! fns the same way engine hook sites do.

#![recursion_limit = "256"]

use ironmud::types::{
    AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource, CharacterData,
};
use ironmud::{SharedConnections, SharedState, World, db::Db, script};
use rhai::Engine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn make_def_counter(key: &str, name: &str, counter: &str, threshold: u32, title: &str) -> AchievementDef {
    AchievementDef {
        key: key.to_string(),
        name: name.to_string(),
        description: format!("Test achievement {}", key),
        category: AchievementCategory::Combat,
        criterion: AchievementCriterion::Counter {
            counter: counter.to_string(),
            threshold,
        },
        reward: AchievementReward {
            title: title.to_string(),
            item_vnum: None,
            gold: None,
        },
        hidden: false,
        source: AchievementSource::Json {
            file: "test.json".to_string(),
        },
    }
}

fn make_def_skill(key: &str, name: &str, skill: &str, level: i32, title: &str) -> AchievementDef {
    AchievementDef {
        key: key.to_string(),
        name: name.to_string(),
        description: format!("Test skill {}", key),
        category: AchievementCategory::Skill,
        criterion: AchievementCriterion::SkillReached {
            skill: skill.to_string(),
            level,
        },
        reward: AchievementReward {
            title: title.to_string(),
            item_vnum: None,
            gold: None,
        },
        hidden: false,
        source: AchievementSource::Json {
            file: "test.json".to_string(),
        },
    }
}

fn make_def_manual(key: &str, name: &str, title: &str) -> AchievementDef {
    AchievementDef {
        key: key.to_string(),
        name: name.to_string(),
        description: format!("Manual {}", key),
        category: AchievementCategory::Builder,
        criterion: AchievementCriterion::Manual,
        reward: AchievementReward {
            title: title.to_string(),
            item_vnum: None,
            gold: None,
        },
        hidden: false,
        source: AchievementSource::Db {
            author: "tester".to_string(),
        },
    }
}

fn build_state(db: Db, defs: Vec<AchievementDef>) -> (SharedState, SharedConnections) {
    let mut achievement_definitions: HashMap<String, AchievementDef> = HashMap::new();
    let mut achievement_index_by_counter: HashMap<String, Vec<String>> = HashMap::new();
    for def in defs {
        let key = def.key.to_lowercase();
        if let AchievementCriterion::Counter { counter, .. } = &def.criterion {
            achievement_index_by_counter
                .entry(counter.clone())
                .or_default()
                .push(key.clone());
        }
        achievement_definitions.insert(key, def);
    }

    let connections: SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let world = World {
        engine: Engine::new(),
        db,
        connections: connections.clone(),
        scripts: HashMap::new(),
        command_metadata: HashMap::new(),
        class_definitions: HashMap::new(),
        trait_definitions: HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: HashMap::new(),
        recipes: HashMap::new(),
        spell_definitions: HashMap::new(),
        achievement_definitions,
        achievement_index_by_counter,
        transports: HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
    };
    (Arc::new(Mutex::new(world)), connections)
}

fn make_character(name: &str) -> CharacterData {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": uuid::Uuid::nil(),
    }))
    .expect("build character")
}

#[test]
fn test_counter_threshold_unlocks_achievement() {
    let db_path = format!("test_ach_counter_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");
        let mut ch = make_character("hero");
        db.save_character_data(ch.clone()).expect("save");

        // World holds its own Db; the test references the same path so
        // db handles point at the same store.
        let (state, connections) = build_state(
            db.clone(),
            vec![make_def_counter(
                "first_blood",
                "First Blood",
                "kills.any",
                1,
                "the Bloodied",
            )],
        );

        // First kill -> unlock.
        let new_v = script::achievements::notify_counter_core(&db, &connections, &state, "hero", "kills.any", 1);
        assert_eq!(new_v, 1);

        ch = db.get_character_data("hero").expect("load").expect("present");
        assert!(
            ch.achievements_unlocked.contains_key("first_blood"),
            "first_blood unlocked"
        );
        assert_eq!(ch.active_title.as_deref(), Some("first_blood"));

        // Bumping again is a no-op for unlocks but counter still increments.
        let new_v = script::achievements::notify_counter_core(&db, &connections, &state, "hero", "kills.any", 1);
        assert_eq!(new_v, 2);
        ch = db.get_character_data("hero").expect("load").expect("present");
        assert_eq!(ch.achievements_unlocked.len(), 1, "still only one unlock");
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_skill_event_unlocks_threshold() {
    let db_path = format!("test_ach_skill_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");
        let ch = make_character("scholar");
        db.save_character_data(ch).expect("save");

        let (state, connections) = build_state(
            db.clone(),
            vec![make_def_skill(
                "skilled_cook",
                "Skilled Cook",
                "cooking",
                5,
                "the Cook",
            )],
        );

        // Below threshold: no unlock.
        script::achievements::notify_event_core(&db, &connections, &state, "scholar", "skill_reached", "cooking:3");
        let ch = db.get_character_data("scholar").expect("load").expect("present");
        assert!(ch.achievements_unlocked.is_empty(), "no unlock below threshold");

        // At/above threshold: unlock fires.
        script::achievements::notify_event_core(&db, &connections, &state, "scholar", "skill_reached", "cooking:5");
        let ch = db.get_character_data("scholar").expect("load").expect("present");
        assert!(ch.achievements_unlocked.contains_key("skilled_cook"));

        // Different skill at same level: no extra unlock.
        script::achievements::notify_event_core(&db, &connections, &state, "scholar", "skill_reached", "swords:5");
        let ch = db.get_character_data("scholar").expect("load").expect("present");
        assert_eq!(ch.achievements_unlocked.len(), 1);
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_admin_toggle_disables_notify() {
    let db_path = format!("test_ach_disabled_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");
        let ch = make_character("npc");
        db.save_character_data(ch).expect("save");

        // Flip the world setting OFF.
        db.set_setting("achievements_enabled", "false").expect("set setting");

        let (state, connections) = build_state(
            db.clone(),
            vec![make_def_counter("first_blood", "First Blood", "kills.any", 1, "the Bloodied")],
        );

        let new_v = script::achievements::notify_counter_core(&db, &connections, &state, "npc", "kills.any", 1);
        assert_eq!(new_v, 0, "notify is a no-op when disabled");

        let ch = db.get_character_data("npc").expect("load").expect("present");
        assert!(ch.achievements_unlocked.is_empty(), "no unlocks when disabled");
        assert_eq!(
            *ch.achievement_counters.get("kills.any").unwrap_or(&0),
            0,
            "counter not bumped when disabled"
        );
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_manual_award_only_for_manual_criterion() {
    let db_path = format!("test_ach_manual_{}.db", std::process::id());
    let _ = std::fs::remove_dir_all(&db_path);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(&db_path).expect("open DB");
        let ch = make_character("hero");
        db.save_character_data(ch).expect("save");

        let (state, connections) = build_state(
            db.clone(),
            vec![
                make_def_manual("arena_champion", "Arena Champion", "Champion of the Arena"),
                make_def_counter("first_blood", "First Blood", "kills.any", 1, "the Bloodied"),
            ],
        );

        // Manual award against a Manual criterion: succeeds.
        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "arena_champion", true);
        assert!(ok, "manual award against Manual criterion succeeds");
        let ch = db.get_character_data("hero").expect("load").expect("present");
        assert!(ch.achievements_unlocked.contains_key("arena_champion"));

        // Manual award against engine criterion: refused.
        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "first_blood", true);
        assert!(!ok, "manual award refused for engine-criterion key");
        let ch = db.get_character_data("hero").expect("load").expect("present");
        assert!(!ch.achievements_unlocked.contains_key("first_blood"));

        // Engine award against Manual criterion: refused (so notify paths
        // can't auto-fire builder achievements).
        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "arena_champion", false);
        assert!(!ok || ch.achievements_unlocked.contains_key("arena_champion"));

        // Idempotent: second manual award returns false.
        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "arena_champion", true);
        assert!(!ok, "second manual award is idempotent");
    }));

    let _ = std::fs::remove_dir_all(&db_path);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_seed_json_files_parse() {
    use std::fs;

    for filename in ["combat.json", "skill.json"] {
        let path = format!("scripts/data/achievements/{}", filename);
        let content = fs::read_to_string(&path).expect(&format!("read {}", path));
        let parsed: Vec<AchievementDef> =
            serde_json::from_str(&content).expect(&format!("parse {}: must deserialize", filename));
        assert!(!parsed.is_empty(), "{} should not be empty", filename);
        for def in &parsed {
            assert!(!def.key.is_empty(), "achievement key must not be empty");
            assert!(!def.name.is_empty(), "achievement name must not be empty");
            assert!(
                !def.reward.title.is_empty(),
                "achievement {} must have a title reward (slice 1 contract)",
                def.key
            );
        }
    }
}
