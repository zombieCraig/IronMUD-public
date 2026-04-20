//! Plant prototype CRUD endpoints

use axum::{
    routing::get,
    extract::{State, Path, Extension},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use super::{ApiState, error::ApiError, auth::{AuthenticatedUser, can_read, can_write}, notify_builders};
use crate::{PlantPrototype, PlantCategory, GrowthStageDef, GrowthStage, Season};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_plant_prototypes).post(create_plant_prototype))
        .route("/:id", get(get_plant_prototype).put(update_plant_prototype).delete(delete_plant_prototype))
        .route("/by-vnum/:vnum", get(get_plant_prototype_by_vnum))
}

#[derive(Deserialize)]
pub struct CreatePlantPrototypeRequest {
    pub name: String,
    pub vnum: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub seed_vnum: String,
    #[serde(default)]
    pub harvest_vnum: String,
    #[serde(default = "default_harvest_min")]
    pub harvest_min: i32,
    #[serde(default = "default_harvest_max")]
    pub harvest_max: i32,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub stages: Vec<GrowthStageDefRequest>,
    #[serde(default)]
    pub preferred_seasons: Vec<String>,
    #[serde(default)]
    pub forbidden_seasons: Vec<String>,
    #[serde(default)]
    pub water_consumption_per_hour: Option<f64>,
    #[serde(default)]
    pub water_capacity: Option<f64>,
    #[serde(default)]
    pub indoor_only: bool,
    #[serde(default)]
    pub min_skill_to_plant: i32,
    #[serde(default)]
    pub base_xp: Option<i32>,
    #[serde(default)]
    pub pest_resistance: Option<i32>,
    #[serde(default)]
    pub multi_harvest: bool,
}

fn default_harvest_min() -> i32 { 1 }
fn default_harvest_max() -> i32 { 3 }

#[derive(Deserialize)]
pub struct GrowthStageDefRequest {
    pub stage: String,
    pub duration_game_hours: i64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub examine_desc: String,
}

#[derive(Deserialize)]
pub struct UpdatePlantPrototypeRequest {
    pub name: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub seed_vnum: Option<String>,
    pub harvest_vnum: Option<String>,
    pub harvest_min: Option<i32>,
    pub harvest_max: Option<i32>,
    pub category: Option<String>,
    pub stages: Option<Vec<GrowthStageDefRequest>>,
    pub preferred_seasons: Option<Vec<String>>,
    pub forbidden_seasons: Option<Vec<String>>,
    pub water_consumption_per_hour: Option<f64>,
    pub water_capacity: Option<f64>,
    pub indoor_only: Option<bool>,
    pub min_skill_to_plant: Option<i32>,
    pub base_xp: Option<i32>,
    pub pest_resistance: Option<i32>,
    pub multi_harvest: Option<bool>,
}

#[derive(Serialize)]
pub struct PlantPrototypeResponse {
    pub success: bool,
    pub data: PlantPrototype,
}

#[derive(Serialize)]
pub struct PlantPrototypeListResponse {
    pub success: bool,
    pub data: Vec<PlantPrototype>,
}

fn parse_season(s: &str) -> Option<Season> {
    match s.to_lowercase().as_str() {
        "spring" => Some(Season::Spring),
        "summer" => Some(Season::Summer),
        "autumn" | "fall" => Some(Season::Autumn),
        "winter" => Some(Season::Winter),
        _ => None,
    }
}

fn parse_seasons(names: &[String]) -> Result<Vec<Season>, ApiError> {
    names.iter().map(|s| {
        parse_season(s).ok_or_else(|| ApiError::InvalidInput(
            format!("Invalid season '{}'. Valid: spring, summer, autumn, winter", s)
        ))
    }).collect()
}

fn parse_stages(defs: &[GrowthStageDefRequest]) -> Result<Vec<GrowthStageDef>, ApiError> {
    defs.iter().map(|d| {
        let stage = GrowthStage::from_str(&d.stage).ok_or_else(|| ApiError::InvalidInput(
            format!("Invalid growth stage '{}'. Valid: {}", d.stage, GrowthStage::all_names().join(", "))
        ))?;
        Ok(GrowthStageDef {
            stage,
            duration_game_hours: d.duration_game_hours,
            description: d.description.clone(),
            examine_desc: d.examine_desc.clone(),
        })
    }).collect()
}

fn parse_category(s: &str) -> Result<PlantCategory, ApiError> {
    PlantCategory::from_str(s).ok_or_else(|| ApiError::InvalidInput(
        format!("Invalid plant category '{}'. Valid: {}", s, PlantCategory::all_names().join(", "))
    ))
}

/// List all plant prototypes
async fn list_plant_prototypes(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<PlantPrototypeListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let prototypes = state.db.list_all_plant_prototypes()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(PlantPrototypeListResponse {
        success: true,
        data: prototypes,
    }))
}

