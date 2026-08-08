//! Bounty board integration tests.
//!
//! The lifecycle is a small state machine with money at the end of it, so most
//! of these are about the edges: who may judge, what a rejection does, whether
//! a claim can be squatted, and whether an accept can be made to pay twice.
//!
//! The generated half has its own hazard. Auditor requests are regenerated on
//! every tick forever, so the two properties that matter are that a repeat run
//! creates nothing and that fixing the underlying problem closes the request
//! whether or not anybody read the board.

#![recursion_limit = "256"]

use std::sync::{Arc, Mutex};

use ironmud::audit::scan::WorldSnapshot;
use ironmud::bounty::{self, Outcome};
use ironmud::db::Db;
use ironmud::types::{CharacterData, ContentKind, ContentOrigin, RequestOrigin, RequestStatus, RoomData};
use serde_json::json;
use uuid::Uuid;

fn fresh_db() -> (Db, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (db, temp)
}

fn shared(db: &Db) -> (ironmud::SharedConnections, ironmud::SharedState) {
    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
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

fn points_of(db: &Db, name: &str) -> i32 {
    db.get_character_data(name).unwrap().unwrap().builder_bounty_points
}

fn post(db: &Db, requester: &str, points: i32) -> i64 {
    bounty::post(
        db,
        requester,
        None,
        None,
        "",
        "A blacksmith",
        "with a dialogue tree",
        points,
        1000,
    )
    .unwrap()
    .ticket_number
}

// ===========================================================================
// The lifecycle
// ===========================================================================

#[test]
fn a_bounty_runs_post_claim_submit_accept_and_pays_once() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    builder(&db, "Ana");
    builder(&db, "Bo");

    let ticket = post(&db, "Ana", 60);
    assert!(bounty::claim(&db, ticket, "Bo", 2000).unwrap().is_ok());
    assert!(
        bounty::submit(&db, ticket, "Bo", vec!["oak:smith".into()], 3000)
            .unwrap()
            .is_ok()
    );
    assert!(
        bounty::accept(&db, &connections, &state, ticket, "Ana", false, 4000)
            .unwrap()
            .is_ok()
    );

    assert_eq!(points_of(&db, "Bo"), 60);
    assert_eq!(points_of(&db, "Ana"), 0, "the requester paid themselves");

    // Accepting again must not pay again.
    let again = bounty::accept(&db, &connections, &state, ticket, "Ana", false, 5000).unwrap();
    assert!(!again.is_ok());
    assert_eq!(points_of(&db, "Bo"), 60, "a second accept paid out twice");
}

#[test]
fn accepted_points_reach_the_builder_score() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    builder(&db, "Ana");
    builder(&db, "Bo");

    let ticket = post(&db, "Ana", 45);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec![], 3000).unwrap();
    bounty::accept(&db, &connections, &state, ticket, "Ana", false, 4000).unwrap();

    ironmud::build_score::process_build_score_tick(&db, &connections, &state, 5000).unwrap();
    let world = state.lock().unwrap();
    let bo = world.build_scores.get("Bo").expect("Bo has a score");
    assert_eq!(bo.bounty_points, 45);
    assert_eq!(bo.total, 45);
    assert_eq!(bo.content_points, 0, "Bo built nothing, only claimed");
}

#[test]
fn only_the_requester_or_an_admin_can_accept() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    builder(&db, "Ana");
    builder(&db, "Bo");

    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec![], 3000).unwrap();

    // The claimant judging their own work is the whole reason this check
    // exists.
    let self_paid = bounty::accept(&db, &connections, &state, ticket, "Bo", false, 4000).unwrap();
    assert!(matches!(self_paid, Outcome::Denied(_)));
    assert_eq!(points_of(&db, "Bo"), 0);

    assert!(
        bounty::accept(&db, &connections, &state, ticket, "Zeus", true, 4000)
            .unwrap()
            .is_ok(),
        "an admin could not unstick it"
    );
}

