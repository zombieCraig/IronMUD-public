//! World rating and milestone integration tests.
//!
//! The rating curves and caps are unit-tested in `src/world_rating.rs`. These
//! run the whole path — scan, rate, record, credit, announce — and pin the two
//! things that would quietly ruin the feature:
//!
//! 1. a fresh install must unlock nothing, or the wall records nothing;
//! 2. a milestone must fire once and stay fired, with the credit it had.

#![recursion_limit = "256"]

use std::collections::HashMap;

use ironmud::audit::scan::WorldSnapshot;
use ironmud::build_score;
use ironmud::db::Db;
use ironmud::types::{CharacterData, ContentOrigin, RoomData};
use ironmud::world_rating;
use serde_json::json;
use uuid::Uuid;

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn shared(db: &Db) -> (ironmud::SharedConnections, ironmud::SharedState) {
    let connections: ironmud::SharedConnections =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
    (connections, state)
}

fn builder(db: &Db, name: &str) {
    let ch: CharacterData = serde_json::from_value(json!({
        "name": name, "password_hash": "", "current_room_id": Uuid::nil(), "is_builder": true,
    }))
    .unwrap();
    db.save_character_data(ch).unwrap();
}

fn area_with_rooms(db: &Db, prefix: &str, author: &str, rooms: usize) -> Uuid {
    let area_id = Uuid::new_v4();
    let mut a: ironmud::types::AreaData =
        serde_json::from_value(json!({"id": area_id, "name": prefix, "prefix": prefix})).unwrap();
    a.authored_by = Some(author.to_string());
    a.origin = ContentOrigin::Builder;
    db.save_area_data(a).unwrap();

    let ids: Vec<Uuid> = (0..rooms).map(|_| Uuid::new_v4()).collect();
    for (i, id) in ids.iter().enumerate() {
        let mut r: RoomData = serde_json::from_value(json!({
            "id": id,
            "title": format!("{prefix} {i}"),
            "description": format!(
                "A long enough description to clear the stub floor, with something in it \
                 worth reading and a number to keep it distinct: {i}."
            ),
            "exits": {"north": ids[(i + 1) % ids.len()]},
            "area_id": area_id,
            "flags": {"city": true},
        }))
        .unwrap();
        r.authored_by = Some(author.to_string());
        r.origin = ContentOrigin::Builder;
        db.save_room_data(r).unwrap();
    }
    area_id
}

fn tick(db: &Db, connections: &ironmud::SharedConnections, state: &ironmud::SharedState, now: i64) {
    build_score::process_build_score_tick(db, connections, state, now).expect("tick");
}

fn rating_of(db: &Db) -> world_rating::WorldRating {
    let snapshot = WorldSnapshot::load(db).unwrap();
    let quality = ironmud::audit::scan::world_quality_pct(&snapshot);
    world_rating::rate(&snapshot.facts(), quality)
}

// ===========================================================================
// The rating, against real worlds
// ===========================================================================

#[test]
fn an_empty_database_rates_as_wilderness() {
    let (db, _t) = fresh_db();
    let r = rating_of(&db);
    assert_eq!(r.tier_key, "wilderness");
    assert_eq!(r.score, 0);
}

#[test]
fn the_shipped_demo_world_is_a_village_held_there_by_having_no_quests() {
    // The single most useful thing this rating says to a new operator, and the
    // reason caps exist: the demo world scores like a Town on the curves, and
    // it is not one.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let r = rating_of(&db);

    assert!(r.score > 0, "the demo world rated 0");
    assert!(r.cap.is_some(), "nothing capped the demo world");
    assert!(
        r.next_step().contains("quest"),
        "the advice did not name the cap: {}",
        r.next_step()
    );
    assert!(
        world_rating::LADDER.index_of(r.score) < world_rating::LADDER.tiers.len() - 4,
        "the demo world rated {} — too generous",
        r.tier_label
    );
}

#[test]
fn quality_moves_the_rating() {
    let (db, _t) = fresh_db();
    area_with_rooms(&db, "good", "Ana", 12);
    let with_good = rating_of(&db);

    // Break every room's description; the quality term should fall and take
    // the rating with it.
    for mut r in db.list_all_rooms().unwrap() {
        r.description = String::new();
        db.save_room_data(r).unwrap();
    }
    let with_bad = rating_of(&db);
    assert!(
        with_bad.score < with_good.score,
        "gutting every description did not lower the rating ({} -> {})",
        with_good.score,
        with_bad.score
    );
}

// ===========================================================================
// Milestones
// ===========================================================================

