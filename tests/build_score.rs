//! Builder-score integration tests.
//!
//! The arithmetic is unit-tested in `src/build_score.rs`. These run the whole
//! thing — scan, score, reconcile — against a real database, and they exist
//! mainly to pin the four properties the design is built on. Every one of them
//! is a way the feature could quietly become a room-count contest:
//!
//! 1. a hundred empty rooms score below five good ones,
//! 2. re-saving the same content earns nothing,
//! 3. deleting content lowers the score,
//! 4. imported and seeded content are worth exactly zero to everybody.

#![recursion_limit = "256"]

use std::collections::HashMap;

use ironmud::audit::scan::WorldSnapshot;
use ironmud::build_score::{self, BuildScores};
use ironmud::db::Db;
use ironmud::types::{CharacterData, ContentKind, ContentOrigin, ItemData, MobileData, RoomData};
use serde_json::json;
use uuid::Uuid;

/// A fresh room context. In the server this is cached on `World` and refreshed
/// by the score tick; here it is cheap enough to build per call.
fn ctx(db: &Db) -> ironmud::audit::AuditCtx {
    ironmud::audit::AuditCtx::build(&db.list_all_rooms().unwrap())
}

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

/// A room good enough to grade well: real description, an exit, flags set,
/// something to examine.
fn good_room(area: Uuid, author: &str, dest: Uuid) -> RoomData {
    let mut r: RoomData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "title": "A Rutted Track",
        "description": format!(
            "Cart ruts run the length of the track, filled with brown water that has not \
             drained since the last rain and will not before the next. {}",
            Uuid::new_v4()
        ),
        "exits": {"north": dest},
        "area_id": area,
        "flags": {"city": true},
        "extra_descs": [{"keywords": ["ruts"], "description": "Deep, and full of water."}],
        "spring_desc": "Mud, and more mud.",
    }))
    .expect("room fixture");
    r.authored_by = Some(author.to_string());
    r.last_edited_by = Some(author.to_string());
    r.origin = ContentOrigin::Builder;
    r
}

/// A room with nothing in it. Grades F.
fn empty_room(area: Uuid, author: &str) -> RoomData {
    let mut r: RoomData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "title": "",
        "description": "",
        "exits": {},
        "area_id": area,
    }))
    .expect("room fixture");
    r.authored_by = Some(author.to_string());
    r.last_edited_by = Some(author.to_string());
    r.origin = ContentOrigin::Builder;
    r
}

fn area(db: &Db, name: &str, author: Option<&str>) -> Uuid {
    let id = Uuid::new_v4();
    let mut a: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": id, "name": name, "prefix": name.to_lowercase()})).unwrap();
    if let Some(author) = author {
        a.authored_by = Some(author.to_string());
        a.origin = ContentOrigin::Builder;
    }
    db.save_area_data(a).unwrap();
    id
}

fn score(db: &Db) -> BuildScores {
    let snapshot = WorldSnapshot::load(db).expect("snapshot");
    build_score::compute(&snapshot, &HashMap::new(), 1000)
}

fn points(db: &Db, name: &str) -> i32 {
    score(db).get(name).map(|b| b.total).unwrap_or(0)
}

// ===========================================================================
// The anti-farm properties
// ===========================================================================

#[test]
fn a_hundred_empty_rooms_score_below_five_good_ones() {
    let (db, _t) = fresh_db();
    let padded = area(&db, "Padded", None);
    let crafted = area(&db, "Crafted", None);

    for _ in 0..100 {
        db.save_room_data(empty_room(padded, "Padder")).unwrap();
    }
    // Five rooms in a ring, so each has a reciprocal exit.
    let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    for (i, id) in ids.iter().enumerate() {
        let mut r = good_room(crafted, "Crafter", ids[(i + 1) % ids.len()]);
        r.id = *id;
        db.save_room_data(r).unwrap();
    }

    let padder = points(&db, "Padder");
    let crafter = points(&db, "Crafter");
    assert!(
        crafter > padder,
        "five good rooms ({crafter}) did not beat a hundred empty ones ({padder})"
    );
}

