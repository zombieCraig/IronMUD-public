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
use ironmud::types::{ItemData, ItemType, RoomData};
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
fn build_audit_room_with_no_vnum_grades_the_room_you_are_in() {
    // `build audit` already defaults here; `build audit room` naming the kind
    // and nothing else used to answer "Which room?" while standing in one.
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit room");
    assert!(out.contains("(room "), "expected a room heading:\n{out}");
    assert!(!out.contains("Which room"), "the kind-only form should not ask:\n{out}");
}

/// The room `run_build_command` stands its builder in.
fn harness_room(db: &Db) -> Uuid {
    db.list_all_rooms()
        .expect("rooms")
        .into_iter()
        .next()
        .expect("a room")
        .id
}

#[test]
fn build_audit_mob_takes_a_keyword_from_the_room_and_grades_the_prototype() {
    let (db, _t) = seeded_db();
    let room_id = harness_room(&db);

    let proto = db
        .list_all_mobiles()
        .expect("mobiles")
        .into_iter()
        .find(|m| m.is_prototype && !m.vnum.is_empty())
        .expect("the demo world ships mobile prototypes");

    let mut instance = proto.clone();
    instance.id = Uuid::new_v4();
    instance.is_prototype = false;
    instance.current_room_id = Some(room_id);
    instance.keywords = vec!["auditbait".to_string()];
    db.save_mobile_data(instance).expect("save instance");

    let out = run_build_command(&db, "audit mob auditbait");
    // The heading carries the prototype's vnum: an instance has no quality of
    // its own, so resolving a keyword must climb to what a builder can fix.
    assert!(
        out.contains(&format!("(mobile {})", proto.vnum)),
        "expected the prototype heading for {}:\n{out}",
        proto.vnum
    );
}

#[test]
fn build_audit_item_takes_a_keyword_from_the_room() {
    let (db, _t) = seeded_db();
    let room_id = harness_room(&db);

    let proto = db
        .list_all_items()
        .expect("items")
        .into_iter()
        .find(|i| i.is_prototype && i.vnum.is_some())
        .expect("the demo world ships item prototypes");
    let vnum = proto.vnum.clone().expect("filtered on Some");

    let mut instance = proto.clone();
    instance.id = Uuid::new_v4();
    instance.is_prototype = false;
    instance.location = ironmud::types::ItemLocation::Room(room_id);
    instance.keywords = vec!["auditbait".to_string()];
    db.save_item_data(instance).expect("save instance");

    let out = run_build_command(&db, "audit item auditbait");
    assert!(
        out.contains(&format!("(item {vnum})")),
        "expected the prototype heading for {vnum}:\n{out}"
    );
}

#[test]
fn a_keyword_that_matches_nothing_in_the_room_still_reports_a_miss() {
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit mob nothinglikethis");
    assert!(out.contains("No mobile matches"), "unexpected output:\n{out}");
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
    // One blocker must land an entity on F. A blocker means the content is
    // broken as shipped, and at the old weight of 45 it scored 55 and graded D
    // — printed in the same colour band as a missing seasonal description,
    // while the header above it counted the blocker in red.
    assert_eq!(
        audit::letter_for(100 - Severity::Blocker.weight()),
        'F',
        "one blocker no longer fails an entity"
    );
    // A warn alone must not, or the two severities stop meaning different
    // things.
    assert_ne!(audit::letter_for(100 - Severity::Warn.weight()), 'F');
}

#[test]
fn an_infinite_liquid_source_is_not_an_empty_container() {
    // `liquid_max == -1` is the fountain/sink sentinel: drink, fill and pour
    // all special-case it, so it must not be graded as "can never be filled".
    let mut fountain = ItemData::new(
        "dungeon sink".to_string(),
        "A small sink is mounted on the wall.".to_string(),
        "A small ceramic basin sits beneath a pair of iron pump handles.".to_string(),
    );
    fountain.item_type = ItemType::LiquidContainer;
    fountain.liquid_max = -1;
    fountain.liquid_current = -1;
    assert!(!audit::audit_item(&fountain).has("item.liquid_no_capacity"));

    // Zero capacity is still a blocker.
    let mut empty = fountain.clone();
    empty.liquid_max = 0;
    empty.liquid_current = 0;
    assert!(audit::audit_item(&empty).has("item.liquid_no_capacity"));

    // So is any other negative — only -1 carries meaning.
    let mut nonsense = fountain.clone();
    nonsense.liquid_max = -5;
    assert!(audit::audit_item(&nonsense).has("item.liquid_no_capacity"));
}

