export declare const recipeToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum?: undefined;
            name?: undefined;
            skill?: undefined;
            skill_required?: undefined;
            auto_learn?: undefined;
            ingredients?: undefined;
            tools?: undefined;
            output_vnum?: undefined;
            output_quantity?: undefined;
            base_xp?: undefined;
            difficulty?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name?: undefined;
            skill?: undefined;
            skill_required?: undefined;
            auto_learn?: undefined;
            ingredients?: undefined;
            tools?: undefined;
            output_vnum?: undefined;
            output_quantity?: undefined;
            base_xp?: undefined;
            difficulty?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description: string;
            };
            skill: {
                type: string;
                enum: string[];
                description: string;
            };
            skill_required: {
                type: string;
                description: string;
            };
            auto_learn: {
                type: string;
                description: string;
            };
            ingredients: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        vnum: {
                            type: string;
                            description: string;
                        };
                        category: {
                            type: string;
                            description: string;
                        };
                        quantity: {
                            type: string;
                            description: string;
                        };
                    };
                    required: never[];
                };
            };
            tools: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        vnum: {
                            type: string;
                            description: string;
                        };
                        category: {
                            type: string;
                            description: string;
                        };
                        location: {
                            type: string;
                            enum: string[];
                            description: string;
                        };
                    };
                    required: never[];
                };
            };
            output_vnum: {
                type: string;
                description: string;
            };
            output_quantity: {
                type: string;
                description: string;
            };
            base_xp: {
                type: string;
                description: string;
            };
            difficulty: {
                type: string;
                description: string;
            };
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description?: undefined;
            };
            skill: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            skill_required: {
                type: string;
                description?: undefined;
            };
            auto_learn: {
                type: string;
                description?: undefined;
            };
            ingredients: {
                type: string;
                items: {
                    type: string;
                    properties: {
                        vnum: {
                            type: string;
                            description: string;
                        };
                        category: {
                            type: string;
                            description: string;
                        };
                        quantity: {
                            type: string;
                            description: string;
                        };
                    };
                    required: never[];
                };
                description?: undefined;
            };
            tools: {
                type: string;
                items: {
                    type: string;
                    properties: {
                        vnum: {
                            type: string;
                            description: string;
                        };
                        category: {
                            type: string;
                            description: string;
                        };
                        location: {
                            type: string;
                            enum: string[];
                            description: string;
                        };
                    };
                    required: never[];
                };
                description?: undefined;
            };
            output_vnum: {
                type: string;
                description?: undefined;
            };
            output_quantity: {
                type: string;
                description?: undefined;
            };
            base_xp: {
                type: string;
                description?: undefined;
            };
            difficulty: {
                type: string;
                description?: undefined;
            };
        };
        required: string[];
    };
})[];
//# sourceMappingURL=recipes.d.ts.map