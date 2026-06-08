//! Quest system slice 1 integration tests.
//!
//! Covers the QuestData / ActiveQuest types, sled persistence, the offer /
//! abandon / try_complete state machine, and the kill / item-turn-in
//! listener entry points exposed by `ironmud::quest`.

#![recursion_limit = "256"]

use ironmud::SharedConnections;
use ironmud::db::Db;
use ironmud::types::{
    ActiveQuest, CharacterData, ItemData, ItemLocation, MobileData, QuestData, QuestObjective, QuestReward,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn fresh_db(_label: &str) -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
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
        socials: ironmud::social::actions::SocialRegistry::default(),
        class_definitions: HashMap::new(),
        trait_definitions: HashMap::new(),
        race_suggestions: Vec::new(),
        race_definitions: HashMap::new(),
        language_definitions: HashMap::new(),
        recipes: HashMap::new(),
        spell_definitions: HashMap::new(),
        achievement_definitions: HashMap::new(),
        achievement_index_by_counter: HashMap::new(),
        custom_skill_definitions: HashMap::new(),
        transports: HashMap::new(),
        chat_sender: None,
        shutdown_sender: None,
        shutdown_cancel_sender: None,
        ip_limiter: Arc::new(ironmud::ratelimit::IpRateLimiter::new()),
        command_throttle: Arc::new(ironmud::throttle::CommandThrottle::new()),
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
    let (db, _temp) = fresh_db("offer_idempotent");
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
    result.unwrap();
}

#[test]
fn quest_kill_listener_increments_progress() {
    let (db, _temp) = fresh_db("kill_listener");
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
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179", &HashMap::new());
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179", &HashMap::new());

        let ch = db.get_character_data("bob").unwrap().unwrap();
        let progress = ch.active_quests.get("qst:2").expect("active");
        assert_eq!(progress.kill_progress.get("179").copied(), Some(2));

        // Third kill auto-completes (only objective is kill).
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "179", &HashMap::new());
        let ch = db.get_character_data("bob").unwrap().unwrap();
        assert!(!ch.active_quests.contains_key("qst:2"));
        assert!(ch.completed_quests.contains("qst:2"));
    }));
    result.unwrap();
}

#[test]
fn quest_item_turn_in_consumes_item_and_advances() {
    let (db, _temp) = fresh_db("item_turn_in");
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
    result.unwrap();
}

#[test]
fn quest_item_turn_in_ignores_unrelated_item() {
    let (db, _temp) = fresh_db("item_unrelated");
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
    result.unwrap();
}

#[test]
fn quest_completion_grants_skill_xp() {
    let (db, _temp) = fresh_db("skill_xp_reward");
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
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "eve", "200", &HashMap::new());

        let ch = db.get_character_data("eve").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:5"));
        let progress = ch.skills.get("tracking").expect("skill recorded");
        assert_eq!(progress.experience, 35);
    }));
    result.unwrap();
}

#[test]
fn quest_repeatable_can_re_accept_after_completion() {
    let (db, _temp) = fresh_db("repeatable");
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
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "frank", "300", &HashMap::new());
        let ch = db.get_character_data("frank").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:6"));
        assert!(!ch.active_quests.contains_key("qst:6"));
        // Re-accept allowed because repeatable.
        assert_eq!(ironmud::quest::offer(&db, "frank", "qst:6"), "");
        let ch = db.get_character_data("frank").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:6"));
    }));
    result.unwrap();
}

#[test]
fn quest_non_repeatable_refuses_re_accept() {
    let (db, _temp) = fresh_db("non_repeatable");
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
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "gina", "400", &HashMap::new());
        let err = ironmud::quest::offer(&db, "gina", "qst:7");
        assert!(err.contains("already completed"));
    }));
    result.unwrap();
}