#[test]
fn the_worst_list_prints_a_long_vnum_whole() {
    // Thirty rooms called "Dungeon Hallway" are told apart by vnum and nothing
    // else, so a truncated vnum makes the row unactionable — the builder cannot
    // paste `dungeon1:du…` into `redit`.
    let (db, _t) = seeded_db();
    let area = db.list_all_areas().expect("areas").into_iter().next().expect("an area");

    let long_vnum = format!("{}:dungeon_hallway_east_branch_27", area.prefix);
    let mut hallway = room(json!({
        "title": "Dungeon Hallway",
        // Short enough to trip room.thin_desc, so the row lands in the worst list.
        "description": "A hallway.",
    }));
    hallway.area_id = Some(area.id);
    hallway.vnum = Some(long_vnum.clone());
    let room_id = hallway.id;
    db.save_room_data(hallway).expect("save room");
    db.set_room_vnum(&room_id, &long_vnum).expect("index vnum");

    let out = run_build_command(&db, &format!("audit area {}", area.name));
    assert!(
        out.contains(&long_vnum),
        "the {}-char vnum was truncated:\n{out}",
        long_vnum.len()
    );
}

// ===========================================================================
// Not knowing how the world works is a false positive
// ===========================================================================

/// An area with an elevator: two floors joined by nothing but the lift, plus
/// the car itself. `docked` writes the exits the transport tick would have
/// written on arrival; leaving it false is the in-transit state.
fn elevator_world(db: &Db, docked: bool) -> (ironmud::types::AreaData, Uuid, Uuid, Uuid) {
    let area: ironmud::types::AreaData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "name": "Iron Tower",
        "prefix": "tower",
        "combat_zone": "pve",
        "level_min": 1,
        "level_max": 5,
    }))
    .expect("area fixture");

    let mut lobby = room(json!({"title": "Lobby", "vnum": "tower:lobby"}));
    let mut penthouse = room(json!({"title": "Penthouse", "vnum": "tower:pent"}));
    let mut car = room(json!({"title": "Elevator Car", "vnum": "tower:car"}));
    for r in [&mut lobby, &mut penthouse, &mut car] {
        r.area_id = Some(area.id);
    }
    if docked {
        lobby.exits.custom.insert("in".into(), car.id);
        car.exits.out = Some(lobby.id);
    }
    let (lobby_id, pent_id, car_id) = (lobby.id, penthouse.id, car.id);

    let mut lift = ironmud::types::TransportData::new("the lift".into(), car_id);
    lift.stops = vec![
        ironmud::types::TransportStop {
            room_id: lobby_id,
            name: "Lobby".into(),
            exit_direction: "in".into(),
        },
        ironmud::types::TransportStop {
            room_id: pent_id,
            name: "Penthouse".into(),
            exit_direction: "in".into(),
        },
    ];

    db.save_area_data(area.clone()).expect("save area");
    for r in [lobby, penthouse, car] {
        db.save_room_data(r).expect("save room");
    }
    db.save_transport(&lift).expect("save transport");
    (area, lobby_id, pent_id, car_id)
}

#[test]
fn a_floor_served_only_by_the_elevator_is_not_an_orphan() {
    let (db, _t) = fresh_db();
    let (area, _lobby, _pent, _car) = elevator_world(&db, true);

    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let report = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
    assert!(
        !report.own.has("area.orphan_rooms"),
        "the penthouse is reachable by pressing a button: {:?}",
        report.own.findings
    );
}

