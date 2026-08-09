//! Attribution and provenance integration tests.
//!
//! The rules themselves are unit-tested in `src/types/provenance.rs`. These
//! exercise them against a real database, through the entry points the four
//! stamping surfaces actually call — and, most importantly, pin the guard that
//! the whole thing exists for: **one import must never be able to claim
//! authorship of a world.**

#![recursion_limit = "256"]

use ironmud::attribution::{self, ContentRef};
use ironmud::db::Db;
use ironmud::types::{Authored, ContentKind, ContentOrigin, RoomData};
use serde_json::json;
use uuid::Uuid;

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn save_room(db: &Db, title: &str) -> Uuid {
    let id = Uuid::new_v4();
    let room: RoomData = serde_json::from_value(json!({
        "id": id,
        "title": title,
        "description": "A room.",
        "exits": {},
    }))
    .expect("room fixture");
    db.save_room_data(room).expect("save room");
    id
}

// ===========================================================================
// The stamping rules, through the database
// ===========================================================================

#[test]
fn a_fresh_row_is_unattributed_and_counts_for_nothing() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "Nowhere");
    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Unknown);
    assert_eq!(p.authored_by, None);
    assert!(!p.origin.counts_for_score());
}

#[test]
fn creating_claims_it_and_marks_it_builder_origin() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "The Iron Gate");
    assert!(attribution::stamp_created(&db, &ContentRef::Room(id), "Ana").unwrap());

    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("Ana"));
    assert_eq!(p.last_edited_by.as_deref(), Some("Ana"));
    assert_eq!(p.origin, ContentOrigin::Builder);
    assert!(p.origin.counts_for_score());
}

#[test]
fn editing_someone_elses_room_does_not_take_it_from_them() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "The Iron Gate");
    attribution::stamp_created(&db, &ContentRef::Room(id), "Ana").unwrap();
    attribution::stamp_edited(&db, &ContentRef::Room(id), "Bo").unwrap();

    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("Ana"), "an edit reassigned authorship");
    assert_eq!(p.last_edited_by.as_deref(), Some("Bo"));
}

#[test]
fn editing_seed_content_never_converts_it_into_your_work() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "Town Square");
    attribution::stamp_unattributed(&db, ContentOrigin::Seed).unwrap();
    attribution::stamp_edited(&db, &ContentRef::Room(id), "Ana").unwrap();

    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Seed);
    assert_eq!(p.authored_by, None);
    assert!(!p.origin.counts_for_score());
}

