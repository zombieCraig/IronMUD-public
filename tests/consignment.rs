//! End-to-end coverage for the consignment market.
//!
//! The pure rules (price bands, refusals, the commission split) are unit-tested
//! beside them in `src/consignment.rs`. What this file covers is the part that
//! can only go wrong in the plumbing: whether a sale actually moves the item
//! and the gold, whether a broker's shelf survives the thing that deletes
//! broker instances, and whether an unsold listing ends up somewhere a player
//! can still reach it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ironmud::db::Db;
use ironmud::types::{ConsignmentListing, ItemData, ItemLocation, MobileData};

fn temp_db(label: &str) -> (Arc<Db>, tempfile::TempDir) {
    let temp = tempfile::Builder::new().prefix(label).tempdir().expect("temp dir");
    let db = Db::open(temp.path()).expect("open db");
    (Arc::new(db), temp)
}

fn connections() -> ironmud::SharedConnections {
    Arc::new(Mutex::new(HashMap::new()))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn character(db: &Db, name: &str, gold: i32) {
    let mut ch: ironmud::CharacterData = serde_json::from_value(serde_json::json!({
        "name": name,
        "password_hash": "",
        "current_room_id": uuid::Uuid::nil(),
    }))
    .expect("build character");
    ch.gold = gold;
    db.save_character_data(ch).expect("save character");
}

/// A broker instance, plus the prototype its shelf is keyed by.
fn broker(db: &Db, vnum: &str, commission: i32) -> MobileData {
    let mut proto = MobileData::new("a consignment broker".to_string());
    proto.vnum = vnum.to_string();
    proto.is_prototype = true;
    proto.flags.consignment = true;
    proto.consignment_commission_pct = commission;
    proto.shop_buys_types = vec!["all".to_string()];
    db.save_mobile_data(proto.clone()).expect("save prototype");

    let mut instance = proto.clone();
    instance.id = uuid::Uuid::new_v4();
    instance.is_prototype = false;
    db.save_mobile_data(instance.clone()).expect("save instance");
    instance
}

fn item_in_inventory(db: &Db, owner: &str, name: &str, value: i32) -> ItemData {
    let mut item = ItemData::new(name.into(), format!("a {name}"), String::new());
    item.value = value;
    item.location = ItemLocation::Inventory(owner.to_lowercase());
    db.save_item_data(item.clone()).expect("save item");
    item
}

fn listing_for(db: &Db, item: &ItemData, seller: &str, broker_vnum: &str, price: i64) -> ConsignmentListing {
    let listing = ConsignmentListing::new(
        item.id,
        item.name.clone(),
        seller.to_string(),
        broker_vnum.to_string(),
        price,
        now(),
        ironmud::consignment::DEFAULT_LISTING_DURATION_SECS,
    );
    db.save_consignment(&listing).expect("save listing");
    let mut held = item.clone();
    held.location = ItemLocation::Container(listing.id);
    db.save_item_data(held).expect("move item to limbo");
    let id = listing.id;
    db.update_character(seller, |c| c.consignment_ids.push(id))
        .expect("record on seller");
    listing
}

/// The shelf is keyed by the broker's **prototype vnum**, not by the instance
/// standing there. Instances are cloned at spawn and deleted by area resets, so
/// an id-keyed shelf would empty itself the first time the zone reset — taking
/// every listed item with it.
#[test]
fn a_brokers_shelf_survives_its_shopkeeper_being_deleted() {
    let (db, _t) = temp_db("shelf_survives");
    character(&db, "seller", 0);
    let instance = broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);
    listing_for(&db, &item, "seller", "broker:1", 400);

    assert_eq!(db.get_consignments_by_broker("broker:1").unwrap().len(), 1);

    // What an area reset does: the live instance goes away.
    db.delete_mobile(&instance.id).expect("delete instance");

    let shelf = db.get_consignments_by_broker("broker:1").unwrap();
    assert_eq!(shelf.len(), 1, "the shelf outlives the shopkeeper");
    assert_eq!(shelf[0].item_name, "Sword");
}

