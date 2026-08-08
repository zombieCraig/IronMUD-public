//! The content auditor over HTTP, so agent-built content can check itself.
//!
//! Most of this world is built through MCP. Until this existed, that half of
//! the building had no quality signal at all: an agent could create a room with
//! no description, no exits and a duplicate of its neighbour's text, and
//! nothing anywhere would say so — while a human doing the same thing in
//! `redit` got a grade toast on the next keystroke.
//!
//! The plan for the builder tier named the dogfood test as its real acceptance
//! criterion: *build one area through MCP end to end and confirm `audit_area`
//! catches what a human reviewer would have caught.* This is the surface that
//! makes that runnable.
//!
//! # Read-only, and cheap enough
//!
//! Every route here is a read. `/audit/world` and `/audit/area/:key` load the
//! room, item, mobile, quest and spawn trees, which is why they are builder
//! commands rather than anything on a player path — the same rule the in-game
//! `build audit` follows. Single-entity routes are keyed fetches.
//!
//! # Trust
//!
//! Findings are engine-authored strings from `src/audit/mod.rs`, not user text,
//! so unlike `src/api/build_requests.rs` there is nothing here to treat as
//! untrusted. The entity *names* echoed back in a finding message are
//! builder-written, and should be read as data like any other content field.

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::get,
};
use serde::Serialize;
use std::sync::Arc;

use super::{
    ApiState,
    auth::{AuthenticatedUser, can_read},
    error::ApiError,
};
use crate::audit::{self, EntityKind, scan};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/world", get(audit_world))
        .route("/report", get(world_report))
        .route("/tracks", get(build_tracks))
        .route("/area/:key", get(audit_area))
        .route("/room/:key", get(|s, u, p| audit_entity(s, u, p, EntityKind::Room)))
        .route("/item/:key", get(|s, u, p| audit_entity(s, u, p, EntityKind::Item)))
        .route("/mobile/:key", get(|s, u, p| audit_entity(s, u, p, EntityKind::Mobile)))
        .route("/quest/:key", get(|s, u, p| audit_entity(s, u, p, EntityKind::Quest)))
}

#[derive(Serialize)]
pub struct FindingOut {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct GradeOut {
    pub score: i32,
    pub letter: char,
    pub blockers: usize,
    pub warns: usize,
    pub polish: usize,
    pub findings: Vec<FindingOut>,
}

/// One graded entity, with who owns it — the two questions are always asked
/// together, which is why `scan::EntityAudit` carries both.
#[derive(Serialize)]
pub struct EntityOut {
    pub label: String,
    pub name: String,
    pub kind: String,
    #[serde(flatten)]
    pub grade: GradeOut,
    pub authored_by: Option<String>,
    pub last_edited_by: Option<String>,
    pub origin: String,
    /// Whether this content can count toward a builder's score at all.
    pub counts_for_score: bool,
}

#[derive(Serialize)]
pub struct ReportOut {
    pub label: String,
    pub score: i32,
    pub letter: char,
    /// The container's own checks, separate from its contents'.
    pub own: Vec<FindingOut>,
    /// Worst contents first — what to open next, not an exhaustive listing.
    pub worst: Vec<EntityOut>,
    pub counts: ReportCounts,
}

#[derive(Serialize)]
pub struct ReportCounts {
    pub rooms: usize,
    pub items: usize,
    pub mobiles: usize,
    pub quests: usize,
    pub areas: usize,
}

/// How many of the worst entries a report carries. A large area has hundreds of
/// rooms; the score still accounts for every one, this just names the ones
/// worth opening first. Matches `WORST_LIMIT` in `src/script/build.rs`.
const WORST_LIMIT: usize = 15;

fn finding_out(f: &audit::Finding) -> FindingOut {
    FindingOut {
        code: f.code.to_string(),
        severity: f.severity.key().to_string(),
        message: f.message.clone(),
    }
}

fn grade_out(g: &audit::Grade) -> GradeOut {
    GradeOut {
        score: g.score,
        letter: g.letter,
        blockers: g.count(audit::Severity::Blocker),
        warns: g.count(audit::Severity::Warn),
        polish: g.count(audit::Severity::Polish),
        findings: g.findings.iter().map(finding_out).collect(),
    }
}

fn entity_out(kind: EntityKind, e: &scan::EntityAudit) -> EntityOut {
    EntityOut {
        label: e.label.clone(),
        name: e.name.clone(),
        kind: kind.key().to_string(),
        grade: grade_out(&e.grade),
        authored_by: e.provenance.authored_by.clone(),
        last_edited_by: e.provenance.last_edited_by.clone(),
        origin: e.provenance.origin.key().to_string(),
        counts_for_score: e.provenance.origin.counts_for_score(),
    }
}

fn report_out(report: &audit::AuditReport) -> ReportOut {
    ReportOut {
        label: report.label.clone(),
        score: report.score,
        letter: report.letter,
        own: report.own.findings.iter().map(finding_out).collect(),
        worst: report
            .worst(None, WORST_LIMIT)
            .into_iter()
            .map(|e| EntityOut {
                label: e.label.clone(),
                name: e.name.clone(),
                kind: e.kind.key().to_string(),
                grade: grade_out(&e.grade),
                authored_by: None,
                last_edited_by: None,
                origin: String::new(),
                counts_for_score: false,
            })
            .collect(),
        counts: ReportCounts {
            rooms: report.count_of(EntityKind::Room),
            items: report.count_of(EntityKind::Item),
            mobiles: report.count_of(EntityKind::Mobile),
            quests: report.count_of(EntityKind::Quest),
            areas: report.count_of(EntityKind::Area),
        },
    }
}

async fn audit_entity(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(key): Path<String>,
    kind: EntityKind,
) -> Result<Json<EntityOut>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let found = scan::scan_entity(&state.db, kind, &key).map_err(|e| ApiError::Internal(e.to_string()))?;
    // Not found is a 404, not a failing grade: content that does not exist has
    // no quality, and reporting an F would read as "this is bad" rather than
    // "you asked about the wrong vnum".
    let found = found.ok_or_else(|| ApiError::NotFound(format!("No {} by that key: {key}", kind.key())))?;
    Ok(Json(entity_out(kind, &found)))
}

