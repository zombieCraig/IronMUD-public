//! Transport CRUD endpoints

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read, can_write},
    error::ApiError,
    notify_builders,
};
use crate::{TransportData, TransportSchedule, TransportState, TransportStop, TransportType, get_opposite_direction};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_transports).post(create_transport))
        .route(
            "/:id",
            get(get_transport).put(update_transport).delete(delete_transport),
        )
        .route("/:id/stops", post(add_stop))
        .route("/:id/stops/:index", delete(remove_stop))
        .route("/:id/connect", post(connect_transport))
        .route("/:id/travel", post(travel_transport))
}

#[derive(Deserialize)]
pub struct CreateTransportRequest {
    pub name: String,
    pub vnum: Option<String>,
    pub transport_type: String,
    pub interior_room_id: String,
    #[serde(default = "default_travel_time")]
    pub travel_time_secs: i64,
    #[serde(default = "default_schedule_type")]
    pub schedule_type: String,
    pub frequency_hours: Option<i32>,
    pub operating_start: Option<u8>,
    pub operating_end: Option<u8>,
    pub dwell_time_secs: Option<i64>,
}

fn default_travel_time() -> i64 {
    30
}
fn default_schedule_type() -> String {
    "ondemand".to_string()
}

#[derive(Deserialize)]
pub struct UpdateTransportRequest {
    pub name: Option<String>,
    pub transport_type: Option<String>,
    pub travel_time_secs: Option<i64>,
    pub schedule_type: Option<String>,
    pub frequency_hours: Option<i32>,
    pub operating_start: Option<u8>,
    pub operating_end: Option<u8>,
    pub dwell_time_secs: Option<i64>,
}

#[derive(Deserialize)]
pub struct AddStopRequest {
    pub room_id: String,
    pub name: String,
    pub exit_direction: String,
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    pub stop_index: usize,
}

#[derive(Deserialize)]
pub struct TravelRequest {
    pub destination_index: usize,
}

#[derive(Serialize)]
pub struct TransportResponse {
    pub success: bool,
    pub data: TransportData,
}

#[derive(Serialize)]
pub struct TransportsListResponse {
    pub success: bool,
    pub data: Vec<TransportData>,
}

fn parse_transport_type(s: &str) -> Option<TransportType> {
    TransportType::from_str(s)
}