#[test]
fn quest_abandon_drops_progress() {
    let (db, _temp) = fresh_db("abandon");
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
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "henry", "500", &HashMap::new());
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "henry", "500", &HashMap::new());
        let ch = db.get_character_data("henry").unwrap().unwrap();
        assert_eq!(
            ch.active_quests.get("qst:8").unwrap().kill_progress.get("500").copied(),
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

/// Slice 3c: party kill credit. Two players damage the same mob; both
/// have a kill quest active. handle_mob_kill is called with damaged_by
/// containing both names — both should advance.
#[test]
fn quest_party_credit_advances_all_damagers() {
    let (db, _temp) = fresh_db("party_credit");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:party", "Slay the Hydra");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "hydra".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");

        save_character(&db, "alice");
        save_character(&db, "bob");
        ironmud::quest::offer(&db, "alice", "qst:party");
        ironmud::quest::offer(&db, "bob", "qst:party");

        let conns = empty_connections();
        let state = make_state(&db, &conns);

        // Bob lands the killing blow; Alice did some damage.
        let mut damaged_by = HashMap::new();
        damaged_by.insert("alice".to_string(), 30);
        damaged_by.insert("bob".to_string(), 20);

        ironmud::quest::handle_mob_kill(&db, &conns, &state, "bob", "hydra", &damaged_by);

        let alice = db.get_character_data("alice").unwrap().unwrap();
        let bob = db.get_character_data("bob").unwrap().unwrap();
        assert!(alice.completed_quests.contains("qst:party"), "alice should be credited");
        assert!(bob.completed_quests.contains("qst:party"), "bob should be credited");
    }));
    result.unwrap();
}

/// Slice 3b: a quest with `duration_secs` set drops from active_quests after
/// the duration elapses. Test by backdating `started_at` and calling the
/// expiry helper directly.
#[test]
fn quest_expires_when_duration_elapsed() {
    let (db, _temp) = fresh_db("expire");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:tl1", "Tick Tock");
        q.duration_secs = Some(60);
        q.objectives.push(QuestObjective::KillMob {
            vnum: "boss".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "racer");
        ironmud::quest::offer(&db, "racer", "qst:tl1");

        // Backdate started_at so duration has already elapsed.
        let mut ch = db.get_character_data("racer").unwrap().unwrap();
        ch.active_quests.get_mut("qst:tl1").unwrap().started_at = 0;
        db.save_character_data(ch).unwrap();

        let conns = empty_connections();
        ironmud::quest::expire_quests_for(&db, &conns, "racer", 9999);

        let ch = db.get_character_data("racer").unwrap().unwrap();
        assert!(!ch.active_quests.contains_key("qst:tl1"));
        // Expiry should NOT mark the quest as completed.
        assert!(!ch.completed_quests.contains("qst:tl1"));

        // Re-offer should be allowed (no completed-non-repeatable block).
        let err = ironmud::quest::offer(&db, "racer", "qst:tl1");
        assert_eq!(err, "");
    }));
    result.unwrap();
}

/// Slice 3a: prereq_quest_vnum gates `offer` until the prereq is completed.
#[test]
fn quest_offer_blocked_by_prereq() {
    let (db, _temp) = fresh_db("offer_prereq");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut prereq = make_quest("qst:p1", "Tutorial");
        prereq.objectives.push(QuestObjective::KillMob {
            vnum: "tut".into(),
            count: 1,
        });
        db.save_quest_data(&prereq).expect("save");

        let mut q = make_quest("qst:p2", "Advanced Hunt");
        q.prereq_quest_vnum = Some("qst:p1".into());
        q.objectives.push(QuestObjective::KillMob {
            vnum: "wolf".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");

        save_character(&db, "novice");

        // Refused without prereq.
        let err = ironmud::quest::offer(&db, "novice", "qst:p2");
        assert!(err.contains("must first complete"), "got: {}", err);

        // Mark prereq as complete; now offer succeeds.
        let mut ch = db.get_character_data("novice").unwrap().unwrap();
        ch.completed_quests.insert("qst:p1".into());
        db.save_character_data(ch).unwrap();
        let err = ironmud::quest::offer(&db, "novice", "qst:p2");
        assert_eq!(err, "");
    }));
    result.unwrap();
}

