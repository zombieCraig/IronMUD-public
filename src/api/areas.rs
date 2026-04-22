//! Area CRUD endpoints

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::items::ItemSummary;
use super::mobiles::MobileSummary;
use super::rooms::RoomSummary;
use super::{
    ApiState,
    auth::{AuthenticatedUser, can_edit_area, can_read, can_write},
    error::ApiError,
    notify_builders,
};
use crate::{AreaData, AreaFlags, AreaPermission, CombatZoneType, RoomData, RoomFlags, SpawnEntityType};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_areas).post(create_area))
        .route("/:id", get(get_area).put(update_area).delete(delete_area))
        .route("/by-prefix/:prefix", get(get_area_by_prefix))
        .route("/:id/rooms", get(list_area_rooms))
        .route("/:id/overview", get(area_overview))
        .route("/:id/reset", post(reset_area))
}

#[derive(Deserialize)]
pub struct CreateAreaRequest {
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub level_min: i32,
    #[serde(default)]
    pub level_max: i32,
    #[serde(default)]
    pub theme: String,
}

#[derive(Deserialize)]
pub struct UpdateAreaRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub level_min: Option<i32>,
    pub level_max: Option<i32>,
    pub theme: Option<String>,
    pub permission_level: Option<AreaPermission>,
    pub trusted_builders: Option<Vec<String>>,
    // Migrant immigration config
    pub immigration_enabled: Option<bool>,
    pub immigration_room_vnum: Option<String>,
    pub immigration_name_pool: Option<String>,
    pub immigration_visual_profile: Option<String>,
    pub migration_interval_days: Option<u8>,
    pub migration_max_per_check: Option<u8>,
    pub immigration_guard_chance: Option<f32>,
    /// Per-flag overrides for the area's default_room_flags template.
    /// Absent keys preserve current state; unknown keys are ignored.
    pub default_room_flags: Option<std::collections::HashMap<String, bool>>,
}

/// Apply a (flag_name -> bool) override map onto an area's default_room_flags.
/// Unknown keys are silently ignored; absent keys preserve current state.
/// `combat_zone` is not mutable through this surface (area has its own
/// combat_zone field; room-level combat_zone is an override, not a default).
fn apply_default_room_flag_overrides(flags: &mut RoomFlags, map: &std::collections::HashMap<String, bool>) {
    for (k, v) in map {
        match k.to_lowercase().as_str() {
            "dark" => flags.dark = *v,
            "no_mob" => flags.no_mob = *v,
            "indoors" => flags.indoors = *v,
            "underwater" => flags.underwater = *v,
            "climate_controlled" => flags.climate_controlled = *v,
            "always_hot" => flags.always_hot = *v,
            "always_cold" => flags.always_cold = *v,
            "city" => flags.city = *v,
            "no_windows" => flags.no_windows = *v,
            "difficult_terrain" => flags.difficult_terrain = *v,
            "dirt_floor" => flags.dirt_floor = *v,
            "property_storage" => flags.property_storage = *v,
            "post_office" => flags.post_office = *v,
            "bank" => flags.bank = *v,
            "garden" => flags.garden = *v,
            "spawn_point" => flags.spawn_point = *v,
            "shallow_water" => flags.shallow_water = *v,
            "deep_water" => flags.deep_water = *v,
            "liveable" => flags.liveable = *v,
            _ => {}
        }
    }
}

#[derive(Serialize)]
pub struct AreaResponse {
    pub success: bool,
    pub data: AreaData,
}

#[derive(Serialize)]
pub struct AreasListResponse {
    pub success: bool,
    pub data: Vec<AreaData>,
}

#[derive(Serialize)]
pub struct RoomsListResponse {
    pub success: bool,
    pub data: Vec<RoomData>,
}

#[derive(Serialize)]
pub struct SpawnPointSummary {
    pub entity_type: String,
    pub vnum: String,
    pub room_vnum: Option<String>,
    pub max_count: i32,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct AreaOverview {
    pub area: AreaData,
    pub rooms: Vec<RoomSummary>,
    pub item_prototypes: Vec<ItemSummary>,
    pub mobile_prototypes: Vec<MobileSummary>,
    pub spawn_points: Vec<SpawnPointSummary>,
}

#[derive(Serialize)]
pub struct AreaOverviewResponse {
    pub success: bool,
    pub data: AreaOverview,
}

/// List all areas
async fn list_areas(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<AreasListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let areas = state
        .db
        .list_all_areas()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AreasListResponse {
        success: true,
        data: areas,
    }))
}

/// Get area by UUID
async fn get_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<AreaResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    Ok(Json(AreaResponse {
        success: true,
        data: area,
    }))
}

/// Get area by prefix
async fn get_area_by_prefix(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(prefix): Path<String>,
) -> Result<Json<AreaResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let areas = state
        .db
        .list_all_areas()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let area = areas
        .into_iter()
        .find(|a| a.prefix.to_lowercase() == prefix.to_lowercase())
        .ok_or_else(|| ApiError::NotFound(format!("Area with prefix '{}' not found", prefix)))?;

    Ok(Json(AreaResponse {
        success: true,
        data: area,
    }))
}

