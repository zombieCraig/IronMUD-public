use super::*;
use crate::types::{CodexMeta, SpellOrigin};
use crate::{CharacterData, PlayerSession, World};
use parking_lot::Mutex;
use std::sync::Arc;

fn temp_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn no_one_online() -> SharedConnections {
    Arc::new(Mutex::new(HashMap::new()))
}

fn saved(db: &Db, name: &str) -> CharacterData {
    let ch: CharacterData = serde_json::from_value(serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": uuid::Uuid::new_v4(),
    }))
    .expect("build character");
    db.save_character_data(ch.clone()).expect("save");
    ch
}

fn online(ch: &CharacterData) -> SharedConnections {
    let conns = no_one_online();
    let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (tx_input, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
    let mut session = PlayerSession::new_for_test(tx_client, tx_input);
    session.character = Some(ch.clone());
    conns.lock().insert(uuid::Uuid::new_v4(), session);
    std::mem::forget((rx, rx_in));
    conns
}

fn session_char(conns: &SharedConnections) -> CharacterData {
    conns
        .lock()
        .values()
        .find_map(|s| s.character.clone())
        .expect("character is online")
}

fn entry(id: &str) -> CodexEntry {
    CodexEntry {
        def: serde_json::from_value(serde_json::json!({
            "id": id,
            "name": "Ember Lance",
            "description": "A lance of living flame.",
            "spell_type": "damage",
            "mana_cost": 40,
            "damage_base": 30,
            "target_type": "enemy",
        }))
        .expect("build def"),
        meta: CodexMeta {
            origin: SpellOrigin::Authored,
            author: Some("Mira".into()),
            god_vnum: "god:sol".into(),
            ..CodexMeta::default()
        },
    }
}

// ---------- settings ----------

#[test]
fn settings_default_when_nothing_is_set() {
    let (db, _t) = temp_db();
    let s = Settings::load(&db);
    assert_eq!(s, Settings::default());
    assert!(!s.enabled, "ships dark");
    assert_eq!(s.min_magic, 10);
    assert_eq!(s.min_spellcraft, 8);
    assert_eq!(s.min_mastered_spells, 5);
    assert_eq!(s.min_favor, 500);
    assert_eq!(s.min_power_tier, 3);
    assert_eq!(s.char_cooldown_days, 30);
    assert_eq!(s.god_petition_cooldown_days, 7);
    assert_eq!(s.pledge_floor_pct, 60);
    assert_eq!(s.regrow_days, 14);
    assert_eq!(s.canon_grace_days, 30);
    assert_eq!(s.component_quality_base, 40);
    assert_eq!(s.component_quality_per_tier, 15);
    assert_eq!(s.max_power, 160);
    assert_eq!(s.hp_pledge_pct, 20);
}

#[test]
fn settings_read_overrides_and_ignore_garbage() {
    let (db, _t) = temp_db();
    db.set_setting("spellcraft.enabled", "on").unwrap();
    db.set_setting("spellcraft.min_magic", "12").unwrap();
    db.set_setting("spellcraft.regrow_days", "soon").unwrap();
    let s = Settings::load(&db);
    assert!(s.enabled);
    assert_eq!(s.min_magic, 12);
    assert_eq!(s.regrow_days, Settings::default().regrow_days, "unparseable falls back");
}

#[test]
fn every_setting_key_is_read() {
    // A registered key that `load` ignores would be a dead admin knob.
    let (db, _t) = temp_db();
    let keys: Vec<_> = setting_keys().collect();
    assert_eq!(keys.len(), 21);
    for key in &keys {
        let v = if *key == "spellcraft.enabled" { "true" } else { "777" };
        db.set_setting(key, v).unwrap();
    }
    let s = Settings::load(&db);
    let debug = format!("{s:?}");
    assert_eq!(debug.matches("777").count(), keys.len() - 1, "{debug}");
    assert!(s.enabled);
}

// ---------- codex storage ----------

#[test]
fn insert_if_absent_claims_an_id_once() {
    let (db, _t) = temp_db();
    assert!(db.insert_codex_if_absent(&entry("cx_ember")).unwrap());
    let mut other = entry("cx_ember");
    other.def.name = "Usurper".into();
    assert!(!db.insert_codex_if_absent(&other).unwrap());
    assert_eq!(db.get_codex("cx_ember").unwrap().unwrap().def.name, "Ember Lance");
}

#[test]
fn update_codex_retries_under_contention() {
    let (db, _t) = temp_db();
    db.insert_codex_if_absent(&entry("cx_ember")).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                for _ in 0..25 {
                    db.update_codex("cx_ember", |e| e.meta.version += 1).unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(
        db.get_codex("cx_ember").unwrap().unwrap().meta.version,
        200,
        "no lost update"
    );
    assert!(
        db.update_codex("cx_missing", |e| e.meta.version += 1)
            .unwrap()
            .is_none()
    );
}

#[test]
fn delete_and_list_codex() {
    let (db, _t) = temp_db();
    db.insert_codex_if_absent(&entry("cx_a")).unwrap();
    db.insert_codex_if_absent(&entry("cx_b")).unwrap();
    assert_eq!(db.list_codex().unwrap().len(), 2);
    assert!(db.delete_codex("cx_a").unwrap());
    assert!(!db.delete_codex("cx_a").unwrap());
    assert_eq!(db.list_codex().unwrap().len(), 1);
}

// ---------- loading ----------

#[test]
fn codex_spells_survive_reload_and_preset_switch() {
    let (db, _t) = temp_db();
    db.insert_codex_if_absent(&entry("cx_ember")).unwrap();
    let state = World::minimal_shared(db.clone(), no_one_online());

    crate::load_game_data(state.clone()).expect("load");
    {
        let world = state.lock();
        let def = world.spell_definitions.get("cx_ember").expect("codex spell loaded");
        assert!(def.codex, "loader marks codex spells");
        assert!(def.scroll_only, "codex spells must be learned, never skill-granted");
        assert!(world.spell_definitions.len() > 1, "preset spells still load");
    }

    // A preset switch (or `admin reload`) replaces the JSON map wholesale;
    // the stale marker proves the replace happened, and the codex must come
    // back on top of it.
    {
        let mut world = state.lock();
        let marker = world.spell_definitions.get("cx_ember").unwrap().clone();
        world.spell_definitions.insert("stale_marker".into(), marker);
    }
    db.set_setting("spell_preset", "fantasy").unwrap();
    crate::load_game_data(state.clone()).expect("reload");
    let world = state.lock();
    assert!(
        !world.spell_definitions.contains_key("stale_marker"),
        "map was replaced"
    );
    assert!(world.spell_definitions.contains_key("cx_ember"));
}

#[test]
fn insert_codex_spell_makes_it_castable_now() {
    let (db, _t) = temp_db();
    let state = World::minimal_shared(db.clone(), no_one_online());
    assert!(insert_codex_spell(&db, &state, &entry("cx_ember")).unwrap());
    assert!(state.lock().spell_definitions.get("cx_ember").unwrap().codex);
    assert!(
        !insert_codex_spell(&db, &state, &entry("cx_ember")).unwrap(),
        "id already taken"
    );
}

// ---------- learning ----------

#[test]
fn learn_is_session_first_and_survives_a_flush() {
    let (db, _t) = temp_db();
    let ch = saved(&db, "Pupil");
    let conns = online(&ch);
    let state = World::minimal_shared(db.clone(), conns.clone());
    insert_codex_spell(&db, &state, &entry("cx_ember")).unwrap();

    assert_eq!(
        learn(&db, &conns, &state, "Pupil", "cx_ember", LearnVia::Scroll),
        LearnResult::Learned
    );
    assert!(session_char(&conns).learned_spells.contains(&"cx_ember".to_string()));

    // The regen tick writes the session over the DB wholesale.
    db.save_character_data(session_char(&conns)).unwrap();
    let stored = db.get_character_data("pupil").unwrap().unwrap();
    assert!(stored.learned_spells.contains(&"cx_ember".to_string()));

    assert_eq!(
        learn(&db, &conns, &state, "Pupil", "cx_ember", LearnVia::Temple),
        LearnResult::AlreadyKnown
    );
}

#[test]
fn learn_refuses_unknown_spells_and_missing_characters() {
    let (db, _t) = temp_db();
    saved(&db, "Pupil");
    let conns = no_one_online();
    let state = World::minimal_shared(db.clone(), conns.clone());
    assert_eq!(
        learn(&db, &conns, &state, "Pupil", "cx_nothing", LearnVia::Admin),
        LearnResult::UnknownSpell
    );
    insert_codex_spell(&db, &state, &entry("cx_ember")).unwrap();
    assert_eq!(
        learn(&db, &conns, &state, "Ghost", "cx_ember", LearnVia::Admin),
        LearnResult::NoCharacter
    );
    // Offline learning writes the DB record.
    assert_eq!(
        learn(&db, &conns, &state, "Pupil", "cx_ember", LearnVia::Admin),
        LearnResult::Learned
    );
    assert!(
        db.get_character_data("pupil")
            .unwrap()
            .unwrap()
            .learned_spells
            .contains(&"cx_ember".to_string())
    );
}

#[test]
fn learn_via_parses_script_names() {
    assert_eq!(LearnVia::parse("Scroll"), Some(LearnVia::Scroll));
    assert_eq!(LearnVia::parse("temple"), Some(LearnVia::Temple));
    assert_eq!(LearnVia::parse("evolution"), Some(LearnVia::Evolution));
    assert_eq!(LearnVia::parse("admin"), Some(LearnVia::Admin));
    assert_eq!(LearnVia::parse("osmosis"), None);
}

// ---------- ancient seed and moderation ----------

fn seed_doc(entries: serde_json::Value) -> String {
    serde_json::json!({ "_doc": "test", "spells": entries }).to_string()
}

fn ancient(id: &str, name: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "def": {
            "id": id, "name": name, "description": "Old <b>magic</b>.",
            "spell_type": "damage", "damage_base": 60, "target_type": "enemy"
        },
        "meta": { "god_vnum": "god:sol", "status": status, "origin": "authored",
                  "flavor": { "caster": "\u{1b}[31mFire\u{1b}[0m" } }
    })
}

#[test]
fn seeding_adds_new_entries_once_and_normalises_them() {
    let (db, _t) = temp_db();
    let mut spells = HashMap::new();
    let doc = seed_doc(serde_json::json!([ancient("cx_old_flame", "Old Flame", "faded")]));

    let first = codex::seed_from_str(&db, &mut spells, &doc);
    assert_eq!(first.added, vec!["cx_old_flame".to_string()], "{first:?}");
    let stored = db.get_codex("cx_old_flame").unwrap().unwrap();
    assert_eq!(
        stored.meta.origin,
        SpellOrigin::Ancient,
        "seeded spells are always Ancient"
    );
    assert_eq!(stored.meta.status, crate::types::CanonStatus::Faded);
    assert!(!stored.def.description.contains('<'), "description escaped");
    assert!(!stored.meta.flavor.caster.contains('\u{1b}'), "ANSI stripped");
    assert!(spells["cx_old_flame"].codex);

    let again = codex::seed_from_str(&db, &mut spells, &doc);
    assert!(again.added.is_empty());
    assert_eq!(again.skipped, 1, "re-seeding never overwrites");
}

#[test]
fn seeding_rejects_bad_entries_and_keeps_good_ones() {
    let (db, _t) = temp_db();
    let mut spells = HashMap::new();
    let mut no_god = ancient("cx_godless", "Godless Hymn", "lost");
    no_god["meta"]["god_vnum"] = serde_json::json!("");
    let mut utility = ancient("cx_utility", "Odd Working", "lost");
    utility["def"]["spell_type"] = serde_json::json!("utility");
    let doc = seed_doc(serde_json::json!([
        ancient("no_prefix", "Plain Name", "lost"),
        no_god,
        utility,
        ancient("cx_good", "Good Spell", "lost"),
        ancient("cx_clash", "Good", "lost"),
    ]));
    let report = codex::seed_from_str(&db, &mut spells, &doc);
    assert_eq!(report.added, vec!["cx_good".to_string()], "{report:?}");
    assert_eq!(report.errors.len(), 4, "{report:?}");
    assert!(
        report.errors.iter().any(|e| e.contains("too close")),
        "prefix clash caught: {report:?}"
    );
}

