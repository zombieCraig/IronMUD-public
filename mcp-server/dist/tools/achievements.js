// Achievement tool definitions — CRUD for builder-authored achievements.
const criterionSchema = {
    oneOf: [
        {
            type: "object",
            properties: {
                kind: { const: "counter" },
                counter: { type: "string", description: "Dotted counter key (e.g. 'kills.goblin')" },
                threshold: { type: "number", description: "Value to reach" },
            },
            required: ["kind", "counter", "threshold"],
        },
        {
            type: "object",
            properties: {
                kind: { const: "skill_reached" },
                skill: { type: "string", description: "Skill key" },
                level: { type: "number", description: "Level to reach" },
            },
            required: ["kind", "skill", "level"],
        },
        {
            type: "object",
            properties: {
                kind: { const: "recipe_learned" },
                recipe_key: { type: "string", description: "Recipe vnum" },
            },
            required: ["kind", "recipe_key"],
        },
        {
            type: "object",
            properties: {
                kind: { const: "owned_lease" },
                area_vnum: { type: "string", description: "Optional area vnum; omit for any lease" },
            },
            required: ["kind"],
        },
        {
            type: "object",
            properties: {
                kind: { const: "gold_held" },
                amount: { type: "number", description: "Gold amount to reach (high-water)" },
            },
            required: ["kind", "amount"],
        },
        {
            type: "object",
            properties: {
                kind: { const: "manual" },
            },
            required: ["kind"],
        },
    ],
};
const rewardSchema = {
    type: "object",
    properties: {
        title: { type: "string", description: "Granted title" },
        item_vnum: { type: "string", description: "Optional item reward" },
        gold: { type: "number", description: "Optional gold reward" },
        morality_delta: {
            type: "number",
            description: "Morality shift applied at unlock. Positive pushes toward Good, negative toward Evil. Clamped into [-200, 200]. Defaults to 0.",
        },
    },
    required: ["title"],
};
export const achievementToolDefinitions = [
    {
        name: "list_achievements",
        description: "List all builder-authored achievements.",
        inputSchema: { type: "object", properties: {} },
    },
    {
        name: "get_achievement",
        description: "Get an achievement by key.",
        inputSchema: {
            type: "object",
            properties: {
                key: { type: "string", description: "Achievement key (e.g. 'slayer_of_goblins')" },
            },
            required: ["key"],
        },
    },
    {
        name: "create_achievement",
        description: "Create a new builder-authored achievement definition.",
        inputSchema: {
            type: "object",
            properties: {
                key: { type: "string", description: "Unique snake_case key." },
                name: { type: "string", description: "Display name." },
                category: {
                    type: "string",
                    enum: ["skill", "combat", "crafting", "exploration", "social", "wealth", "builder"],
                },
            },
            required: ["key", "name"],
        },
    },
    {
        name: "update_achievement",
        description: "Update an existing achievement definition.",
        inputSchema: {
            type: "object",
            properties: {
                key: { type: "string", description: "Achievement key to update." },
                name: { type: "string" },
                description: { type: "string" },
                category: {
                    type: "string",
                    enum: ["skill", "combat", "crafting", "exploration", "social", "wealth", "builder"],
                },
                criterion: criterionSchema,
                reward: rewardSchema,
                hidden: { type: "boolean" },
            },
            required: ["key"],
        },
    },
    {
        name: "delete_achievement",
        description: "Delete an achievement definition.",
        inputSchema: {
            type: "object",
            properties: {
                key: { type: "string", description: "Achievement key to delete" },
            },
            required: ["key"],
        },
    },
];
//# sourceMappingURL=achievements.js.map