fn build_schedule(
    schedule_type: &str,
    frequency_hours: Option<i32>,
    operating_start: Option<u8>,
    operating_end: Option<u8>,
    dwell_time_secs: Option<i64>,
) -> Result<TransportSchedule, ApiError> {
    match schedule_type.to_lowercase().as_str() {
        "ondemand" | "on_demand" => Ok(TransportSchedule::OnDemand),
        "gametime" | "game_time" => Ok(TransportSchedule::GameTime {
            frequency_hours: frequency_hours.unwrap_or(1),
            operating_start: operating_start.unwrap_or(6),
            operating_end: operating_end.unwrap_or(22),
            dwell_time_secs: dwell_time_secs.unwrap_or(30),
        }),
        _ => Err(ApiError::InvalidInput(format!(
            "Invalid schedule_type '{}'. Use: ondemand, gametime",
            schedule_type
        ))),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// List all transports
async fn list_transports(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<TransportsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let transports = state
        .db
        .list_all_transports()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(TransportsListResponse {
        success: true,
        data: transports,
    }))
}

/// Get transport by UUID or vnum
async fn get_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let transport = find_transport(&state, &id)?;

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Create a new transport
async fn create_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateTransportRequest>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    // Parse transport type
    let transport_type = parse_transport_type(&req.transport_type).ok_or_else(|| {
        ApiError::InvalidInput(format!(
            "Invalid transport_type '{}'. Use: elevator, bus, train, ferry, airship",
            req.transport_type
        ))
    })?;

    // Validate interior room exists
    let interior_room_id = Uuid::parse_str(&req.interior_room_id)
        .map_err(|_| ApiError::InvalidInput("Invalid interior_room_id UUID format".into()))?;

    let _room = state
        .db
        .get_room_data(&interior_room_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Interior room '{}' not found", req.interior_room_id)))?;

    // Check vnum uniqueness
    if let Some(ref vnum) = req.vnum {
        if let Ok(Some(_)) = state.db.get_transport_by_vnum(vnum) {
            return Err(ApiError::VnumInUse(format!("Transport vnum '{}' already in use", vnum)));
        }
    }

    // Build schedule
    let schedule = build_schedule(
        &req.schedule_type,
        req.frequency_hours,
        req.operating_start,
        req.operating_end,
        req.dwell_time_secs,
    )?;

    let mut transport = TransportData::new(req.name.clone(), interior_room_id);
    transport.vnum = req.vnum.clone();
    transport.transport_type = transport_type;
    transport.travel_time_secs = req.travel_time_secs;
    transport.schedule = schedule;

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Transport '{}' created by {}", req.name, user.api_key.name),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Update an existing transport
async fn update_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTransportRequest>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut transport = find_transport(&state, &id)?;

    if let Some(ref name) = req.name {
        transport.name = name.clone();
    }

    if let Some(ref transport_type_str) = req.transport_type {
        transport.transport_type = parse_transport_type(transport_type_str).ok_or_else(|| {
            ApiError::InvalidInput(format!(
                "Invalid transport_type '{}'. Use: elevator, bus, train, ferry, airship",
                transport_type_str
            ))
        })?;
    }

    if let Some(travel_time_secs) = req.travel_time_secs {
        transport.travel_time_secs = travel_time_secs;
    }

    // Update schedule if schedule_type is provided
    if let Some(ref schedule_type) = req.schedule_type {
        transport.schedule = build_schedule(
            schedule_type,
            req.frequency_hours.or_else(|| match &transport.schedule {
                TransportSchedule::GameTime { frequency_hours, .. } => Some(*frequency_hours),
                _ => None,
            }),
            req.operating_start.or_else(|| match &transport.schedule {
                TransportSchedule::GameTime { operating_start, .. } => Some(*operating_start),
                _ => None,
            }),
            req.operating_end.or_else(|| match &transport.schedule {
                TransportSchedule::GameTime { operating_end, .. } => Some(*operating_end),
                _ => None,
            }),
            req.dwell_time_secs.or_else(|| match &transport.schedule {
                TransportSchedule::GameTime { dwell_time_secs, .. } => Some(*dwell_time_secs),
                _ => None,
            }),
        )?;
    } else {
        // Update individual schedule fields if no schedule_type change
        if let TransportSchedule::GameTime {
            ref mut frequency_hours,
            ref mut operating_start,
            ref mut operating_end,
            ref mut dwell_time_secs,
        } = transport.schedule
        {
            if let Some(fh) = req.frequency_hours {
                *frequency_hours = fh;
            }
            if let Some(os) = req.operating_start {
                *operating_start = os;
            }
            if let Some(oe) = req.operating_end {
                *operating_end = oe;
            }
            if let Some(dt) = req.dwell_time_secs {
                *dwell_time_secs = dt;
            }
        }
    }

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Transport '{}' updated by {}", transport.name, user.api_key.name),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Delete a transport
async fn delete_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let transport = find_transport(&state, &id)?;
    let transport_name = transport.name.clone();

    // Clean up exits if transport is currently connected to a stop
    if transport.state == TransportState::Stopped && !transport.stops.is_empty() {
        if let Some(stop) = transport.stops.get(transport.current_stop_index) {
            let _ = state.db.clear_room_exit(&stop.room_id, &stop.exit_direction);
            let interior_exit = get_opposite_direction(&stop.exit_direction).unwrap_or("out");
            let _ = state.db.clear_room_exit(&transport.interior_room_id, interior_exit);

            // Clear dynamic description
            if let Ok(Some(mut room)) = state.db.get_room_data(&stop.room_id) {
                room.dynamic_desc = None;
                let _ = state.db.save_room_data(room);
            }
        }
    }

    state
        .db
        .delete_transport(transport.id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Transport '{}' deleted by {}", transport_name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Transport '{}' deleted", transport_name)
    })))
}

