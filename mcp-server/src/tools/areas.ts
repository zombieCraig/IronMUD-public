export const areaToolDefinitions = [
  {
    name: "list_areas",
    description: "List all areas in the MUD",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "get_area",
    description: "Get an area by UUID or prefix",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Area UUID or prefix",
        },
      },
      required: ["identifier"],
    },
  },
  {
    name: "create_area",
    description: "Create a new area",
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Area name (e.g., 'Dark Forest')",
        },
        prefix: {
          type: "string",
          description: "Area prefix for vnums (e.g., 'forest')",
        },
        description: {
          type: "string",
          description: "Area description",
        },
        level_min: {
          type: "number",
          description: "Minimum recommended level",
        },
        level_max: {
          type: "number",
          description: "Maximum recommended level",
        },
        theme: {
          type: "string",
          description: "Area theme/category",
        },
      },
      required: ["name", "prefix"],
    },
  },
  {
    name: "update_area",
    description: "Update an existing area",
    inputSchema: {
      type: "object",
      properties: {
        id: {
          type: "string",
          description: "Area UUID",
        },
        name: { type: "string" },
        prefix: { type: "string" },
        description: { type: "string" },
        level_min: { type: "number" },
        level_max: { type: "number" },
        theme: { type: "string" },
        immigration_enabled: { type: "boolean", description: "Enable/disable migrant spawning for this area" },
        immigration_room_vnum: { type: "string", description: "Room vnum where migrants arrive" },
        immigration_name_pool: { type: "string", description: "Name pool file (e.g. 'generic', 'japan')" },
        immigration_visual_profile: { type: "string", description: "Visual profile file (e.g. 'human')" },
        migration_interval_days: { type: "number", description: "Game-days between migration checks (1-30)" },
        migration_max_per_check: { type: "number", description: "Max migrants spawned per check" },
        immigration_guard_chance: { type: "number", description: "Per-spawn chance (0.0-1.0) that an immigrant arrives as a town guard" },
        default_room_flags: {
          type: "object",
          description: "Template RoomFlags copied into every newly-created room in this area. Existing rooms are not retroactively updated. Absent keys preserve current state.",
          properties: {
            dark: { type: "boolean" },
            no_mob: { type: "boolean" },
            indoors: { type: "boolean" },
            underwater: { type: "boolean" },
            climate_controlled: { type: "boolean" },
            always_hot: { type: "boolean" },
            always_cold: { type: "boolean" },
            city: { type: "boolean" },
            no_windows: { type: "boolean" },
            difficult_terrain: { type: "boolean" },
            dirt_floor: { type: "boolean" },
            property_storage: { type: "boolean" },
            post_office: { type: "boolean" },
            bank: { type: "boolean" },
            garden: { type: "boolean" },
            spawn_point: { type: "boolean" },
            shallow_water: { type: "boolean" },
            deep_water: { type: "boolean" },
            liveable: { type: "boolean" },
          },
          additionalProperties: false,
        },
      },
      required: ["id"],
    },
  },
  {
    name: "delete_area",
    description: "Delete an area (rooms will be unassigned, not deleted)",
    inputSchema: {
      type: "object",
      properties: {
        id: {
          type: "string",
          description: "Area UUID",
        },
      },
      required: ["id"],
    },
  },
  {
    name: "reset_area",
    description: "Trigger respawn for all spawn points in an area",
    inputSchema: {
      type: "object",
      properties: {
        id: {
          type: "string",
          description: "Area UUID",
        },
      },
      required: ["id"],
    },
  },
  {
    name: "list_rooms_in_area",
    description: "List all rooms in an area",
    inputSchema: {
      type: "object",
      properties: {
        area_id: {
          type: "string",
          description: "Area UUID",
        },
      },
      required: ["area_id"],
    },
  },
  {
    name: "get_area_overview",
    description: "Get a compact overview of an entire area: rooms, items, mobiles, and spawn points. Use this for discovery instead of listing full entities. Use get_room/get_item/get_mobile by vnum for detail when editing.",
    inputSchema: {
      type: "object",
      properties: {
        identifier: {
          type: "string",
          description: "Area UUID or prefix",
        },
      },
      required: ["identifier"],
    },
  },
];