#[test]
fn shipped_ancient_seed_file_parses() {
    let (db, _t) = temp_db();
    let mut spells = HashMap::new();
    let report = codex::seed_ancient(&db, &mut spells);
    assert!(report.errors.is_empty(), "{report:?}");
}

#[test]
fn moderation_writes_mirror_into_world() {
    let (db, _t) = temp_db();
    let state = World::minimal_shared(db.clone(), no_one_online());
    insert_codex_spell(&db, &state, &entry("cx_ember")).unwrap();

    codex::rename(&db, &state, "cx_ember", "Sunspear").unwrap();
    assert_eq!(state.lock().spell_definitions["cx_ember"].name, "Sunspear");
    assert_eq!(db.get_codex("cx_ember").unwrap().unwrap().meta.version, 1);
    assert!(codex::rename(&db, &state, "cx_ember", "x").is_err(), "name rules apply");

    codex::set_veto(&db, &state, "cx_ember", true).unwrap();
    let m = db.get_codex("cx_ember").unwrap().unwrap().meta;
    assert!(m.vetoed);
    assert_eq!(m.status, crate::types::CanonStatus::Faded);

    codex::set_status(&db, &state, "cx_ember", crate::types::CanonStatus::Lost).unwrap();
    assert_eq!(
        db.get_codex("cx_ember").unwrap().unwrap().meta.status,
        crate::types::CanonStatus::Lost
    );
    assert!(codex::set_status(&db, &state, "cx_nope", crate::types::CanonStatus::Lost).is_err());
}

// ---------- petition gate (S3) ----------

mod gate_tests {
    use super::*;
    use crate::spellcraft::gate::{self, Clock, petition_status};
    use crate::types::{DeityConfig, DeitySpellcraft, SkillProgress, SpellProgress, WorshipState};

    const GOD: &str = "god:sol";
    const DAY: i64 = 1000;
    const NOW: i64 = 1_800_000_000;

    fn clock() -> Clock {
        Clock {
            today: DAY,
            now_secs: NOW,
        }
    }

    fn god(db: &Db, craft: Option<DeitySpellcraft>) {
        let mut m = crate::types::MobileData::new("Sol".to_string());
        m.vnum = GOD.to_string();
        m.is_prototype = true;
        m.deity = Some(DeityConfig {
            spellcraft: craft,
            ..DeityConfig::default()
        });
        db.save_mobile_data(m).expect("save god");
    }

    fn craft() -> DeitySpellcraft {
        DeitySpellcraft {
            allowed_types: vec!["damage".into()],
            canon_cap: 2,
            unlock_quest_id: Some("rite".into()),
            ..DeitySpellcraft::default()
        }
    }

    /// A character who meets every default requirement.
    fn eligible() -> CharacterData {
        let mut ch: CharacterData = serde_json::from_value(serde_json::json!({
            "name": "Adept", "password_hash": "", "current_room_id": uuid::Uuid::nil(),
        }))
        .unwrap();
        let mut w = WorshipState::new(GOD, DAY);
        w.favor = 600;
        ch.worship = Some(w);
        ch.skills.insert(
            "magic".into(),
            SkillProgress {
                level: 10,
                experience: 0,
            },
        );
        ch.skills.insert(
            "spellcraft".into(),
            SkillProgress {
                level: 8,
                experience: 0,
            },
        );
        for i in 0..5 {
            ch.spell_progress.insert(
                format!("s{i}"),
                SpellProgress {
                    level: 10,
                    experience: 0,
                },
            );
        }
        ch.completed_quests.insert("rite".into());
        ch
    }

    fn unmet(db: &Db, ch: &CharacterData) -> Vec<&'static str> {
        petition_status(db, ch, GOD, &Settings::default(), clock())
            .requirements
            .iter()
            .filter(|r| !r.met)
            .map(|r| r.key)
            .collect()
    }

    #[test]
    fn an_eligible_worshipper_may_petition() {
        let (db, _t) = temp_db();
        god(&db, Some(craft()));
        let ch = eligible();
        assert_eq!(unmet(&db, &ch), Vec::<&str>::new());
        assert!(gate::can_petition(&db, &ch, GOD, &Settings::default(), clock()));
    }

    /// Break each requirement alone: exactly that line fails, and the verdict
    /// follows the checklist.
    #[test]
    fn checklist_and_verdict_agree_for_every_requirement() {
        let (db, _t) = temp_db();
        god(&db, Some(craft()));
        type Break = fn(&mut CharacterData);
        let cases: Vec<(&str, Break)> = vec![
            ("worship", |c| c.worship.as_mut().unwrap().god_vnum = "god:other".into()),
            ("magic", |c| c.skills.get_mut("magic").unwrap().level = 9),
            ("spellcraft", |c| c.skills.get_mut("spellcraft").unwrap().level = 7),
            ("mastered_spells", |c| {
                c.spell_progress.remove("s0");
            }),
            ("favor", |c| c.worship.as_mut().unwrap().favor = 499),
            ("unlock_quest", |c| {
                c.completed_quests.clear();
            }),
            ("calm", |c| c.worship.as_mut().unwrap().anger_stage = 1),
            ("char_cooldown", |c| c.last_petition_at = Some(NOW - 86_400)),
        ];
        for (key, breaker) in cases {
            let mut ch = eligible();
            breaker(&mut ch);
            let failing = unmet(&db, &ch);
            // Leaving the god also fails the lines that need *this* god.
            if key == "worship" {
                assert!(failing.contains(&"worship"), "{key}: {failing:?}");
            } else {
                assert_eq!(failing, vec![key], "breaking {key}");
            }
            assert!(
                !gate::can_petition(&db, &ch, GOD, &Settings::default(), clock()),
                "{key}"
            );
        }
    }

    #[test]
    fn world_state_requirements_fail_alone_too() {
        let (db, _t) = temp_db();
        god(&db, Some(craft()));
        let ch = eligible();

        // The god heard someone else two days ago (default cooldown 7).
        db.update_canon(GOD, |c| {
            c.last_petition_day = Some(DAY - 2);
            true
        })
        .unwrap();
        assert_eq!(unmet(&db, &ch), vec!["god_cooldown"]);

        // Canon full (cap 2), both slots still in grace.
        db.update_canon(GOD, |c| {
            c.last_petition_day = None;
            c.slots = vec![
                crate::types::CanonSlot {
                    spell_id: "cx_a".into(),
                    sealed_day: DAY - 1,
                    ..Default::default()
                },
                crate::types::CanonSlot {
                    spell_id: "cx_b".into(),
                    sealed_day: DAY - 5,
                    ..Default::default()
                },
            ];
            true
        })
        .unwrap();
        assert_eq!(unmet(&db, &ch), vec!["canon_room"]);
        assert!(!gate::can_petition(&db, &ch, GOD, &Settings::default(), clock()));

        // Tribute owed: anger derived from overdue days.
        let mut late = eligible();
        late.worship.as_mut().unwrap().last_tribute_day = DAY - 20;
        db.update_canon(GOD, |c| {
            c.slots.clear();
            true
        })
        .unwrap();
        assert_eq!(unmet(&db, &late), vec!["calm"]);
    }

    #[test]
    fn a_god_without_spellcraft_never_hears_petitions() {
        let (db, _t) = temp_db();
        god(&db, None);
        let status = petition_status(&db, &eligible(), GOD, &Settings::default(), clock());
        assert!(!status.god_grants);
        assert!(!status.can_petition());
    }

    #[test]
    fn per_god_overrides_beat_server_settings() {
        let (db, _t) = temp_db();
        let mut c = craft();
        c.min_favor = Some(750);
        c.petition_cooldown_days = Some(1);
        god(&db, Some(c));
        db.update_canon(GOD, |c| {
            c.last_petition_day = Some(DAY - 2);
            true
        })
        .unwrap();
        // Favor 600 meets the server's 500 but not the god's 750; the god's
        // 1-day cooldown has passed although the server's 7 would not have.
        assert_eq!(unmet(&db, &eligible()), vec!["favor"]);

        let mut favored = eligible();
        favored.worship.as_mut().unwrap().favor = 750;
        assert!(gate::can_petition(&db, &favored, GOD, &Settings::default(), clock()));
    }

    #[test]
    fn server_settings_move_the_gates() {
        let (db, _t) = temp_db();
        god(&db, Some(craft()));
        db.set_setting("spellcraft.min_spellcraft", "9").unwrap();
        let settings = Settings::load(&db);
        let failing: Vec<_> = petition_status(&db, &eligible(), GOD, &settings, clock())
            .requirements
            .into_iter()
            .filter(|r| !r.met)
            .map(|r| r.key)
            .collect();
        assert_eq!(failing, vec!["spellcraft"]);
    }

    #[test]
    fn mastered_count_uses_the_lifetime_counter() {
        let mut ch = eligible();
        ch.spell_progress.clear();
        ch.achievement_counters.insert(gate::SPELLS_MASTERED_COUNTER.into(), 6);
        assert_eq!(gate::spells_mastered(&ch), 6, "evolved-away masteries still count");
    }

    #[test]
    fn mastering_a_spell_awards_spellcraft_once() {
        let (db, _t) = temp_db();
        let ch = saved(&db, "Mage");
        let conns = online(&ch);
        let state = World::minimal_shared(db.clone(), conns.clone());
        let spark: crate::SpellDefinition = serde_json::from_value(serde_json::json!({
            "id": "spark", "name": "Spark", "description": "", "spell_type": "damage"
        }))
        .unwrap();
        state.lock().spell_definitions.insert("spark".into(), spark);
        crate::script::spells::learn_spell(&db, &conns, "Mage", "spark");

        let out = crate::script::spells::add_spell_experience(&db, &conns, &state, "Mage", "spark", 100_000);
        assert!(out.mastered);
        let live = session_char(&conns);
        let total = |c: &CharacterData| {
            let s = &c.skills["spellcraft"];
            (0..s.level).map(crate::script::characters::xp_for_level).sum::<i32>() + s.experience
        };
        assert_eq!(total(&live), XP_PER_MASTERY);
        assert_eq!(live.achievement_counters[gate::SPELLS_MASTERED_COUNTER], 1);

        // More XP at the cap: no second award.
        let again = crate::script::spells::add_spell_experience(&db, &conns, &state, "Mage", "spark", 100_000);
        assert!(!again.mastered);
        let live = session_char(&conns);
        assert_eq!(total(&live), XP_PER_MASTERY);
        assert_eq!(live.achievement_counters[gate::SPELLS_MASTERED_COUNTER], 1);
    }
}

// ---------- petition offer (S4) ----------

mod offer_tests {
    use super::*;
    use crate::spellcraft::petition;
    use crate::types::{
        CanonSlot, DeityConfig, DeitySpellcraft, DraftStep, SkillProgress, SpellDraft, SpellProgress, TrialRule,
        WorshipState,
    };

    const GOD: &str = "god:sol";

    struct World2 {
        db: Db,
        conns: SharedConnections,
        state: crate::SharedState,
        temple: uuid::Uuid,
        _t: tempfile::TempDir,
    }

