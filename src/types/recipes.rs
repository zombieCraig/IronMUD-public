//! Recipe / crafting system types.

use super::serde_defaults::default_one;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub skill: String, // "crafting" or "cooking"
    #[serde(default)]
    pub skill_required: i32, // Minimum skill level (0-10)
    #[serde(default)]
    pub auto_learn: bool, // Learned automatically at skill level?
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,
    #[serde(default)]
    pub tools: Vec<RecipeTool>,
    pub output_vnum: String, // Prototype vnum to spawn
    #[serde(default = "default_one")]
    pub output_quantity: i32, // How many to produce
    #[serde(default)]
    pub base_xp: i32, // XP awarded on success
    #[serde(default = "default_one")]
    pub difficulty: i32, // Affects quality roll (1-10)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeIngredient {
    #[serde(default)]
    pub vnum: Option<String>, // Exact vnum (if specified)
    #[serde(default)]
    pub category: Option<String>, // Category match (e.g., "flour", "meat")
    #[serde(default = "default_one")]
    pub quantity: i32, // How many needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeTool {
    #[serde(default)]
    pub vnum: Option<String>, // Exact tool vnum (if specified)
    #[serde(default)]
    pub category: Option<String>, // Tool category (e.g., "knife", "forge")
    #[serde(default)]
    pub location: ToolLocation, // Where to find it
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ToolLocation {
    #[default]
    Inventory, // Must be in player's inventory
    Room,   // Must be in current room
    Either, // Can be in inventory OR room
}