#[test]
fn stamping_something_that_does_not_exist_is_a_no_op_not_an_error() {
    let (db, _t) = fresh_db();
    assert!(!attribution::stamp_created(&db, &ContentRef::Room(Uuid::new_v4()), "Ana").unwrap());
    assert!(!attribution::stamp_edited(&db, &ContentRef::Quest("nope".into()), "Ana").unwrap());
    assert!(
        attribution::read(&db, &ContentRef::Room(Uuid::new_v4()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_blank_builder_name_claims_nothing() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "The Iron Gate");
    assert!(!attribution::stamp_created(&db, &ContentRef::Room(id), "   ").unwrap());
    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.authored_by, None);
}

#[test]
fn content_refs_parse_by_kind_and_reject_a_bad_uuid() {
    let id = Uuid::new_v4();
    assert_eq!(ContentRef::parse("room", &id.to_string()), Some(ContentRef::Room(id)));
    assert_eq!(ContentRef::parse("mob", &id.to_string()), Some(ContentRef::Mobile(id)));
    // Quests key by vnum, so any string is valid.
    assert_eq!(
        ContentRef::parse("quest", "oakvale:errand"),
        Some(ContentRef::Quest("oakvale:errand".into()))
    );
    // A uuid-keyed kind given a vnum must not silently resolve to something.
    assert_eq!(ContentRef::parse("room", "oakvale:square"), None);
    assert_eq!(ContentRef::parse("nonsense", &id.to_string()), None);
}

// ===========================================================================
// Claiming an area
// ===========================================================================

fn save_area(db: &Db, name: &str, prefix: &str) -> Uuid {
    let id = Uuid::new_v4();
    let area = serde_json::from_value(json!({"id": id, "name": name, "prefix": prefix})).expect("area fixture");
    db.save_area_data(area).expect("save area");
    id
}

fn save_room_in(db: &Db, area_id: Option<Uuid>, title: &str) -> Uuid {
    let id = Uuid::new_v4();
    let room: RoomData = serde_json::from_value(json!({
        "id": id,
        "title": title,
        "description": "A room.",
        "exits": {},
        "area_id": area_id,
    }))
    .expect("room fixture");
    db.save_room_data(room).expect("save room");
    id
}

#[test]
fn claiming_an_area_takes_its_unattributed_rows_and_nothing_else() {
    let (db, _t) = fresh_db();
    let area = save_area(&db, "Dungeon Crawl Level 1", "dungeon1");
    let other = save_area(&db, "Midgaard", "midgaard");

    let mine = save_room_in(&db, Some(area), "The Entrance");
    let theirs = save_room_in(&db, Some(area), "The Vault");
    let elsewhere = save_room_in(&db, Some(other), "Market Square");
    let unhomed = save_room_in(&db, None, "Limbo");
    attribution::stamp_created(&db, &ContentRef::Room(theirs), "Bo").unwrap();

    let out = attribution::claim_area(&db, area, "zombieCraig").unwrap();
    // The area row itself plus the one unattributed room in it.
    assert_eq!(out.claimed.areas, 1);
    assert_eq!(out.claimed.rooms, 1);
    assert_eq!(out.already_credited, 1);

    let p = attribution::read(&db, &ContentRef::Room(mine)).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("zombieCraig"));
    assert_eq!(p.origin, ContentOrigin::Builder, "a claim must make it count");

    let p = attribution::read(&db, &ContentRef::Room(theirs)).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("Bo"), "a claim reassigned authorship");

    for outside in [elsewhere, unhomed] {
        let p = attribution::read(&db, &ContentRef::Room(outside)).unwrap().unwrap();
        assert_eq!(p.authored_by, None, "a claim reached outside its area");
    }
}

#[test]
fn claiming_never_takes_seed_or_imported_content() {
    // The anti-cheat. `stamp_created` sets Builder origin, and Builder origin
    // is what scores — so if a claim were allowed to touch imported rows, one
    // `build claim` over an imported area would be a cheat code.
    let (db, _t) = fresh_db();
    let area = save_area(&db, "Imported Zone", "imported");
    let room = save_room_in(&db, Some(area), "A Translated Room");
    attribution::stamp_unattributed(&db, ContentOrigin::Import).unwrap();

    let out = attribution::claim_area(&db, area, "zombieCraig").unwrap();
    assert_eq!(out.claimed.total(), 0);
    assert_eq!(out.skipped_import, 2, "the area row and the room");

    let p = attribution::read(&db, &ContentRef::Room(room)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Import);
    assert_eq!(p.authored_by, None);
    assert!(!p.origin.counts_for_score());
}

#[test]
fn claiming_twice_claims_nothing_the_second_time() {
    let (db, _t) = fresh_db();
    let area = save_area(&db, "Dungeon Crawl Level 1", "dungeon1");
    save_room_in(&db, Some(area), "The Entrance");

    let first = attribution::claim_area(&db, area, "zombieCraig").unwrap();
    let second = attribution::claim_area(&db, area, "zombieCraig").unwrap();
    assert_eq!(first.claimed.total(), 2);
    assert_eq!(second.claimed.total(), 0);
    assert_eq!(second.already_credited, 2);
}

#[test]
fn a_blank_builder_name_claims_nothing_from_an_area() {
    let (db, _t) = fresh_db();
    let area = save_area(&db, "Dungeon Crawl Level 1", "dungeon1");
    let room = save_room_in(&db, Some(area), "The Entrance");

    let out = attribution::claim_area(&db, area, "  ").unwrap();
    assert_eq!(out.claimed.total(), 0);
    let p = attribution::read(&db, &ContentRef::Room(room)).unwrap().unwrap();
    assert_eq!(p.authored_by, None);
}

// ===========================================================================
// The guard the whole feature exists for
// ===========================================================================

