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
    ItemData, ItemLocation,
};
use ironmud::{SharedConnections, SharedState, World, db::Db, script};
use rhai::Engine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tempfile;

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
            morality_delta: 0,
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
            morality_delta: 0,
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
            morality_delta: 0,
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
        socials: ironmud::social::actions::SocialRegistry::default(),
        class_definitions: HashMap::new(),
        trait_definitions: HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: HashMap::new(),
        language_definitions: HashMap::new(),
        recipes: HashMap::new(),
        spell_definitions: HashMap::new(),
        achievement_definitions,
        achievement_index_by_counter,
        custom_skill_definitions: HashMap::new(),
        transports: HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
        ip_limiter: Arc::new(ironmud::ratelimit::IpRateLimiter::new()),
        command_throttle: Arc::new(ironmud::throttle::CommandThrottle::new()),
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
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
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

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_skill_event_unlocks_threshold() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let ch = make_character("scholar");
        db.save_character_data(ch).expect("save");

        let (state, connections) = build_state(
            db.clone(),
            vec![make_def_skill("skilled_cook", "Skilled Cook", "cooking", 5, "the Cook")],
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

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_admin_toggle_disables_notify() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
        let ch = make_character("npc");
        db.save_character_data(ch).expect("save");

        // Flip the world setting OFF.
        db.set_setting("achievements_enabled", "false").expect("set setting");

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

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_manual_award_only_for_manual_criterion() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");
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

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Builder-path setup: empty achievement map + engine wired up the same way
/// the live server does, so we can call `create_achievement` / setters /
/// `delete_achievement` and inspect what landed in the world map.
fn setup_builder_engine() -> (Engine, SharedState, SharedConnections, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let db = Db::open(temp.path()).expect("open db");
    let (state, connections) = build_state(db.clone(), vec![]);
    let mut engine = Engine::new();
    script::achievements::register(&mut engine, Arc::new(db), connections.clone(), state.clone());
    (engine, state, connections, temp)
}

#[test]
fn create_achievement_syncs_world_map_and_defaults_hidden_true() {
    let (engine, state, _conns, _temp) = setup_builder_engine();
    let err: String = engine
        .eval(r#"create_achievement("hero_a", "Hero A", "alice")"#)
        .expect("eval create");
    assert_eq!(err, "", "create_achievement returned error: {}", err);

    let world = state.lock().unwrap();
    let def = world
        .achievement_definitions
        .get("hero_a")
        .expect("world should have the new achievement after create");
    assert_eq!(def.key, "hero_a");
    assert_eq!(def.name, "Hero A");
    assert!(def.hidden, "new achievements must default hidden=true");
    assert!(matches!(def.criterion, AchievementCriterion::Manual));
    assert!(matches!(def.source, AchievementSource::Db { .. }));
}

#[test]
fn get_achievement_def_finds_builder_authored_entry() {
    let (engine, _state, _conns, _temp) = setup_builder_engine();
    engine
        .eval::<String>(r#"create_achievement("looker", "The Looker", "bob")"#)
        .expect("create");
    // Pre-fix this returned () because world.achievement_definitions was stale.
    let looked_up: rhai::Dynamic = engine.eval(r#"get_achievement_def("looker")"#).expect("get_def");
    assert!(
        !looked_up.is_unit(),
        "get_achievement_def returned () for a freshly created entry"
    );
}

#[test]
fn list_achievement_defs_includes_new_achievement() {
    let (engine, _state, _conns, _temp) = setup_builder_engine();
    engine
        .eval::<String>(r#"create_achievement("listed", "Listed One", "alice")"#)
        .expect("create");
    let listed: rhai::Array = engine.eval(r#"list_achievement_defs()"#).expect("list");
    let keys: Vec<String> = listed
        .iter()
        .filter_map(|d| {
            d.clone()
                .try_cast::<rhai::Map>()
                .and_then(|m| m.get("key").and_then(|k| k.clone().into_string().ok()))
        })
        .collect();
    assert!(
        keys.contains(&"listed".to_string()),
        "list_achievement_defs missed the new entry; got {:?}",
        keys
    );
}

#[test]
fn counter_criterion_updates_world_index() {
    let (engine, state, _conns, _temp) = setup_builder_engine();
    engine
        .eval::<String>(r#"create_achievement("five_kills", "Five Kills", "alice")"#)
        .expect("create");
    let err: String = engine
        .eval(r#"set_achievement_criterion_counter("five_kills", "mobs_killed", 5)"#)
        .expect("set counter");
    assert_eq!(err, "");

    let world = state.lock().unwrap();
    let def = world.achievement_definitions.get("five_kills").unwrap();
    match &def.criterion {
        AchievementCriterion::Counter { counter, threshold } => {
            assert_eq!(counter, "mobs_killed");
            assert_eq!(*threshold, 5);
        }
        other => panic!("expected Counter criterion, got {:?}", other),
    }
    // Counter index must list this achievement under the counter key, or the
    // notify path will never reach it.
    let bucket = world
        .achievement_index_by_counter
        .get("mobs_killed")
        .expect("counter index missing the new bucket");
    assert!(bucket.contains(&"five_kills".to_string()));
}

#[test]
fn delete_achievement_removes_from_world_and_index() {
    let (engine, state, _conns, _temp) = setup_builder_engine();
    engine
        .eval::<String>(r#"create_achievement("removable", "Removable", "alice")"#)
        .expect("create");
    engine
        .eval::<String>(r#"set_achievement_criterion_counter("removable", "ctr", 1)"#)
        .expect("counter");

    let ok: bool = engine.eval(r#"delete_achievement("removable")"#).expect("delete");
    assert!(ok);

    let world = state.lock().unwrap();
    assert!(!world.achievement_definitions.contains_key("removable"));
    // Counter index must drop the now-orphaned bucket entry.
    let bucket = world.achievement_index_by_counter.get("ctr");
    assert!(
        bucket.map(|v| !v.iter().any(|k| k == "removable")).unwrap_or(true),
        "deleted achievement still listed in counter index"
    );
}

#[test]
fn set_hidden_persists_through_world_map() {
    let (engine, state, _conns, _temp) = setup_builder_engine();
    engine
        .eval::<String>(r#"create_achievement("toggle_me", "Toggle", "alice")"#)
        .expect("create");
    // create defaults hidden=true; flip it off and re-read via the world map.
    let err: String = engine
        .eval(r#"set_achievement_hidden("toggle_me", false)"#)
        .expect("set hidden");
    assert_eq!(err, "");
    let world = state.lock().unwrap();
    assert!(!world.achievement_definitions.get("toggle_me").unwrap().hidden);
}

#[test]
fn test_item_reward_delivered_to_inventory() {
    let temp = tempfile::tempdir().expect("create temp dir");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let db = Db::open(temp.path()).expect("open DB");

        // Prototype the reward item.
        let mut proto = ItemData::new(
            "test glove".to_string(),
            "a test glove".to_string(),
            "A test glove lies here.".to_string(),
        );
        proto.vnum = Some("glove_test".to_string());
        proto.is_prototype = true;
        db.save_item_data(proto).expect("save prototype");

        let ch = make_character("hero");
        db.save_character_data(ch).expect("save char");

        // Manual achievement whose reward grants the glove.
        let mut def = make_def_manual("glove_award", "Glove Award", "the Gloved");
        def.reward.item_vnum = Some("glove_test".to_string());
        let (state, connections) = build_state(db.clone(), vec![def]);

        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "glove_award", true);
        assert!(ok, "manual award should unlock");

        // The reward item is now a live instance in hero's inventory.
        let instances = db.get_item_instances_by_vnum("glove_test").expect("instances");
        assert_eq!(instances.len(), 1, "exactly one glove delivered");
        match &instances[0].location {
            ItemLocation::Inventory(owner) => assert_eq!(owner, "hero"),
            other => panic!("glove not in inventory: {:?}", other),
        }

        // Idempotent: re-awarding an already-unlocked achievement delivers nothing more.
        let ok = script::achievements::award_core(&db, &connections, &state, "hero", "glove_award", true);
        assert!(!ok, "second award is a no-op");
        let instances = db.get_item_instances_by_vnum("glove_test").expect("instances");
        assert_eq!(instances.len(), 1, "no duplicate delivery on idempotent re-award");
    }));

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
