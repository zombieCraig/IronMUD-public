//! Quest system slice 1 integration tests.
//!
//! Covers the QuestData / ActiveQuest types, sled persistence, the offer /
//! abandon / try_complete state machine, and the kill / item-turn-in
//! listener entry points exposed by `ironmud::quest`.

#![recursion_limit = "256"]

use ironmud::SharedConnections;
use ironmud::db::Db;
use ironmud::types::{
    ActiveQuest, CharacterData, ItemData, ItemLocation, MobileData, QuestData, QuestObjective,
    QuestReward,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn temp_db_path(label: &str) -> String {
    format!("/tmp/ironmud_quest_test_{}_{}", label, std::process::id())
}

fn fresh_db(label: &str) -> (Db, String) {
    let path = temp_db_path(label);
    let _ = std::fs::remove_dir_all(&path);
    let db = Db::open(&path).expect("open db");
    (db, path)
}

fn empty_connections() -> SharedConnections {
    Arc::new(Mutex::new(std::collections::HashMap::new()))
}

fn make_state(db: &Db, connections: &SharedConnections) -> ironmud::SharedState {
    Arc::new(Mutex::new(ironmud::World {
        engine: rhai::Engine::new(),
        db: db.clone(),
        connections: connections.clone(),
        scripts: HashMap::new(),
        command_metadata: HashMap::new(),
        class_definitions: HashMap::new(),
        trait_definitions: HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: HashMap::new(),
        language_definitions: HashMap::new(),
        recipes: HashMap::new(),
        spell_definitions: HashMap::new(),
        achievement_definitions: HashMap::new(),
        achievement_index_by_counter: HashMap::new(),
        transports: HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
    }))
}

fn make_quest(vnum: &str, name: &str) -> QuestData {
    QuestData::new(vnum.to_string(), name.to_string())
}

fn save_character(db: &Db, name: &str) -> CharacterData {
    let json = serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    });
    let mut ch: CharacterData = serde_json::from_value(json).expect("char");
    ch.gold = 100;
    db.save_character_data(ch.clone()).expect("save char");
    ch
}

#[test]
fn quest_data_round_trips_through_json() {
    let mut q = make_quest("qst:9000", "Wolf Hunt");
    q.summary = "The pack leader stalks the eastern wood.".into();
    q.objectives.push(QuestObjective::KillMob {
        vnum: "9001".into(),
        count: 3,
    });
    q.rewards.push(QuestReward::Gold { amount: 50 });
    q.rewards.push(QuestReward::SkillXp {
        skill: "wilderness".into(),
        amount: 30,
    });
    q.repeatable = true;
    q.giver_mob_vnum = Some("9100".into());

    let json = serde_json::to_string(&q).expect("ser");
    let back: QuestData = serde_json::from_str(&json).expect("de");
    assert_eq!(back.vnum, "qst:9000");
    assert_eq!(back.objectives.len(), 1);
    assert_eq!(back.rewards.len(), 2);
    assert!(back.repeatable);
    assert_eq!(back.giver_mob_vnum.as_deref(), Some("9100"));
    if let QuestObjective::KillMob { count, .. } = &back.objectives[0] {
        assert_eq!(*count, 3);
    } else {
        panic!("expected KillMob");
    }
}