#[test]
fn re_saving_the_same_content_earns_nothing() {
    // There is no event to farm. This is the whole reason the score is a scan
    // rather than a counter.
    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);
    let dest = Uuid::new_v4();
    let room = good_room(a, "Ana", dest);
    db.save_room_data(room.clone()).unwrap();

    let first = points(&db, "Ana");
    for _ in 0..50 {
        db.save_room_data(room.clone()).unwrap();
    }
    assert_eq!(points(&db, "Ana"), first, "re-saving moved the score");
}

#[test]
fn deleting_your_own_work_lowers_your_score() {
    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);
    let ids: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
    for (i, id) in ids.iter().enumerate() {
        let mut r = good_room(a, "Ana", ids[(i + 1) % ids.len()]);
        r.id = *id;
        db.save_room_data(r).unwrap();
    }
    let before = points(&db, "Ana");
    assert!(before > 0);

    db.delete_room(&ids[0]).unwrap();
    let after = points(&db, "Ana");
    assert!(
        after < before,
        "deleting content did not lower the score ({before} -> {after})"
    );
}

#[test]
fn imported_and_seeded_content_is_worth_nothing_to_anybody() {
    let (db, _t) = fresh_db();
    let a = area(&db, "Imported", None);
    for origin in [ContentOrigin::Import, ContentOrigin::Seed, ContentOrigin::Unknown] {
        for _ in 0..20 {
            let mut r = good_room(a, "Importer", Uuid::new_v4());
            r.origin = origin;
            db.save_room_data(r).unwrap();
        }
    }
    let scores = score(&db);
    assert_eq!(
        scores.get("Importer").map(|b| b.total).unwrap_or(0),
        0,
        "content that did not come from a builder was credited"
    );
    assert_eq!(scores.credited_entities, 0);
    assert!(scores.uncredited_entities >= 60);
}

#[test]
fn the_seeded_demo_world_credits_nobody() {
    // The end-to-end version of the guard: an operator who boots a fresh
    // server must not find that the demo world has already crowned someone.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let scores = score(&db);
    assert!(
        scores.builders.is_empty(),
        "the demo world produced builder scores: {:?}",
        scores.builders.keys().collect::<Vec<_>>()
    );
    assert!(scores.uncredited_entities > 100);
}

#[test]
fn padding_one_area_pays_less_than_starting_another() {
    // Diminishing returns are per (author, area, kind), so breadth beats
    // depth-in-one-place. If this ever inverts, the score is telling builders
    // to make one enormous corridor.
    let (db, _t) = fresh_db();
    let one = area(&db, "One", None);
    let two_a = area(&db, "TwoA", None);
    let two_b = area(&db, "TwoB", None);

    for _ in 0..60 {
        db.save_room_data(good_room(one, "Deep", Uuid::new_v4())).unwrap();
    }
    for _ in 0..30 {
        db.save_room_data(good_room(two_a, "Broad", Uuid::new_v4())).unwrap();
        db.save_room_data(good_room(two_b, "Broad", Uuid::new_v4())).unwrap();
    }

    let deep = points(&db, "Deep");
    let broad = points(&db, "Broad");
    assert!(
        broad > deep,
        "sixty rooms in two areas ({broad}) did not beat sixty in one ({deep})"
    );
}

#[test]
fn a_mobile_with_a_dialogue_tree_is_worth_more_than_one_without() {
    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);

    let base = json!({
        "id": Uuid::new_v4(),
        "name": "a goblin guard",
        "short_desc": "A goblin guard leans on a rusted pike.",
        "long_desc": "Squat and thick-shouldered, watching the road with flat yellow eyes.",
        "keywords": ["goblin", "guard", "pike"],
        "level": 4,
        "damage_dice": "2d6+1",
        "gold": 20,
        "is_prototype": true,
        "area_id": a,
        "alignment": -20,
    });

    let mut plain: MobileData = serde_json::from_value(base.clone()).unwrap();
    plain.authored_by = Some("Plain".into());
    plain.origin = ContentOrigin::Builder;
    db.save_mobile_data(plain).unwrap();

    let mut talker: MobileData = serde_json::from_value(base).unwrap();
    talker.id = Uuid::new_v4();
    talker.authored_by = Some("Talker".into());
    talker.origin = ContentOrigin::Builder;
    talker.dialogue_tree = Some(Default::default());
    db.save_mobile_data(talker).unwrap();

    assert!(
        points(&db, "Talker") > points(&db, "Plain"),
        "a dialogue tree bought nothing"
    );
}