/// Slice 3a: min_player_skill_total gates `offer` until the player's summed
/// skill levels meet the threshold.
#[test]
fn quest_offer_blocked_by_skill_total() {
    let (db, _temp) = fresh_db("offer_skill");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:s1", "Master Class");
        q.min_player_skill_total = Some(15);
        q.objectives.push(QuestObjective::KillMob {
            vnum: "boss".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "apprentice");

        // Total = 0; refused.
        let err = ironmud::quest::offer(&db, "apprentice", "qst:s1");
        assert!(err.contains("not skilled"), "got: {}", err);

        // Bump skills past threshold.
        let mut ch = db.get_character_data("apprentice").unwrap().unwrap();
        ch.skills.insert(
            "swords".into(),
            ironmud::types::SkillProgress {
                level: 8,
                experience: 0,
            },
        );
        ch.skills.insert(
            "shields".into(),
            ironmud::types::SkillProgress {
                level: 8,
                experience: 0,
            },
        );
        db.save_character_data(ch).unwrap();

        let err = ironmud::quest::offer(&db, "apprentice", "qst:s1");
        assert_eq!(err, "");
    }));
    result.unwrap();
}

/// §4.E.3: achievement_set_prereq gates `offer` until min_count keys in the
/// set are unlocked. Tracks the same map QuestReward::Achievement writes to,
/// so upstream investigation quests can stamp the keys and the endgame
/// quest's offer flips on automatically.
#[test]
fn quest_offer_blocked_by_achievement_set_prereq() {
    let (db, _temp) = fresh_db("offer_achievement_set");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:endgame", "Court of the Concord");
        q.achievement_set_prereq = Some(ironmud::types::AchievementSetPrereq {
            keys: vec![
                "investigation_q1".into(),
                "investigation_q2".into(),
                "investigation_q3".into(),
                "investigation_q4".into(),
                "investigation_q5".into(),
            ],
            min_count: 3,
        });
        q.objectives.push(QuestObjective::KillMob {
            vnum: "stranger".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "investigator");

        // Zero unlocked: refused.
        let err = ironmud::quest::offer(&db, "investigator", "qst:endgame");
        assert!(err.contains("haven't proven enough"), "got: {}", err);

        // Two unlocked: still refused (below threshold of 3).
        let mut ch = db.get_character_data("investigator").unwrap().unwrap();
        ch.achievements_unlocked.insert(
            "investigation_q1".into(),
            ironmud::types::AchievementUnlock { unlocked_at: 1 },
        );
        ch.achievements_unlocked.insert(
            "investigation_q2".into(),
            ironmud::types::AchievementUnlock { unlocked_at: 1 },
        );
        db.save_character_data(ch).unwrap();
        let err = ironmud::quest::offer(&db, "investigator", "qst:endgame");
        assert!(err.contains("haven't proven enough"), "got: {}", err);

        // Third unlocked: gate opens.
        let mut ch = db.get_character_data("investigator").unwrap().unwrap();
        ch.achievements_unlocked.insert(
            "investigation_q4".into(),
            ironmud::types::AchievementUnlock { unlocked_at: 1 },
        );
        db.save_character_data(ch).unwrap();
        let err = ironmud::quest::offer(&db, "investigator", "qst:endgame");
        assert_eq!(err, "");
        let ch = db.get_character_data("investigator").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:endgame"));
    }));
    result.unwrap();
}

