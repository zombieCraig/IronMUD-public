export declare const plantToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            seed_vnum?: undefined;
            harvest_vnum?: undefined;
            harvest_min?: undefined;
            harvest_max?: undefined;
            category?: undefined;
            stages?: undefined;
            preferred_seasons?: undefined;
            forbidden_seasons?: undefined;
            water_consumption_per_hour?: undefined;
            water_capacity?: undefined;
            indoor_only?: undefined;
            min_skill_to_plant?: undefined;
            base_xp?: undefined;
            pest_resistance?: undefined;
            multi_harvest?: undefined;
            id?: undefined;
        };
        required?: undefined;
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier: {
                type: string;
                description: string;
            };
            name?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            seed_vnum?: undefined;
            harvest_vnum?: undefined;
            harvest_min?: undefined;
            harvest_max?: undefined;
            category?: undefined;
            stages?: undefined;
            preferred_seasons?: undefined;
            forbidden_seasons?: undefined;
            water_consumption_per_hour?: undefined;
            water_capacity?: undefined;
            indoor_only?: undefined;
            min_skill_to_plant?: undefined;
            base_xp?: undefined;
            pest_resistance?: undefined;
            multi_harvest?: undefined;
            id?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            name: {
                type: string;
                description: string;
            };
            vnum: {
                type: string;
                description: string;
            };
            keywords: {
                items: {
                    type: string;
                };
                type: string;
                description: string;
            };
            seed_vnum: {
                type: string;
                description: string;
            };
            harvest_vnum: {
                type: string;
                description: string;
            };
            harvest_min: {
                type: string;
                description: string;
            };
            harvest_max: {
                type: string;
                description: string;
            };
            category: {
                type: string;
                enum: string[];
                description: string;
            };
            stages: {
                type: string;
                description: string;
                items: {
                    type: string;
                    properties: {
                        stage: {
                            type: string;
                            enum: string[];
                            description: string;
                        };
                        duration_game_hours: {
                            type: string;
                            description: string;
                        };
                        description: {
                            type: string;
                            description: string;
                        };
                        examine_desc: {
                            type: string;
                            description: string;
                        };
                    };
                    required: string[];
                };
            };
            preferred_seasons: {
                type: string;
                items: {
                    type: string;
                    enum: string[];
                };
                description: string;
            };
            forbidden_seasons: {
                type: string;
                items: {
                    type: string;
                    enum: string[];
                };
                description: string;
            };
            water_consumption_per_hour: {
                type: string;
                description: string;
            };
            water_capacity: {
                type: string;
                description: string;
            };
            indoor_only: {
                type: string;
                description: string;
            };
            min_skill_to_plant: {
                type: string;
                description: string;
            };
            base_xp: {
                type: string;
                description: string;
            };
            pest_resistance: {
                type: string;
                description: string;
            };
            multi_harvest: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            id?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            id: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description?: undefined;
            };
            keywords: {
                items: {
                    type: string;
                };
                type: string;
                description?: undefined;
            };
            seed_vnum: {
                type: string;
                description?: undefined;
            };
            harvest_vnum: {
                type: string;
                description?: undefined;
            };
            harvest_min: {
                type: string;
                description?: undefined;
            };
            harvest_max: {
                type: string;
                description?: undefined;
            };
            category: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            stages: {
                type: string;
                items: {
                    type: string;
                    properties: {
                        stage: {
                            type: string;
                            enum?: undefined;
                            description?: undefined;
                        };
                        duration_game_hours: {
                            type: string;
                            description?: undefined;
                        };
                        description: {
                            type: string;
                            description?: undefined;
                        };
                        examine_desc: {
                            type: string;
                            description?: undefined;
                        };
                    };
                    required: string[];
                };
                description?: undefined;
            };
            preferred_seasons: {
                type: string;
                items: {
                    type: string;
                    enum?: undefined;
                };
                description?: undefined;
            };
            forbidden_seasons: {
                type: string;
                items: {
                    type: string;
                    enum?: undefined;
                };
                description?: undefined;
            };
            water_consumption_per_hour: {
                type: string;
                description?: undefined;
            };
            water_capacity: {
                type: string;
                description?: undefined;
            };
            indoor_only: {
                type: string;
                description?: undefined;
            };
            min_skill_to_plant: {
                type: string;
                description?: undefined;
            };
            base_xp: {
                type: string;
                description?: undefined;
            };
            pest_resistance: {
                type: string;
                description?: undefined;
            };
            multi_harvest: {
                type: string;
                description?: undefined;
            };
            identifier?: undefined;
            vnum?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            id: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            keywords?: undefined;
            seed_vnum?: undefined;
            harvest_vnum?: undefined;
            harvest_min?: undefined;
            harvest_max?: undefined;
            category?: undefined;
            stages?: undefined;
            preferred_seasons?: undefined;
            forbidden_seasons?: undefined;
            water_consumption_per_hour?: undefined;
            water_capacity?: undefined;
            indoor_only?: undefined;
            min_skill_to_plant?: undefined;
            base_xp?: undefined;
            pest_resistance?: undefined;
            multi_harvest?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=plants.d.ts.map