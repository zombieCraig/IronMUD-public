export const plantToolDefinitions = [
    {
        name: "list_plant_prototypes",
        description: "List all plant prototypes (gardening species templates)",
        inputSchema: {
            type: "object",
            properties: {},
        },
    },
    {
        name: "get_plant_prototype",
        description: "Get a plant prototype by UUID or vnum",
        inputSchema: {
            type: "object",
            properties: {
                identifier: {
                    type: "string",
                    description: "Plant prototype UUID or vnum (e.g., 'plants:tomato')",
                },
            },
            required: ["identifier"],
        },
    },
    {
        name: "create_plant_prototype",
        description: "Create a new plant prototype (gardening species template). Defines growth stages, seasons, water needs, harvest items, and skill requirements.",
        inputSchema: {
            type: "object",
            properties: {
                name: {
                    type: "string",
                    description: "Plant name (e.g., 'Tomato Plant')",
                },
                vnum: {
                    type: "string",
                    description: "Unique vnum (e.g., 'plants:tomato')",
                },
                keywords: {
                    items: { type: "string" },
                    type: "array",
                    description: "Keywords for targeting the plant",
                },
                seed_vnum: {
                    type: "string",
                    description: "Item vnum of the seed used to plant this (e.g., 'garden:tomato_seeds')",
                },
                harvest_vnum: {
                    type: "string",
                    description: "Item vnum of the produce when harvested (e.g., 'garden:tomato')",
                },
                harvest_min: {
                    type: "number",
                    description: "Minimum harvest yield (default: 1)",
                },
                harvest_max: {
                    type: "number",
                    description: "Maximum harvest yield (default: 3)",
                },
                category: {
                    type: "string",
                    enum: ["vegetable", "herb", "flower", "fruit", "grain"],
                    description: "Plant category",
                },
                stages: {
                    type: "array",
                    description: "Growth stage definitions with duration and descriptions",
                    items: {
                        type: "object",
                        properties: {
                            stage: {
                                type: "string",
                                enum: ["seed", "sprout", "seedling", "growing", "mature", "flowering", "wilting", "dead"],
                                description: "Growth stage name",
                            },
                            duration_game_hours: {
                                type: "number",
                                description: "How long this stage lasts in game hours (1 game hour = 2 real minutes)",
                            },
                            description: {
                                type: "string",
                                description: "Room display text when plant is at this stage",
                            },
                            examine_desc: {
                                type: "string",
                                description: "Detail text shown when examining the plant at this stage",
                            },
                        },
                        required: ["stage", "duration_game_hours"],
                    },
                },
                preferred_seasons: {
                    type: "array",
                    items: { type: "string", enum: ["spring", "summer", "autumn", "winter"] },
                    description: "Seasons where growth is boosted (x1.25 speed)",
                },
                forbidden_seasons: {
                    type: "array",
                    items: { type: "string", enum: ["spring", "summer", "autumn", "winter"] },
                    description: "Seasons where growth is blocked entirely",
                },
                water_consumption_per_hour: {
                    type: "number",
                    description: "Water consumed per game hour (default: 1.0)",
                },
                water_capacity: {
                    type: "number",
                    description: "Maximum water level (default: 100.0)",
                },
                indoor_only: {
                    type: "boolean",
                    description: "Whether plant can only grow indoors/in pots",
                },
                min_skill_to_plant: {
                    type: "number",
                    description: "Minimum gardening skill level to plant (0-10)",
                },
                base_xp: {
                    type: "number",
                    description: "Base XP awarded on harvest (default: 10)",
                },
                pest_resistance: {
                    type: "number",
                    description: "Resistance to pests, 0-100 (default: 30)",
                },
                multi_harvest: {
                    type: "boolean",
                    description: "Whether plant resets to Growing after harvest instead of dying",
                },
            },
            required: ["name", "vnum"],
        },
    },
    {
        name: "update_plant_prototype",
        description: "Update an existing plant prototype",
        inputSchema: {
            type: "object",
            properties: {
                id: {
                    type: "string",
                    description: "Plant prototype UUID",
                },
                name: { type: "string" },
                keywords: { items: { type: "string" }, type: "array" },
                seed_vnum: { type: "string" },
                harvest_vnum: { type: "string" },
                harvest_min: { type: "number" },
                harvest_max: { type: "number" },
                category: {
                    type: "string",
                    enum: ["vegetable", "herb", "flower", "fruit", "grain"],
                },
                stages: {
                    type: "array",
                    items: {
                        type: "object",
                        properties: {
                            stage: { type: "string" },
                            duration_game_hours: { type: "number" },
                            description: { type: "string" },
                            examine_desc: { type: "string" },
                        },
                        required: ["stage", "duration_game_hours"],
                    },
                },
                preferred_seasons: {
                    type: "array",
                    items: { type: "string" },
                },
                forbidden_seasons: {
                    type: "array",
                    items: { type: "string" },
                },
                water_consumption_per_hour: { type: "number" },
                water_capacity: { type: "number" },
                indoor_only: { type: "boolean" },
                min_skill_to_plant: { type: "number" },
                base_xp: { type: "number" },
                pest_resistance: { type: "number" },
                multi_harvest: { type: "boolean" },
            },
            required: ["id"],
        },
    },
    {
        name: "delete_plant_prototype",
        description: "Delete a plant prototype",
        inputSchema: {
            type: "object",
            properties: {
                id: {
                    type: "string",
                    description: "Plant prototype UUID",
                },
            },
            required: ["id"],
        },
    },
];
//# sourceMappingURL=plants.js.map