#[test]
fn nobody_can_post_claim_and_accept_their_own_bounty() {
    // The one loop that pays forever. Posting your own work and doing it
    // yourself is allowed — that is real work — but signing it off is not, or
    // post → claim → submit → accept is 500 builder points a turn with no
    // second person anywhere in it.
    //
    // Not exempt for admins: on most servers a builder *is* an admin, so an
    // admin exemption would leave the loop open for exactly the people who can
    // reach it.
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    builder(&db, "Ana");
    let mut ch = db.get_character_data("Ana").unwrap().unwrap();
    ch.is_admin = true;
    db.save_character_data(ch).unwrap();

    let ticket = post(&db, "Ana", 500);
    assert!(bounty::claim(&db, ticket, "Ana", 2000).unwrap().is_ok());
    assert!(bounty::submit(&db, ticket, "Ana", vec![], 3000).unwrap().is_ok());

    for is_admin in [false, true] {
        let outcome = bounty::accept(&db, &connections, &state, ticket, "Ana", is_admin, 4000).unwrap();
        assert!(
            matches!(outcome, Outcome::Denied(_)),
            "signed off on their own work (is_admin={is_admin})"
        );
    }
    assert_eq!(points_of(&db, "Ana"), 0, "paid themselves");

    // A second person can still sign it off, which is the point.
    assert!(
        bounty::accept(&db, &connections, &state, ticket, "Zeus", true, 5000)
            .unwrap()
            .is_ok()
    );
    assert_eq!(points_of(&db, "Ana"), 500);
}

#[test]
fn dropping_submitted_work_takes_the_credit_with_it() {
    // `reject` always cleared `fulfilled_by`; `drop` did not, so open work went
    // back on the board still rendering "Submitted by Bo".
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    builder(&db, "Bo");

    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec!["oak:1".into()], 3000).unwrap();
    assert!(bounty::drop_claim(&db, ticket, "Bo", false, 4000).unwrap().is_ok());

    let r = db.get_build_request_by_ticket(ticket).unwrap().unwrap();
    assert_eq!(r.status, RequestStatus::Open);
    assert_eq!(r.claimed_by, None);
    assert_eq!(r.fulfilled_by, None, "open work still credited a claimant");
    assert!(r.linked.is_empty());
}

#[test]
fn two_posts_never_share_a_ticket_number() {
    // `max(existing) + 1` alone hands a deleted ticket's number out again, and
    // `get_build_request_by_ticket` is a `.find()` — the loser of a collision
    // becomes permanently unreachable: unshowable, unclaimable, unpayable.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");

    let first = post(&db, "Ana", 10);
    let second = post(&db, "Ana", 10);
    assert_ne!(first, second);

    let doomed = db.get_build_request_by_ticket(second).unwrap().unwrap();
    db.delete_build_request(&doomed.id).unwrap();

    let third = post(&db, "Ana", 10);
    assert!(
        third > second,
        "deleting the top ticket recycled its number ({third} after {second})"
    );
}

#[test]
fn rejecting_returns_the_work_to_the_board_with_a_note() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec!["x".into()], 3000).unwrap();

    assert!(
        bounty::reject(&db, ticket, "Ana", false, "The dialogue tree is missing.", 4000)
            .unwrap()
            .is_ok()
    );

    let r = db.get_build_request_by_ticket(ticket).unwrap().unwrap();
    assert_eq!(r.status, RequestStatus::Open, "a rejection killed the request");
    assert!(r.claimed_by.is_none());
    assert!(r.fulfilled_by.is_none());
    assert!(r.linked.is_empty());
    assert_eq!(r.notes.len(), 1);
    assert!(r.notes[0].message.contains("dialogue tree"));
}

#[test]
fn you_cannot_submit_somebody_elses_claim() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();

    let stolen = bounty::submit(&db, ticket, "Cy", vec![], 3000).unwrap();
    assert!(matches!(stolen, Outcome::Denied(_)));
}

#[test]
fn a_claimed_bounty_cannot_be_claimed_again() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    assert!(bounty::claim(&db, ticket, "Bo", 2000).unwrap().is_ok());
    assert!(matches!(
        bounty::claim(&db, ticket, "Cy", 2100).unwrap(),
        Outcome::WrongState(_)
    ));
}

#[test]
fn a_builder_may_claim_their_own_request() {
    // Blocking it would only mean they do the work without telling anyone.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    assert!(bounty::claim(&db, ticket, "Ana", 2000).unwrap().is_ok());
}