/// The seller's cut lands in the **bank**, not in carried gold: they are
/// usually offline when their goods sell, and the bank is already the account
/// reachable from anywhere.
#[test]
fn a_sale_banks_the_price_minus_commission_and_moves_the_item() {
    let (db, _t) = temp_db("sale");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    character(&db, "buyer", 1000);
    let instance = broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);
    let listing = listing_for(&db, &item, "seller", "broker:1", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, id) { buy_consignment(who, id) }")
        .expect("compile");
    let out: rhai::Map = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &ast,
            "attempt",
            ("buyer".to_string(), listing.id.to_string()),
        )
        .expect("callable");
    assert!(out["ok"].clone().cast::<bool>(), "{:?}", out["message"]);

    let buyer = db.get_character_data("buyer").unwrap().unwrap();
    assert_eq!(buyer.gold, 600, "the buyer paid the full price");

    let seller = db.get_character_data("seller").unwrap().unwrap();
    assert_eq!(seller.bank_gold, 360, "price minus the 10% commission, into the bank");
    assert_eq!(
        seller.gold, 0,
        "carried gold is untouched — they were not standing there"
    );
    assert!(seller.consignment_ids.is_empty(), "the listing is off their books");

    let sold = db.get_item_data(&item.id).unwrap().unwrap();
    assert!(
        matches!(&sold.location, ItemLocation::Inventory(n) if n == "buyer"),
        "the item reached the buyer: {:?}",
        sold.location
    );
    assert!(db.get_consignment(&listing.id).unwrap().is_none(), "listing consumed");

    // A new counter, and `top` grows a board for it with no leaderboard edit —
    // boards are discovered from character data.
    assert_eq!(seller.achievement_counters.get("items.sold").copied(), Some(1));

    let _ = instance;
}

#[test]
fn a_buyer_who_cannot_pay_changes_nothing() {
    let (db, _t) = temp_db("too_poor");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    character(&db, "buyer", 10);
    broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);
    let listing = listing_for(&db, &item, "seller", "broker:1", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, id) { buy_consignment(who, id) }")
        .expect("compile");
    let out: rhai::Map = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &ast,
            "attempt",
            ("buyer".to_string(), listing.id.to_string()),
        )
        .expect("callable");
    assert!(!out["ok"].clone().cast::<bool>());

    assert_eq!(db.get_character_data("buyer").unwrap().unwrap().gold, 10);
    assert_eq!(db.get_character_data("seller").unwrap().unwrap().bank_gold, 0);
    assert!(db.get_consignment(&listing.id).unwrap().is_some(), "still for sale");
}

/// Buying your own listing would be a commission-priced way to move gold from
/// carried to banked, which is not a trade.
#[test]
fn you_cannot_buy_your_own_listing() {
    let (db, _t) = temp_db("self_buy");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 1000);
    broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);
    let listing = listing_for(&db, &item, "seller", "broker:1", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, id) { buy_consignment(who, id) }")
        .expect("compile");
    let out: rhai::Map = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &ast,
            "attempt",
            ("seller".to_string(), listing.id.to_string()),
        )
        .expect("callable");
    assert!(!out["ok"].clone().cast::<bool>());
    assert_eq!(db.get_character_data("seller").unwrap().unwrap().gold, 1000);
}

/// Listing an item is refused below a quarter of its value. A 500-gold sword at
/// 1 gold is a gold hand-off wearing a shop's clothes, and the commission
/// cannot sink what was never priced.
#[test]
fn the_price_floor_refuses_a_laundering_price() {
    let (db, _t) = temp_db("floor");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    let instance = broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 500);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, item, broker, price) { consign_item(who, item, broker, price) }")
        .expect("compile");

    let try_price = |price: i64| -> rhai::Map {
        engine
            .call_fn(
                &mut rhai::Scope::new(),
                &ast,
                "attempt",
                (
                    "seller".to_string(),
                    item.id.to_string(),
                    instance.id.to_string(),
                    price,
                ),
            )
            .expect("callable")
    };

    let refused = try_price(1);
    assert!(!refused["ok"].clone().cast::<bool>());
    let msg = refused["message"].clone().cast::<String>();
    assert!(msg.contains("125"), "names the floor: {msg}");
    assert!(db.list_all_consignments().unwrap().is_empty(), "nothing was listed");

    // 5000 is ten times value — the ceiling, still allowed.
    assert!(try_price(5000)["ok"].clone().cast::<bool>());
}

