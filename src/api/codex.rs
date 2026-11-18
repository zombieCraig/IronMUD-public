//! Divine spellcraft codex endpoints: read the codex and each god's canon,
//! and (admin only) moderate codex spells. Moderation goes through
//! `crate::spellcraft::codex`, which writes the record and mirrors it into
//! `World.spell_definitions`.

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read},
    error::ApiError,
    notify_builders,
};
use crate::spellcraft::codex as codex_ops;
use crate::types::{CanonStatus, CodexEntry, SpellOrigin};

/// Routes nested under `/codex`.
pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_codex))
        .route("/:id", get(get_codex))
        .route("/:id/status", put(set_status))
        .route("/:id/veto", put(set_veto))
}

/// Routes nested under `/canons`.
pub fn canon_routes() -> Router<Arc<ApiState>> {
    Router::new().route("/:god", get(get_canon))
}

#[derive(Serialize)]
pub struct CodexSummary {
    pub id: String,
    pub name: String,
    pub god_vnum: String,
    pub god_name: String,
    pub status: CanonStatus,
    pub origin: SpellOrigin,
    pub author: Option<String>,
    pub author_disgraced: bool,
    pub vetoed: bool,
    pub spell_type: String,
    pub tier: u8,
    pub sealed_day: i64,
    pub casts_by_others: u64,
}

#[derive(Serialize)]
pub struct CodexListResponse {
    pub success: bool,
    pub data: Vec<CodexSummary>,
}

#[derive(Serialize)]
pub struct CodexResponse {
    pub success: bool,
    pub data: CodexEntry,
}

#[derive(Serialize)]
pub struct CanonSlotView {
    pub spell_id: String,
    /// Spell name from the codex; None if the codex record is missing.
    pub spell_name: Option<String>,
    pub sealed_day: i64,
    pub devotion: f64,
    pub warned: bool,
    pub devotion_day: i64,
}

#[derive(Serialize)]
pub struct CanonView {
    pub god_vnum: String,
    pub god_name: String,
    pub influence: u32,
    pub last_petition_day: Option<i64>,
    pub slots: Vec<CanonSlotView>,
}

#[derive(Serialize)]
pub struct CanonResponse {
    pub success: bool,
    pub data: CanonView,
}

fn require_read(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if can_read(user) {
        Ok(())
    } else {
        Err(ApiError::Forbidden("Read permission required".into()))
    }
}

fn require_admin(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.api_key.permissions.admin {
        Ok(())
    } else {
        Err(ApiError::Forbidden("Admin permission required".into()))
    }
}

async fn list_codex(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<CodexListResponse>, ApiError> {
    require_read(&user)?;
    let mut entries = state.db.list_codex()?;
    entries.sort_by(|a, b| a.def.id.cmp(&b.def.id));
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let data = entries
        .into_iter()
        .map(|e| {
            let god_name = names
                .entry(e.meta.god_vnum.clone())
                .or_insert_with(|| crate::spellcraft::price::god_name(&state.db, &e.meta.god_vnum))
                .clone();
            CodexSummary {
                tier: crate::spellcraft::power::spell_tier(&e.def),
                id: e.def.id,
                name: e.def.name,
                god_vnum: e.meta.god_vnum,
                god_name,
                status: e.meta.status,
                origin: e.meta.origin,
                author: e.meta.author,
                author_disgraced: e.meta.author_disgraced,
                vetoed: e.meta.vetoed,
                spell_type: e.def.spell_type,
                sealed_day: e.meta.sealed_day,
                casts_by_others: e.meta.casts_by_others,
            }
        })
        .collect();
    Ok(Json(CodexListResponse { success: true, data }))
}

async fn get_codex(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<CodexResponse>, ApiError> {
    require_read(&user)?;
    match state.db.get_codex(&id)? {
        Some(entry) => Ok(Json(CodexResponse {
            success: true,
            data: entry,
        })),
        None => Err(ApiError::NotFound(format!("Codex spell '{}' not found", id))),
    }
}

async fn get_canon(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(god): Path<String>,
) -> Result<Json<CanonResponse>, ApiError> {
    require_read(&user)?;
    // `get_canon` returns an empty canon for an unknown god; treat a canon
    // that has never held a spell nor taken a petition as absent.
    let canon = state.db.get_canon(&god)?;
    if canon.slots.is_empty() && canon.last_petition_day.is_none() {
        return Err(ApiError::NotFound(format!("No canon for god '{}'", god)));
    }
    let mut slots = Vec::with_capacity(canon.slots.len());
    for s in canon.slots {
        let spell_name = state.db.get_codex(&s.spell_id)?.map(|e| e.def.name);
        slots.push(CanonSlotView {
            spell_id: s.spell_id,
            spell_name,
            sealed_day: s.sealed_day,
            devotion: s.devotion,
            warned: s.warned,
            devotion_day: s.devotion_day,
        });
    }
    let god_name = crate::spellcraft::price::god_name(&state.db, &canon.god_vnum);
    Ok(Json(CanonResponse {
        success: true,
        data: CanonView {
            god_vnum: canon.god_vnum,
            god_name,
            influence: canon.influence,
            last_petition_day: canon.last_petition_day,
            slots,
        },
    }))
}

#[derive(Deserialize)]
pub struct SetStatusRequest {
    pub status: String,
}

#[derive(Deserialize)]
pub struct SetVetoRequest {
    pub vetoed: bool,
}

/// Map a moderation failure: a missing spell is 404, anything else 500.
fn moderation_error(db: &crate::db::Db, id: &str, msg: String) -> ApiError {
    match db.get_codex(id) {
        Ok(None) => ApiError::NotFound(format!("Codex spell '{}' not found", id)),
        _ => ApiError::Internal(msg),
    }
}

async fn set_status(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<SetStatusRequest>,
) -> Result<Json<CodexResponse>, ApiError> {
    require_admin(&user)?;
    let status = codex_ops::parse_status(&req.status)
        .ok_or_else(|| ApiError::InvalidInput("status must be one of: canon, faded, lost".into()))?;
    let entry =
        codex_ops::set_status(&state.db, &state.state, &id, status).map_err(|e| moderation_error(&state.db, &id, e))?;
    notify_builders(
        &state.connections,
        &format!(
            "[API] {} set codex spell '{}' to {}",
            user.api_key.owner_character,
            id,
            codex_ops::status_str(status)
        ),
    );
    Ok(Json(CodexResponse {
        success: true,
        data: entry,
    }))
}

async fn set_veto(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(req): Json<SetVetoRequest>,
) -> Result<Json<CodexResponse>, ApiError> {
    require_admin(&user)?;
    let entry = codex_ops::set_veto(&state.db, &state.state, &id, req.vetoed)
        .map_err(|e| moderation_error(&state.db, &id, e))?;
    notify_builders(
        &state.connections,
        &format!(
            "[API] {} {} codex spell '{}'",
            user.api_key.owner_character,
            if req.vetoed { "vetoed" } else { "lifted the veto on" },
            id
        ),
    );
    Ok(Json(CodexResponse {
        success: true,
        data: entry,
    }))
}
