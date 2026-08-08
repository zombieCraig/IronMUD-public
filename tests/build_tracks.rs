//! Progress-track integration tests.
//!
//! The predicate logic is unit-tested in `src/build_tracks.rs`. These check the
//! two things that only show up against real content: that the shipped track
//! JSON parses and means what it says, and that `build next` actually points
//! somewhere useful instead of at a wall.

#![recursion_limit = "256"]

use std::sync::{Arc, Mutex};

use ironmud::audit::scan::WorldSnapshot;
use ironmud::build_tracks::{self, Predicate, TrackDef, TrackScope};
use ironmud::db::Db;
use ironmud::types::{ContentKind, ContentOrigin, MobileData, RoomData};
use serde_json::json;
use uuid::Uuid;

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn shipped_tracks() -> Vec<TrackDef> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("scripts/data/build_tracks").expect("track dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read track");
        out.push(serde_json::from_str(&content).unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display())));
    }
    out.sort_by(|a: &TrackDef, b: &TrackDef| a.key.cmp(&b.key));
    out
}

fn area(db: &Db, prefix: &str, author: &str) -> Uuid {
    let id = Uuid::new_v4();
    let mut a: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": id, "name": prefix, "prefix": prefix})).unwrap();
    a.authored_by = Some(author.to_string());
    a.origin = ContentOrigin::Builder;
    db.save_area_data(a).unwrap();
    id
}

fn room(db: &Db, area_id: Uuid, author: &str, patch: serde_json::Value) -> Uuid {
    let id = Uuid::new_v4();
    let mut base = json!({
        "id": id,
        "title": "A Rutted Track",
        "description": "Cart ruts run the length of the track, filled with brown water that has \
                        not drained since the last rain and will not before the next.",
        "exits": {},
        "area_id": area_id,
    });
    for (k, v) in patch.as_object().unwrap() {
        base.as_object_mut().unwrap().insert(k.clone(), v.clone());
    }
    let mut r: RoomData = serde_json::from_value(base).unwrap();
    r.authored_by = Some(author.to_string());
    r.origin = ContentOrigin::Builder;
    db.save_room_data(r).unwrap();
    id
}

fn snapshot(db: &Db) -> WorldSnapshot {
    WorldSnapshot::load(db).expect("snapshot")
}

// ===========================================================================
// The shipped tracks
// ===========================================================================

#[test]
fn the_shipped_tracks_parse_and_cover_both_scopes() {
    let tracks = shipped_tracks();
    assert!(tracks.len() >= 2, "expected at least the two shipped tracks");
    assert!(tracks.iter().any(|t| t.scope == TrackScope::Area));
    assert!(tracks.iter().any(|t| t.scope == TrackScope::Builder));
    for t in &tracks {
        assert!(!t.steps.is_empty(), "{} has no steps", t.key);
        assert!(!t.name.is_empty());
        for s in &t.steps {
            assert!(!s.label.is_empty(), "{}/{} has no label", t.key, s.key);
        }
    }
}

#[test]
fn every_has_system_step_names_a_system_the_engine_can_report() {
    // A step naming a system `systems_used` never emits is a step nobody can
    // ever tick. Nothing else would catch it — the JSON is valid, the track
    // loads, and the box simply stays empty forever.
    let vocabulary = build_tracks::systems_used(
        &[&reference_room()],
        &[&reference_item()],
        &[&reference_mobile()],
        &[&reference_quest()],
        &[&reference_area()],
    );

    for t in shipped_tracks() {
        for s in &t.steps {
            if let Predicate::HasSystem { system } = &s.predicate {
                assert!(
                    vocabulary.contains(system),
                    "{}/{} asks for system '{system}', which systems_used never reports",
                    t.key,
                    s.key
                );
            }
        }
    }
}

#[test]
fn every_no_finding_step_names_a_code_the_auditor_emits() {
    // Same failure in the other direction: a step waiting on a finding code
    // that does not exist is permanently satisfied, and silently.
    let known = audit_codes_in_source();
    for t in shipped_tracks() {
        for s in &t.steps {
            if let Predicate::NoFinding { code } = &s.predicate {
                assert!(
                    known.contains(code),
                    "{}/{} waits on finding '{code}', which src/audit/mod.rs never emits",
                    t.key,
                    s.key
                );
            }
        }
    }
}

/// Every finding code that appears as a string literal in the auditor.
fn audit_codes_in_source() -> std::collections::HashSet<String> {
    let src = std::fs::read_to_string("src/audit/mod.rs").expect("read auditor");
    let mut out = std::collections::HashSet::new();
    for part in src.split('"').skip(1).step_by(2) {
        let looks_like_code = part.contains('.')
            && !part.contains(' ')
            && part.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_');
        if looks_like_code {
            out.insert(part.to_string());
        }
    }
    out
}

