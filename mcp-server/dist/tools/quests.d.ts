export declare const questToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            vnum?: undefined;
            name?: undefined;
            keywords?: undefined;
            summary?: undefined;
            description?: undefined;
            completion_text?: undefined;
            objectives?: undefined;
            rewards?: undefined;
            repeatable?: undefined;
            giver_mob_vnum?: undefined;
            prereq_quest_vnum?: undefined;
            min_player_skill_total?: undefined;
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
            keywords?: undefined;
            summary?: undefined;
            description?: undefined;
            completion_text?: undefined;
            objectives?: undefined;
            rewards?: undefined;
            repeatable?: undefined;
            giver_mob_vnum?: undefined;
            prereq_quest_vnum?: undefined;
            min_player_skill_total?: undefined;
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
            keywords: {
                type: string;
                items: {
                    type: string;
                };
                description: string;
            };
            summary: {
                type: string;
                description: string;
            };
            description: {
                type: string;
                description: string;
            };
            completion_text: {
                type: string;
                description: string;
            };
            objectives: {
                type: string;
                items: {
                    oneOf: ({
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            count: {
                                type: string;
                                description: string;
                            };
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            qty: {
                                type: string;
                                description: string;
                            };
                            return_to_mob_vnum: {
                                type: string;
                                description: string;
                            };
                            count?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            count?: undefined;
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            var: {
                                type: string;
                                description: string;
                            };
                            value: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            count?: undefined;
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                        };
                        required: string[];
                    })[];
                };
            };
            rewards: {
                type: string;
                items: {
                    oneOf: ({
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            amount: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            qty: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            skill?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
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
                            amount: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            qty?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            key: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            recipe_id: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            key?: undefined;
                        };
                        required: string[];
                    })[];
                };
            };
            repeatable: {
                type: string;
                description: string;
            };
            giver_mob_vnum: {
                type: string;
                description: string;
            };
            prereq_quest_vnum: {
                type: string;
                description: string;
            };
            min_player_skill_total: {
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
            keywords: {
                type: string;
                items: {
                    type: string;
                };
                description?: undefined;
            };
            summary: {
                type: string;
                description?: undefined;
            };
            description: {
                type: string;
                description?: undefined;
            };
            completion_text: {
                type: string;
                description?: undefined;
            };
            objectives: {
                type: string;
                items: {
                    oneOf: ({
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            count: {
                                type: string;
                                description: string;
                            };
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            qty: {
                                type: string;
                                description: string;
                            };
                            return_to_mob_vnum: {
                                type: string;
                                description: string;
                            };
                            count?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            count?: undefined;
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                            var?: undefined;
                            value?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            var: {
                                type: string;
                                description: string;
                            };
                            value: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            count?: undefined;
                            qty?: undefined;
                            return_to_mob_vnum?: undefined;
                        };
                        required: string[];
                    })[];
                };
            };
            rewards: {
                type: string;
                items: {
                    oneOf: ({
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            amount: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            vnum: {
                                type: string;
                                description: string;
                            };
                            qty: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            skill?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
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
                            amount: {
                                type: string;
                                description: string;
                            };
                            vnum?: undefined;
                            qty?: undefined;
                            key?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            key: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            recipe_id?: undefined;
                        };
                        required: string[];
                    } | {
                        type: string;
                        properties: {
                            kind: {
                                const: string;
                            };
                            recipe_id: {
                                type: string;
                                description: string;
                            };
                            amount?: undefined;
                            vnum?: undefined;
                            qty?: undefined;
                            skill?: undefined;
                            key?: undefined;
                        };
                        required: string[];
                    })[];
                };
            };
            repeatable: {
                type: string;
                description?: undefined;
            };
            giver_mob_vnum: {
                type: string;
                description?: undefined;
            };
            prereq_quest_vnum: {
                type: string;
                description?: undefined;
            };
            min_player_skill_total: {
                type: string;
                description?: undefined;
            };
        };
        required: string[];
    };
})[];
//# sourceMappingURL=quests.d.ts.map