#[test]
fn the_elevator_car_grades_the_same_docked_or_in_transit() {
    // `ticks::transport` writes the car's exits on arrival and clears them on
    // departure, so reading `RoomExits` alone grades this room differently
    // depending on where the car happens to be. A grade that moves on its own
    // is a grade nobody can act on.
    let mut grades = Vec::new();
    for docked in [true, false] {
        let (db, _t) = fresh_db();
        let (area, _lobby, _pent, car_id) = elevator_world(&db, docked);
        let snap = WorldSnapshot::load(&db).expect("snapshot");
        let car = db.get_room_data(&car_id).expect("read").expect("car");
        let g = audit_room(&car, snap.ctx());
        assert!(
            !g.has("room.no_exits"),
            "docked={docked}: the car is a vehicle, not a sealed box: {:?}",
            g.findings
        );
        let report = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
        assert!(!report.own.has("area.orphan_rooms"), "docked={docked}");
        grades.push(g.letter);
    }
    assert_eq!(grades[0], grades[1], "the car's letter moved with the lift");
}

#[test]
fn a_safe_zone_is_not_asked_for_a_level_range() {
    // A level range is a danger warning, and the city everyone starts in has
    // no danger to warn about.
    let (db, _t) = fresh_db();
    let mut area: ironmud::types::AreaData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "name": "IronMUD Plaza",
        "prefix": "plaza",
        "combat_zone": "safe",
    }))
    .expect("area fixture");
    area.level_min = 0;
    area.level_max = 0;
    db.save_area_data(area.clone()).expect("save area");

    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let safe = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
    assert!(!safe.own.has("area.no_level_range"), "{:?}", safe.own.findings);

    // And a combat zone still is.
    area.combat_zone = ironmud::types::CombatZoneType::Pve;
    db.save_area_data(area.clone()).expect("save area");
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let pve = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
    assert!(pve.own.has("area.no_level_range"), "{:?}", pve.own.findings);
}

#[test]
fn deliberate_scenery_is_not_an_untyped_item() {
    // A `misc` item with an extra description is scenery doing its job. So is
    // one flagged `no_get`. Both are a builder saying "on purpose".
    let mut coaster = ItemData::new(
        "a drink coaster".to_string(),
        "A drink coaster lies here.".to_string(),
        "A cork coaster, ringed by the ghosts of a hundred glasses.".to_string(),
    );
    coaster.item_type = ItemType::Misc;
    assert!(
        audit::audit_item(&coaster).has("item.untyped"),
        "a bare misc item is still flagged"
    );

    let mut examined = coaster.clone();
    examined.extra_descs.push(
        serde_json::from_value(json!({"keywords": ["rings"], "description": "Overlapping stains."}))
            .expect("extra desc"),
    );
    assert!(!audit::audit_item(&examined).has("item.untyped"));

    let mut fixed = coaster.clone();
    fixed.flags.no_get = true;
    assert!(!audit::audit_item(&fixed).has("item.untyped"));
}

// ===========================================================================
// Waivers — reviewed false positives
// ===========================================================================

/// An area with one item whose keywords reach nothing, so
/// `item.keywords_miss_nouns` fires and there is something to review.
fn unaddressable_item_world(db: &Db) -> (ironmud::types::AreaData, String) {
    let area: ironmud::types::AreaData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "name": "IronMUD Plaza",
        "prefix": "plaza",
        "owner": "Zoe",
        "permission_level": "trusted",
    }))
    .expect("area fixture");

    let mut coaster = ItemData::new(
        "widget".to_string(),
        "A blackvien drink coaster rests on the bar.".to_string(),
        "A cork coaster, ringed by the ghosts of a hundred glasses.".to_string(),
    );
    coaster.item_type = ItemType::Misc;
    coaster.is_prototype = true;
    coaster.area_id = Some(area.id);
    coaster.vnum = Some("plaza:coaster".into());
    coaster.keywords = vec!["widget".into()];

    db.save_area_data(area.clone()).expect("save area");
    db.save_item_data(coaster).expect("save item");
    (area, "plaza:coaster".to_string())
}

fn item_grade(db: &Db, vnum: &str) -> ironmud::audit::Grade {
    ironmud::audit::scan::scan_entity(db, EntityKind::Item, vnum)
        .expect("scan")
        .expect("item exists")
        .grade
}