    fn world(trials: Vec<TrialRule>) -> World2 {
        let (db, t) = temp_db();
        let mut m = crate::types::MobileData::new("Sol".to_string());
        m.vnum = GOD.to_string();
        m.is_prototype = true;
        m.deity = Some(DeityConfig {
            spellcraft: Some(DeitySpellcraft {
                allowed_types: vec!["damage".into()],
                allowed_elements: vec!["fire".into()],
                allowed_targets: vec!["enemy".into()],
                trial_rules: trials,
                ..DeitySpellcraft::default()
            }),
            ..DeityConfig::default()
        });
        db.save_mobile_data(m).unwrap();
        let mut q = crate::types::QuestData::new("rite".into(), "Rite of Embers".into());
        q.repeatable = true;
        db.save_quest_data(&q).unwrap();
        let room: crate::types::RoomData = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(), "title": "Temple", "description": "", "exits": {},
            "flags": {"temple": true},
        }))
        .unwrap();
        let temple = room.id;
        db.save_room_data(room).unwrap();
        let conns = no_one_online();
        let state = World::minimal_shared(db.clone(), conns.clone());
        World2 {
            db,
            conns,
            state,
            temple,
            _t: t,
        }
    }

    fn rite() -> Vec<TrialRule> {
        vec![TrialRule {
            quest_id: "rite".into(),
            ..TrialRule::default()
        }]
    }

    /// An eligible worshipper, online, in the temple, with a finished draft.
    fn petitioner(w: &World2, name: &str, spell: &str) -> uuid::Uuid {
        let mut ch = saved(&w.db, name);
        ch.current_room_id = w.temple;
        let mut ws = WorshipState::new(GOD, 0);
        ws.favor = 600;
        ws.last_tribute_day = crate::script::worship::current_absolute_day(&w.db);
        ch.worship = Some(ws);
        ch.skills.insert(
            "magic".into(),
            SkillProgress {
                level: 10,
                experience: 0,
            },
        );
        ch.skills.insert(
            "spellcraft".into(),
            SkillProgress {
                level: 8,
                experience: 0,
            },
        );
        for i in 0..5 {
            ch.spell_progress.insert(
                format!("s{i}"),
                SpellProgress {
                    level: 10,
                    experience: 0,
                },
            );
        }
        w.db.save_character_data(ch.clone()).unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_in, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut s = PlayerSession::new_for_test(tx, tx_in);
        s.character = Some(ch);
        s.olc_mode = Some(petition::MODE.to_string());
        s.spell_draft = Some(SpellDraft {
            god_vnum: GOD.into(),
            step: DraftStep::Review,
            spell_type: Some("damage".into()),
            element: Some("fire".into()),
            target: Some("enemy".into()),
            magnitude: Some(90),
            name: Some(spell.into()),
            description: Some("A lance of noon.".into()),
            ..SpellDraft::default()
        });
        std::mem::forget((rx, rx_in));
        let id = uuid::Uuid::new_v4();
        w.conns.lock().insert(id, s);
        id
    }

    fn live(w: &World2, id: uuid::Uuid) -> CharacterData {
        w.conns.lock()[&id].character.clone().unwrap()
    }

    #[test]
    fn an_accepted_offer_sets_the_trial_and_freezes_the_draft() {
        let w = world(rite());
        let id = petitioner(&w, "Adept", "Sun Lance");
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(out.contains("accepts your petition"), "{out}");
        let ch = live(&w, id);
        assert!(ch.active_quests.contains_key("rite"));
        let p = ch.pending_petition.expect("frozen");
        assert_eq!(p.spell.id, "cx_sun_lance");
        assert_eq!((p.power, p.tier), (90, 3));
        assert_eq!(p.trial_quest_id, "rite");
        assert!(p.spell.scroll_only && p.spell.codex);
        assert!(ch.last_petition_at.is_some());
        {
            let conns = w.conns.lock();
            let s = &conns[&id];
            assert!(s.olc_mode.is_none() && s.spell_draft.is_none(), "mode closed");
        }
        let today = crate::script::worship::current_absolute_day(&w.db);
        assert_eq!(w.db.get_canon(GOD).unwrap().last_petition_day, Some(today));
        // Survives the regen flush (session-first).
        w.db.save_character_data(live(&w, id)).unwrap();
        assert!(
            w.db.get_character_data("adept")
                .unwrap()
                .unwrap()
                .pending_petition
                .is_some()
        );
    }

    #[test]
    fn the_cooldown_claim_is_exclusive_under_concurrent_offers() {
        let w = Arc::new(world(rite()));
        let ids: Vec<_> = (0..4)
            .map(|i| {
                petitioner(
                    &w,
                    &format!("Adept{i}"),
                    &format!("Sun Lance {}", ["Alpha", "Beta", "Gamma", "Delta"][i]),
                )
            })
            .collect();
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let (w, id) = (w.clone(), *id);
                std::thread::spawn(move || petition::offer(&w.db, &w.conns, &w.state, id))
            })
            .collect();
        let outs: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let accepted = outs.iter().filter(|o| o.contains("accepts your petition")).count();
        assert_eq!(accepted, 1, "{outs:#?}");
        let with_trial = ids
            .iter()
            .filter(|id| live(&w, **id).active_quests.contains_key("rite"))
            .count();
        assert_eq!(with_trial, 1, "only the winner was set a trial");
    }

    #[test]
    fn a_full_canon_in_grace_refuses_before_any_trial() {
        let w = world(rite());
        let id = petitioner(&w, "Adept", "Sun Lance");
        let today = crate::script::worship::current_absolute_day(&w.db);
        w.db.update_canon(GOD, |c| {
            c.slots = (0..5)
                .map(|i| CanonSlot {
                    spell_id: format!("cx_{i}"),
                    sealed_day: today,
                    ..Default::default()
                })
                .collect();
            true
        })
        .unwrap();
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(!out.contains("accepts"), "{out}");
        let ch = live(&w, id);
        assert!(ch.active_quests.is_empty() && ch.pending_petition.is_none());
        assert_eq!(w.db.get_canon(GOD).unwrap().last_petition_day, None, "nothing claimed");
    }

    #[test]
    fn no_trial_means_no_petition_and_no_claim() {
        let w = world(vec![]);
        let id = petitioner(&w, "Adept", "Sun Lance");
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(out.contains("has no task for you yet"), "{out}");
        assert_eq!(w.db.get_canon(GOD).unwrap().last_petition_day, None);
        assert!(
            w.conns.lock()[&id].spell_draft.is_some(),
            "draft kept to try again later"
        );
    }

    #[test]
    fn a_refused_trial_releases_the_claim() {
        let w = world(rite());
        let mut q = w.db.get_quest_data("rite").unwrap().unwrap();
        q.repeatable = false;
        w.db.save_quest_data(&q).unwrap();
        let id = petitioner(&w, "Adept", "Sun Lance");
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Adept", |c| {
            c.completed_quests.insert("rite".into())
        });
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(out.contains("already completed"), "{out}");
        assert_eq!(w.db.get_canon(GOD).unwrap().last_petition_day, None, "claim released");
        assert!(live(&w, id).pending_petition.is_none());
    }

    #[test]
    fn a_name_taken_since_drafting_is_refused_at_offer() {
        let w = world(rite());
        let id = petitioner(&w, "Adept", "Sun Lance");
        let taken: crate::SpellDefinition =
            serde_json::from_value(serde_json::json!({"id": "sun_lance", "name": "Sun Lance", "description": ""}))
                .unwrap();
        w.state.lock().spell_definitions.insert("sun_lance".into(), taken);
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(out.contains("too close"), "{out}");
        assert_eq!(w.db.get_canon(GOD).unwrap().last_petition_day, None);
    }

    #[test]
    fn being_attacked_breaks_off_the_petition_and_frees_the_input() {
        let w = world(rite());
        let id = petitioner(&w, "Adept", "Sun Lance");
        assert_eq!(petition::route(&w.conns, id), Some("scripts/commands/pray.rhai"));
        w.conns
            .lock()
            .get_mut(&id)
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .combat
            .in_combat = true;
        assert_eq!(petition::route(&w.conns, id), None, "the line runs as a command");
        let s = &w.conns.lock()[&id];
        assert!(s.olc_mode.is_none() && s.spell_draft.is_none());
    }
}

mod price_tests {
    use super::*;
    use crate::spellcraft::price;
    use crate::types::{DivineMark, EffectType, ItemAffect, MarkState, PledgeAmounts, PledgeKind, PledgeStatus};

    fn sturdy(db: &Db, name: &str) -> CharacterData {
        let mut ch = saved(db, name);
        ch.max_hp = 200;
        ch.hp = 200;
        ch.max_stamina = 100;
        ch.stamina = 100;
        ch.max_mana = 100;
        ch.mana = 100;
        db.save_character_data(ch.clone()).unwrap();
        ch
    }

    fn hp(n: i32) -> PledgeAmounts {
        PledgeAmounts {
            hp: n,
            ..Default::default()
        }
    }

