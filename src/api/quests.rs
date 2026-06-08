//! Quest CRUD endpoints. Quests are keyed by vnum (e.g. `qst:100`); the
//! vnum is the canonical id in the `quests` sled tree. Per-player progress
//! lives on `CharacterData` and is not exposed here.

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
    validate::{DESCRIPTION_MAX, NAME_MAX, SHORT_DESC_MAX, check_text_len},
};
use crate::types::{QuestData, QuestObjective, QuestReward};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_quests).post(create_quest))
        .route("/:vnum", get(get_quest).put(update_quest).delete(delete_quest))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectiveRequest {
    KillMob {
        vnum: String,
        #[serde(default = "default_one_i32")]
        count: i32,
    },
    KillAnyMob {
        #[serde(default)]
        vnums: Vec<String>,
        #[serde(default = "default_one_i32")]
        count: i32,
    },
    BringItem {
        vnum: String,
        #[serde(default = "default_one_i32")]
        qty: i32,
        #[serde(default)]
        return_to_mob_vnum: Option<String>,
    },
    VisitRoom {
        vnum: String,
    },
    DgFlag {
        var: String,
        value: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RewardRequest {
    Gold {
        amount: i64,
    },
    Item {
        vnum: String,
        #[serde(default = "default_one_i32")]
        qty: i32,
    },
    SkillXp {
        skill: String,
        amount: i32,
    },
    Achievement {
        key: String,
    },
    LearnRecipe {
        recipe_id: String,
    },
    EmbraceClan {
        clan: String,
    },
    EmbraceAnarch {
        #[serde(default)]
        discipline: Option<String>,
    },
}

#[derive(Deserialize)]
pub struct CreateQuestRequest {
    pub vnum: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub completion_text: String,
    #[serde(default)]
    pub objectives: Vec<ObjectiveRequest>,
    #[serde(default)]
    pub rewards: Vec<RewardRequest>,
    #[serde(default)]
    pub repeatable: bool,
    #[serde(default)]
    pub giver_mob_vnum: Option<String>,
    #[serde(default)]
    pub prereq_quest_vnum: Option<String>,
    #[serde(default)]
    pub min_player_skill_total: Option<i32>,
    #[serde(default)]
    pub duration_secs: Option<i64>,
    #[serde(default)]
    pub achievement_set_prereq: Option<AchievementSetPrereqRequest>,
}

#[derive(Deserialize)]
pub struct UpdateQuestRequest {
    pub name: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub completion_text: Option<String>,
    pub objectives: Option<Vec<ObjectiveRequest>>,
    pub rewards: Option<Vec<RewardRequest>>,
    pub repeatable: Option<bool>,
    pub giver_mob_vnum: Option<String>,
    pub prereq_quest_vnum: Option<String>,
    pub min_player_skill_total: Option<i32>,
    pub duration_secs: Option<i64>,
    pub achievement_set_prereq: Option<AchievementSetPrereqRequest>,
}

/// Wire shape for `achievement_set_prereq`. Empty `keys` or non-positive
/// `min_count` clears the prereq on update.
#[derive(Deserialize)]
pub struct AchievementSetPrereqRequest {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub min_count: i32,
}

fn convert_achievement_set(req: &AchievementSetPrereqRequest) -> Option<crate::types::AchievementSetPrereq> {
    let cleaned: Vec<String> = req
        .keys
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    if cleaned.is_empty() || req.min_count <= 0 {
        return None;
    }
    Some(crate::types::AchievementSetPrereq {
        keys: cleaned,
        min_count: req.min_count,
    })
}

fn default_one_i32() -> i32 {
    1
}

#[derive(Serialize)]
pub struct QuestResponse {
    pub success: bool,
    pub data: QuestData,
}

#[derive(Serialize)]
pub struct QuestListResponse {
    pub success: bool,
    pub data: Vec<QuestData>,
}

fn convert_objective(req: &ObjectiveRequest) -> Result<QuestObjective, ApiError> {
    match req {
        ObjectiveRequest::KillMob { vnum, count } => {
            if vnum.trim().is_empty() {
                return Err(ApiError::InvalidInput("KillMob vnum required".into()));
            }
            Ok(QuestObjective::KillMob {
                vnum: vnum.clone(),
                count: (*count).max(1),
            })
        }
        ObjectiveRequest::KillAnyMob { vnums, count } => {
            let cleaned: Vec<String> = vnums
                .iter()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect();
            if cleaned.is_empty() {
                return Err(ApiError::InvalidInput("KillAnyMob requires at least one vnum".into()));
            }
            Ok(QuestObjective::KillAnyMob {
                vnums: cleaned,
                count: (*count).max(1),
            })
        }
        ObjectiveRequest::BringItem {
            vnum,
            qty,
            return_to_mob_vnum,
        } => {
            if vnum.trim().is_empty() {
                return Err(ApiError::InvalidInput("BringItem vnum required".into()));
            }
            Ok(QuestObjective::BringItem {
                vnum: vnum.clone(),
                qty: (*qty).max(1),
                return_to_mob_vnum: return_to_mob_vnum
                    .as_ref()
                    .and_then(|s| if s.trim().is_empty() { None } else { Some(s.clone()) }),
            })
        }
        ObjectiveRequest::VisitRoom { vnum } => {
            if vnum.trim().is_empty() {
                return Err(ApiError::InvalidInput("VisitRoom vnum required".into()));
            }
            Ok(QuestObjective::VisitRoom { vnum: vnum.clone() })
        }
        ObjectiveRequest::DgFlag { var, value } => {
            if var.trim().is_empty() {
                return Err(ApiError::InvalidInput("DgFlag var required".into()));
            }
            Ok(QuestObjective::DgFlag {
                var: var.clone(),
                value: value.clone(),
            })
        }
    }
}

fn convert_reward(req: &RewardRequest) -> Result<QuestReward, ApiError> {
    match req {
        RewardRequest::Gold { amount } => Ok(QuestReward::Gold { amount: *amount }),
        RewardRequest::Item { vnum, qty } => {
            if vnum.trim().is_empty() {
                return Err(ApiError::InvalidInput("Item reward vnum required".into()));
            }
            Ok(QuestReward::Item {
                vnum: vnum.clone(),
                qty: (*qty).max(1),
            })
        }
        RewardRequest::SkillXp { skill, amount } => {
            if skill.trim().is_empty() {
                return Err(ApiError::InvalidInput("SkillXp skill required".into()));
            }
            Ok(QuestReward::SkillXp {
                skill: skill.clone(),
                amount: *amount,
            })
        }
        RewardRequest::Achievement { key } => {
            if key.trim().is_empty() {
                return Err(ApiError::InvalidInput("Achievement key required".into()));
            }
            Ok(QuestReward::Achievement { key: key.clone() })
        }
        RewardRequest::LearnRecipe { recipe_id } => {
            if recipe_id.trim().is_empty() {
                return Err(ApiError::InvalidInput("LearnRecipe recipe_id required".into()));
            }
            Ok(QuestReward::LearnRecipe {
                recipe_id: recipe_id.clone(),
            })
        }
        RewardRequest::EmbraceClan { clan } => {
            let trimmed = clan.trim().to_lowercase();
            if trimmed.is_empty() {
                return Err(ApiError::InvalidInput("EmbraceClan clan required".into()));
            }
            Ok(QuestReward::EmbraceClan { clan: trimmed })
        }
        RewardRequest::EmbraceAnarch { discipline } => {
            let normalized = discipline
                .as_ref()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty());
            if let Some(d) = normalized.as_ref() {
                let allowed = crate::script::vampire::known_disciplines();
                if !allowed.iter().any(|x| x == d) {
                    return Err(ApiError::InvalidInput(format!(
                        "EmbraceAnarch discipline `{}` not in known set (expected one of: {})",
                        d,
                        allowed.join(", ")
                    )));
                }
            }
            Ok(QuestReward::EmbraceAnarch { discipline: normalized })
        }
    }
}

async fn list_quests(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<QuestListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let quests = state
        .db
        .list_all_quests()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(QuestListResponse {
        success: true,
        data: quests,
    }))
}

