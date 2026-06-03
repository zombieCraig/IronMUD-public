//! Achievement CRUD endpoints. Achievements can be engine-defined (JSON) or
//! builder-authored (sled tree). Engine definitions are read-only via API.

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read, can_write},
    error::ApiError,
    notify_builders,
    validate::{check_text_len, NAME_MAX},
};
use crate::types::{
    AchievementCategory, AchievementCriterion, AchievementDef, AchievementReward, AchievementSource,
};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_achievements).post(create_achievement))
        .route("/:key", get(get_achievement).put(update_achievement).delete(delete_achievement))
}

#[derive(Serialize)]
pub struct AchievementSummary {
    pub key: String,
    pub name: String,
    pub category: AchievementCategory,
    pub source: AchievementSource,
    pub hidden: bool,
}

#[derive(Serialize)]
pub struct AchievementListResponse {
    pub success: bool,
    pub data: Vec<AchievementSummary>,
}

#[derive(Serialize)]
pub struct AchievementResponse {
    pub success: bool,
    pub data: AchievementDef,
}

async fn list_achievements(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<AchievementListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let db_defs = state.db.list_all_achievements().map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut data = Vec::new();

    for def in db_defs {
        data.push(AchievementSummary {
            key: def.key,
            name: def.name,
            category: def.category,
            source: def.source,
            hidden: def.hidden,
        });
    }

    Ok(Json(AchievementListResponse { success: true, data }))
}

async fn get_achievement(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(key): Path<String>,
) -> Result<Json<AchievementResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    match state.db.get_achievement(&key.to_lowercase()).map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(def) => Ok(Json(AchievementResponse { success: true, data: def })),
        None => Err(ApiError::NotFound(format!("Achievement '{}' not found in database", key))),
    }
}

#[derive(Deserialize)]
pub struct CreateAchievementRequest {
    pub key: String,
    pub name: String,
    pub category: Option<AchievementCategory>,
}

async fn create_achievement(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateAchievementRequest>,
) -> Result<Json<AchievementResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let key = req.key.to_lowercase().trim().to_string();
    if key.is_empty() {
        return Err(ApiError::InvalidInput("Key is required".into()));
    }
    check_text_len("name", &req.name, NAME_MAX)?;

    if state.db.get_achievement(&key).map_err(|e| ApiError::Internal(e.to_string()))?.is_some() {
        return Err(ApiError::Conflict(format!("Achievement '{}' already exists", key)));
    }

    let def = AchievementDef {
        key: key.clone(),
        name: req.name,
        description: String::new(),
        category: req.category.unwrap_or(AchievementCategory::Builder),
        criterion: AchievementCriterion::Manual,
        reward: AchievementReward::default(),
        hidden: false,
        source: AchievementSource::Db { author: user.api_key.owner_character.clone() },
    };

    state.db.save_achievement(def.clone()).map_err(|e| ApiError::Internal(e.to_string()))?;
    // Mirror into the live world so the engine notify path (and player-facing
    // `achievements` list) sees it without a restart.
    crate::script::achievements::sync_world_after_save(&state.state, def.clone());

    notify_builders(&state.connections, &format!("[API] {} created achievement '{}'", user.api_key.owner_character, key));

    Ok(Json(AchievementResponse { success: true, data: def }))
}

#[derive(Deserialize)]
pub struct UpdateAchievementRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<AchievementCategory>,
    pub criterion: Option<AchievementCriterion>,
    pub reward: Option<AchievementReward>,
    pub hidden: Option<bool>,
}

async fn update_achievement(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(key): Path<String>,
    Json(req): Json<UpdateAchievementRequest>,
) -> Result<Json<AchievementResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let key = key.to_lowercase();
    let mut def = match state.db.get_achievement(&key).map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(d) => d,
        None => return Err(ApiError::NotFound(format!("Achievement '{}' not found", key))),
    };

    if let Some(name) = req.name {
        check_text_len("name", &name, NAME_MAX)?;
        def.name = name;
    }
    if let Some(description) = req.description {
        def.description = description;
    }
    if let Some(category) = req.category {
        def.category = category;
    }
    if let Some(criterion) = req.criterion {
        def.criterion = criterion;
    }
    if let Some(reward) = req.reward {
        def.reward = reward;
    }
    if let Some(hidden) = req.hidden {
        def.hidden = hidden;
    }

    state.db.save_achievement(def.clone()).map_err(|e| ApiError::Internal(e.to_string()))?;
    // Mirror the updated definition (criterion/counter index, hidden, etc.)
    // into the live world so the running engine picks it up immediately.
    crate::script::achievements::sync_world_after_save(&state.state, def.clone());

    notify_builders(&state.connections, &format!("[API] {} updated achievement '{}'", user.api_key.owner_character, key));

    Ok(Json(AchievementResponse { success: true, data: def }))
}

async fn delete_achievement(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let key = key.to_lowercase();
    if state.db.delete_achievement(&key).map_err(|e| ApiError::Internal(e.to_string()))? {
        crate::script::achievements::sync_world_after_delete(&state.state, &key);
        notify_builders(&state.connections, &format!("[API] {} deleted achievement '{}'", user.api_key.owner_character, key));
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Err(ApiError::NotFound(format!("Achievement '{}' not found", key)))
    }
}
