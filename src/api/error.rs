//! API error types and responses

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// API error types
#[derive(Debug)]
pub enum ApiError {
    /// Missing or invalid API key
    Unauthorized,
    /// Insufficient permissions
    Forbidden(String),
    /// Resource not found
    NotFound(String),
    /// Vnum already exists
    VnumInUse(String),
    /// Validation failed
    InvalidInput(String),
    /// Resource conflict
    Conflict(String),
    /// Server error
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Missing or invalid API key".to_string(),
            ),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            ApiError::VnumInUse(msg) => (StatusCode::CONFLICT, "VNUM_IN_USE", msg),
            ApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, "INVALID_INPUT", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg),
            ApiError::Internal(msg) => {
                // The detail (DB/serde errors, paths) is for the server log,
                // not the client.
                let reference = uuid::Uuid::new_v4().simple().to_string();
                let reference = &reference[..8];
                tracing::error!("API internal error [{}]: {}", reference, msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    format!("Internal server error (ref {reference}); see the server log"),
                )
            }
        };

        let body = Json(json!({
            "success": false,
            "error": {
                "code": code,
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}