/// Get plant prototype by UUID
async fn get_plant_prototype(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<PlantPrototypeResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let proto = state.db.get_plant_prototype(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Plant prototype '{}' not found", id)))?;

    Ok(Json(PlantPrototypeResponse {
        success: true,
        data: proto,
    }))
}

/// Get plant prototype by vnum
async fn get_plant_prototype_by_vnum(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<PlantPrototypeResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let proto = state.db.get_plant_prototype_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Plant prototype with vnum '{}' not found", vnum)))?;

    Ok(Json(PlantPrototypeResponse {
        success: true,
        data: proto,
    }))
}

/// Create a new plant prototype
async fn create_plant_prototype(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreatePlantPrototypeRequest>,
) -> Result<Json<PlantPrototypeResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Check vnum uniqueness
    if let Ok(Some(_)) = state.db.get_plant_prototype_by_vnum(&req.vnum) {
        return Err(ApiError::Conflict(format!("Plant prototype with vnum '{}' already exists", req.vnum)));
    }

    let category = if let Some(ref cat) = req.category {
        parse_category(cat)?
    } else {
        PlantCategory::default()
    };

    let stages = parse_stages(&req.stages)?;
    let preferred_seasons = parse_seasons(&req.preferred_seasons)?;
    let forbidden_seasons = parse_seasons(&req.forbidden_seasons)?;

    let proto = PlantPrototype {
        id: Uuid::new_v4(),
        vnum: Some(req.vnum),
        name: req.name,
        keywords: req.keywords,
        seed_vnum: req.seed_vnum,
        harvest_vnum: req.harvest_vnum,
        harvest_min: req.harvest_min,
        harvest_max: req.harvest_max,
        category,
        stages,
        preferred_seasons,
        forbidden_seasons,
        water_consumption_per_hour: req.water_consumption_per_hour.unwrap_or(1.0),
        water_capacity: req.water_capacity.unwrap_or(100.0),
        indoor_only: req.indoor_only,
        min_skill_to_plant: req.min_skill_to_plant,
        base_xp: req.base_xp.unwrap_or(10),
        pest_resistance: req.pest_resistance.unwrap_or(30),
        multi_harvest: req.multi_harvest,
        is_prototype: true,
    };

    state.db.save_plant_prototype(proto.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Plant prototype '{}' created by {}",
        proto.name, user.api_key.name
    ));

    Ok(Json(PlantPrototypeResponse {
        success: true,
        data: proto,
    }))
}

/// Update an existing plant prototype
async fn update_plant_prototype(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePlantPrototypeRequest>,
) -> Result<Json<PlantPrototypeResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut proto = state.db.get_plant_prototype(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Plant prototype '{}' not found", id)))?;

    if let Some(name) = req.name {
        proto.name = name;
    }
    if let Some(keywords) = req.keywords {
        proto.keywords = keywords;
    }
    if let Some(seed_vnum) = req.seed_vnum {
        proto.seed_vnum = seed_vnum;
    }
    if let Some(harvest_vnum) = req.harvest_vnum {
        proto.harvest_vnum = harvest_vnum;
    }
    if let Some(harvest_min) = req.harvest_min {
        proto.harvest_min = harvest_min;
    }
    if let Some(harvest_max) = req.harvest_max {
        proto.harvest_max = harvest_max;
    }
    if let Some(ref cat) = req.category {
        proto.category = parse_category(cat)?;
    }
    if let Some(ref stages) = req.stages {
        proto.stages = parse_stages(stages)?;
    }
    if let Some(ref seasons) = req.preferred_seasons {
        proto.preferred_seasons = parse_seasons(seasons)?;
    }
    if let Some(ref seasons) = req.forbidden_seasons {
        proto.forbidden_seasons = parse_seasons(seasons)?;
    }
    if let Some(v) = req.water_consumption_per_hour {
        proto.water_consumption_per_hour = v;
    }
    if let Some(v) = req.water_capacity {
        proto.water_capacity = v;
    }
    if let Some(v) = req.indoor_only {
        proto.indoor_only = v;
    }
    if let Some(v) = req.min_skill_to_plant {
        proto.min_skill_to_plant = v;
    }
    if let Some(v) = req.base_xp {
        proto.base_xp = v;
    }
    if let Some(v) = req.pest_resistance {
        proto.pest_resistance = v;
    }
    if let Some(v) = req.multi_harvest {
        proto.multi_harvest = v;
    }

    state.db.save_plant_prototype(proto.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Plant prototype '{}' updated by {}",
        proto.name, user.api_key.name
    ));

    Ok(Json(PlantPrototypeResponse {
        success: true,
        data: proto,
    }))
}

/// Delete a plant prototype
async fn delete_plant_prototype(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let proto = state.db.get_plant_prototype(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Plant prototype '{}' not found", id)))?;

    state.db.delete_plant_prototype(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(&state.connections, &format!(
        "[API] Plant prototype '{}' deleted by {}",
        proto.name, user.api_key.name
    ));

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Plant prototype '{}' deleted", proto.name)
    })))
}