#[test]
fn claims_expire_and_the_work_returns_to_the_board() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 1_000_000).unwrap();

    // Just inside the window: nothing moves.
    assert_eq!(
        bounty::expire_claims(&db, 1_000_000 + bounty::CLAIM_EXPIRY_SECS - 1).unwrap(),
        0
    );
    assert_eq!(
        db.get_build_request_by_ticket(ticket).unwrap().unwrap().status,
        RequestStatus::Claimed
    );

    // Past it: back on the board, with a note saying why.
    assert_eq!(
        bounty::expire_claims(&db, 1_000_000 + bounty::CLAIM_EXPIRY_SECS + 1).unwrap(),
        1
    );
    let r = db.get_build_request_by_ticket(ticket).unwrap().unwrap();
    assert_eq!(r.status, RequestStatus::Open);
    assert!(r.claimed_by.is_none());
    assert!(r.notes.iter().any(|n| n.message.contains("expired")));
}

#[test]
fn a_submitted_bounty_does_not_expire() {
    // Somebody is waiting on a decision, not sitting on the work.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 1_000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec![], 2_000).unwrap();

    assert_eq!(bounty::expire_claims(&db, 9_999_999).unwrap(), 0);
    assert_eq!(
        db.get_build_request_by_ticket(ticket).unwrap().unwrap().status,
        RequestStatus::Submitted
    );
}

#[test]
fn acting_on_a_ticket_that_does_not_exist_is_a_miss_not_a_crash() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    assert_eq!(bounty::claim(&db, 999, "Bo", 1).unwrap(), Outcome::NotFound);
    assert_eq!(bounty::submit(&db, 999, "Bo", vec![], 1).unwrap(), Outcome::NotFound);
    assert_eq!(
        bounty::accept(&db, &connections, &state, 999, "Ana", true, 1).unwrap(),
        Outcome::NotFound
    );
}

#[test]
fn ticket_numbers_do_not_repeat() {
    let (db, _t) = fresh_db();
    let a = post(&db, "Ana", 10);
    let b = post(&db, "Ana", 10);
    let c = post(&db, "Bo", 10);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert!(c > a);
}

// ===========================================================================
// The generated half
// ===========================================================================

fn broken_area(db: &Db, prefix: &str, rooms: usize) -> Uuid {
    let area_id = Uuid::new_v4();
    db.save_area_data(serde_json::from_value(json!({"id": area_id, "name": prefix, "prefix": prefix})).unwrap())
        .unwrap();
    for i in 0..rooms {
        let r: RoomData = serde_json::from_value(json!({
            "id": Uuid::new_v4(),
            "title": format!("Room {i}"),
            "description": "",
            "exits": {},
            "area_id": area_id,
        }))
        .unwrap();
        db.save_room_data(r).unwrap();
    }
    area_id
}

fn regenerate(db: &Db, now: i64) -> (usize, usize) {
    let snapshot = WorldSnapshot::load(db).expect("snapshot");
    bounty::regenerate(db, &snapshot, now).expect("regenerate")
}

#[test]
fn the_auditor_puts_work_on_the_board_before_anybody_posts() {
    // The property that makes this guided rather than merely scored: a builder
    // who has never used the board still finds work on it.
    let (db, _t) = fresh_db();
    broken_area(&db, "broken", 3);
    let (created, closed) = regenerate(&db, 1000);
    assert!(created > 0, "the auditor generated nothing from a broken area");
    assert_eq!(closed, 0);

    let rows = db.list_build_requests().unwrap();
    assert!(rows.iter().all(|r| r.origin == RequestOrigin::Auditor));
    assert!(rows.iter().all(|r| r.requester == bounty::SYSTEM_REQUESTER));
    assert!(rows.iter().all(|r| r.points > 0));
    assert!(rows.iter().all(|r| r.finding_code.is_some()));
}

#[test]
fn regenerating_over_an_unchanged_world_creates_nothing() {
    // It runs every tick forever. Without dedupe the board doubles every five
    // minutes.
    let (db, _t) = fresh_db();
    broken_area(&db, "broken", 3);
    let (first, _) = regenerate(&db, 1000);
    assert!(first > 0);
    let (second, closed) = regenerate(&db, 2000);
    assert_eq!(second, 0, "a repeat run raised {second} duplicates");
    assert_eq!(closed, 0);
}

