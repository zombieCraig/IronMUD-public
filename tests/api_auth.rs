//! HTTP-level tests for API key authentication (security hardening 1.9).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ironmud::api::{ApiState, create_router};
use ironmud::db::Db;
use ironmud::{ApiKey, ApiPermissions};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn setup() -> (Arc<ApiState>, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(temp.path()).expect("open db");
    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    let state = ironmud::World::minimal_shared(db.clone(), connections.clone());
    (Arc::new(ApiState { db, connections, state }), temp)
}

fn key(id: Uuid, key_hash: String) -> ApiKey {
    ApiKey {
        id,
        key_hash,
        name: "test".to_string(),
        owner_character: "alice".to_string(),
        permissions: ApiPermissions {
            read: true,
            write: false,
            admin: false,
            player_logs: None,
        },
        created_at: 0,
        last_used_at: None,
        enabled: true,
    }
}

async fn get_health(api: &Arc<ApiState>, token: &str) -> axum::response::Response {
    create_router(api.clone())
        .oneshot(
            Request::get("/api/v1/health")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn current_key_is_accepted_without_deprecation_headers() {
    let (api, _t) = setup();
    let id = Uuid::new_v4();
    let generated = ironmud::api_key_auth::generate(id);
    api.db.save_api_key(&key(id, generated.key_hash)).unwrap();

    let resp = get_health(&api, &generated.raw).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get("deprecation").is_none());
}

#[tokio::test]
async fn legacy_key_is_accepted_with_deprecation_headers() {
    let (api, _t) = setup();
    let raw = "legacyHttpKey0123456789abcdefghijkl";
    let id = Uuid::new_v4();
    api.db
        .save_api_key(&key(id, api.db.hash_password(raw).unwrap()))
        .unwrap();

    let resp = get_health(&api, raw).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("deprecation").unwrap(), "true");
    let warning = resp.headers().get("warning").unwrap().to_str().unwrap();
    assert!(warning.contains(&id.to_string()), "{warning}");
}

#[tokio::test]
async fn bad_and_oversized_tokens_are_rejected() {
    let (api, _t) = setup();
    let generated = ironmud::api_key_auth::generate(Uuid::new_v4());
    // Not stored → unknown key.
    assert_eq!(
        get_health(&api, &generated.raw).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(get_health(&api, "imk_nope").await.status(), StatusCode::UNAUTHORIZED);
    let huge = "a".repeat(10_000);
    assert_eq!(get_health(&api, &huge).await.status(), StatusCode::UNAUTHORIZED);
}