#[test]
fn quest_offer_adds_to_active_then_idempotent() {
    let (db, path) = fresh_db("offer_idempotent");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let q = make_quest("qst:1", "First");
        db.save_quest_data(&q).expect("save quest");
        save_character(&db, "alice");

        assert_eq!(ironmud::quest::offer(&db, "alice", "qst:1"), "");
        let ch = db.get_character_data("alice").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:1"));
        // Second offer fails (already on quest).
        let err = ironmud::quest::offer(&db, "alice", "qst:1");
        assert!(err.contains("already"));
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_kill_listener_increments_progress() {
    let (db, path) = fresh_db("kill_listener");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:2", "Kill the Mice");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "179".into(),
            count: 3,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "bob");
        ironmud::quest::offer(&db, "bob", "qst:2");

        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179");
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179");

        let ch = db.get_character_data("bob").unwrap().unwrap();
        let progress = ch.active_quests.get("qst:2").expect("active");
        assert_eq!(progress.kill_progress.get("179").copied(), Some(2));

        // Third kill auto-completes (only objective is kill).
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179");
        let ch = db.get_character_data("bob").unwrap().unwrap();
        assert!(!ch.active_quests.contains_key("qst:2"));
        assert!(ch.completed_quests.contains("qst:2"));
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_item_turn_in_consumes_item_and_advances() {
    let (db, path) = fresh_db("item_turn_in");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Quest "bring 1 mouse_tail to mob 100".
        let mut q = make_quest("qst:3", "Tail Tribute");
        q.objectives.push(QuestObjective::BringItem {
            vnum: "5001".into(),
            qty: 1,
            return_to_mob_vnum: Some("100".into()),
        });
        q.rewards.push(QuestReward::Gold { amount: 25 });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "carol");
        ironmud::quest::offer(&db, "carol", "qst:3");

        // Make a mob with vnum "100".
        let mut mob = MobileData::new("courier".into());
        mob.vnum = "100".into();
        db.save_mobile_data(mob.clone()).expect("save mob");

        // Make an item in carol's inventory with vnum "5001".
        let mut item = ItemData::new("a mouse tail".into(), "a mouse tail".into(), "a mouse tail".into());
        item.vnum = Some("5001".into());
        item.is_prototype = false;
        item.location = ItemLocation::Inventory("carol".into());
        db.save_item_data(item.clone()).expect("save item");

        let conns = empty_connections();
        let state = make_state(&db, &conns);
        let consumed = ironmud::quest::handle_item_to_mob(&db, &conns, &state, "carol", &mob, &item);
        assert!(consumed, "quest must consume the matching item");

        // Item should be deleted, quest should be completed (only objective + auto).
        assert!(db.get_item_data(&item.id).unwrap().is_none());
        let ch = db.get_character_data("carol").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:3"));
        assert_eq!(ch.gold, 100 + 25, "gold reward applied");
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_item_turn_in_ignores_unrelated_item() {
    let (db, path) = fresh_db("item_unrelated");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:4", "Specific Pelt");
        q.objectives.push(QuestObjective::BringItem {
            vnum: "5001".into(),
            qty: 1,
            return_to_mob_vnum: Some("100".into()),
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "dan");
        ironmud::quest::offer(&db, "dan", "qst:4");

        let mut mob = MobileData::new("courier".into());
        mob.vnum = "100".into();
        db.save_mobile_data(mob.clone()).expect("save mob");

        let mut item = ItemData::new("a stick".into(), "a stick".into(), "a stick".into());
        item.vnum = Some("9999".into()); // Wrong vnum.
        item.is_prototype = false;
        item.location = ItemLocation::Inventory("dan".into());
        db.save_item_data(item.clone()).expect("save item");

        let conns = empty_connections();
        let state = make_state(&db, &conns);
        let consumed = ironmud::quest::handle_item_to_mob(&db, &conns, &state, "dan", &mob, &item);
        assert!(!consumed);
        // Item still exists.
        assert!(db.get_item_data(&item.id).unwrap().is_some());
        let ch = db.get_character_data("dan").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:4"));
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_completion_grants_skill_xp() {
    let (db, path) = fresh_db("skill_xp_reward");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:5", "Xp Quest");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "200".into(),
            count: 1,
        });
        q.rewards.push(QuestReward::SkillXp {
            skill: "tracking".into(),
            amount: 35,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "eve");
        ironmud::quest::offer(&db, "eve", "qst:5");
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "eve", "200");

        let ch = db.get_character_data("eve").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:5"));
        let progress = ch.skills.get("tracking").expect("skill recorded");
        assert_eq!(progress.experience, 35);
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_repeatable_can_re_accept_after_completion() {
    let (db, path) = fresh_db("repeatable");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:6", "Endless");
        q.repeatable = true;
        q.objectives.push(QuestObjective::KillMob {
            vnum: "300".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "frank");
        ironmud::quest::offer(&db, "frank", "qst:6");
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "frank", "300");
        let ch = db.get_character_data("frank").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:6"));
        assert!(!ch.active_quests.contains_key("qst:6"));
        // Re-accept allowed because repeatable.
        assert_eq!(ironmud::quest::offer(&db, "frank", "qst:6"), "");
        let ch = db.get_character_data("frank").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:6"));
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_non_repeatable_refuses_re_accept() {
    let (db, path) = fresh_db("non_repeatable");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:7", "Once");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "400".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "gina");
        ironmud::quest::offer(&db, "gina", "qst:7");
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "gina", "400");
        let err = ironmud::quest::offer(&db, "gina", "qst:7");
        assert!(err.contains("already completed"));
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn quest_abandon_drops_progress() {
    let (db, path) = fresh_db("abandon");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:8", "Quitter");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "500".into(),
            count: 5,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "henry");
        ironmud::quest::offer(&db, "henry", "qst:8");

        // Make some progress, then abandon.
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "henry", "500");
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "henry", "500");
        let ch = db.get_character_data("henry").unwrap().unwrap();
        assert_eq!(
            ch.active_quests
                .get("qst:8")
                .unwrap()
                .kill_progress
                .get("500")
                .copied(),
            Some(2)
        );

        assert_eq!(ironmud::quest::abandon(&db, "henry", "qst:8"), "");
        let ch = db.get_character_data("henry").unwrap().unwrap();
        assert!(!ch.active_quests.contains_key("qst:8"));
        assert!(!ch.completed_quests.contains("qst:8"));

        // Re-accept resets progress.
        ironmud::quest::offer(&db, "henry", "qst:8");
        let ch = db.get_character_data("henry").unwrap().unwrap();
        let progress = ch.active_quests.get("qst:8").unwrap();
        assert!(progress.kill_progress.is_empty());
    }));
    let _ = std::fs::remove_dir_all(&path);
    result.unwrap();
}

#[test]
fn active_quest_default_skip_serializes_clean() {
    // Empty progress should serialize to a small payload thanks to the
    // skip_if helpers on each field.
    let aq = ActiveQuest::default();
    let s = serde_json::to_string(&aq).unwrap();
    assert!(!s.contains("kill_progress"));
    assert!(!s.contains("item_progress"));
    assert!(!s.contains("rooms_visited"));
    assert!(!s.contains("flags_set"));
}