#[test]
fn an_item_you_are_not_holding_cannot_be_listed() {
    let (db, _t) = temp_db("not_held");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    character(&db, "other", 0);
    let instance = broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "other", "Sword", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, item, broker, price) { consign_item(who, item, broker, price) }")
        .expect("compile");
    let out: rhai::Map = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &ast,
            "attempt",
            (
                "seller".to_string(),
                item.id.to_string(),
                instance.id.to_string(),
                400i64,
            ),
        )
        .expect("callable");
    assert!(!out["ok"].clone().cast::<bool>());
    assert!(db.list_all_consignments().unwrap().is_empty());
}

/// The per-player cap stops one seller wallpapering a broker.
#[test]
fn the_listing_cap_is_enforced_per_player_per_broker() {
    let (db, _t) = temp_db("cap");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    let mut instance = broker(&db, "broker:1", 10);
    instance.consignment_max_listings_per_player = 2;
    db.save_mobile_data(instance.clone()).expect("save cap");

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, item, broker, price) { consign_item(who, item, broker, price) }")
        .expect("compile");

    let list_one = |n: usize| -> rhai::Map {
        let item = item_in_inventory(&db, "seller", &format!("Sword{n}"), 400);
        engine
            .call_fn(
                &mut rhai::Scope::new(),
                &ast,
                "attempt",
                (
                    "seller".to_string(),
                    item.id.to_string(),
                    instance.id.to_string(),
                    400i64,
                ),
            )
            .expect("callable")
    };

    assert!(list_one(1)["ok"].clone().cast::<bool>());
    assert!(list_one(2)["ok"].clone().cast::<bool>());
    let third = list_one(3);
    assert!(!third["ok"].clone().cast::<bool>(), "the third is over the cap");
    assert_eq!(db.list_all_consignments().unwrap().len(), 2);
}

/// An unsold listing goes to escrow rather than being deleted. Escrow already
/// means "items the game is holding for a named player", so a second holding
/// pen would be one too many — and destroying a player's goods because they
/// mispriced them is not a market, it is a fine.
#[test]
fn an_expired_listing_lands_in_escrow_rather_than_being_destroyed() {
    let (db, _t) = temp_db("expiry");
    character(&db, "seller", 0);
    broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);

    // Backdate past its expiry.
    let mut listing = listing_for(&db, &item, "seller", "broker:1", 400);
    listing.expires_at = now() - 1;
    db.save_consignment(&listing).expect("backdate");

    ironmud::consignment::process_expired_consignments(&db, &connections(), now()).expect("expiry runs");

    assert!(
        db.get_consignment(&listing.id).unwrap().is_none(),
        "the listing is gone"
    );
    let seller = db.get_character_data("seller").unwrap().unwrap();
    assert!(seller.consignment_ids.is_empty());
    assert_eq!(seller.escrow_ids.len(), 1, "and it became an escrow row");

    let escrow = db.get_escrow(&seller.escrow_ids[0]).unwrap().unwrap();
    assert_eq!(escrow.items, vec![item.id], "holding the actual item");
    assert!(
        db.get_item_data(&item.id).unwrap().is_some(),
        "the item still exists — expiry is not a deletion"
    );
}

