//! DG Scripts trigger-prototype CRUD endpoints.
//!
//! Proto vnums are free-form strings (the tbamud importer uses the numeric
//! `.trg` vnum; builder UX accepts memorable names like `guard_lock_door`).
//! The body is run through the DG analyzer on create/update — parse errors
//! reject the write, non-fatal issues come back as `warnings`.

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
};
use crate::types::{DgAttachKind, DgTriggerProto};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_protos).post(create_proto))
        .route(
            "/:vnum",
            get(get_proto).put(update_proto).delete(delete_proto),
        )
}

#[derive(Serialize)]
pub struct DgProtoSummary {
    pub vnum: String,
    pub name: String,
    pub kind: String,
    pub flags: String,
}

#[derive(Serialize)]
pub struct DgProtoView {
    pub vnum: String,
    pub name: String,
    pub kind: String,
    pub flags: String,
    pub numeric_arg: i32,
    pub arglist: String,
    pub body: String,
}

#[derive(Serialize)]
pub struct DgProtoListResponse {
    pub success: bool,
    pub data: Vec<DgProtoSummary>,
}

#[derive(Serialize)]
pub struct DgProtoResponse {
    pub success: bool,
    pub data: DgProtoView,
    /// Non-fatal analyzer warnings for the saved body. Empty on read or
    /// when the body parses clean.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Live instances refreshed after a save. Zero on create / read.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub refreshed_instances: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn kind_str(k: DgAttachKind) -> &'static str {
    match k {
        DgAttachKind::Mob => "mob",
        DgAttachKind::Obj => "obj",
        DgAttachKind::Room => "room",
    }
}

fn parse_kind(s: &str) -> Result<DgAttachKind, ApiError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "mob" | "mobile" => Ok(DgAttachKind::Mob),
        "obj" | "item" => Ok(DgAttachKind::Obj),
        "room" => Ok(DgAttachKind::Room),
        other => Err(ApiError::InvalidInput(format!(
            "kind must be one of mob/obj/room (got '{}')",
            other
        ))),
    }
}

fn to_view(p: DgTriggerProto) -> DgProtoView {
    DgProtoView {
        vnum: p.vnum,
        name: p.name,
        kind: kind_str(p.attach_kind).to_string(),
        flags: p.flags,
        numeric_arg: p.numeric_arg,
        arglist: p.arglist,
        body: p.body,
    }
}

async fn list_protos(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<DgProtoListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let protos = state
        .db
        .list_dg_trigger_protos()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let data = protos
        .into_iter()
        .map(|p| DgProtoSummary {
            vnum: p.vnum,
            name: p.name,
            kind: kind_str(p.attach_kind).to_string(),
            flags: p.flags,
        })
        .collect();
    Ok(Json(DgProtoListResponse { success: true, data }))
}

async fn get_proto(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<DgProtoResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    match state
        .db
        .get_dg_trigger_proto(vnum.trim())
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(p) => Ok(Json(DgProtoResponse {
            success: true,
            data: to_view(p),
            warnings: Vec::new(),
            refreshed_instances: 0,
        })),
        None => Err(ApiError::NotFound(format!(
            "DG proto '{}' not found",
            vnum
        ))),
    }
}

#[derive(Deserialize)]
pub struct CreateDgProtoRequest {
    pub vnum: String,
    pub name: String,
    /// "mob" | "obj" | "room"
    pub kind: String,
    /// Letter-flag string (e.g. "b" for OnIdle, "c" for OnCommand).
    pub flags: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub numeric_arg: Option<i32>,
    #[serde(default)]
    pub arglist: Option<String>,
}

async fn create_proto(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateDgProtoRequest>,
) -> Result<Json<DgProtoResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }
    let vnum = req.vnum.trim().to_string();
    if vnum.is_empty() {
        return Err(ApiError::InvalidInput("vnum is required".into()));
    }
    if state
        .db
        .get_dg_trigger_proto(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some()
    {
        return Err(ApiError::Conflict(format!(
            "DG proto '{}' already exists",
            vnum
        )));
    }
    let kind = parse_kind(&req.kind)?;
    let proto = DgTriggerProto {
        vnum: vnum.clone(),
        name: req.name.trim().to_string(),
        attach_kind: kind,
        flags: req.flags.trim().to_string(),
        numeric_arg: req.numeric_arg.unwrap_or(100),
        arglist: req.arglist.unwrap_or_default(),
        body: req.body,
    };
    // Use the save-with-refresh path so the analyzer rejects parse errors
    // even on create — a malformed body shouldn't make it into the store.
    let (refreshed, warnings) = state
        .db
        .save_dg_trigger_proto_with_refresh(&proto)
        .map_err(|e| ApiError::InvalidInput(e.to_string()))?;
    notify_builders(
        &state.connections,
        &format!(
            "[API] {} created DG proto '{}'",
            user.api_key.owner_character, vnum
        ),
    );
    Ok(Json(DgProtoResponse {
        success: true,
        data: to_view(proto),
        warnings,
        refreshed_instances: refreshed,
    }))
}

#[derive(Deserialize)]
pub struct UpdateDgProtoRequest {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub flags: Option<String>,
    pub body: Option<String>,
    pub numeric_arg: Option<i32>,
    pub arglist: Option<String>,
}

async fn update_proto(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
    Json(req): Json<UpdateDgProtoRequest>,
) -> Result<Json<DgProtoResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }
    let vnum = vnum.trim().to_string();
    let mut proto = match state
        .db
        .get_dg_trigger_proto(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(p) => p,
        None => {
            return Err(ApiError::NotFound(format!(
                "DG proto '{}' not found",
                vnum
            )));
        }
    };
    if let Some(name) = req.name {
        proto.name = name.trim().to_string();
    }
    if let Some(kind) = req.kind {
        proto.attach_kind = parse_kind(&kind)?;
    }
    if let Some(flags) = req.flags {
        proto.flags = flags.trim().to_string();
    }
    if let Some(body) = req.body {
        proto.body = body;
    }
    if let Some(n) = req.numeric_arg {
        proto.numeric_arg = n;
    }
    if let Some(arglist) = req.arglist {
        proto.arglist = arglist;
    }
    let (refreshed, warnings) = state
        .db
        .save_dg_trigger_proto_with_refresh(&proto)
        .map_err(|e| ApiError::InvalidInput(e.to_string()))?;
    notify_builders(
        &state.connections,
        &format!(
            "[API] {} updated DG proto '{}' ({} instance(s) refreshed)",
            user.api_key.owner_character, vnum, refreshed
        ),
    );
    Ok(Json(DgProtoResponse {
        success: true,
        data: to_view(proto),
        warnings,
        refreshed_instances: refreshed,
    }))
}

async fn delete_proto(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }
    let vnum = vnum.trim().to_string();
    let existed = state
        .db
        .get_dg_trigger_proto(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .is_some();
    if !existed {
        return Err(ApiError::NotFound(format!(
            "DG proto '{}' not found",
            vnum
        )));
    }
    state
        .db
        .delete_dg_trigger_proto(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    notify_builders(
        &state.connections,
        &format!(
            "[API] {} deleted DG proto '{}'",
            user.api_key.owner_character, vnum
        ),
    );
    Ok(Json(serde_json::json!({ "success": true })))
}
