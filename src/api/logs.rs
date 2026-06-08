//! API for retrieving server logs

use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::{ApiState, auth::AuthenticatedUser, error::ApiError};
use crate::session::broadcast::get_builder_debug_lines;

/// Routes for log retrieval
pub fn routes() -> Router<Arc<ApiState>> {
    Router::new().route("/builder-debug", get(get_builder_debug))
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