#[test]
fn the_seeded_demo_world_is_engine_content_and_credits_nobody() {
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");

    let rooms = db.list_all_rooms().unwrap();
    assert!(!rooms.is_empty());
    for r in &rooms {
        assert_eq!(r.origin, ContentOrigin::Seed, "room {:?} was not stamped", r.vnum);
        assert_eq!(r.authored_by, None);
        assert!(!r.origin.counts_for_score());
    }
    for m in db.list_all_mobiles().unwrap() {
        assert_eq!(m.origin, ContentOrigin::Seed);
    }
    for i in db.list_all_items().unwrap() {
        assert_eq!(i.origin, ContentOrigin::Seed);
    }
    for a in db.list_all_areas().unwrap() {
        assert_eq!(a.origin, ContentOrigin::Seed);
    }
}

#[test]
fn a_bulk_origin_pass_cannot_take_credit_away_from_a_builder() {
    // The scenario: a builder has been working, and then somebody runs an
    // import (or re-runs the seed guard) over the same world.
    let (db, _t) = fresh_db();
    let mine = save_room(&db, "My Room");
    let legacy = save_room(&db, "Legacy Room");
    attribution::stamp_created(&db, &ContentRef::Room(mine), "Ana").unwrap();

    let counts = attribution::stamp_unattributed(&db, ContentOrigin::Import).unwrap();
    assert_eq!(counts.rooms, 1, "only the unattributed row should have moved");

    let p = attribution::read(&db, &ContentRef::Room(mine)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Builder);
    assert_eq!(p.authored_by.as_deref(), Some("Ana"));

    let p = attribution::read(&db, &ContentRef::Room(legacy)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Import);
}

#[test]
fn a_bulk_origin_pass_is_idempotent() {
    let (db, _t) = fresh_db();
    save_room(&db, "A Room");
    let first = attribution::stamp_unattributed(&db, ContentOrigin::Seed).unwrap();
    let second = attribution::stamp_unattributed(&db, ContentOrigin::Seed).unwrap();
    assert_eq!(first.rooms, 1);
    assert_eq!(second.total(), 0, "the second pass should have nothing to do");
}

#[test]
fn seeding_twice_does_not_restamp_anything() {
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    let id = save_room(&db, "A Builder Room");
    attribution::stamp_created(&db, &ContentRef::Room(id), "Ana").unwrap();

    // The seed guard refuses a non-empty world, so this is a no-op — but the
    // assertion is about what happens if that guard ever changes.
    ironmud::seed::seed_demo_world(&db).expect("seed again");
    let p = attribution::read(&db, &ContentRef::Room(id)).unwrap().unwrap();
    assert_eq!(p.origin, ContentOrigin::Builder);
    assert_eq!(p.authored_by.as_deref(), Some("Ana"));
}

// ===========================================================================
// The OLC surface, driven for real
// ===========================================================================

/// Run one OLC command through its actual Rhai script.
///
/// The stamps are six call sites spread across six editor scripts, and a
/// missing one is invisible until a builder notices their work credits nobody.
/// Nothing static catches that, so it is driven here.
fn run_olc(db: &Db, command: &str, args: &str) -> String {
    let room_id = db
        .list_all_rooms()
        .expect("rooms")
        .into_iter()
        .next()
        .map(|r| r.id)
        .unwrap_or_else(Uuid::new_v4);
    run_olc_as(db, command, args, "Ana", room_id, false)
}

