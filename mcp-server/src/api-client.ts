import axios, { AxiosInstance, AxiosError } from "axios";
import type {
  ApiResponse,
  ListResponse,
  Area,
  Room,
  Item,
  Mobile,
  SpawnPoint,
  Transport,
  PlantPrototype,
  ItemSummary,
  RoomSummary,
  MobileSummary,
  AreaOverview,
  CreateAreaRequest,
  CreateRoomRequest,
  CreateItemRequest,
  CreateMobileRequest,
  CreateSpawnPointRequest,
  CreateTransportRequest,
  CreatePlantPrototypeRequest,
  AddTransportStopRequest,
  ConnectTransportRequest,
  TravelTransportRequest,
  SetExitRequest,
  AddDoorRequest,
  AddTriggerRequest,
  AddExtraDescRequest,
  AddDialogueRequest,
  AddMobileTriggerRequest,
  AddItemTriggerRequest,
  AddSpawnDependencyRequest,
  SpawnEntityRequest,
  BugReport,
  UpdateBugReportRequest,
  AddBugNoteRequest,
} from "./types.js";

export class IronMUDApiClient {
  private client: AxiosInstance;

  constructor(baseUrl: string, apiKey: string) {
    this.client = axios.create({
      baseURL: baseUrl,
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
    });
  }

  private async request<T>(
    method: "get" | "post" | "put" | "delete",
    path: string,
    data?: unknown
  ): Promise<T> {
    try {
      const response = await this.client.request<ApiResponse<T>>({
        method,
        url: path,
        data,
      });
      if (!response.data.success) {
        throw new Error(
          response.data.error?.message || "Unknown API error"
        );
      }
      return response.data.data as T;
    } catch (error) {
      if (error instanceof AxiosError) {
        const apiError = error.response?.data?.error;
        throw new Error(
          apiError?.message || error.message || "API request failed"
        );
      }
      throw error;
    }
  }

  private async requestWithMeta<T>(
    method: "get" | "post" | "put" | "delete",
    path: string,
    data?: unknown
  ): Promise<{ data: T; refreshed_instances?: number }> {
    try {
      const response = await this.client.request<ApiResponse<T>>({
        method,
        url: path,
        data,
      });
      if (!response.data.success) {
        throw new Error(
          response.data.error?.message || "Unknown API error"
        );
      }
      return {
        data: response.data.data as T,
        refreshed_instances: response.data.refreshed_instances,
      };
    } catch (error) {
      if (error instanceof AxiosError) {
        const apiError = error.response?.data?.error;
        throw new Error(
          apiError?.message || error.message || "API request failed"
        );
      }
      throw error;
    }
  }

  private async listRequest<T>(path: string): Promise<T[]> {
    try {
      const response = await this.client.get<ListResponse<T>>(path);
      if (!response.data.success) {
        throw new Error("Failed to list entities");
      }
      return response.data.data;
    } catch (error) {
      if (error instanceof AxiosError) {
        const apiError = error.response?.data?.error;
        throw new Error(
          apiError?.message || error.message || "API request failed"
        );
      }
      throw error;
    }
  }

  // Health check
  async health(): Promise<{ status: string; version: string }> {
    const response = await this.client.get("/health");
    return response.data;
  }

  // Areas
  async listAreas(): Promise<Area[]> {
    return this.listRequest<Area>("/areas");
  }

  async getArea(id: string): Promise<Area> {
    return this.request<Area>("get", `/areas/${id}`);
  }

  async getAreaByPrefix(prefix: string): Promise<Area> {
    return this.request<Area>("get", `/areas/by-prefix/${prefix}`);
  }

  async createArea(data: CreateAreaRequest): Promise<Area> {
    return this.request<Area>("post", "/areas", data);
  }

  async updateArea(
    id: string,
    data: Partial<CreateAreaRequest>
  ): Promise<Area> {
    return this.request<Area>("put", `/areas/${id}`, data);
  }

  async deleteArea(id: string): Promise<void> {
    await this.request("delete", `/areas/${id}`);
  }

  async resetArea(id: string): Promise<{ message: string; spawned_count: number }> {
    return this.request<{ message: string; spawned_count: number }>("post", `/areas/${id}/reset`);
  }

  async listAreaRooms(areaId: string): Promise<Room[]> {
    return this.listRequest<Room>(`/areas/${areaId}/rooms`);
  }