/// Add a stop to a transport
async fn add_stop(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<AddStopRequest>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut transport = find_transport(&state, &id)?;

    // Validate room exists
    let room_id =
        Uuid::parse_str(&req.room_id).map_err(|_| ApiError::InvalidInput("Invalid room_id UUID format".into()))?;

    let _room = state
        .db
        .get_room_data(&room_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Room '{}' not found", req.room_id)))?;

    let stop = TransportStop {
        room_id,
        name: req.name.clone(),
        exit_direction: req.exit_direction.clone(),
    };

    transport.stops.push(stop);

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!(
            "[API] Stop '{}' added to transport '{}' by {}",
            req.name, transport.name, user.api_key.name
        ),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Remove a stop from a transport
async fn remove_stop(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((id, index)): Path<(String, usize)>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut transport = find_transport(&state, &id)?;

    if index >= transport.stops.len() {
        return Err(ApiError::NotFound(format!("Stop index {} not found", index)));
    }

    let removed_name = transport.stops[index].name.clone();
    transport.stops.remove(index);

    // Adjust current_stop_index if needed
    if transport.current_stop_index >= transport.stops.len() && !transport.stops.is_empty() {
        transport.current_stop_index = transport.stops.len() - 1;
    }

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!(
            "[API] Stop '{}' removed from transport '{}' by {}",
            removed_name, transport.name, user.api_key.name
        ),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Connect transport to a stop (creates exits)
async fn connect_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut transport = find_transport(&state, &id)?;

    if req.stop_index >= transport.stops.len() {
        return Err(ApiError::InvalidInput(format!(
            "Stop index {} out of range (transport has {} stops)",
            req.stop_index,
            transport.stops.len()
        )));
    }

    let stop = &transport.stops[req.stop_index];
    let stop_room_id = stop.room_id;
    let exit_direction = stop.exit_direction.clone();

    // Create exit from stop room to vehicle interior
    state
        .db
        .set_room_exit(&stop_room_id, &exit_direction, &transport.interior_room_id)
        .map_err(|e| ApiError::Internal(format!("Failed to create exit from stop to interior: {}", e)))?;

    // Create exit from vehicle interior to stop room
    let interior_exit = get_opposite_direction(&exit_direction).unwrap_or("out");
    state
        .db
        .set_room_exit(&transport.interior_room_id, interior_exit, &stop_room_id)
        .map_err(|e| ApiError::Internal(format!("Failed to create exit from interior to stop: {}", e)))?;

    // Update transport state
    transport.current_stop_index = req.stop_index;
    transport.state = TransportState::Stopped;
    transport.last_state_change = now_secs();

    // Set dynamic description on stop room
    if let Ok(Some(mut room)) = state.db.get_room_data(&stop_room_id) {
        room.dynamic_desc = Some(format!("The {} is here, doors open.", transport.name));
        let _ = state.db.save_room_data(room);
    }

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let stop_name = transport.stops[req.stop_index].name.clone();
    notify_builders(
        &state.connections,
        &format!(
            "[API] Transport '{}' connected to stop '{}' by {}",
            transport.name, stop_name, user.api_key.name
        ),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Start on-demand travel to a destination stop
async fn travel_transport(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<TravelRequest>,
) -> Result<Json<TransportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut transport = find_transport(&state, &id)?;

    // Verify transport is on-demand
    if !matches!(transport.schedule, TransportSchedule::OnDemand) {
        return Err(ApiError::InvalidInput(
            "Only on-demand transports can be started via the travel endpoint".into(),
        ));
    }

    // Verify currently stopped
    if transport.state != TransportState::Stopped {
        return Err(ApiError::Conflict("Transport is already moving".into()));
    }

    // Validate destination
    if req.destination_index >= transport.stops.len() {
        return Err(ApiError::InvalidInput(format!(
            "Destination index {} out of range (transport has {} stops)",
            req.destination_index,
            transport.stops.len()
        )));
    }

    // Validate current stop index
    if transport.current_stop_index >= transport.stops.len() {
        return Err(ApiError::InvalidInput("Transport current stop index is invalid".into()));
    }

    // Disconnect from current stop (remove exits)
    let current_stop = &transport.stops[transport.current_stop_index];
    let stop_room_id = current_stop.room_id;
    let exit_direction = current_stop.exit_direction.clone();

    let _ = state.db.clear_room_exit(&stop_room_id, &exit_direction);
    let interior_exit = get_opposite_direction(&exit_direction).unwrap_or("out");
    let _ = state.db.clear_room_exit(&transport.interior_room_id, interior_exit);

    // Clear dynamic description from stop room
    if let Ok(Some(mut room)) = state.db.get_room_data(&stop_room_id) {
        room.dynamic_desc = None;
        let _ = state.db.save_room_data(room);
    }

    // Set destination and state
    transport.current_stop_index = req.destination_index;
    transport.state = TransportState::Moving;
    transport.last_state_change = now_secs();

    state
        .db
        .save_transport(&transport)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let dest_name = transport.stops[req.destination_index].name.clone();
    notify_builders(
        &state.connections,
        &format!(
            "[API] Transport '{}' traveling to '{}' by {}",
            transport.name, dest_name, user.api_key.name
        ),
    );

    Ok(Json(TransportResponse {
        success: true,
        data: transport,
    }))
}

/// Helper: find transport by UUID or vnum
fn find_transport(state: &Arc<ApiState>, id: &str) -> Result<TransportData, ApiError> {
    // Try UUID first
    if let Ok(uuid) = Uuid::parse_str(id) {
        if let Some(transport) = state
            .db
            .get_transport(uuid)
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            return Ok(transport);
        }
    }

    // Fall back to vnum lookup
    state
        .db
        .get_transport_by_vnum(id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Transport '{}' not found", id)))
}