/// §4.E.5: KillAnyMob accumulates a shared counter across the listed vnums
/// and auto-completes a kill-only quest when the threshold is met.
#[test]
fn quest_kill_any_mob_objective_accumulates_and_completes() {
    let (db, _temp) = fresh_db("kill_any");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:bounty", "Hunter Bounty");
        q.objectives.push(QuestObjective::KillAnyMob {
            vnums: vec!["hunter_a".into(), "hunter_b".into(), "hunter_c".into()],
            count: 3,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "deputy");
        ironmud::quest::offer(&db, "deputy", "qst:bounty");

        let conns = empty_connections();
        let state = make_state(&db, &conns);
        // Two kills across different vnums in the set both credit the shared
        // counter.
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "deputy", "hunter_a", &HashMap::new());
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "deputy", "hunter_c", &HashMap::new());

        let ch = db.get_character_data("deputy").unwrap().unwrap();
        let progress = ch.active_quests.get("qst:bounty").expect("active");
        let key = ironmud::types::kill_any_key(&["hunter_a".into(), "hunter_b".into(), "hunter_c".into()]);
        assert_eq!(progress.kill_any_progress.get(&key).copied(), Some(2));
        assert!(
            progress.kill_progress.is_empty(),
            "KillAnyMob must not touch the per-vnum kill_progress bucket"
        );

        // A kill of a vnum NOT in the set is ignored.
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "deputy", "innocent", &HashMap::new());
        let ch = db.get_character_data("deputy").unwrap().unwrap();
        let progress = ch.active_quests.get("qst:bounty").unwrap();
        assert_eq!(progress.kill_any_progress.get(&key).copied(), Some(2));

        // Third in-set kill hits the threshold and auto-completes.
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "deputy", "hunter_b", &HashMap::new());
        let ch = db.get_character_data("deputy").unwrap().unwrap();
        assert!(!ch.active_quests.contains_key("qst:bounty"));
        assert!(ch.completed_quests.contains("qst:bounty"));
    }));
    result.unwrap();
}

/// kill_any_key is order-independent and dedupes so the same logical set
/// hashes to the same storage key.
#[test]
fn kill_any_key_is_stable_across_input_order() {
    let k1 = ironmud::types::kill_any_key(&["c".into(), "a".into(), "b".into()]);
    let k2 = ironmud::types::kill_any_key(&["b".into(), "c".into(), "a".into()]);
    let k3 = ironmud::types::kill_any_key(&["a".into(), "a".into(), "b".into(), "c".into()]);
    assert_eq!(k1, k2);
    assert_eq!(k1, k3);
    assert_eq!(k1, "a,b,c");
}

/// Slice 2a: VisitRoom listener advances rooms_visited and auto-completes a
/// visit-only quest.
#[test]
fn quest_visit_room_listener_completes_visit_quest() {
    let (db, _temp) = fresh_db("visit_listener");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:60", "Wander to the Crossroads");
        q.objectives.push(QuestObjective::VisitRoom { vnum: "tav_42".into() });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "wanderer");
        ironmud::quest::offer(&db, "wanderer", "qst:60");

        let conns = empty_connections();
        let state = make_state(&db, &conns);
        ironmud::quest::handle_room_visit(&db, &conns, &state, "wanderer", "tav_42");

        let ch = db.get_character_data("wanderer").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:60"));
        assert!(!ch.active_quests.contains_key("qst:60"));
    }));
    result.unwrap();
}

/// Slice 2b: DgFlag listener advances flags_set and auto-completes a
/// flag-only quest.
#[test]
fn quest_dg_flag_listener_completes_flag_quest() {
    let (db, _temp) = fresh_db("dg_flag_listener");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:61", "Solve the Riddle");
        q.objectives.push(QuestObjective::DgFlag {
            var: "puzzle_solved".into(),
            value: "1".into(),
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "thinker");
        ironmud::quest::offer(&db, "thinker", "qst:61");

        let conns = empty_connections();
        let state = make_state(&db, &conns);

        // Wrong value — must not advance.
        ironmud::quest::handle_dg_flag_set(&db, &conns, &state, "thinker", "puzzle_solved", "0");
        let ch = db.get_character_data("thinker").unwrap().unwrap();
        assert!(ch.active_quests.contains_key("qst:61"));
        let p = ch.active_quests.get("qst:61").unwrap();
        assert!(p.flags_set.is_empty());

        // Right value — must auto-complete.
        ironmud::quest::handle_dg_flag_set(&db, &conns, &state, "thinker", "puzzle_solved", "1");
        let ch = db.get_character_data("thinker").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:61"));
    }));
    result.unwrap();
}

