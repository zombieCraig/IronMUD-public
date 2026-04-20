//! Room CRUD endpoints

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_edit_area, can_read, can_write},
    error::ApiError,
    notify_builders,
};
use crate::{
    CombatZoneType, DoorState, ExtraDesc, RoomData, RoomExits, RoomFlags, RoomTrigger, TriggerType, WaterType,
};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_rooms).post(create_room))
        .route("/summary", get(list_rooms_summary))
        .route("/:id", get(get_room).put(update_room).delete(delete_room))
        .route("/by-vnum/:vnum", get(get_room_by_vnum))
        .route("/:id/exits/:direction", put(set_exit).delete(remove_exit))
        .route("/:id/doors/:direction", put(set_door).delete(remove_door))
        .route("/:id/triggers", post(add_trigger))
        .route("/:id/triggers/:index", delete(remove_trigger))
        .route("/:id/extra", post(add_extra_desc))
        .route("/:id/extra/:keyword", delete(remove_extra_desc))
}

#[derive(Deserialize)]
pub struct ListRoomsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub area_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    pub title: String,
    pub description: String,
    pub area_id: Option<String>,
    pub vnum: Option<String>,
    #[serde(default)]
    pub flags: RoomFlagsRequest,
}

#[derive(Deserialize, Default)]
pub struct RoomFlagsRequest {
    #[serde(default)]
    pub dark: bool,
    #[serde(default)]
    pub no_mob: bool,
    #[serde(default)]
    pub indoors: bool,
    #[serde(default)]
    pub safe: bool,
    #[serde(default)]
    pub dirt_floor: bool,
    #[serde(default)]
    pub garden: bool,
    #[serde(default)]
    pub climate_controlled: bool,
    #[serde(default)]
    pub city: bool,
    #[serde(default)]
    pub difficult_terrain: bool,
    #[serde(default)]
    pub shallow_water: bool,
    #[serde(default)]
    pub deep_water: bool,
    #[serde(default)]
    pub liveable: bool,
}

#[derive(Deserialize)]
pub struct UpdateRoomRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub area_id: Option<String>,
    pub flags: Option<RoomFlagsRequest>,
    pub living_capacity: Option<i32>,
}

#[derive(Deserialize)]
pub struct SetExitRequest {
    pub target_room_id: String,
}

#[derive(Deserialize)]
pub struct SetDoorRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_closed: bool,
    #[serde(default)]
    pub is_locked: bool,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct AddTriggerRequest {
    pub trigger_type: String,
    pub script_name: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_interval")]
    pub interval_secs: i64,
    #[serde(default = "default_chance")]
    pub chance: i32,
}

fn default_interval() -> i64 {
    60
}
fn default_chance() -> i32 {
    100
}

#[derive(Deserialize)]
pub struct AddExtraDescRequest {
    pub keywords: Vec<String>,
    pub description: String,
}

#[derive(Serialize)]
pub struct RoomResponse {
    pub success: bool,
    pub data: RoomData,
}

