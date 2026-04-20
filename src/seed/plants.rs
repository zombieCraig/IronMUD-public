use anyhow::Result;

use crate::db::Db;
use crate::types::*;

use super::seed_uuid;

pub fn seed_plants(db: &Db) -> Result<()> {
    let plants = vec![
        // Tomato — vegetable, spring/summer, 6 growth stages, harvest 1-3
        PlantPrototype {
            id: seed_uuid("plant:tomato"),
            vnum: Some("plants:tomato".to_string()),
            name: "Tomato Plant".to_string(),
            keywords: vec!["tomato".to_string(), "plant".to_string()],
            seed_vnum: "oakvale:tomato_seeds".to_string(),
            harvest_vnum: "oakvale:tomato".to_string(),
            harvest_min: 1,
            harvest_max: 3,
            category: PlantCategory::Vegetable,
            stages: vec![
                GrowthStageDef {
                    stage: GrowthStage::Seed,
                    duration_game_hours: 6,
                    description: "A small mound of freshly turned soil marks where tomato seeds have been planted."
                        .to_string(),
                    examine_desc:
                        "The soil is damp and carefully patted down. Tiny seeds lie just beneath the surface."
                            .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Sprout,
                    duration_game_hours: 12,
                    description: "A tiny green sprout has pushed through the soil.".to_string(),
                    examine_desc: "Two small seed leaves unfurl from a delicate stem, reaching toward the light."
                        .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Seedling,
                    duration_game_hours: 18,
                    description: "A small tomato seedling grows here, its leaves broadening.".to_string(),
                    examine_desc: "The seedling has developed its first true leaves, jagged and aromatic.".to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Growing,
                    duration_game_hours: 24,
                    description: "A tomato plant grows vigorously, small yellow flowers appearing on its stems."
                        .to_string(),
                    examine_desc: "The plant is bushy and healthy, with clusters of tiny yellow star-shaped flowers."
                        .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Mature,
                    duration_game_hours: 36,
                    description: "A tomato plant hangs heavy with ripe red fruit, ready for harvest.".to_string(),
                    examine_desc:
                        "Plump, sun-warmed tomatoes dangle from sturdy vines. They're perfectly ripe and ready to pick."
                            .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Wilting,
                    duration_game_hours: 48,
                    description: "A tomato plant droops sadly, its remaining fruit beginning to soften.".to_string(),
                    examine_desc:
                        "The leaves are yellowing and the fruit is overripe. It should have been harvested sooner."
                            .to_string(),
                },
            ],
            preferred_seasons: vec![Season::Spring, Season::Summer],
            forbidden_seasons: vec![Season::Winter],
            water_consumption_per_hour: 1.5,
            water_capacity: 100.0,
            indoor_only: false,
            min_skill_to_plant: 0,
            base_xp: 10,
            pest_resistance: 30,
            multi_harvest: false,
            is_prototype: true,
        },
        // Herb — herb, multi-harvest, min skill 1
        PlantPrototype {
            id: seed_uuid("plant:herb"),
            vnum: Some("plants:herb".to_string()),
            name: "Herb Plant".to_string(),
            keywords: vec!["herb".to_string(), "plant".to_string()],
            seed_vnum: "oakvale:herb_seeds".to_string(),
            harvest_vnum: "oakvale:herb".to_string(),
            harvest_min: 1,
            harvest_max: 2,
            category: PlantCategory::Herb,
            stages: vec![
                GrowthStageDef {
                    stage: GrowthStage::Seed,
                    duration_game_hours: 4,
                    description: "A patch of soil has been prepared and sown with tiny herb seeds.".to_string(),
                    examine_desc: "The soil smells faintly of loam. Herb seeds are scattered just below the surface."
                        .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Sprout,
                    duration_game_hours: 8,
                    description: "Delicate green sprouts emerge from the soil, fragrant even at this size.".to_string(),
                    examine_desc: "A cluster of tiny green shoots, already giving off a pleasant herbal aroma."
                        .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Seedling,
                    duration_game_hours: 12,
                    description: "A young herb plant grows here, its leaves bright and aromatic.".to_string(),
                    examine_desc: "The herb is developing nicely, with several pairs of fragrant leaves.".to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Growing,
                    duration_game_hours: 18,
                    description: "A lush herb plant fills the air with its rich, earthy fragrance.".to_string(),
                    examine_desc: "The plant is thick with aromatic leaves, perfect for cooking or medicine."
                        .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Mature,
                    duration_game_hours: 0, // Multi-harvest resets to Growing
                    description: "A mature herb plant offers a bounty of fragrant leaves, ready for picking."
                        .to_string(),
                    examine_desc:
                        "Dense clusters of perfectly formed leaves await harvest. The plant will regrow after picking."
                            .to_string(),
                },
                GrowthStageDef {
                    stage: GrowthStage::Wilting,
                    duration_game_hours: 72,
                    description: "A withering herb plant struggles on, its leaves turning brown at the edges."
                        .to_string(),
                    examine_desc:
                        "The plant has seen better days. Its leaves are drying out and losing their fragrance."
                            .to_string(),
                },
            ],
            preferred_seasons: vec![Season::Spring, Season::Summer, Season::Autumn],
            forbidden_seasons: Vec::new(),
            water_consumption_per_hour: 0.8,
            water_capacity: 80.0,
            indoor_only: false,
            min_skill_to_plant: 1,
            base_xp: 8,
            pest_resistance: 50,
            multi_harvest: true,
            is_prototype: true,
        },
    ];

    for plant in plants {
        db.save_plant_prototype(plant)?;
    }

    tracing::info!("Seeded 2 plant prototypes");
    Ok(())
}