/// `consignments take <n>` parses its subcommand with `starts_with`, which is
/// the kind of Rhai string method that turns out not to exist (`trim` famously
/// does not). Compilation would not catch it — Rhai resolves methods at call
/// time — so the parse path is driven here.
#[test]
fn the_consignments_subcommand_parse_path_works() {
    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    let ast = engine
        .compile(
            r#"
            fn probe(word) {
                if word.starts_with("take ") || word.starts_with("withdraw ") {
                    let parts = word.split(" ");
                    return parts[parts.len() - 1];
                }
                return "";
            }
            "#,
        )
        .expect("compiles");

    let probe = |s: &str| -> String {
        engine
            .call_fn::<String>(&mut rhai::Scope::new(), &ast, "probe", (s.to_string(),))
            .expect("starts_with and split both resolve at runtime")
    };
    assert_eq!(probe("take 3"), "3");
    assert_eq!(probe("withdraw 12"), "12");
    assert_eq!(probe(""), "");
    assert_eq!(probe("nonsense"), "");
}

/// Put characters online so the session-first write path is the one under test.
///
/// This matters more here than anywhere else in the suite: a DB-only write to
/// an online player is not merely racy, it is *reverted*, and every earlier test
/// in this file runs with an empty connections table where the DB is the only
/// copy. That is exactly why the bug below survived nine passing tests.
fn online(conns: &ironmud::SharedConnections, db: &Db, names: &[&str]) {
    for name in names {
        let ch = db.get_character_data(name).expect("read").expect("exists");
        let (tx_client, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_input, rx_in) = tokio::sync::mpsc::channel::<ironmud::InputEvent>(1);
        let mut session = ironmud::PlayerSession::new_for_test(tx_client, tx_input);
        session.character = Some(ch);
        conns.lock().unwrap().insert(uuid::Uuid::new_v4(), session);
        std::mem::forget((rx, rx_in));
    }
}

/// Copy every online session back to the database, exactly as the thirst,
/// hunger and regen ticks do about once a minute.
fn flush_sessions(conns: &ironmud::SharedConnections, db: &Db) {
    let guard = conns.lock().unwrap();
    for s in guard.values() {
        if let Some(ref c) = s.character {
            db.save_character_data(c.clone()).expect("flush");
        }
    }
}

/// The regression test this feature shipped without.
///
/// Both sides of a sale were written straight to the database while the buyer
/// and seller were online, so the session copies still held the old balances.
/// `notify_counter_core` at the end of the sale then saved the buyer's stale
/// session over the deduction — the purchase refunded itself inside the same
/// call — and the next tick flush destroyed the seller's proceeds. Every test
/// above this one passes with the bug present, because none of them puts anyone
/// online.
#[test]
fn a_sale_between_two_online_players_actually_moves_the_money() {
    let (db, _t) = temp_db("online_sale");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    character(&db, "buyer", 1000);
    online(&conns, &db, &["seller", "buyer"]);

    broker(&db, "broker:1", 10);
    let item = item_in_inventory(&db, "seller", "Sword", 400);
    let listing = listing_for(&db, &item, "seller", "broker:1", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, id) { buy_consignment(who, id) }")
        .expect("compile");
    let out: rhai::Map = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &ast,
            "attempt",
            ("buyer".to_string(), listing.id.to_string()),
        )
        .expect("callable");
    assert!(out["ok"].clone().cast::<bool>(), "{:?}", out["message"]);

    // The session copies are what the rest of the game reads, so they are what
    // the assertions have to look at.
    let session_of = |name: &str| -> ironmud::CharacterData {
        let guard = conns.lock().unwrap();
        guard
            .values()
            .find_map(|s| {
                s.character
                    .as_ref()
                    .filter(|c| c.name.eq_ignore_ascii_case(name))
                    .cloned()
            })
            .expect("online")
    };
    assert_eq!(session_of("buyer").gold, 600, "the buyer's live copy paid");
    assert_eq!(session_of("seller").bank_gold, 360, "the seller's live copy was paid");

    flush_sessions(&conns, &db);
    let buyer = db.get_character_data("buyer").unwrap().unwrap();
    let seller = db.get_character_data("seller").unwrap().unwrap();
    assert_eq!(buyer.gold, 600, "and a tick flush does not refund the purchase");
    assert_eq!(seller.bank_gold, 360, "nor destroy the proceeds");

    // Both money counters ride the sale too, so a broker purchase feeds the
    // same boards a shop purchase does.
    assert_eq!(buyer.achievement_counters.get("gold.spent").copied(), Some(400));
    assert_eq!(seller.achievement_counters.get("gold.earned").copied(), Some(360));
}

