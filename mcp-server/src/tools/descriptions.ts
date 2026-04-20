// Tool definitions for AI-assisted description generation

export const descriptionToolDefinitions = [
  {
    name: "get_room_context",
    description:
      "Gather contextual information for generating a room description. Returns the room's current state, area theme, connected rooms, and suggested atmospheric elements based on flags. Use this before writing or updating a room description.",
    inputSchema: {
      type: "object",
      properties: {
        room_id: {
          type: "string",
          description: "Room UUID or vnum to get context for",
        },
        style_hints: {
          type: "string",
          enum: ["atmospheric", "brief", "detailed"],
          description:
            "Optional style preference: 'atmospheric' for mood focus, 'brief' for minimal, 'detailed' for comprehensive",
        },
      },
      required: ["room_id"],
    },
  },
  {
    name: "get_item_context",
    description:
      "Gather contextual information for generating an item description. Returns the item's properties, type-specific guidance, and flag-based description elements. Use this before writing item short_desc or long_desc.",
    inputSchema: {
      type: "object",
      properties: {
        item_id: {
          type: "string",
          description: "Item UUID or vnum to get context for",
        },
        description_type: {
          type: "string",
          enum: ["short_desc", "long_desc", "both"],
          description:
            "Which description to generate: 'short_desc' for inventory/examine, 'long_desc' for ground appearance, 'both' for both",
        },
      },
      required: ["item_id"],
    },
  },
  {
    name: "get_mobile_context",
    description:
      "Gather contextual information for generating a mobile (NPC/monster) description. Returns the mobile's properties, detected role, and behavior-based description hints. Use this before writing mobile short_desc or long_desc.",
    inputSchema: {
      type: "object",
      properties: {
        mobile_id: {
          type: "string",
          description: "Mobile UUID or vnum to get context for",
        },
        description_type: {
          type: "string",
          enum: ["short_desc", "long_desc", "both"],
          description:
            "Which description to generate: 'short_desc' for combat/examine, 'long_desc' for room appearance, 'both' for both",
        },
      },
      required: ["mobile_id"],
    },
  },
  {
    name: "get_description_examples",
    description:
      "Find example descriptions from existing entities in the game. Useful for understanding the style and format of descriptions in a particular area or for a particular entity type.",
    inputSchema: {
      type: "object",
      properties: {
        entity_type: {
          type: "string",
          enum: ["room", "item", "mobile"],
          description: "Type of entity to find examples for",
        },
        filter: {
          type: "object",
          properties: {
            area_prefix: {
              type: "string",
              description: "Only include entities with vnums starting with this prefix",
            },
            item_type: {
              type: "string",
              enum: [
                "misc",
                "armor",
                "weapon",
                "container",
                "liquid_container",
                "food",
                "key",
                "gold",
              ],
              description: "For items only: filter by item type",
            },
            has_flag: {
              type: "string",
              description: "Only include entities that have this flag set (e.g., 'dark', 'aggressive')",
            },
            min_length: {
              type: "number",
              description: "Minimum description length in characters",
            },
            max_length: {
              type: "number",
              description: "Maximum description length in characters",
            },
          },
          description: "Optional filters to narrow down examples",
        },
        limit: {
          type: "number",
          description: "Maximum number of examples to return (default: 3)",
        },
      },
      required: ["entity_type"],
    },
  },
];
