//! Spawn point CRUD endpoints

use axum::{
    routing::{get, post, delete},
    extract::{State, Path, Query, Extension},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use super::{ApiState, error::ApiError, auth::{AuthenticatedUser, can_read, can_edit_area}, notify_builders};
use crate::{SpawnPointData, SpawnEntityType, SpawnDependency, SpawnDestination, WearLocation};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_spawn_points).post(create_spawn_point))
        .route("/:id", get(get_spawn_point).put(update_spawn_point).delete(delete_spawn_point))
        .route("/:id/dependencies", post(add_dependency))
        .route("/:id/dependencies/:index", delete(remove_dependency))
}

#[derive(Deserialize)]
pub struct ListSpawnPointsQuery {
    pub area_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateSpawnPointRequest {
    pub area_id: String,
    pub room_id: String,
    pub entity_type: String,
    pub vnum: String,
    #[serde(default = "default_max_count")]
    pub max_count: i32,
    #[serde(default = "default_respawn_interval")]
    pub respawn_interval_secs: i64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_max_count() -> i32 { 1 }
fn default_respawn_interval() -> i64 { 300 }
fn default_enabled() -> bool { true }

#[derive(Deserialize)]
pub struct UpdateSpawnPointRequest {
    pub max_count: Option<i32>,
    pub respawn_interval_secs: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct AddDependencyRequest {
    pub item_vnum: String,
    pub destination: String,
    pub wear_location: Option<String>,
    #[serde(default = "default_dep_count")]
    pub count: i32,
}

fn default_dep_count() -> i32 { 1 }

#[derive(Serialize)]
pub struct SpawnPointResponse {
    pub success: bool,
    pub data: SpawnPointData,
}

#[derive(Serialize)]
pub struct SpawnPointsListResponse {
    pub success: bool,
    pub data: Vec<SpawnPointData>,
}

fn parse_entity_type(s: &str) -> Option<SpawnEntityType> {
    match s.to_lowercase().as_str() {
        "mobile" | "mob" => Some(SpawnEntityType::Mobile),
        "item" | "obj" | "object" => Some(SpawnEntityType::Item),
        _ => None,
    }
}

fn parse_destination(s: &str, wear_location: Option<&str>) -> Option<SpawnDestination> {
    match s.to_lowercase().as_str() {
        "inventory" | "inv" => Some(SpawnDestination::Inventory),
        "equipped" | "equip" => {
            let loc = wear_location
                .and_then(WearLocation::from_str)
                .unwrap_or(WearLocation::Wielded);
            Some(SpawnDestination::Equipped(loc))
        }
        "container" => Some(SpawnDestination::Container),
        _ => None,
    }
}

/// List spawn points
async fn list_spawn_points(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ListSpawnPointsQuery>,
) -> Result<Json<SpawnPointsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mut spawn_points = state.db.list_all_spawn_points()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by area_id if provided
    if let Some(ref area_id_str) = query.area_id {
        let area_uuid = Uuid::parse_str(area_id_str)
            .map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;
        spawn_points.retain(|sp| sp.area_id == area_uuid);
    }

    Ok(Json(SpawnPointsListResponse {
        success: true,
        data: spawn_points,
    }))
}

/// Get spawn point by UUID
async fn get_spawn_point(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<SpawnPointResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let spawn_point = state.db.get_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Spawn point '{}' not found", id)))?;

    Ok(Json(SpawnPointResponse {
        success: true,
        data: spawn_point,
    }))
}

/// Create a new spawn point
async fn create_spawn_point(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateSpawnPointRequest>,
) -> Result<Json<SpawnPointResponse>, ApiError> {
    // Parse and validate area_id
    let area_uuid = Uuid::parse_str(&req.area_id)
        .map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;

    let area = state.db.get_area_data(&area_uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", req.area_id)))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden("You don't have permission to create spawn points in this area".into()));
    }

    // Parse and validate room_id
    let room_uuid = Uuid::parse_str(&req.room_id)
        .map_err(|_| ApiError::InvalidInput("Invalid room_id UUID format".into()))?;

    let _room = state.db.get_room_data(&room_uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", req.room_id)))?;

    // Parse entity type
    let entity_type = parse_entity_type(&req.entity_type)
        .ok_or_else(|| ApiError::InvalidInput(format!("Invalid entity_type '{}'. Use: mobile, item", req.entity_type)))?;

    // Verify vnum exists based on entity type
    match entity_type {
        SpawnEntityType::Mobile => {
            if state.db.get_mobile_by_vnum(&req.vnum)
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .is_none()
            {
                return Err(ApiError::NotFound(format!("Mobile prototype '{}' not found", req.vnum)));
            }
        }
        SpawnEntityType::Item => {
            if state.db.get_item_by_vnum(&req.vnum)
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .is_none()
            {
                return Err(ApiError::NotFound(format!("Item prototype '{}' not found", req.vnum)));
            }
        }
    }

    let spawn_point = SpawnPointData {
        id: Uuid::new_v4(),
        area_id: area_uuid,
        room_id: room_uuid,
        entity_type,
        vnum: req.vnum.clone(),
        max_count: req.max_count,
        respawn_interval_secs: req.respawn_interval_secs,
        enabled: req.enabled,
        last_spawn_time: 0,
        spawned_entities: Vec::new(),
        dependencies: Vec::new(),
    };

    state.db.save_spawn_point(spawn_point.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Spawn point for '{}' created by {}",
        req.vnum, user.api_key.name
    ));

    Ok(Json(SpawnPointResponse {
        success: true,
        data: spawn_point,
    }))
}

/// Update an existing spawn point
async fn update_spawn_point(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSpawnPointRequest>,
) -> Result<Json<SpawnPointResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut spawn_point = state.db.get_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Spawn point '{}' not found", id)))?;

    // Check permission via area
    let area = state.db.get_area_data(&spawn_point.area_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Associated area not found".into()))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden("You don't have permission to edit this spawn point".into()));
    }

    // Apply updates
    if let Some(max_count) = req.max_count {
        spawn_point.max_count = max_count;
    }
    if let Some(respawn_interval_secs) = req.respawn_interval_secs {
        spawn_point.respawn_interval_secs = respawn_interval_secs;
    }
    if let Some(enabled) = req.enabled {
        spawn_point.enabled = enabled;
    }

    state.db.save_spawn_point(spawn_point.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Spawn point for '{}' updated by {}",
        spawn_point.vnum, user.api_key.name
    ));