/// Create a new area
async fn create_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateAreaRequest>,
) -> Result<Json<AreaResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Validate prefix format (alphanumeric + underscore only)
    if !req.prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ApiError::InvalidInput(
            "Prefix must contain only alphanumeric characters and underscores".into(),
        ));
    }

    // Check prefix uniqueness
    let areas = state
        .db
        .list_all_areas()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if areas
        .iter()
        .any(|a| a.prefix.to_lowercase() == req.prefix.to_lowercase())
    {
        return Err(ApiError::Conflict(format!(
            "Area prefix '{}' already exists",
            req.prefix
        )));
    }

    let area = AreaData {
        id: Uuid::new_v4(),
        name: req.name,
        prefix: req.prefix,
        description: req.description,
        level_min: req.level_min,
        level_max: req.level_max,
        theme: req.theme,
        owner: Some(user.api_key.owner_character.clone()),
        permission_level: AreaPermission::AllBuilders,
        trusted_builders: Vec::new(),
        city_forage_table: Vec::new(),
        wilderness_forage_table: Vec::new(),
        shallow_water_forage_table: Vec::new(),
        deep_water_forage_table: Vec::new(),
        underwater_forage_table: Vec::new(),
        combat_zone: CombatZoneType::default(),
        flags: AreaFlags::default(),
        default_room_flags: RoomFlags::default(),
        immigration_enabled: false,
        immigration_room_vnum: String::new(),
        immigration_name_pool: String::new(),
        immigration_visual_profile: String::new(),
        migration_interval_days: 0,
        migration_max_per_check: 0,
        migrant_sim_defaults: None,
        last_migration_check_day: None,
        immigration_variation_chances: crate::types::ImmigrationVariationChances::default(),
        immigration_family_chance: crate::types::ImmigrationFamilyChance::default(),
    };

    state
        .db
        .save_area_data(area.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Area '{}' created by {}", area.name, user.api_key.name),
    );

    Ok(Json(AreaResponse {
        success: true,
        data: area,
    }))
}

/// Update an existing area
async fn update_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAreaRequest>,
) -> Result<Json<AreaResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden(
            "You don't have permission to edit this area".into(),
        ));
    }

    // Apply updates
    if let Some(name) = req.name {
        area.name = name;
    }
    if let Some(description) = req.description {
        area.description = description;
    }
    if let Some(level_min) = req.level_min {
        area.level_min = level_min;
    }
    if let Some(level_max) = req.level_max {
        area.level_max = level_max;
    }
    if let Some(theme) = req.theme {
        area.theme = theme;
    }
    if let Some(permission_level) = req.permission_level {
        area.permission_level = permission_level;
    }
    if let Some(trusted_builders) = req.trusted_builders {
        area.trusted_builders = trusted_builders;
    }
    if let Some(v) = req.immigration_enabled {
        area.immigration_enabled = v;
    }
    if let Some(v) = req.immigration_room_vnum {
        area.immigration_room_vnum = v;
    }
    if let Some(v) = req.immigration_name_pool {
        area.immigration_name_pool = v;
    }
    if let Some(v) = req.immigration_visual_profile {
        area.immigration_visual_profile = v;
    }
    if let Some(v) = req.migration_interval_days {
        area.migration_interval_days = v.clamp(1, 30);
    }
    if let Some(v) = req.migration_max_per_check {
        area.migration_max_per_check = v;
    }
    if let Some(v) = req.immigration_guard_chance {
        area.immigration_variation_chances.guard = v.clamp(0.0, 1.0);
    }
    if let Some(map) = req.default_room_flags {
        apply_default_room_flag_overrides(&mut area.default_room_flags, &map);
    }

    state
        .db
        .save_area_data(area.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Area '{}' updated by {}", area.name, user.api_key.name),
    );

    Ok(Json(AreaResponse {
        success: true,
        data: area,
    }))
}

/// Delete an area (rooms become unassigned)
async fn delete_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden(
            "You don't have permission to delete this area".into(),
        ));
    }

    // Unassign rooms from this area
    let rooms = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    for mut room in rooms {
        if room.area_id == Some(uuid) {
            room.area_id = None;
            state
                .db
                .save_room_data(room)
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }
    }

    state
        .db
        .delete_area(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Area '{}' deleted by {}", area.name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Area '{}' deleted", area.name)
    })))
}

/// List rooms in an area
async fn list_area_rooms(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<RoomsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    // Verify area exists
    let _area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    let rooms = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .filter(|r| r.area_id == Some(uuid))
        .collect();

    Ok(Json(RoomsListResponse {
        success: true,
        data: rooms,
    }))
}

