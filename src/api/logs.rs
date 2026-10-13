//! API for retrieving server logs

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::{ApiState, auth::AuthenticatedUser, error::ApiError};
use crate::player_log::{PlayerLogEntry, PlayerSessionSummary};
use crate::session::broadcast::get_builder_debug_lines;

/// Routes for log retrieval
pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/builder-debug", get(get_builder_debug))
        .route("/players", get(get_player_sessions))
        .route("/players/:name", get(get_player_log))
}

#[derive(Deserialize)]
pub struct LogQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct LogResponse {
    success: bool,
    data: Vec<String>,
}

/// GET /api/v1/logs/builder-debug
async fn get_builder_debug(
    Extension(user): Extension<AuthenticatedUser>,
    State(_state): State<Arc<ApiState>>,
    Query(query): Query<LogQuery>,
) -> Result<Json<LogResponse>, ApiError> {
    // Only admins or those with read access (we assume builders have API keys with read access)
    // Actually, let's be more specific: only admins can see debug logs for now,
    // or we can just trust the API key permissions.
    if !user.api_key.permissions.read {
        return Err(ApiError::Unauthorized);
    }

    // For now, let's allow anyone with a valid API key that has read permissions.
    // In a real MUD, we might want to check if the owner_character is a builder/admin.

    let limit = query.limit.unwrap_or(50).min(100);
    let lines = get_builder_debug_lines(limit);

    Ok(Json(LogResponse {
        success: true,
        data: lines,
    }))
}

#[derive(Deserialize)]
pub struct PlayerSessionsQuery {
    name: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct PlayerSessionsResponse {
    success: bool,
    data: Vec<PlayerSessionSummary>,
}

#[derive(Deserialize)]
pub struct PlayerLogQuery {
    /// Only entries at or after this unix time.
    since: Option<i64>,
    limit: Option<usize>,
    /// k-th most recent session (0 = latest, live included); overrides since/limit.
    session: Option<usize>,
}

#[derive(Serialize)]
pub struct PlayerLogResponse {
    success: bool,
    data: Vec<PlayerLogEntry>,
}

/// Player logs hold everything players typed, including private tells, so
/// they need an admin key rather than the read permission builders get.
fn require_admin(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.api_key.permissions.admin {
        Ok(())
    } else {
        Err(ApiError::Forbidden("Player logs require an admin API key".into()))
    }
}

/// GET /api/v1/logs/players?name=&limit=
/// Finished sessions, newest first.
async fn get_player_sessions(
    Extension(user): Extension<AuthenticatedUser>,
    State(state): State<Arc<ApiState>>,
    Query(query): Query<PlayerSessionsQuery>,
) -> Result<Json<PlayerSessionsResponse>, ApiError> {
    require_admin(&user)?;
    let limit = query.limit.unwrap_or(50).min(500);
    let name = query.name.as_deref().filter(|n| !n.is_empty());
    let data = state.db.list_player_sessions(name, limit)?;
    Ok(Json(PlayerSessionsResponse { success: true, data }))
}

/// GET /api/v1/logs/players/{name}?since=&limit=&session=
/// One player's activity log, oldest → newest.
async fn get_player_log(
    Extension(user): Extension<AuthenticatedUser>,
    State(state): State<Arc<ApiState>>,
    Path(name): Path<String>,
    Query(query): Query<PlayerLogQuery>,
) -> Result<Json<PlayerLogResponse>, ApiError> {
    require_admin(&user)?;
    let data = match query.session {
        Some(k) => crate::script::player_log::session_entries(&state.db, &name, k),
        None => {
            let limit = query
                .limit
                .unwrap_or(200)
                .min(crate::player_log::PLAYER_LOG_MAX_PER_CHAR);
            state.db.list_player_log(&name, query.since.unwrap_or(0), limit)?
        }
    };
    Ok(Json(PlayerLogResponse { success: true, data }))
}
