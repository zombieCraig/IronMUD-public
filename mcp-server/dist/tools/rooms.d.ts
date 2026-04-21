export declare const roomToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            area_id: {
                type: string;
                description: string;
            };
            vnum_prefix: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            room_id?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
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
            area_id?: undefined;
            vnum_prefix?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            room_id?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            title: {
                type: string;
                description: string;
            };
            description: {
                type: string;
                description: string;
            };
            area_id: {
                type: string;
                description: string;
            };
            vnum: {
                type: string;
                description: string;
            };
            flags: {
                type: string;
                properties: {
                    dark: {
                        type: string;
                    };
                    no_mob: {
                        type: string;
                    };
                    indoors: {
                        type: string;
                    };
                    safe: {
                        type: string;
                    };
                    shallow_water: {
                        type: string;
                    };
                    deep_water: {
                        type: string;
                    };
                    liveable: {
                        type: string;
                        description: string;
                    };
                };
            };
            vnum_prefix?: undefined;
            identifier?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            room_id?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
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
            title: {
                type: string;
                description?: undefined;
            };
            description: {
                type: string;
                description?: undefined;
            };
            flags: {
                type: string;
                properties?: undefined;
            };
            living_capacity: {
                type: string;
                description: string;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            vnum?: undefined;
            room_id?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
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
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            living_capacity?: undefined;
            room_id?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            direction: {
                type: string;
                enum: string[];
                description: string;
            };
            target_room_id: {
                type: string;
                description: string;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            direction: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            direction: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            name: {
                type: string;
                description: string;
            };
            is_closed: {
                type: string;
                default: boolean;
            };
            is_locked: {
                type: string;
                default: boolean;
            };
            key_id: {
                type: string;
                description: string;
            };
            keywords: {
                type: string;
                items: {
                    type: string;
                };
                description?: undefined;
            };
            description: {
                type: string;
                description?: undefined;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            target_room_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            direction: {
                type: string;
                enum?: undefined;
                description?: undefined;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            trigger_type: {
                type: string;
                enum: string[];
            };
            script_name: {
                type: string;
                description: string;
            };
            enabled: {
                type: string;
                default: boolean;
            };
            interval_secs: {
                type: string;
                description: string;
            };
            chance: {
                type: string;
                description: string;
            };
            args: {
                type: string;
                items: {
                    type: string;
                };
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            index: {
                type: string;
                description: string;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
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
            description: {
                type: string;
                description?: undefined;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
            keyword?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            keyword: {
                type: string;
                description: string;
            };
            area_id?: undefined;
            vnum_prefix?: undefined;
            identifier?: undefined;
            title?: undefined;
            description?: undefined;
            vnum?: undefined;
            flags?: undefined;
            id?: undefined;
            living_capacity?: undefined;
            direction?: undefined;
            target_room_id?: undefined;
            name?: undefined;
            is_closed?: undefined;
            is_locked?: undefined;
            key_id?: undefined;
            keywords?: undefined;
            trigger_type?: undefined;
            script_name?: undefined;
            enabled?: undefined;
            interval_secs?: undefined;
            chance?: undefined;
            args?: undefined;
            index?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=rooms.d.ts.map