#[test]
fn fixing_the_problem_closes_the_request_without_anybody_reading_the_board() {
    let (db, _t) = fresh_db();
    let area_id = broken_area(&db, "broken", 2);
    regenerate(&db, 1000);

    let before = db
        .list_build_requests()
        .unwrap()
        .iter()
        .filter(|r| r.status == RequestStatus::Open)
        .count();
    assert!(before > 0);

    // Write every description. The `room.no_desc` findings stop firing.
    for mut r in db.list_all_rooms().unwrap() {
        if r.area_id == Some(area_id) {
            r.description = "A long enough description to clear the stub floor, and then some more \
                             words after it so the thin-description check is quiet too."
                .into();
            db.save_room_data(r).unwrap();
        }
    }

    let (_, closed) = regenerate(&db, 3000);
    assert!(closed > 0, "fixing the rooms closed nothing");
    let resolved = db
        .list_build_requests()
        .unwrap()
        .into_iter()
        .find(|r| r.finding_code.as_deref() == Some("room.no_desc"))
        .expect("the request still exists as a record");
    assert_eq!(resolved.status, RequestStatus::Accepted);
    assert!(resolved.notes.iter().any(|n| n.message.contains("no longer occurs")));
    assert!(
        resolved.fulfilled_by.is_none(),
        "an auto-closed request credited somebody"
    );
}

#[test]
fn claimed_auditor_work_is_never_yanked_out_from_under_the_claimant() {
    let (db, _t) = fresh_db();
    let area_id = broken_area(&db, "broken", 2);
    regenerate(&db, 1000);

    let ticket = db
        .list_build_requests()
        .unwrap()
        .into_iter()
        .find(|r| r.finding_code.as_deref() == Some("room.no_desc"))
        .expect("a description request exists")
        .ticket_number;
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();

    for mut r in db.list_all_rooms().unwrap() {
        if r.area_id == Some(area_id) {
            r.description = "A long enough description to clear the stub floor, and then some more \
                             words after it so the thin-description check is quiet too."
                .into();
            db.save_room_data(r).unwrap();
        }
    }
    regenerate(&db, 3000);

    let r = db.get_build_request_by_ticket(ticket).unwrap().unwrap();
    assert_eq!(
        r.status,
        RequestStatus::Claimed,
        "somebody's claim was auto-closed under them"
    );
    assert_eq!(r.claimed_by.as_deref(), Some("Bo"));
}

#[test]
fn one_broken_area_cannot_flood_the_board() {
    // Without a cap the board becomes that area's audit output, and every
    // other request on it is invisible.
    let (db, _t) = fresh_db();
    broken_area(&db, "wreck", 60);
    regenerate(&db, 1000);

    let live: Vec<_> = db
        .list_build_requests()
        .unwrap()
        .into_iter()
        .filter(|r| r.status.is_live() && r.area_label == "wreck")
        .collect();
    assert!(
        live.len() <= bounty::AUDITOR_CAP_PER_AREA,
        "one area put {} requests on the board",
        live.len()
    );
    assert!(!live.is_empty());
}

#[test]
fn generated_requests_never_pay_for_polish() {
    // A board full of suggestions is a board nobody reads.
    let (db, _t) = fresh_db();
    ironmud::seed::seed_demo_world(&db).expect("seed");
    regenerate(&db, 1000);

    // Every polish code the auditor can emit against the demo world.
    for r in db.list_build_requests().unwrap() {
        let code = r.finding_code.unwrap_or_default();
        assert!(
            !code.ends_with("no_extra_descs") && !code.ends_with("inert") && !code.ends_with("no_seasonal_desc"),
            "a polish finding was posted as a bounty: {code}"
        );
    }
}

#[test]
fn a_blocker_pays_more_than_a_warning() {
    let (db, _t) = fresh_db();
    broken_area(&db, "broken", 2);
    regenerate(&db, 1000);

    let rows = db.list_build_requests().unwrap();
    // `room.no_desc` is a blocker; `area.no_mobiles` is a warning.
    let blocker = rows.iter().find(|r| r.finding_code.as_deref() == Some("room.no_desc"));
    let warning = rows
        .iter()
        .find(|r| r.finding_code.as_deref() == Some("area.no_mobiles"));
    if let (Some(b), Some(w)) = (blocker, warning) {
        assert!(
            b.points > w.points,
            "blocker paid {} and warning paid {}",
            b.points,
            w.points
        );
    }
}

#[test]
fn the_tick_keeps_the_board_current() {
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    broken_area(&db, "broken", 3);

    ironmud::build_score::process_build_score_tick(&db, &connections, &state, 1000).unwrap();
    assert!(
        !db.list_build_requests().unwrap().is_empty(),
        "the tick did not populate the board"
    );
}

// ===========================================================================
// The command
// ===========================================================================