#[test]
fn a_fresh_install_unlocks_nothing() {
    // If the demo world lights up the wall on first boot, the wall is not a
    // record of anything and the first real milestone is taken from whoever
    // earns it.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let (connections, state) = shared(&db);

    tick(&db, &connections, &state, 1000);
    assert!(
        db.list_world_milestones().unwrap().is_empty(),
        "a fresh install unlocked: {:?}",
        db.list_world_milestones()
            .unwrap()
            .iter()
            .map(|m| m.key.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn crossing_a_threshold_records_it_once_with_the_builders_who_were_there() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let (connections, state) = shared(&db);

    area_with_rooms(&db, "big", "Ana", 100);
    tick(&db, &connections, &state, 5000);

    let recorded = db.list_world_milestones().unwrap();
    let hundred = recorded
        .iter()
        .find(|m| m.key == "world_hundred_rooms")
        .expect("a hundred rooms did not register");
    assert_eq!(hundred.unlocked_at, 5000);
    assert_eq!(hundred.contributors, vec!["Ana".to_string()]);

    // Running again changes nothing — the date and the credit are the record.
    tick(&db, &connections, &state, 9999);
    let again = db.get_world_milestone("world_hundred_rooms").unwrap().unwrap();
    assert_eq!(again.unlocked_at, 5000, "a re-run rewrote the date");
    assert_eq!(again.contributors, vec!["Ana".to_string()]);
}

#[test]
fn a_milestone_stays_recorded_even_if_the_world_shrinks_back() {
    // The world *did* pass a hundred rooms. Deleting them afterwards does not
    // un-happen it, and the builder score already handles the "you no longer
    // own this" half.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let (connections, state) = shared(&db);

    area_with_rooms(&db, "big", "Ana", 100);
    tick(&db, &connections, &state, 5000);
    assert!(db.get_world_milestone("world_hundred_rooms").unwrap().is_some());

    for r in db.list_all_rooms().unwrap() {
        db.delete_room(&r.id).unwrap();
    }
    tick(&db, &connections, &state, 6000);
    assert!(
        db.get_world_milestone("world_hundred_rooms").unwrap().is_some(),
        "shrinking the world erased its history"
    );
}

#[test]
fn an_imported_world_does_not_unlock_milestones_nobody_earned() {
    // `WorldFacts` counts content of every origin, because the size of the
    // world is the size of the world. `contributors` is origin-gated. Import a
    // CircleMUD world and the two disagree completely: a dozen milestones met
    // on the first tick, all announced, all permanently consumed, all credited
    // to nobody. Milestones wait for a builder to be on the board — the first
    // hand-built room releases them.
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    let area_id = Uuid::new_v4();
    db.save_area_data(serde_json::from_value(json!({"id": area_id, "name": "Imported", "prefix": "imp"})).unwrap())
        .unwrap();
    for i in 0..110 {
        let mut r: RoomData = serde_json::from_value(json!({
            "id": Uuid::new_v4(),
            "title": format!("Room {i}"),
            "description": "A room that came in from somewhere else entirely, with enough words in it.",
            "exits": {},
            "area_id": area_id,
        }))
        .unwrap();
        r.origin = ContentOrigin::Import;
        db.save_room_data(r).unwrap();
    }

    tick(&db, &connections, &state, 7000);
    assert!(
        db.get_world_milestone("world_hundred_rooms").unwrap().is_none(),
        "an import unlocked a milestone with nobody to credit for it"
    );

    // One builder with one scored room is enough to release the wall: the
    // world really does have those rooms, and now somebody is building it.
    builder(&db, "Ana");
    area_with_rooms(&db, "ana", "Ana", 1);
    tick(&db, &connections, &state, 7100);
    let m = db
        .get_world_milestone("world_hundred_rooms")
        .unwrap()
        .expect("a builder on the board should release the milestone");
    assert_eq!(m.contributors, vec!["Ana".to_string()]);
}

#[test]
fn the_wall_shows_progress_toward_the_ones_not_yet_reached() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let (connections, state) = shared(&db);
    area_with_rooms(&db, "mid", "Ana", 60);
    tick(&db, &connections, &state, 1000);

    let facts = WorldSnapshot::load(&db).unwrap().facts();
    let rating = rating_of(&db);
    let wall = ironmud::world_milestones::wall(&db, &facts, &rating).unwrap();

    let hundred = wall.iter().find(|r| r.key == "world_hundred_rooms").unwrap();
    assert!(hundred.unlocked_at.is_none());
    assert_eq!(hundred.have, 60);
    assert_eq!(hundred.want, 100);
    assert_eq!(wall.len(), world_rating::WORLD_GOALS.len());
}

// ===========================================================================
// Housekeeping
// ===========================================================================

#[test]
fn every_world_goal_has_a_definition_to_award() {
    // A goal with no matching `AchievementDef` would record on the wall and
    // then silently fail to credit anybody, because the award path reads the
    // registry by key.
    use ironmud::types::{AchievementCriterion, AchievementDef};

    let content = std::fs::read_to_string("scripts/data/achievements/world.json").expect("world.json");
    let defs: Vec<AchievementDef> = serde_json::from_str(&content).expect("parse world.json");
    let keys: Vec<&str> = defs.iter().map(|d| d.key.as_str()).collect();

    for (key, _) in world_rating::WORLD_GOALS {
        assert!(keys.contains(key), "{key} has no definition in world.json");
    }
    assert_eq!(defs.len(), world_rating::WORLD_GOALS.len(), "world.json has strays");

    for def in &defs {
        // Manual, or the counter-driven engine would try to award them too.
        assert!(
            matches!(def.criterion, AchievementCriterion::Manual),
            "{} is not a manual criterion",
            def.key
        );
        assert_eq!(def.reward.trait_points, 0, "{} pays trait points", def.key);
    }
}

#[test]
fn the_tick_installs_the_rating_where_the_command_reads_it() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    area_with_rooms(&db, "home", "Ana", 15);

    assert!(!state.lock().unwrap().world_report.is_ready());
    tick(&db, &connections, &state, 4242);

    let world = state.lock().unwrap();
    assert!(world.world_report.is_ready());
    assert_eq!(world.world_report.generated_at, 4242);
    assert_eq!(world.world_report.facts.room_count, 15);
    assert!(world.world_report.rating.is_some());
}

