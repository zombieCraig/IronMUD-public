//! Recipe CRUD endpoints (crafting and cooking formulas)
//!
//! Recipes are keyed by vnum (e.g. `smith:iron_sword`). Unlike most other
//! entities they have no UUID — the vnum is the canonical id in the `recipes`
//! sled tree.

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
use crate::{Recipe, RecipeIngredient, RecipeTool, ToolLocation};

pub fn routes() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(list_recipes).post(create_recipe))
        .route("/:vnum", get(get_recipe).put(update_recipe).delete(delete_recipe))
}

#[derive(Deserialize)]
pub struct IngredientRequest {
    #[serde(default)]
    pub vnum: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default = "default_one")]
    pub quantity: i32,
}

#[derive(Deserialize)]
pub struct ToolRequest {
    #[serde(default)]
    pub vnum: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// "inventory" | "room" | "either" (default: "inventory")
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateRecipeRequest {
    /// Recipe vnum, e.g. "smith:iron_sword". Used as the unique key.
    pub vnum: String,
    pub name: String,
    /// "crafting" or "cooking"
    pub skill: String,
    #[serde(default)]
    pub skill_required: i32,
    #[serde(default)]
    pub auto_learn: bool,
    #[serde(default)]
    pub ingredients: Vec<IngredientRequest>,
    #[serde(default)]
    pub tools: Vec<ToolRequest>,
    pub output_vnum: String,
    #[serde(default = "default_one")]
    pub output_quantity: i32,
    #[serde(default)]
    pub base_xp: i32,
    #[serde(default = "default_one")]
    pub difficulty: i32,
}

#[derive(Deserialize)]
pub struct UpdateRecipeRequest {
    pub name: Option<String>,
    pub skill: Option<String>,
    pub skill_required: Option<i32>,
    pub auto_learn: Option<bool>,
    pub ingredients: Option<Vec<IngredientRequest>>,
    pub tools: Option<Vec<ToolRequest>>,
    pub output_vnum: Option<String>,
    pub output_quantity: Option<i32>,
    pub base_xp: Option<i32>,
    pub difficulty: Option<i32>,
}

fn default_one() -> i32 {
    1
}

#[derive(Serialize)]
pub struct RecipeResponse {
    pub success: bool,
    pub data: Recipe,
}

#[derive(Serialize)]
pub struct RecipeListResponse {
    pub success: bool,
    pub data: Vec<Recipe>,
}

fn parse_skill(s: &str) -> Result<String, ApiError> {
    let lower = s.to_lowercase();
    if lower == "crafting" || lower == "cooking" {
        Ok(lower)
    } else {
        Err(ApiError::InvalidInput(format!(
            "Invalid skill '{}'. Valid: crafting, cooking",
            s
        )))
    }
}

fn parse_location(s: &str) -> Result<ToolLocation, ApiError> {
    match s.to_lowercase().as_str() {
        "inv" | "inventory" => Ok(ToolLocation::Inventory),
        "room" => Ok(ToolLocation::Room),
        "either" => Ok(ToolLocation::Either),
        _ => Err(ApiError::InvalidInput(format!(
            "Invalid tool location '{}'. Valid: inventory, room, either",
            s
        ))),
    }
}

fn convert_ingredient(req: &IngredientRequest) -> Result<RecipeIngredient, ApiError> {
    if req.vnum.is_none() && req.category.is_none() {
        return Err(ApiError::InvalidInput(
            "Each ingredient must specify either 'vnum' or 'category'".into(),
        ));
    }
    if req.quantity < 1 {
        return Err(ApiError::InvalidInput("Ingredient quantity must be >= 1".into()));
    }
    Ok(RecipeIngredient {
        vnum: req.vnum.clone(),
        category: req.category.clone(),
        quantity: req.quantity,
    })
}

fn convert_tool(req: &ToolRequest) -> Result<RecipeTool, ApiError> {
    if req.vnum.is_none() && req.category.is_none() {
        return Err(ApiError::InvalidInput(
            "Each tool must specify either 'vnum' or 'category'".into(),
        ));
    }
    let location = match &req.location {
        Some(l) => parse_location(l)?,
        None => ToolLocation::Inventory,
    };
    Ok(RecipeTool {
        vnum: req.vnum.clone(),
        category: req.category.clone(),
        location,
    })
}

/// List all recipes
async fn list_recipes(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<RecipeListResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let recipes = state
        .db
        .list_all_recipes()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(RecipeListResponse {
        success: true,
        data: recipes,
    }))
}

