use anyhow::Result;

use crate::db::Db;
use crate::types::*;

pub fn seed_recipes(db: &Db) -> Result<()> {
    let recipes = vec![
        // Cooking: Hearty Stew (meat + vegetable, skill 1)
        Recipe {
            id: "hearty_stew".to_string(),
            name: "Hearty Stew".to_string(),
            skill: "cooking".to_string(),
            skill_required: 1,
            auto_learn: true,
            ingredients: vec![
                RecipeIngredient {
                    vnum: None,
                    category: Some("meat".to_string()),
                    quantity: 1,
                },
                RecipeIngredient {
                    vnum: None,
                    category: Some("vegetable".to_string()),
                    quantity: 1,
                },
            ],
            tools: Vec::new(),
            output_vnum: "oakvale:stew".to_string(),
            output_quantity: 1,
            base_xp: 15,
            difficulty: 3,
        },
        // Cooking: Bread (flour, skill 0)
        Recipe {
            id: "bread".to_string(),
            name: "Bread".to_string(),
            skill: "cooking".to_string(),
            skill_required: 0,
            auto_learn: true,
            ingredients: vec![RecipeIngredient {
                vnum: Some("oakvale:flour".to_string()),
                category: None,
                quantity: 1,
            }],
            tools: Vec::new(),
            output_vnum: "oakvale:bread".to_string(),
            output_quantity: 2,
            base_xp: 5,
            difficulty: 1,
        },
        // Crafting: Bandage (leather, skill 0)
        Recipe {
            id: "bandage".to_string(),
            name: "Bandage".to_string(),
            skill: "crafting".to_string(),
            skill_required: 0,
            auto_learn: true,
            ingredients: vec![RecipeIngredient {
                vnum: Some("oakvale:leather".to_string()),
                category: None,
                quantity: 1,
            }],
            tools: Vec::new(),
            output_vnum: "oakvale:bandage".to_string(),
            output_quantity: 2,
            base_xp: 5,
            difficulty: 1,
        },
        // Crafting: Torch (wood, skill 0)
        Recipe {
            id: "torch".to_string(),
            name: "Torch".to_string(),
            skill: "crafting".to_string(),
            skill_required: 0,
            auto_learn: true,
            ingredients: vec![RecipeIngredient {
                vnum: Some("oakvale:wood".to_string()),
                category: None,
                quantity: 1,
            }],
            tools: Vec::new(),
            output_vnum: "oakvale:torch".to_string(),
            output_quantity: 1,
            base_xp: 5,
            difficulty: 1,
        },
    ];

    for recipe in recipes {
        db.save_recipe(recipe)?;
    }

    tracing::info!("Seeded 4 recipes");
    Ok(())
}