#[derive(Serialize)]
pub struct RoomsListResponse {
    pub success: bool,
    pub data: Vec<RoomData>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct RoomSummary {
    pub vnum: Option<String>,
    pub title: String,
    pub exits: Vec<String>,
    pub flags: Vec<String>,
    pub has_doors: Vec<String>,
    pub trigger_count: usize,
    pub extra_desc_count: usize,
}

#[derive(Serialize)]
pub struct RoomsSummaryResponse {
    pub success: bool,
    pub data: Vec<RoomSummary>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct RoomSummaryQuery {
    pub area_id: Option<String>,
    pub vnum_prefix: Option<String>,
}

impl RoomSummary {
    pub fn from_room(room: &RoomData) -> Self {
        let mut exits = Vec::new();
        if room.exits.north.is_some() {
            exits.push("north".to_string());
        }
        if room.exits.south.is_some() {
            exits.push("south".to_string());
        }
        if room.exits.east.is_some() {
            exits.push("east".to_string());
        }
        if room.exits.west.is_some() {
            exits.push("west".to_string());
        }
        if room.exits.up.is_some() {
            exits.push("up".to_string());
        }
        if room.exits.down.is_some() {
            exits.push("down".to_string());
        }

        let mut flags = Vec::new();
        if room.flags.dark {
            flags.push("dark".to_string());
        }
        if room.flags.no_mob {
            flags.push("no_mob".to_string());
        }
        if room.flags.indoors {
            flags.push("indoors".to_string());
        }
        if room.flags.combat_zone.is_some() {
            flags.push("safe".to_string());
        }
        if room.flags.shallow_water {
            flags.push("shallow_water".to_string());
        }
        if room.flags.deep_water {
            flags.push("deep_water".to_string());
        }
        if room.flags.difficult_terrain {
            flags.push("difficult_terrain".to_string());
        }
        if room.flags.city {
            flags.push("city".to_string());
        }
        if room.flags.garden {
            flags.push("garden".to_string());
        }

        let has_doors: Vec<String> = room.doors.keys().cloned().collect();

        RoomSummary {
            vnum: room.vnum.clone(),
            title: room.title.clone(),
            exits,
            flags,
            has_doors,
            trigger_count: room.triggers.len(),
            extra_desc_count: room.extra_descs.len(),
        }
    }
}

/// List rooms with pagination
async fn list_rooms(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ListRoomsQuery>,
) -> Result<Json<RoomsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mut rooms = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by area_id if provided
    if let Some(ref area_id_str) = query.area_id {
        let area_uuid =
            Uuid::parse_str(area_id_str).map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;
        rooms.retain(|r| r.area_id == Some(area_uuid));
    }

    let total = rooms.len();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);

    let rooms: Vec<RoomData> = rooms.into_iter().skip(offset).take(limit).collect();

    Ok(Json(RoomsListResponse {
        success: true,
        data: rooms,
        total,
    }))
}

/// List room summaries (compact)
async fn list_rooms_summary(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<RoomSummaryQuery>,
) -> Result<Json<RoomsSummaryResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let mut rooms = state
        .db
        .list_all_rooms()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by area_id if provided
    if let Some(ref area_id_str) = query.area_id {
        let area_uuid =
            Uuid::parse_str(area_id_str).map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;
        rooms.retain(|r| r.area_id == Some(area_uuid));
    }

    // Filter by vnum_prefix if provided
    if let Some(ref prefix) = query.vnum_prefix {
        rooms.retain(|r| {
            r.vnum
                .as_ref()
                .map_or(false, |v| v.starts_with(&format!("{}:", prefix)))
        });
    }

    let summaries: Vec<RoomSummary> = rooms.iter().map(RoomSummary::from_room).collect();
    let total = summaries.len();

    Ok(Json(RoomsSummaryResponse {
        success: true,
        data: summaries,
        total,
    }))
}