// ===========================================================================
// Reconciliation onto characters
// ===========================================================================

fn builder_char(db: &Db, name: &str) -> CharacterData {
    let ch: CharacterData = serde_json::from_value(json!({
        "name": name,
        "password_hash": "",
        "current_room_id": Uuid::nil(),
        "is_builder": true,
    }))
    .unwrap();
    db.save_character_data(ch.clone()).unwrap();
    ch
}

fn run_tick(db: &Db) {
    let connections: ironmud::SharedConnections =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
    build_score::process_build_score_tick(db, &connections, &state, 1234).expect("tick");
}

fn counter(db: &Db, name: &str, key: &str) -> u32 {
    db.get_character_data(name)
        .unwrap()
        .unwrap()
        .achievement_counters
        .get(key)
        .copied()
        .unwrap_or(0)
}

#[test]
fn the_tick_writes_counters_onto_the_builder() {
    let (db, _t) = fresh_db();
    builder_char(&db, "Ana");
    let a = area(&db, "Home", Some("Ana"));
    for _ in 0..3 {
        db.save_room_data(good_room(a, "Ana", Uuid::new_v4())).unwrap();
    }

    run_tick(&db);
    assert_eq!(counter(&db, "Ana", "build.rooms"), 3);
    assert_eq!(counter(&db, "Ana", "build.areas"), 1);
    assert!(counter(&db, "Ana", "build.score") > 0);
}

#[test]
fn a_demoted_builders_counters_do_not_freeze() {
    // Reconciling only characters that hold the builder bit left a demoted
    // builder ranked on the Building boards at their last reading forever —
    // nothing wrote them again, so no deletion could move the number. Anyone
    // the scan still credits gets reconciled whether or not they are a builder
    // today.
    let (db, _t) = fresh_db();
    builder_char(&db, "Ana");
    let a = area(&db, "Home", Some("Ana"));
    let ids: Vec<Uuid> = (0..3)
        .map(|_| {
            let r = good_room(a, "Ana", Uuid::new_v4());
            let id = r.id;
            db.save_room_data(r).unwrap();
            id
        })
        .collect();
    run_tick(&db);
    assert_eq!(counter(&db, "Ana", "build.rooms"), 3);

    // Ana loses the bit, and then her rooms go.
    let mut ch = db.get_character_data("Ana").unwrap().unwrap();
    ch.is_builder = false;
    db.save_character_data(ch).unwrap();
    for id in &ids {
        db.delete_room(id).unwrap();
    }
    run_tick(&db);

    assert_eq!(
        counter(&db, "Ana", "build.rooms"),
        0,
        "a demoted builder kept a score no deletion could move"
    );
}

#[test]
fn counters_reconcile_downward_when_content_goes_away() {
    // The property `notify_counter_core` cannot provide, and the reason
    // `reconcile_counter_core` exists: a builder's room count is a fact about
    // the world right now, not a tally of things that once happened.
    let (db, _t) = fresh_db();
    builder_char(&db, "Ana");
    let a = area(&db, "Home", Some("Ana"));
    let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
    for id in &ids {
        let mut r = good_room(a, "Ana", Uuid::new_v4());
        r.id = *id;
        db.save_room_data(r).unwrap();
    }

    run_tick(&db);
    assert_eq!(counter(&db, "Ana", "build.rooms"), 4);
    let high = counter(&db, "Ana", "build.score");

    for id in &ids[..3] {
        db.delete_room(id).unwrap();
    }
    run_tick(&db);
    assert_eq!(counter(&db, "Ana", "build.rooms"), 1);
    assert!(counter(&db, "Ana", "build.score") < high);
}

#[test]
fn a_builder_who_has_built_nothing_gets_zeroes_not_stale_numbers() {
    let (db, _t) = fresh_db();
    builder_char(&db, "Ana");
    let a = area(&db, "Home", None);
    let room = good_room(a, "Ana", Uuid::new_v4());
    let room_id = room.id;
    db.save_room_data(room).unwrap();

    run_tick(&db);
    assert_eq!(counter(&db, "Ana", "build.rooms"), 1);

    db.delete_room(&room_id).unwrap();
    run_tick(&db);
    assert_eq!(
        counter(&db, "Ana", "build.rooms"),
        0,
        "the last non-zero reading was left in place"
    );
}

