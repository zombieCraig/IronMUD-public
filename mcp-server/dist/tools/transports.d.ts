export declare const transportToolDefinitions: ({
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            transport_id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
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
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            transport_id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
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
            transport_type: {
                type: string;
                enum: string[];
                description: string;
            };
            interior_room_id: {
                type: string;
                description: string;
            };
            travel_time_secs: {
                type: string;
                default: number;
                description: string;
            };
            schedule_type: {
                type: string;
                enum: string[];
                default: string;
                description: string;
            };
            frequency_hours: {
                type: string;
                description: string;
            };
            operating_start: {
                type: string;
                description: string;
            };
            operating_end: {
                type: string;
                description: string;
            };
            dwell_time_secs: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            id?: undefined;
            transport_id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
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
            transport_type: {
                type: string;
                enum: string[];
                description?: undefined;
            };
            travel_time_secs: {
                type: string;
                default?: undefined;
                description?: undefined;
            };
            schedule_type: {
                type: string;
                enum: string[];
                default?: undefined;
                description?: undefined;
            };
            frequency_hours: {
                type: string;
                description?: undefined;
            };
            operating_start: {
                type: string;
                description?: undefined;
            };
            operating_end: {
                type: string;
                description?: undefined;
            };
            dwell_time_secs: {
                type: string;
                description?: undefined;
            };
            identifier?: undefined;
            vnum?: undefined;
            interior_room_id?: undefined;
            transport_id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
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
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            transport_id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            transport_id: {
                type: string;
                description: string;
            };
            room_id: {
                type: string;
                description: string;
            };
            name: {
                type: string;
                description: string;
            };
            exit_direction: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            vnum?: undefined;
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            index?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            transport_id: {
                type: string;
                description: string;
            };
            index: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            stop_index?: undefined;
            destination_index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            transport_id: {
                type: string;
                description: string;
            };
            stop_index: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            destination_index?: undefined;
        };
        required: string[];
    };
} | {
    name: string;
    description: string;
    inputSchema: {
        type: string;
        properties: {
            transport_id: {
                type: string;
                description: string;
            };
            destination_index: {
                type: string;
                description: string;
            };
            identifier?: undefined;
            name?: undefined;
            vnum?: undefined;
            transport_type?: undefined;
            interior_room_id?: undefined;
            travel_time_secs?: undefined;
            schedule_type?: undefined;
            frequency_hours?: undefined;
            operating_start?: undefined;
            operating_end?: undefined;
            dwell_time_secs?: undefined;
            id?: undefined;
            room_id?: undefined;
            exit_direction?: undefined;
            index?: undefined;
            stop_index?: undefined;
        };
        required: string[];
    };
})[];
//# sourceMappingURL=transports.d.ts.map