  async getAreaOverview(areaId: string): Promise<AreaOverview> {
    return this.request<AreaOverview>("get", `/areas/${areaId}/overview`);
  }

  // Rooms
  async listRooms(limit?: number, offset?: number): Promise<Room[]> {
    const params = new URLSearchParams();
    if (limit !== undefined) params.set("limit", limit.toString());
    if (offset !== undefined) params.set("offset", offset.toString());
    const query = params.toString();
    return this.listRequest<Room>(`/rooms${query ? `?${query}` : ""}`);
  }

  async listRoomsSummary(areaId?: string, vnumPrefix?: string): Promise<RoomSummary[]> {
    const params = new URLSearchParams();
    if (areaId) params.set("area_id", areaId);
    if (vnumPrefix) params.set("vnum_prefix", vnumPrefix);
    const query = params.toString();
    return this.listRequest<RoomSummary>(`/rooms/summary${query ? `?${query}` : ""}`);
  }

  async getRoom(id: string): Promise<Room> {
    return this.request<Room>("get", `/rooms/${id}`);
  }

  async getRoomByVnum(vnum: string): Promise<Room> {
    return this.request<Room>("get", `/rooms/by-vnum/${encodeURIComponent(vnum)}`);
  }

  async createRoom(data: CreateRoomRequest): Promise<Room> {
    return this.request<Room>("post", "/rooms", data);
  }

  async updateRoom(
    id: string,
    data: Partial<CreateRoomRequest>
  ): Promise<Room> {
    return this.request<Room>("put", `/rooms/${id}`, data);
  }

  async deleteRoom(id: string): Promise<void> {
    await this.request("delete", `/rooms/${id}`);
  }

  async setRoomExit(
    roomId: string,
    direction: string,
    data: SetExitRequest
  ): Promise<Room> {
    return this.request<Room>(
      "put",
      `/rooms/${roomId}/exits/${direction}`,
      data
    );
  }

  async removeRoomExit(roomId: string, direction: string): Promise<Room> {
    return this.request<Room>(
      "delete",
      `/rooms/${roomId}/exits/${direction}`
    );
  }

  async addRoomDoor(
    roomId: string,
    direction: string,
    data: AddDoorRequest
  ): Promise<Room> {
    return this.request<Room>(
      "put",
      `/rooms/${roomId}/doors/${direction}`,
      data
    );
  }

  async removeRoomDoor(roomId: string, direction: string): Promise<Room> {
    return this.request<Room>(
      "delete",
      `/rooms/${roomId}/doors/${direction}`
    );
  }

  async addRoomTrigger(roomId: string, data: AddTriggerRequest): Promise<Room> {
    return this.request<Room>("post", `/rooms/${roomId}/triggers`, data);
  }

  async removeRoomTrigger(roomId: string, index: number): Promise<Room> {
    return this.request<Room>("delete", `/rooms/${roomId}/triggers/${index}`);
  }

  async addRoomExtraDesc(
    roomId: string,
    data: AddExtraDescRequest
  ): Promise<Room> {
    return this.request<Room>("post", `/rooms/${roomId}/extra`, data);
  }

  async removeRoomExtraDesc(
    roomId: string,
    keyword: string
  ): Promise<Room> {
    return this.request<Room>(
      "delete",
      `/rooms/${roomId}/extra/${encodeURIComponent(keyword)}`
    );
  }

  // Items
  async listItems(
    limit?: number,
    offset?: number,
    itemType?: string
  ): Promise<Item[]> {
    const params = new URLSearchParams();
    if (limit !== undefined) params.set("limit", limit.toString());
    if (offset !== undefined) params.set("offset", offset.toString());
    if (itemType) params.set("item_type", itemType);
    const query = params.toString();
    return this.listRequest<Item>(`/items${query ? `?${query}` : ""}`);
  }

  async listItemPrototypes(): Promise<Item[]> {
    return this.listRequest<Item>("/items/prototypes");
  }

  async listItemPrototypesSummary(vnumPrefix?: string): Promise<ItemSummary[]> {
    const params = new URLSearchParams();
    if (vnumPrefix) params.set("vnum_prefix", vnumPrefix);
    const query = params.toString();
    return this.listRequest<ItemSummary>(`/items/prototypes/summary${query ? `?${query}` : ""}`);
  }

