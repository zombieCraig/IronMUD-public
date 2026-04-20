import { IronMUDApiClient } from "./api-client.js";
import type {
  Room,
  Item,
  Mobile,
  Area,
  RoomContext,
  ItemContext,
  MobileContext,
  ConnectedRoom,
  ItemType,
  MobileFlags,
} from "./types.js";

// Style guide constant for MUD descriptions
export const MUD_STYLE_GUIDE = `MUD Description Style Guide:

1. LENGTH: Keep descriptions concise but evocative. Room descriptions: 2-4 sentences. Item/mobile short_desc: 5-15 words. Long_desc: 1-2 sentences.

2. PERSPECTIVE: Write in second person for rooms ("You stand in..."). Write in third person for items/mobiles ("A rusty sword lies here").

3. SENSORY DETAILS: Include multiple senses - sight, sound, smell, texture. Avoid purple prose.

4. VERB TENSE: Use present tense. "The wind howls" not "The wind howled".

5. AVOID: Meta-gaming references, real-world brand names, fourth-wall breaking, excessive adjectives.

6. EXITS: Don't explicitly list exits in descriptions - the game handles that. Instead, hint at them naturally ("A path leads north into darkness").

7. ATMOSPHERE: Match the area's theme. A crypt should feel different from a marketplace.

8. INTERACTIVITY: Mention objects players can examine or interact with.`;

// Suggested elements based on room flags
const FLAG_ELEMENTS: Record<string, string[]> = {
  dark: ["darkness", "shadows", "dim light", "gloom", "blackness"],
  indoors: ["walls", "ceiling", "floor", "enclosed space", "shelter"],
  safe: ["peaceful", "protected", "sanctuary", "calm", "secure"],
  no_mob: ["quiet", "still", "undisturbed", "serene"],
  death_trap: ["danger", "peril", "treacherous", "deadly"],
};

// Theme-based elements
const THEME_ELEMENTS: Record<string, string[]> = {
  undead: ["cold", "decay", "musty", "bones", "death", "silence"],
  forest: ["trees", "leaves", "wildlife", "dappled light", "bark", "moss"],
  cave: ["stone", "dripping water", "echoes", "stalactites", "dampness"],
  castle: ["stone walls", "tapestries", "torches", "grandeur", "architecture"],
  swamp: ["murky water", "mud", "insects", "decay", "humidity", "reeds"],
  desert: ["sand", "heat", "dryness", "wind", "sun", "mirages"],
  ocean: ["salt", "waves", "wind", "gulls", "spray", "endless horizon"],
  mountain: ["peaks", "thin air", "rock", "wind", "cold", "vastness"],
  city: ["crowds", "buildings", "commerce", "noise", "streets"],
  dungeon: ["stone", "chains", "darkness", "damp", "echoes", "despair"],
};

// Item type guidance
const ITEM_TYPE_GUIDANCE: Record<string, string> = {
  weapon:
    "Describe the weapon's construction, balance, and lethal features. Mention materials and craftsmanship.",
  armor:
    "Focus on protection level, materials, and how it would feel to wear. Note any decorative elements.",
  container:
    "Describe capacity hints, material, and opening mechanism. What might it hold?",
  liquid_container:
    "Describe the vessel's material and construction. Hint at what liquids it might contain.",
  food: "Describe appearance, aroma, and freshness. Make it appetizing (or disgusting if rotten).",
  key: "Describe unique identifying features - shape, material, engravings. Keys should be distinctive.",
  misc: "Focus on the most interesting aspect of the item. What makes it notable?",
  gold: "Describe the quantity, mint marks, or condition of the coins.",
};

// Item flag elements
const ITEM_FLAG_ELEMENTS: Record<string, string> = {
  glow: "glows softly",
  hum: "hums with power",
  invisible: "shimmers faintly",
  no_drop: "feels bound to your soul",
  no_get: "is firmly fixed in place",
  unique: "radiates an aura of singularity",
  no_sell: "seems to reject the concept of commerce",
};

// Mobile role detection based on flags
function detectMobileRole(flags: MobileFlags): string {
  if (flags.shopkeeper) return "merchant";
  if (flags.healer) return "healer";
  if (flags.aggressive) return "aggressive monster";
  if (flags.sentinel) return "stationary guard";
  if (flags.scavenger) return "scavenger";
  if (flags.thief) return "thief";
  if (flags.cant_swim) return "land-bound creature";
  return "neutral NPC";
}

// Behavior hints based on flags
function getMobileBehaviorHints(flags: MobileFlags): string[] {
  const hints: string[] = [];
  if (flags.aggressive) hints.push("Describe as threatening, hostile, or ready to attack");
  if (flags.sentinel) hints.push("Emphasize stillness, watchfulness, or duty");
  if (flags.shopkeeper) hints.push("Include mercantile elements - apron, coins, wares");
  if (flags.healer) hints.push("Include healing imagery - herbs, kindness, wisdom");
  if (flags.scavenger) hints.push("Describe opportunistic, hungry, or cunning behavior");
  if (flags.thief) hints.push("Describe as sneaky, light-fingered, or untrustworthy");
  if (flags.cant_swim) hints.push("Describe as a land creature that avoids water");
  return hints;
}

