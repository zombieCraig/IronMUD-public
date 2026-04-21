#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema, ListResourcesRequestSchema, ReadResourceRequestSchema, } from "@modelcontextprotocol/sdk/types.js";
import { IronMUDApiClient } from "./api-client.js";
import { areaToolDefinitions } from "./tools/areas.js";
import { roomToolDefinitions } from "./tools/rooms.js";
import { itemToolDefinitions } from "./tools/items.js";
import { mobileToolDefinitions } from "./tools/mobiles.js";
import { spawnPointToolDefinitions } from "./tools/spawn-points.js";
import { transportToolDefinitions } from "./tools/transports.js";
import { descriptionToolDefinitions } from "./tools/descriptions.js";
import { plantToolDefinitions } from "./tools/plants.js";
import { bugToolDefinitions } from "./tools/bugs.js";
import { buildRoomContext, buildItemContext, buildMobileContext, getDescriptionExamples, } from "./description-context.js";
// Helper to format auto-refresh info for MCP output
function formatRefreshSuffix(refreshed) {
    if (refreshed !== undefined && refreshed > 0) {
        return `\n\n(${refreshed} spawned instance(s) auto-refreshed)`;
    }
    return "";
}
// Vnum resolution helpers — accept UUID or vnum, return UUID
async function resolveRoomId(apiClient, id) {
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(id))
        return id;
    const room = await apiClient.getRoomByVnum(id);
    return room.id;
}
async function resolveItemId(apiClient, id) {
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(id))
        return id;
    const item = await apiClient.getItemByVnum(id);
    return item.id;
}
async function resolveBugId(apiClient, id) {
    // If it looks like a UUID, use directly
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(id))
        return id;
    // Otherwise treat as ticket number
    const num = parseInt(id, 10);
    if (isNaN(num))
        throw new Error(`Invalid bug identifier: ${id}`);
    const report = await apiClient.getBugReportByTicket(num);
    return report.id;
}
async function resolveMobileId(apiClient, id) {
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(id))
        return id;
    const mobile = await apiClient.getMobileByVnum(id);
    return mobile.id;
}
// Configuration from environment
const API_URL = process.env.IRONMUD_API_URL || "http://localhost:4001/api/v1";
const API_KEY = process.env.IRONMUD_API_KEY;
if (!API_KEY) {
    console.error("Error: IRONMUD_API_KEY environment variable is required");
    process.exit(1);
}
// Initialize API client
const api = new IronMUDApiClient(API_URL, API_KEY);
// Create MCP server
const server = new Server({
    name: "ironmud-mcp",
    version: "0.1.0",
}, {
    capabilities: {
        tools: {},
        resources: {},
    },
});
// Combine all tool definitions
const allTools = [
    ...areaToolDefinitions,
    ...roomToolDefinitions,
    ...itemToolDefinitions,
    ...mobileToolDefinitions,
    ...spawnPointToolDefinitions,
    ...transportToolDefinitions,
    ...descriptionToolDefinitions,
    ...plantToolDefinitions,
    ...bugToolDefinitions,
];
// Handle list tools request
server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
        tools: allTools,
    };
});
// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
        // Area tools
        switch (name) {
            case "list_areas": {
                const areas = await api.listAreas();
                return {
                    content: [{ type: "text", text: JSON.stringify(areas, null, 2) }],
                };
            }
            case "get_area": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                try {
                    const area = await api.getArea(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(area, null, 2) }],
                    };
                }
                catch {
                    const area = await api.getAreaByPrefix(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(area, null, 2) }],
                    };
                }
            }
            case "create_area": {
                const area = await api.createArea({
                    name: args?.name,
                    prefix: args?.prefix,
                    description: args?.description,
                    level_min: args?.level_min,
                    level_max: args?.level_max,
                    theme: args?.theme,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(area, null, 2) }],
                };
            }
            case "update_area": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const area = await api.updateArea(id, {
                    name: args?.name,
                    prefix: args?.prefix,
                    description: args?.description,
                    level_min: args?.level_min,
                    level_max: args?.level_max,
                    theme: args?.theme,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(area, null, 2) }],
                };
            }
            case "delete_area": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                await api.deleteArea(id);
                return {
                    content: [{ type: "text", text: `Area ${id} deleted successfully` }],
                };
            }
            case "reset_area": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const result = await api.resetArea(id);
                return {
                    content: [
                        {
                            type: "text",
                            text: `Area reset. ${result.spawned_count} entities scheduled for respawn.`,
                        },
                    ],
                };
            }
            case "list_rooms_in_area": {
                const areaId = args?.area_id;
                if (!areaId)
                    throw new Error("area_id is required");
                const rooms = await api.listAreaRooms(areaId);
                return {
                    content: [{ type: "text", text: JSON.stringify(rooms, null, 2) }],
                };
            }
            case "get_area_overview": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                let areaId;
                try {
                    const area = await api.getArea(identifier);
                    areaId = area.id;
                }
                catch {
                    const area = await api.getAreaByPrefix(identifier);
                    areaId = area.id;
                }
                const overview = await api.getAreaOverview(areaId);
                return {
                    content: [{ type: "text", text: JSON.stringify(overview, null, 2) }],
                };
            }
            // Room tools
            case "list_rooms_summary": {
                const roomSummaries = await api.listRoomsSummary(args?.area_id, args?.vnum_prefix);
                return {
                    content: [{ type: "text", text: JSON.stringify(roomSummaries, null, 2) }],
                };
            }
            case "get_room": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                try {
                    const room = await api.getRoom(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                    };
                }
                catch {
                    const room = await api.getRoomByVnum(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                    };
                }
            }
            case "create_room": {
                const room = await api.createRoom({
                    title: args?.title,
                    description: args?.description,
                    area_id: args?.area_id,
                    vnum: args?.vnum,
                    flags: args?.flags,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "update_room": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedRoomId = await resolveRoomId(api, id);
                const room = await api.updateRoom(resolvedRoomId, {
                    title: args?.title,
                    description: args?.description,
                    flags: args?.flags,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "delete_room": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedRoomId = await resolveRoomId(api, id);
                await api.deleteRoom(resolvedRoomId);
                return {
                    content: [{ type: "text", text: `Room ${id} deleted successfully` }],
                };
            }
            case "set_room_exit": {
                const roomId = args?.room_id;
                const direction = args?.direction;
                const targetRoomId = args?.target_room_id;
                if (!roomId || !direction || !targetRoomId) {
                    throw new Error("room_id, direction, and target_room_id are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const resolvedTargetRoomId = await resolveRoomId(api, targetRoomId);
                const room = await api.setRoomExit(resolvedRoomId, direction, {
                    target_room_id: resolvedTargetRoomId,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "remove_room_exit": {
                const roomId = args?.room_id;
                const direction = args?.direction;
                if (!roomId || !direction) {
                    throw new Error("room_id and direction are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.removeRoomExit(resolvedRoomId, direction);
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "add_room_door": {
                const roomId = args?.room_id;
                const direction = args?.direction;
                const doorName = args?.name;
                if (!roomId || !direction || !doorName) {
                    throw new Error("room_id, direction, and name are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.addRoomDoor(resolvedRoomId, direction, {
                    name: doorName,
                    is_closed: args?.is_closed,
                    is_locked: args?.is_locked,
                    key_id: args?.key_id,
                    keywords: args?.keywords,
                    description: args?.description,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "remove_room_door": {
                const roomId = args?.room_id;
                const direction = args?.direction;
                if (!roomId || !direction) {
                    throw new Error("room_id and direction are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.removeRoomDoor(resolvedRoomId, direction);
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "add_room_trigger": {
                const roomId = args?.room_id;
                const triggerType = args?.trigger_type;
                const scriptName = args?.script_name;
                if (!roomId || !triggerType || !scriptName) {
                    throw new Error("room_id, trigger_type, and script_name are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.addRoomTrigger(resolvedRoomId, {
                    trigger_type: triggerType,
                    script_name: scriptName,
                    enabled: args?.enabled,
                    interval_secs: args?.interval_secs,
                    chance: args?.chance,
                    args: args?.args,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "remove_room_trigger": {
                const roomId = args?.room_id;
                const index = args?.index;
                if (!roomId || index === undefined) {
                    throw new Error("room_id and index are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.removeRoomTrigger(resolvedRoomId, index);
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "add_room_extra_desc": {
                const roomId = args?.room_id;
                const keywords = args?.keywords;
                const description = args?.description;
                if (!roomId || !keywords || !description) {
                    throw new Error("room_id, keywords, and description are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.addRoomExtraDesc(resolvedRoomId, { keywords, description });
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            case "remove_room_extra_desc": {
                const roomId = args?.room_id;
                const keyword = args?.keyword;
                if (!roomId || !keyword) {
                    throw new Error("room_id and keyword are required");
                }
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const room = await api.removeRoomExtraDesc(resolvedRoomId, keyword);
                return {
                    content: [{ type: "text", text: JSON.stringify(room, null, 2) }],
                };
            }
            // Item tools
            case "list_items": {
                const items = await api.listItems(args?.limit, args?.offset, args?.item_type);
                return {
                    content: [{ type: "text", text: JSON.stringify(items, null, 2) }],
                };
            }
            case "list_item_prototypes": {
                const items = await api.listItemPrototypes();
                return {
                    content: [{ type: "text", text: JSON.stringify(items, null, 2) }],
                };
            }
            case "list_item_prototypes_summary": {
                const itemSummaries = await api.listItemPrototypesSummary(args?.vnum_prefix);
                return {
                    content: [{ type: "text", text: JSON.stringify(itemSummaries, null, 2) }],
                };
            }
            case "get_item": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                try {
                    const item = await api.getItem(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(item, null, 2) }],
                    };
                }
                catch {
                    const item = await api.getItemByVnum(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(item, null, 2) }],
                    };
                }
            }
            case "create_item": {
                const item = await api.createItem({
                    name: args?.name,
                    short_desc: args?.short_desc,
                    long_desc: args?.long_desc,
                    vnum: args?.vnum,
                    keywords: args?.keywords,
                    item_type: args?.item_type,
                    weight: args?.weight,
                    value: args?.value,
                    wear_location: args?.wear_location,
                    damage_dice_count: args?.damage_dice_count,
                    damage_dice_sides: args?.damage_dice_sides,
                    damage_type: args?.damage_type,
                    armor_class: args?.armor_class,
                    flags: args?.flags,
                    caliber: args?.caliber,
                    ranged_type: args?.ranged_type,
                    magazine_size: args?.magazine_size,
                    fire_mode: args?.fire_mode,
                    supported_fire_modes: args?.supported_fire_modes,
                    noise_level: args?.noise_level,
                    two_handed: args?.two_handed,
                    ammo_count: args?.ammo_count,
                    ammo_damage_bonus: args?.ammo_damage_bonus,
                    attachment_slot: args?.attachment_slot,
                    attachment_accuracy_bonus: args?.attachment_accuracy_bonus,
                    attachment_noise_reduction: args?.attachment_noise_reduction,
                    attachment_magazine_bonus: args?.attachment_magazine_bonus,
                    weapon_skill: args?.weapon_skill,
                    plant_prototype_vnum: args?.plant_prototype_vnum,
                    fertilizer_duration: args?.fertilizer_duration,
                    treats_infestation: args?.treats_infestation,
                    liquid_type: args?.liquid_type,
                    liquid_current: args?.liquid_current,
                    liquid_max: args?.liquid_max,
                    medical_tier: args?.medical_tier,
                    medical_uses: args?.medical_uses,
                    treats_wound_types: args?.treats_wound_types,
                    food_nutrition: args?.food_nutrition,
                    food_spoil_duration: args?.food_spoil_duration,
                    food_effects: args?.food_effects,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(item, null, 2) }],
                };
            }
            case "update_item": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedItemId = await resolveItemId(api, id);
                const updateData = {};
                const itemFields = [
                    "name", "short_desc", "long_desc", "vnum", "item_type", "keywords", "weight", "value", "flags",
                    "damage_dice_count", "damage_dice_sides", "damage_type", "armor_class",
                    "wear_location", "weapon_skill",
                    "caliber", "ranged_type", "magazine_size", "fire_mode", "supported_fire_modes",
                    "noise_level", "two_handed", "ammo_count", "ammo_damage_bonus",
                    "attachment_slot", "attachment_accuracy_bonus", "attachment_noise_reduction",
                    "attachment_magazine_bonus", "plant_prototype_vnum", "fertilizer_duration",
                    "treats_infestation", "liquid_type", "liquid_current", "liquid_max",
                    "medical_tier", "medical_uses", "treats_wound_types",
                    "food_nutrition", "food_spoil_duration", "food_effects",
                ];
                for (const field of itemFields) {
                    if (args?.[field] !== undefined) {
                        updateData[field] = args[field];
                    }
                }
                const itemResult = await api.updateItem(resolvedItemId, updateData);
                return {
                    content: [{ type: "text", text: JSON.stringify(itemResult.data, null, 2) + formatRefreshSuffix(itemResult.refreshed_instances) }],
                };
            }
            case "delete_item": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedItemId = await resolveItemId(api, id);
                await api.deleteItem(resolvedItemId);
                return {
                    content: [{ type: "text", text: `Item ${id} deleted successfully` }],
                };
            }
            case "spawn_item": {
                const vnum = args?.vnum;
                const roomId = args?.room_id;
                if (!vnum || !roomId)
                    throw new Error("vnum and room_id are required");
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const item = await api.spawnItem(vnum, { room_id: resolvedRoomId });
                return {
                    content: [{ type: "text", text: JSON.stringify(item, null, 2) }],
                };
            }
            // Mobile tools
            case "list_mobiles": {
                const mobiles = await api.listMobiles(args?.limit, args?.offset);
                return {
                    content: [{ type: "text", text: JSON.stringify(mobiles, null, 2) }],
                };
            }
            case "list_mobile_prototypes": {
                const mobiles = await api.listMobilePrototypes();
                return {
                    content: [{ type: "text", text: JSON.stringify(mobiles, null, 2) }],
                };
            }
            case "list_mobile_prototypes_summary": {
                const mobileSummaries = await api.listMobilePrototypesSummary(args?.vnum_prefix);
                return {
                    content: [{ type: "text", text: JSON.stringify(mobileSummaries, null, 2) }],
                };
            }
            case "get_mobile": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                try {
                    const mobile = await api.getMobile(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(mobile, null, 2) }],
                    };
                }
                catch {
                    const mobile = await api.getMobileByVnum(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(mobile, null, 2) }],
                    };
                }
            }
            case "create_mobile": {
                const mobile = await api.createMobile({
                    name: args?.name,
                    short_desc: args?.short_desc,
                    long_desc: args?.long_desc,
                    vnum: args?.vnum,
                    keywords: args?.keywords,
                    level: args?.level,
                    max_hp: args?.max_hp,
                    damage_dice: args?.damage_dice,
                    armor_class: args?.armor_class,
                    flags: args?.flags,
                    healer_type: args?.healer_type,
                    healing_free: args?.healing_free,
                    healing_cost_multiplier: args?.healing_cost_multiplier,
                    shop_sell_rate: args?.shop_sell_rate,
                    shop_buy_rate: args?.shop_buy_rate,
                    shop_buys_types: args?.shop_buys_types,
                    shop_stock: args?.shop_stock,
                    shop_preset_vnum: args?.shop_preset_vnum,
                    daily_routine: args?.daily_routine,
                    simulation: args?.simulation,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(mobile, null, 2) }],
                };
            }
            case "update_mobile": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedMobileId = await resolveMobileId(api, id);
                const mobileUpdateData = {};
                const mobileFields = [
                    "name", "short_desc", "long_desc", "vnum", "keywords", "level", "max_hp", "armor_class", "gold", "flags",
                    "healer_type", "healing_free", "healing_cost_multiplier",
                    "shop_sell_rate", "shop_buy_rate", "shop_buys_types", "shop_stock", "shop_preset_vnum",
                    "daily_routine", "simulation", "remove_simulation",
                ];
                for (const field of mobileFields) {
                    if (args?.[field] !== undefined) {
                        mobileUpdateData[field] = args[field];
                    }
                }
                const mobileResult = await api.updateMobile(resolvedMobileId, mobileUpdateData);
                return {
                    content: [{ type: "text", text: JSON.stringify(mobileResult.data, null, 2) + formatRefreshSuffix(mobileResult.refreshed_instances) }],
                };
            }
            case "delete_mobile": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const resolvedMobileId = await resolveMobileId(api, id);
                await api.deleteMobile(resolvedMobileId);
                return {
                    content: [{ type: "text", text: `Mobile ${id} deleted successfully` }],
                };
            }
            case "add_mobile_dialogue": {
                const mobileId = args?.mobile_id;
                const keyword = args?.keyword;
                const response = args?.response;
                if (!mobileId || !keyword || !response) {
                    throw new Error("mobile_id, keyword, and response are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const dialogueResult = await api.addMobileDialogue(resolvedMobileId, { keyword, response });
                return {
                    content: [{ type: "text", text: JSON.stringify(dialogueResult.data, null, 2) + formatRefreshSuffix(dialogueResult.refreshed_instances) }],
                };
            }
            case "remove_mobile_dialogue": {
                const mobileId = args?.mobile_id;
                const keyword = args?.keyword;
                if (!mobileId || !keyword) {
                    throw new Error("mobile_id and keyword are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const rmDialogueResult = await api.removeMobileDialogue(resolvedMobileId, keyword);
                return {
                    content: [{ type: "text", text: JSON.stringify(rmDialogueResult.data, null, 2) + formatRefreshSuffix(rmDialogueResult.refreshed_instances) }],
                };
            }
            case "add_mobile_routine": {
                const mobileId = args?.mobile_id;
                const startHour = args?.start_hour;
                const activity = args?.activity;
                if (!mobileId || startHour === undefined || !activity) {
                    throw new Error("mobile_id, start_hour, and activity are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const routineResult = await api.addMobileRoutine(resolvedMobileId, {
                    start_hour: startHour,
                    activity,
                    destination_vnum: args?.destination_vnum,
                    transition_message: args?.transition_message,
                    suppress_wander: args?.suppress_wander,
                    dialogue_overrides: args?.dialogue_overrides,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(routineResult.data, null, 2) + formatRefreshSuffix(routineResult.refreshed_instances) }],
                };
            }
            case "remove_mobile_routine": {
                const mobileId = args?.mobile_id;
                const index = args?.index;
                if (!mobileId || index === undefined) {
                    throw new Error("mobile_id and index are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const rmRoutineResult = await api.removeMobileRoutine(resolvedMobileId, index);
                return {
                    content: [{ type: "text", text: JSON.stringify(rmRoutineResult.data, null, 2) + formatRefreshSuffix(rmRoutineResult.refreshed_instances) }],
                };
            }
            case "spawn_mobile": {
                const vnum = args?.vnum;
                const roomId = args?.room_id;
                if (!vnum || !roomId)
                    throw new Error("vnum and room_id are required");
                const resolvedRoomId = await resolveRoomId(api, roomId);
                const mobile = await api.spawnMobile(vnum, { room_id: resolvedRoomId });
                return {
                    content: [{ type: "text", text: JSON.stringify(mobile, null, 2) }],
                };
            }
            case "add_mobile_trigger": {
                const mobileId = args?.mobile_id;
                const triggerType = args?.trigger_type;
                const scriptName = args?.script_name;
                if (!mobileId || !triggerType || !scriptName) {
                    throw new Error("mobile_id, trigger_type, and script_name are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const triggerResult = await api.addMobileTrigger(resolvedMobileId, {
                    trigger_type: triggerType,
                    script_name: scriptName,
                    enabled: args?.enabled,
                    interval_secs: args?.interval_secs,
                    chance: args?.chance,
                    args: args?.args,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(triggerResult.data, null, 2) + formatRefreshSuffix(triggerResult.refreshed_instances) }],
                };
            }
            case "remove_mobile_trigger": {
                const mobileId = args?.mobile_id;
                const index = args?.index;
                if (!mobileId || index === undefined) {
                    throw new Error("mobile_id and index are required");
                }
                const resolvedMobileId = await resolveMobileId(api, mobileId);
                const rmTriggerResult = await api.removeMobileTrigger(resolvedMobileId, index);
                return {
                    content: [{ type: "text", text: JSON.stringify(rmTriggerResult.data, null, 2) + formatRefreshSuffix(rmTriggerResult.refreshed_instances) }],
                };
            }
            // Spawn point tools
            case "list_spawn_points": {
                const spawnPoints = await api.listSpawnPoints(args?.area_id);
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoints, null, 2) }],
                };
            }
            case "get_spawn_point": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const spawnPoint = await api.getSpawnPoint(id);
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoint, null, 2) }],
                };
            }
            case "create_spawn_point": {
                const spawnPoint = await api.createSpawnPoint({
                    area_id: args?.area_id,
                    room_id: args?.room_id,
                    entity_type: args?.entity_type,
                    vnum: args?.vnum,
                    max_count: args?.max_count,
                    respawn_interval_secs: args?.respawn_interval_secs,
                    enabled: args?.enabled,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoint, null, 2) }],
                };
            }
            case "update_spawn_point": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const spawnPoint = await api.updateSpawnPoint(id, {
                    max_count: args?.max_count,
                    respawn_interval_secs: args?.respawn_interval_secs,
                    enabled: args?.enabled,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoint, null, 2) }],
                };
            }
            case "delete_spawn_point": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                await api.deleteSpawnPoint(id);
                return {
                    content: [{ type: "text", text: `Spawn point ${id} deleted successfully` }],
                };
            }
            case "add_spawn_dependency": {
                const spawnPointId = args?.spawn_point_id;
                const itemVnum = args?.item_vnum;
                const destination = args?.destination;
                if (!spawnPointId || !itemVnum || !destination) {
                    throw new Error("spawn_point_id, item_vnum, and destination are required");
                }
                const spawnPoint = await api.addSpawnDependency(spawnPointId, {
                    item_vnum: itemVnum,
                    destination,
                    wear_location: args?.wear_location,
                    count: args?.count,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoint, null, 2) }],
                };
            }
            case "remove_spawn_dependency": {
                const spawnPointId = args?.spawn_point_id;
                const index = args?.index;
                if (!spawnPointId || index === undefined) {
                    throw new Error("spawn_point_id and index are required");
                }
                const spawnPoint = await api.removeSpawnDependency(spawnPointId, index);
                return {
                    content: [{ type: "text", text: JSON.stringify(spawnPoint, null, 2) }],
                };
            }
            // Transport tools
            case "list_transports": {
                const transports = await api.listTransports();
                return {
                    content: [{ type: "text", text: JSON.stringify(transports, null, 2) }],
                };
            }
            case "get_transport": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                const transport = await api.getTransport(identifier);
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "create_transport": {
                const transport = await api.createTransport({
                    name: args?.name,
                    vnum: args?.vnum,
                    transport_type: args?.transport_type,
                    interior_room_id: args?.interior_room_id,
                    travel_time_secs: args?.travel_time_secs,
                    schedule_type: args?.schedule_type,
                    frequency_hours: args?.frequency_hours,
                    operating_start: args?.operating_start,
                    operating_end: args?.operating_end,
                    dwell_time_secs: args?.dwell_time_secs,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "update_transport": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const transportUpdateData = {};
                const transportFields = [
                    "name", "transport_type", "travel_time_secs", "schedule_type",
                    "frequency_hours", "operating_start", "operating_end", "dwell_time_secs",
                ];
                for (const field of transportFields) {
                    if (args?.[field] !== undefined) {
                        transportUpdateData[field] = args[field];
                    }
                }
                const transport = await api.updateTransport(id, transportUpdateData);
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "delete_transport": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                await api.deleteTransport(id);
                return {
                    content: [{ type: "text", text: `Transport ${id} deleted successfully` }],
                };
            }
            case "add_transport_stop": {
                const transportId = args?.transport_id;
                const roomId = args?.room_id;
                const stopName = args?.name;
                const exitDirection = args?.exit_direction;
                if (!transportId || !roomId || !stopName || !exitDirection) {
                    throw new Error("transport_id, room_id, name, and exit_direction are required");
                }
                const transport = await api.addTransportStop(transportId, {
                    room_id: roomId,
                    name: stopName,
                    exit_direction: exitDirection,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "remove_transport_stop": {
                const transportId = args?.transport_id;
                const index = args?.index;
                if (!transportId || index === undefined) {
                    throw new Error("transport_id and index are required");
                }
                const transport = await api.removeTransportStop(transportId, index);
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "connect_transport": {
                const transportId = args?.transport_id;
                const stopIndex = args?.stop_index;
                if (!transportId || stopIndex === undefined) {
                    throw new Error("transport_id and stop_index are required");
                }
                const transport = await api.connectTransport(transportId, stopIndex);
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "start_transport_travel": {
                const transportId = args?.transport_id;
                const destinationIndex = args?.destination_index;
                if (!transportId || destinationIndex === undefined) {
                    throw new Error("transport_id and destination_index are required");
                }
                const transport = await api.startTransportTravel(transportId, destinationIndex);
                return {
                    content: [{ type: "text", text: JSON.stringify(transport, null, 2) }],
                };
            }
            case "list_transport_types": {
                const types = ["elevator", "bus", "train", "ferry", "airship"];
                return {
                    content: [{ type: "text", text: JSON.stringify(types, null, 2) }],
                };
            }
            // Plant prototype tools
            case "list_plant_prototypes": {
                const plants = await api.listPlantPrototypes();
                return {
                    content: [{ type: "text", text: JSON.stringify(plants, null, 2) }],
                };
            }
            case "get_plant_prototype": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                try {
                    const plant = await api.getPlantPrototype(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(plant, null, 2) }],
                    };
                }
                catch {
                    const plant = await api.getPlantPrototypeByVnum(identifier);
                    return {
                        content: [{ type: "text", text: JSON.stringify(plant, null, 2) }],
                    };
                }
            }
            case "create_plant_prototype": {
                const plant = await api.createPlantPrototype({
                    name: args?.name,
                    vnum: args?.vnum,
                    keywords: args?.keywords,
                    seed_vnum: args?.seed_vnum,
                    harvest_vnum: args?.harvest_vnum,
                    harvest_min: args?.harvest_min,
                    harvest_max: args?.harvest_max,
                    category: args?.category,
                    stages: args?.stages,
                    preferred_seasons: args?.preferred_seasons,
                    forbidden_seasons: args?.forbidden_seasons,
                    water_consumption_per_hour: args?.water_consumption_per_hour,
                    water_capacity: args?.water_capacity,
                    indoor_only: args?.indoor_only,
                    min_skill_to_plant: args?.min_skill_to_plant,
                    base_xp: args?.base_xp,
                    pest_resistance: args?.pest_resistance,
                    multi_harvest: args?.multi_harvest,
                });
                return {
                    content: [{ type: "text", text: JSON.stringify(plant, null, 2) }],
                };
            }
            case "update_plant_prototype": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                const plantUpdateData = {};
                const plantFields = [
                    "name", "keywords", "seed_vnum", "harvest_vnum", "harvest_min", "harvest_max",
                    "category", "stages", "preferred_seasons", "forbidden_seasons",
                    "water_consumption_per_hour", "water_capacity", "indoor_only",
                    "min_skill_to_plant", "base_xp", "pest_resistance", "multi_harvest",
                ];
                for (const field of plantFields) {
                    if (args?.[field] !== undefined) {
                        plantUpdateData[field] = args[field];
                    }
                }
                const plant = await api.updatePlantPrototype(id, plantUpdateData);
                return {
                    content: [{ type: "text", text: JSON.stringify(plant, null, 2) }],
                };
            }
            case "delete_plant_prototype": {
                const id = args?.id;
                if (!id)
                    throw new Error("id is required");
                await api.deletePlantPrototype(id);
                return {
                    content: [{ type: "text", text: `Plant prototype ${id} deleted successfully` }],
                };
            }
            // Description context tools
            case "get_room_context": {
                const roomId = args?.room_id;
                if (!roomId)
                    throw new Error("room_id is required");
                const context = await buildRoomContext(api, roomId, args?.style_hints);
                return {
                    content: [{ type: "text", text: JSON.stringify(context, null, 2) }],
                };
            }
            case "get_item_context": {
                const itemId = args?.item_id;
                if (!itemId)
                    throw new Error("item_id is required");
                const context = await buildItemContext(api, itemId, args?.description_type);
                return {
                    content: [{ type: "text", text: JSON.stringify(context, null, 2) }],
                };
            }
            case "get_mobile_context": {
                const mobileId = args?.mobile_id;
                if (!mobileId)
                    throw new Error("mobile_id is required");
                const context = await buildMobileContext(api, mobileId, args?.description_type);
                return {
                    content: [{ type: "text", text: JSON.stringify(context, null, 2) }],
                };
            }
            case "get_description_examples": {
                const entityType = args?.entity_type;
                if (!entityType)
                    throw new Error("entity_type is required");
                const filter = args?.filter;
                const limit = args?.limit || 3;
                const examples = await getDescriptionExamples(api, entityType, filter, limit);
                return {
                    content: [{ type: "text", text: JSON.stringify(examples, null, 2) }],
                };
            }
            // Bug report tools
            case "list_bug_reports": {
                const reports = await api.listBugReports(args?.status);
                return {
                    content: [{ type: "text", text: JSON.stringify(reports, null, 2) }],
                };
            }
            case "get_bug_report": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                const bugId = await resolveBugId(api, identifier);
                const report = await api.getBugReport(bugId);
                return {
                    content: [{ type: "text", text: JSON.stringify(report, null, 2) }],
                };
            }
            case "update_bug_report": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                const bugId = await resolveBugId(api, identifier);
                const updateData = {};
                if (args?.status !== undefined)
                    updateData.status = args.status;
                if (args?.priority !== undefined)
                    updateData.priority = args.priority;
                const updated = await api.updateBugReport(bugId, updateData);
                return {
                    content: [{ type: "text", text: JSON.stringify(updated, null, 2) }],
                };
            }
            case "add_bug_note": {
                const identifier = args?.identifier;
                const author = args?.author;
                const message = args?.message;
                if (!identifier || !author || !message) {
                    throw new Error("identifier, author, and message are required");
                }
                const bugId = await resolveBugId(api, identifier);
                const noted = await api.addBugNote(bugId, { author, message });
                return {
                    content: [{ type: "text", text: JSON.stringify(noted, null, 2) }],
                };
            }
            case "close_bug_report": {
                const identifier = args?.identifier;
                const resolvedBy = args?.resolved_by;
                if (!identifier || !resolvedBy) {
                    throw new Error("identifier and resolved_by are required");
                }
                const bugId = await resolveBugId(api, identifier);
                // Close = set status to Closed + add resolution note
                const note = args?.note || "";
                const updatePayload = { status: "Closed" };
                const closed = await api.updateBugReport(bugId, updatePayload);
                // Add note if provided
                if (note) {
                    await api.addBugNote(bugId, { author: resolvedBy, message: note });
                }
                // Re-fetch to include the note
                const final_report = await api.getBugReport(bugId);
                return {
                    content: [{ type: "text", text: JSON.stringify(final_report, null, 2) }],
                };
            }
            case "delete_bug_report": {
                const identifier = args?.identifier;
                if (!identifier)
                    throw new Error("identifier is required");
                const bugId = await resolveBugId(api, identifier);
                await api.deleteBugReport(bugId);
                return {
                    content: [
                        { type: "text", text: `Bug report ${identifier} deleted successfully` },
                    ],
                };
            }
            default:
                throw new Error(`Unknown tool: ${name}`);
        }
    }
    catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        return {
            content: [{ type: "text", text: `Error: ${message}` }],
            isError: true,
        };
    }
});
// Handle list resources request
server.setRequestHandler(ListResourcesRequestSchema, async () => {
    return {
        resources: [
            {
                uri: "ironmud://areas",
                name: "All Areas",
                description: "List of all areas in the MUD",
                mimeType: "application/json",
            },
        ],
    };
});
// Handle read resource request
server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
    const { uri } = request.params;
    if (uri === "ironmud://areas") {
        const areas = await api.listAreas();
        return {
            contents: [
                {
                    uri,
                    mimeType: "application/json",
                    text: JSON.stringify(areas, null, 2),
                },
            ],
        };
    }
    // Handle ironmud://area/{prefix}
    if (uri.startsWith("ironmud://area/")) {
        const prefix = uri.slice("ironmud://area/".length);
        const area = await api.getAreaByPrefix(prefix);
        return {
            contents: [
                {
                    uri,
                    mimeType: "application/json",
                    text: JSON.stringify(area, null, 2),
                },
            ],
        };
    }
    // Handle ironmud://room/{vnum}
    if (uri.startsWith("ironmud://room/")) {
        const vnum = decodeURIComponent(uri.slice("ironmud://room/".length));
        const room = await api.getRoomByVnum(vnum);
        return {
            contents: [
                {
                    uri,
                    mimeType: "application/json",
                    text: JSON.stringify(room, null, 2),
                },
            ],
        };
    }
    // Handle ironmud://item/{vnum}
    if (uri.startsWith("ironmud://item/")) {
        const vnum = decodeURIComponent(uri.slice("ironmud://item/".length));
        const item = await api.getItemByVnum(vnum);
        return {
            contents: [
                {
                    uri,
                    mimeType: "application/json",
                    text: JSON.stringify(item, null, 2),
                },
            ],
        };
    }
    // Handle ironmud://mobile/{vnum}
    if (uri.startsWith("ironmud://mobile/")) {
        const vnum = decodeURIComponent(uri.slice("ironmud://mobile/".length));
        const mobile = await api.getMobileByVnum(vnum);
        return {
            contents: [
                {
                    uri,
                    mimeType: "application/json",
                    text: JSON.stringify(mobile, null, 2),
                },
            ],
        };
    }
    throw new Error(`Unknown resource: ${uri}`);
});
// Start server
async function main() {
    const transport = new StdioServerTransport();
    await server.connect(transport);
    console.error("IronMUD MCP server running on stdio");
}
main().catch((error) => {
    console.error("Fatal error:", error);
    process.exit(1);
});
//# sourceMappingURL=index.js.map