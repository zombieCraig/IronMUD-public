import { IronMUDApiClient } from "./api-client.js";
import type { RoomContext, ItemContext, MobileContext, ItemType } from "./types.js";
export declare const MUD_STYLE_GUIDE = "MUD Description Style Guide:\n\n1. LENGTH: Keep descriptions concise but evocative. Room descriptions: 2-4 sentences. Item/mobile short_desc: 5-15 words. Long_desc: 1-2 sentences.\n\n2. PERSPECTIVE: Write in second person for rooms (\"You stand in...\"). Write in third person for items/mobiles (\"A rusty sword lies here\").\n\n3. SENSORY DETAILS: Include multiple senses - sight, sound, smell, texture. Avoid purple prose.\n\n4. VERB TENSE: Use present tense. \"The wind howls\" not \"The wind howled\".\n\n5. AVOID: Meta-gaming references, real-world brand names, fourth-wall breaking, excessive adjectives.\n\n6. EXITS: Don't explicitly list exits in descriptions - the game handles that. Instead, hint at them naturally (\"A path leads north into darkness\").\n\n7. ATMOSPHERE: Match the area's theme. A crypt should feel different from a marketplace.\n\n8. INTERACTIVITY: Mention objects players can examine or interact with.";
/**
 * Build context for room description generation
 */
export declare function buildRoomContext(api: IronMUDApiClient, roomId: string, styleHints?: string): Promise<RoomContext>;
/**
 * Build context for item description generation
 */
export declare function buildItemContext(api: IronMUDApiClient, itemId: string, descriptionType?: string): Promise<ItemContext>;
/**
 * Build context for mobile description generation
 */
export declare function buildMobileContext(api: IronMUDApiClient, mobileId: string, descriptionType?: string): Promise<MobileContext>;
/**
 * Get example descriptions from existing entities
 */
export declare function getDescriptionExamples(api: IronMUDApiClient, entityType: "room" | "item" | "mobile", filter?: {
    area_prefix?: string;
    item_type?: ItemType;
    has_flag?: string;
    min_length?: number;
    max_length?: number;
}, limit?: number): Promise<Array<{
    vnum?: string;
    name: string;
    description?: string;
    short_desc?: string;
    long_desc?: string;
    flags: Record<string, boolean>;
}>>;
//# sourceMappingURL=description-context.d.ts.map