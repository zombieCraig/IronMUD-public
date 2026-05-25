export declare const achievementToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key?: undefined;
            name?: undefined;
            category?: undefined;
            description?: undefined;
            criterion?: undefined;
            reward?: undefined;
            hidden?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key: {
                type: string;
                description: string;
            };
            name?: undefined;
            category?: undefined;
            description?: undefined;
            criterion?: undefined;
            reward?: undefined;
            hidden?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description: string;
            };
            category: {
                type: string;
                enum: string[];
            };
            description?: undefined;
            criterion?: undefined;
            reward?: undefined;
            hidden?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            key: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description?: undefined;
            };
            description: {
                type: string;
            };
            category: {
                type: string;
                enum: string[];
            };
            criterion: {
                oneOf: ({
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        counter: {
                            type: string;
                            description: string;
                        };
                        threshold: {
                            type: string;
                            description: string;
                        };
                        skill?: undefined;
                        level?: undefined;
                        recipe_key?: undefined;
                        area_vnum?: undefined;
                        amount?: undefined;
                    };
                    required: string[];
                } | {
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        skill: {
                            type: string;
                            description: string;
                        };
                        level: {
                            type: string;
                            description: string;
                        };
                        counter?: undefined;
                        threshold?: undefined;
                        recipe_key?: undefined;
                        area_vnum?: undefined;
                        amount?: undefined;
                    };
                    required: string[];
                } | {
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        recipe_key: {
                            type: string;
                            description: string;
                        };
                        counter?: undefined;
                        threshold?: undefined;
                        skill?: undefined;
                        level?: undefined;
                        area_vnum?: undefined;
                        amount?: undefined;
                    };
                    required: string[];
                } | {
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        area_vnum: {
                            type: string;
                            description: string;
                        };
                        counter?: undefined;
                        threshold?: undefined;
                        skill?: undefined;
                        level?: undefined;
                        recipe_key?: undefined;
                        amount?: undefined;
                    };
                    required: string[];
                } | {
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        amount: {
                            type: string;
                            description: string;
                        };
                        counter?: undefined;
                        threshold?: undefined;
                        skill?: undefined;
                        level?: undefined;
                        recipe_key?: undefined;
                        area_vnum?: undefined;
                    };
                    required: string[];
                } | {
                    type: string;
                    properties: {
                        kind: {
                            const: string;
                        };
                        counter?: undefined;
                        threshold?: undefined;
                        skill?: undefined;
                        level?: undefined;
                        recipe_key?: undefined;
                        area_vnum?: undefined;
                        amount?: undefined;
                    };
                    required: string[];
                })[];
            };
            reward: {
                type: string;
                properties: {
                    title: {
                        type: string;
                        description: string;
                    };
                    item_vnum: {
                        type: string;
                        description: string;
                    };
                    gold: {
                        type: string;
                        description: string;
                    };
                    morality_delta: {
                        type: string;
                        description: string;
                    };
                };
                required: string[];
            };
            hidden: {
                type: string;
            };
        };
        required: string[];
    };
})[];
//# sourceMappingURL=achievements.d.ts.map