#[test]
fn non_builders_are_not_given_builder_counters() {
    let (db, _t) = fresh_db();
    let player: CharacterData = serde_json::from_value(json!({
        "name": "Player",
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .unwrap();
    db.save_character_data(player).unwrap();

    run_tick(&db);
    let ch = db.get_character_data("Player").unwrap().unwrap();
    assert!(
        ch.achievement_counters.keys().all(|k| !k.starts_with("build.")),
        "a non-builder was given builder counters: {:?}",
        ch.achievement_counters
    );
}

#[test]
fn bounty_points_stand_alone_and_survive_deletion() {
    // A bounty pays for work whose product may be spread across content the
    // claimant does not own, so it is the one stored term. It must not vanish
    // because somebody else later deleted the room it paid for.
    let (db, _t) = fresh_db();
    let snapshot = WorldSnapshot::load(&db).unwrap();
    let mut bounty = HashMap::new();
    bounty.insert("Ana".to_string(), 120);
    let scores = build_score::compute(&snapshot, &bounty, 1);

    let ana = scores.get("Ana").expect("bounty-only builder has a score");
    assert_eq!(ana.total, 120);
    assert_eq!(ana.content_points, 0);
    assert_eq!(ana.entities(), 0);
}

// ===========================================================================
// The grade toast
// ===========================================================================

#[test]
fn a_grade_that_did_not_move_says_nothing() {
    use ironmud::audit::scan;

    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);
    // The neighbour has to exist, or the room starts out already carrying a
    // `room.dangling_exit` blocker — and a blocker is an F on its own, so
    // emptying the description would move it from F to F and announce nothing.
    let neighbour = good_room(a, "Ana", Uuid::new_v4());
    let neighbour_id = neighbour.id;
    db.save_room_data(neighbour).unwrap();
    let mut room = good_room(a, "Ana", neighbour_id);
    room.vnum = Some("home:track".into());
    let id = room.id;
    db.save_room_data(room).unwrap();

    let before = scan::grade_snapshot(&db, ContentKind::Room, &id.to_string(), &ctx(&db));

    // A change that does not cross a letter boundary.
    let mut r = db.get_room_data(&id).unwrap().unwrap();
    r.title = "A Rutted Track, Renamed".into();
    db.save_room_data(r).unwrap();
    assert!(
        scan::grade_change_line(&db, &before, &ctx(&db)).is_none(),
        "a move inside one band announced itself"
    );

    // A change that does.
    let mut r = db.get_room_data(&id).unwrap().unwrap();
    r.description = String::new();
    db.save_room_data(r).unwrap();
    let line = scan::grade_change_line(&db, &before, &ctx(&db)).expect("crossing a band must announce");
    assert!(line.contains("now grades"), "{line}");
}

#[test]
fn editing_a_room_parks_a_grade_snapshot_on_the_session() {
    // The binding and the editor wiring, end to end. Without this, a missing
    // `note_grade_before` call in one editor is invisible: the toast simply
    // never appears for that editor and nothing fails.
    use ironmud::{PlayerSession, World};
    use std::sync::{Arc, Mutex};

    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);
    let mut room = good_room(a, "Ana", Uuid::new_v4());
    room.vnum = Some("home:track".into());
    let room_id = room.id;
    db.save_room_data(room).unwrap();

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());
    let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": "Ana", "password_hash": "", "is_builder": true, "current_room_id": room_id,
        }))
        .unwrap(),
    );
    connections.lock().unwrap().insert(conn_id, session);

    // A FREE-STANDING engine, deliberately not `world.engine`, and the World
    // lock is never held across `call_fn`.
    //
    // `note_grade_before` reads the cached audit context off `World`, and
    // `std::sync::Mutex` is not reentrant — running a script while holding the
    // World lock deadlocks. The server avoids this by cloning the AST and
    // dropping the lock before `call_fn`; a test is simpler if it never takes
    // the lock at all. Same trap `tests/attribution.rs` documents.
    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    ironmud::script::register_rhai_functions(&mut engine, Arc::new(db.clone()), connections.clone(), state.clone());
    let ast = engine
        .compile_file("scripts/commands/redit.rhai".into())
        .expect("compile redit");

    let mut scope = rhai::Scope::new();
    let r: Result<(), _> = engine.call_fn(
        &mut scope,
        &ast,
        "run_command",
        ("title A Better Name".to_string(), conn_id.to_string()),
    );
    r.expect("redit run_command");

    let pending = connections
        .lock()
        .unwrap()
        .get(&conn_id)
        .and_then(|s| s.pending_audit.clone());
    let pending = pending.expect("redit did not park a grade snapshot");
    assert_eq!(pending.kind, ContentKind::Room);
    assert!(pending.before.is_some(), "the pre-edit grade was not captured");
    // `contains`, not `==`: `redit title` re-joins its arguments and leaves a
    // leading space on the stored title. Pre-existing, unrelated to the
    // snapshot this test is about, and not worth changing arg parsing for
    // inside a scoring change.
    assert!(
        db.get_room_data(&room_id)
            .unwrap()
            .unwrap()
            .title
            .contains("A Better Name")
    );
}

