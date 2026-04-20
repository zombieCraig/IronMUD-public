export const spawnPointToolDefinitions = [
  {
    name: "list_spawn_points",
    description: "List spawn points, optionally filtered by area",
    inputSchema: {
      type: "object",
      properties: {
        area_id: { type: "string", description: "Filter by area UUID" },
      },
    },
  },
  {
    name: "get_spawn_point",
    description: "Get a spawn point by UUID",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Spawn point UUID" },
      },
      required: ["id"],
    },
  },
  {
    name: "create_spawn_point",
    description:
      "Create a spawn point to make mobiles/items respawn. IMPORTANT: Without spawn points, entities will NOT respawn after death/pickup!",
    inputSchema: {
      type: "object",
      properties: {
        area_id: { type: "string", description: "Area UUID" },
        room_id: { type: "string", description: "Room UUID where entity spawns" },
        entity_type: {
          type: "string",
          enum: ["mobile", "item"],
          description: "Type of entity to spawn",
        },
        vnum: { type: "string", description: "Vnum of the prototype to spawn" },
        max_count: {
          type: "number",
          default: 1,
          description: "Maximum simultaneous instances",
        },
        respawn_interval_secs: {
          type: "number",
          default: 300,
          description: "Seconds between respawns",
        },
        enabled: { type: "boolean", default: true },
      },
      required: ["area_id", "room_id", "entity_type", "vnum"],
    },
  },
  {
    name: "update_spawn_point",
    description: "Update spawn point settings",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Spawn point UUID" },
        max_count: { type: "number" },
        respawn_interval_secs: { type: "number" },
        enabled: { type: "boolean" },
      },
      required: ["id"],
    },
  },
  {
    name: "delete_spawn_point",
    description: "Delete a spawn point",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Spawn point UUID" },
      },
      required: ["id"],
    },
  },
  {
    name: "add_spawn_dependency",
    description:
      "Add an item that spawns WITH a mobile (e.g., equipment the mobile carries)",
    inputSchema: {
      type: "object",
      properties: {
        spawn_point_id: { type: "string", description: "Spawn point UUID" },
        item_vnum: { type: "string", description: "Item vnum to spawn" },
        destination: {
          type: "string",
          enum: ["inventory", "equipped", "container"],
          description: "Where the item appears on the mobile",
        },
        wear_location: {
          type: "string",
          description: "For 'equipped': where to wear the item",
        },
        count: { type: "number", default: 1 },
      },
      required: ["spawn_point_id", "item_vnum", "destination"],
    },
  },
  {
    name: "remove_spawn_dependency",
    description: "Remove a spawn dependency",
    inputSchema: {
      type: "object",
      properties: {
        spawn_point_id: { type: "string" },
        index: { type: "number", description: "Dependency index to remove" },
      },
      required: ["spawn_point_id", "index"],
    },
  },
];