#[test]
fn the_rating_and_the_scores_come_from_one_scan() {
    // Both halves are installed by the same tick, so they cannot describe
    // different moments. A builder reading "you own 40 rooms" beside "the
    // world has 12" would have no way to tell which was stale.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let (connections, state) = shared(&db);
    area_with_rooms(&db, "home", "Ana", 30);
    tick(&db, &connections, &state, 8000);

    let world = state.lock().unwrap();
    assert_eq!(world.build_scores.generated_at, world.world_report.generated_at);
    assert_eq!(
        world
            .build_scores
            .get("Ana")
            .unwrap()
            .tally(ironmud::types::ContentKind::Room)
            .count,
        world.world_report.facts.room_count
    );
}

#[test]
fn milestone_evaluation_is_cheap_to_repeat() {
    // It runs every tick forever, so the no-op path has to be a read and
    // nothing else. This asserts behaviour, not timing: a second run over an
    // unchanged world must write nothing.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let (connections, state) = shared(&db);
    area_with_rooms(&db, "big", "Ana", 100);

    let facts = WorldSnapshot::load(&db).unwrap().facts();
    let rating = rating_of(&db);
    let scores = build_score::compute(&WorldSnapshot::load(&db).unwrap(), &HashMap::new(), 1);

    let first = ironmud::world_milestones::evaluate(&db, &connections, &state, &facts, &rating, &scores, 100).unwrap();
    assert!(!first.is_empty());
    let second = ironmud::world_milestones::evaluate(&db, &connections, &state, &facts, &rating, &scores, 200).unwrap();
    assert!(second.is_empty(), "a repeat run unlocked {second:?} again");
}

// ===========================================================================
// The command, driven for real
// ===========================================================================

fn run_world_command(db: &Db, args: &str) -> String {
    use ironmud::{PlayerSession, World};
    use std::sync::{Arc, Mutex};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());

    // The command reads a cache, so the tick has to have run.
    build_score::process_build_score_tick(db, &connections, &state, 1000).expect("tick");

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": "Ana", "password_hash": "", "is_builder": true, "current_room_id": Uuid::nil(),
        }))
        .unwrap(),
    );
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
        .compile_file("scripts/commands/world.rhai".into())
        .unwrap_or_else(|e| panic!("compile world.rhai: {e}"));

    let mut scope = rhai::Scope::new();
    let result: Result<(), _> =
        engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    result.unwrap_or_else(|e| panic!("world.rhai run_command: {e}"));

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn the_world_command_renders_the_rating_and_names_the_cap() {
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let out = run_world_command(&db, "");
    assert!(out.contains("The World"), "unexpected output:\n{out}");
    assert!(out.contains("What it is made of"), "components missing:\n{out}");
    assert!(out.contains("Held at"), "the cap was not explained:\n{out}");
}

#[test]
fn the_world_command_renders_the_wall_with_progress() {
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let out = run_world_command(&db, "milestones");
    assert!(out.contains("World Milestones"), "unexpected output:\n{out}");
    assert!(out.contains("Ahead"), "the unreached milestones are missing:\n{out}");
    assert!(out.contains(" / "), "no progress figures:\n{out}");
    assert!(
        out.contains("Village / Town"),
        "tier progress read as raw indices:\n{out}"
    );
}
