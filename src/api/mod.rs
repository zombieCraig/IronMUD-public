//! REST API for IronMUD building tools
//!
//! This module provides a REST API for creating and modifying MUD content,
//! designed for integration with Claude Code via MCP (Model Context Protocol).

pub mod achievements;
pub mod areas;
pub mod audit;
pub mod auth;
pub mod bugs;
pub mod build_requests;
pub mod dg_protos;
pub mod error;
pub mod items;
pub mod logs;
pub mod mobiles;
pub mod plants;
pub mod quests;
pub mod quotas;
pub mod recipes;
pub mod rooms;
pub mod spawn;
pub mod transports;
pub mod validate;

use axum::{Json, Router, middleware, routing::get};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::SharedConnections;
use crate::SharedState;
use crate::db::Db;

/// Shared state for API handlers
pub struct ApiState {
    pub db: Db,
    pub connections: SharedConnections,
    /// Live game world, so API mutations can mirror DB writes into the
    /// in-memory caches the running server reads (e.g. the achievement
    /// definition map / counter index). Without this, an achievement
    /// created or edited over the API wouldn't be seen by the engine
    /// notify path until a server restart.
    pub state: SharedState,
}

/// In-flight API requests served at once; the rest wait their turn.
const MAX_CONCURRENT_REQUESTS: usize = 32;

/// Create the API router with all routes
pub fn create_router(state: Arc<ApiState>) -> Router {
    let api_routes = Router::new()
        .nest("/areas", areas::routes())
        .nest("/rooms", rooms::routes())
        .nest("/items", items::routes())
        .nest("/mobiles", mobiles::routes())
        .nest("/spawn-points", spawn::routes())
        .nest("/transports", transports::routes())
        .nest("/plants", plants::routes())
        .nest("/recipes", recipes::routes())
        .nest("/quests", quests::routes())
        .nest("/achievements", achievements::routes())
        .nest("/bugs", bugs::routes())
        .nest("/bounties", build_requests::routes())
        .nest("/audit", audit::routes())
        .nest("/dg-protos", dg_protos::routes())
        .nest("/logs", logs::routes())
        .route("/health", get(health_check))
        .layer(middleware::from_fn_with_state(state.clone(), auth::auth_middleware));

    Router::new()
        .nest("/api/v1", api_routes)
        .layer(
            CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any), // Note: credentials are NOT allowed (no .allow_credentials(true)),
                                                                                      // which prevents browser-based CSRF with stored tokens.
        )
        .layer(TraceLayer::new_for_http())
        // Every field is length-checked by its handler; this bounds what the
        // JSON parser will buffer before any of that runs.
        .layer(axum::extract::DefaultBodyLimit::max(validate::REQUEST_BODY_MAX))
        // Back-pressure for the whole API: past this many in-flight requests
        // new ones queue instead of piling more blocking work on the server.
        .layer(tower::limit::ConcurrencyLimitLayer::new(MAX_CONCURRENT_REQUESTS))
        .with_state(state)
}

/// Build identifier exposed via `/health` and logs. Format: `<cargo_version>-<git_short_hash>`,
/// with `-dirty` appended when the build tree had uncommitted changes.
pub const BUILD_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-", env!("IRONMUD_GIT_HASH"));

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": BUILD_VERSION
    }))
}

/// Run the API server on the specified bind address and port
pub async fn run_api_server(state: Arc<ApiState>, bind: &str, port: u16) {
    warn_about_api_keys(&state.db);
    let app = create_router(state);
    let addr = format!("{}:{}", bind, port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("REST API listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

/// Notify builders about API changes
pub fn notify_builders(connections: &SharedConnections, message: &str) {
    {
        let conns = connections.lock();
        for session in conns.values() {
            if let Some(ref character) = session.character {
                if character.is_builder || character.is_admin {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}

/// Log once at startup for each key still in the deprecated legacy format,
/// and each key whose owner no longer holds the role its permissions need.
fn warn_about_api_keys(db: &Db) {
    let Ok(keys) = db.list_all_api_keys() else {
        return;
    };
    for key in keys
        .iter()
        .filter(|k| k.enabled && crate::api_key_auth::is_legacy_hash(&k.key_hash))
    {
        tracing::warn!(
            "[DEPRECATED] API key '{}' ({}) uses the legacy format, which is slow to verify and \
             will stop working in a future release. Rotate it: `ironmud-admin api-key rotate {}`",
            key.name,
            key.id,
            key.id
        );
    }
    for key in keys
        .iter()
        .filter(|k| k.enabled && k.permissions.admin && k.permissions.player_logs.is_none())
    {
        tracing::warn!(
            "API key '{}' ({}) can read player logs only because it predates the separate \
             player_logs scope. Keep it with `ironmud-admin api-key player-logs {} on`, or \
             remove it with `... off`.",
            key.name,
            key.id,
            key.id
        );
    }
    for key in keys.iter().filter(|k| k.enabled) {
        let effective = auth::effective_permissions(db, key);
        if effective.write != key.permissions.write || effective.admin != key.permissions.admin {
            tracing::warn!(
                "API key '{}' ({}) is limited to {}: its owner '{}' is not {}",
                key.name,
                key.id,
                if effective.write { "write" } else { "read-only" },
                key.owner_character,
                if effective.write {
                    "an admin"
                } else {
                    "a builder or admin"
                },
            );
        }
    }
}