/// Get room by UUID
async fn get_room(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<RoomResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Get room by vnum
async fn get_room_by_vnum(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<RoomResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let room = state
        .db
        .get_room_by_vnum(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room with vnum '{}' not found", vnum)))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Create a new room
async fn create_room(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Parse area_id if provided
    let area_id = if let Some(ref area_id_str) = req.area_id {
        let uuid =
            Uuid::parse_str(area_id_str).map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;

        // Verify area exists and user has permission
        let area = state
            .db
            .get_area_data(&uuid)
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("Area '{}' not found", area_id_str)))?;

        if !can_edit_area(&user, &area) {
            return Err(ApiError::Forbidden(
                "You don't have permission to add rooms to this area".into(),
            ));
        }

        Some(uuid)
    } else {
        None
    };

    // Check vnum uniqueness if provided
    if let Some(ref vnum) = req.vnum {
        if state
            .db
            .get_room_by_vnum(vnum)
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .is_some()
        {
            return Err(ApiError::VnumInUse(format!("Vnum '{}' is already in use", vnum)));
        }
    }

    let combat_zone = if req.flags.safe {
        Some(CombatZoneType::Safe)
    } else {
        None
    };

    let room = RoomData {
        id: Uuid::new_v4(),
        title: req.title,
        description: req.description,
        exits: RoomExits::default(),
        flags: RoomFlags {
            dark: req.flags.dark,
            no_mob: req.flags.no_mob,
            indoors: req.flags.indoors,
            combat_zone,
            dirt_floor: req.flags.dirt_floor,
            garden: req.flags.garden,
            climate_controlled: req.flags.climate_controlled,
            city: req.flags.city,
            difficult_terrain: req.flags.difficult_terrain,
            shallow_water: req.flags.shallow_water,
            deep_water: req.flags.deep_water,
            liveable: req.flags.liveable,
            ..Default::default()
        },
        vnum: req.vnum,
        area_id,
        triggers: Vec::new(),
        doors: HashMap::new(),
        extra_descs: Vec::new(),
        catch_table: Vec::new(),
        property_template_id: None,
        spring_desc: None,
        summer_desc: None,
        autumn_desc: None,
        winter_desc: None,
        dynamic_desc: None,
        water_type: WaterType::default(),
        is_property_template: false,
        is_template_entrance: false,
        property_lease_id: None,
        property_entrance: false,
        recent_departures: Vec::new(),
        blood_trails: Vec::new(),
        traps: Vec::new(),
        living_capacity: 0,
        residents: Vec::new(),
    };

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Register vnum in the index so it's findable by vnum lookup
    if let Some(ref vnum) = room.vnum {
        state
            .db
            .set_room_vnum(&room.id, vnum)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    notify_builders(
        &state.connections,
        &format!(
            "[API] Room '{}' created by {}",
            room.vnum.as_ref().unwrap_or(&room.id.to_string()),
            user.api_key.name
        ),
    );

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Update an existing room
async fn update_room(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRoomRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission if room belongs to an area
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Apply updates
    if let Some(title) = req.title {
        room.title = title;
    }
    if let Some(description) = req.description {
        room.description = description;
    }
    if let Some(area_id_str) = req.area_id {
        let new_area_id =
            Uuid::parse_str(&area_id_str).map_err(|_| ApiError::InvalidInput("Invalid area_id UUID format".into()))?;
        room.area_id = Some(new_area_id);
    }
    if let Some(flags) = req.flags {
        room.flags.dark = flags.dark;
        room.flags.no_mob = flags.no_mob;
        room.flags.indoors = flags.indoors;
        room.flags.combat_zone = if flags.safe { Some(CombatZoneType::Safe) } else { None };
        room.flags.dirt_floor = flags.dirt_floor;
        room.flags.garden = flags.garden;
        room.flags.climate_controlled = flags.climate_controlled;
        room.flags.city = flags.city;
        room.flags.difficult_terrain = flags.difficult_terrain;
        room.flags.shallow_water = flags.shallow_water;
        room.flags.deep_water = flags.deep_water;
        room.flags.liveable = flags.liveable;
    }
    if let Some(cap) = req.living_capacity {
        room.living_capacity = cap.max(0);
    }

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!(
            "[API] Room '{}' updated by {}",
            room.vnum.as_ref().unwrap_or(&room.id.to_string()),
            user.api_key.name
        ),
    );

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Delete a room
async fn delete_room(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to delete this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let room_name = room.vnum.clone().unwrap_or_else(|| room.id.to_string());

    // Clear vnum from index before deleting
    if room.vnum.is_some() {
        state
            .db
            .clear_room_vnum(&uuid)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    state
        .db
        .delete_room(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Room '{}' deleted by {}", room_name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Room '{}' deleted", room_name)
    })))
}

