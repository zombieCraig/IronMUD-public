// src/script/crafting.rs
// Crafting and cooking recipe functions

use rhai::Engine;
use std::sync::Arc;
use crate::db::Db;
use crate::{ItemData, ItemLocation, ItemType, Recipe, RecipeIngredient, RecipeTool, ToolLocation};
use crate::SharedState;

/// Register crafting-related functions
pub fn register(engine: &mut Engine, db: Arc<Db>, state: SharedState) {
    // ========== Crafting/Cooking Recipe Functions ==========

    // Register Recipe type with getters
    engine.register_type_with_name::<Recipe>("Recipe")
        .register_get("id", |r: &mut Recipe| r.id.clone())
        .register_get("name", |r: &mut Recipe| r.name.clone())
        .register_get("skill", |r: &mut Recipe| r.skill.clone())
        .register_get("skill_required", |r: &mut Recipe| r.skill_required as i64)
        .register_get("auto_learn", |r: &mut Recipe| r.auto_learn)
        .register_get("ingredients", |r: &mut Recipe| {
            r.ingredients.iter().map(|i| rhai::Dynamic::from(i.clone())).collect::<Vec<_>>()
        })
        .register_get("tools", |r: &mut Recipe| {
            r.tools.iter().map(|t| rhai::Dynamic::from(t.clone())).collect::<Vec<_>>()
        })
        .register_get("output_vnum", |r: &mut Recipe| r.output_vnum.clone())
        .register_get("output_quantity", |r: &mut Recipe| r.output_quantity as i64)
        .register_get("base_xp", |r: &mut Recipe| r.base_xp as i64)
        .register_get("difficulty", |r: &mut Recipe| r.difficulty as i64);

    // Register RecipeIngredient type with getters
    engine.register_type_with_name::<RecipeIngredient>("RecipeIngredient")
        .register_get("vnum", |i: &mut RecipeIngredient| {
            i.vnum.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT)
        })
        .register_get("category", |i: &mut RecipeIngredient| {
            i.category.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT)
        })
        .register_get("quantity", |i: &mut RecipeIngredient| i.quantity as i64);

    // Register RecipeTool type with getters
    engine.register_type_with_name::<RecipeTool>("RecipeTool")
        .register_get("vnum", |t: &mut RecipeTool| {
            t.vnum.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT)
        })
        .register_get("category", |t: &mut RecipeTool| {
            t.category.clone().map(rhai::Dynamic::from).unwrap_or(rhai::Dynamic::UNIT)
        })
        .register_get("location", |t: &mut RecipeTool| {
            match t.location {
                ToolLocation::Inventory => "Inventory".to_string(),
                ToolLocation::Room => "Room".to_string(),
                ToolLocation::Either => "Either".to_string(),
            }
        });

    // get_all_recipes() -> Array<Recipe>
    let cloned_state = state.clone();
    engine.register_fn("get_all_recipes", move || -> Vec<rhai::Dynamic> {
        let state_guard = cloned_state.lock().unwrap();
        state_guard.recipes.values()
            .map(|r: &Recipe| rhai::Dynamic::from(r.clone()))
            .collect()
    });

    // get_recipes_by_skill(skill) -> Array<Recipe>
    let cloned_state = state.clone();
    engine.register_fn("get_recipes_by_skill", move |skill: String| -> Vec<rhai::Dynamic> {
        let state_guard = cloned_state.lock().unwrap();
        let skill_lower = skill.to_lowercase();
        state_guard.recipes.values()
            .filter(|r| r.skill.to_lowercase() == skill_lower)
            .map(|r: &Recipe| rhai::Dynamic::from(r.clone()))
            .collect()
    });

    // get_recipe(id) -> Recipe or ()
    let cloned_state = state.clone();
    engine.register_fn("get_recipe", move |id: String| -> rhai::Dynamic {
        let state_guard = cloned_state.lock().unwrap();
        match state_guard.recipes.get(&id) {
            Some(recipe) => rhai::Dynamic::from(recipe.clone()),
            None => rhai::Dynamic::UNIT,
        }
    });

    // knows_recipe(char_name, recipe_id) -> bool
    // Returns true if character knows the recipe (either auto-learned at skill level or explicitly learned)
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("knows_recipe", move |char_name: String, recipe_id: String| -> bool {
        // Get the recipe
        let recipe: Recipe = {
            let state_guard = cloned_state.lock().unwrap();
            match state_guard.recipes.get(&recipe_id) {
                Some(r) => r.clone(),
                None => return false,
            }
        };

        // Get the character
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => {
                // Check if explicitly learned
                if char.learned_recipes.contains(&recipe_id) {
                    return true;
                }

                // Check if auto-learned at skill level
                if recipe.auto_learn {
                    let skill_level = char.skills.get(&recipe.skill.to_lowercase())
                        .map(|s| s.level)
                        .unwrap_or(0);
                    return skill_level >= recipe.skill_required;
                }

                false
            }
            _ => false,
        }
    });

    // learn_recipe(char_name, recipe_id) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("learn_recipe", move |char_name: String, recipe_id: String| -> bool {
        // Verify recipe exists
        {
            let state_guard = cloned_state.lock().unwrap();
            if !state_guard.recipes.contains_key(&recipe_id) {
                return false;
            }
        }

        // Add to character's learned recipes
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(mut char)) => {
                char.learned_recipes.insert(recipe_id);
                cloned_db.save_character_data(char).is_ok()
            }
            _ => false,
        }
    });

    // get_learned_recipes(char_name) -> Array<String>
    let cloned_db = db.clone();
    engine.register_fn("get_learned_recipes", move |char_name: String| -> Vec<rhai::Dynamic> {
        match cloned_db.get_character_data(&char_name.to_lowercase()) {
            Ok(Some(char)) => {
                char.learned_recipes.iter()
                    .map(|id: &String| rhai::Dynamic::from(id.clone()))
                    .collect()
            }
            _ => Vec::new(),
        }
    });

    // find_ingredients(char_name, ingredient) -> Array<item_id strings>
    // Finds items in inventory matching the ingredient (by vnum or category)
    // Returns up to quantity matching item IDs
    let cloned_db = db.clone();
    engine.register_fn("find_ingredients", move |char_name: String, ingredient: RecipeIngredient| -> Vec<rhai::Dynamic> {
        let mut matches = Vec::new();

        // Get character's inventory items
        let items = match cloned_db.get_items_in_inventory(&char_name.to_lowercase()) {
            Ok(items) => items,
            Err(_) => return Vec::new(),
        };

        // Check if this is a liquid ingredient (category starts with "@liquid:")
        let is_liquid_ingredient = ingredient.category.as_ref()
            .map(|c| c.starts_with("@liquid:"))
            .unwrap_or(false);

        if is_liquid_ingredient {
            // Extract liquid type from "@liquid:TYPE" format
            let liquid_type_str = ingredient.category.as_ref().unwrap()[8..].to_lowercase();

            // Find all liquid containers with matching liquid type that have liquid
            for item in items {
                if item.item_type == ItemType::LiquidContainer
                    && item.liquid_type.to_display_string().to_lowercase() == liquid_type_str
                    && item.liquid_current > 0
                {
                    matches.push(rhai::Dynamic::from(item.id.to_string()));
                    // Don't limit by quantity for liquids - return all matching containers
                    // The Rhai script will check if total liquid amount is sufficient
                }
            }
        } else {
            // Standard item matching by vnum or category
            for item in items {
                let is_match = if let Some(ref vnum) = ingredient.vnum {
                    // Match by exact vnum
                    item.vnum.as_ref() == Some(vnum)
                } else if let Some(ref category) = ingredient.category {
                    // Match by category
                    item.categories.iter().any(|c| c.to_lowercase() == category.to_lowercase())
                } else {
                    false
                };

                if is_match {
                    matches.push(rhai::Dynamic::from(item.id.to_string()));
                    if matches.len() >= ingredient.quantity as usize {
                        break;
                    }
                }
            }
        }

        matches
    });

    // find_tool(char_name, room_id, tool) -> item_id string or ()
    // Finds a tool matching the requirements in inventory, room, or either
    let cloned_db = db.clone();
    engine.register_fn("find_tool", move |char_name: String, room_id: String, tool: RecipeTool| -> rhai::Dynamic {
        let room_uuid = uuid::Uuid::parse_str(&room_id).ok();

        // Helper to check if item matches tool requirements
        let matches_tool = |item: &ItemData, tool: &RecipeTool| -> bool {
            if let Some(ref vnum) = tool.vnum {
                item.vnum.as_ref() == Some(vnum)
            } else if let Some(ref category) = tool.category {
                item.categories.iter().any(|c| c.to_lowercase() == category.to_lowercase())
            } else {
                false
            }
        };

        // Check inventory if location allows
        if tool.location == ToolLocation::Inventory || tool.location == ToolLocation::Either {
            if let Ok(items) = cloned_db.get_items_in_inventory(&char_name.to_lowercase()) {
                for item in items {
                    if matches_tool(&item, &tool) {
                        return rhai::Dynamic::from(item.id.to_string());
                    }
                }
            }
        }

        // Check room if location allows
        if (tool.location == ToolLocation::Room || tool.location == ToolLocation::Either) && room_uuid.is_some() {
            if let Ok(items) = cloned_db.get_items_in_room(&room_uuid.unwrap()) {
                for item in items {
                    if matches_tool(&item, &tool) {
                        return rhai::Dynamic::from(item.id.to_string());
                    }
                }
            }
        }

        rhai::Dynamic::UNIT
    });

    // consume_items(item_ids) -> bool
    // Deletes all items in the provided array (for consuming ingredients)
    let cloned_db = db.clone();
    engine.register_fn("consume_items", move |item_ids: rhai::Array| -> bool {
        for id_dyn in item_ids {
            if let Ok(id_str) = id_dyn.into_string() {
                if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                    if cloned_db.delete_item(&uuid).is_err() {
                        return false;
                    }
                }
            }
        }
        true
    });

    // consume_liquid_ingredient(container_ids, amount_needed) -> bool
    // Reduces liquid in containers to consume the specified amount
    // Containers are never deleted - they remain for refilling
    let cloned_db = db.clone();
    engine.register_fn("consume_liquid_ingredient", move |container_ids: rhai::Array, amount_needed: i64| -> bool {
        let mut remaining = amount_needed;

        for id_dyn in container_ids {
            if remaining <= 0 {
                break;
            }

            if let Ok(id_str) = id_dyn.into_string() {
                if let Ok(uuid) = uuid::Uuid::parse_str(&id_str) {
                    if let Ok(Some(mut item)) = cloned_db.get_item_data(&uuid) {
                        // Only process liquid containers with liquid
                        if item.item_type == ItemType::LiquidContainer && item.liquid_current > 0 {
                            // Calculate how much to take from this container
                            let take_amount = std::cmp::min(item.liquid_current as i64, remaining);
                            item.liquid_current -= take_amount as i32;
                            remaining -= take_amount;

                            // Save the updated container (keep it even if empty)
                            if cloned_db.save_item_data(item).is_err() {
                                return false;
                            }
                        }
                    }
                }
            }
        }

        // Return true if we consumed enough liquid
        remaining <= 0
    });

    // spawn_crafted_item(vnum, char_name, quality_tier) -> ItemData or ()
    // Spawns an item from prototype and sets its quality based on tier
    // quality_tier: 0=Poor(25), 1=Normal(50), 2=Good(75), 3=Excellent(100)
    let cloned_db = db.clone();
    engine.register_fn("spawn_crafted_item", move |vnum: String, char_name: String, quality_tier: i64| -> rhai::Dynamic {
        // Find prototype
        let prototype = match cloned_db.get_item_by_vnum(&vnum) {
            Ok(Some(p)) => p,
            _ => return rhai::Dynamic::UNIT,
        };

        // Create new item from prototype
        let mut item = ItemData::new(
            prototype.name.clone(),
            prototype.short_desc.clone(),
            prototype.long_desc.clone(),
        );
        item.vnum = prototype.vnum.clone();
        item.keywords = prototype.keywords.clone();
        item.item_type = prototype.item_type.clone();
        item.value = prototype.value;
        item.weight = prototype.weight;
        item.flags = prototype.flags.clone();
        item.wear_locations = prototype.wear_locations.clone();
        item.categories = prototype.categories.clone();

        // Copy food properties if applicable
        item.food_nutrition = prototype.food_nutrition;
        item.food_effects = prototype.food_effects.clone();
        item.food_spoil_duration = prototype.food_spoil_duration;

        // Copy other relevant properties
        item.triggers = prototype.triggers.clone();

        // Set quality based on tier
        item.quality = match quality_tier {
            0 => 25,   // Poor
            1 => 50,   // Normal
            2 => 75,   // Good
            _ => 100,  // Excellent (3+)
        };

        // Apply quality multiplier to food nutrition if applicable
        if item.food_nutrition > 0 {
            let multiplier = match quality_tier {
                0 => 0.7,
                1 => 1.0,
                2 => 1.2,
                _ => 1.5,
            };
            item.food_nutrition = (item.food_nutrition as f64 * multiplier) as i32;
        }

        // Set item location to character inventory
        item.location = ItemLocation::Inventory(char_name.to_lowercase());

        // Save the item
        match cloned_db.save_item_data(item.clone()) {
            Ok(_) => rhai::Dynamic::from(item),
            Err(_) => rhai::Dynamic::UNIT,
        }
    });

    // get_item_category(item_id) -> String or () (backward compat: returns first category)
    let cloned_db = db.clone();
    engine.register_fn("get_item_category", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => {
                    match item.categories.first() {
                        Some(cat) => rhai::Dynamic::from(cat.clone()),
                        None => rhai::Dynamic::UNIT,
                    }
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // set_item_category(item_id, category) -> bool (backward compat: sets as single-element vec)
    let cloned_db = db.clone();
    engine.register_fn("set_item_category", move |item_id: String, category: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.categories = if category.is_empty() { Vec::new() } else { vec![category.to_lowercase()] };
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_item_categories(item_id) -> Array<String>
    let cloned_db = db.clone();
    engine.register_fn("get_item_categories", move |item_id: String| -> Vec<rhai::Dynamic> {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => {
                    item.categories.iter().map(|c| rhai::Dynamic::from(c.clone())).collect()
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    });

    // set_item_categories(item_id, categories_array) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_categories", move |item_id: String, categories: rhai::Array| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.categories = categories.into_iter()
                        .filter_map(|c| c.into_string().ok())
                        .map(|c| c.to_lowercase())
                        .collect();
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // add_item_category(item_id, category) -> bool
    let cloned_db = db.clone();
    engine.register_fn("add_item_category", move |item_id: String, category: String| -> bool {
        if category.is_empty() {
            return false;
        }
        let cat_lower = category.to_lowercase();
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    if !item.categories.iter().any(|c| c.to_lowercase() == cat_lower) {
                        item.categories.push(cat_lower);
                    }
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // remove_item_category(item_id, category) -> bool
    let cloned_db = db.clone();
    engine.register_fn("remove_item_category", move |item_id: String, category: String| -> bool {
        let cat_lower = category.to_lowercase();
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    let before_len = item.categories.len();
                    item.categories.retain(|c| c.to_lowercase() != cat_lower);
                    if item.categories.len() != before_len {
                        cloned_db.save_item_data(item).is_ok()
                    } else {
                        false
                    }
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_all_item_categories() -> Array<String> (sorted unique set of all categories across all items)
    let cloned_db = db.clone();
    engine.register_fn("get_all_item_categories", move || -> Vec<rhai::Dynamic> {
        let mut all_categories = std::collections::BTreeSet::new();
        if let Ok(items) = cloned_db.list_all_items() {
            for item in items {
                for cat in &item.categories {
                    all_categories.insert(cat.to_lowercase());
                }
            }
        }
        all_categories.into_iter().map(|c| rhai::Dynamic::from(c)).collect()
    });

    // get_item_teaches_recipe(item_id) -> String or ()
    let cloned_db = db.clone();
    engine.register_fn("get_item_teaches_recipe", move |item_id: String| -> rhai::Dynamic {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => {
                    match item.teaches_recipe {
                        Some(recipe_id) => rhai::Dynamic::from(recipe_id),
                        None => rhai::Dynamic::UNIT,
                    }
                }
                _ => rhai::Dynamic::UNIT,
            }
        } else {
            rhai::Dynamic::UNIT
        }
    });

    // set_item_teaches_recipe(item_id, recipe_id) -> bool
    let cloned_db = db.clone();
    engine.register_fn("set_item_teaches_recipe", move |item_id: String, recipe_id: String| -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(mut item)) => {
                    item.teaches_recipe = if recipe_id.is_empty() { None } else { Some(recipe_id) };
                    cloned_db.save_item_data(item).is_ok()
                }
                _ => false,
            }
        } else {
            false
        }
    });

    // get_item_quality(item_id) -> i64
    let cloned_db = db.clone();
    engine.register_fn("get_item_quality", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) => item.quality as i64,
                _ => 0,
            }
        } else {
            0
        }
    });

    // get_item_liquid_current(item_id) -> i64
    // Returns the current liquid amount in a container, or 0 if not a liquid container
    let cloned_db = db.clone();
    engine.register_fn("get_item_liquid_current", move |item_id: String| -> i64 {
        if let Ok(uuid) = uuid::Uuid::parse_str(&item_id) {
            match cloned_db.get_item_data(&uuid) {
                Ok(Some(item)) if item.item_type == ItemType::LiquidContainer => item.liquid_current as i64,
                _ => 0,
            }
        } else {
            0
        }
    });

    // calculate_craft_quality(skill_level, tool_quality, ingredient_quality, difficulty) -> i64
    // Returns quality tier: 0=Poor, 1=Normal, 2=Good, 3=Excellent
    engine.register_fn("calculate_craft_quality", |skill_level: i64, tool_quality: i64, ingredient_quality: i64, difficulty: i64| -> i64 {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Calculate quality score (0-100 scale)
        let mut quality_score: i64 = skill_level * 10;           // 0-100 from skill
        quality_score += tool_quality / 4;                        // 0-25 from tools
        quality_score += ingredient_quality / 4;                  // 0-25 from ingredients
        quality_score -= difficulty * 5;                          // penalty for difficulty
        quality_score += rng.gen_range(-15..=15);                 // random variance

        // Clamp and determine tier
        quality_score = quality_score.max(0).min(100);

        if quality_score >= 90 {
            3 // Excellent
        } else if quality_score >= 70 {
            2 // Good
        } else if quality_score >= 40 {
            1 // Normal
        } else {
            0 // Poor
        }
    });

    // get_quality_tier_name(tier) -> String
    engine.register_fn("get_quality_tier_name", |tier: i64| -> String {
        match tier {
            0 => "poor".to_string(),
            1 => "normal".to_string(),
            2 => "good".to_string(),
            _ => "excellent".to_string(),
        }
    });

    // ========== Recipe OLC Functions ==========

    // new_recipe(vnum) -> Recipe
    // Creates a new recipe with default values
    engine.register_fn("new_recipe", |vnum: String| -> Recipe {
        Recipe {
            id: vnum,
            name: "New Recipe".to_string(),
            skill: "crafting".to_string(),
            skill_required: 0,
            auto_learn: true,
            ingredients: Vec::new(),
            tools: Vec::new(),
            output_vnum: String::new(),
            output_quantity: 1,
            base_xp: 10,
            difficulty: 1,
        }
    });

    // save_recipe(recipe) -> bool
    // Saves a recipe to the database and in-memory cache
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("save_recipe", move |recipe: Recipe| -> bool {
        // Save to database
        if cloned_db.save_recipe(recipe.clone()).is_err() {
            return false;
        }
        // Update in-memory cache
        let mut state_guard = cloned_state.lock().unwrap();
        state_guard.recipes.insert(recipe.id.clone(), recipe);
        true
    });

    // delete_recipe(vnum) -> bool
    // Deletes a recipe from the database and in-memory cache
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("delete_recipe", move |vnum: String| -> bool {
        // Delete from database
        match cloned_db.delete_recipe(&vnum) {
            Ok(true) => {
                // Remove from in-memory cache
                let mut state_guard = cloned_state.lock().unwrap();
                state_guard.recipes.remove(&vnum);
                true
            }
            _ => false,
        }
    });

    // recipe_exists(vnum) -> bool
    let cloned_state = state.clone();
    engine.register_fn("recipe_exists", move |vnum: String| -> bool {
        let state_guard = cloned_state.lock().unwrap();
        state_guard.recipes.contains_key(&vnum.to_lowercase()) || state_guard.recipes.contains_key(&vnum)
    });

    // set_recipe_name(vnum, name) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_name", move |vnum: String, name: String| -> bool {
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.name = name;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_skill(vnum, skill) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_skill", move |vnum: String, skill: String| -> bool {
        let skill_lower = skill.to_lowercase();
        if skill_lower != "cooking" && skill_lower != "crafting" {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.skill = skill_lower;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_level(vnum, level) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_level", move |vnum: String, level: i64| -> bool {
        if level < 0 || level > 10 {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.skill_required = level as i32;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_autolearn(vnum, autolearn) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_autolearn", move |vnum: String, autolearn: bool| -> bool {
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.auto_learn = autolearn;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_difficulty(vnum, difficulty) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_difficulty", move |vnum: String, difficulty: i64| -> bool {
        if difficulty < 1 || difficulty > 10 {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.difficulty = difficulty as i32;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_xp(vnum, xp) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_xp", move |vnum: String, xp: i64| -> bool {
        if xp < 0 {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.base_xp = xp as i32;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // set_recipe_output(vnum, output_vnum, quantity) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("set_recipe_output", move |vnum: String, output_vnum: String, quantity: i64| -> bool {
        if quantity < 1 {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            recipe.output_vnum = output_vnum;
            recipe.output_quantity = quantity as i32;
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // add_recipe_ingredient(vnum, item_vnum_or_category, quantity) -> bool
    // Use @category for category match, or exact vnum for vnum match
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("add_recipe_ingredient", move |vnum: String, item_ref: String, quantity: i64| -> bool {
        if quantity < 1 {
            return false;
        }
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            let ingredient = if item_ref.starts_with("@liquid:") {
                // Liquid ingredient - preserve full @liquid:TYPE format
                RecipeIngredient {
                    vnum: None,
                    category: Some(item_ref.clone()),
                    quantity: quantity as i32,
                }
            } else if item_ref.starts_with('@') {
                // Category match - strip @ prefix
                RecipeIngredient {
                    vnum: None,
                    category: Some(item_ref[1..].to_string()),
                    quantity: quantity as i32,
                }
            } else {
                // Vnum match
                RecipeIngredient {
                    vnum: Some(item_ref),
                    category: None,
                    quantity: quantity as i32,
                }
            };
            recipe.ingredients.push(ingredient);
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // remove_recipe_ingredient(vnum, index) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("remove_recipe_ingredient", move |vnum: String, index: i64| -> bool {
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            let idx = index as usize;
            if idx < recipe.ingredients.len() {
                recipe.ingredients.remove(idx);
                cloned_db.save_recipe(recipe.clone()).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    });

    // add_recipe_tool(vnum, item_vnum_or_category, location) -> bool
    // Use @category for category match, or exact vnum for vnum match
    // location: "inventory", "room", or "either"
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("add_recipe_tool", move |vnum: String, item_ref: String, location: String| -> bool {
        let tool_location = match location.to_lowercase().as_str() {
            "inventory" | "inv" => ToolLocation::Inventory,
            "room" => ToolLocation::Room,
            "either" => ToolLocation::Either,
            _ => return false,
        };
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            let tool = if item_ref.starts_with('@') {
                // Category match
                RecipeTool {
                    vnum: None,
                    category: Some(item_ref[1..].to_string()),
                    location: tool_location,
                }
            } else {
                // Vnum match
                RecipeTool {
                    vnum: Some(item_ref),
                    category: None,
                    location: tool_location,
                }
            };
            recipe.tools.push(tool);
            cloned_db.save_recipe(recipe.clone()).is_ok()
        } else {
            false
        }
    });

    // remove_recipe_tool(vnum, index) -> bool
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("remove_recipe_tool", move |vnum: String, index: i64| -> bool {
        let mut state_guard = cloned_state.lock().unwrap();
        if let Some(recipe) = state_guard.recipes.get_mut(&vnum) {
            let idx = index as usize;
            if idx < recipe.tools.len() {
                recipe.tools.remove(idx);
                cloned_db.save_recipe(recipe.clone()).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    });

    // rename_recipe_vnum(old_vnum, new_vnum) -> bool
    // Renames a recipe by creating a new one with the new vnum and deleting the old
    let cloned_db = db.clone();
    let cloned_state = state.clone();
    engine.register_fn("rename_recipe_vnum", move |old_vnum: String, new_vnum: String| -> bool {
        if old_vnum == new_vnum {
            return true; // No change needed
        }
        let mut state_guard = cloned_state.lock().unwrap();
        // Check new vnum doesn't already exist
        if state_guard.recipes.contains_key(&new_vnum) {
            return false;
        }
        // Get the old recipe
        if let Some(mut recipe) = state_guard.recipes.remove(&old_vnum) {
            // Update the id
            recipe.id = new_vnum.clone();
            // Delete old from database
            let _ = cloned_db.delete_recipe(&old_vnum);
            // Save with new vnum
            if cloned_db.save_recipe(recipe.clone()).is_ok() {
                state_guard.recipes.insert(new_vnum, recipe);
                true
            } else {
                // Restore old recipe on failure
                recipe.id = old_vnum.clone();
                state_guard.recipes.insert(old_vnum, recipe);
                false
            }
        } else {
            false
        }
    });

    // search_recipes(keyword) -> Array<Recipe>
    let cloned_state = state.clone();
    engine.register_fn("search_recipes", move |keyword: String| -> Vec<rhai::Dynamic> {
        let keyword_lower = keyword.to_lowercase();
        let state_guard = cloned_state.lock().unwrap();
        state_guard.recipes.values()
            .filter(|r| {
                r.id.to_lowercase().contains(&keyword_lower) ||
                r.name.to_lowercase().contains(&keyword_lower) ||
                r.output_vnum.to_lowercase().contains(&keyword_lower)
            })
            .map(|r| rhai::Dynamic::from(r.clone()))
            .collect()
    });

}
