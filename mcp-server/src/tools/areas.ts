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
        climate: {
          type: "string",
          enum: ["temperate", "tropical", "arid", "tundra", "subarctic"],
          description: "Climate preset that filters globally-rolled weather into a locally-permitted condition (e.g. tropical converts snow to rain) and shifts effective temperature. Defaults to temperate.",
        },
        combat_zone: {
          type: "string",
          enum: ["pve", "safe", "pvp"],
          description: "Combat zone type. pve = players attack mobs only (default), safe = no combat, pvp = players can attack other players. Rooms inherit this unless overridden at room level.",
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
        climate: {
          type: "string",
          enum: ["temperate", "tropical", "arid", "tundra", "subarctic"],
          description: "Climate preset filtering area weather and temperature. Unknown values are ignored.",
        },
        combat_zone: {
          type: "string",
          enum: ["pve", "safe", "pvp"],
          description: "Combat zone type. pve = players attack mobs only (default), safe = no combat, pvp = players can attack other players. Rooms inherit this unless overridden at room level. Unknown values are ignored.",
        },
        immigration_enabled: { type: "boolean", description: "Enable/disable migrant spawning for this area" },
        immigration_room_vnum: { type: "string", description: "Room vnum where migrants arrive" },
        donation_room_vnum: { type: "string", description: "Room vnum that accepts player `donate <item>` (empty string disables)" },
        immigration_name_pool: { type: "string", description: "Name pool file (e.g. 'generic', 'japan')" },
        immigration_visual_profile: { type: "string", description: "Visual profile file (e.g. 'human')" },
        migration_interval_days: { type: "number", description: "Game-days between migration checks (1-30)" },
        migration_max_per_check: { type: "number", description: "Max migrants spawned per check" },
        immigration_guard_chance: { type: "number", description: "Per-spawn chance (0.0-1.0) that an immigrant arrives as a town guard" },
        immigration_healer_chance: { type: "number", description: "Per-spawn chance (0.0-1.0) that an immigrant arrives as a herbalist healer" },
        immigration_scavenger_chance: { type: "number", description: "Per-spawn chance (0.0-1.0) that an immigrant arrives as a scavenger" },
        immigration_vampire_chance: { type: "number", description: "Per-spawn chance (0.0-1.0) that an immigrant arrives as a freshly-embraced vampire (random clan + starter discipline). Auto-suppressed when the area's combat_zone is 'safe'." },
        migrant_starting_gold: {
          type: "object",
          description: "Inclusive [min, max] range for a new migrant's starting purse. {min:0,max:0} reverts to legacy 'broke at spawn' behavior.",
          properties: {
            min: { type: "number", description: "Minimum starting gold (>= 0)" },
            max: { type: "number", description: "Maximum starting gold (>= min)" },
          },
          additionalProperties: false,
        },
        guard_wage_per_hour: { type: "number", description: "Hourly area-treasury wage paid to migrant guards anywhere in this area. 0 disables." },
        healer_wage_per_hour: { type: "number", description: "Hourly 'patient visits' wage paid to migrant healers anywhere in this area. 0 disables." },
        scavenger_wage_per_hour: { type: "number", description: "Hourly scrounging wage paid to migrant scavengers while away from home. 0 disables." },
        max_rooms: { type: "number", description: "Soft cap on rooms attributed to this area. 0 / negative clears (unlimited). Enforced at create-time only." },
        max_items: { type: "number", description: "Soft cap on item prototypes attributed to this area. 0 / negative clears (unlimited)." },
        max_mobiles: { type: "number", description: "Soft cap on mobile prototypes attributed to this area. 0 / negative clears (unlimited)." },
        max_spawn_points: { type: "number", description: "Soft cap on spawn points in this area. 0 / negative clears (unlimited)." },
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
  {
    name: "list_forage_tables",
    description: "Show all five forage tables for an area (city, wilderness, shallow_water, deep_water, underwater). Each table is an ordered list of { vnum, min_skill, rarity } entries consulted when a player forages in a room whose flags match.",
    inputSchema: {
      type: "object",
      properties: {
        area_id: { type: "string", description: "Area UUID" },
      },
      required: ["area_id"],
    },
  },
  {
    name: "add_forage_entry",
    description: "Add (or upsert) an item entry to one of an area's forage tables. If the vnum is already in the chosen table, min_skill and rarity are overwritten. The forage_type must match the room-flag tier the player forages in (e.g. 'shallow_water' rooms roll against the shallow_water table).",
    inputSchema: {
      type: "object",
      properties: {
        area_id: { type: "string", description: "Area UUID" },
        forage_type: {
          type: "string",
          enum: ["city", "wilderness", "shallow_water", "deep_water", "underwater"],
          description: "Which forage table to update",
        },
        vnum: { type: "string", description: "Item prototype vnum to potentially spawn" },
        min_skill: {
          type: "number",
          description: "Minimum foraging skill (0-10) before this entry can roll. Default 0.",
        },
        rarity: {
          type: "string",
          enum: ["common", "uncommon", "rare", "legendary"],
          description: "Drop rarity — drives XP multiplier and pick weight",
        },
      },
      required: ["area_id", "forage_type", "vnum", "rarity"],
    },
  },
  {
    name: "remove_forage_entry",
    description: "Remove an item entry from one of an area's forage tables.",
    inputSchema: {
      type: "object",
      properties: {
        area_id: { type: "string", description: "Area UUID" },
        forage_type: {
          type: "string",
          enum: ["city", "wilderness", "shallow_water", "deep_water", "underwater"],
        },
        vnum: { type: "string", description: "Item prototype vnum to remove" },
      },
      required: ["area_id", "forage_type", "vnum"],
    },
  },
];