/// [`run_olc`] as a named builder, standing somewhere specific.
///
/// `build claim` needs all three: it defaults its key to the area of the room
/// you are in, gates on `build_mode`, and authorises against the area owner by
/// name.
fn run_olc_as(db: &Db, command: &str, args: &str, who: &str, room_id: Uuid, build_mode: bool) -> String {
    use ironmud::{PlayerSession, World};
    use std::sync::{Arc, Mutex};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": who,
            "password_hash": "",
            "is_builder": true,
            "build_mode": build_mode,
            "current_room_id": room_id,
        }))
        .expect("character"),
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
        .compile_file(format!("scripts/commands/{command}.rhai").into())
        .unwrap_or_else(|e| panic!("compile {command}.rhai: {e}"));

    let mut scope = rhai::Scope::new();
    let result: Result<(), _> =
        engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    result.unwrap_or_else(|e| panic!("{command}.rhai run_command: {e}"));

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn redit_create_claims_the_room_for_the_builder() {
    let (db, _t) = fresh_db();
    let before: std::collections::HashSet<Uuid> = db.list_all_rooms().unwrap().into_iter().map(|r| r.id).collect();

    run_olc(&db, "redit", "create The Iron Gate");

    let made: Vec<RoomData> = db
        .list_all_rooms()
        .unwrap()
        .into_iter()
        .filter(|r| !before.contains(&r.id))
        .collect();
    assert_eq!(made.len(), 1, "redit create did not make exactly one room");
    assert_eq!(made[0].authored_by.as_deref(), Some("Ana"));
    assert_eq!(made[0].origin, ContentOrigin::Builder);
}

#[test]
fn oedit_and_medit_create_claim_their_prototypes() {
    let (db, _t) = fresh_db();

    run_olc(&db, "oedit", "create Rusty Pike");
    let item = db
        .list_all_items()
        .unwrap()
        .into_iter()
        .find(|i| i.is_prototype)
        .expect("oedit create made no prototype");
    assert_eq!(item.authored_by.as_deref(), Some("Ana"));
    assert_eq!(item.origin, ContentOrigin::Builder);

    run_olc(&db, "medit", "create goblin guard");
    let mob = db
        .list_all_mobiles()
        .unwrap()
        .into_iter()
        .find(|m| m.is_prototype)
        .expect("medit create made no prototype");
    assert_eq!(mob.authored_by.as_deref(), Some("Ana"));
    assert_eq!(mob.origin, ContentOrigin::Builder);
}

#[test]
fn quedit_create_claims_the_quest_and_editing_records_the_edit() {
    let (db, _t) = fresh_db();
    run_olc(&db, "quedit", "create oak:errand Fetch the Ledger");

    let q = db.get_quest_data("oak:errand").unwrap().expect("quest not created");
    assert_eq!(q.authored_by.as_deref(), Some("Ana"));
    assert_eq!(q.origin, ContentOrigin::Builder);

    run_olc(&db, "quedit", "oak:errand summary Find the missing ledger.");
    let q = db.get_quest_data("oak:errand").unwrap().unwrap();
    assert_eq!(q.last_edited_by.as_deref(), Some("Ana"));
    assert_eq!(q.summary, "Find the missing ledger.");
}

#[test]
fn acreate_claims_the_area() {
    let (db, _t) = fresh_db();
    run_olc(&db, "acreate", "gate The Iron Gate");

    let area = db
        .list_all_areas()
        .unwrap()
        .into_iter()
        .find(|a| a.prefix == "gate")
        .expect("acreate made no area");
    assert_eq!(area.authored_by.as_deref(), Some("Ana"));
    assert_eq!(area.origin, ContentOrigin::Builder);
    assert_eq!(area.owner.as_deref(), Some("Ana"));
}

#[test]
fn viewing_content_is_not_touching_it() {
    // `redit` and `quedit` with no subcommand (or `show`) must not move
    // `last_edited_by` — otherwise every builder who looks at a room appears
    // in its history, and the field stops meaning anything.
    let (db, _t) = fresh_db();
    run_olc(&db, "quedit", "create oak:errand Fetch the Ledger");
    let db2 = &db;
    attribution::stamp_edited(db2, &ContentRef::Quest("oak:errand".into()), "Bo").unwrap();

    run_olc(&db, "quedit", "oak:errand show");
    let q = db.get_quest_data("oak:errand").unwrap().unwrap();
    assert_eq!(
        q.last_edited_by.as_deref(),
        Some("Bo"),
        "a read-only subcommand recorded an edit"
    );
}

// ===========================================================================
// Serde back-compat
// ===========================================================================

