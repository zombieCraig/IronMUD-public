export declare const descriptionToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            room_id: {
                type: string;
                description: string;
            };
            style_hints: {
                type: string;
                enum: string[];
                description: string;
            };
            item_id?: undefined;
            description_type?: undefined;
            mobile_id?: undefined;
            entity_type?: undefined;
            filter?: undefined;
            limit?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            item_id: {
                type: string;
                description: string;
            };
            description_type: {
                type: string;
                enum: string[];
                description: string;
            };
            room_id?: undefined;
            style_hints?: undefined;
            mobile_id?: undefined;
            entity_type?: undefined;
            filter?: undefined;
            limit?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            mobile_id: {
                type: string;
                description: string;
            };
            description_type: {
                type: string;
                enum: string[];
                description: string;
            };
            room_id?: undefined;
            style_hints?: undefined;
            item_id?: undefined;
            entity_type?: undefined;
            filter?: undefined;
            limit?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            entity_type: {
                type: string;
                enum: string[];
                description: string;
            };
            filter: {
                type: string;
                properties: {
                    area_prefix: {
                        type: string;
                        description: string;
                    };
                    item_type: {
                        type: string;
                        enum: string[];
                        description: string;
                    };
                    has_flag: {
                        type: string;
                        description: string;
                    };
                    min_length: {
                        type: string;
                        description: string;
                    };
                    max_length: {
                        type: string;
                        description: string;
                    };
                };
                description: string;
            };
            limit: {
                type: string;
                description: string;
            };
            room_id?: undefined;
            style_hints?: undefined;
            item_id?: undefined;
            description_type?: undefined;
            mobile_id?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=descriptions.d.ts.map