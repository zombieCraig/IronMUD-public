export declare const spawnPointToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            area_id: {
                type: string;
                description: string;
            };
            id?: undefined;
            room_id?: undefined;
            entity_type?: undefined;
            vnum?: undefined;
            max_count?: undefined;
            respawn_interval_secs?: undefined;
            enabled?: undefined;
            bury_on_spawn?: undefined;
            spawn_point_id?: undefined;
            item_vnum?: undefined;
            destination?: undefined;
            wear_location?: undefined;
            count?: undefined;
            index?: undefined;
        };
        required?: undefined;
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
            area_id?: undefined;
            room_id?: undefined;
            entity_type?: undefined;
            vnum?: undefined;
            max_count?: undefined;
            respawn_interval_secs?: undefined;
            enabled?: undefined;
            bury_on_spawn?: undefined;
            spawn_point_id?: undefined;
            item_vnum?: undefined;
            destination?: undefined;
            wear_location?: undefined;
            count?: undefined;
            index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            area_id: {
                type: string;
                description: string;
            };
            room_id: {
                type: string;
                description: string;
            };
            entity_type: {
                type: string;
                enum: string[];
                description: string;
            };
            vnum: {
                type: string;
                description: string;
            };
            max_count: {
                type: string;
                default: number;
                description: string;
            };
            respawn_interval_secs: {
                type: string;
                default: number;
                description: string;
            };
            enabled: {
                type: string;
                default: boolean;
            };
            bury_on_spawn: {
                type: string;
                default: boolean;
                description: string;
            };
            id?: undefined;
            spawn_point_id?: undefined;
            item_vnum?: undefined;
            destination?: undefined;
            wear_location?: undefined;
            count?: undefined;
            index?: undefined;
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
            max_count: {
                type: string;
                default?: undefined;
                description?: undefined;
            };
            respawn_interval_secs: {
                type: string;
                default?: undefined;
                description?: undefined;
            };
            enabled: {
                type: string;
                default?: undefined;
            };
            bury_on_spawn: {
                type: string;
                description: string;
                default?: undefined;
            };
            area_id?: undefined;
            room_id?: undefined;
            entity_type?: undefined;
            vnum?: undefined;
            spawn_point_id?: undefined;
            item_vnum?: undefined;
            destination?: undefined;
            wear_location?: undefined;
            count?: undefined;
            index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            spawn_point_id: {
                type: string;
                description: string;
            };
            item_vnum: {
                type: string;
                description: string;
            };
            destination: {
                type: string;
                enum: string[];
                description: string;
            };
            wear_location: {
                type: string;
                description: string;
            };
            count: {
                type: string;
                default: number;
            };
            area_id?: undefined;
            id?: undefined;
            room_id?: undefined;
            entity_type?: undefined;
            vnum?: undefined;
            max_count?: undefined;
            respawn_interval_secs?: undefined;
            enabled?: undefined;
            bury_on_spawn?: undefined;
            index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            spawn_point_id: {
                type: string;
                description?: undefined;
            };
            index: {
                type: string;
                description: string;
            };
            area_id?: undefined;
            id?: undefined;
            room_id?: undefined;
            entity_type?: undefined;
            vnum?: undefined;
            max_count?: undefined;
            respawn_interval_secs?: undefined;
            enabled?: undefined;
            bury_on_spawn?: undefined;
            item_vnum?: undefined;
            destination?: undefined;
            wear_location?: undefined;
            count?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=spawn-points.d.ts.map