/// Set an exit
async fn set_exit(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, direction)): Path<(String, String)>,
    Json(req): Json<SetExitRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let target_uuid = Uuid::parse_str(&req.target_room_id)
        .map_err(|_| ApiError::InvalidInput("Invalid target_room_id UUID format".into()))?;

    // Verify target room exists
    let _target = state
        .db
        .get_room_data(&target_uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Target room '{}' not found", req.target_room_id)))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Set the exit
    match direction.to_lowercase().as_str() {
        "north" | "n" => room.exits.north = Some(target_uuid),
        "south" | "s" => room.exits.south = Some(target_uuid),
        "east" | "e" => room.exits.east = Some(target_uuid),
        "west" | "w" => room.exits.west = Some(target_uuid),
        "up" | "u" => room.exits.up = Some(target_uuid),
        "down" | "d" => room.exits.down = Some(target_uuid),
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid direction '{}'. Use: north, south, east, west, up, down",
                direction
            )));
        }
    }

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Remove an exit
async fn remove_exit(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, direction)): Path<(String, String)>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Remove the exit
    match direction.to_lowercase().as_str() {
        "north" | "n" => room.exits.north = None,
        "south" | "s" => room.exits.south = None,
        "east" | "e" => room.exits.east = None,
        "west" | "w" => room.exits.west = None,
        "up" | "u" => room.exits.up = None,
        "down" | "d" => room.exits.down = None,
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid direction '{}'. Use: north, south, east, west, up, down",
                direction
            )));
        }
    }

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Set a door
async fn set_door(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, direction)): Path<(String, String)>,
    Json(req): Json<SetDoorRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Validate direction
    let dir_key = match direction.to_lowercase().as_str() {
        "north" | "n" => "north",
        "south" | "s" => "south",
        "east" | "e" => "east",
        "west" | "w" => "west",
        "up" | "u" => "up",
        "down" | "d" => "down",
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid direction '{}'. Use: north, south, east, west, up, down",
                direction
            )));
        }
    };

    let key_id = req
        .key_id
        .as_ref()
        .map(|s| Uuid::parse_str(s))
        .transpose()
        .map_err(|_| ApiError::InvalidInput("Invalid key_id UUID format".into()))?;

    let door = DoorState {
        name: req.name,
        description: req.description,
        is_closed: req.is_closed,
        is_locked: req.is_locked,
        key_id,
        keywords: req.keywords,
    };

    room.doors.insert(dir_key.to_string(), door);

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Remove a door
async fn remove_door(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, direction)): Path<(String, String)>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Normalize direction
    let dir_key = match direction.to_lowercase().as_str() {
        "north" | "n" => "north",
        "south" | "s" => "south",
        "east" | "e" => "east",
        "west" | "w" => "west",
        "up" | "u" => "up",
        "down" | "d" => "down",
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid direction '{}'. Use: north, south, east, west, up, down",
                direction
            )));
        }
    };

    room.doors.remove(dir_key);

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Add a trigger
async fn add_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddTriggerRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Parse trigger type
    let trigger_type = match req.trigger_type.to_lowercase().as_str() {
        "enter" | "onenter" => TriggerType::OnEnter,
        "exit" | "leave" | "onexit" => TriggerType::OnExit,
        "look" | "onlook" => TriggerType::OnLook,
        "periodic" => TriggerType::Periodic,
        "time" | "ontimechange" => TriggerType::OnTimeChange,
        "weather" | "onweatherchange" => TriggerType::OnWeatherChange,
        "season" | "onseasonchange" => TriggerType::OnSeasonChange,
        "month" | "onmonthchange" => TriggerType::OnMonthChange,
        _ => {
            return Err(ApiError::InvalidInput(format!(
                "Invalid trigger type '{}'. Use: enter, exit, look, periodic, time, weather, season, month",
                req.trigger_type
            )));
        }
    };

    let trigger = RoomTrigger {
        trigger_type,
        script_name: req.script_name,
        enabled: true,
        interval_secs: req.interval_secs,
        last_fired: 0,
        chance: req.chance,
        args: req.args,
    };

    room.triggers.push(trigger);

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Remove a trigger by index
async fn remove_trigger(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    if index >= room.triggers.len() {
        return Err(ApiError::NotFound(format!("Trigger index {} not found", index)));
    }

    room.triggers.remove(index);

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Add an extra description
async fn add_extra_desc(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddExtraDescRequest>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let extra = ExtraDesc {
        keywords: req.keywords,
        description: req.description,
    };

    room.extra_descs.push(extra);

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}

/// Remove an extra description by keyword
async fn remove_extra_desc(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, keyword)): Path<(String, String)>,
) -> Result<Json<RoomResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut room = state
        .db
        .get_room_data(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", id)))?;

    // Check permission
    if let Some(area_id) = room.area_id {
        if let Some(area) = state
            .db
            .get_area_data(&area_id)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            if !can_edit_area(&user, &area) {
                return Err(ApiError::Forbidden(
                    "You don't have permission to edit this room".into(),
                ));
            }
        }
    } else if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let keyword_lower = keyword.to_lowercase();
    let original_len = room.extra_descs.len();
    room.extra_descs
        .retain(|ed| !ed.keywords.iter().any(|k| k.to_lowercase() == keyword_lower));

    if room.extra_descs.len() == original_len {
        return Err(ApiError::NotFound(format!(
            "Extra description with keyword '{}' not found",
            keyword
        )));
    }

    state
        .db
        .save_room_data(room.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RoomResponse {
        success: true,
        data: room,
    }))
}