async fn get_quest(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<QuestResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let quest = state
        .db
        .get_quest_data(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Quest '{}' not found", vnum)))?;
    Ok(Json(QuestResponse {
        success: true,
        data: quest,
    }))
}

async fn create_quest(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateQuestRequest>,
) -> Result<Json<QuestResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }
    if req.vnum.trim().is_empty() {
        return Err(ApiError::InvalidInput("vnum is required".into()));
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::InvalidInput("name is required".into()));
    }
    if let Ok(Some(_)) = state.db.get_quest_data(&req.vnum) {
        return Err(ApiError::Conflict(format!(
            "Quest with vnum '{}' already exists",
            req.vnum
        )));
    }

    check_text_len("name", &req.name, NAME_MAX)?;
    check_text_len("summary", &req.summary, SHORT_DESC_MAX)?;
    check_text_len("description", &req.description, DESCRIPTION_MAX)?;
    check_text_len("completion_text", &req.completion_text, DESCRIPTION_MAX)?;

    let objectives = req
        .objectives
        .iter()
        .map(convert_objective)
        .collect::<Result<Vec<_>, _>>()?;
    let rewards = req.rewards.iter().map(convert_reward).collect::<Result<Vec<_>, _>>()?;

    let quest = QuestData {
        vnum: req.vnum,
        name: req.name,
        keywords: req.keywords,
        summary: req.summary,
        description: req.description,
        completion_text: req.completion_text,
        objectives,
        rewards,
        repeatable: req.repeatable,
        giver_mob_vnum: req
            .giver_mob_vnum
            .and_then(|s| if s.trim().is_empty() { None } else { Some(s) }),
        prereq_quest_vnum: req
            .prereq_quest_vnum
            .and_then(|s| if s.trim().is_empty() { None } else { Some(s) }),
        min_player_skill_total: req.min_player_skill_total,
        duration_secs: req.duration_secs.and_then(|n| if n <= 0 { None } else { Some(n) }),
        achievement_set_prereq: req.achievement_set_prereq.as_ref().and_then(convert_achievement_set),
    };

    state
        .db
        .save_quest_data(&quest)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Quest '{}' created by {}", quest.name, user.api_key.name),
    );

    Ok(Json(QuestResponse {
        success: true,
        data: quest,
    }))
}