    #[test]
    fn taking_a_pledge_lowers_and_clamps_every_ceiling() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        let amounts = PledgeAmounts {
            hp: 40,
            stamina: 20,
            mana: 10,
            resolve: 0,
        };
        price::take_pledge(&db, &conns, "Ada", "ember", "sol", amounts).expect("pledge");
        let ch = session_char(&conns);
        assert_eq!(ch.effective_max_hp(), 160);
        assert_eq!(ch.hp, 160, "current HP clamped under the new ceiling");
        assert_eq!(ch.effective_max_stamina(), 80);
        assert_eq!(ch.stamina, 80);
        assert_eq!(ch.max_mana, 90, "max mana is stored net of the pledge");
        assert_eq!(ch.mana, 90);
    }

    #[test]
    fn the_floor_holds_across_several_pledges() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        price::take_pledge(&db, &conns, "Ada", "a", "sol", hp(50)).expect("first");
        price::take_pledge(&db, &conns, "Ada", "b", "sol", hp(30)).expect("second: 80 of 200 is at the 40% limit");
        let refused = price::take_pledge(&db, &conns, "Ada", "c", "sol", hp(1));
        assert!(refused.is_err(), "a third would breach the 60% floor");
        let ch = session_char(&conns);
        assert_eq!(ch.effective_max_hp(), 120);
        // Even if the pledges somehow exceed it (e.g. base HP fell later), the
        // floor is applied against the pre-pledge value at read time.
        let mut lowered = ch.clone();
        lowered.max_hp = 100;
        assert_eq!(lowered.effective_max_hp(), 60, "never below 60% of 100");
    }

    #[test]
    fn a_pledge_survives_the_session_flush() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        price::take_pledge(&db, &conns, "Ada", "ember", "sol", hp(40)).expect("pledge");
        // The regen tick saves the session copy over the DB: that copy
        // must already hold the pledge.
        let flushed = session_char(&conns);
        db.save_character_data(flushed).unwrap();
        let back = db.get_character_data("Ada").unwrap().unwrap();
        assert_eq!(back.divine_pledges.len(), 1);
        assert_eq!(back.effective_max_hp(), 160);
    }

    #[test]
    fn release_regrows_and_forfeit_takes_it_back() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        let id = price::take_pledge(&db, &conns, "Ada", "ember", "sol", hp(40)).unwrap();
        price::release_pledge(&db, &conns, "Ada", &id).expect("release");
        let today = crate::script::worship::current_absolute_day(&db);

        let mut ch = session_char(&conns);
        assert_eq!(ch.effective_max_hp(), 160, "nothing grows back on the release day");
        assert!(price::refresh_pledges(&mut ch, today + 7, 14));
        assert_eq!(ch.effective_max_hp(), 180, "half regrown at the midpoint");
        assert!(!price::refresh_pledges(&mut ch, today + 7, 14), "refresh is idempotent");
        assert!(price::refresh_pledges(&mut ch, today + 14, 14));
        assert_eq!(ch.effective_max_hp(), 200);
        assert_eq!(ch.divine_pledges[0].status, PledgeStatus::Repaid);

        let id2 = price::take_pledge(&db, &conns, "Ada", "x", "sol", hp(20)).unwrap();
        price::forfeit_pledge(&db, &conns, "Ada", &id2).expect("forfeit");
        assert!(price::release_pledge(&db, &conns, "Ada", "p99").is_err());
        let ch = session_char(&conns);
        let p2 = ch.divine_pledges.iter().find(|p| p.id == id2).unwrap();
        assert_eq!(p2.status, PledgeStatus::Forfeit);
        assert_eq!(p2.holding(PledgeKind::Hp), 20, "forfeit holds in full");
    }

    #[test]
    fn mana_pledge_survives_a_mana_recalculation() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        let state = World::minimal_shared(db.clone(), conns.clone());
        let before = crate::script::spells::recalculate_max_mana(&db, &conns, &state, "Ada");
        price::take_pledge(
            &db,
            &conns,
            "Ada",
            "x",
            "sol",
            PledgeAmounts {
                mana: 10,
                ..Default::default()
            },
        )
        .unwrap();
        crate::script::spells::recalculate_max_mana(&db, &conns, &state, "Ada");
        assert_eq!(session_char(&conns).max_mana as i64, before - 10);
    }

    #[test]
    fn replicants_pledge_resolve_for_stamina() {
        let (db, _t) = temp_db();
        let mut ch = sturdy(&db, "Roy");
        ch.replicant_state = Some(crate::types::ReplicantState::newly_incepted(0));
        db.save_character_data(ch.clone()).unwrap();
        let conns = online(&ch);
        let full = ch.effective_max_resolve();
        price::take_pledge(
            &db,
            &conns,
            "Roy",
            "x",
            "sol",
            PledgeAmounts {
                stamina: 2,
                ..Default::default()
            },
        )
        .expect("pledge");
        let ch = session_char(&conns);
        assert_eq!(ch.divine_pledges[0].amounts.stamina, 0);
        assert_eq!(ch.divine_pledges[0].amounts.resolve, 2);
        assert_eq!(ch.effective_max_stamina(), 100, "tireless stamina is untouched");
        assert!(ch.effective_max_resolve() < full);
    }

    fn brand() -> DivineMark {
        DivineMark {
            id: String::new(),
            god_vnum: "sol".into(),
            spell_id: Some("ember".into()),
            body_part: "hand".into(),
            short_desc: "a sun-brand across the palm".into(),
            affects: vec![ItemAffect {
                effect_type: EffectType::RegenPenalty,
                magnitude: 20,
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            }],
            worse_affect: None,
            state: MarkState::Live,
        }
    }

    fn mark_buffs(ch: &CharacterData) -> Vec<(EffectType, i32)> {
        ch.active_buffs
            .iter()
            .filter(|b| b.source.starts_with("mark:"))
            .map(|b| (b.effect_type, b.magnitude))
            .collect()
    }

    #[test]
    fn marks_stamp_fade_worsen_and_unstamp_idempotently() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        let id = price::stamp_mark(&db, &conns, "Ada", brand()).expect("stamp");
        assert_eq!(mark_buffs(&session_char(&conns)), vec![(EffectType::RegenPenalty, 20)]);

        price::fade_mark(&db, &conns, "Ada", &id).unwrap();
        price::fade_mark(&db, &conns, "Ada", &id).unwrap();
        let ch = session_char(&conns);
        assert!(mark_buffs(&ch).is_empty(), "a faded mark stamps nothing");
        assert_eq!(
            price::mark_lines(&ch),
            vec!["a sun-brand across the palm, long cooled on their hand"]
        );

        let worse = ItemAffect {
            effect_type: EffectType::Curse,
            ..brand().affects[0].clone()
        };
        price::worsen_mark(&db, &conns, "Ada", &id, worse.clone()).unwrap();
        price::worsen_mark(&db, &conns, "Ada", &id, worse).unwrap();
        assert_eq!(
            mark_buffs(&session_char(&conns)),
            vec![(EffectType::RegenPenalty, 20), (EffectType::Curse, 20)],
            "restamping never doubles a buff"
        );

        price::unstamp_mark(&db, &conns, "Ada", &id).unwrap();
        let ch = session_char(&conns);
        assert!(ch.divine_marks.is_empty());
        assert!(mark_buffs(&ch).is_empty());
    }

    #[test]
    fn admin_commands_drive_the_primitives() {
        let (db, _t) = temp_db();
        let ch = sturdy(&db, "Ada");
        let conns = online(&ch);
        let out = price::admin_pledge(&db, &conns, &["ada", "add", "hp=40", "stamina=10", "god=sol"]);
        assert!(out.contains("p1"), "{out}");
        let list = price::admin_pledge(&db, &conns, &["ada", "list"]);
        assert!(list.contains("Max HP 160 (-40 pledged)"), "{list}");
        assert!(list.contains("Weary"), "{list}");
        assert!(price::admin_pledge(&db, &conns, &["ada", "release", "p1"]).contains("released"));
        assert!(price::admin_pledge(&db, &conns, &["ada", "add", "hp=lots"]).contains("Bad amount"));
        let out = price::admin_mark(
            &db,
            &conns,
            &["ada", "add", "sol", "hand", "regen_penalty:25", "a", "brand"],
        );
        assert!(out.contains("m1"), "{out}");
        assert!(price::admin_mark(&db, &conns, &["ada", "list"]).contains("a brand"));
        assert!(price::admin_mark(&db, &conns, &["ada", "worsen", "m1", "nonsense"]).contains("Unknown effect"));
        assert_eq!(price::admin_mark(&db, &conns, &["ada", "remove", "m1"]), "Done.");
        assert!(price::admin_mark(&db, &conns, &["nobody", "list"]).contains("No character"));
    }
}

mod ritual_tests {
    use super::*;
    use crate::spellcraft::ritual::{self, Backlash, FixedDice, SealStep, SeededDice};
    use crate::spellcraft::{petition, price};
    use crate::types::{
        CanonStatus, DeityConfig, DeitySpellcraft, DraftStep, ItemData, ItemLocation, PledgeKind, PledgeStatus,
        PriceStyle, SkillProgress, SpellDraft, SpellProgress, TrialRule, WorshipState,
    };

    const GOD: &str = "god:sol";

    struct W {
        db: Db,
        conns: SharedConnections,
        state: crate::SharedState,
        temple: uuid::Uuid,
        _t: tempfile::TempDir,
    }