#[test]
fn a_waived_finding_leaves_the_grade_the_tally_and_the_bounty_board() {
    let (db, _t) = fresh_db();
    let (area, vnum) = unaddressable_item_world(&db);

    let before = item_grade(&db, &vnum);
    let finding = before
        .findings
        .iter()
        .find(|f| f.code == "item.keywords_miss_nouns")
        .expect("the lint fires on this fixture")
        .clone();

    db.store_audit_waiver(&ironmud::types::AuditWaiver {
        code: finding.code.to_string(),
        target: vnum.clone(),
        area_prefix: area.prefix.clone(),
        reason: "the coaster answers to `coaster` in play".into(),
        fingerprint: ironmud::types::fingerprint(finding.code, &finding.message),
        severity: finding.severity.key().to_string(),
        reviewed_by: "Zoe".into(),
        created_at: 1,
    })
    .expect("store waiver");

    let after = item_grade(&db, &vnum);
    assert!(!after.has("item.keywords_miss_nouns"), "{:?}", after.findings);
    assert_eq!(after.waived.len(), 1, "the waived finding is kept, not dropped");
    assert!(after.score > before.score, "{} -> {}", before.score, after.score);

    // And the area report agrees — one chokepoint, not one per surface.
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let report = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
    assert_eq!(report.waived_count(), 1);
    let (_, warns, _) = report.severity_counts();
    let unreviewed = report
        .all_findings()
        .iter()
        .filter(|(_, f)| f.code == "item.keywords_miss_nouns")
        .count();
    assert_eq!(unreviewed, 0, "a waived finding still counted in the tally");
    let _ = warns;
}

#[test]
fn editing_the_text_a_waiver_was_written_about_revives_the_finding() {
    // A waiver is a judgement about a piece of text. Change the text and the
    // judgement has to be made again, or a waiver written for one wording goes
    // on hiding a genuinely different problem years later.
    let (db, _t) = fresh_db();
    let (area, vnum) = unaddressable_item_world(&db);

    let finding = item_grade(&db, &vnum)
        .findings
        .iter()
        .find(|f| f.code == "item.keywords_miss_nouns")
        .expect("fires")
        .clone();
    db.store_audit_waiver(&ironmud::types::AuditWaiver {
        code: finding.code.to_string(),
        target: vnum.clone(),
        area_prefix: area.prefix,
        reason: "reviewed".into(),
        fingerprint: ironmud::types::fingerprint(finding.code, &finding.message),
        severity: finding.severity.key().to_string(),
        reviewed_by: "Zoe".into(),
        created_at: 1,
    })
    .expect("store waiver");
    assert!(!item_grade(&db, &vnum).has("item.keywords_miss_nouns"));

    let mut item = db.get_item_by_vnum(&vnum).expect("read").expect("item");
    item.short_desc = "A chrome neural jack rests on a sterile tray.".into();
    db.save_item_data(item).expect("save item");

    let after = item_grade(&db, &vnum);
    assert!(
        after.has("item.keywords_miss_nouns"),
        "the waiver outlived the text it was about: {:?}",
        after.findings
    );
    assert!(after.waived.is_empty());
}

#[test]
fn a_waiver_is_scoped_to_one_entity_not_to_the_code() {
    let (db, _t) = fresh_db();
    let (area, vnum) = unaddressable_item_world(&db);

    let mut other = db.get_item_by_vnum(&vnum).expect("read").expect("item");
    other.id = Uuid::new_v4();
    other.vnum = Some("plaza:mug".into());
    db.save_item_data(other).expect("save item");

    let finding = item_grade(&db, &vnum)
        .findings
        .iter()
        .find(|f| f.code == "item.keywords_miss_nouns")
        .expect("fires")
        .clone();
    db.store_audit_waiver(&ironmud::types::AuditWaiver {
        code: finding.code.to_string(),
        target: vnum.clone(),
        area_prefix: area.prefix,
        reason: "reviewed".into(),
        fingerprint: ironmud::types::fingerprint(finding.code, &finding.message),
        severity: finding.severity.key().to_string(),
        reviewed_by: "Zoe".into(),
        created_at: 1,
    })
    .expect("store waiver");

    assert!(!item_grade(&db, &vnum).has("item.keywords_miss_nouns"));
    assert!(
        item_grade(&db, "plaza:mug").has("item.keywords_miss_nouns"),
        "waiving one entity silenced another"
    );
}