// Reference entities that exercise every system key at once.
fn reference_room() -> RoomData {
    serde_json::from_value(json!({
        "id": Uuid::new_v4(), "title": "t", "description": "d", "exits": {},
        "extra_descs": [{"keywords": ["x"], "description": "y"}],
        "spring_desc": "s",
        "traps": [{"trap_type": "spike", "owner_name": "a", "damage": 1, "detect_difficulty": 1,
                   "disarm_difficulty": 1, "charges": 1, "effect": "damage", "placed_at": 0}],
        "contextual_commands": [{"verb": "pull"}],
        "doors": {"north": {"name": "door", "is_closed": false, "is_locked": false, "description": null}},
        "exit_delays": {"north": 3},
        "catch_table": [{"vnum": "fish", "weight": 1, "min_skill": 0, "rarity": "common"}],
    }))
    .unwrap()
}

fn reference_item() -> ironmud::types::ItemData {
    serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "n", "short_desc": "s", "long_desc": "l",
        "item_type": "container",
        "extra_descs": [{"keywords": ["x"], "description": "y"}],
        "affects": [{"effect_type": "strength_boost", "magnitude": 1}],
    }))
    .unwrap()
}

fn reference_mobile() -> MobileData {
    let mut m: MobileData = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "n", "short_desc": "s", "long_desc": "l",
        "dialogue": {"hello": "hi"},
        "daily_routine": [{"start_hour": 8, "activity": "working"}],
        "faction": "guild",
        "spoken_language": "trade",
        "alignment": -20,
        "combat_spells": ["frost_bolt"],
        "flags": {"shopkeeper": true, "healer": true},
        "shop_stock": ["bread"],
    }))
    .unwrap();
    m.dialogue_tree = Some(Default::default());
    m
}

fn reference_quest() -> ironmud::types::QuestData {
    serde_json::from_value(json!({
        "vnum": "q", "name": "n", "summary": "s", "description": "", "completion_text": "",
        "objectives": [
            {"kind": "visit_room", "vnum": "a"},
            {"kind": "visit_room", "vnum": "b"}
        ],
        "rewards": [],
        "prereq_quest_vnum": "p",
    }))
    .unwrap()
}

fn reference_area() -> ironmud::types::AreaData {
    serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "n", "prefix": "p",
        "level_min": 1, "level_max": 5,
        "immigration_enabled": true,
        "wilderness_forage_table": [{"vnum": "herb", "min_skill": 0, "rarity": "common"}],
    }))
    .unwrap()
}

// ===========================================================================
// Evaluation against real content
// ===========================================================================

#[test]
fn a_bare_area_completes_almost_nothing() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "bare", "Ana");
    room(&db, area_id, "Ana", json!({}));

    let snap = snapshot(&db);
    let ctx = snap.ctx();
    let a = snap.areas[0].clone();
    let facts = build_tracks::area_facts(&snap, &a, &ctx);

    let readiness = shipped_tracks()
        .into_iter()
        .find(|t| t.key == "area_readiness")
        .expect("area_readiness ships");
    let p = build_tracks::evaluate(&readiness, &facts);
    assert!(!p.complete());
    assert!(p.done() < p.total() / 2, "a one-room area ticked {} steps", p.done());
    assert!(p.next_step().is_some());
}

#[test]
fn the_builders_path_ticks_as_a_builder_uses_systems() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({}));

    let path = shipped_tracks()
        .into_iter()
        .find(|t| t.key == "builders_path")
        .expect("builders_path ships");

    let before = {
        let snap = snapshot(&db);
        let ctx = snap.ctx();
        build_tracks::evaluate(&path, &build_tracks::builder_facts(&snap, &ctx, "Ana")).done()
    };

    // Add an extra description — one specific step, nothing else.
    room(
        &db,
        area_id,
        "Ana",
        json!({"extra_descs": [{"keywords": ["ruts"], "description": "Deep, and full of water."}]}),
    );

    let after = {
        let snap = snapshot(&db);
        let ctx = snap.ctx();
        build_tracks::evaluate(&path, &build_tracks::builder_facts(&snap, &ctx, "Ana"))
    };
    assert_eq!(after.done(), before + 1, "using a system did not tick exactly one step");
    assert!(
        after.steps.iter().any(|s| s.key == "extra_desc" && s.done),
        "the extra-description step did not tick"
    );
}

#[test]
fn another_builders_work_does_not_tick_your_path() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Bo");
    room(
        &db,
        area_id,
        "Bo",
        json!({"extra_descs": [{"keywords": ["ruts"], "description": "Deep."}]}),
    );

    let snap = snapshot(&db);
    let ctx = snap.ctx();
    let facts = build_tracks::builder_facts(&snap, &ctx, "Ana");
    assert_eq!(facts.counts.get(&ContentKind::Room).copied().unwrap_or(0), 0);
    assert!(!facts.systems.contains("extra_desc"));
}