#[test]
fn a_creation_always_reports_because_it_moved_from_nothing() {
    use ironmud::audit::scan;

    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);
    let room = good_room(a, "Ana", Uuid::new_v4());
    let id = room.id;

    // Snapshot before it exists — this is what a create branch does.
    let before = scan::grade_snapshot(&db, ContentKind::Room, &id.to_string(), &ctx(&db));
    assert!(before.before.is_none());

    db.save_room_data(room).unwrap();
    let line = scan::grade_change_line(&db, &before, &ctx(&db)).expect("a creation must report");
    assert!(line.contains("graded"), "{line}");
}

// ===========================================================================
// Housekeeping
// ===========================================================================

#[test]
fn every_builder_achievement_targets_a_counter_the_scan_writes() {
    // `tests/achievements.rs` proves the counter name appears somewhere in the
    // source. This proves it is one the build-score tick actually reconciles,
    // which is the stronger claim and the one that matters: a `build.*`
    // achievement pointed at a key nothing sets can never unlock.
    use ironmud::types::{AchievementCriterion, AchievementDef};

    let written: Vec<&str> = build_score::BuilderScore::default()
        .counters()
        .into_iter()
        .map(|(k, _)| k)
        .collect();

    let content = std::fs::read_to_string("scripts/data/achievements/builder.json").expect("builder.json");
    let defs: Vec<AchievementDef> = serde_json::from_str(&content).expect("parse builder.json");
    assert!(!defs.is_empty());

    for def in &defs {
        if let AchievementCriterion::Counter { counter, .. } = &def.criterion {
            assert!(
                written.contains(&counter.as_str()) || counter == "build.bounties",
                "{} targets {counter}, which the build-score scan never writes",
                def.key
            );
        }
    }
}

#[test]
fn builder_achievements_never_pay_in_trait_points() {
    // Trait points are a *player* currency: they buy character power. Paying
    // them for building would let a builder buy their way up the other ladder,
    // and would put builder rewards inside the scarcity budget
    // `tests/achievements.rs` guards. Builder work pays in builder points and
    // titles.
    use ironmud::types::AchievementDef;

    let content = std::fs::read_to_string("scripts/data/achievements/builder.json").expect("builder.json");
    let defs: Vec<AchievementDef> = serde_json::from_str(&content).expect("parse builder.json");
    for def in &defs {
        assert_eq!(def.reward.trait_points, 0, "{} pays trait points for building", def.key);
        assert!(!def.reward.title.is_empty(), "{} has no title to show for it", def.key);
    }
}

#[test]
fn an_unfinished_item_still_scores_something_but_not_much() {
    // F-grade content is not worth zero — it exists, and a builder mid-draft
    // should not read as having done nothing — but it must be small enough
    // that shipping it deliberately is never the play.
    let (db, _t) = fresh_db();
    let a = area(&db, "Home", None);

    let mut rough: ItemData = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "thing", "short_desc": "", "long_desc": "",
        "is_prototype": true, "area_id": a,
    }))
    .unwrap();
    rough.authored_by = Some("Rough".into());
    rough.origin = ContentOrigin::Builder;
    db.save_item_data(rough).unwrap();

    let scores = score(&db);
    let rough_points = scores.get("Rough").map(|b| b.total).unwrap_or(0);
    assert!(
        rough_points < build_score::kind_weight(ContentKind::Item) / 4,
        "an unfinished item scored {rough_points}"
    );
}