#[test]
fn build_waive_records_a_review_and_the_finding_stops_printing() {
    let (db, _t) = fresh_db();
    // Unowned, so `author_can_edit_area` lets the test builder through.
    let (_area, vnum) = unaddressable_item_world(&db);
    let mut area = db.list_all_areas().expect("areas").remove(0);
    area.owner = None;
    db.save_area_data(area).expect("save area");

    let listed = run_build_command(&db, "audit code item.keywords_miss_nouns");
    assert!(listed.contains(&vnum), "the code listing did not find it:\n{listed}");

    let out = run_build_command(
        &db,
        &format!("waive item.keywords_miss_nouns {vnum} players call it a coaster"),
    );
    assert!(out.contains("Reviewed."), "unexpected output:\n{out}");

    let after = run_build_command(&db, &format!("audit item {vnum}"));
    assert!(
        !after.contains("warn    item.keywords_miss_nouns"),
        "the waived finding is still listed as live:\n{after}"
    );
    assert!(after.contains("1 reviewed"), "the review is not counted:\n{after}");
    assert!(
        after.contains("Reviewed") && after.contains("item.keywords_miss_nouns"),
        "the review is invisible — suppression has to stay auditable:\n{after}"
    );

    let list = run_build_command(&db, "waive list");
    assert!(list.contains("item.keywords_miss_nouns"), "{list}");

    let removed = run_build_command(&db, &format!("waive remove item.keywords_miss_nouns {vnum}"));
    assert!(removed.contains("Restored."), "{removed}");
    let back = run_build_command(&db, &format!("audit item {vnum}"));
    assert!(back.contains("Players cannot address this"), "{back}");
}

#[test]
fn a_non_admin_cannot_waive_a_blocker() {
    // A blocker says the content is broken as shipped. That is the judgement a
    // builder is most tempted to overrule and least well placed to.
    let (db, _t) = fresh_db();
    let area: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": Uuid::new_v4(), "name": "Tower", "prefix": "tower"})).expect("area");
    let mut sealed = room(json!({"title": "Sealed Vault", "vnum": "tower:vault"}));
    sealed.area_id = Some(area.id);
    let id = sealed.id;
    db.save_area_data(area).expect("save area");
    db.save_room_data(sealed).expect("save room");
    db.set_room_vnum(&id, "tower:vault").expect("index vnum");

    let out = run_build_command(&db, "waive room.no_exits tower:vault it is an elevator stop");
    assert!(out.contains("BLOCKER"), "expected a refusal explaining why:\n{out}");
    assert!(
        run_build_command(&db, "audit room tower:vault").contains("no exits"),
        "the blocker was silenced by a non-admin"
    );
}

#[test]
fn waiving_a_finding_that_is_not_firing_is_refused() {
    // The finding has to be live: that is what supplies the message the waiver
    // fingerprints, and it stops anyone pre-silencing a code they have never
    // seen fire.
    let (db, _t) = fresh_db();
    let (_area, vnum) = unaddressable_item_world(&db);
    let out = run_build_command(&db, &format!("waive item.no_value {vnum} not for sale"));
    assert!(out.contains("not firing"), "unexpected output:\n{out}");
}

#[test]
fn build_waive_all_reviews_every_target_in_the_area_at_once() {
    let (db, _t) = fresh_db();
    let (_area, vnum) = unaddressable_item_world(&db);
    let mut area = db.list_all_areas().expect("areas").remove(0);
    area.owner = None;
    db.save_area_data(area).expect("save area");

    let mut second = db.get_item_by_vnum(&vnum).expect("read").expect("item");
    second.id = Uuid::new_v4();
    second.vnum = Some("plaza:mug".into());
    db.save_item_data(second).expect("save item");

    let out = run_build_command(
        &db,
        "waive all item.keywords_miss_nouns plaza the nouns are room dressing",
    );
    assert!(out.contains("Reviewed."), "unexpected output:\n{out}");
    assert!(out.contains('2'), "both targets should have been reviewed:\n{out}");
    assert!(!item_grade(&db, &vnum).has("item.keywords_miss_nouns"));
    assert!(!item_grade(&db, "plaza:mug").has("item.keywords_miss_nouns"));
}