  async getItem(id: string): Promise<Item> {
    return this.request<Item>("get", `/items/${id}`);
  }

  async getItemByVnum(vnum: string): Promise<Item> {
    return this.request<Item>("get", `/items/by-vnum/${encodeURIComponent(vnum)}`);
  }

  async createItem(data: CreateItemRequest): Promise<Item> {
    return this.request<Item>("post", "/items", data);
  }

  async updateItem(
    id: string,
    data: Partial<CreateItemRequest>
  ): Promise<{ data: Item; refreshed_instances?: number }> {
    return this.requestWithMeta<Item>("put", `/items/${id}`, data);
  }

  async deleteItem(id: string): Promise<void> {
    await this.request("delete", `/items/${id}`);
  }

  async spawnItem(vnum: string, data: SpawnEntityRequest): Promise<Item> {
    return this.request<Item>(
      "post",
      `/items/${encodeURIComponent(vnum)}/spawn`,
      data
    );
  }

  async addItemTrigger(
    itemId: string,
    data: AddItemTriggerRequest
  ): Promise<{ data: Item; refreshed_instances?: number }> {
    return this.requestWithMeta<Item>(
      "post",
      `/items/${itemId}/triggers`,
      data
    );
  }

  async removeItemTrigger(
    itemId: string,
    index: number
  ): Promise<{ data: Item; refreshed_instances?: number }> {
    return this.requestWithMeta<Item>(
      "delete",
      `/items/${itemId}/triggers/${index}`
    );
  }

  // Mobiles
  async listMobiles(limit?: number, offset?: number): Promise<Mobile[]> {
    const params = new URLSearchParams();
    if (limit !== undefined) params.set("limit", limit.toString());
    if (offset !== undefined) params.set("offset", offset.toString());
    const query = params.toString();
    return this.listRequest<Mobile>(`/mobiles${query ? `?${query}` : ""}`);
  }

  async listMobilePrototypes(): Promise<Mobile[]> {
    return this.listRequest<Mobile>("/mobiles/prototypes");
  }

  async listMobilePrototypesSummary(vnumPrefix?: string): Promise<MobileSummary[]> {
    const params = new URLSearchParams();
    if (vnumPrefix) params.set("vnum_prefix", vnumPrefix);
    const query = params.toString();
    return this.listRequest<MobileSummary>(`/mobiles/prototypes/summary${query ? `?${query}` : ""}`);
  }

  async getMobile(id: string): Promise<Mobile> {
    return this.request<Mobile>("get", `/mobiles/${id}`);
  }

  async getMobileByVnum(vnum: string): Promise<Mobile> {
    return this.request<Mobile>(
      "get",
      `/mobiles/by-vnum/${encodeURIComponent(vnum)}`
    );
  }

  async createMobile(data: CreateMobileRequest): Promise<Mobile> {
    return this.request<Mobile>("post", "/mobiles", data);
  }

  async updateMobile(
    id: string,
    data: Partial<CreateMobileRequest>
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>("put", `/mobiles/${id}`, data);
  }

  async deleteMobile(id: string): Promise<void> {
    await this.request("delete", `/mobiles/${id}`);
  }