/**
 * Build context for room description generation
 */
export async function buildRoomContext(
  api: IronMUDApiClient,
  roomId: string,
  styleHints?: string
): Promise<RoomContext> {
  // Get the room
  let room: Room;
  try {
    room = await api.getRoom(roomId);
  } catch {
    room = await api.getRoomByVnum(roomId);
  }

  // Get area info if room has area_id
  let area: RoomContext["area"];
  if (room.area_id) {
    try {
      const areaData = await api.getArea(room.area_id);
      area = {
        name: areaData.name,
        theme: areaData.theme,
        level_min: areaData.level_min,
        level_max: areaData.level_max,
      };
    } catch {
      // Area may not exist
    }
  }

  // Get connected rooms
  const connectedRoomsData = await api.getConnectedRooms(room.id);
  const connected_rooms: ConnectedRoom[] = connectedRoomsData.map(({ direction, room: r }) => ({
    direction,
    room_id: r.id,
    title: r.title,
    has_door: !!room.doors[direction],
    door_name: room.doors[direction]?.name,
  }));

  // Build suggested elements from flags and theme
  const suggested_elements: string[] = [];

  // Add flag-based elements
  for (const [flag, elements] of Object.entries(FLAG_ELEMENTS)) {
    if (room.flags[flag as keyof typeof room.flags]) {
      suggested_elements.push(...elements);
    }
  }

  // Add theme-based elements
  if (area?.theme) {
    const themeKey = area.theme.toLowerCase();
    if (THEME_ELEMENTS[themeKey]) {
      suggested_elements.push(...THEME_ELEMENTS[themeKey]);
    }
  }

  // Customize style guide based on hints
  let style_guide = MUD_STYLE_GUIDE;
  if (styleHints === "brief") {
    style_guide += "\n\nBRIEF MODE: Keep description to 1-2 sentences maximum.";
  } else if (styleHints === "atmospheric") {
    style_guide += "\n\nATMOSPHERIC MODE: Emphasize mood, atmosphere, and sensory details.";
  } else if (styleHints === "detailed") {
    style_guide += "\n\nDETAILED MODE: Include more environmental details and interactive elements.";
  }

  return {
    room: {
      id: room.id,
      title: room.title,
      current_description: room.description,
      vnum: room.vnum,
      flags: room.flags,
    },
    area,
    connected_rooms,
    suggested_elements: [...new Set(suggested_elements)], // Dedupe
    style_guide,
  };
}

/**
 * Build context for item description generation
 */
export async function buildItemContext(
  api: IronMUDApiClient,
  itemId: string,
  descriptionType?: string
): Promise<ItemContext> {
  // Get the item
  let item: Item;
  try {
    item = await api.getItem(itemId);
  } catch {
    item = await api.getItemByVnum(itemId);
  }

  // Get type guidance
  const type_guidance =
    ITEM_TYPE_GUIDANCE[item.item_type] || ITEM_TYPE_GUIDANCE.misc;

  // Build flag elements
  const flag_elements: string[] = [];
  for (const [flag, description] of Object.entries(ITEM_FLAG_ELEMENTS)) {
    if (item.flags[flag as keyof typeof item.flags]) {
      flag_elements.push(description);
    }
  }

  // Customize style guide based on description type
  let style_guide = MUD_STYLE_GUIDE;
  if (descriptionType === "short_desc") {
    style_guide +=
      "\n\nSHORT DESC: Write a brief phrase (5-15 words) describing the item as it appears in inventory or when examined. Example: 'a rusty iron sword with a notched blade'";
  } else if (descriptionType === "long_desc") {
    style_guide +=
      "\n\nLONG DESC: Write 1-2 sentences describing the item as it appears on the ground. Example: 'A rusty iron sword lies here, its notched blade covered in dried blood.'";
  } else {
    style_guide +=
      "\n\nBOTH DESCRIPTIONS NEEDED:\n- short_desc: Brief phrase for inventory (5-15 words)\n- long_desc: 1-2 sentences for ground appearance";
  }

  return {
    item: {
      id: item.id,
      name: item.name,
      item_type: item.item_type,
      current_short_desc: item.short_desc,
      current_long_desc: item.long_desc,
      flags: item.flags,
      weight: item.weight,
      value: item.value,
      wear_locations: item.wear_locations,
      damage_dice_count: item.damage_dice_count,
      damage_dice_sides: item.damage_dice_sides,
      damage_type: item.damage_type,
      armor_class: item.armor_class,
    },
    type_guidance,
    flag_elements,
    style_guide,
  };
}

