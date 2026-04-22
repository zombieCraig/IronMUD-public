import type { Area, Room, Item, Mobile, SpawnPoint, Transport, PlantPrototype, ItemSummary, RoomSummary, MobileSummary, AreaOverview, CreateAreaRequest, UpdateAreaRequest, CreateRoomRequest, CreateItemRequest, CreateMobileRequest, CreateSpawnPointRequest, CreateTransportRequest, CreatePlantPrototypeRequest, AddTransportStopRequest, SetExitRequest, AddDoorRequest, AddTriggerRequest, AddExtraDescRequest, AddDialogueRequest, AddMobileTriggerRequest, AddItemTriggerRequest, AddSpawnDependencyRequest, SpawnEntityRequest, BugReport, UpdateBugReportRequest, AddBugNoteRequest } from "./types.js";
export declare class IronMUDApiClient {
    private client;
    constructor(baseUrl: string, apiKey: string);
    private request;
    private requestWithMeta;
    private listRequest;
    health(): Promise<{
        status: string;
        version: string;
    }>;
    listAreas(): Promise<Area[]>;
    getArea(id: string): Promise<Area>;
    getAreaByPrefix(prefix: string): Promise<Area>;
    createArea(data: CreateAreaRequest): Promise<Area>;
    updateArea(id: string, data: UpdateAreaRequest): Promise<Area>;
    deleteArea(id: string): Promise<void>;
    resetArea(id: string): Promise<{
        message: string;
        spawned_count: number;
    }>;
    listAreaRooms(areaId: string): Promise<Room[]>;
    getAreaOverview(areaId: string): Promise<AreaOverview>;
    listRooms(limit?: number, offset?: number): Promise<Room[]>;
    listRoomsSummary(areaId?: string, vnumPrefix?: string): Promise<RoomSummary[]>;
    getRoom(id: string): Promise<Room>;
    getRoomByVnum(vnum: string): Promise<Room>;
    createRoom(data: CreateRoomRequest): Promise<Room>;
    updateRoom(id: string, data: Partial<CreateRoomRequest>): Promise<Room>;
    deleteRoom(id: string): Promise<void>;
    setRoomExit(roomId: string, direction: string, data: SetExitRequest): Promise<Room>;
    removeRoomExit(roomId: string, direction: string): Promise<Room>;
    addRoomDoor(roomId: string, direction: string, data: AddDoorRequest): Promise<Room>;
    removeRoomDoor(roomId: string, direction: string): Promise<Room>;
    addRoomTrigger(roomId: string, data: AddTriggerRequest): Promise<Room>;
    removeRoomTrigger(roomId: string, index: number): Promise<Room>;
    addRoomExtraDesc(roomId: string, data: AddExtraDescRequest): Promise<Room>;
    removeRoomExtraDesc(roomId: string, keyword: string): Promise<Room>;
    listItems(limit?: number, offset?: number, itemType?: string): Promise<Item[]>;
    listItemPrototypes(): Promise<Item[]>;
    listItemPrototypesSummary(vnumPrefix?: string): Promise<ItemSummary[]>;
    getItem(id: string): Promise<Item>;
    getItemByVnum(vnum: string): Promise<Item>;
    createItem(data: CreateItemRequest): Promise<Item>;
    updateItem(id: string, data: Partial<CreateItemRequest>): Promise<{
        data: Item;
        refreshed_instances?: number;
    }>;
    deleteItem(id: string): Promise<void>;
    spawnItem(vnum: string, data: SpawnEntityRequest): Promise<Item>;
    addItemTrigger(itemId: string, data: AddItemTriggerRequest): Promise<{
        data: Item;
        refreshed_instances?: number;
    }>;
    removeItemTrigger(itemId: string, index: number): Promise<{
        data: Item;
        refreshed_instances?: number;
    }>;
    listMobiles(limit?: number, offset?: number): Promise<Mobile[]>;
    listMobilePrototypes(): Promise<Mobile[]>;
    listMobilePrototypesSummary(vnumPrefix?: string): Promise<MobileSummary[]>;
    getMobile(id: string): Promise<Mobile>;
    getMobileByVnum(vnum: string): Promise<Mobile>;
    createMobile(data: CreateMobileRequest): Promise<Mobile>;
    updateMobile(id: string, data: Partial<CreateMobileRequest>): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    deleteMobile(id: string): Promise<void>;
    addMobileDialogue(mobileId: string, data: AddDialogueRequest): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    removeMobileDialogue(mobileId: string, keyword: string): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    addMobileRoutine(mobileId: string, data: {
        start_hour: number;
        activity: string;
        destination_vnum?: string;
        transition_message?: string;
        suppress_wander?: boolean;
        dialogue_overrides?: Record<string, string>;
    }): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    removeMobileRoutine(mobileId: string, index: number): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    addMobileTrigger(mobileId: string, data: AddMobileTriggerRequest): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    removeMobileTrigger(mobileId: string, index: number): Promise<{
        data: Mobile;
        refreshed_instances?: number;
    }>;
    spawnMobile(vnum: string, data: SpawnEntityRequest): Promise<Mobile>;
    listSpawnPoints(areaId?: string): Promise<SpawnPoint[]>;
    getSpawnPoint(id: string): Promise<SpawnPoint>;
    createSpawnPoint(data: CreateSpawnPointRequest): Promise<SpawnPoint>;
    updateSpawnPoint(id: string, data: Partial<CreateSpawnPointRequest>): Promise<SpawnPoint>;
    deleteSpawnPoint(id: string): Promise<void>;
    addSpawnDependency(spawnPointId: string, data: AddSpawnDependencyRequest): Promise<SpawnPoint>;
    removeSpawnDependency(spawnPointId: string, index: number): Promise<SpawnPoint>;
    listTransports(): Promise<Transport[]>;
    getTransport(id: string): Promise<Transport>;
    createTransport(data: CreateTransportRequest): Promise<Transport>;
    updateTransport(id: string, data: Partial<CreateTransportRequest>): Promise<Transport>;
    deleteTransport(id: string): Promise<void>;
    addTransportStop(transportId: string, data: AddTransportStopRequest): Promise<Transport>;
    removeTransportStop(transportId: string, index: number): Promise<Transport>;
    connectTransport(transportId: string, stopIndex: number): Promise<Transport>;
    startTransportTravel(transportId: string, destinationIndex: number): Promise<Transport>;
    listPlantPrototypes(): Promise<PlantPrototype[]>;
    getPlantPrototype(id: string): Promise<PlantPrototype>;
    getPlantPrototypeByVnum(vnum: string): Promise<PlantPrototype>;
    createPlantPrototype(data: CreatePlantPrototypeRequest): Promise<PlantPrototype>;
    updatePlantPrototype(id: string, data: Partial<CreatePlantPrototypeRequest>): Promise<PlantPrototype>;
    deletePlantPrototype(id: string): Promise<void>;
    listBugReports(status?: string): Promise<BugReport[]>;
    getBugReport(id: string): Promise<BugReport>;
    getBugReportByTicket(num: number): Promise<BugReport>;
    updateBugReport(id: string, data: UpdateBugReportRequest): Promise<BugReport>;
    addBugNote(id: string, data: AddBugNoteRequest): Promise<BugReport>;
    deleteBugReport(id: string): Promise<void>;
    /**
     * Get all rooms connected to a room via exits
     */
    getConnectedRooms(roomId: string): Promise<{
        direction: string;
        room: Room;
    }[]>;
    /**
     * Search for entities of a type with optional filters
     */
    searchItems(filter?: {
        item_type?: string;
        area_prefix?: string;
        has_flag?: string;
        limit?: number;
    }): Promise<Item[]>;
    searchMobiles(filter?: {
        area_prefix?: string;
        has_flag?: string;
        limit?: number;
    }): Promise<Mobile[]>;
    searchRooms(filter?: {
        area_prefix?: string;
        has_flag?: string;
        limit?: number;
    }): Promise<Room[]>;
}
//# sourceMappingURL=api-client.d.ts.map