#[test]
fn rows_written_before_provenance_existed_still_load() {
    // No `origin`, no `authored_by`, no `last_edited_by` — exactly what every
    // row in an existing database looks like.
    let room: RoomData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "title": "Old Room",
        "description": "Written before any of this existed.",
        "exits": {},
    }))
    .expect("a pre-provenance row must still deserialise");
    assert_eq!(room.origin, ContentOrigin::Unknown);
    assert_eq!(room.authored_by, None);
    assert_eq!(room.provenance().origin, ContentOrigin::Unknown);
}

#[test]
fn provenance_survives_a_round_trip_through_sled() {
    let (db, _t) = fresh_db();
    let id = save_room(&db, "The Iron Gate");
    attribution::stamp_created(&db, &ContentRef::Room(id), "Ana").unwrap();
    attribution::stamp_edited(&db, &ContentRef::Room(id), "Bo").unwrap();

    let room = db.get_room_data(&id).unwrap().unwrap();
    let json = serde_json::to_value(&room).unwrap();
    assert_eq!(json["authored_by"], "Ana");
    assert_eq!(json["last_edited_by"], "Bo");
    assert_eq!(json["origin"], "builder");

    let back: RoomData = serde_json::from_value(json).unwrap();
    assert_eq!(back.provenance(), room.provenance());
}

#[test]
fn every_content_kind_can_be_stamped_and_read_back() {
    // A kind that cannot round-trip is a kind the score will silently skip.
    let (db, _t) = fresh_db();
    let mut refs: Vec<ContentRef> = Vec::new();

    let room_id = save_room(&db, "Room");
    refs.push(ContentRef::Room(room_id));

    let item_id = Uuid::new_v4();
    db.save_item_data(
        serde_json::from_value(
            json!({"id": item_id, "name": "thing", "short_desc": "a thing", "long_desc": "A thing."}),
        )
        .unwrap(),
    )
    .unwrap();
    refs.push(ContentRef::Item(item_id));

    let mob_id = Uuid::new_v4();
    db.save_mobile_data(
        serde_json::from_value(json!({"id": mob_id, "name": "a mob", "short_desc": "a mob", "long_desc": "A mob."}))
            .unwrap(),
    )
    .unwrap();
    refs.push(ContentRef::Mobile(mob_id));

    let area_id = Uuid::new_v4();
    db.save_area_data(serde_json::from_value(json!({"id": area_id, "name": "Area", "prefix": "ar"})).unwrap())
        .unwrap();
    refs.push(ContentRef::Area(area_id));

    db.save_quest_data(
        &serde_json::from_value(json!({"vnum": "q1", "name": "Q", "summary": "s", "description": "",
                                       "completion_text": "", "objectives": [], "rewards": []}))
        .unwrap(),
    )
    .unwrap();
    refs.push(ContentRef::Quest("q1".into()));

    assert_eq!(refs.len(), ContentKind::ALL.len(), "a content kind is untested");

    for r in &refs {
        assert!(
            attribution::stamp_created(&db, r, "Ana").unwrap(),
            "could not stamp {r:?}"
        );
        let p = attribution::read(&db, r).unwrap().unwrap();
        assert_eq!(p.authored_by.as_deref(), Some("Ana"), "{r:?} lost its author");
        assert_eq!(p.origin, ContentOrigin::Builder, "{r:?} lost its origin");
    }
}

#[test]
fn stamp_created_does_not_take_content_that_already_names_a_builder() {
    // Every call site today is a genuine creation, so this is a guard rather
    // than a live bug — but `stamp_created` is the one function that *can*
    // reassign authorship, and that safety used to live entirely in its
    // callers. A create-or-replace endpoint added later would have become
    // credit theft with no diff to `provenance.rs`.
    let (db, _t) = fresh_db();
    let id = save_room(&db, "The Iron Gate");
    let target = ContentRef::Room(id);

    attribution::stamp_created(&db, &target, "Ana").unwrap();
    attribution::stamp_created(&db, &target, "Bo").unwrap();

    let p = attribution::read(&db, &target).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("Ana"), "a second create took the room");
}