/// Withdrawal happens at the counter.
///
/// Without a placement check the shelf is a free cross-map teleport and a bag
/// with no weight: consign ten things, walk across the world, take them all
/// back. The rule lives in `unconsign_item` rather than in the two scripts that
/// call it, so both spellings get the same answer.
#[test]
fn taking_a_listing_back_requires_standing_at_its_broker() {
    let (db, _t) = temp_db("unconsign_placement");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    let instance = broker(&db, "broker:1", 10);
    let counter_room = uuid::Uuid::new_v4();
    db.move_mobile_to_room(&instance.id, &counter_room)
        .expect("place the broker");

    let item = item_in_inventory(&db, "seller", "Sword", 400);
    let listing = listing_for(&db, &item, "seller", "broker:1", 400);

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn attempt(who, room, id) { unconsign_item(who, room, id) }")
        .expect("compile");
    let attempt = |room: uuid::Uuid| -> rhai::Map {
        engine
            .call_fn(
                &mut rhai::Scope::new(),
                &ast,
                "attempt",
                ("seller".to_string(), room.to_string(), listing.id.to_string()),
            )
            .expect("callable")
    };

    let far_away = attempt(uuid::Uuid::new_v4());
    assert!(!far_away["ok"].clone().cast::<bool>(), "not from across the world");
    assert!(
        db.get_consignment(&listing.id).unwrap().is_some(),
        "and the listing stays put"
    );

    let at_the_counter = attempt(counter_room);
    assert!(
        at_the_counter["ok"].clone().cast::<bool>(),
        "{:?}",
        at_the_counter["message"]
    );
    let back = db.get_item_data(&item.id).unwrap().unwrap();
    assert!(
        matches!(&back.location, ItemLocation::Inventory(n) if n == "seller"),
        "{:?}",
        back.location
    );
}

