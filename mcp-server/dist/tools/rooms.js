export const roomToolDefinitions = [
    {
        name: "list_rooms_summary",
        description: "List room summaries (compact: vnum, title, exits, flags). Use for discovery, then get_room by vnum for detail.",
        inputSchema: {
            type: "object",
            properties: {
                area_id: {
                    type: "string",
                    description: "Filter by area UUID",
                },
                vnum_prefix: {
                    type: "string",
                    description: "Filter by vnum prefix (e.g., 'training' to match 'training:*')",
                },
            },
        },
    },
    {
        name: "get_room",
        description: "Get a room by UUID or vnum",
        inputSchema: {
            type: "object",
            properties: {
                identifier: {
                    type: "string",
                    description: "Room UUID or vnum",
                },
            },
            required: ["identifier"],
        },
    },
    {
        name: "create_room",
        description: "Create a new room",
        inputSchema: {
            type: "object",
            properties: {
                title: {
                    type: "string",
                    description: "Room title (shown when entering)",
                },
                description: {
                    type: "string",
                    description: "Full room description",
                },
                area_id: {
                    type: "string",
                    description: "Area UUID to assign room to",
                },
                vnum: {
                    type: "string",
                    description: "Unique vnum (e.g., 'forest:entrance')",
                },
                flags: {
                    type: "object",
                    properties: {
                        dark: { type: "boolean" },
                        no_mob: { type: "boolean" },
                        indoors: { type: "boolean" },
                        safe: { type: "boolean" },
                        shallow_water: { type: "boolean" },
                        deep_water: { type: "boolean" },
                        liveable: { type: "boolean", description: "Migrant NPCs can claim this room as a home" },
                    },
                },
            },
            required: ["title", "description"],
        },
    },
    {
        name: "update_room",
        description: "Update an existing room",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Room UUID or vnum" },
                title: { type: "string" },
                description: { type: "string" },
                flags: { type: "object" },
                living_capacity: {
                    type: "number",
                    description: "Maximum migrants that can reside in this room (requires liveable flag)",
                },
            },
            required: ["id"],
        },
    },
    {
        name: "delete_room",
        description: "Delete a room",
        inputSchema: {
            type: "object",
            properties: {
                id: { type: "string", description: "Room UUID or vnum" },
            },
            required: ["id"],
        },
    },
    {
        name: "set_room_exit",
        description: "Connect two rooms with an exit",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Source room UUID or vnum" },
                direction: {
                    type: "string",
                    enum: ["north", "south", "east", "west", "up", "down"],
                    description: "Exit direction",
                },
                target_room_id: { type: "string", description: "Target room UUID or vnum" },
            },
            required: ["room_id", "direction", "target_room_id"],
        },
    },
    {
        name: "remove_room_exit",
        description: "Remove an exit from a room",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                direction: {
                    type: "string",
                    enum: ["north", "south", "east", "west", "up", "down"],
                },
            },
            required: ["room_id", "direction"],
        },
    },
    {
        name: "add_room_door",
        description: "Add a door to a room exit",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                direction: {
                    type: "string",
                    enum: ["north", "south", "east", "west", "up", "down"],
                },
                name: { type: "string", description: "Door name (e.g., 'wooden door')" },
                is_closed: { type: "boolean", default: true },
                is_locked: { type: "boolean", default: false },
                key_id: { type: "string", description: "UUID of key that unlocks door" },
                keywords: { type: "array", items: { type: "string" } },
                description: { type: "string" },
            },
            required: ["room_id", "direction", "name"],
        },
    },
    {
        name: "remove_room_door",
        description: "Remove a door from a room exit",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                direction: { type: "string" },
            },
            required: ["room_id", "direction"],
        },
    },
    {
        name: "add_room_trigger",
        description: "Add a trigger script to a room",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                trigger_type: {
                    type: "string",
                    enum: ["enter", "exit", "look", "periodic", "time", "weather", "season", "month"],
                },
                script_name: { type: "string", description: "Script filename without extension" },
                enabled: { type: "boolean", default: true },
                interval_secs: { type: "number", description: "For timer triggers" },
                chance: { type: "number", description: "Trigger probability (0-100)" },
                args: { type: "array", items: { type: "string" } },
            },
            required: ["room_id", "trigger_type", "script_name"],
        },
    },
    {
        name: "remove_room_trigger",
        description: "Remove a trigger from a room",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                index: { type: "number", description: "Trigger index to remove" },
            },
            required: ["room_id", "index"],
        },
    },
    {
        name: "add_room_extra_desc",
        description: "Add an extra description to a room (examine keyword)",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                keywords: {
                    type: "array",
                    items: { type: "string" },
                    description: "Keywords that trigger description",
                },
                description: { type: "string" },
            },
            required: ["room_id", "keywords", "description"],
        },
    },
    {
        name: "remove_room_extra_desc",
        description: "Remove an extra description from a room",
        inputSchema: {
            type: "object",
            properties: {
                room_id: { type: "string", description: "Room UUID or vnum" },
                keyword: { type: "string", description: "Keyword to remove" },
            },
            required: ["room_id", "keyword"],
        },
    },
];
//# sourceMappingURL=rooms.js.map