fn run_bounty(db: &Db, args: &str, actor: &str, is_admin: bool) -> String {
    use ironmud::{PlayerSession, World};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": actor, "password_hash": "", "is_builder": true, "is_admin": is_admin,
            "current_room_id": Uuid::nil(),
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
        .compile_file("scripts/commands/bounty.rhai".into())
        .expect("compile bounty.rhai");

    let mut scope = rhai::Scope::new();
    let r: Result<(), _> = engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    r.expect("bounty.rhai run_command");

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn the_board_renders_and_an_empty_one_says_how_to_fill_it() {
    let (db, _t) = fresh_db();
    let empty = run_bounty(&db, "", "Ana", false);
    assert!(
        empty.contains("bounty post"),
        "an empty board gave no way forward:\n{empty}"
    );

    broken_area(&db, "broken", 2);
    regenerate(&db, 1000);
    let out = run_bounty(&db, "", "Ana", false);
    assert!(out.contains("Bounty Board"), "unexpected output:\n{out}");
    assert!(out.contains("SYSTEM"), "generated work is not marked:\n{out}");
    assert!(out.contains("pts"), "points are missing:\n{out}");
}

#[test]
fn posting_and_claiming_work_through_the_command() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    builder(&db, "Bo");

    let posted = run_bounty(&db, "post 40 A blacksmith with a dialogue tree", "Ana", false);
    assert!(posted.contains("Posted as bounty #"), "unexpected output:\n{posted}");

    let ticket = db.list_build_requests().unwrap()[0].ticket_number;
    let claimed = run_bounty(&db, &format!("claim {ticket}"), "Bo", false);
    assert!(claimed.contains("Done"), "unexpected output:\n{claimed}");
    assert_eq!(
        db.get_build_request_by_ticket(ticket)
            .unwrap()
            .unwrap()
            .claimed_by
            .as_deref(),
        Some("Bo")
    );

    // And the refusal path speaks rather than failing silently.
    let again = run_bounty(&db, &format!("claim {ticket}"), "Cy", false);
    assert!(again.contains("not open"), "unexpected output:\n{again}");
}

#[test]
fn a_posted_title_keeps_the_capitals_it_was_typed_with() {
    // The command normalises its input to match verbs, and the title used to
    // be read off that normalised copy — so "A Blacksmith For Oakvale" was
    // stored, and shown to everyone else, as "a blacksmith for oakvale".
    let (db, _t) = fresh_db();
    builder(&db, "Ana");

    run_bounty(&db, "post 40 A Blacksmith For Oakvale", "Ana", false);
    let r = &db.list_build_requests().unwrap()[0];
    assert_eq!(r.title, "A Blacksmith For Oakvale");
}

#[test]
fn a_rejection_reason_keeps_its_capitals_too() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    builder(&db, "Bo");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();
    bounty::submit(&db, ticket, "Bo", vec![], 3000).unwrap();

    run_bounty(
        &db,
        &format!("reject {ticket} The Dialogue Tree Is Missing"),
        "Ana",
        false,
    );
    let r = db.get_build_request_by_ticket(ticket).unwrap().unwrap();
    assert_eq!(
        r.notes.last().map(|n| n.message.as_str()),
        Some("The Dialogue Tree Is Missing")
    );
}

#[test]
fn a_non_numeric_ticket_prints_usage_instead_of_raising() {
    // `bounty.rhai` was the only command script still calling Rhai's built-in
    // `parse_int`, which raises on non-numeric input — the error escaped
    // run_command rather than reaching the player.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");

    for args in ["claim x", "show abc", "post lots a thing"] {
        let out = run_bounty(&db, args, "Ana", false);
        assert!(
            !out.is_empty() && !out.to_lowercase().contains("error"),
            "`bounty {args}` did not answer cleanly:\n{out}"
        );
    }
}

#[test]
fn done_submits_rather_than_listing_closed_work() {
    // `bounty done <n>` is the documented alias for submit, and matching
    // "done" in the listing branch shadowed it into a closed-board render.
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    builder(&db, "Bo");
    let ticket = post(&db, "Ana", 20);
    bounty::claim(&db, ticket, "Bo", 2000).unwrap();

    run_bounty(&db, &format!("done {ticket}"), "Bo", false);
    assert_eq!(
        db.get_build_request_by_ticket(ticket).unwrap().unwrap().status,
        RequestStatus::Submitted
    );
}

