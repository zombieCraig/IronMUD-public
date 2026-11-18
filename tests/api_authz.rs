//! HTTP-level tests for API authorization rules (security hardening 2.2).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ironmud::api::{ApiState, create_router};
use ironmud::db::Db;
use ironmud::{ApiKey, ApiPermissions, AreaData, AreaPermission, RoomData};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct Harness {
    api: Arc<ApiState>,
    _temp: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let db = Db::open(temp.path()).expect("open db");
        let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
        let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
        Harness {
            api: Arc::new(ApiState { db, connections, state }),
            _temp: temp,
        }
    }

    fn db(&self) -> &Db {
        &self.api.db
    }

    /// Save `name` as a character with the given roles.
    fn character(&self, name: &str, builder: bool, admin: bool) {
        let mut ch: ironmud::CharacterData = serde_json::from_value(json!({
            "name": name, "password_hash": "", "current_room_id": Uuid::nil(),
        }))
        .unwrap();
        ch.is_builder = builder;
        ch.is_admin = admin;
        self.db().save_character_data(ch).unwrap();
    }

    /// Create a current-format key for `character` and return the raw token.
    /// The owner is saved as a builder (admin when `admin`), as a real key's
    /// owner must be for its write/admin grants to count.
    fn key(&self, character: &str, write: bool, admin: bool) -> String {
        if self.db().get_character_data(character).unwrap().is_none() {
            self.character(character, true, admin);
        }
        self.raw_key(character, write, admin)
    }

    /// Create a key without touching the owner's character record.
    fn raw_key(&self, character: &str, write: bool, admin: bool) -> String {
        let id = Uuid::new_v4();
        let generated = ironmud::api_key_auth::generate(id);
        self.db()
            .save_api_key(&ApiKey {
                id,
                key_hash: generated.key_hash,
                name: format!("{character}-key"),
                owner_character: character.to_string(),
                permissions: ApiPermissions {
                    read: true,
                    write,
                    admin,
                    player_logs: None,
                },
                created_at: 0,
                last_used_at: None,
                enabled: true,
            })
            .unwrap();
        generated.raw
    }

    fn area(&self, owner: &str, perm: AreaPermission, trusted: &[&str]) -> Uuid {
        let id = Uuid::new_v4();
        let mut area: AreaData = serde_json::from_value(json!({
            "id": id, "name": format!("{owner}'s area"), "prefix": format!("a{}", &id.simple().to_string()[..6]),
        }))
        .unwrap();
        area.owner = Some(owner.to_string());
        area.permission_level = perm;
        area.trusted_builders = trusted.iter().map(|s| s.to_string()).collect();
        self.db().save_area_data(area).unwrap();
        id
    }

    fn room(&self, area: Option<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        let mut room: RoomData = serde_json::from_value(json!({
            "id": id, "title": "Room", "description": "A room.", "exits": {},
        }))
        .unwrap();
        room.area_id = area;
        self.db().save_room_data(room).unwrap();
        id
    }

    async fn call(&self, method: &str, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        let resp = create_router(self.api.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("/api/v1{path}"))
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn read_only_key_cannot_edit_a_room_whose_area_was_deleted() {
    let h = Harness::new();
    let reader = h.key("reader", false, false);
    let room = h.room(Some(Uuid::new_v4())); // dangling area id
    let (status, _) = h
        .call("PUT", &format!("/rooms/{room}"), &reader, json!({"title": "pwned"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(h.db().get_room_data(&room).unwrap().unwrap().title, "Room");
}

#[tokio::test]
async fn moving_a_room_into_a_foreign_area_is_refused() {
    let h = Harness::new();
    let mallory = h.key("mallory", true, false);
    let foreign = h.area("alice", AreaPermission::OwnerOnly, &[]);
    let room = h.room(None);
    let (status, _) = h
        .call(
            "PUT",
            &format!("/rooms/{room}"),
            &mallory,
            json!({"area_id": foreign.to_string()}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(h.db().get_room_data(&room).unwrap().unwrap().area_id, None);
}

#[tokio::test]
async fn transport_stops_need_rights_over_the_stop_room_and_a_free_exit() {
    let h = Harness::new();
    let mallory = h.key("mallory", true, false);
    let own = h.area("mallory", AreaPermission::OwnerOnly, &[]);
    let foreign = h.area("alice", AreaPermission::OwnerOnly, &[]);
    let interior = h.room(Some(own));
    let foreign_room = h.room(Some(foreign));

    let (status, body) = h
        .call(
            "POST",
            "/transports",
            &mallory,
            json!({"name": "Lift", "transport_type": "elevator", "interior_room_id": interior.to_string()}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tid = body["data"]["id"].as_str().unwrap().to_string();

    // A stop in someone else's area: refused.
    let (status, _) = h
        .call(
            "POST",
            &format!("/transports/{tid}/stops"),
            &mallory,
            json!({"room_id": foreign_room.to_string(), "name": "Heist", "exit_direction": "north"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A stop in an owned room whose north exit already exists: refused.
    let lobby = h.room(Some(own));
    let elsewhere = h.room(Some(own));
    h.db().set_room_exit(&lobby, "north", &elsewhere).unwrap();
    let (status, _) = h
        .call(
            "POST",
            &format!("/transports/{tid}/stops"),
            &mallory,
            json!({"room_id": lobby.to_string(), "name": "Lobby", "exit_direction": "north"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Free direction in an owned room: fine.
    let (status, body) = h
        .call(
            "POST",
            &format!("/transports/{tid}/stops"),
            &mallory,
            json!({"room_id": lobby.to_string(), "name": "Lobby", "exit_direction": "east"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Someone else can't manage a transport whose interior they can't edit.
    let other = h.key("other", true, false);
    let (status, _) = h
        .call("DELETE", &format!("/transports/{tid}"), &other, Value::Null)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn spawn_points_are_bounded_and_need_the_room() {
    let h = Harness::new();
    let mallory = h.key("mallory", true, false);
    let own = h.area("mallory", AreaPermission::OwnerOnly, &[]);
    let foreign = h.area("alice", AreaPermission::OwnerOnly, &[]);
    let own_room = h.room(Some(own));
    let foreign_room = h.room(Some(foreign));
    let mut proto = ironmud::MobileData::new("rat".to_string());
    proto.vnum = "rat".to_string();
    proto.is_prototype = true;
    h.db().save_mobile_data(proto).unwrap();

    let req = |room: Uuid, max: i32, interval: i64| {
        json!({
            "area_id": own.to_string(), "room_id": room.to_string(), "entity_type": "mobile",
            "vnum": "rat", "max_count": max, "respawn_interval_secs": interval,
        })
    };
    let (s, _) = h
        .call("POST", "/spawn-points", &mallory, req(own_room, 1000, 300))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "max_count over the cap");
    let (s, _) = h.call("POST", "/spawn-points", &mallory, req(own_room, 5, 1)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "interval under the floor");
    let (s, _) = h
        .call("POST", "/spawn-points", &mallory, req(foreign_room, 5, 300))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "room in a foreign area");
    let (s, body) = h.call("POST", "/spawn-points", &mallory, req(own_room, 5, 300)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn api_spawn_respects_unique() {
    let h = Harness::new();
    let mallory = h.key("mallory", true, false);
    let room = h.room(None);
    let mut proto = ironmud::MobileData::new("king".to_string());
    proto.vnum = "king".to_string();
    proto.is_prototype = true;
    proto.flags.unique = true;
    h.db().save_mobile_data(proto).unwrap();

    let body = json!({"room_id": room.to_string()});
    let (s, resp) = h.call("POST", "/mobiles/king/spawn", &mallory, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{resp}");
    assert_eq!(resp["data"]["current_room_id"], room.to_string());
    let (s, _) = h.call("POST", "/mobiles/king/spawn", &mallory, body).await;
    assert_eq!(s, StatusCode::CONFLICT, "a second unique king must be refused");
}

#[tokio::test]
async fn only_the_owner_governs_an_area() {
    let h = Harness::new();
    let trustee = h.key("trustee", true, false);
    let owner = h.key("owner", true, false);
    let area = h.area("owner", AreaPermission::Trusted, &["trustee"]);

    // A trustee may edit content fields...
    let (s, _) = h
        .call("PUT", &format!("/areas/{area}"), &trustee, json!({"theme": "swamp"}))
        .await;
    assert_eq!(s, StatusCode::OK);
    // ...but not governance, nor delete.
    let (s, _) = h
        .call(
            "PUT",
            &format!("/areas/{area}"),
            &trustee,
            json!({"permission_level": "all_builders"}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h
        .call("PUT", &format!("/areas/{area}"), &trustee, json!({"max_rooms": 0}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h.call("DELETE", &format!("/areas/{area}"), &trustee, Value::Null).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, _) = h
        .call(
            "PUT",
            &format!("/areas/{area}"),
            &owner,
            json!({"trusted_builders": []}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn quest_gold_rewards_are_bounded() {
    let h = Harness::new();
    let key = h.key("builder", true, false);
    for amount in [-500i64, 10_000_000] {
        let (s, body) = h
            .call(
                "POST",
                "/quests",
                &key,
                json!({"vnum": format!("q{}", amount.abs()), "name": "Rich",
                       "rewards": [{"kind": "gold", "amount": amount}]}),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "amount {amount}: {body}");
    }
    // Control: the same request with a sane amount succeeds.
    let (s, body) = h
        .call(
            "POST",
            "/quests",
            &key,
            json!({"vnum": "qok", "name": "Fair", "rewards": [{"kind": "gold", "amount": 500}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn key_permissions_are_capped_by_the_owners_current_role() {
    let h = Harness::new();
    // A write key whose owner is an ordinary player (or was demoted): the
    // in-game editors would refuse them, so the API does too.
    h.character("player", false, false);
    let player = h.raw_key("player", true, false);
    let (s, _) = h
        .call("POST", "/quests", &player, json!({"vnum": "qp", "name": "Nope"}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h
        .call("POST", "/achievements", &player, json!({"key": "ap", "name": "Nope"}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Reads still work.
    let (s, _) = h.call("GET", "/quests", &player, Value::Null).await;
    assert_eq!(s, StatusCode::OK);

    // A key for a character that no longer exists can't write either.
    let ghost = h.raw_key("ghost", true, true);
    let (s, _) = h
        .call("POST", "/quests", &ghost, json!({"vnum": "qg", "name": "Nope"}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // An admin-flagged key whose owner is only a builder can't govern
    // someone else's area.
    h.character("bob", true, false);
    let bob = h.raw_key("bob", true, true);
    let area = h.area("alice", AreaPermission::AllBuilders, &[]);
    let (s, _) = h.call("DELETE", &format!("/areas/{area}"), &bob, Value::Null).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Promote the player: the same key now writes.
    h.character("player", true, false);
    let (s, body) = h
        .call("POST", "/quests", &player, json!({"vnum": "qp", "name": "Now"}))
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn oversized_text_fields_are_rejected() {
    let h = Harness::new();
    let writer = h.key("builder", true, false);

    let (s, body) = h
        .call(
            "POST",
            "/dg-protos",
            &writer,
            json!({"vnum": "big", "name": "Big", "kind": "mob", "flags": "g", "body": "x".repeat(40 * 1024)}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("body exceeds"), "{body}");

    let area = h.area("builder", AreaPermission::OwnerOnly, &[]);
    let many: Vec<String> = (0..200).map(|i| format!("b{i}")).collect();
    let (s, body) = h
        .call(
            "PUT",
            &format!("/areas/{area}"),
            &writer,
            json!({"trusted_builders": many}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("maximum"), "{body}");
}

#[tokio::test]
async fn player_logs_need_their_own_scope() {
    let h = Harness::new();
    let legacy = h.key("boss", true, true);
    // A key from before the scope existed keeps its access.
    let (s, body) = h.call("GET", "/logs/players", &legacy, json!(null)).await;
    assert_eq!(s, StatusCode::OK, "{body}");

    // An admin key without the scope is refused.
    for mut k in h.db().list_all_api_keys().unwrap() {
        k.permissions.player_logs = Some(false);
        h.db().save_api_key(&k).unwrap();
    }
    let (s, body) = h.call("GET", "/logs/players", &legacy, json!(null)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
}

// ---------- divine spellcraft codex ----------

impl Harness {
    /// Store a codex spell belonging to `god` and seat it in that god's canon.
    fn codex_spell(&self, id: &str, name: &str, god: &str) {
        let entry: ironmud::CodexEntry = serde_json::from_value(json!({
            "def": {"id": id, "name": name, "description": "A prayer.", "spell_type": "heal"},
            "meta": {"origin": "authored", "author": "alice", "god_vnum": god, "sealed_day": 12},
        }))
        .unwrap();
        assert!(self.db().insert_codex_if_absent(&entry).unwrap());
        self.db()
            .update_canon(god, |c| {
                c.slots.push(ironmud::CanonSlot {
                    spell_id: id.to_string(),
                    sealed_day: 12,
                    devotion: 3.0,
                    ..Default::default()
                });
                c.influence = 3;
                true
            })
            .unwrap();
    }
}

#[tokio::test]
async fn codex_list_get_and_canon_are_readable() {
    let h = Harness::new();
    h.codex_spell("cx_mend", "Mending Light", "god_sun");
    let reader = h.key("reader", false, false);

    let (s, body) = h.call("GET", "/codex", &reader, Value::Null).await;
    assert_eq!(s, StatusCode::OK);
    let list = body["data"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], "cx_mend");
    assert_eq!(list[0]["god_vnum"], "god_sun");
    assert_eq!(list[0]["status"], "canon");
    assert_eq!(list[0]["origin"], "authored");
    assert_eq!(list[0]["author"], "alice");
    assert!(list[0]["tier"].is_u64());

    let (s, body) = h.call("GET", "/codex/cx_mend", &reader, Value::Null).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["def"]["name"], "Mending Light");
    assert_eq!(body["data"]["meta"]["sealed_day"], 12);

    let (s, _) = h.call("GET", "/codex/cx_nope", &reader, Value::Null).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, body) = h.call("GET", "/canons/god_sun", &reader, Value::Null).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["influence"], 3);
    assert_eq!(body["data"]["slots"][0]["spell_id"], "cx_mend");
    assert_eq!(body["data"]["slots"][0]["spell_name"], "Mending Light");
    assert!(body["data"]["god_name"].is_string());

    let (s, _) = h.call("GET", "/canons/god_nobody", &reader, Value::Null).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn codex_moderation_requires_admin() {
    let h = Harness::new();
    h.codex_spell("cx_mend", "Mending Light", "god_sun");
    let writer = h.key("builder", true, false);
    let (s, _) = h
        .call("PUT", "/codex/cx_mend/status", &writer, json!({"status": "lost"}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h
        .call("PUT", "/codex/cx_mend/veto", &writer, json!({"vetoed": true}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let e = h.db().get_codex("cx_mend").unwrap().unwrap();
    assert_eq!(e.meta.status, ironmud::CanonStatus::Canon);
    assert!(!e.meta.vetoed);
}

#[tokio::test]
async fn codex_moderation_writes_and_mirrors_into_world() {
    let h = Harness::new();
    h.codex_spell("cx_mend", "Mending Light", "god_sun");
    let admin = h.key("root", true, true);

    let (s, _) = h
        .call("PUT", "/codex/cx_mend/status", &admin, json!({"status": "bogus"}))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, body) = h
        .call("PUT", "/codex/cx_mend/status", &admin, json!({"status": "lost"}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["meta"]["status"], "lost");
    assert_eq!(
        h.db().get_codex("cx_mend").unwrap().unwrap().meta.status,
        ironmud::CanonStatus::Lost
    );
    assert!(h.api.state.lock().spell_definitions.contains_key("cx_mend"));

    let (s, body) = h
        .call("PUT", "/codex/cx_mend/veto", &admin, json!({"vetoed": true}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["data"]["meta"]["vetoed"], true);
    assert_eq!(body["data"]["meta"]["status"], "faded");

    let (s, _) = h
        .call("PUT", "/codex/cx_nope/status", &admin, json!({"status": "canon"}))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
