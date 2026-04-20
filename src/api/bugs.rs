//! Bug report API endpoints
//! All GET endpoints only return admin-approved reports to protect against prompt injection.

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read, can_write},
    error::ApiError,
};
use crate::{AdminNote, BugPriority, BugReport, BugStatus};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_bugs))
        .route("/:id", get(get_bug).put(update_bug).delete(delete_bug))
        .route("/by-ticket/:num", get(get_bug_by_ticket))
        .route("/:id/notes", axum::routing::post(add_note))
}

#[derive(Deserialize)]
pub struct ListBugsQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct UpdateBugRequest {
    pub status: Option<String>,
    pub priority: Option<String>,
}

#[derive(Deserialize)]
pub struct AddNoteRequest {
    pub author: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct BugReportResponse {
    pub success: bool,
    pub data: BugReport,
}

#[derive(Serialize)]
pub struct BugReportsListResponse {
    pub success: bool,
    pub data: Vec<BugReport>,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub success: bool,
}

/// List bug reports (approved only)
async fn list_bugs(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ListBugsQuery>,
) -> Result<Json<BugReportsListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let status_filter = query.status.as_ref().and_then(|s| BugStatus::from_str(s));

    // API always returns approved-only
    let mut reports = state
        .db
        .list_bug_reports(status_filter.as_ref(), true)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Apply offset/limit
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);
    if offset > 0 {
        reports = reports.into_iter().skip(offset).collect();
    }
    reports.truncate(limit);

    Ok(Json(BugReportsListResponse {
        success: true,
        data: reports,
    }))
}

/// Get bug report by UUID (approved only)
async fn get_bug(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<BugReportResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let report = state
        .db
        .get_bug_report(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Bug report '{}' not found", id)))?;

    // API only returns approved reports
    if !report.approved {
        return Err(ApiError::NotFound(format!("Bug report '{}' not found", id)));
    }

    Ok(Json(BugReportResponse {
        success: true,
        data: report,
    }))
}

/// Get bug report by ticket number (approved only)
async fn get_bug_by_ticket(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(num): Path<i64>,
) -> Result<Json<BugReportResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }

    let report = state
        .db
        .get_bug_report_by_ticket(num)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Bug report #{} not found", num)))?;

    // API only returns approved reports
    if !report.approved {
        return Err(ApiError::NotFound(format!("Bug report #{} not found", num)));
    }

    Ok(Json(BugReportResponse {
        success: true,
        data: report,
    }))
}

/// Update bug report status/priority
async fn update_bug(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBugRequest>,
) -> Result<Json<BugReportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut report = state
        .db
        .get_bug_report(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Bug report '{}' not found", id)))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if let Some(ref status_str) = body.status {
        let new_status = BugStatus::from_str(status_str).ok_or_else(|| {
            ApiError::InvalidInput(format!(
                "Invalid status: {}. Valid: open, inprogress, resolved, closed",
                status_str
            ))
        })?;
        if new_status == BugStatus::Closed || new_status == BugStatus::Resolved {
            report.resolved_at = Some(now);
        }
        report.status = new_status;
    }

    if let Some(ref priority_str) = body.priority {
        let new_priority = BugPriority::from_str(priority_str).ok_or_else(|| {
            ApiError::InvalidInput(format!(
                "Invalid priority: {}. Valid: low, normal, high, critical",
                priority_str
            ))
        })?;
        report.priority = new_priority;
    }

    report.updated_at = now;
    state
        .db
        .save_bug_report(report.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(BugReportResponse {
        success: true,
        data: report,
    }))
}

/// Add admin note to a bug report
async fn add_note(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(body): Json<AddNoteRequest>,
) -> Result<Json<BugReportResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let mut report = state
        .db
        .get_bug_report(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Bug report '{}' not found", id)))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    report.admin_notes.push(AdminNote {
        author: body.author,
        message: body.message,
        created_at: now,
    });
    report.updated_at = now;

    state
        .db
        .save_bug_report(report.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(BugReportResponse {
        success: true,
        data: report,
    }))
}

/// Delete bug report
async fn delete_bug(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::InvalidInput("Invalid UUID format".into()))?;

    let deleted = state
        .db
        .delete_bug_report(&uuid)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if !deleted {
        return Err(ApiError::NotFound(format!("Bug report '{}' not found", id)));
    }

    Ok(Json(DeleteResponse { success: true }))
}
