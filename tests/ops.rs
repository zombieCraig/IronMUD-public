//! Ops endpoints: /healthz and /metrics, plus the admin probe client.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use ironmud::db::Db;
use ironmud::heartbeat;
use ironmud::ops::{self, OpsState};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn setup() -> (Arc<OpsState>, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(temp.path()).expect("open db");
    let connections: ironmud::SharedConnections = Arc::new(Mutex::new(HashMap::new()));
    (Arc::new(OpsState { db, connections }), temp)
}

async fn get(state: &Arc<OpsState>, path: &str) -> axum::response::Response {
    ops::router(state.clone())
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn defaults_are_loopback_on_4002() {
    assert_eq!(ops::DEFAULT_OPS_PORT, 4002);
    assert!(ops::is_loopback_bind(ops::DEFAULT_OPS_BIND));
}

#[tokio::test]
async fn healthz_ok_with_fresh_ticks() {
    let (state, _tmp) = setup();
    heartbeat::register("ops_test_fresh", Duration::from_secs(60));
    heartbeat::beat("ops_test_fresh");

    let resp = get(&state, "/healthz").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["ticks"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn metrics_reports_ticks_and_sessions() {
    let (state, _tmp) = setup();
    heartbeat::register("ops_test_metrics", Duration::from_secs(60));
    heartbeat::beat("ops_test_metrics");

    let resp = get(&state, "/metrics").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(ct.starts_with("text/plain; version=0.0.4"), "content type: {ct}");

    let text = body_string(resp).await;
    assert!(text.contains("ironmud_players_online 0\n"));
    assert!(text.contains("ironmud_sessions_open 0\n"));
    assert!(text.contains("ironmud_tick_stale{tick=\"ops_test_metrics\"} 0\n"));
    assert!(text.contains("ironmud_build_info{version=\""));
    assert!(text.contains("ironmud_db_size_bytes "));
}

#[tokio::test]
async fn unknown_path_is_404() {
    let (state, _tmp) = setup();
    assert_eq!(get(&state, "/api/v1/areas").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn probe_health_reads_a_live_listener() {
    let (state, _tmp) = setup();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, ops::router(state)).await.unwrap();
    });

    let (code, body) =
        tokio::task::spawn_blocking(move || ops::probe_health("127.0.0.1", port, Duration::from_secs(5)))
            .await
            .unwrap()
            .expect("probe succeeds");
    assert_eq!(code, 200);
    assert!(body.contains("\"status\":\"ok\""), "body: {body}");
}

#[test]
fn probe_health_errors_when_nothing_listens() {
    // Bind then drop to get a port that is (almost certainly) closed.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(ops::probe_health("127.0.0.1", port, Duration::from_secs(2)).is_err());
}