  async addMobileDialogue(
    mobileId: string,
    data: AddDialogueRequest
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "post",
      `/mobiles/${mobileId}/dialogue`,
      data
    );
  }

  async removeMobileDialogue(
    mobileId: string,
    keyword: string
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "delete",
      `/mobiles/${mobileId}/dialogue/${encodeURIComponent(keyword)}`
    );
  }

  async addMobileRoutine(
    mobileId: string,
    data: {
      start_hour: number;
      activity: string;
      destination_vnum?: string;
      transition_message?: string;
      suppress_wander?: boolean;
      dialogue_overrides?: Record<string, string>;
    }
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "post",
      `/mobiles/${mobileId}/routine`,
      data
    );
  }

  async removeMobileRoutine(
    mobileId: string,
    index: number
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "delete",
      `/mobiles/${mobileId}/routine/${index}`
    );
  }

  async addMobileTrigger(
    mobileId: string,
    data: AddMobileTriggerRequest
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "post",
      `/mobiles/${mobileId}/triggers`,
      data
    );
  }

  async removeMobileTrigger(
    mobileId: string,
    index: number
  ): Promise<{ data: Mobile; refreshed_instances?: number }> {
    return this.requestWithMeta<Mobile>(
      "delete",
      `/mobiles/${mobileId}/triggers/${index}`
    );
  }

  async spawnMobile(
    vnum: string,
    data: SpawnEntityRequest
  ): Promise<Mobile> {
    return this.request<Mobile>(
      "post",
      `/mobiles/${encodeURIComponent(vnum)}/spawn`,
      data
    );
  }

  // Spawn Points
  async listSpawnPoints(areaId?: string): Promise<SpawnPoint[]> {
    const params = new URLSearchParams();
    if (areaId) params.set("area_id", areaId);
    const query = params.toString();
    return this.listRequest<SpawnPoint>(
      `/spawn-points${query ? `?${query}` : ""}`
    );
  }

  async getSpawnPoint(id: string): Promise<SpawnPoint> {
    return this.request<SpawnPoint>("get", `/spawn-points/${id}`);
  }

  async createSpawnPoint(data: CreateSpawnPointRequest): Promise<SpawnPoint> {
    return this.request<SpawnPoint>("post", "/spawn-points", data);
  }

  async updateSpawnPoint(
    id: string,
    data: Partial<CreateSpawnPointRequest>
  ): Promise<SpawnPoint> {
    return this.request<SpawnPoint>("put", `/spawn-points/${id}`, data);
  }

  async deleteSpawnPoint(id: string): Promise<void> {
    await this.request("delete", `/spawn-points/${id}`);
  }

  async addSpawnDependency(
    spawnPointId: string,
    data: AddSpawnDependencyRequest
  ): Promise<SpawnPoint> {
    return this.request<SpawnPoint>(
      "post",
      `/spawn-points/${spawnPointId}/dependencies`,
      data
    );
  }

  async removeSpawnDependency(
    spawnPointId: string,
    index: number
  ): Promise<SpawnPoint> {
    return this.request<SpawnPoint>(
      "delete",
      `/spawn-points/${spawnPointId}/dependencies/${index}`
    );
  }

  // Transports
  async listTransports(): Promise<Transport[]> {
    return this.listRequest<Transport>("/transports");
  }

  async getTransport(id: string): Promise<Transport> {
    return this.request<Transport>("get", `/transports/${encodeURIComponent(id)}`);
  }

  async createTransport(data: CreateTransportRequest): Promise<Transport> {
    return this.request<Transport>("post", "/transports", data);
  }

  async updateTransport(
    id: string,
    data: Partial<CreateTransportRequest>
  ): Promise<Transport> {
    return this.request<Transport>(
      "put",
      `/transports/${encodeURIComponent(id)}`,
      data
    );
  }

  async deleteTransport(id: string): Promise<void> {
    await this.request("delete", `/transports/${encodeURIComponent(id)}`);
  }

  async addTransportStop(
    transportId: string,
    data: AddTransportStopRequest
  ): Promise<Transport> {
    return this.request<Transport>(
      "post",
      `/transports/${encodeURIComponent(transportId)}/stops`,
      data
    );
  }

  async removeTransportStop(
    transportId: string,
    index: number
  ): Promise<Transport> {
    return this.request<Transport>(
      "delete",
      `/transports/${encodeURIComponent(transportId)}/stops/${index}`
    );
  }

  async connectTransport(
    transportId: string,
    stopIndex: number
  ): Promise<Transport> {
    return this.request<Transport>(
      "post",
      `/transports/${encodeURIComponent(transportId)}/connect`,
      { stop_index: stopIndex } as ConnectTransportRequest
    );
  }

  async startTransportTravel(
    transportId: string,
    destinationIndex: number
  ): Promise<Transport> {
    return this.request<Transport>(
      "post",
      `/transports/${encodeURIComponent(transportId)}/travel`,
      { destination_index: destinationIndex } as TravelTransportRequest
    );
  }

  // Plant Prototypes
  async listPlantPrototypes(): Promise<PlantPrototype[]> {
    return this.listRequest<PlantPrototype>("/plants");
  }

  async getPlantPrototype(id: string): Promise<PlantPrototype> {
    return this.request<PlantPrototype>("get", `/plants/${id}`);
  }

  async getPlantPrototypeByVnum(vnum: string): Promise<PlantPrototype> {
    return this.request<PlantPrototype>(
      "get",
      `/plants/by-vnum/${encodeURIComponent(vnum)}`
    );
  }

  async createPlantPrototype(
    data: CreatePlantPrototypeRequest
  ): Promise<PlantPrototype> {
    return this.request<PlantPrototype>("post", "/plants", data);
  }

  async updatePlantPrototype(
    id: string,
    data: Partial<CreatePlantPrototypeRequest>
  ): Promise<PlantPrototype> {
    return this.request<PlantPrototype>("put", `/plants/${id}`, data);
  }

  async deletePlantPrototype(id: string): Promise<void> {
    await this.request("delete", `/plants/${id}`);
  }

  // Bug Reports (approved only - see admin approval gate)
  async listBugReports(status?: string): Promise<BugReport[]> {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    const query = params.toString();
    return this.listRequest<BugReport>(
      `/bugs${query ? `?${query}` : ""}`
    );
  }

  async getBugReport(id: string): Promise<BugReport> {
    return this.request<BugReport>("get", `/bugs/${id}`);
  }

  async getBugReportByTicket(num: number): Promise<BugReport> {
    return this.request<BugReport>("get", `/bugs/by-ticket/${num}`);
  }

  async updateBugReport(
    id: string,
    data: UpdateBugReportRequest
  ): Promise<BugReport> {
    return this.request<BugReport>("put", `/bugs/${id}`, data);
  }

  async addBugNote(
    id: string,
    data: AddBugNoteRequest
  ): Promise<BugReport> {
    return this.request<BugReport>("post", `/bugs/${id}/notes`, data);
  }

  async deleteBugReport(id: string): Promise<void> {
    await this.request("delete", `/bugs/${id}`);
  }

  // Helper methods for description context gathering

  /**
   * Get all rooms connected to a room via exits
   */
  async getConnectedRooms(
    roomId: string
  ): Promise<{ direction: string; room: Room }[]> {
    const room = await this.getRoom(roomId);
    const connected: { direction: string; room: Room }[] = [];

    const directions = ["north", "south", "east", "west", "up", "down"] as const;
    for (const dir of directions) {
      const targetId = room.exits[dir];
      if (targetId) {
        try {
          const targetRoom = await this.getRoom(targetId);
          connected.push({ direction: dir, room: targetRoom });
        } catch {
          // Target room may not exist, skip
        }
      }
    }

    return connected;
  }

  /**
   * Search for entities of a type with optional filters
   */
  async searchItems(filter?: {
    item_type?: string;
    area_prefix?: string;
    has_flag?: string;
    limit?: number;
  }): Promise<Item[]> {
    let items = await this.listItems(filter?.limit || 50, 0, filter?.item_type);

    if (filter?.area_prefix) {
      items = items.filter(
        (item) => item.vnum && item.vnum.startsWith(filter.area_prefix + ":")
      );
    }

    if (filter?.has_flag) {
      const flag = filter.has_flag as keyof typeof items[0]["flags"];
      items = items.filter((item) => item.flags[flag]);
    }

    return items.slice(0, filter?.limit || 10);
  }

  async searchMobiles(filter?: {
    area_prefix?: string;
    has_flag?: string;
    limit?: number;
  }): Promise<Mobile[]> {
    let mobiles = await this.listMobiles(filter?.limit || 50);

    if (filter?.area_prefix) {
      mobiles = mobiles.filter(
        (mobile) =>
          mobile.vnum && mobile.vnum.startsWith(filter.area_prefix + ":")
      );
    }

    if (filter?.has_flag) {
      const flag = filter.has_flag as keyof typeof mobiles[0]["flags"];
      mobiles = mobiles.filter((mobile) => mobile.flags[flag]);
    }

    return mobiles.slice(0, filter?.limit || 10);
  }

  async searchRooms(filter?: {
    area_prefix?: string;
    has_flag?: string;
    limit?: number;
  }): Promise<Room[]> {
    let rooms = await this.listRooms(filter?.limit || 50);

    if (filter?.area_prefix) {
      rooms = rooms.filter(
        (room) => room.vnum && room.vnum.startsWith(filter.area_prefix + ":")
      );
    }

    if (filter?.has_flag) {
      const flag = filter.has_flag as keyof typeof rooms[0]["flags"];
      rooms = rooms.filter((room) => room.flags[flag]);
    }

    return rooms.slice(0, filter?.limit || 10);
  }
}