#[test]
fn build_credits_names_the_builders_and_counts_what_is_unattributed() {
    // The relatedness surface. Attribution shipped before anything displayed
    // it, and `get_content_credit` sat registered with no caller at all.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");

    // One hand-built room in a seeded area, so the listing has both a credited
    // builder and a pile of engine content to report as unattributed.
    let area = db.list_all_areas().unwrap().into_iter().next().expect("an area");
    let mut room: RoomData = serde_json::from_value(json!({
        "id": Uuid::new_v4(),
        "title": "The Iron Gate",
        "description": "Iron, and a gate.",
        "exits": {},
        "area_id": area.id,
    }))
    .unwrap();
    room.authored_by = Some("Ana".into());
    room.origin = ContentOrigin::Builder;
    db.save_room_data(room).unwrap();

    let out = run_olc(&db, "build", &format!("credits {}", area.prefix));
    assert!(out.contains("Credits"), "no header:\n{out}");
    assert!(out.contains("Ana"), "the builder is not named:\n{out}");
    assert!(out.contains("unattributed"), "seed content not accounted for:\n{out}");

    // And the world-wide form works without standing anywhere.
    let world = run_olc(&db, "build", "credits world");
    assert!(world.contains("Credits"), "world credits did not render:\n{world}");
}

#[test]
fn build_claim_credits_the_owner_for_the_area_they_are_standing_in() {
    // End to end through the real script, because the binding is resolved at
    // call time: a typo in `claim_area_content` compiles fine and fails only
    // when a builder types the command.
    let (db, _t) = fresh_db();
    let area_id = save_area(&db, " Dungeon Crawl Level 1", "dungeon1");
    let mut area = db.get_area_data(&area_id).unwrap().unwrap();
    area.owner = Some("zombieCraig".into());
    db.save_area_data(area).unwrap();
    let room = save_room_in(&db, Some(area_id), "The Entrance");

    let out = run_olc_as(&db, "build", "claim", "zombieCraig", room, true);
    assert!(out.contains("Claim"), "no header:\n{out}");

    let p = attribution::read(&db, &ContentRef::Room(room)).unwrap().unwrap();
    assert_eq!(p.authored_by.as_deref(), Some("zombieCraig"), "{out}");
    assert_eq!(p.origin, ContentOrigin::Builder);
}

#[test]
fn build_claim_refuses_someone_who_does_not_own_the_area() {
    let (db, _t) = fresh_db();
    let area_id = save_area(&db, "Dungeon Crawl Level 1", "dungeon1");
    let mut area = db.get_area_data(&area_id).unwrap().unwrap();
    area.owner = Some("zombieCraig".into());
    db.save_area_data(area).unwrap();
    let room = save_room_in(&db, Some(area_id), "The Entrance");

    let out = run_olc_as(&db, "build", "claim", "Interloper", room, true);
    assert!(out.contains("zombieCraig"), "the refusal should name the owner:\n{out}");

    let p = attribution::read(&db, &ContentRef::Room(room)).unwrap().unwrap();
    assert_eq!(p.authored_by, None, "a non-owner claimed the area:\n{out}");
}

#[test]
fn build_claim_refuses_an_unowned_area() {
    let (db, _t) = fresh_db();
    let area_id = save_area(&db, "Dungeon Crawl Level 1", "dungeon1");
    let room = save_room_in(&db, Some(area_id), "The Entrance");

    let out = run_olc_as(&db, "build", "claim", "Opportunist", room, true);
    assert!(out.contains("no owner"), "{out}");

    let p = attribution::read(&db, &ContentRef::Room(room)).unwrap().unwrap();
    assert_eq!(p.authored_by, None, "an unowned area was claimable:\n{out}");
}

#[test]
fn build_audit_area_resolves_the_area_you_are_standing_in() {
    // The reported bug: the key defaulted to the area *name*, and this area's
    // name has a leading space, so the lookup missed and the command said
    // "No area matches ' Dungeon Crawl Level 1'."
    let (db, _t) = fresh_db();
    let area_id = save_area(&db, " Dungeon Crawl Level 1", "dungeon1");
    let room = save_room_in(&db, Some(area_id), "The Entrance");

    for args in ["audit area", "audit area dungeon1", "audit area Dungeon Crawl Level 1"] {
        let out = run_olc_as(&db, "build", args, "zombieCraig", room, true);
        assert!(!out.contains("No area matches"), "`build {args}` failed:\n{out}");
    }
}