/**
 * Build context for mobile description generation
 */
export async function buildMobileContext(
  api: IronMUDApiClient,
  mobileId: string,
  descriptionType?: string
): Promise<MobileContext> {
  // Get the mobile
  let mobile: Mobile;
  try {
    mobile = await api.getMobile(mobileId);
  } catch {
    mobile = await api.getMobileByVnum(mobileId);
  }

  // Detect role from flags
  const role = detectMobileRole(mobile.flags);

  // Get behavior hints
  const behavior_hints = getMobileBehaviorHints(mobile.flags);

  // Try to get area info from current room
  let area: MobileContext["area"];
  if (mobile.current_room_id) {
    try {
      const room = await api.getRoom(mobile.current_room_id);
      if (room.area_id) {
        const areaData = await api.getArea(room.area_id);
        area = {
          name: areaData.name,
          theme: areaData.theme,
        };
      }
    } catch {
      // Ignore errors
    }
  }

  // Customize style guide based on description type
  let style_guide = MUD_STYLE_GUIDE;
  if (descriptionType === "short_desc") {
    style_guide +=
      "\n\nSHORT DESC: Write a brief phrase (5-15 words) describing the mobile. Used in combat and when examining. Example: 'a grizzled old soldier with a scarred face'";
  } else if (descriptionType === "long_desc") {
    style_guide +=
      "\n\nLONG DESC: Write 1-2 sentences describing the mobile in the room. Example: 'A grizzled old soldier stands here, eyeing you warily with his one good eye.'";
  } else {
    style_guide +=
      "\n\nBOTH DESCRIPTIONS NEEDED:\n- short_desc: Brief phrase for combat/examine (5-15 words)\n- long_desc: 1-2 sentences for room appearance";
  }

  return {
    mobile: {
      id: mobile.id,
      name: mobile.name,
      level: mobile.level,
      current_short_desc: mobile.short_desc,
      current_long_desc: mobile.long_desc,
      flags: mobile.flags,
      dialogue_keywords: Object.keys(mobile.dialogue || {}),
    },
    role,
    behavior_hints,
    area,
    style_guide,
  };
}

/**
 * Get example descriptions from existing entities
 */
export async function getDescriptionExamples(
  api: IronMUDApiClient,
  entityType: "room" | "item" | "mobile",
  filter?: {
    area_prefix?: string;
    item_type?: ItemType;
    has_flag?: string;
    min_length?: number;
    max_length?: number;
  },
  limit: number = 3
): Promise<
  Array<{
    vnum?: string;
    name: string;
    description?: string;
    short_desc?: string;
    long_desc?: string;
    flags: Record<string, boolean>;
  }>
> {
  const examples: Array<{
    vnum?: string;
    name: string;
    description?: string;
    short_desc?: string;
    long_desc?: string;
    flags: Record<string, boolean>;
  }> = [];

  try {
    switch (entityType) {
      case "room": {
        const rooms = await api.searchRooms({
          area_prefix: filter?.area_prefix,
          has_flag: filter?.has_flag,
          limit: limit * 3, // Get more to filter
        });

        for (const room of rooms) {
          const desc = room.description;
          if (filter?.min_length && desc.length < filter.min_length) continue;
          if (filter?.max_length && desc.length > filter.max_length) continue;

          examples.push({
            vnum: room.vnum,
            name: room.title,
            description: desc,
            flags: room.flags as Record<string, boolean>,
          });

          if (examples.length >= limit) break;
        }
        break;
      }

      case "item": {
        const items = await api.searchItems({
          area_prefix: filter?.area_prefix,
          item_type: filter?.item_type,
          has_flag: filter?.has_flag,
          limit: limit * 3,
        });

        for (const item of items) {
          const desc = item.long_desc;
          if (filter?.min_length && desc.length < filter.min_length) continue;
          if (filter?.max_length && desc.length > filter.max_length) continue;

          examples.push({
            vnum: item.vnum,
            name: item.name,
            short_desc: item.short_desc,
            long_desc: item.long_desc,
            flags: item.flags as Record<string, boolean>,
          });

          if (examples.length >= limit) break;
        }
        break;
      }

      case "mobile": {
        const mobiles = await api.searchMobiles({
          area_prefix: filter?.area_prefix,
          has_flag: filter?.has_flag,
          limit: limit * 3,
        });

        for (const mobile of mobiles) {
          const desc = mobile.long_desc;
          if (filter?.min_length && desc.length < filter.min_length) continue;
          if (filter?.max_length && desc.length > filter.max_length) continue;

          examples.push({
            vnum: mobile.vnum,
            name: mobile.name,
            short_desc: mobile.short_desc,
            long_desc: mobile.long_desc,
            flags: mobile.flags as Record<string, boolean>,
          });

          if (examples.length >= limit) break;
        }
        break;
      }
    }
  } catch {
    // Return whatever examples we have
  }

  return examples;
}