/// `list` and `buy` must agree about what number a shelf row wears.
///
/// The shelf used to be unreachable whenever any shopkeeper stood in the room —
/// both scripts returned on the shopkeeper branch before the broker branch ran,
/// including for a mob that was *itself* both, which the builder guide
/// documents as supported. The fix appends the shelf to the shop page under one
/// continuous numbering, which is only safe if both sides compute it the same
/// way. This drives the real scripts to check that they do.
#[test]
fn the_shelf_continues_the_shop_page_numbering() {
    let (db, _t) = temp_db("numbering");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    character(&db, "buyer", 1000);
    let instance = broker(&db, "broker:1", 0);
    // `MobileData` getters live behind a private module, so the scripts get the
    // one field they need off the broker as a plain map. Everything under test
    // here is the numbering, not the mob.
    let broker_ref = {
        let mut m = rhai::Map::new();
        m.insert("id".into(), rhai::Dynamic::from(instance.id.to_string()));
        m.insert("name".into(), rhai::Dynamic::from(instance.name.clone()));
        m
    };

    // Two things on the shelf, so picking the wrong one is visible.
    let first = item_in_inventory(&db, "seller", "Dagger", 100);
    let second = item_in_inventory(&db, "seller", "Shield", 200);
    let mut first_listing = listing_for(&db, &first, "seller", "broker:1", 100);
    let mut second_listing = listing_for(&db, &second, "seller", "broker:1", 200);
    // The shelf is oldest-first, and both of these were listed in the same
    // second. Space them so the expected order is a fact rather than a
    // coin-flip between two equal timestamps.
    first_listing.listed_at = now() - 60;
    second_listing.listed_at = now();
    db.save_consignment(&first_listing).expect("restamp");
    db.save_consignment(&second_listing).expect("restamp");

    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(128, 128);
    let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
    resolver.set_base_path("scripts/lib");
    engine.set_module_resolver(resolver);
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());

    let sink = captured.clone();
    engine.register_fn("send_client_message", move |_id: String, msg: String| {
        sink.lock().unwrap().push(msg);
    });
    engine.register_fn("broadcast_to_room", |_r: String, _m: String, _e: String| {});
    engine.register_fn("try_parse_int", |s: String| -> rhai::Dynamic {
        s.trim()
            .parse::<i64>()
            .map(rhai::Dynamic::from)
            .unwrap_or(rhai::Dynamic::UNIT)
    });

    // `list`: ask for the shelf as if the shop above it printed three rows, so
    // the shelf's first row is number 4.
    let list_ast = engine
        .compile_file("scripts/commands/list.rhai".into())
        .expect("list.rhai compiles");
    let section: String = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &list_ast,
            "consignment_section",
            (rhai::Dynamic::from(broker_ref.clone()), 4_i64),
        )
        .expect("consignment_section is callable");
    assert!(
        section.contains("  4. Dagger"),
        "oldest first, starting at 4: {section:?}"
    );
    assert!(section.contains("  5. Shield"), "and continues: {section:?}");

    // `buy`: the same offset, from the other side. Number 5 must resolve to the
    // shield — the second row — not to the first.
    let buy_ast = engine
        .compile_file("scripts/commands/buy.rhai".into())
        .expect("buy.rhai compiles");
    let char_map = {
        let mut m = rhai::Map::new();
        m.insert("name".into(), rhai::Dynamic::from("buyer".to_string()));
        m.insert(
            "current_room_id".into(),
            rhai::Dynamic::from(uuid::Uuid::nil().to_string()),
        );
        m
    };
    let _: () = engine
        .call_fn(
            &mut rhai::Scope::new(),
            &buy_ast,
            "buy_from_consignment",
            (
                rhai::Dynamic::from(broker_ref),
                rhai::Dynamic::from(char_map),
                4_i64,
                "5".to_string(),
                uuid::Uuid::nil().to_string(),
            ),
        )
        .expect("buy_from_consignment is callable");

    assert!(
        db.get_consignment(&second_listing.id).unwrap().is_none(),
        "number 5 bought the second row: {:?}",
        captured.lock().unwrap()
    );
    let bought = db.get_item_data(&second.id).unwrap().unwrap();
    assert!(
        matches!(&bought.location, ItemLocation::Inventory(n) if n == "buyer"),
        "{:?}",
        bought.location
    );
}

/// The shelf is bought from by position, so its order has to be one a player
/// can rely on.
///
/// Sled returns rows in key order — listing-uuid order, which is to say random
/// — so before `shelf_order` the shelf was arbitrary, and two `list` calls
/// either side of a new listing could reshuffle everything a player had already
/// read a number off. Oldest first is both stable and the order a shelf ought
/// to have.
#[test]
fn the_shelf_is_ordered_oldest_first() {
    let (db, _t) = temp_db("shelf_order");
    let conns = connections();
    let state = ironmud::World::minimal_shared((*db).clone(), conns.clone());

    character(&db, "seller", 0);
    let instance = broker(&db, "broker:1", 0);

    // Created in an order that has nothing to do with their ids.
    let names = ["Dagger", "Shield", "Helm", "Cloak"];
    for (n, name) in names.iter().enumerate() {
        let item = item_in_inventory(&db, "seller", name, 100);
        let mut listing = listing_for(&db, &item, "seller", "broker:1", 100);
        listing.listed_at = now() - (1000 - n as i64 * 10);
        db.save_consignment(&listing).expect("restamp");
    }

    let mut engine = rhai::Engine::new();
    ironmud::script::consignment::register(&mut engine, db.clone(), conns.clone(), state.clone());
    let ast = engine
        .compile("fn shelf(id) { let out = []; for l in consignments_at(id) { out.push(l.item_name); } out }")
        .expect("compile");
    let shelf: rhai::Array = engine
        .call_fn(&mut rhai::Scope::new(), &ast, "shelf", (instance.id.to_string(),))
        .expect("callable");

    let order: Vec<String> = shelf.into_iter().map(|d| d.cast::<String>()).collect();
    assert_eq!(order, names, "oldest listing first, every time");
}