/// Get a compact overview of an area
async fn area_overview(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<AreaOverviewResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    let prefix = &area.prefix;

    // Rooms in this area
    let rooms: Vec<RoomSummary> = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .iter()
        .filter(|r| r.area_id == Some(uuid))
        .map(RoomSummary::from_room)
        .collect();

    // Item prototypes matching area prefix
    let item_prototypes: Vec<ItemSummary> = state
        .db
        .list_all_items()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .iter()
        .filter(|i| {
            i.is_prototype
                && i.vnum
                    .as_ref()
                    .map_or(false, |v| v.starts_with(&format!("{}:", prefix)))
        })
        .map(ItemSummary::from_item)
        .collect();

    // Mobile prototypes matching area prefix
    let mobile_prototypes: Vec<MobileSummary> = state
        .db
        .list_all_mobiles()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .iter()
        .filter(|m| m.is_prototype && m.vnum.starts_with(&format!("{}:", prefix)))
        .map(MobileSummary::from_mobile)
        .collect();

    // Build a room UUID -> vnum lookup for spawn point room resolution
    let all_rooms = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let room_vnum_map: std::collections::HashMap<Uuid, Option<String>> =
        all_rooms.iter().map(|r| (r.id, r.vnum.clone())).collect();

    // Spawn points for this area
    let spawn_points: Vec<SpawnPointSummary> = state
        .db
        .list_all_spawn_points()
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .iter()
        .filter(|sp| sp.area_id == uuid)
        .map(|sp| SpawnPointSummary {
            entity_type: match sp.entity_type {
                SpawnEntityType::Mobile => "mobile".to_string(),
                SpawnEntityType::Item => "item".to_string(),
            },
            vnum: sp.vnum.clone(),
            room_vnum: room_vnum_map.get(&sp.room_id).cloned().flatten(),
            max_count: sp.max_count,
            enabled: sp.enabled,
        })
        .collect();

    Ok(Json(AreaOverviewResponse {
        success: true,
        data: AreaOverview {
            area,
            rooms,
            item_prototypes,
            mobile_prototypes,
            spawn_points,
        },
    }))
}

/// Trigger area reset (respawn)
async fn reset_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let area = state
        .db
        .get_area_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", id)))?;

    if !can_edit_area(&user, &area) {
        return Err(ApiError::Forbidden(
            "You don't have permission to reset this area".into(),
        ));
    }

    // Get spawn points for this area and reset them
    let spawn_points = state
        .db
        .list_all_spawn_points()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut spawned_count = 0;
    for sp in spawn_points {
        if sp.area_id == uuid && sp.enabled {
            // Cleanup dead refs first (removes only non-existent entities)
            let _ = state.db.cleanup_spawn_point_refs(&sp.id);

            // Reload to get updated count after cleanup
            let mut sp = match state.db.get_spawn_point(&sp.id) {
                Ok(Some(s)) => s,
                _ => continue,
            };

            // Count existing entities of the same vnum already in the room
            // to prevent duplicates from manual spawns or untracked entities
            let existing_in_room = match sp.entity_type {
                SpawnEntityType::Mobile => state
                    .db
                    .get_mobiles_in_room(&sp.room_id)
                    .unwrap_or_default()
                    .iter()
                    .filter(|m| m.vnum == sp.vnum)
                    .count() as i32,
                SpawnEntityType::Item => state
                    .db
                    .get_items_in_room(&sp.room_id)
                    .unwrap_or_default()
                    .iter()
                    .filter(|i| i.vnum.as_deref() == Some(&sp.vnum))
                    .count() as i32,
            };

            // Spawn up to max, considering both tracked and untracked entities
            while (sp.spawned_entities.len() as i32) < sp.max_count
                && (existing_in_room + spawned_count as i32) < sp.max_count
            {
                let spawned_id =
                    match sp.entity_type {
                        SpawnEntityType::Mobile => state
                            .db
                            .spawn_mobile_from_prototype(&sp.vnum)
                            .ok()
                            .flatten()
                            .and_then(|m| {
                                let _ = state.db.move_mobile_to_room(&m.id, &sp.room_id);
                                Some(m.id)
                            }),
                        SpawnEntityType::Item => {
                            state
                                .db
                                .spawn_item_from_prototype(&sp.vnum)
                                .ok()
                                .flatten()
                                .and_then(|i| {
                                    let _ = state.db.move_item_to_room(&i.id, &sp.room_id);
                                    Some(i.id)
                                })
                        }
                    };

                if let Some(id) = spawned_id {
                    sp.spawned_entities.push(id);
                    spawned_count += 1;
                } else {
                    break; // Failed to spawn, stop trying
                }
            }

            sp.last_spawn_time = now;
            state
                .db
                .save_spawn_point(sp)
                .map_err(|e| ApiError::Internal(e.to_string()))?;
        }
    }

    notify_builders(
        &state.connections,
        &format!(
            "[API] Area '{}' reset by {}: {} spawn points triggered",
            area.name, user.api_key.name, spawned_count
        ),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "data": {
            "message": format!("Area '{}' reset triggered", area.name),
            "spawned_count": spawned_count
        }
    })))
}