#[test]
fn show_renders_one_bounty_in_full() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let ticket = post(&db, "Ana", 40);
    let out = run_bounty(&db, &format!("show {ticket}"), "Ana", false);
    assert!(out.contains("A blacksmith"), "unexpected output:\n{out}");
    assert!(out.contains("40 points"), "points missing:\n{out}");
    assert!(out.contains("asked by Ana"), "requester missing:\n{out}");
}

#[test]
fn posting_stamps_the_area_you_are_standing_in() {
    let (db, _t) = fresh_db();
    builder(&db, "Ana");
    let area_id = Uuid::new_v4();
    db.save_area_data(serde_json::from_value(json!({"id": area_id, "name": "Oakvale", "prefix": "oak"})).unwrap())
        .unwrap();
    let room: RoomData = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "title": "Square", "description": "d", "exits": {}, "area_id": area_id,
    }))
    .unwrap();
    let room_id = room.id;
    db.save_room_data(room).unwrap();

    // Stand the builder in that room.
    let mut ch = db.get_character_data("Ana").unwrap().unwrap();
    ch.current_room_id = room_id;
    db.save_character_data(ch).unwrap();

    // The helper builds its own session, so pass the room through the
    // character it creates by re-running with the saved character's room.
    let out = run_bounty_in_room(&db, "post 20 A well", "Ana", room_id);
    assert!(out.contains("Posted"), "unexpected output:\n{out}");
    assert_eq!(db.list_build_requests().unwrap()[0].area_label, "oak");
}

fn run_bounty_in_room(db: &Db, args: &str, actor: &str, room_id: Uuid) -> String {
    use ironmud::{PlayerSession, World};

    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let state = World::minimal_shared(db.clone(), connections.clone());
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (in_tx, _in_rx) = tokio::sync::mpsc::channel(8);
    let conn_id = Uuid::new_v4();
    let mut session = PlayerSession::new_for_test(out_tx, in_tx);
    session.character = Some(
        serde_json::from_value(json!({
            "name": actor, "password_hash": "", "is_builder": true, "current_room_id": room_id,
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
        .compile_file("scripts/commands/bounty.rhai".into())
        .expect("compile bounty.rhai");
    let mut scope = rhai::Scope::new();
    let r: Result<(), _> = engine.call_fn(&mut scope, &ast, "run_command", (args.to_string(), conn_id.to_string()));
    r.expect("bounty.rhai run_command");

    let mut out = String::new();
    while let Ok(chunk) = out_rx.try_recv() {
        out.push_str(&chunk);
    }
    out
}

#[test]
fn the_bounties_counter_is_a_tally_not_a_reconciliation() {
    // Unlike every other `build.*` counter, bounties filled counts events. A
    // bounty you completed stays completed even if somebody deletes the room
    // it paid for, so this one must not be reset by the scan.
    let (db, _t) = fresh_db();
    let (connections, state) = shared(&db);
    builder(&db, "Ana");
    builder(&db, "Bo");

    for _ in 0..3 {
        let ticket = post(&db, "Ana", 10);
        bounty::claim(&db, ticket, "Bo", 2000).unwrap();
        bounty::submit(&db, ticket, "Bo", vec![], 3000).unwrap();
        bounty::accept(&db, &connections, &state, ticket, "Ana", false, 4000).unwrap();
    }
    assert_eq!(
        db.get_character_data("Bo")
            .unwrap()
            .unwrap()
            .achievement_counters
            .get("build.bounties")
            .copied(),
        Some(3)
    );

    ironmud::build_score::process_build_score_tick(&db, &connections, &state, 5000).unwrap();
    assert_eq!(
        db.get_character_data("Bo")
            .unwrap()
            .unwrap()
            .achievement_counters
            .get("build.bounties")
            .copied(),
        Some(3),
        "the scan reset a tally it does not own"
    );
}

#[test]
fn content_kinds_round_trip_through_a_posted_request() {
    let (db, _t) = fresh_db();
    let r = bounty::post(
        &db,
        "Ana",
        Some(ContentKind::Mobile),
        None,
        "",
        "A blacksmith",
        "",
        30,
        1000,
    )
    .unwrap();
    let back = db.get_build_request_by_ticket(r.ticket_number).unwrap().unwrap();
    assert_eq!(back.kind, Some(ContentKind::Mobile));
    assert_eq!(back.origin, RequestOrigin::Builder);
    // And provenance is untouched by any of this — the board does not author
    // content, it asks for it.
    assert_eq!(ContentOrigin::default(), ContentOrigin::Unknown);
}
