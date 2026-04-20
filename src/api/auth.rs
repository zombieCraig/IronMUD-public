//! API authentication middleware and permission helpers

use super::{ApiState, error::ApiError};
use crate::{ApiKey, AreaData, AreaPermission};
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Authenticated user extracted from the API key
#[derive(Clone)]
pub struct AuthenticatedUser {
    pub api_key: ApiKey,
}

/// Authentication middleware that validates Bearer tokens
pub async fn auth_middleware(
    State(state): State<Arc<ApiState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    if !auth_header.starts_with("Bearer ") {
        return Err(ApiError::Unauthorized);
    }

    let token = &auth_header[7..];

    // Look up the API key by verifying against stored hashes
    let api_key = state
        .db
        .find_api_key_by_raw_key(token)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::Unauthorized)?;

    if !api_key.enabled {
        return Err(ApiError::Forbidden("API key is disabled".into()));
    }

    // Update last_used timestamp
    let _ = state.db.update_api_key_last_used(&api_key.id);

    let user = AuthenticatedUser { api_key };
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

/// Check if a user can read data
pub fn can_read(user: &AuthenticatedUser) -> bool {
    user.api_key.permissions.read
}

/// Check if a user can write data
pub fn can_write(user: &AuthenticatedUser) -> bool {
    user.api_key.permissions.write
}

/// Check if a user can edit an area based on area permissions
pub fn can_edit_area(user: &AuthenticatedUser, area: &AreaData) -> bool {
    // Admin keys bypass all permission checks
    if user.api_key.permissions.admin {
        return true;
    }

    // Must have write permission
    if !user.api_key.permissions.write {
        return false;
    }

    let character_name = &user.api_key.owner_character;

    match area.permission_level {
        AreaPermission::AllBuilders => true,
        AreaPermission::OwnerOnly => area.owner.as_ref() == Some(character_name),
        AreaPermission::Trusted => {
            area.owner.as_ref() == Some(character_name)
                || area
                    .trusted_builders
                    .iter()
                    .any(|b| b.to_lowercase() == character_name.to_lowercase())
        }
    }
}