#[test]
fn seeded_content_does_not_tick_anybodys_path() {
    // The provenance guard, one layer up: importing a world must not complete
    // the tutorial for whoever ran the import.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let snap = snapshot(&db);
    let ctx = snap.ctx();

    for name in ["Ana", "Bo"] {
        let facts = build_tracks::builder_facts(&snap, &ctx, name);
        assert!(facts.systems.is_empty(), "{name} inherited systems from seed content");
        assert_eq!(facts.counts.values().sum::<usize>(), 0);
    }
}

#[test]
fn an_area_track_sees_the_area_itself() {
    // `count of area min 1` inside an area scope has to mean "you have one",
    // or an area-scoped step about areas can never be satisfied.
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({}));
    let snap = snapshot(&db);
    let ctx = snap.ctx();
    let facts = build_tracks::area_facts(&snap, &snap.areas[0].clone(), &ctx);
    assert_eq!(facts.counts.get(&ContentKind::Area).copied(), Some(1));
}

// ===========================================================================
// The command
// ===========================================================================

fn run_build(db: &Db, args: &str, builder: &str) -> String {
    use ironmud::{PlayerSession, World};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());
    {
        let mut world = state.lock().unwrap();
        world.build_tracks = shipped_tracks();
    }

    let room_id = db
        .list_all_rooms()
        .unwrap()
        .into_iter()
        .next()
        .map(|r| r.id)
        .unwrap_or_else(Uuid::new_v4);
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": builder, "password_hash": "", "is_builder": true, "current_room_id": room_id,
        }))
        .unwrap(),
    );
    connections.lock().unwrap().insert(conn_id, session);

    // Free-standing engine: the bindings lock SharedState, and holding the
    // World lock across a script call deadlocks a non-reentrant mutex.
    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    ironmud::script::register_rhai_functions(&mut engine, Arc::new(db.clone()), connections.clone(), state.clone());
    let ast = engine
        .compile_file("scripts/commands/build.rhai".into())
        .expect("compile build.rhai");

    let mut scope = rhai::Scope::new();
    let r: Result<(), _> = engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    r.expect("build.rhai run_command");

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn build_track_renders_the_builders_path_with_hints() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({}));

    let out = run_build(&db, "track", "Ana");
    assert!(out.contains("The Builder's Path"), "unexpected output:\n{out}");
    assert!(out.contains("[x]"), "no completed steps shown:\n{out}");
    assert!(out.contains("[ ]"), "no outstanding steps shown:\n{out}");
    assert!(out.contains("medit create"), "hints are missing:\n{out}");
}

#[test]
fn build_track_on_an_area_renders_its_readiness() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({}));

    let out = run_build(&db, "track home", "Ana");
    assert!(out.contains("Area Readiness"), "unexpected output:\n{out}");
    assert!(out.contains("Twenty rooms"), "steps missing:\n{out}");
}

#[test]
fn build_next_points_somewhere_specific() {
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({}));

    let out = run_build(&db, "next", "Ana");
    assert!(out.contains("Next"), "unexpected output:\n{out}");
    assert!(
        out.contains("Area Readiness") || out.contains("Builder's Path"),
        "next said nothing about either track:\n{out}"
    );
    // It has to name an actual action, not just a category.
    assert!(out.contains("`"), "no concrete command suggested:\n{out}");
}

#[test]
fn build_next_leads_with_broken_work_when_there_is_any() {
    // A blocker beats a checklist step: it is already in the world and a
    // player can already walk into it.
    let (db, _t) = fresh_db();
    let area_id = area(&db, "home", "Ana");
    room(&db, area_id, "Ana", json!({"title": "", "description": ""}));

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
    ironmud::build_score::process_build_score_tick(&db, &connections, &state, 100).unwrap();

    // Re-run through the command's own state, which needs the same scan.
    let out = run_build_with_scan(&db, "next", "Ana");
    assert!(out.contains("blocker"), "broken work was not led with:\n{out}");
}

fn run_build_with_scan(db: &Db, args: &str, builder: &str) -> String {
    use ironmud::{PlayerSession, World};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());
    {
        let mut world = state.lock().unwrap();
        world.build_tracks = shipped_tracks();
    }
    ironmud::build_score::process_build_score_tick(db, &connections, &state, 100).unwrap();

    let room_id = db
        .list_all_rooms()
        .unwrap()
        .into_iter()
        .next()
        .map(|r| r.id)
        .unwrap_or_else(Uuid::new_v4);
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": builder, "password_hash": "", "is_builder": true, "current_room_id": room_id,
        }))
        .unwrap(),
    );
    connections.lock().unwrap().insert(conn_id, session);

    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    ironmud::script::register_rhai_functions(&mut engine, Arc::new(db.clone()), connections.clone(), state.clone());
    let ast = engine
        .compile_file("scripts/commands/build.rhai".into())
        .expect("compile build.rhai");
    let mut scope = rhai::Scope::new();
    let r: Result<(), _> = engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    r.expect("build.rhai run_command");

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}