/// Slice 2c: describe_quest_offers returns the right cue based on character
/// state: offerable -> "has a quest for you", completable -> "awaits your
/// return".
#[test]
fn quest_describe_quest_offers_returns_cue() {
    let (db, _temp) = fresh_db("describe_offers");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut q = make_quest("qst:70", "Hunt the Wolves");
        q.giver_mob_vnum = Some("hunter".into());
        q.objectives.push(QuestObjective::KillMob {
            vnum: "wolf".into(),
            count: 1,
        });
        db.save_quest_data(&q).expect("save");
        save_character(&db, "scout");

        // No active, not completed -> offerable.
        let cue = ironmud::quest::describe_quest_offers(&db, "scout", "hunter");
        assert_eq!(cue.as_deref(), Some("(has a quest for you)"));

        ironmud::quest::offer(&db, "scout", "qst:70");

        // Active but not completable -> no cue.
        let cue = ironmud::quest::describe_quest_offers(&db, "scout", "hunter");
        assert_eq!(cue, None);

        // Advance progress -> completable -> "awaits your return".
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        // We don't want to auto-complete (handle_mob_kill auto-completes
        // kill-only quests). Manually mark progress instead.
        let mut ch = db.get_character_data("scout").unwrap().unwrap();
        ch.active_quests
            .get_mut("qst:70")
            .unwrap()
            .kill_progress
            .insert("wolf".into(), 1);
        db.save_character_data(ch).unwrap();
        let cue = ironmud::quest::describe_quest_offers(&db, "scout", "hunter");
        assert_eq!(cue.as_deref(), Some("(awaits your return)"));

        // Sanity: try_complete drops the cue.
        ironmud::quest::try_complete(&db, &conns, &state, "scout", "qst:70");
        let cue = ironmud::quest::describe_quest_offers(&db, "scout", "hunter");
        // Non-repeatable -> already-completed mobs get nothing.
        assert_eq!(cue, None);
    }));
    result.unwrap();
}

/// Slice 1.1: the CompleteQuest dialogue effect (and try_complete in general)
/// must grant Achievement rewards via crate::script::achievements::award_core.
/// Before slice 1.1, achievement rewards from a CompleteQuest dialogue effect
/// were skipped with a "(reward pending)" line. Now they go through the
/// canonical try_complete path.
#[test]
fn quest_complete_grants_achievement_reward_via_try_complete() {
    use ironmud::types::{
        AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource,
    };
    let (db, _temp) = fresh_db("achievement_reward");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Achievement: must be Manual so award_core(manual=true) accepts it.
        let ach = AchievementDef {
            key: "wolf_slayer".into(),
            name: "Wolf Slayer".into(),
            description: "Cleared the eastern wolves.".into(),
            category: AchievementCategory::Combat,
            criterion: AchievementCriterion::Manual,
            reward: AchievementReward::default(),
            hidden: false,
            source: AchievementSource::Db { author: "test".into() },
        };

        // Quest: kill 1 wolf -> achievement.
        let mut q = make_quest("qst:50", "Wolf Hunt");
        q.objectives.push(QuestObjective::KillMob {
            vnum: "9001".into(),
            count: 1,
        });
        q.rewards.push(QuestReward::Achievement {
            key: "wolf_slayer".into(),
        });
        db.save_quest_data(&q).expect("save quest");
        save_character(&db, "hero");
        ironmud::quest::offer(&db, "hero", "qst:50");

        // Build state with the achievement def registered.
        let conns = empty_connections();
        let state = make_state(&db, &conns);
        {
            let mut world = state.lock().unwrap();
            world.achievement_definitions.insert("wolf_slayer".into(), ach);
        }

        // Listener path advances + auto-completes (kill-only quest).
        ironmud::quest::handle_mob_kill(&db, &conns, &state, "hero", "9001", &HashMap::new());

        let ch = db.get_character_data("hero").unwrap().unwrap();
        assert!(ch.completed_quests.contains("qst:50"));
        assert!(
            ch.achievements_unlocked.contains_key("wolf_slayer"),
            "achievement should have been awarded; unlocked = {:?}",
            ch.achievements_unlocked.keys().collect::<Vec<_>>()
        );
    }));
    result.unwrap();
}
