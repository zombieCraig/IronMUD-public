//! Content auditor integration tests.
//!
//! The unit tests inside `src/audit/mod.rs` pin each check against a
//! hand-built entity. These run the auditor against a real database — the
//! demo world the repo actually ships — because the point of the auditor is
//! not that it can be made to fire, it is that it fires on the content in
//! front of it.
//!
//! The calibration tests at the bottom are the important ones. A grading
//! scale that can be satisfied by volume is worse than no scale, so "a
//! hundred empty rooms score below five good ones" is pinned here rather than
//! left to judgement.

#![recursion_limit = "256"]

use ironmud::audit::scan::WorldSnapshot;
use ironmud::audit::{self, AuditCtx, EntityKind, Severity, audit_room};
use ironmud::db::Db;
use ironmud::types::RoomData;
use serde_json::json;
use uuid::Uuid;

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn seeded_db() -> (Db, tempfile::TempDir) {
    let (db, temp) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed demo world");
    (db, temp)
}

fn room(patch: serde_json::Value) -> RoomData {
    let mut base = json!({
        "id": Uuid::new_v4(),
        "title": "A Rutted Track",
        "description": "Cart ruts run the length of the track, filled with brown water that \
                        has not drained since the last rain and will not before the next.",
        "exits": {},
    });
    let obj = base.as_object_mut().unwrap();
    for (k, v) in patch.as_object().unwrap() {
        obj.insert(k.clone(), v.clone());
    }
    serde_json::from_value(base).expect("room fixture")
}

// ===========================================================================
// Against the world the repo ships
// ===========================================================================

#[test]
fn the_demo_world_loads_into_a_snapshot() {
    let (db, _t) = seeded_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    assert!(snap.areas.len() >= 5, "expected the five demo areas");
    assert!(snap.rooms.len() >= 50, "expected ~56 demo rooms");
    assert!(!snap.mobiles.is_empty());
    // Prototypes only: instances are spawned content, not authored content.
    assert!(snap.mobiles.iter().all(|m| m.is_prototype));
    assert!(snap.items.iter().all(|i| i.is_prototype));
}

#[test]
fn the_demo_world_audit_finds_the_gaps_we_know_about() {
    let (db, _t) = seeded_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let report = ironmud::audit::scan::scan_world(&snap);

    // The demo world ships no quests and no bulletin boards. If the auditor
    // comes back clean on it, the auditor is not earning its keep.
    assert!(
        report.own.has("world.no_quests"),
        "demo world has no quests and the audit did not say so: {:?}",
        report.own.findings
    );
    assert!(report.own.has("world.no_boards"));
    assert!(
        report.own.count(Severity::Blocker) > 0,
        "expected at least one world-level blocker"
    );
    assert!(report.score < 100);
}

#[test]
fn every_demo_area_gets_a_grade_and_a_letter_from_the_one_table() {
    let (db, _t) = seeded_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let ctx = snap.ctx();
    for area in &snap.areas {
        let report = ironmud::audit::scan::scan_area(&snap, area, &ctx);
        assert!(
            (0..=100).contains(&report.score),
            "{} scored {}",
            area.name,
            report.score
        );
        assert_eq!(
            report.letter,
            audit::letter_for(report.score),
            "{} letter disagrees with the table",
            area.name
        );
        assert!(report.count_of(EntityKind::Room) > 0, "{} has no rooms", area.name);
    }
}

#[test]
fn a_world_context_does_not_call_cross_area_exits_dangling() {
    // The trap this guards: building the context from one area's rooms makes
    // every exit out of that area look like a hole in the world.
    let (db, _t) = seeded_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let ctx = snap.ctx();
    let dangling = snap
        .rooms
        .iter()
        .filter(|r| audit_room(r, &ctx).has("room.dangling_exit"))
        .count();
    assert_eq!(dangling, 0, "the shipped demo world should have no broken exits");
}

#[test]
fn scan_entity_finds_a_seeded_room_by_vnum_and_misses_on_nonsense() {
    let (db, _t) = seeded_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let vnum = snap
        .rooms
        .iter()
        .find_map(|r| r.vnum.clone())
        .expect("demo rooms carry vnums");

    let hit = ironmud::audit::scan::scan_entity(&db, EntityKind::Room, &vnum).expect("scan");
    assert!(hit.is_some(), "vnum {vnum} did not resolve");

    let miss = ironmud::audit::scan::scan_entity(&db, EntityKind::Room, "no-such-room").expect("scan");
    assert!(miss.is_none(), "a room that does not exist must not be graded");
}

#[test]
fn an_empty_world_is_blocked_rather_than_perfect() {
    // Nothing to grade must not read as nothing wrong. This is the degenerate
    // case a rollup average gets wrong if the container's own findings are
    // ignored when there are no children.
    let (db, _t) = fresh_db();
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let report = ironmud::audit::scan::scan_world(&snap);
    assert!(report.own.has("world.empty"));
    assert_eq!(report.letter, 'F');
}

// ===========================================================================
// The command, driven for real
// ===========================================================================