#[test]
fn a_prototype_belongs_to_the_area_its_vnum_names_even_unstamped() {
    // Reported from a live server: `build audit area Islands` said "No mobile
    // prototypes — the area is uninhabited" with a sow standing in the room.
    // The API's create_mobile/create_item require a vnum and take area_id as
    // optional, so an area built through MCP files its rooms and orphans
    // everything else. The vnum names the area; the auditor now reads it.
    let (db, _t) = fresh_db();
    let area: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": Uuid::new_v4(), "name": "Islands", "prefix": "islands"}))
            .expect("area fixture");
    db.save_area_data(area.clone()).expect("save area");

    let mut beach = room(json!({"title": "Sandy Beach", "vnum": "islands:sandy_beach_3"}));
    beach.area_id = None;
    db.save_room_data(beach).expect("save room");

    let mut sow = ironmud::types::MobileData::new("sow".to_string());
    sow.is_prototype = true;
    sow.area_id = None;
    sow.vnum = "islands:sow".to_string();
    db.save_mobile_data(sow).expect("save mobile");

    let mut shell = ItemData::new(
        "shell".to_string(),
        "A conch shell lies half-buried in the sand.".to_string(),
        "A conch shell, its pink throat scoured pale by the surf.".to_string(),
    );
    shell.item_type = ItemType::Misc;
    shell.is_prototype = true;
    shell.area_id = None;
    shell.vnum = Some("islands:shell".into());
    db.save_item_data(shell).expect("save item");

    let snap = WorldSnapshot::load(&db).expect("snapshot");
    let contents = snap.area_contents(area.id);
    assert_eq!(contents.mobiles.len(), 1, "the sow belongs to Islands");
    assert_eq!(contents.items.len(), 1, "the shell belongs to Islands");
    assert_eq!(contents.rooms.len(), 1);

    let report = ironmud::audit::scan::scan_area(&snap, &area, snap.ctx());
    assert!(!report.own.has("area.no_mobiles"), "{:?}", report.own.findings);
    assert!(!report.own.has("area.no_items"), "{:?}", report.own.findings);

    // And nothing is counted twice: content filed by vnum is not also unfiled.
    let facts = snap.facts();
    assert_eq!(facts.unfiled_mobiles, 0);
    assert_eq!(facts.unfiled_items, 0);
    assert_eq!(snap.orphan_room_count(), 0);

    // An unrelated prefix still names no area.
    let mut stray = ironmud::types::MobileData::new("gull".to_string());
    stray.is_prototype = true;
    stray.vnum = "nowhere:gull".to_string();
    db.save_mobile_data(stray).expect("save mobile");
    let snap = WorldSnapshot::load(&db).expect("snapshot");
    assert_eq!(snap.facts().unfiled_mobiles, 1);
    assert_eq!(snap.area_contents(area.id).mobiles.len(), 1);
}

#[test]
fn the_backfill_files_unstamped_content_and_leaves_the_ambiguous_alone() {
    // The auditor reads membership by vnum, but `area_id` is what the edit
    // permission gate and the per-area quotas read, so the stored field has to
    // catch up with what the vnums have been saying.
    let (db, _t) = fresh_db();
    for (name, prefix) in [("Islands", "islands"), ("Twin A", "twin"), ("Twin B", "twin")] {
        let area: ironmud::types::AreaData =
            serde_json::from_value(json!({"id": Uuid::new_v4(), "name": name, "prefix": prefix}))
                .expect("area fixture");
        db.save_area_data(area).expect("save area");
    }
    let islands = db
        .list_all_areas()
        .expect("areas")
        .into_iter()
        .find(|a| a.prefix == "islands")
        .expect("islands");

    let mut beach = room(json!({"title": "Sandy Beach", "vnum": "islands:sandy_beach_3"}));
    beach.area_id = None;
    let beach_id = beach.id;
    db.save_room_data(beach).expect("save room");

    let mut sow = ironmud::types::MobileData::new("sow".to_string());
    sow.is_prototype = true;
    sow.vnum = "islands:sow".to_string();
    let sow_id = sow.id;
    db.save_mobile_data(sow).expect("save mobile");

    // A live instance is not authored content — it inherits at spawn.
    let mut piglet = ironmud::types::MobileData::new("piglet".to_string());
    piglet.is_prototype = false;
    piglet.vnum = "islands:piglet".to_string();
    let piglet_id = piglet.id;
    db.save_mobile_data(piglet).expect("save mobile");

    // A prefix two areas answer to names neither of them.
    let mut ghost = ironmud::types::MobileData::new("ghost".to_string());
    ghost.is_prototype = true;
    ghost.vnum = "twin:ghost".to_string();
    let ghost_id = ghost.id;
    db.save_mobile_data(ghost).expect("save mobile");

    db.migrate_prototypes_to_vnum_areas().expect("backfill");

    assert_eq!(
        db.get_room_data(&beach_id).expect("read").expect("room").area_id,
        Some(islands.id)
    );
    assert_eq!(
        db.get_mobile_data(&sow_id).expect("read").expect("mob").area_id,
        Some(islands.id)
    );
    assert_eq!(
        db.get_mobile_data(&piglet_id).expect("read").expect("mob").area_id,
        None,
        "instances are spawned content, not authored content"
    );
    assert_eq!(
        db.get_mobile_data(&ghost_id).expect("read").expect("mob").area_id,
        None,
        "an ambiguous prefix is skipped rather than guessed at"
    );

    // One-shot, and it never overwrites a stamp: re-running changes nothing.
    let mut moved = db.get_mobile_data(&sow_id).expect("read").expect("mob");
    moved.area_id = None;
    db.save_mobile_data(moved).expect("save mobile");
    db.migrate_prototypes_to_vnum_areas().expect("backfill");
    assert_eq!(
        db.get_mobile_data(&sow_id).expect("read").expect("mob").area_id,
        None,
        "the guard key should have made the second run a no-op"
    );
}