    fn world(style: PriceStyle) -> W {
        let (db, t) = temp_db();
        let mut m = crate::types::MobileData::new("Sol".to_string());
        m.vnum = GOD.to_string();
        m.is_prototype = true;
        m.deity = Some(DeityConfig {
            spellcraft: Some(DeitySpellcraft {
                allowed_types: vec!["damage".into()],
                allowed_elements: vec!["fire".into()],
                allowed_targets: vec!["enemy".into()],
                trial_rules: vec![TrialRule {
                    quest_id: "rite".into(),
                    ..TrialRule::default()
                }],
                price_style: style,
                ..DeitySpellcraft::default()
            }),
            ..DeityConfig::default()
        });
        db.save_mobile_data(m).unwrap();
        let mut q = crate::types::QuestData::new("rite".into(), "Rite of Embers".into());
        q.repeatable = true;
        db.save_quest_data(&q).unwrap();
        let room: crate::types::RoomData = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(), "title": "Temple", "description": "", "exits": {},
            "flags": {"temple": true},
        }))
        .unwrap();
        let temple = room.id;
        db.save_room_data(room).unwrap();
        let conns = no_one_online();
        let state = World::minimal_shared(db.clone(), conns.clone());
        W {
            db,
            conns,
            state,
            temple,
            _t: t,
        }
    }

    fn component(w: &W, owner: &str, category: &str, quality: i32) -> uuid::Uuid {
        let mut item = ItemData::new("shard".into(), "a shard".into(), "A shard lies here.".into());
        item.categories = vec![category.to_string()];
        item.quality = quality;
        item.location = ItemLocation::Inventory(owner.to_string());
        let id = item.id;
        w.db.save_item_data(item).unwrap();
        id
    }

    fn online(w: &W, ch: CharacterData) -> uuid::Uuid {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_in, rx_in) = tokio::sync::mpsc::channel::<crate::InputEvent>(1);
        let mut s = PlayerSession::new_for_test(tx, tx_in);
        s.character = Some(ch);
        std::mem::forget((rx, rx_in));
        let id = uuid::Uuid::new_v4();
        w.conns.lock().insert(id, s);
        id
    }

    /// An author with an accepted petition, the trial done, mana and the
    /// components in hand, standing in the temple.
    fn author(w: &W, name: &str) -> uuid::Uuid {
        author_of(w, name, "Sun Lance")
    }

    fn author_of(w: &W, name: &str, spell: &str) -> uuid::Uuid {
        let mut ch = saved(&w.db, name);
        ch.current_room_id = w.temple;
        ch.max_hp = 200;
        ch.hp = 200;
        ch.max_stamina = 100;
        ch.stamina = 100;
        ch.max_mana = 500;
        ch.mana = 500;
        let mut ws = WorshipState::new(GOD, 0);
        ws.favor = 600;
        ws.last_tribute_day = crate::script::worship::current_absolute_day(&w.db);
        ch.worship = Some(ws);
        ch.skills.insert(
            "magic".into(),
            SkillProgress {
                level: 10,
                experience: 0,
            },
        );
        ch.skills.insert(
            "spellcraft".into(),
            SkillProgress {
                level: 8,
                experience: 0,
            },
        );
        for i in 0..5 {
            ch.spell_progress.insert(
                format!("s{i}"),
                SpellProgress {
                    level: 10,
                    experience: 0,
                },
            );
        }
        w.db.save_character_data(ch.clone()).unwrap();
        let id = online(w, ch);
        {
            let mut conns = w.conns.lock();
            let s = conns.get_mut(&id).unwrap();
            s.olc_mode = Some(petition::MODE.to_string());
            s.spell_draft = Some(SpellDraft {
                god_vnum: GOD.into(),
                step: DraftStep::Review,
                spell_type: Some("damage".into()),
                element: Some("fire".into()),
                target: Some("enemy".into()),
                magnitude: Some(90),
                name: Some(spell.into()),
                description: Some("A lance of noon.".into()),
                ..SpellDraft::default()
            });
        }
        let out = petition::offer(&w.db, &w.conns, &w.state, id);
        assert!(out.contains("accepts your petition"), "{out}");
        // The trial is done.
        crate::script::achievements::apply_to_character(&w.db, &w.conns, name, |c| {
            c.active_quests.remove("rite");
            c.completed_quests.insert("rite".into());
            true
        });
        component(w, name, "essence:fire", 40);
        component(w, name, "relic:god:sol", 0);
        id
    }

    fn live(w: &W, id: uuid::Uuid) -> CharacterData {
        w.conns.lock()[&id].character.clone().unwrap()
    }

    fn settings(w: &W) -> Settings {
        Settings::load(&w.db)
    }

    /// Begin, then run the tick past the first stage to a boundary.
    fn to_boundary(w: &W, name: &str, id: uuid::Uuid, now: i64) -> i64 {
        let out = ritual::begin(&w.db, &w.conns, &w.state, name, now, &mut FixedDice::new(&[]));
        assert!(out.contains("begin the rite"), "{out}");
        let later = now + settings(w).ritual_stage_secs;
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, later, &mut FixedDice::new(&[]));
        assert!(live(w, id).pending_ritual.unwrap().at_boundary);
        later
    }

    #[test]
    fn the_misfire_table_is_fair_with_seeded_dice() {
        let mut dice = SeededDice::new(42);
        let mut counts = [0u32; 4];
        let n = 6000;
        for _ in 0..n {
            let mut ch = CharacterData {
                pending_ritual: Some(sample_ritual()),
                ..saved_free("x")
            };
            ch.max_hp = 100;
            ch.hp = 100;
            match ritual::roll_backlash(&mut ch, &mut dice) {
                Backlash::LoseGain => counts[0] += 1,
                Backlash::Trauma(_) => counts[1] += 1,
                Backlash::ExtraMark => counts[2] += 1,
                Backlash::Collapse => counts[3] += 1,
            }
        }
        let near = |got: u32, want: f64| (got as f64 - want).abs() < want * 0.1;
        assert!(near(counts[0], n as f64 / 3.0), "{counts:?}");
        assert!(near(counts[1], n as f64 / 3.0), "{counts:?}");
        assert!(near(counts[2], n as f64 / 6.0), "{counts:?}");
        assert!(near(counts[3], n as f64 / 6.0), "{counts:?}");
    }

    fn saved_free(name: &str) -> CharacterData {
        serde_json::from_value(serde_json::json!({
            "name": name, "password_hash": "", "current_room_id": uuid::Uuid::new_v4(),
        }))
        .unwrap()
    }

    fn sample_ritual() -> crate::types::RitualState {
        serde_json::from_value(serde_json::json!({
            "god_vnum": GOD, "room_id": uuid::Uuid::new_v4(), "power_bonus": 1,
        }))
        .unwrap()
    }

    #[test]
    fn components_are_claimed_once_and_the_poorest_sufficient_first() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        let fine = component(&w, "Adept", "essence:fire", 90);
        let weak = component(&w, "Adept", "essence:fire", 10);
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        let left: Vec<_> =
            w.db.get_items_in_inventory("adept")
                .unwrap()
                .iter()
                .map(|i| i.id)
                .collect();
        assert!(left.contains(&fine), "the quality-90 shard was spared");
        assert!(left.contains(&weak), "quality 10 is below the 40 needed");
        assert_eq!(left.len(), 2, "the 40 essence and the relic were consumed");
        let r = live(&w, id).pending_ritual.expect("begun");
        assert_eq!(r.claimed_components.len(), 2);
        let again = ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1001, &mut FixedDice::new(&[]));
        assert!(again.contains("already under way"), "{again}");
        assert_eq!(
            w.db.get_items_in_inventory("adept").unwrap().len(),
            2,
            "nothing claimed twice"
        );
    }

    #[test]
    fn missing_components_start_nothing() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        let relic =
            w.db.get_items_in_inventory("adept")
                .unwrap()
                .into_iter()
                .find(|i| i.categories[0].starts_with("relic"))
                .unwrap();
        w.db.delete_item(&relic.id).unwrap();
        let out = ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        assert!(out.contains("relic:god:sol x1"), "{out}");
        assert!(live(&w, id).pending_ritual.is_none());
        assert_eq!(
            w.db.get_items_in_inventory("adept").unwrap().len(),
            1,
            "the essence is untouched"
        );
    }

    #[test]
    fn minor_disturbances_add_at_most_one_risk_per_stage() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        for _ in 0..5 {
            ritual::on_command(&w.db, &w.conns, &w.state, id, "get");
        }
        let later = 1000 + settings(&w).ritual_stage_secs;
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, later, &mut FixedDice::new(&[]));
        let r = live(&w, id).pending_ritual.unwrap();
        assert_eq!(r.risk, 1);
        assert!(!r.disturbed_minor, "paid at the boundary");
    }

    #[test]
    fn the_whitelist_is_free_and_majors_bypass_the_cap() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        for cmd in ["look", "say", "status", "pray", "who"] {
            ritual::on_command(&w.db, &w.conns, &w.state, id, cmd);
        }
        let r = live(&w, id).pending_ritual.unwrap();
        assert!(!r.disturbed_minor && r.risk == 0, "free commands cost nothing");

        // Majors stack and roll the table at once (a 2 = lose the gain).
        let s = settings(&w);
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Adept", |c| {
            ritual::major(c, 0, &s, &mut FixedDice::new(&[2]), "x");
            ritual::major(c, 0, &s, &mut FixedDice::new(&[2]), "x");
            true
        });
        assert_eq!(live(&w, id).pending_ritual.unwrap().risk, 4);
    }

    #[test]
    fn someone_else_speaking_or_walking_in_disturbs_the_ritualist() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        let mut other = saved(&w.db, "Brawler");
        other.current_room_id = w.temple;
        let bid = online(&w, other);
        ritual::on_command(&w.db, &w.conns, &w.state, bid, "look");
        assert!(!live(&w, id).pending_ritual.unwrap().disturbed_minor);
        ritual::on_command(&w.db, &w.conns, &w.state, bid, "say");
        assert!(live(&w, id).pending_ritual.unwrap().disturbed_minor);

        // Clear it, then have Brawler walk out.
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Adept", |c| {
            c.pending_ritual.as_mut().unwrap().disturbed_minor = false;
            true
        });
        let before = ritual::on_command(&w.db, &w.conns, &w.state, bid, "go").unwrap();
        w.conns
            .lock()
            .get_mut(&bid)
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .current_room_id = uuid::Uuid::new_v4();
        ritual::after_command(&w.db, &w.conns, bid, before);
        assert!(live(&w, id).pending_ritual.unwrap().disturbed_minor);
    }

    #[test]
    fn a_push_rolls_risk_dice_and_a_six_on_the_table_collapses_with_half_the_pledge() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        let t = to_boundary(&w, "Adept", id, 1000);
        // risk 1 -> one die: a 1 misfires; table 6 = collapse.
        let out = ritual::push(&w.db, &w.conns, "Adept", t, &mut FixedDice::new(&[1, 6]));
        assert!(out.is_empty(), "{out}");
        let ch = live(&w, id);
        assert!(ch.pending_ritual.is_none() && ch.pending_petition.is_none());
        // Flesh base HP pledge at power 90 is 18 + 9 = 27; one push adds 13;
        // half of 40 is taken, growing back.
        let p = &ch.divine_pledges[0];
        assert_eq!(p.amounts.hp, 20);
        assert!(matches!(p.status, PledgeStatus::Regrowing { .. }));
        assert!(w.db.get_codex("cx_sun_lance").unwrap().is_none());
        let today = crate::script::worship::current_absolute_day(&w.db);
        assert_eq!(
            w.db.get_canon(GOD).unwrap().last_petition_day,
            Some(today),
            "cooldown stays spent"
        );
    }

    #[test]
    fn a_kept_push_empowers_the_sealed_spell_and_the_author_pays() {
        let w = world(PriceStyle::Weariness);
        let id = author(&w, "Adept");
        let t = to_boundary(&w, "Adept", id, 1000);
        ritual::push(&w.db, &w.conns, "Adept", t, &mut FixedDice::new(&[4]));
        let t2 = t + settings(&w).ritual_stage_secs;
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, t2, &mut FixedDice::new(&[]));
        let out = ritual::seal(&w.db, &w.conns, &w.state, "Adept");
        assert!(out.contains("Sun Lance is written into Sol's canon"), "{out}");

        let entry = w.db.get_codex("cx_sun_lance").unwrap().expect("sealed");
        assert_eq!(entry.def.damage_base, 103, "90 + 15%");
        assert_eq!(entry.meta.author.as_deref(), Some("Adept"));
        assert_eq!(entry.meta.status, CanonStatus::Canon);
        assert!(
            w.db.get_canon(GOD)
                .unwrap()
                .slots
                .iter()
                .any(|s| s.spell_id == "cx_sun_lance")
        );
        assert!(w.state.lock().spell_definitions.contains_key("cx_sun_lance"));

        let ch = live(&w, id);
        assert!(ch.learned_spells.contains(&"cx_sun_lance".to_string()));
        assert!(ch.pending_ritual.is_none() && ch.pending_petition.is_none());
        assert_eq!(ch.pledged(PledgeKind::Hp), 18 + 9, "base 18 + one push");
        assert_eq!(ch.pledged(PledgeKind::Stamina), 36, "Weariness");
        assert_eq!(ch.worship.as_ref().unwrap().favor, 600 - 250);
        assert_eq!(ch.divine_marks.len(), 1);
        assert!(
            ch.active_buffs
                .iter()
                .any(|b| b.effect_type == crate::types::EffectType::RegenPenalty),
            "the Weariness mark slows recovery"
        );
        // Survives the session flush.
        w.db.save_character_data(ch).unwrap();
        assert!(
            w.db.get_character_data("adept")
                .unwrap()
                .unwrap()
                .learned_spells
                .contains(&"cx_sun_lance".into())
        );
    }

    #[test]
    fn seal_is_safe_to_rerun_after_a_crash_at_every_step() {
        for stop in [SealStep::Codex, SealStep::Canon, SealStep::Character, SealStep::Done] {
            let w = world(PriceStyle::Flesh);
            let id = author(&w, "Adept");
            to_boundary(&w, "Adept", id, 1000);
            ritual::seal_upto(&w.db, &w.conns, &w.state, "Adept", stop);
            let again = ritual::seal(&w.db, &w.conns, &w.state, "Adept");
            if stop >= SealStep::Character {
                assert!(again.contains("not performing a rite"), "{stop:?}: {again}");
            } else {
                assert!(again.contains("written into"), "{stop:?}: {again}");
            }
            let ch = live(&w, id);
            assert_eq!(w.db.list_codex().unwrap().len(), 1, "{stop:?}: one codex entry");
            assert_eq!(w.db.get_canon(GOD).unwrap().slots.len(), 1, "{stop:?}: one slot");
            assert_eq!(ch.divine_pledges.len(), 1, "{stop:?}: one pledge");
            assert_eq!(ch.divine_marks.len(), 1, "{stop:?}: one mark");
            assert_eq!(ch.worship.as_ref().unwrap().favor, 350, "{stop:?}: favor spent once");
            assert_eq!(ch.learned_spells.iter().filter(|s| *s == "cx_sun_lance").count(), 1);
        }
    }

    #[test]
    fn mana_running_dry_collapses_the_rite() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Adept", |c| {
            c.mana = 0;
            true
        });
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, 1000 + 120, &mut FixedDice::new(&[]));
        let ch = live(&w, id);
        assert!(ch.pending_ritual.is_none() && ch.pending_petition.is_none());
    }

    #[test]
    fn leaving_the_room_or_the_game_too_long_collapses_it() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        let away = 1000 + settings(&w).ritual_linkdead_secs + 1;
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, away, &mut FixedDice::new(&[]));
        assert!(live(&w, id).pending_ritual.is_none(), "linkdead too long");

        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        w.conns
            .lock()
            .get_mut(&id)
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .current_room_id = uuid::Uuid::new_v4();
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, 1001, &mut FixedDice::new(&[]));
        assert!(live(&w, id).pending_ritual.is_none(), "walked out");
    }

    #[test]
    fn fighting_is_a_major_disturbance_once_per_bout() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        w.conns
            .lock()
            .get_mut(&id)
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .combat
            .in_combat = true;
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, 1001, &mut FixedDice::new(&[2]));
        ritual::process_ritual_tick(&w.db, &w.conns, &w.state, 1002, &mut FixedDice::new(&[2]));
        assert_eq!(live(&w, id).pending_ritual.unwrap().risk, 2, "counted once");
    }

    #[test]
    fn aid_from_the_faithful_steadies_and_witnesses_others_give_half() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        ritual::begin(&w.db, &w.conns, &w.state, "Adept", 1000, &mut FixedDice::new(&[]));
        ritual::on_command(&w.db, &w.conns, &w.state, id, "get");

        let mut friend = saved(&w.db, "Friend");
        friend.current_room_id = w.temple;
        friend.mana = 100;
        friend.worship = Some(WorshipState::new(GOD, 0));
        online(&w, friend);
        let out = ritual::aid(&w.db, &w.conns, &w.state, "Friend", "adept", &mut FixedDice::new(&[]));
        assert!(out.contains("witness"), "{out}");
        let r = live(&w, id).pending_ritual.unwrap();
        let give = 25; // stage = half the spell's 50 mana
        assert_eq!(r.mana_pool, give);
        assert!(!r.disturbed_minor, "steadied");
        assert_eq!(r.witnesses, vec!["Friend".to_string()]);
        let again = ritual::aid(&w.db, &w.conns, &w.state, "Friend", "adept", &mut FixedDice::new(&[]));
        assert!(again.contains("already aided"), "{again}");

        let mut stranger = saved(&w.db, "Stranger");
        stranger.current_room_id = w.temple;
        stranger.mana = 100;
        online(&w, stranger);
        ritual::aid(
            &w.db,
            &w.conns,
            &w.state,
            "Stranger",
            "adept",
            &mut FixedDice::new(&[6]),
        );
        let r = live(&w, id).pending_ritual.unwrap();
        assert_eq!(r.mana_pool, give + give / 2);
        assert_eq!(r.witnesses.len(), 1);
    }

    #[test]
    fn the_seal_clamps_a_pledge_that_would_breach_the_floor() {
        let w = world(PriceStyle::Flesh);
        let id = author(&w, "Adept");
        price::take_pledge(
            &w.db,
            &w.conns,
            "Adept",
            "old",
            GOD,
            crate::types::PledgeAmounts {
                hp: 70,
                ..Default::default()
            },
        )
        .unwrap();
        to_boundary(&w, "Adept", id, 1000);
        ritual::seal(&w.db, &w.conns, &w.state, "Adept");
        let ch = live(&w, id);
        assert_eq!(ch.pledged(PledgeKind::Hp), 80, "only 10 more fit above the 60% floor");
        assert_eq!(ch.effective_max_hp(), 120);
    }

    // ---------- S7: canon life ----------

    use crate::spellcraft::canon;
    use crate::types::{Canon, CanonSlot, MarkState};

    fn god_cfg_edit(w: &W, f: impl FnOnce(&mut DeitySpellcraft)) {
        let mut m = w.db.get_mobile_by_vnum(GOD).unwrap().expect("god");
        let d = m.deity.as_mut().unwrap();
        f(d.spellcraft.as_mut().unwrap());
        w.db.save_mobile_data(m).unwrap();
    }

    /// Seal Adept's Sun Lance: no push.
    fn seal_sun_lance(w: &W) -> uuid::Uuid {
        let id = author(w, "Adept");
        to_boundary(w, "Adept", id, 1000);
        let out = ritual::seal(&w.db, &w.conns, &w.state, "Adept");
        assert!(out.contains("written into"), "{out}");
        id
    }

    /// Age every slot past grace and let the god hear petitions again.
    fn age_canon(w: &W) {
        w.db.update_canon(GOD, |c| {
            for s in &mut c.slots {
                s.sealed_day -= 1000;
            }
            c.last_petition_day = None;
            true
        })
        .unwrap();
    }

    fn slot(id: &str, devotion: f64, sealed_day: i64) -> CanonSlot {
        CanonSlot {
            spell_id: id.into(),
            sealed_day,
            devotion,
            warned: false,
            devotion_day: sealed_day,
        }
    }

    #[test]
    fn devotion_halves_every_half_life() {
        assert!((canon::decay(10.0, 15) - 5.0).abs() < 1e-9);
        assert!((canon::decay(10.0, 30) - 2.5).abs() < 1e-9);
        assert_eq!(canon::decay(10.0, 0), 10.0);
        assert_eq!(
            canon::decay(10.0, -4),
            10.0,
            "a clock that ran back does not grow devotion"
        );
    }

    #[test]
    fn devotion_counts_casts_floors_by_learners_and_warns_the_lowest() {
        let mut c = Canon {
            slots: vec![slot("cx_a", 8.0, 100), slot("cx_b", 8.0, 100)],
            ..Canon::default()
        };
        let casts = HashMap::from([("cx_a".to_string(), 3u32)]);
        let known = HashMap::from([("cx_b".to_string(), 20u32)]);
        // 15 days: a decays to 4 then +3; b decays to 4 but 20 learners floor it at 10.
        let warned = canon::apply_devotion(&mut c, &casts, &known, 115, 2);
        assert!((c.slots[0].devotion - 7.0).abs() < 1e-9);
        assert_eq!(c.slots[1].devotion, 10.0);
        assert_eq!(c.influence, 17);
        assert_eq!(warned, vec!["cx_a".to_string()], "full canon: lowest is warned");
        assert!(c.slots[0].warned && !c.slots[1].warned);
        // Already warned: no second omen.
        assert!(canon::apply_devotion(&mut c, &HashMap::new(), &known, 115, 2).is_empty());
        // Room in the canon: nobody is next to go.
        canon::apply_devotion(&mut c, &HashMap::new(), &known, 115, 3);
        assert!(c.slots.iter().all(|s| !s.warned));
    }

    #[test]
    fn displacement_skips_slots_still_in_grace() {
        let c = Canon {
            slots: vec![
                slot("cx_new", 0.0, 95),
                slot("cx_old", 3.0, 10),
                slot("cx_older", 5.0, 0),
            ],
            ..Canon::default()
        };
        assert_eq!(canon::pick_displaced(&c, 100, 30), Some(1), "the new one is protected");
        assert_eq!(canon::pick_displaced(&c, 20, 30), None);
    }

    #[test]
    fn sealing_into_a_full_canon_fades_the_least_devoted_and_frees_its_author() {
        let w = world(PriceStyle::Flesh);
        god_cfg_edit(&w, |c| c.canon_cap = 1);
        let adept = seal_sun_lance(&w);
        assert!(live(&w, adept).pledged(PledgeKind::Hp) > 0);
        age_canon(&w);

        let mira = author_of(&w, "Mira", "Dawn Spear");
        to_boundary(&w, "Mira", mira, 5000);
        let out = ritual::seal(&w.db, &w.conns, &w.state, "Mira");
        assert!(out.contains("Dawn Spear"), "{out}");

        let canon_now = w.db.get_canon(GOD).unwrap();
        assert_eq!(canon_now.slots.len(), 1);
        assert_eq!(canon_now.slots[0].spell_id, "cx_dawn_spear");
        let old = w.db.get_codex("cx_sun_lance").unwrap().unwrap();
        assert_eq!(old.meta.status, CanonStatus::Faded);

        let a = live(&w, adept);
        assert!(
            a.divine_pledges
                .iter()
                .all(|p| matches!(p.status, PledgeStatus::Regrowing { .. })),
            "the displaced author's pledge regrows"
        );
        assert!(a.divine_marks.iter().all(|m| m.state == MarkState::Faded));
    }

    #[test]
    fn an_offline_displaced_author_is_told_by_mail() {
        let w = world(PriceStyle::Flesh);
        god_cfg_edit(&w, |c| c.canon_cap = 1);
        let adept = seal_sun_lance(&w);
        // Adept logs off.
        let ch = live(&w, adept);
        w.db.save_character_data(ch).unwrap();
        w.conns.lock().remove(&adept);
        age_canon(&w);

        let mira = author_of(&w, "Mira", "Dawn Spear");
        to_boundary(&w, "Mira", mira, 5000);
        ritual::seal(&w.db, &w.conns, &w.state, "Mira");

        let stored = w.db.get_character_data("adept").unwrap().unwrap();
        assert!(stored.divine_pledges.iter().all(|p| p.status != PledgeStatus::Active));
        let mail = w.db.get_mail_for_recipient("adept").unwrap();
        assert!(mail.iter().any(|m| m.body.contains("fall from the canon")), "{mail:?}");
    }

    #[test]
    fn restoring_a_faded_spell_relights_it_and_moves_authorship() {
        let w = world(PriceStyle::Flesh);
        god_cfg_edit(&w, |c| {
            c.trial_rules.push(TrialRule {
                quest_id: "rite".into(),
                restoration: true,
                ..TrialRule::default()
            })
        });
        let adept = seal_sun_lance(&w);
        let _ = adept;
        canon::fade_spell(&w.db, &w.conns, &w.state, "cx_sun_lance");
        age_canon(&w);
        w.db.update_canon(GOD, |c| {
            c.slots.clear();
            true
        })
        .unwrap();

        // Mira restores it: she needs gates, the trial and components, as for
        // a petition. `author` gets her through the gates and into the temple.
        let mira = author_of(&w, "Mira", "Dawn Spear");
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Mira", |c| {
            c.pending_petition = None;
            c.last_petition_at = None;
            true
        });
        w.db.update_canon(GOD, |c| {
            c.last_petition_day = None;
            true
        })
        .unwrap();
        let out = canon::restore(&w.db, &w.conns, "Mira", "sun lance");
        assert!(out.contains("will hear you restore Sun Lance"), "{out}");
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Mira", |c| {
            c.active_quests.remove("rite");
            c.completed_quests.insert("rite".into());
            true
        });
        component(&w, "Mira", "essence:fire", 40);
        component(&w, "Mira", "relic:god:sol", 0);
        let t = to_boundary(&w, "Mira", mira, 9000);
        let refused = ritual::push(&w.db, &w.conns, "Mira", t, &mut FixedDice::new(&[4]));
        assert!(refused.contains("cannot be pushed"), "{refused}");
        let out = ritual::seal(&w.db, &w.conns, &w.state, "Mira");
        assert!(out.contains("Sun Lance"), "{out}");

        let e = w.db.get_codex("cx_sun_lance").unwrap().unwrap();
        assert_eq!(e.meta.status, CanonStatus::Canon);
        assert_eq!(e.meta.author.as_deref(), Some("Mira"));
        assert_eq!(e.meta.lineage.len(), 2);
        assert_eq!(e.meta.lineage[1].kind, crate::types::LineageKind::Restored);
        assert!(
            w.db.get_canon(GOD)
                .unwrap()
                .slots
                .iter()
                .any(|s| s.spell_id == "cx_sun_lance")
        );
        assert!(w.db.get_codex("cx_sun_lance_2").unwrap().is_none(), "no new entry");
        assert!(
            w.db.get_codex("cx_dawn_spear").unwrap().is_none(),
            "the abandoned petition wrote nothing"
        );
        assert!(live(&w, mira).learned_spells.contains(&"cx_sun_lance".to_string()));
        // Rerun is harmless.
        assert!(canon::relight(&w.db, &w.state, "Mira", "cx_sun_lance", &[], 0, 99).is_some());
        assert_eq!(w.db.get_codex("cx_sun_lance").unwrap().unwrap().meta.lineage.len(), 2);
    }

    #[test]
    fn restore_needs_a_faded_spell_and_a_restoration_trial() {
        let w = world(PriceStyle::Flesh);
        seal_sun_lance(&w);
        age_canon(&w);
        let mira = author_of(&w, "Mira", "Dawn Spear");
        let _ = mira;
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Mira", |c| {
            c.pending_petition = None;
            c.last_petition_at = None;
            true
        });
        w.db.update_canon(GOD, |c| {
            c.last_petition_day = None;
            true
        })
        .unwrap();
        let out = canon::restore(&w.db, &w.conns, "Mira", "sun lance");
        assert!(out.contains("no faded spell"), "{out}");
        canon::fade_spell(&w.db, &w.conns, &w.state, "cx_sun_lance");
        let out = canon::restore(&w.db, &w.conns, "Mira", "sun lance");
        assert!(out.contains("sets no task"), "{out}");
    }

    #[test]
    fn temple_teaching_shares_canon_spells_only_with_the_favored() {
        let w = world(PriceStyle::Flesh);
        seal_sun_lance(&w);
        let mut ch = saved(&w.db, "Pupil");
        ch.gold = 100;
        ch.skills.insert(
            "magic".into(),
            SkillProgress {
                level: 10,
                experience: 0,
            },
        );
        assert!(
            canon::teach_canon(&w.db, &mut ch, GOD, None, 10, 0).is_err(),
            "not a worshipper"
        );
        let mut ws = WorshipState::new(GOD, 0);
        ws.favor = 50;
        ch.worship = Some(ws);
        assert!(
            canon::teach_canon(&w.db, &mut ch, GOD, None, 10, 0).is_err(),
            "only Noticed"
        );
        ch.worship.as_mut().unwrap().favor = 150;
        assert!(
            canon::teach_canon(&w.db, &mut ch, GOD, None, 500, 0).is_err(),
            "too poor"
        );
        assert_eq!(ch.gold, 100);
        let out = canon::teach_canon(&w.db, &mut ch, GOD, None, 10, 20).expect("taught");
        assert!(out.contains("Sun Lance"), "{out}");
        assert_eq!(ch.gold, 90);
        assert_eq!(ch.worship.as_ref().unwrap().favor, 130);
        assert!(ch.learned_spells.contains(&"cx_sun_lance".to_string()));
        assert!(
            canon::teach_canon(&w.db, &mut ch, GOD, None, 0, 0).is_err(),
            "nothing left"
        );

        let mut other = ch.clone();
        other.learned_spells.clear();
        canon::fade_spell(&w.db, &w.conns, &w.state, "cx_sun_lance");
        let refused = canon::teach_canon(&w.db, &mut other, GOD, Some("Sun Lance"), 0, 0).unwrap_err();
        assert!(refused.contains("faded"), "{refused}");
    }

    #[test]
    fn the_author_casts_the_full_form_and_learners_cap_at_seven() {
        let w = world(PriceStyle::Flesh);
        let adept = seal_sun_lance(&w);
        let _ = adept;
        let a = canon::cast_info(&w.db, "Adept", "cx_sun_lance", "Sun Lance");
        assert!(a.codex && a.true_form);
        assert_eq!(a.seen_as, "Sun Lance, gift of Sol");
        let l = canon::cast_info(&w.db, "Pupil", "cx_sun_lance", "Sun Lance");
        assert!(l.codex && !l.true_form);
        let plain = canon::cast_info(&w.db, "Adept", "firebolt", "Firebolt");
        assert!(!plain.codex && plain.seen_as == "Firebolt");

        assert_eq!(canon::mastery_cap(&w.db, "Adept", "cx_sun_lance"), 10);
        assert_eq!(
            canon::mastery_cap(&w.db, "Pupil", "cx_sun_lance"),
            canon::LEARNER_MASTERY_CAP
        );
        assert_eq!(canon::mastery_cap(&w.db, "Pupil", "firebolt"), 10);

        let mut pupil = saved(&w.db, "Pupil");
        pupil.learned_spells.push("cx_sun_lance".into());
        w.db.save_character_data(pupil).unwrap();
        let out =
            crate::script::spells::add_spell_experience(&w.db, &w.conns, &w.state, "Pupil", "cx_sun_lance", 1_000_000);
        assert_eq!(out.new_level, canon::LEARNER_MASTERY_CAP);
        assert!(!out.mastered);

        // Disgrace takes the full form away.
        w.db.update_codex("cx_sun_lance", |e| e.meta.author_disgraced = true)
            .unwrap();
        assert!(!canon::cast_info(&w.db, "Adept", "cx_sun_lance", "Sun Lance").true_form);
    }

    #[test]
    fn only_the_faithful_add_devotion() {
        let w = world(PriceStyle::Flesh);
        seal_sun_lance(&w);
        canon::note_cast(&w.db, &w.conns, &w.state, "Adept", "cx_sun_lance");
        canon::note_cast(&w.db, &w.conns, &w.state, "Adept", "firebolt");
        let mut heretic = saved(&w.db, "Heretic");
        heretic.worship = Some(WorshipState::new("god:other", 0));
        online(&w, heretic);
        canon::note_cast(&w.db, &w.conns, &w.state, "Heretic", "cx_sun_lance");
        assert_eq!(
            w.state.lock().codex_cast_counts.get("cx_sun_lance").copied(),
            Some(canon::CastTally {
                faithful: 1,
                by_others: 1
            }),
            "the author's own cast is devotion, the heretic's is fame"
        );

        let adept = w
            .conns
            .lock()
            .values()
            .filter_map(|s| s.character.clone())
            .find(|c| c.name == "Adept")
            .unwrap();
        canon::drain_devotion(&w.db, &w.conns, &w.state, &[adept]);
        assert!(w.state.lock().codex_cast_counts.is_empty());
        assert_eq!(w.db.get_codex("cx_sun_lance").unwrap().unwrap().meta.casts_by_others, 1);
        let c = w.db.get_canon(GOD).unwrap();
        assert!(
            (c.slots[0].devotion - 1.0).abs() < 1e-9,
            "one cast; one learner floors at 0.5"
        );
    }

    // ---------- S8: heresy and apostasy ----------

    use crate::spellcraft::heresy;
    use crate::types::Grudge;

    const MOON: &str = "god:moon";

    fn heresy_world() -> W {
        let w = world(PriceStyle::Flesh);
        w.db.set_setting("spellcraft.enabled", "true").unwrap();
        w
    }

    /// A second god; `enemy` makes it Sol's enemy.
    fn second_god(w: &W, enemy: bool) {
        let mut m = crate::types::MobileData::new("Moon".to_string());
        m.vnum = MOON.to_string();
        m.is_prototype = true;
        m.deity = Some(DeityConfig {
            enemy_god_vnums: if enemy { vec![GOD.to_string()] } else { Vec::new() },
            ..DeityConfig::default()
        });
        w.db.save_mobile_data(m).unwrap();
    }

    /// An online character worshipping `god` (or none), standing in the temple.
    fn believer(w: &W, name: &str, god: Option<&str>) -> uuid::Uuid {
        let mut ch = saved(&w.db, name);
        ch.current_room_id = w.temple;
        ch.max_hp = 200;
        ch.hp = 200;
        ch.gold = 1000;
        ch.skills.insert(
            "magic".into(),
            SkillProgress {
                level: 10,
                experience: 0,
            },
        );
        ch.worship = god.map(|g| {
            let mut ws = WorshipState::new(g, 0);
            ws.favor = 300;
            ws
        });
        w.db.save_character_data(ch.clone()).unwrap();
        online(w, ch)
    }

    fn wrath_from(ch: &CharacterData, god: &str) -> bool {
        let src = format!("wrath:{god}");
        ch.active_buffs.iter().any(|b| b.source == src)
    }

    #[test]
    fn the_heresy_tier_matrix() {
        assert_eq!(heresy::tier(Some("a"), "a", false, false), None, "same god");
        assert_eq!(heresy::tier(Some("a"), "", false, false), None, "a godless spell");
        assert_eq!(heresy::tier(None, "a", false, false), Some(1), "godless caster");
        assert_eq!(heresy::tier(Some("b"), "a", false, false), Some(2), "neutral");
        assert_eq!(heresy::tier(Some("b"), "a", true, false), Some(3), "enemies");
        assert_eq!(heresy::tier(None, "a", false, true), Some(2), "godless apostate");
        assert_eq!(
            heresy::tier(Some("b"), "a", true, true),
            Some(4),
            "apostate stacks with enemy"
        );
        assert_eq!(heresy::chance(1, 0), 10);
        assert_eq!(heresy::chance(3, 2), 45);
        assert_eq!(heresy::chance(4, 100), 95, "capped");
        assert_eq!(heresy::backlash(100, 2), 10);
        assert_eq!(heresy::backlash(5, 1), 1, "at least one");
    }

    #[test]
    fn grudges_survive_faith_changes_and_atonement_clears_only_your_own() {
        let w = heresy_world();
        second_god(&w, false);
        let id = believer(&w, "Vex", Some(GOD));
        crate::script::achievements::apply_to_character(&w.db, &w.conns, "Vex", |c| {
            c.divine_grudges.insert(GOD.into(), Grudge { count: 2, last_day: 1 });
            c.divine_grudges.insert(MOON.into(), Grudge { count: 3, last_day: 1 });
            c.active_buffs.push(crate::types::ActiveBuff {
                effect_type: crate::types::EffectType::Curse,
                magnitude: 1,
                remaining_secs: 1000,
                source: format!("wrath:{MOON}"),
                damage_type: None,
                vs_effect: None,
                skill_key: None,
            });
            true
        });

        assert_eq!(crate::script::worship::atone_worship(&w.db, &w.conns, "Vex"), 500);
        let ch = live(&w, id);
        assert!(!ch.divine_grudges.contains_key(GOD), "your god forgives");
        assert_eq!(ch.divine_grudges[MOON].count, 3, "another god does not");
        assert!(wrath_from(&ch, MOON), "nor does its wrath lift");

        assert!(crate::script::worship::clear_worship(&w.db, &w.conns, "Vex"));
        let ch = live(&w, id);
        assert_eq!(ch.divine_grudges[MOON].count, 3);
        assert_eq!(ch.former_faiths.len(), 1);
        assert!(!ch.former_faiths[0].renounced, "an admin clear is not apostasy");
        assert!(!heresy::repact_blocked(&w.db, &ch));

        crate::script::worship::force_worship_pact(&w.db, &w.conns, "Vex", GOD);
        assert_eq!(live(&w, id).divine_grudges[MOON].count, 3);
    }

    #[test]
    fn learning_another_gods_spell_needs_a_timely_confirmation() {
        let w = heresy_world();
        second_god(&w, false);
        seal_sun_lance(&w);
        let id = believer(&w, "Nyx", Some(MOON));
        let learn = |confirmed: bool, now: i64| {
            crate::spellcraft::learn_with(
                &w.db,
                &w.conns,
                &w.state,
                "Nyx",
                "cx_sun_lance",
                crate::spellcraft::LearnVia::Scroll,
                confirmed,
                now,
                &mut FixedDice::new(&[]),
            )
        };
        let warning = heresy::learn_warning(&w.db, &w.conns, "Nyx", "cx_sun_lance");
        assert!(warning.contains("a prayer of Sol"), "{warning}");

        // A blind "confirm" with nothing open only warns.
        assert_eq!(learn(true, 1000), crate::spellcraft::LearnResult::NeedsConfirm);
        // The window lapses.
        assert_eq!(
            learn(true, 1000 + heresy::CONFIRM_SECS + 1),
            crate::spellcraft::LearnResult::NeedsConfirm
        );
        assert!(!live(&w, id).learned_spells.contains(&"cx_sun_lance".to_string()));
        // Warned, then confirmed in time.
        assert_eq!(learn(false, 2000), crate::spellcraft::LearnResult::NeedsConfirm);
        assert_eq!(learn(true, 2000 + 5), crate::spellcraft::LearnResult::Learned);
        let ch = live(&w, id);
        assert!(ch.learned_spells.contains(&"cx_sun_lance".to_string()));
        assert_eq!(ch.divine_grudges[GOD].count, 1, "the learning offence, once");
        assert_eq!(ch.divine_grudges[MOON].count, 1);
        assert!(ch.hp == 200, "no backlash for learning");

        // The spell's own faithful learn freely.
        let sol = believer(&w, "Sun", Some(GOD));
        let _ = sol;
        assert_eq!(
            crate::spellcraft::learn_with(
                &w.db,
                &w.conns,
                &w.state,
                "Sun",
                "cx_sun_lance",
                crate::spellcraft::LearnVia::Scroll,
                false,
                0,
                &mut FixedDice::new(&[]),
            ),
            crate::spellcraft::LearnResult::Learned
        );
    }

    #[test]
    fn a_heretic_cast_always_backlashes_and_both_gods_may_answer() {
        let w = heresy_world();
        second_god(&w, true);
        seal_sun_lance(&w);
        let id = believer(&w, "Nyx", Some(MOON));
        let power = crate::spellcraft::power::power_score(&w.db.get_codex("cx_sun_lance").unwrap().unwrap().def);

        // Rolls miss: only the backlash, at the enemy tier.
        let out = heresy::on_foreign_cast(&w.db, &w.conns, "Nyx", "cx_sun_lance", &mut FixedDice::new(&[]));
        assert!(out.contains("does not want to come for you"), "{out}");
        let ch = live(&w, id);
        assert_eq!(ch.hp, 200 - heresy::backlash(power, 3));
        assert!(!wrath_from(&ch, GOD) && !wrath_from(&ch, MOON));

        // Rolls hit: Moon punishes the heresy, Sol strikes the thief.
        heresy::on_foreign_cast(&w.db, &w.conns, "Nyx", "cx_sun_lance", &mut FixedDice::new(&[1, 1]));
        let ch = live(&w, id);
        assert!(wrath_from(&ch, MOON) && wrath_from(&ch, GOD));
        assert!(ch.worship.as_ref().unwrap().favor < 300, "favor lost");
        assert_eq!(ch.divine_grudges[GOD].count, 2);

        // Your own god's spell is no heresy.
        let sun = believer(&w, "Sun", Some(GOD));
        assert_eq!(
            heresy::on_foreign_cast(&w.db, &w.conns, "Sun", "cx_sun_lance", &mut FixedDice::new(&[1, 1])),
            ""
        );
        assert_eq!(live(&w, sun).hp, 200);
    }

    #[test]
    fn renouncing_a_god_smites_forfeits_worsens_and_disgraces() {
        let w = heresy_world();
        let id = seal_sun_lance(&w);
        let pledged = live(&w, id).pledged(PledgeKind::Hp);
        assert!(pledged > 0);

        // Confirming without the warning first only warns.
        let warn = heresy::renounce(&w.db, &w.conns, &w.state, "Adept", true, 1000);
        assert!(warn.contains("worship renounce confirm"), "{warn}");
        assert!(live(&w, id).worship.is_some());
        // Too late.
        let late = heresy::renounce(
            &w.db,
            &w.conns,
            &w.state,
            "Adept",
            true,
            1000 + heresy::CONFIRM_SECS + 1,
        );
        assert!(late.contains("worship renounce confirm"), "{late}");
        let out = heresy::renounce(&w.db, &w.conns, &w.state, "Adept", true, 1100);
        assert!(out.contains("You renounce Sol"), "{out}");

        let ch = live(&w, id);
        assert!(ch.worship.is_none());
        let f = ch.former_faiths.last().unwrap();
        assert!(f.renounced && f.god_vnum == GOD);
        assert_eq!(f.authored, vec!["cx_sun_lance".to_string()]);
        assert_eq!(ch.divine_grudges[GOD].count, heresy::RENOUNCE_GRUDGE);
        assert!(ch.divine_pledges.iter().all(|p| p.status == PledgeStatus::Forfeit));
        assert_eq!(ch.pledged(PledgeKind::Hp), pledged, "the HP stays gone");
        assert!(ch.divine_marks.iter().all(|m| m.state == MarkState::Worsened));
        assert!(wrath_from(&ch, GOD), "smitten on the spot");
        assert!(heresy::repact_blocked(&w.db, &ch));
        assert_eq!(
            crate::script::worship::create_worship_pact(&w.db, &w.conns, "Adept", GOD),
            "cooldown"
        );

        let e = w.db.get_codex("cx_sun_lance").unwrap().unwrap();
        assert!(e.meta.author_disgraced);
        assert_eq!(e.meta.status, CanonStatus::Canon, "the god keeps the gift");
        assert!(!canon::cast_info(&w.db, "Adept", "cx_sun_lance", "Sun Lance").true_form);

        // Casting it now is apostate heresy: godless 1 + apostate 1.
        let power = crate::spellcraft::power::power_score(&e.def);
        let before = live(&w, id).hp;
        heresy::on_foreign_cast(&w.db, &w.conns, "Adept", "cx_sun_lance", &mut FixedDice::new(&[]));
        let ch = live(&w, id);
        assert_eq!(before - ch.hp, heresy::backlash(power, 2));
        assert_eq!(ch.divine_grudges[GOD].count, heresy::RENOUNCE_GRUDGE + 1);
    }

    #[test]
    fn a_spell_of_a_god_this_world_lacks_is_no_heresy() {
        let w = heresy_world();
        seal_sun_lance(&w);
        w.db.update_codex("cx_sun_lance", |e| e.meta.god_vnum = "god:gone".into())
            .unwrap();
        believer(&w, "Nyx", None);
        assert!(!heresy::is_foreign(&w.db, &w.conns, "Nyx", "cx_sun_lance"));
    }

    // ---------- S9: scribing, fame, the Lost scan ----------

    use crate::spellcraft::{fame, scribe};

    fn live_mut(w: &W, name: &str, f: impl FnOnce(&mut CharacterData)) {
        let mut conns = w.conns.lock();
        let ch = conns
            .values_mut()
            .filter_map(|s| s.character.as_mut())
            .find(|c| c.name == name)
            .expect("online");
        f(ch);
    }

    fn live_named(w: &W, name: &str) -> CharacterData {
        w.conns
            .lock()
            .values()
            .filter_map(|s| s.character.clone())
            .find(|c| c.name == name)
            .expect("online")
    }

    /// Sealed Sun Lance, spellcraft on, Adept flush with mana.
    fn scribe_world() -> W {
        let w = heresy_world();
        seal_sun_lance(&w);
        live_mut(&w, "Adept", |c| c.mana = 500);
        w
    }

    /// A worshipper of Sol who knows Sun Lance, with mana and some skill.
    fn pupil(w: &W, spellcraft: i32, mana: i32) {
        let mut ch = saved(&w.db, "Pupil");
        ch.worship = Some(WorshipState::new(GOD, 0));
        ch.learned_spells.push("cx_sun_lance".into());
        ch.skills.insert(
            "spellcraft".into(),
            SkillProgress {
                level: spellcraft,
                experience: 0,
            },
        );
        ch.mana = mana;
        online(w, ch);
    }

    #[test]
    fn the_author_inks_a_blank_in_place_and_pays_mana() {
        let w = scribe_world();
        let blank = component(&w, "Adept", scribe::BLANK_SCROLL_CATEGORY, 10);
        let cost = w.state.lock().spell_definitions["cx_sun_lance"].mana_cost.max(1);

        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Adept", "sun");
        assert!(out.contains("You ink Sun Lance"), "{out}");
        let scroll = w.db.get_item_data(&blank).unwrap().unwrap();
        assert_eq!(scroll.teaches_spell.as_deref(), Some("cx_sun_lance"));
        assert_eq!(scroll.short_desc, "a scroll inked with Sun Lance");
        assert!(scroll.keywords.contains(&"lance".to_string()));
        assert!(scroll.categories.iter().any(|c| c == scribe::INSCRIBED_CATEGORY));
        assert!(!scroll.categories.iter().any(|c| c == scribe::BLANK_SCROLL_CATEGORY));
        assert_eq!(live_named(&w, "Adept").mana, 500 - cost);
        assert_eq!(
            live_named(&w, "Adept").achievement_counters.get(fame::SCRIBED_COUNTER),
            Some(&1)
        );

        // The scroll is not a blank any more.
        let again = scribe::scribe(&w.db, &w.conns, &w.state, "Adept", "sun lance");
        assert!(again.contains("blank scroll"), "{again}");
    }

    #[test]
    fn others_wait_out_the_authors_monopoly_and_need_skill_and_mana() {
        let w = scribe_world();
        pupil(&w, 1, 500);
        let blank = component(&w, "Pupil", scribe::BLANK_SCROLL_CATEGORY, 10);

        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "sun lance");
        assert!(out.contains("need spellcraft 3"), "{out}");
        live_mut(&w, "Pupil", |c| c.skills.get_mut("spellcraft").unwrap().level = 5);

        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "sun lance");
        assert!(out.contains("still Adept's to write"), "{out}");
        assert!(w.db.get_item_data(&blank).unwrap().unwrap().teaches_spell.is_none());

        w.db.update_codex("cx_sun_lance", |e| e.meta.sealed_day -= 100).unwrap();
        live_mut(&w, "Pupil", |c| c.mana = 0);
        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "sun lance");
        assert!(out.contains("mana"), "{out}");
        assert!(w.db.get_item_data(&blank).unwrap().unwrap().teaches_spell.is_none());

        live_mut(&w, "Pupil", |c| c.mana = 500);
        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "sun lance");
        assert!(out.contains("You ink"), "{out}");

        // Unknown spells and vetoed ones are refused.
        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "firebolt");
        assert!(out.contains("know no spell"), "{out}");
        crate::spellcraft::codex::set_veto(&w.db, &w.state, "cx_sun_lance", true).unwrap();
        component(&w, "Pupil", scribe::BLANK_SCROLL_CATEGORY, 10);
        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Pupil", "sun lance");
        assert!(out.contains("will not stay"), "{out}");
    }

    #[test]
    fn scribing_is_dark_while_spellcraft_is_off() {
        let w = world(PriceStyle::Flesh);
        let out = scribe::scribe(&w.db, &w.conns, &w.state, "Adept", "sun lance");
        assert!(out.contains("don't know how"), "{out}");
    }

    #[test]
    fn milestones_mark_each_threshold_once() {
        assert_eq!(fame::milestone_crossed(99, 100), Some(100));
        assert_eq!(fame::milestone_crossed(100, 150), None);
        assert_eq!(fame::milestone_crossed(50, 1500), Some(1000));
        assert_eq!(fame::milestone_crossed(9_999, 10_000), Some(10_000));
        assert_eq!(fame::milestone_crossed(0, 99), None);
    }

    #[test]
    fn fame_comes_from_lineage_and_others_casts() {
        let mut a = entry("cx_a");
        a.meta.lineage = vec![crate::types::LineageEvent {
            kind: crate::types::LineageKind::Authored,
            who: "Mira".into(),
            day: 1,
        }];
        a.meta.casts_by_others = 40;
        let mut b = entry("cx_b");
        b.meta.author = Some("Tam".into());
        b.meta.lineage = vec![
            crate::types::LineageEvent {
                kind: crate::types::LineageKind::Authored,
                who: "Mira".into(),
                day: 1,
            },
            crate::types::LineageEvent {
                kind: crate::types::LineageKind::Restored,
                who: "Tam".into(),
                day: 9,
            },
        ];
        b.meta.casts_by_others = 7;
        let f = fame::compute(&[a, b]);
        assert_eq!(
            f["mira"],
            fame::AuthorFame {
                authored: 2,
                restored: 0,
                canon_casts: 40
            }
        );
        assert_eq!(
            f["tam"],
            fame::AuthorFame {
                authored: 0,
                restored: 1,
                canon_casts: 7
            },
            "a restorer inherits the spell's fame from then on"
        );
    }

    #[test]
    fn reconcile_writes_fame_counters_and_the_drain_counts_others_casts() {
        let w = heresy_world();
        seal_sun_lance(&w);
        w.db.update_codex("cx_sun_lance", |e| e.meta.casts_by_others = 99)
            .unwrap();
        pupil(&w, 5, 100);
        canon::note_cast(&w.db, &w.conns, &w.state, "Pupil", "cx_sun_lance");
        let chars = vec![live_named(&w, "Adept"), live_named(&w, "Pupil")];
        canon::drain_devotion(&w.db, &w.conns, &w.state, &chars);
        assert_eq!(
            w.db.get_codex("cx_sun_lance").unwrap().unwrap().meta.casts_by_others,
            100
        );

        fame::process(&w.db, &w.conns, &w.state, &chars);
        let adept = live_named(&w, "Adept");
        assert_eq!(adept.achievement_counters.get(fame::CANON_CASTS_COUNTER), Some(&100));
        assert_eq!(adept.achievement_counters.get("spells.authored"), Some(&1));
        assert!(
            !live_named(&w, "Pupil")
                .achievement_counters
                .contains_key(fame::CANON_CASTS_COUNTER)
        );
        assert_eq!(crate::progress::renown(&adept).spells_authored, 1);
    }

    #[test]
    fn only_forgotten_faded_spells_go_lost() {
        let faded = |id: &str| {
            let mut e = entry(id);
            e.meta.status = CanonStatus::Faded;
            e
        };
        let mut canon_spell = entry("cx_canon");
        canon_spell.meta.status = CanonStatus::Canon;
        let entries = vec![faded("cx_known"), faded("cx_written"), faded("cx_gone"), canon_spell];
        let mut knower = saved_free("Knower");
        knower.learned_spells.push("cx_known".into());
        let mut scroll = ItemData::new("scroll".into(), "a scroll".into(), "".into());
        scroll.teaches_spell = Some("cx_written".into());
        assert_eq!(
            fame::lost_candidates(&entries, &[knower], &[scroll]),
            vec!["cx_gone".to_string()]
        );
    }
}