async fn update_quest(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
    Json(req): Json<UpdateQuestRequest>,
) -> Result<Json<QuestResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut quest = state
        .db
        .get_quest_data(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Quest '{}' not found", vnum)))?;

    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(ApiError::InvalidInput("name cannot be empty".into()));
        }
        check_text_len("name", &name, NAME_MAX)?;
        quest.name = name;
    }
    if let Some(kws) = req.keywords {
        quest.keywords = kws;
    }
    if let Some(s) = req.summary {
        check_text_len("summary", &s, SHORT_DESC_MAX)?;
        quest.summary = s;
    }
    if let Some(s) = req.description {
        check_text_len("description", &s, DESCRIPTION_MAX)?;
        quest.description = s;
    }
    if let Some(s) = req.completion_text {
        check_text_len("completion_text", &s, DESCRIPTION_MAX)?;
        quest.completion_text = s;
    }
    if let Some(objs) = req.objectives {
        quest.objectives = objs.iter().map(convert_objective).collect::<Result<Vec<_>, _>>()?;
    }
    if let Some(rs) = req.rewards {
        quest.rewards = rs.iter().map(convert_reward).collect::<Result<Vec<_>, _>>()?;
    }
    if let Some(v) = req.repeatable {
        quest.repeatable = v;
    }
    if let Some(v) = req.giver_mob_vnum {
        quest.giver_mob_vnum = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = req.prereq_quest_vnum {
        quest.prereq_quest_vnum = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = req.min_player_skill_total {
        quest.min_player_skill_total = if v <= 0 { None } else { Some(v) };
    }
    if let Some(v) = req.duration_secs {
        quest.duration_secs = if v <= 0 { None } else { Some(v) };
    }
    if let Some(set_req) = req.achievement_set_prereq {
        quest.achievement_set_prereq = convert_achievement_set(&set_req);
    }

    state
        .db
        .save_quest_data(&quest)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Quest '{}' updated by {}", quest.name, user.api_key.name),
    );

    Ok(Json(QuestResponse {
        success: true,
        data: quest,
    }))
}

async fn delete_quest(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }
    let quest = state
        .db
        .get_quest_data(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Quest '{}' not found", vnum)))?;
    state
        .db
        .delete_quest(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    notify_builders(
        &state.connections,
        &format!("[API] Quest '{}' deleted by {}", quest.name, user.api_key.name),
    );
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Quest '{}' deleted", quest.name)
    })))
}