#[test]
fn the_area_header_splits_its_tally_between_itself_and_its_contents() {
    // The complaint this answers: a header reading "3 blocker" over a list in
    // which only two rows were blockers, because the third belonged to the area
    // itself and was printed in a different block.
    let (db, _t) = seeded_db();
    let out = run_build_command(&db, "audit area Oakvale Village");
    assert!(
        out.contains("on the area,") || out.contains("clean"),
        "the split tally is missing:\n{out}"
    );
}

#[test]
fn prose_with_curly_quotes_and_dashes_grades_instead_of_panicking() {
    // Reported from a live server: `build audit code area` dropped the
    // connection. The keyword lint walked the display string a byte at a
    // time, and a UTF-8 continuation byte read as a latin-1 character can be
    // alphabetic — so the word boundary landed mid-character and slicing it
    // panicked. Builder prose is full of curly apostrophes and em dashes.
    let (db, _t) = seeded_db();
    let area: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": Uuid::new_v4(), "name": "Islands", "prefix": "islands"}))
            .expect("area fixture");
    db.save_area_data(area.clone()).expect("save area");

    let mut beach = room(json!({"title": "Sandy Beach", "vnum": "islands:beach"}));
    beach.area_id = Some(area.id);
    beach.description = "Powdery sand — pale as bone — runs down to the water\u{2019}s edge.".to_string();
    db.save_room_data(beach).expect("save room");

    for short_desc in [
        "a sow\u{2019}s piglet snuffles here.",
        "a piglet — small and pink — stands here.",
        "un cochon rôti repose ici.",
        "\u{4e00}\u{5934}\u{732a}",
    ] {
        let mut m = ironmud::types::MobileData::new("piglet".to_string());
        m.is_prototype = true;
        m.vnum = format!("islands:{}", Uuid::new_v4());
        m.short_desc = short_desc.to_string();
        m.area_id = Some(area.id);
        // Every one of these grades. The assertion that matters is that the
        // call returns at all.
        let grade = ironmud::audit::audit_mobile(&m);
        assert!((0..=100).contains(&grade.score), "{short_desc}");
        db.save_mobile_data(m).expect("save mobile");
    }

    let mut shell = ItemData::new(
        "shell".to_string(),
        "a conch shell — pink-throated — lies here.".to_string(),
        "A conch shell, its throat scoured pale by the surf.".to_string(),
    );
    shell.item_type = ItemType::Misc;
    shell.is_prototype = true;
    shell.area_id = Some(area.id);
    shell.vnum = Some("islands:shell".into());
    db.save_item_data(shell).expect("save item");

    // The command that fell over: it walks every entity in the world.
    let out = run_build_command(&db, "audit code mobile.keywords_miss_nouns");
    assert!(!out.is_empty());
    let out = run_build_command(&db, "audit area islands");
    assert!(out.contains("Islands"), "unexpected output:\n{out}");
}