    Ok(Json(SpawnPointResponse {
        success: true,
        data: spawn_point,
    }))
}

/// Delete a spawn point
async fn delete_spawn_point(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let spawn_point = state.db.get_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Spawn point '{}' not found", id)))?;

    // Check permission via area
    let area = state.db.get_area_data(&spawn_point.area_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Associated area not found".into()))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden("You don't have permission to delete this spawn point".into()));
    }

    let vnum = spawn_point.vnum.clone();

    state.db.delete_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Spawn point for '{}' deleted by {}",
        vnum, user.api_key.name
    ));

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Spawn point for '{}' deleted", vnum)
    })))
}

/// Add a spawn dependency
async fn add_dependency(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddDependencyRequest>,
) -> Result<Json<SpawnPointResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut spawn_point = state.db.get_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Spawn point '{}' not found", id)))?;

    // Check permission via area
    let area = state.db.get_area_data(&spawn_point.area_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Associated area not found".into()))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden("You don't have permission to edit this spawn point".into()));
    }

    // Verify item vnum exists
    if state.db.get_item_by_vnum(&req.item_vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_none()
    {
        return Err(ApiError::NotFound(format!("Item prototype '{}' not found", req.item_vnum)));
    }

    // Parse destination
    let destination = parse_destination(&req.destination, req.wear_location.as_deref())
        .ok_or_else(|| ApiError::InvalidInput(format!(
            "Invalid destination '{}'. Use: inventory, equipped, container",
            req.destination
        )))?;

    let dependency = SpawnDependency {
        item_vnum: req.item_vnum,
        destination,
        count: req.count,
        chance: 100,
    };

    spawn_point.dependencies.push(dependency);

    state.db.save_spawn_point(spawn_point.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(SpawnPointResponse {
        success: true,
        data: spawn_point,
    }))
}

/// Remove a spawn dependency by index
async fn remove_dependency(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<SpawnPointResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut spawn_point = state.db.get_spawn_point(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Spawn point '{}' not found", id)))?;

    // Check permission via area
    let area = state.db.get_area_data(&spawn_point.area_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("Associated area not found".into()))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden("You don't have permission to edit this spawn point".into()));
    }

    if index >= spawn_point.dependencies.len() {
        return Err(ApiError::NotFound(format!("Dependency index {} not found", index)));
    }

    spawn_point.dependencies.remove(index);

    state.db.save_spawn_point(spawn_point.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(SpawnPointResponse {
        success: true,
        data: spawn_point,
    }))
}
