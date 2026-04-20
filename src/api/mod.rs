//! REST API for IronMUD building tools
//!
//! This module provides a REST API for creating and modifying MUD content,
//! designed for integration with Claude Code via MCP (Model Context Protocol).

pub mod error;
pub mod auth;
pub mod areas;
pub mod rooms;
pub mod items;
pub mod mobiles;
pub mod spawn;
pub mod transports;
pub mod plants;
pub mod bugs;

use axum::{
    Router,
    routing::get,
    middleware,
    Json,
};
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use std::sync::Arc;

use crate::db::Db;
use crate::SharedConnections;

/// Shared state for API handlers
pub struct ApiState {
    pub db: Db,
    pub connections: SharedConnections,
}

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
        .nest("/bugs", bugs::routes())
        .route("/health", get(health_check))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware
        ));

    Router::new()
        .nest("/api/v1", api_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
            // Note: credentials are NOT allowed (no .allow_credentials(true)),
            // which prevents browser-based CSRF with stored tokens.
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Run the API server on the specified bind address and port
pub async fn run_api_server(state: Arc<ApiState>, bind: &str, port: u16) {
    let app = create_router(state);
    let addr = format!("{}:{}", bind, port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("REST API listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

/// Notify builders about API changes
pub fn notify_builders(connections: &SharedConnections, message: &str) {
    if let Ok(conns) = connections.lock() {
        for session in conns.values() {
            if let Some(ref character) = session.character {
                if character.is_builder || character.is_admin {
                    let _ = session.sender.send(format!("{}\n", message));
                }
            }
        }
    }
}
