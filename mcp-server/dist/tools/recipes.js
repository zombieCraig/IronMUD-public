// Recipe tool definitions — crafting and cooking formulas.
//
// Recipes are keyed by vnum (e.g. "smith:iron_sword"). Ingredients and tools
// each take either an exact `vnum` or a `category` tag (matches any item
// prototype with that category). Liquids use the `category: "liquid:<type>"`
// convention (e.g. `liquid:water`).
const ingredientSchema = {
    type: "object",
    properties: {
        vnum: {
            type: "string",
            description: "Exact item vnum (mutually exclusive with category). Use when only a specific prototype counts.",
        },
        category: {
            type: "string",
            description: "Category tag match (e.g. 'flour', 'sticks', 'shells'). For liquids use 'liquid:<type>' (e.g. 'liquid:water').",
        },
        quantity: {
            type: "number",
            description: "How many units needed. Default 1. For liquids this is sips.",
        },
    },
    required: [],
};
const toolSchema = {
    type: "object",
    properties: {
        vnum: { type: "string", description: "Exact tool vnum" },
        category: { type: "string", description: "Tool category tag (e.g. 'forge', 'knife')" },
        location: {
            type: "string",
            enum: ["inv", "inventory", "room", "either"],
            description: "Where the tool must be. 'inventory' = carried, 'room' = present in the room (forge, oven), 'either' = both accepted. Default: inventory.",
        },
    },
    required: [],
};
export const recipeToolDefinitions = [
    {
        name: "list_recipes",
        description: "List all crafting and cooking recipes.",
        inputSchema: { type: "object", properties: {} },
    },
    {
        name: "get_recipe",
        description: "Get a recipe by vnum.",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Recipe vnum (e.g. 'smith:iron_sword')" },
            },
            required: ["vnum"],
        },
    },
    {
        name: "create_recipe",
        description: "Create a new crafting or cooking recipe. Ingredients and tools may each reference items by exact vnum or by category tag (matches any prototype with that category). For liquid ingredients use category 'liquid:<type>' (e.g. 'liquid:water').",
        inputSchema: {
            type: "object",
            properties: {
                vnum: {
                    type: "string",
                    description: "Unique recipe vnum, e.g. 'islands:shell_necklace'. Used as the canonical id.",
                },
                name: { type: "string", description: "Display name shown in recipe list (e.g. 'Shell Necklace')" },
                skill: {
                    type: "string",
                    enum: ["crafting", "cooking"],
                    description: "Which skill tree governs this recipe",
                },
                skill_required: {
                    type: "number",
                    description: "Minimum skill level to attempt (0-10). Default 0.",
                },
                auto_learn: {
                    type: "boolean",
                    description: "If true, players learn this automatically upon reaching skill_required. If false, they need a trainer or recipe book.",
                },
                ingredients: {
                    type: "array",
                    description: "Items consumed on craft.",
                    items: ingredientSchema,
                },
                tools: {
                    type: "array",
                    description: "Items required but not consumed (forge, knife, oven, etc.).",
                    items: toolSchema,
                },
                output_vnum: {
                    type: "string",
                    description: "Item prototype vnum produced on success (must exist).",
                },
                output_quantity: {
                    type: "number",
                    description: "How many of the output to spawn. Default 1.",
                },
                base_xp: {
                    type: "number",
                    description: "Base crafting XP awarded on success.",
                },
                difficulty: {
                    type: "number",
                    description: "Difficulty 1-10 — higher means more likely to fail at low skill. Default 1.",
                },
            },
            required: ["vnum", "name", "skill", "output_vnum"],
        },
    },
    {
        name: "update_recipe",
        description: "Update an existing recipe. Any omitted field is left unchanged. Passing a new `ingredients` or `tools` array REPLACES the list (additive updates are not supported).",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Recipe vnum to update" },
                name: { type: "string" },
                skill: { type: "string", enum: ["crafting", "cooking"] },
                skill_required: { type: "number" },
                auto_learn: { type: "boolean" },
                ingredients: { type: "array", items: ingredientSchema },
                tools: { type: "array", items: toolSchema },
                output_vnum: { type: "string" },
                output_quantity: { type: "number" },
                base_xp: { type: "number" },
                difficulty: { type: "number" },
            },
            required: ["vnum"],
        },
    },
    {
        name: "delete_recipe",
        description: "Delete a recipe by vnum.",
        inputSchema: {
            type: "object",
            properties: {
                vnum: { type: "string", description: "Recipe vnum to delete" },
            },
            required: ["vnum"],
        },
    },
];
//# sourceMappingURL=recipes.js.map