async fn audit_area(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(key): Path<String>,
) -> Result<Json<ReportOut>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let snapshot = scan::WorldSnapshot::load(&state.db).map_err(|e| ApiError::Internal(e.to_string()))?;
    let needle = key.trim().to_lowercase();
    let area = snapshot
        .areas
        .iter()
        .find(|a| a.prefix.to_lowercase() == needle || a.name.to_lowercase() == needle || a.id.to_string() == needle)
        .ok_or_else(|| ApiError::NotFound(format!("No area by that key: {key}")))?;
    // The context is always the *full* room set, even for one area: an
    // area-scoped context calls every exit leaving the area dangling.
    let report = scan::scan_area(&snapshot, area, snapshot.ctx());
    Ok(Json(report_out(&report)))
}

async fn audit_world(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<ReportOut>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let snapshot = scan::WorldSnapshot::load(&state.db).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(report_out(&scan::scan_world(&snapshot))))
}

#[derive(Serialize)]
pub struct ComponentOut {
    pub key: String,
    pub label: String,
    pub score: i32,
    pub weight: i32,
}

#[derive(Serialize)]
pub struct WorldReportOut {
    pub ready: bool,
    pub generated_at: i64,
    pub quality_pct: i32,
    pub score: i32,
    pub tier: String,
    pub tier_label: String,
    pub next_step: String,
    /// Set when a structural absence is holding the rating below what its
    /// component scores would give it.
    pub cap_reason: Option<String>,
    pub components: Vec<ComponentOut>,
    pub rooms: usize,
    pub areas: usize,
    pub quests: usize,
    pub mobiles: usize,
    pub items: usize,
}

/// The cached world rating, as the `world` command reads it.
///
/// Read from the cache rather than recomputed, so asking cannot become an
/// expensive operation an agent runs in a loop. `ready: false` means the first
/// scan has not landed yet — within five minutes of boot — and must be rendered
/// as "not surveyed", never as "the world is empty".
async fn world_report(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<WorldReportOut>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let world = state
        .state
        .lock()
        .map_err(|_| ApiError::Internal("world lock poisoned".into()))?;
    let report = &world.world_report;
    let facts = &report.facts;
    let mut out = WorldReportOut {
        ready: report.is_ready(),
        generated_at: report.generated_at,
        quality_pct: report.quality_pct,
        score: 0,
        tier: String::new(),
        tier_label: String::new(),
        next_step: String::new(),
        cap_reason: None,
        components: Vec::new(),
        rooms: facts.room_count,
        areas: facts.area_count,
        quests: facts.quest_count,
        mobiles: facts.mobile_count,
        items: facts.item_count,
    };
    if let Some(rating) = report.rating.as_ref() {
        out.score = rating.score;
        out.tier = rating.tier_key.to_string();
        out.tier_label = rating.tier_label.to_string();
        out.next_step = rating.next_step();
        out.cap_reason = rating.cap.as_ref().map(|c| c.reason.to_string());
        out.components = rating
            .components
            .iter()
            .map(|c| ComponentOut {
                key: c.key.to_string(),
                label: c.label.to_string(),
                score: c.score,
                weight: c.weight,
            })
            .collect();
    }
    Ok(Json(out))
}

#[derive(Serialize)]
pub struct TrackStepOut {
    pub key: String,
    pub label: String,
    pub hint: String,
    pub done: bool,
}

#[derive(Serialize)]
pub struct TrackOut {
    pub key: String,
    pub name: String,
    pub scope: String,
    pub done: usize,
    pub total: usize,
    /// The first unfinished step, or `None` when the track is complete.
    pub next_step: Option<String>,
    pub steps: Vec<TrackStepOut>,
}

/// Builder progress tracks for the calling key's owner character.
///
/// Builder-scope tracks answer "have you used the engine", which is the
/// tutorial this engine never had — and an agent that can read it can work
/// through it the same way a person does.
async fn build_tracks(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<TrackOut>>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let defs: Vec<crate::build_tracks::TrackDef> = {
        let world = state
            .state
            .lock()
            .map_err(|_| ApiError::Internal("world lock poisoned".into()))?;
        world.build_tracks.clone()
    };
    let snapshot = scan::WorldSnapshot::load(&state.db).map_err(|e| ApiError::Internal(e.to_string()))?;
    let facts = crate::build_tracks::builder_facts(&snapshot, snapshot.ctx(), &user.api_key.owner_character);

    let out: Vec<TrackOut> = defs
        .iter()
        .filter(|d| d.scope == crate::build_tracks::TrackScope::Builder)
        .map(|d| {
            let progress = crate::build_tracks::evaluate(d, &facts);
            TrackOut {
                key: d.key.clone(),
                name: d.name.clone(),
                scope: "builder".to_string(),
                done: progress.done(),
                total: progress.steps.len(),
                next_step: progress.next_step().map(|s| s.label.clone()),
                steps: progress
                    .steps
                    .iter()
                    .map(|s| TrackStepOut {
                        key: s.key.clone(),
                        label: s.label.clone(),
                        hint: s.hint.clone(),
                        done: s.done,
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Json(out))
}
