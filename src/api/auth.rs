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

/// Parse an optional area_id string from a request body and check the
/// caller is allowed to author content into that area. Returns:
/// - `Ok(None)` when the input is None / empty / blank — the caller is
///   creating an orphan prototype, which any builder may do.
/// - `Ok(Some(uuid))` when the area exists and `can_edit_area` permits.
/// - `Err(InvalidInput)` for malformed UUIDs / `NotFound` for missing areas
///   / `Forbidden` when the caller lacks rights.
pub fn parse_and_authorize_area(
    db: &crate::db::Db,
    user: &AuthenticatedUser,
    area_id_str: Option<&str>,
) -> Result<Option<uuid::Uuid>, super::error::ApiError> {
    use super::error::ApiError;
    let raw = match area_id_str {
        Some(s) if !s.trim().is_empty() => s,
        _ => return Ok(None),
    };
    let uuid =
        uuid::Uuid::parse_str(raw.trim()).map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;
    let area = db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", uuid)))?;
    if !can_edit_area(user, &area) {
        return Err(ApiError::Forbidden(
            "You don't have permission to author content for this area".into(),
        ));
    }
    Ok(Some(uuid))
}

/// Permission gate for mutating an existing prototype that may have an
/// owning area. Orphans (None) are editable by any builder; stamped
/// prototypes require `can_edit_area` rights. Returns Err(Forbidden)
/// when the caller is not allowed.
pub fn authorize_existing_area(
    db: &crate::db::Db,
    user: &AuthenticatedUser,
    area_id: Option<uuid::Uuid>,
) -> Result<(), super::error::ApiError> {
    use super::error::ApiError;
    let Some(uuid) = area_id else {
        return Ok(());
    };
    let area = match db.get_area_data(&uuid).map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(a) => a,
        // Dangling area_id (the area was deleted out from under the
        // prototype): treat as orphan and allow.
        None => return Ok(()),
    };
    if !can_edit_area(user, &area) {
        return Err(ApiError::Forbidden(
            "You don't have permission to edit prototypes owned by this area".into(),
        ));
    }
    Ok(())
}

/// Name-based area permission check used by non-API call sites
/// (DG opcode gate, in-game scripts, etc.). Mirrors `can_edit_area`'s
/// area-permission semantics but skips the API-key admin/write bits
/// since callers already have a character context. Owner-less areas
/// are open to any builder.
///
/// Note: this does NOT check `is_admin` on the named character. The
/// DG gate and Rhai-side paths handle admin bypass before calling
/// here, since they may want different short-circuit policies (e.g.
/// admin authors are pre-authorized in the opcode gate).
pub fn author_can_edit_area(character_name: &str, area: &AreaData) -> bool {
    let Some(owner) = area.owner.as_ref() else {
        return true;
    };
    match area.permission_level {
        AreaPermission::AllBuilders => true,
        AreaPermission::OwnerOnly => owner.eq_ignore_ascii_case(character_name),
        AreaPermission::Trusted => {
            owner.eq_ignore_ascii_case(character_name)
                || area
                    .trusted_builders
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(character_name))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiKey, ApiPermissions};
    use uuid::Uuid;

    fn open_temp_db(_tag: &str) -> (crate::db::Db, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = crate::db::Db::open(temp.path()).expect("open db");
        (db, temp)
    }

    fn user_for(character: &str, write: bool, admin: bool) -> AuthenticatedUser {
        AuthenticatedUser {
            api_key: ApiKey {
                id: Uuid::new_v4(),
                key_hash: String::new(),
                name: format!("{character}-key"),
                owner_character: character.to_string(),
                permissions: ApiPermissions {
                    read: true,
                    write,
                    admin,
                },
                created_at: 0,
                last_used_at: None,
                enabled: true,
            },
        }
    }

    fn save_area(db: &crate::db::Db, owner: Option<&str>, perm: AreaPermission) -> Uuid {
        let area: AreaData = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "test-area",
            "prefix": "ta",
            "owner": owner,
            "permission_level": match perm {
                AreaPermission::AllBuilders => "all_builders",
                AreaPermission::OwnerOnly => "owner_only",
                AreaPermission::Trusted => "trusted",
            },
            "trusted_builders": [],
        }))
        .expect("build area");
        let id = area.id;
        db.save_area_data(area).expect("save area");
        id
    }

    #[test]
    fn authorize_existing_area_blocks_non_owner_on_owner_only() {
        let (db, _temp) = open_temp_db("owner_only_block");
        let area_id = save_area(&db, Some("alice"), AreaPermission::OwnerOnly);
        let bob = user_for("bob", true, false);
        let res = authorize_existing_area(&db, &bob, Some(area_id));
        assert!(matches!(res, Err(super::super::error::ApiError::Forbidden(_))));
    }

    #[test]
    fn authorize_existing_area_allows_owner_on_owner_only() {
        let (db, _temp) = open_temp_db("owner_only_allow");
        let area_id = save_area(&db, Some("alice"), AreaPermission::OwnerOnly);
        let alice = user_for("alice", true, false);
        assert!(authorize_existing_area(&db, &alice, Some(area_id)).is_ok());
    }

    #[test]
    fn authorize_existing_area_allows_all_builders() {
        let (db, _temp) = open_temp_db("all_builders");
        let area_id = save_area(&db, Some("alice"), AreaPermission::AllBuilders);
        let bob = user_for("bob", true, false);
        assert!(authorize_existing_area(&db, &bob, Some(area_id)).is_ok());
    }

    #[test]
    fn authorize_existing_area_admin_bypasses_owner_only() {
        let (db, _temp) = open_temp_db("admin_bypass");
        let area_id = save_area(&db, Some("alice"), AreaPermission::OwnerOnly);
        let admin = user_for("eve", true, true);
        assert!(authorize_existing_area(&db, &admin, Some(area_id)).is_ok());
    }

    #[test]
    fn authorize_existing_area_passes_orphans() {
        let (db, _temp) = open_temp_db("orphan");
        let bob = user_for("bob", true, false);
        assert!(authorize_existing_area(&db, &bob, None).is_ok());
    }

    #[test]
    fn authorize_existing_area_passes_dangling_uuid() {
        let (db, _temp) = open_temp_db("dangling");
        let bob = user_for("bob", true, false);
        // Random UUID that has no area row — historically treated as orphan.
        assert!(authorize_existing_area(&db, &bob, Some(Uuid::new_v4())).is_ok());
    }
}
