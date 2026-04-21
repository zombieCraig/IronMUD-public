export const transportToolDefinitions = [
    {
        name: "list_transports",
        description: "List all transports (elevators, buses, trains, etc.)",
        inputSchema: {
            type: "object",
            properties: {},
        },
    },
    {
        name: "get_transport",
        description: "Get a transport by UUID or vnum",
        inputSchema: {
            type: "object",
            properties: {
                identifier: {
                    type: "string",
                    description: "Transport UUID or vnum",
                },
            },
            required: ["identifier"],
        },
    },
    {
        name: "create_transport",
        description: "Create a new transport (elevator, bus, train, ferry, or airship). Requires an interior room that serves as the vehicle cabin.",
        inputSchema: {
            type: "object",
            properties: {
                name: { type: "string", description: "Display name (e.g., 'Main Elevator')" },
                vnum: { type: "string", description: "Optional vnum identifier" },
                transport_type: {
                    type: "string",
                    enum: ["elevator", "bus", "train", "ferry", "airship"],
                    description: "Type of transport",
                },
                interior_room_id: {
                    type: "string",
                    description: "UUID of the room that serves as the vehicle interior",
                },
                travel_time_secs: {
                    type: "number",
                    default: 30,
                    description: "Seconds to travel between stops",
                },
                schedule_type: {
                    type: "string",
                    enum: ["ondemand", "gametime"],
                    default: "ondemand",
                    description: "Schedule type: ondemand (button-press) or gametime (automatic schedule)",
                },
                frequency_hours: {
                    type: "number",
                    description: "For gametime: hours between departures",
                },
                operating_start: {
                    type: "number",
                    description: "For gametime: hour to start operating (0-23)",
                },
                operating_end: {
                    type: "number",
                    description: "For gametime: hour to stop operating (0-23)",
                },
                dwell_time_secs: {
                    type: "number",
                    description: "For gametime: seconds to wait at each stop",
                },
            },
            required: ["name", "transport_type", "interior_room_id"],
        },
    },
    {
        name: "update_transport",
        description: "Update transport settings (name, type, schedule, travel time)",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Transport UUID or vnum" },
                name: { type: "string" },
                transport_type: {
                    type: "string",
                    enum: ["elevator", "bus", "train", "ferry", "airship"],
                },
                travel_time_secs: { type: "number" },
                schedule_type: {
                    type: "string",
                    enum: ["ondemand", "gametime"],
                },
                frequency_hours: { type: "number" },
                operating_start: { type: "number" },
                operating_end: { type: "number" },
                dwell_time_secs: { type: "number" },
            },
            required: ["id"],
        },
    },
    {
        name: "delete_transport",
        description: "Delete a transport. Automatically cleans up any exits if transport is currently connected to a stop.",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Transport UUID or vnum" },
            },
            required: ["id"],
        },
    },
    {
        name: "add_transport_stop",
        description: "Add a stop to a transport. Each stop defines a room and the exit direction used to enter the transport from that room.",
        inputSchema: {
            type: "object",
            properties: {
                transport_id: { type: "string", description: "Transport UUID or vnum" },
                room_id: { type: "string", description: "Room UUID for this stop" },
                name: {
                    type: "string",
                    description: "Stop name (e.g., 'Street Level', 'Floor 7')",
                },
                exit_direction: {
                    type: "string",
                    description: "Direction from the stop room to enter the transport (e.g., 'up', 'east', 'elevator')",
                },
            },
            required: ["transport_id", "room_id", "name", "exit_direction"],
        },
    },
    {
        name: "remove_transport_stop",
        description: "Remove a stop from a transport by index",
        inputSchema: {
            type: "object",
            properties: {
                transport_id: { type: "string", description: "Transport UUID or vnum" },
                index: { type: "number", description: "Stop index to remove (0-based)" },
            },
            required: ["transport_id", "index"],
        },
    },
    {
        name: "connect_transport",
        description: "Connect a transport to a specific stop (creates bidirectional exits between the stop room and interior room). Sets transport state to Stopped.",
        inputSchema: {
            type: "object",
            properties: {
                transport_id: { type: "string", description: "Transport UUID or vnum" },
                stop_index: {
                    type: "number",
                    description: "Index of the stop to connect to (0-based)",
                },
            },
            required: ["transport_id", "stop_index"],
        },
    },
    {
        name: "start_transport_travel",
        description: "Start on-demand travel to a destination stop. Disconnects from current stop (removes exits), sets state to Moving. The transport tick system handles arrival.",
        inputSchema: {
            type: "object",
            properties: {
                transport_id: { type: "string", description: "Transport UUID or vnum" },
                destination_index: {
                    type: "number",
                    description: "Index of the destination stop (0-based)",
                },
            },
            required: ["transport_id", "destination_index"],
        },
    },
    {
        name: "list_transport_types",
        description: "List all valid transport types",
        inputSchema: {
            type: "object",
            properties: {},
        },
    },
];
//# sourceMappingURL=transports.js.map