/// Get a recipe by vnum
async fn get_recipe(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<RecipeResponse>, ApiError> {
    if !can_read(&user) {
        return Err(ApiError::Forbidden("Read permission required".into()));
    }
    let recipe = state
        .db
        .get_recipe(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Recipe '{}' not found", vnum)))?;
    Ok(Json(RecipeResponse {
        success: true,
        data: recipe,
    }))
}

/// Create a new recipe
async fn create_recipe(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateRecipeRequest>,
) -> Result<Json<RecipeResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    if req.vnum.trim().is_empty() {
        return Err(ApiError::InvalidInput("vnum is required".into()));
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::InvalidInput("name is required".into()));
    }
    if req.output_vnum.trim().is_empty() {
        return Err(ApiError::InvalidInput("output_vnum is required".into()));
    }

    if let Ok(Some(_)) = state.db.get_recipe(&req.vnum) {
        return Err(ApiError::Conflict(format!(
            "Recipe with vnum '{}' already exists",
            req.vnum
        )));
    }

    let skill = parse_skill(&req.skill)?;

    let ingredients = req
        .ingredients
        .iter()
        .map(convert_ingredient)
        .collect::<Result<Vec<_>, _>>()?;
    let tools = req.tools.iter().map(convert_tool).collect::<Result<Vec<_>, _>>()?;

    let recipe = Recipe {
        id: req.vnum,
        name: req.name,
        skill,
        skill_required: req.skill_required.clamp(0, 10),
        auto_learn: req.auto_learn,
        ingredients,
        tools,
        output_vnum: req.output_vnum,
        output_quantity: req.output_quantity.max(1),
        base_xp: req.base_xp.max(0),
        difficulty: req.difficulty.clamp(1, 10),
    };

    state
        .db
        .save_recipe(recipe.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Recipe '{}' created by {}", recipe.name, user.api_key.name),
    );

    Ok(Json(RecipeResponse {
        success: true,
        data: recipe,
    }))
}

/// Update an existing recipe
async fn update_recipe(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
    Json(req): Json<UpdateRecipeRequest>,
) -> Result<Json<RecipeResponse>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let mut recipe = state
        .db
        .get_recipe(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Recipe '{}' not found", vnum)))?;

    if let Some(name) = req.name {
        recipe.name = name;
    }
    if let Some(skill) = req.skill {
        recipe.skill = parse_skill(&skill)?;
    }
    if let Some(v) = req.skill_required {
        recipe.skill_required = v.clamp(0, 10);
    }
    if let Some(v) = req.auto_learn {
        recipe.auto_learn = v;
    }
    if let Some(ingredients) = req.ingredients {
        recipe.ingredients = ingredients
            .iter()
            .map(convert_ingredient)
            .collect::<Result<Vec<_>, _>>()?;
    }
    if let Some(tools) = req.tools {
        recipe.tools = tools.iter().map(convert_tool).collect::<Result<Vec<_>, _>>()?;
    }
    if let Some(v) = req.output_vnum {
        if v.trim().is_empty() {
            return Err(ApiError::InvalidInput("output_vnum cannot be empty".into()));
        }
        recipe.output_vnum = v;
    }
    if let Some(v) = req.output_quantity {
        recipe.output_quantity = v.max(1);
    }
    if let Some(v) = req.base_xp {
        recipe.base_xp = v.max(0);
    }
    if let Some(v) = req.difficulty {
        recipe.difficulty = v.clamp(1, 10);
    }

    state
        .db
        .save_recipe(recipe.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Recipe '{}' updated by {}", recipe.name, user.api_key.name),
    );

    Ok(Json(RecipeResponse {
        success: true,
        data: recipe,
    }))
}

/// Delete a recipe
async fn delete_recipe(
    State(state): State<Arc<ApiState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(vnum): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !can_write(&user) {
        return Err(ApiError::Forbidden("Write permission required".into()));
    }

    let recipe = state
        .db
        .get_recipe(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Recipe '{}' not found", vnum)))?;

    state
        .db
        .delete_recipe(&vnum)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    notify_builders(
        &state.connections,
        &format!("[API] Recipe '{}' deleted by {}", recipe.name, user.api_key.name),
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Recipe '{}' deleted", recipe.name)
    })))
}