/// Run `build <args>` through the actual Rhai script and return what the
/// player would have seen.
///
/// `test_all_scripts_compile` proves the script parses and
/// `test_scripts_call_registered_functions` proves it only calls things that
/// exist — neither proves it runs. The rendering helpers here index maps,
/// iterate `counts.keys()` and concatenate coloured columns, and every one of
/// those is a runtime failure the static analyzers cannot see.
fn run_build_command(db: &Db, args: &str) -> String {
    use ironmud::{PlayerSession, World};
    use std::sync::{Arc, Mutex};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    // Stand the builder in a real room so `build audit` with no argument has
    // something to grade.
    let first_room = db
        .list_all_rooms()
        .expect("rooms")
        .into_iter()
        .next()
        .map(|r| r.id)
        .unwrap_or_else(Uuid::new_v4);
    let character: ironmud::types::CharacterData = serde_json::from_value(json!({
        "name": "Auditor",
        "password_hash": "",
        "is_builder": true,
        "current_room_id": first_room,
    }))
    .expect("character");
    session.character = Some(character);
    connections.lock().unwrap().insert(conn_id, session);

    // A FREE-STANDING engine, deliberately not `world.engine`.
    //
    // Bindings like `get_world_rating` lock `SharedState`, and
    // `std::sync::Mutex` is not reentrant — running a script while holding the
    // World lock deadlocks. The server avoids this by cloning the AST and
    // dropping the lock before `call_fn`; a test is simpler if it never takes
    // the lock at all.
    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    ironmud::script::register_rhai_functions(&mut engine, Arc::new(db.clone()), connections.clone(), state.clone());
    let ast = engine
        .compile_file(format!("scripts/commands/build.rhai").into())
        .unwrap_or_else(|e| panic!("compile build.rhai: {e}"));

    let mut scope = rhai::Scope::new();
    let result: Result<(), _> =
        engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    result.unwrap_or_else(|e| panic!("build.rhai run_command: {e}"));

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn build_audit_world_renders_against_the_demo_world() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit world");
    assert!(out.contains("World Audit"), "unexpected output:\n{out}");
    assert!(out.contains("world.no_quests"), "the world finding is missing:\n{out}");
    assert!(out.contains("Oakvale"), "the area table is missing:\n{out}");
}

#[test]
fn build_audit_area_renders_contents_and_worst_first() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit area Oakvale Village");
    assert!(out.contains("Contents"), "unexpected output:\n{out}");
    assert!(out.contains("room"), "the contents table is missing:\n{out}");
}

#[test]
fn build_audit_with_no_argument_grades_the_room_you_are_in() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit");
    assert!(out.contains("(room "), "expected a room heading:\n{out}");
}

#[test]
fn build_audit_on_a_missing_vnum_says_so_instead_of_grading_nothing() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit room no-such-vnum");
    assert!(out.contains("No room matches"), "unexpected output:\n{out}");
}

#[test]
fn an_unknown_subcommand_prints_the_usage() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "wibble");
    assert!(out.contains("build audit world"), "unexpected output:\n{out}");
}

// ===========================================================================
// Calibration — the scale must not be satisfiable by volume
// ===========================================================================

#[test]
fn a_hundred_empty_rooms_score_below_five_good_ones() {
    let ctx = AuditCtx::empty();

    let empty: Vec<RoomData> = (0..100)
        .map(|_| room(json!({"title": "", "description": ""})))
        .collect();
    let empty_mean: i32 = empty.iter().map(|r| audit_room(r, &ctx).score).sum::<i32>() / empty.len() as i32;

    let good: Vec<RoomData> = (0..5)
        .map(|i| {
            room(json!({
                "exits": {"north": Uuid::new_v4()},
                "flags": {"city": true},
                "extra_descs": [{"keywords": ["ruts"], "description": "Deep and water-filled."}],
                "spring_desc": format!("Mud, {i} deep."),
            }))
        })
        .collect();
    let good_mean: i32 = good.iter().map(|r| audit_room(r, &ctx).score).sum::<i32>() / good.len() as i32;

    assert!(
        good_mean > empty_mean,
        "five good rooms ({good_mean}) did not beat a hundred empty ones ({empty_mean})"
    );
    assert!(
        empty_mean < 45,
        "an empty room graded {empty_mean} — the scale rewards existing"
    );
}

#[test]
fn copying_one_room_a_hundred_times_is_caught_every_time() {
    // `rcopy` is the fastest way to inflate a room count, so the duplicate
    // check has to fire on the copies rather than only on the pair.
    let template = room(json!({"exits": {"north": Uuid::new_v4()}}));
    let copies: Vec<RoomData> = (0..100)
        .map(|_| {
            let mut c = template.clone();
            c.id = Uuid::new_v4();
            c
        })
        .collect();
    let ctx = AuditCtx::build(&copies);
    let flagged = copies
        .iter()
        .filter(|r| audit_room(r, &ctx).has("room.duplicate_desc"))
        .count();
    assert_eq!(flagged, 100);
}

#[test]
fn fixing_a_finding_always_raises_the_score() {
    // The direction of the scale, pinned once. A change that fixes something
    // and lowers the grade is a bug in the weights.
    let ctx = AuditCtx::empty();
    let before = audit_room(&room(json!({"description": ""})), &ctx).score;
    let after = audit_room(&room(json!({})), &ctx).score;
    assert!(after > before, "before {before}, after {after}");
}

#[test]
fn severity_weights_stay_ordered() {
    assert!(Severity::Blocker.weight() > Severity::Warn.weight());
    assert!(Severity::Warn.weight() > Severity::Polish.weight());
    // Two blockers must land an entity on F, or "broken" is survivable. The
    // check is on the letter rather than the raw score because the letter is
    // what every consumer reads.
    assert_eq!(
        audit::letter_for(100 - Severity::Blocker.weight() * 2),
        'F',
        "two blockers no longer floor an entity"
    );
    // And one blocker alone must not — a single flaw is a D, not a write-off.
    assert_ne!(audit::letter_for(100 - Severity::Blocker.weight()), 'F');
}
